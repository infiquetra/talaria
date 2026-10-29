use super::*;
use crate::session::{Event, PromptKey, PromptKind, SessionId, SessionState};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use std::fmt;
use std::time::{Duration, Instant};

pub const MAX_TURNS: u32 = 8;
pub const TURN_TIMEOUT: Duration = Duration::from_secs(300);
pub const QUIT_DEADLINE: Duration = Duration::from_secs(2);
pub const END_REASON: &str = "configure_closed";
pub const ALLOWED_TOOLS: &[&str] = &["clarify"];
pub const SESSION_TOOLSETS: &[&str] = &["clarify", "no_mcp"];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSpec {
    pub request_id: String,
    pub toolsets: Vec<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
}
// Preserve the fixed event interface; Session carries the canonical event by value.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigureInput {
    Open {
        request: String,
        profile: String,
        connected: bool,
        model: Option<(String, String)>,
    },
    Gathered(Box<Context>),
    GatherFailed(String),
    Created(Box<CanonicalSnapshot>),
    CreateRefused {
        field: Option<String>,
        detail: Option<String>,
        message: String,
    },
    Session(Event),
    ClarifyAnswered {
        key: PromptKey,
        answer: String,
    },
    Submitted,
    SubmitFailed(String),
    ServerRequest {
        request_id: String,
        method: String,
    },
    Key(ConfigureKey),
    /// A viewport-relative detail position computed by the presentation layer.
    DetailScroll(usize),
    StepDone {
        index: usize,
        result: StepResult,
    },
    Detached,
    Tick,
    LinkLost,
    LinkBack,
    Quit,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigureEffect {
    Gather,
    CreateSession(CreateSpec),
    Submit {
        session_id: String,
        text: String,
        submission_id: String,
    },
    Interrupt {
        session_id: String,
        execution_generation: i64,
    },
    DenyApproval {
        key: PromptKey,
    },
    RespondClarify {
        key: PromptKey,
        answer: String,
    },
    RefuseServerRequest {
        request_id: String,
    },
    Apply {
        index: usize,
        step: ApplyStep,
    },
    Detach {
        session_id: String,
        subscription_id: String,
    },
    ForgetHistoryLine(String),
    ClearEditor,
    Open(crate::commands::LocalAction),
    Notice(String),
    CloseOverlay,
}
pub struct ConfigureState {
    detect: SecretDetector,
    phase: Phase,
    opened: bool,
    closed: bool,
    request: String,
    profile: String,
    model: Option<(String, String)>,
    context: Option<Context>,
    session: Option<SessionState>,
    session_id: Option<String>,
    subscription_id: Option<String>,
    generation: i64,
    detached: bool,
    notice_sent: bool,
    summary: String,
    result_kind: Option<&'static str>,
    message: String,
    rows: Vec<Row>,
    selected: Vec<bool>,
    outcomes: Vec<Option<RowOutcome>>,
    cursor: usize,
    detail: bool,
    detail_scroll: usize,
    discarding: bool,
    refining: bool,
    editor: String,
    pending_line: String,
    prose: Vec<String>,
    in_block: bool,
    rounds: u8,
    id_prefix: String,
    serial: u64,
    deadline: Option<Instant>,
    turn_started: Option<Instant>,
    steps: Vec<ApplyStep>,
    step_index: usize,
    apply_started: bool,
    confirm_step: Option<String>,
    event_cursor: Option<(Option<String>, u64)>,
}

impl fmt::Debug for ConfigureState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Context and incomplete lines are deliberately not diagnostic data.
        f.debug_struct("ConfigureState")
            .field("phase", &self.phase)
            .field("rows", &self.rows)
            .field("summary", &self.summary)
            .field("message", &self.message)
            .field("context_rounds", &self.rounds)
            .field("closed", &self.closed)
            .finish()
    }
}

impl ConfigureState {
    pub fn new(detect: SecretDetector) -> Self {
        Self::new_with_prefix(detect, "talaria-configure")
    }

    /// Use an application launch/run prefix so gateway idempotency keys never cross runs.
    pub fn new_with_prefix(detect: SecretDetector, prefix: impl Into<String>) -> Self {
        Self {
            detect,
            phase: Phase::Gathering,
            opened: false,
            closed: false,
            request: String::new(),
            profile: String::new(),
            model: None,
            context: None,
            session: None,
            session_id: None,
            subscription_id: None,
            generation: 0,
            detached: false,
            notice_sent: false,
            summary: String::new(),
            result_kind: None,
            message: String::new(),
            rows: Vec::new(),
            selected: Vec::new(),
            outcomes: Vec::new(),
            cursor: 0,
            detail: false,
            detail_scroll: 0,
            discarding: false,
            refining: false,
            editor: String::new(),
            pending_line: String::new(),
            prose: Vec::new(),
            in_block: false,
            rounds: 0,
            id_prefix: prefix.into(),
            serial: 0,
            deadline: None,
            turn_started: None,
            steps: Vec::new(),
            step_index: 0,
            apply_started: false,
            confirm_step: None,
            event_cursor: None,
        }
    }
    pub fn from_reply(ctx: Context, reply_text: &str, detect: SecretDetector) -> Self {
        let mut state = Self::new(detect);
        state.profile = state.clean(&ctx.profile);
        state.context = Some(ctx);
        state.opened = true;
        state.accept_reply(reply_text, None);
        state
    }
    pub fn update(&mut self, input: ConfigureInput, now: Instant) -> Vec<ConfigureEffect> {
        if self.closed {
            return match input {
                ConfigureInput::Created(snapshot) if self.session_id.is_none() => {
                    self.remember_session(*snapshot);
                    self.detach()
                }
                _ => Vec::new(),
            };
        }
        match input {
            ConfigureInput::DetailScroll(offset) => {
                if self.detail && matches!(self.phase, Phase::Review | Phase::Done) {
                    self.detail_scroll = offset;
                }
                Vec::new()
            }
            ConfigureInput::Open {
                request,
                profile,
                connected,
                model,
            } => {
                if self.opened {
                    return Vec::new();
                }
                if (self.detect)(&request) {
                    self.closed = true;
                    return vec![ConfigureEffect::ForgetHistoryLine(String::new()), ConfigureEffect::Notice("That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.".into())];
                }
                let characters = request.trim().chars().count();
                if characters > 4_000 {
                    self.closed = true;
                    return vec![ConfigureEffect::Notice(format!(
                        "The request is too long ({characters} characters; at most 4,000)."
                    ))];
                }
                let request = match parse_configure_args(&request) {
                    Ok(request) => request,
                    Err(text) => {
                        self.closed = true;
                        return vec![ConfigureEffect::Notice(text.into())];
                    }
                };
                if !connected {
                    self.closed = true;
                    return vec![ConfigureEffect::Notice(
                        "Not connected to Hermes. /configure needs the gateway.".into(),
                    )];
                }
                self.request = self.clean(&request);
                self.profile = self.clean(&profile);
                self.model =
                    model.map(|(provider, model)| (self.clean(&provider), self.clean(&model)));
                self.phase = Phase::Gathering;
                self.opened = true;
                vec![ConfigureEffect::Gather]
            }
            ConfigureInput::Gathered(ctx) if self.phase == Phase::Gathering => {
                self.context = Some(*ctx);
                self.phase = Phase::Starting;
                self.serial += 1;
                vec![ConfigureEffect::CreateSession(CreateSpec {
                    request_id: format!("{}-create-{}", self.id_prefix, self.serial),
                    toolsets: SESSION_TOOLSETS
                        .iter()
                        .map(|value| (*value).into())
                        .collect(),
                    provider: self.model.as_ref().map(|value| value.0.clone()),
                    model: self.model.as_ref().map(|value| value.1.clone()),
                })]
            }
            ConfigureInput::GatherFailed(error) => self.fail(&error),
            ConfigureInput::Created(snapshot) if self.phase == Phase::Starting => {
                let tools = snapshot.info.tools.clone();
                let yolo = snapshot.info.yolo;
                self.remember_session(*snapshot);
                let mut names: Vec<_> = tools
                    .as_ref()
                    .into_iter()
                    .flat_map(|tools| tools.values())
                    .flatten()
                    .cloned()
                    .collect();
                names.sort();
                if yolo == Some(true) || names.iter().any(|name| name != "clarify") {
                    self.phase = Phase::Failed;
                    self.message = if yolo == Some(true) {
                        "The configuration session runs with approvals off. Nothing was sent; /configure stopped.".into()
                    } else {
                        format!(
                            "The configuration session was given tools Talaria did not ask for ({}). Nothing was sent; /configure stopped.",
                            self.clean(&names.join(", "))
                        )
                    };
                    return self.detach();
                }
                if names.is_empty() || yolo != Some(false) {
                    self.phase = Phase::Failed;
                    self.message = "Talaria could not confirm which tools the configuration session has, so nothing was sent; /configure stopped.".into();
                    return self.detach();
                }
                let Some(ctx) = &self.context else {
                    return self.fail("missing configuration context");
                };
                let text = build_brief(ctx, &self.request, self.detect).text;
                self.submit(text, now)
            }
            ConfigureInput::CreateRefused {
                field,
                detail,
                message,
            } if self.phase == Phase::Starting => {
                self.phase = Phase::Failed;
                self.message = if field.as_deref() == Some("toolsets") {
                    format!(
                        "Hermes refused the configuration session's tools ({}). /configure needs the clarify tool; check agent.disabled_toolsets with /config.",
                        self.clean(detail.as_deref().unwrap_or(&message))
                    )
                } else {
                    format!(
                        "The configuration agent failed: {}",
                        self.clean(detail.as_deref().unwrap_or(&message))
                    )
                };
                Vec::new()
            }
            ConfigureInput::Session(event) => self.session_event(event, now),
            ConfigureInput::ClarifyAnswered { key, answer } if self.phase == Phase::Question => {
                if !self.session.as_ref().is_some_and(|session| {
                    session
                        .open_prompts()
                        .any(|prompt| prompt.key == key && prompt.kind == PromptKind::Clarify)
                }) {
                    return Vec::new();
                }
                if (self.detect)(&answer) {
                    self.message = secret_input_message().into();
                    return vec![ConfigureEffect::ClearEditor];
                }
                self.message.clear();
                self.phase = Phase::Thinking;
                if let Some(session) = &mut self.session {
                    crate::session::update(
                        session,
                        Event::AnswerClarify {
                            key: key.clone(),
                            answer: answer.clone(),
                        },
                    );
                }
                vec![ConfigureEffect::RespondClarify { key, answer }]
            }
            ConfigureInput::Submitted => Vec::new(),
            ConfigureInput::SubmitFailed(error)
                if matches!(self.phase, Phase::Thinking | Phase::Question) =>
            {
                self.fail(&error)
            }
            ConfigureInput::ServerRequest { request_id, method } if self.running() => {
                let mut effects = vec![ConfigureEffect::RefuseServerRequest { request_id }];
                effects.extend(self.stop_effects());
                self.phase = Phase::Failed;
                self.message = format!(
                    "The configuration agent tried to use {}. Talaria stopped it; nothing was changed.",
                    self.clean(&method)
                );
                self.deadline = None;
                effects
            }
            ConfigureInput::Key(key) => self.key(key, now),
            ConfigureInput::StepDone { index, result }
                if self.phase == Phase::Applying
                    && index == self.step_index
                    && self.confirm_step.is_none() =>
            {
                self.step_done(result)
            }
            ConfigureInput::Tick
                if self.running() && self.deadline.is_some_and(|deadline| now >= deadline) =>
            {
                let effects = self.stop_effects();
                self.phase = Phase::Failed;
                self.message =
                    "No reply within 5 minutes. Talaria stopped the agent; nothing was changed."
                        .into();
                self.deadline = None;
                effects
            }
            ConfigureInput::LinkLost => {
                self.deadline = None;
                if self.phase == Phase::Applying {
                    for row in self.current_step_rows() {
                        self.outcomes[row] = Some(RowOutcome::Unknown("lost connection".into()));
                    }
                    self.finish_apply();
                    self.message = "Lost the connection to Hermes while applying. Some changes may have been applied; check /config.".into();
                } else {
                    self.phase = Phase::Failed;
                    self.message = "Lost the connection to Hermes. Nothing was changed.".into();
                }
                self.detach()
            }
            ConfigureInput::Quit => {
                let mut effects = if self.running() {
                    self.stop_effects()
                } else {
                    Vec::new()
                };
                effects.extend(self.close());
                effects
            }
            _ => Vec::new(),
        }
    }
    pub fn view(&self, now: Instant) -> ConfigureView {
        let mut confirmation = None;
        let mut title = format!("Configure · profile {}", self.profile);
        if let Some((provider, model)) = &self.model {
            title.push_str(&format!(" · {provider}/{model}"));
        }
        let mut lines = Vec::new();
        let mut push = |text: String, row: Option<usize>| {
            lines.push(ViewLine {
                text: self.clean(&text),
                row,
            })
        };
        match self.phase {
            Phase::Gathering => push(format!("Reading profile {}…", self.profile), None),
            Phase::Starting => push("Starting the configuration agent…".into(), None),
            Phase::Thinking => {
                for line in &self.prose {
                    push(line.clone(), None);
                }
                let seconds = self
                    .turn_started
                    .map(|started| now.saturating_duration_since(started).as_secs())
                    .unwrap_or(0);
                push(
                    format!("Asking the configuration agent… {seconds}s · esc cancels"),
                    None,
                );
            }
            Phase::Question => {
                if let Some(session) = &self.session {
                    for prompt in session
                        .open_prompts()
                        .filter(|prompt| prompt.kind == PromptKind::Clarify)
                    {
                        if let Some(question) = &prompt.question {
                            push(question.clone(), None);
                        }
                        for choice in &prompt.choices {
                            push(choice.clone(), None);
                        }
                    }
                }
            }
            Phase::Review | Phase::Confirming | Phase::Applying | Phase::Done => {
                if self.phase == Phase::Done {
                    push(self.done_message(), None);
                } else {
                    for line in self.summary.lines().take(3) {
                        push(line.into(), None);
                    }
                }
                let mut next = 0;
                for (index, row) in self.rows.iter().enumerate() {
                    let (mark, reason) = match &self.outcomes[index] {
                        Some(outcome) => outcome_display(outcome),
                        None => match &row.status {
                            RowStatus::Ready => {
                                if self.phase == Phase::Applying
                                    && self.current_step_rows().contains(&index)
                                {
                                    ("…".into(), None)
                                } else {
                                    (
                                        format!(
                                            "[{}]{}",
                                            if self.selected[index] { "x" } else { " " },
                                            if row.destructive { " !" } else { "" }
                                        ),
                                        None,
                                    )
                                }
                            }
                            RowStatus::NoChange => ("=".into(), None),
                            RowStatus::Invalid(reason) | RowStatus::Refused(reason) => {
                                ("✗".into(), Some(reason.as_str()))
                            }
                            RowStatus::HandOff => {
                                next += 1;
                                (
                                    if next <= 9 {
                                        format!("→ {next}")
                                    } else {
                                        "→".into()
                                    },
                                    None,
                                )
                            }
                        },
                    };
                    let mut text = format!("{mark} {}", row.text);
                    if let Some(reason) = reason {
                        text.push_str(&format!(" · {reason}"));
                    }
                    if let Some(label) = &row.effect_label {
                        text.push_str(&format!(" · {label}"));
                    }
                    push(text, Some(index));
                    if self.detail && index == self.cursor {
                        for detail in &row.detail {
                            push(format!("  {detail}"), Some(index));
                        }
                    }
                }
                if self.phase == Phase::Confirming {
                    let count = self.selected.iter().filter(|selected| **selected).count();
                    let destructive = self
                        .rows
                        .iter()
                        .zip(&self.selected)
                        .filter(|(row, selected)| **selected && row.destructive)
                        .count();
                    confirmation = Some(format!(
                        "Apply {count} change(s), {destructive} removal(s) or switch-off(s) to profile {}?",
                        self.profile
                    ));
                }
                if self.phase == Phase::Done {
                    if self.rows.iter().any(|row| row.status == RowStatus::HandOff) {
                        push("Next steps:".into(), None);
                        for row in self
                            .rows
                            .iter()
                            .filter(|row| row.status == RowStatus::HandOff)
                        {
                            push(row.text.clone(), None);
                        }
                    }
                    let mut labels = std::collections::BTreeSet::new();
                    for (row, outcome) in self.rows.iter().zip(&self.outcomes) {
                        if matches!(outcome, Some(RowOutcome::Applied | RowOutcome::Adjusted(_))) {
                            if let Some(label) = &row.effect_label {
                                labels.insert(label.clone());
                            }
                            if matches!(
                                row.change,
                                Change::McpAdd { .. }
                                    | Change::McpEnable { .. }
                                    | Change::McpRemove { .. }
                            ) {
                                labels.insert("new sessions; /mcp apply reloads this one".into());
                            }
                        }
                    }
                    for label in labels {
                        push(label, None);
                    }
                }
            }
            _ => {}
        }
        if !self.message.is_empty() {
            push(self.message.clone(), None);
        }
        if let Some(message) = &self.confirm_step {
            confirmation = Some(message.clone());
        }
        if self.discarding {
            confirmation = Some("Discard this proposal?".into());
        }
        if self.refining {
            push(format!("Refine: {}", self.clean(&self.editor)), None);
        }
        ConfigureView {
            title: self.clean(&title),
            phase: self.phase.clone(),
            lines,
            confirmation: confirmation.map(|text| self.clean(&text)),
            footer: if self.discarding {
                "y discard · n back · esc back".into()
            } else if self.confirm_step.is_some() {
                "y apply · n skip · esc skip".into()
            } else if self.phase == Phase::Confirming {
                "y apply · n back · esc back".into()
            } else if self.refining {
                "enter sends · esc back".into()
            } else if self.detail {
                "↑ ↓ select · PgUp/PgDn scroll · d close detail · a apply".into()
            } else {
                "arrows select · space toggle · a apply · d detail · r refine · esc closes".into()
            },
            selected_row: (!self.rows.is_empty()).then_some(self.cursor),
            detail_scroll: self.detail.then_some(self.detail_scroll),
        }
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub fn session(&self) -> Option<&SessionState> {
        self.session.as_ref()
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn result_kind(&self) -> Option<&str> {
        self.result_kind
    }
    pub fn context_rounds(&self) -> u8 {
        self.rounds
    }
    pub fn hidden_session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    pub fn refinement_active(&self) -> bool {
        self.refining
    }
    pub fn is_closed(&self) -> bool {
        !self.opened || self.closed
    }
}

impl ConfigureState {
    fn clean(&self, text: &str) -> String {
        super::reply::sanitize_text(text, self.detect)
    }

    fn running(&self) -> bool {
        matches!(self.phase, Phase::Thinking | Phase::Question)
    }

    fn fail(&mut self, error: &str) -> Vec<ConfigureEffect> {
        self.phase = Phase::Failed;
        self.deadline = None;
        self.message = format!("The configuration agent failed: {}", self.clean(error));
        Vec::new()
    }

    fn remember_session(&mut self, snapshot: CanonicalSnapshot) {
        self.session_id = Some(snapshot.session_id.clone());
        self.subscription_id = Some(snapshot.subscription_id.clone());
        self.generation = snapshot.execution_generation;
        if let (Some(provider), Some(model)) = (&snapshot.info.provider, &snapshot.info.model) {
            self.model = Some((self.clean(provider), self.clean(model)));
        }
        // Only the filtered snapshot enters the embedded prompt/transcript state.
        if let Ok(mut value) = serde_json::to_value(&snapshot) {
            scrub_value(&mut value, self.detect);
            if let Ok(snapshot) = serde_json::from_value(value) {
                self.session = Some(SessionState::from_snapshot(snapshot));
            }
        }
        if self.session.is_none() {
            self.session = Some(SessionState::empty(SessionId::new(snapshot.session_id)));
        }
    }

    fn submit(&mut self, text: String, now: Instant) -> Vec<ConfigureEffect> {
        let Some(session_id) = self.session_id.clone() else {
            return self.fail("configuration session is closed");
        };
        if self.detached {
            return self.fail("configuration session is closed");
        }
        // Builders check individual fields; this final gate prevents an outbound leak.
        if (self.detect)(&text) {
            return self.fail("configuration text contains a secret");
        }
        self.serial += 1;
        self.phase = Phase::Thinking;
        self.deadline = Some(now + TURN_TIMEOUT);
        self.turn_started = Some(now);
        self.message.clear();
        self.pending_line.clear();
        self.prose.clear();
        self.in_block = false;
        vec![ConfigureEffect::Submit {
            session_id,
            text,
            submission_id: format!("{}-submit-{}", self.id_prefix, self.serial),
        }]
    }

    fn detach(&mut self) -> Vec<ConfigureEffect> {
        if self.detached {
            return Vec::new();
        }
        match (&self.session_id, &self.subscription_id) {
            (Some(session_id), Some(subscription_id)) => {
                self.detached = true;
                vec![ConfigureEffect::Detach {
                    session_id: session_id.clone(),
                    subscription_id: subscription_id.clone(),
                }]
            }
            _ => Vec::new(),
        }
    }

    fn stop_effects(&self) -> Vec<ConfigureEffect> {
        let mut effects: Vec<_> = self
            .session
            .as_ref()
            .into_iter()
            .flat_map(|session| session.open_prompts())
            .filter(|prompt| prompt.kind == PromptKind::Approval)
            .map(|prompt| ConfigureEffect::DenyApproval {
                key: prompt.key.clone(),
            })
            .collect();
        if let Some(session_id) = &self.session_id {
            effects.push(ConfigureEffect::Interrupt {
                session_id: session_id.clone(),
                execution_generation: self.generation,
            });
        }
        effects
    }

    fn close(&mut self) -> Vec<ConfigureEffect> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        self.refining = false;
        self.editor.clear();
        self.pending_line.clear();
        self.deadline = None;
        let mut effects = self.detach();
        if !self.notice_sent {
            self.notice_sent = true;
            let summary: String = if self.summary.is_empty() {
                &self.request
            } else {
                &self.summary
            }
            .chars()
            .take(60)
            .collect();
            let mut notice = if self.apply_started {
                let (applied, failed, unknown, _) = self.counts();
                format!(
                    "/configure {summary}: {applied} applied, {failed} failed, {unknown} unknown."
                )
            } else {
                format!("/configure {summary}: nothing was applied.")
            };
            for row in self
                .rows
                .iter()
                .filter(|row| row.status == RowStatus::HandOff)
            {
                notice.push('\n');
                notice.push_str(&row.text);
            }
            effects.push(ConfigureEffect::Notice(self.clean(&notice)));
        }
        effects.push(ConfigureEffect::CloseOverlay);
        effects
    }

    fn session_event(&mut self, event: Event, now: Instant) -> Vec<ConfigureEffect> {
        if !self.running() {
            return Vec::new();
        }
        let Event::Session {
            session_id,
            frame,
            mut raw,
        } = event
        else {
            return Vec::new();
        };
        if self.session_id.as_deref() != Some(session_id.as_str()) {
            return Vec::new();
        }
        if let Some(seq) = frame.seq {
            if self
                .event_cursor
                .as_ref()
                .is_some_and(|(epoch, last)| *epoch == frame.replay_epoch && seq <= *last)
            {
                return Vec::new();
            }
            self.event_cursor = Some((frame.replay_epoch.clone(), seq));
        }
        let name = frame.type_name().unwrap_or_default().to_owned();
        let payload = raw.get("payload").cloned().unwrap_or(Value::Null);
        if let Some(generation) = frame.execution_generation {
            self.generation = generation;
        }
        // Partial model lines never enter SessionState or its Debug output.
        if name == "message.delta" {
            if self.phase != Phase::Thinking {
                return Vec::new();
            }
            if let Some(chunk) = payload["text"].as_str() {
                self.stream(chunk);
            }
            raw["payload"]["text"] = Value::String(String::new());
        }
        scrub_value(&mut raw, self.detect);
        if let Some(session) = &mut self.session
            && let Ok(frame) = hermes_contract::events::EventFrame::from_params(&raw)
        {
            crate::session::update(
                session,
                Event::Session {
                    session_id,
                    frame,
                    raw,
                },
            );
        }
        match name.as_str() {
            "message.complete" if self.phase == Phase::Thinking => {
                self.deadline = None;
                self.pending_line.clear();
                match payload["status"].as_str().unwrap_or("complete") {
                    "complete" => {
                        self.accept_reply(payload["text"].as_str().unwrap_or_default(), Some(now))
                    }
                    "interrupted" => {
                        self.phase = Phase::Cancelled;
                        self.message = "Cancelled. Nothing was changed.".into();
                        Vec::new()
                    }
                    _ => {
                        let error = payload["error"]
                            .as_str()
                            .or_else(|| payload["failure_reason"].as_str())
                            .or_else(|| payload["text"].as_str())
                            .unwrap_or("unknown error");
                        self.fail(error)
                    }
                }
            }
            "clarify.request" => {
                self.phase = Phase::Question;
                self.message.clear();
                Vec::new()
            }
            "approval.request" => {
                let effects = self.stop_effects();
                self.phase = Phase::Failed;
                self.deadline = None;
                self.message = format!(
                    "The configuration agent tried to use {}. Talaria stopped it; nothing was changed.",
                    self.clean(payload["command"].as_str().unwrap_or("an approval"))
                );
                effects
            }
            "tool.generating" | "tool.start" if payload["name"].as_str() != Some("clarify") => {
                let effects = self.stop_effects();
                self.phase = Phase::Failed;
                self.deadline = None;
                self.message = format!(
                    "The configuration agent tried to use {}. Talaria stopped it; nothing was changed.",
                    self.clean(payload["name"].as_str().unwrap_or("an unknown tool"))
                );
                effects
            }
            "error" => self.fail(payload["message"].as_str().unwrap_or("unknown error")),
            _ => Vec::new(),
        }
    }

    fn stream(&mut self, chunk: &str) {
        self.pending_line.push_str(chunk);
        while let Some(end) = self.pending_line.find('\n') {
            let line: String = self.pending_line.drain(..=end).collect();
            if line.trim_start().starts_with("```") {
                self.in_block = !self.in_block;
            } else if !self.in_block {
                self.prose
                    .push(self.clean(line.trim_end_matches(['\r', '\n'])));
                if self.prose.len() > 12 {
                    self.prose.remove(0);
                }
            }
        }
    }

    fn accept_reply(&mut self, text: &str, now: Option<Instant>) -> Vec<ConfigureEffect> {
        let parsed = parse_reply(text).map(|reply| super::reply::scrub_reply(reply, self.detect));
        self.message.clear();
        match parsed {
            Ok(Reply::Proposal(proposal)) => {
                let Some(ctx) = &self.context else {
                    return self.fail("missing configuration context");
                };
                match validate(&proposal, ctx, self.detect) {
                    Ok(rows) => {
                        self.result_kind = Some("proposal");
                        self.summary = self.clean(&proposal.summary);
                        self.selected = rows.iter().map(|row| row.status == RowStatus::Ready && !row.destructive).collect();
                        self.outcomes = vec![None; rows.len()];
                        self.rows = rows;
                        self.cursor = 0;
                        self.detail = false;
                        self.phase = Phase::Review;
                    }
                    Err(ProposalError::TooMany(count)) => self.unreadable(format!("The proposal has {count} changes; Talaria takes at most 20. r to narrow it down.")),
                    Err(ProposalError::WrongProfile { named, connected }) => self.unreadable(format!("/configure changes the profile Talaria is connected to ({connected}). To change {named}, start talaria --profile {named} and run /configure there.")),
                }
            }
            Ok(Reply::Need(need)) => {
                self.rounds = self.rounds.saturating_add(1);
                if self.rounds > MAX_CONTEXT_ROUNDS {
                    self.unreadable(format!(
                        "The agent asked for more context {} times. r to rephrase · esc closes",
                        self.rounds
                    ));
                } else if let (Some(ctx), Some(now)) = (&self.context, now) {
                    return self.submit(build_context_reply(ctx, &need, self.detect), now);
                } else {
                    self.unreadable(
                        "The agent asked for more context. r to rephrase · esc closes".into(),
                    );
                }
            }
            Ok(Reply::Answer(answer)) => {
                let answer = self.clean(&answer);
                self.result_kind = Some("answer");
                self.summary = answer.clone();
                self.phase = Phase::Answer;
                self.message =
                    format!("{answer}\nNothing to change. r asks something else · esc closes");
            }
            Ok(Reply::Cannot(reason)) => {
                let reason = self.clean(&reason);
                self.result_kind = Some("cannot");
                self.summary = reason.clone();
                self.phase = Phase::Cannot;
                self.message = format!("The agent cannot do this: {reason}");
            }
            Err(ReplyError::NoBlock) => {
                self.unreadable(
                    "The agent replied without a proposal. r asks again · esc closes".into(),
                );
                self.result_kind = Some("no_block");
            }
            Err(ReplyError::TooManyChanges(count)) => self.unreadable(format!(
                "The proposal has {count} changes; Talaria takes at most 20. r to narrow it down."
            )),
            Err(error) => self.unreadable(format!(
                "The agent's proposal could not be read: {}. r asks again · esc closes",
                self.clean(&format!("{error:?}"))
            )),
        }
        Vec::new()
    }

    fn unreadable(&mut self, message: String) {
        self.phase = Phase::NoProposal;
        self.message = self.clean(&message);
        self.summary = self.message.clone();
        self.result_kind = Some("unreadable");
    }
}

fn secret_input_message() -> &'static str {
    "That text looks like it contains a key or token, so it was not sent. Add keys with /providers."
}

fn scrub_value(value: &mut Value, detect: SecretDetector) {
    match value {
        Value::String(text) => *text = super::reply::sanitize_text(text, detect),
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| scrub_value(value, detect)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| scrub_value(value, detect)),
        _ => {}
    }
}

impl ConfigureState {
    fn key(&mut self, key: ConfigureKey, now: Instant) -> Vec<ConfigureEffect> {
        if self.refining {
            match key {
                ConfigureKey::Esc => {
                    self.refining = false;
                    self.editor.clear();
                    self.message.clear();
                }
                ConfigureKey::Enter => {
                    let text = std::mem::take(&mut self.editor);
                    if (self.detect)(&text) {
                        self.message = secret_input_message().into();
                        return vec![ConfigureEffect::ClearEditor];
                    }
                    if text.trim().is_empty() {
                        return Vec::new();
                    }
                    self.refining = false;
                    return self.submit(build_refine_text(&text), now);
                }
                ConfigureKey::Backspace => {
                    self.editor.pop();
                }
                ConfigureKey::Char(ch) if !ch.is_control() => {
                    if self.editor.chars().count() < 2_000 {
                        self.editor.push(ch);
                    }
                }
                ConfigureKey::Paste(text) => {
                    let left = 2_000usize.saturating_sub(self.editor.chars().count());
                    self.editor
                        .extend(text.chars().filter(|ch| !ch.is_control()).take(left));
                }
                _ => {}
            }
            return Vec::new();
        }
        if self.discarding {
            match key {
                ConfigureKey::Char('y') => return self.close(),
                ConfigureKey::Char('n') | ConfigureKey::Esc => self.discarding = false,
                _ => {}
            }
            return Vec::new();
        }
        if self.confirm_step.is_some() {
            match key {
                ConfigureKey::Char('y') => {
                    self.confirm_step = None;
                    if let Some(ApplyStep::MainModel { confirmed, .. }) =
                        self.steps.get_mut(self.step_index)
                    {
                        *confirmed = true;
                    }
                    return self.current_apply_effect();
                }
                ConfigureKey::Char('n') | ConfigureKey::Esc => {
                    self.confirm_step = None;
                    return self.step_done(StepResult::Rows(
                        self.current_step_rows()
                            .into_iter()
                            .map(|row| (row, RowOutcome::NotApplied("not confirmed".into())))
                            .collect(),
                    ));
                }
                _ => return Vec::new(),
            }
        }
        if self.phase == Phase::Confirming {
            match key {
                ConfigureKey::Char('n') | ConfigureKey::Esc => self.phase = Phase::Review,
                ConfigureKey::Char('y') => {
                    let Some(ctx) = &self.context else {
                        return self.fail("missing configuration context");
                    };
                    self.steps = plan_apply(&self.rows, &self.selected, ctx);
                    self.step_index = 0;
                    self.apply_started = true;
                    self.phase = Phase::Applying;
                    if self.steps.is_empty() {
                        self.finish_apply();
                        return Vec::new();
                    }
                    return self.current_apply_effect();
                }
                _ => {}
            }
            return Vec::new();
        }
        if self.phase == Phase::Applying {
            if key == ConfigureKey::Esc {
                self.message = "Applying… wait for it to finish.".into();
            }
            return Vec::new();
        }
        if key == ConfigureKey::Esc {
            if self.running() {
                let effects = self.stop_effects();
                self.phase = Phase::Cancelled;
                self.deadline = None;
                self.message = "Cancelled. Nothing was changed.".into();
                return effects;
            }
            if self.phase == Phase::Review && self.selected.iter().any(|selected| *selected) {
                self.discarding = true;
                return Vec::new();
            }
            return self.close();
        }
        if key == ConfigureKey::Enter
            && matches!(self.phase, Phase::Done | Phase::Cancelled | Phase::Failed)
        {
            return self.close();
        }
        if matches!(self.phase, Phase::Review | Phase::Done) {
            if self.detail {
                let limit = self.rows.get(self.cursor).map_or(0, |row| {
                    row.text.chars().count()
                        + row
                            .detail
                            .iter()
                            .map(|line| line.chars().count())
                            .sum::<usize>()
                });
                match key {
                    ConfigureKey::PageUp => {
                        self.detail_scroll = self.detail_scroll.saturating_sub(5);
                        return Vec::new();
                    }
                    ConfigureKey::PageDown => {
                        self.detail_scroll = self.detail_scroll.saturating_add(5).min(limit);
                        return Vec::new();
                    }
                    _ => {}
                }
            }
            match key {
                ConfigureKey::Up
                | ConfigureKey::Down
                | ConfigureKey::PageUp
                | ConfigureKey::PageDown
                | ConfigureKey::Home
                | ConfigureKey::End => {
                    self.cursor = match key {
                        ConfigureKey::Up => self.cursor.saturating_sub(1),
                        ConfigureKey::Down => self.cursor.saturating_add(1),
                        ConfigureKey::PageUp => self.cursor.saturating_sub(10),
                        ConfigureKey::PageDown => self.cursor.saturating_add(10),
                        ConfigureKey::Home => 0,
                        _ => self.rows.len().saturating_sub(1),
                    }
                    .min(self.rows.len().saturating_sub(1));
                    self.detail = false;
                }
                ConfigureKey::Space if self.phase == Phase::Review => {
                    if self
                        .rows
                        .get(self.cursor)
                        .is_some_and(|row| row.status == RowStatus::Ready)
                    {
                        self.selected[self.cursor] = !self.selected[self.cursor];
                    }
                }
                ConfigureKey::Char('d') => {
                    self.detail = !self.detail;
                    self.detail_scroll = 0;
                }
                ConfigureKey::Char('a')
                    if self.phase == Phase::Review
                        && self.selected.iter().any(|selected| *selected) =>
                {
                    self.phase = Phase::Confirming
                }
                ConfigureKey::Char(number @ '1'..='9') => {
                    let index = usize::try_from(u32::from(number) - u32::from('1')).unwrap_or(0);
                    if let Some(row) = self
                        .rows
                        .iter()
                        .filter(|row| row.status == RowStatus::HandOff)
                        .nth(index)
                    {
                        let action = handoff_action(row);
                        let mut effects = self.close();
                        effects.push(ConfigureEffect::Open(action));
                        return effects;
                    }
                }
                _ => {}
            }
        }
        if key == ConfigureKey::Char('r')
            && matches!(
                self.phase,
                Phase::Review | Phase::Answer | Phase::Cannot | Phase::NoProposal
            )
        {
            self.refining = true;
            self.editor.clear();
            self.message.clear();
        }
        Vec::new()
    }

    fn current_step_rows(&self) -> Vec<usize> {
        self.steps
            .get(self.step_index)
            .map(step_rows)
            .unwrap_or_default()
    }

    fn current_apply_effect(&self) -> Vec<ConfigureEffect> {
        self.steps
            .get(self.step_index)
            .map(|step| {
                vec![ConfigureEffect::Apply {
                    index: self.step_index,
                    step: step.clone(),
                }]
            })
            .unwrap_or_default()
    }

    fn step_done(&mut self, result: StepResult) -> Vec<ConfigureEffect> {
        let rows = self.current_step_rows();
        match result {
            StepResult::NeedsConfirm { row, message } => {
                if rows == [row]
                    && matches!(
                        self.steps.get(self.step_index),
                        Some(ApplyStep::MainModel {
                            confirmed: false,
                            ..
                        })
                    )
                {
                    self.confirm_step = Some(self.clean(&message));
                    return Vec::new();
                }
                for row in rows {
                    self.outcomes[row] =
                        Some(RowOutcome::Unknown("invalid confirmation result".into()));
                }
                self.finish_apply();
                return Vec::new();
            }
            StepResult::Rows(outcomes) => {
                for row in rows {
                    let mut matches = outcomes.iter().filter(|(index, _)| *index == row);
                    let outcome = matches
                        .next()
                        .map(|(_, outcome)| outcome.clone())
                        .unwrap_or_else(|| RowOutcome::Unknown("no result".into()));
                    self.outcomes[row] = Some(if matches.next().is_some() {
                        RowOutcome::Unknown("duplicate result".into())
                    } else {
                        sanitize_outcome(outcome, self.detect)
                    });
                }
            }
        }
        if self
            .outcomes
            .iter()
            .any(|outcome| matches!(outcome, Some(RowOutcome::Unknown(_))))
        {
            self.finish_apply();
            return Vec::new();
        }
        self.step_index += 1;
        if self.step_index == self.steps.len() {
            self.finish_apply();
            Vec::new()
        } else {
            self.current_apply_effect()
        }
    }

    fn finish_apply(&mut self) {
        for (selected, outcome) in self.selected.iter().zip(&mut self.outcomes) {
            if *selected && outcome.is_none() {
                *outcome = Some(RowOutcome::NotAttempted);
            }
        }
        self.confirm_step = None;
        self.phase = Phase::Done;
        self.message.clear();
    }

    fn counts(&self) -> (usize, usize, usize, usize) {
        self.outcomes
            .iter()
            .flatten()
            .fold((0, 0, 0, 0), |(a, f, u, s), outcome| match outcome {
                RowOutcome::Applied | RowOutcome::Adjusted(_) | RowOutcome::NoChange(_) => {
                    (a + 1, f, u, s)
                }
                RowOutcome::Failed(_) | RowOutcome::NotApplied(_) | RowOutcome::Conflict(_) => {
                    (a, f + 1, u, s)
                }
                RowOutcome::Unknown(_) => (a, f, u + 1, s),
                RowOutcome::NotAttempted => (a, f, u, s + 1),
            })
    }

    fn done_message(&self) -> String {
        let (applied, failed, unknown, skipped) = self.counts();
        format!(
            "Configured profile {}: {applied} applied, {failed} failed, {unknown} unknown, {skipped} not attempted.",
            self.profile
        )
    }
}

fn step_rows(step: &ApplyStep) -> Vec<usize> {
    match step {
        ApplyStep::Settings { rows, .. } => rows.iter().map(|(index, _)| *index).collect(),
        ApplyStep::MainModel { row, .. }
        | ApplyStep::McpAdd { row, .. }
        | ApplyStep::McpEnable { row, .. }
        | ApplyStep::McpRemove { row, .. }
        | ApplyStep::Capability { row, .. }
        | ApplyStep::CronAdd { row, .. }
        | ApplyStep::CronControl { row, .. } => vec![*row],
    }
}

fn sanitize_outcome(outcome: RowOutcome, detect: SecretDetector) -> RowOutcome {
    let clean = |text: String| super::reply::sanitize_text(&text, detect);
    match outcome {
        RowOutcome::Adjusted(text) => RowOutcome::Adjusted(clean(text)),
        RowOutcome::NoChange(text) => RowOutcome::NoChange(clean(text)),
        RowOutcome::NotApplied(text) => RowOutcome::NotApplied(clean(text)),
        RowOutcome::Conflict(text) => RowOutcome::Conflict(clean(text)),
        RowOutcome::Failed(text) => RowOutcome::Failed(clean(text)),
        RowOutcome::Unknown(text) => RowOutcome::Unknown(clean(text)),
        outcome => outcome,
    }
}

fn outcome_display(outcome: &RowOutcome) -> (String, Option<&str>) {
    match outcome {
        RowOutcome::Applied => ("✓".into(), None),
        RowOutcome::Adjusted(text) => ("~".into(), Some(text)),
        RowOutcome::NoChange(text) => ("=".into(), Some(text)),
        RowOutcome::NotApplied(text) | RowOutcome::Failed(text) => ("✗".into(), Some(text)),
        RowOutcome::Conflict(text) => ("!".into(), Some(text)),
        RowOutcome::Unknown(text) => ("?".into(), Some(text)),
        RowOutcome::NotAttempted => ("-".into(), Some("not attempted")),
    }
}

fn handoff_action(row: &Row) -> crate::commands::LocalAction {
    if matches!(&row.change, Change::Set { key, .. } if key == "memory.provider") {
        return crate::commands::LocalAction::MemorySettings;
    }
    let screen = match &row.change {
        Change::Open { screen, .. } => *screen,
        Change::McpAdd { .. } | Change::McpEnable { .. } | Change::McpRemove { .. } => Screen::Mcp,
        Change::Toggle { .. } => Screen::Capabilities,
        Change::CronAdd { .. }
        | Change::CronPause { .. }
        | Change::CronResume { .. }
        | Change::CronRemove { .. } => Screen::Cron,
        Change::MainModel { .. } => Screen::Providers,
        _ => Screen::Settings,
    };
    screen_action(screen, None)
}
pub fn parse_configure_args(arg: &str) -> Result<String, &'static str> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Err("Usage: /configure <what you want to change>");
    }
    if arg.chars().count() > 4_000 {
        return Err("The request is too long (at most 4,000 characters).");
    }
    Ok(arg.into())
}
pub fn screen_action(screen: Screen, _target: Option<&str>) -> crate::commands::LocalAction {
    use crate::commands::LocalAction;
    match screen {
        Screen::Providers => LocalAction::Providers(crate::providers::OpenTarget::Tab(
            crate::providers::ProvidersTab::Keys,
        )),
        Screen::Mcp => LocalAction::Mcp(crate::mcp::McpOpen::Tab(crate::mcp::McpTab::Servers)),
        Screen::Capabilities => LocalAction::Capabilities,
        Screen::Cron => LocalAction::Cron,
        Screen::Profiles => LocalAction::Profiles,
        Screen::Connectors => LocalAction::Connectors(crate::connectors::ConnectorsOpen::Tab(
            crate::connectors::ConnectorsTab::Connectors,
        )),
        Screen::Settings => LocalAction::Config,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN: &str = "sk-configure-test-token";

    #[test]
    fn memory_provider_proposals_only_open_the_owner_screen() {
        let context = context();
        let proposal = Proposal {
            profile: Some("fixture".into()),
            summary: "Choose memory".into(),
            changes: vec![Change::Set {
                key: "memory.provider".into(),
                value: json!("retaindb"),
                why: None,
            }],
        };
        let rows = crate::configure::validate(&proposal, &context, detect).unwrap();
        assert_eq!(rows[0].status, RowStatus::HandOff);
        assert!(rows[0].text.contains("/memory-settings"));
        assert_eq!(
            handoff_action(&rows[0]),
            crate::commands::LocalAction::MemorySettings
        );
        assert!(crate::configure::plan_apply(&rows, &[true], &context).is_empty());
    }

    fn detect(text: &str) -> bool {
        text.contains(TOKEN)
    }

    fn context() -> Context {
        Context {
            profile: "fixture".into(),
            hermes_version: None,
            schema: crate::settings::ConfigSchemaView {
                fields: vec![],
                category_order: vec![],
            },
            effective: json!({}),
            raw: json!({}),
            defaults: json!({}),
            inventory: Inventory {
                mcp_servers: Section::Unavailable("not gathered".into()),
                mcp_catalogue: Section::Unavailable("not gathered".into()),
                skills: Section::Unavailable("not gathered".into()),
                toolsets: Section::Unavailable("not gathered".into()),
                plugins: Section::Unavailable("not gathered".into()),
                cron_jobs: Section::Unavailable("not gathered".into()),
                providers: Section::Unavailable("not gathered".into()),
                main_model: Section::Unavailable("not gathered".into()),
                routes: Section::Unavailable("not gathered".into()),
                profiles: Section::Unavailable("not gathered".into()),
                connectors: Section::Unavailable("not gathered".into()),
            },
        }
    }

    #[test]
    fn application_run_prefixes_separate_create_and_submit_idempotency_keys() {
        fn keys(prefix: &str) -> (String, String) {
            let now = Instant::now();
            let mut state = ConfigureState::new_with_prefix(detect, prefix);
            state.update(
                ConfigureInput::Open {
                    request: "turn on the completion bell".into(),
                    profile: "fixture".into(),
                    connected: true,
                    model: None,
                },
                now,
            );
            let effects = state.update(ConfigureInput::Gathered(Box::new(context())), now);
            let ConfigureEffect::CreateSession(spec) = &effects[0] else {
                panic!("gathered context must create a session");
            };
            let create = spec.request_id.clone();
            state.session_id = Some("configuration-session".into());
            let effects = state.submit("request".into(), now);
            let ConfigureEffect::Submit { submission_id, .. } = &effects[0] else {
                panic!("an admitted configuration request must submit");
            };
            (create, submission_id.clone())
        }

        let first = keys("talaria-launch-one-configure-one");
        let second = keys("talaria-launch-one-configure-two");
        let third = keys("talaria-launch-two-configure-one");
        assert_ne!(first.0, first.1);
        assert_ne!(first.0, second.0);
        assert_ne!(first.1, second.1);
        assert_ne!(first.0, third.0);
        assert_ne!(first.1, third.1);
        assert!(first.0.starts_with("talaria-launch-one-configure-one-"));
        assert!(first.1.starts_with("talaria-launch-one-configure-one-"));
    }

    #[test]
    fn refused_clarification_answer_stays_visible_inside_question_phase() {
        let now = Instant::now();
        let mut state = ConfigureState::new(detect);
        state.session_id = Some("configuration-session".into());
        state.session = Some(SessionState::empty(SessionId::new("configuration-session")));
        state.phase = Phase::Thinking;
        let raw = json!({
            "session_id":"configuration-session", "type":"clarify.request",
            "seq":1, "replay_epoch":"epoch-one", "execution_generation":1,
            "payload":{
                "kind":"clarify", "prompt_id":"question-one", "execution_generation":1,
                "question":"Which setting?", "choices":["skip"], "multi_select":false
            }
        });
        let frame = hermes_contract::events::EventFrame::from_params(&raw).unwrap();
        state.update(
            ConfigureInput::Session(Event::Session {
                session_id: SessionId::new("configuration-session"),
                frame,
                raw,
            }),
            now,
        );
        let effects = state.update(
            ConfigureInput::ClarifyAnswered {
                key: PromptKey {
                    session_id: SessionId::new("configuration-session"),
                    prompt_id: "question-one".into(),
                    execution_generation: 1,
                },
                answer: TOKEN.into(),
            },
            now,
        );
        assert_eq!(effects, vec![ConfigureEffect::ClearEditor]);
        let view = state.view(now);
        assert_eq!(view.phase, Phase::Question);
        let visible = view.text_lines().join("\n");
        assert!(visible.contains("Which setting?"));
        assert!(visible.contains(secret_input_message()));
        assert!(!format!("{visible} {state:?} {effects:?}").contains(TOKEN));
    }

    #[test]
    fn streaming_keeps_recent_complete_lines_and_checks_secrets_before_retention() {
        let mut state = ConfigureState::new(detect);
        state.phase = Phase::Thinking;
        for index in 0..20 {
            state.stream(&format!("safe line {index}\n"));
        }
        assert_eq!(state.prose.len(), 12);
        assert_eq!(state.prose.first().unwrap(), "safe line 8");
        state.stream("sk-configure-");
        let before_newline = state.view(Instant::now()).text_lines().join("\n");
        assert!(!before_newline.contains("sk-configure-"));
        state.stream("test-token");
        assert!(!format!("{state:?}").contains(TOKEN));
        state.stream("\n");
        assert_eq!(state.prose.len(), 12);
        assert_eq!(state.prose.last().unwrap(), WITHHELD);
        state.stream(
            "```talaria-configure\n{\"hidden\":\"model block\"}\n```\nvisible after block\n",
        );
        let visible = state.view(Instant::now()).text_lines().join("\n");
        assert!(!visible.contains(TOKEN));
        assert!(!visible.contains("model block"));
        assert!(visible.contains(WITHHELD));
        assert!(visible.contains("visible after block"));
        assert_eq!(state.prose.len(), 12);
    }
}
