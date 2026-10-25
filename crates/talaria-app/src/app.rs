//! Application loop: one channel, a pure update, and the effect executor.
//!
//! The loop owns the session state, the link and the services. It reaches the terminal only
//! through [`Screen`], which `talaria-cli` implements with `talaria-tui`, so this crate never
//! depends on a terminal library. Every gateway call runs in a spawned task and comes back as an
//! [`AppMessage`]; the loop itself never waits on the network.

use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hermes_client::liveness::{Link, LinkEvent, LinkState};
use hermes_client::record::{RecordError, Recorder, RecorderOptions};
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_client::{Inbound, ServerRequest};
use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::commands::{
    CommandOutcome, CommandsEffect, CommandsInput, CommandsState, LocalAction, LocalCommandTable,
    PaletteContext, PaletteEffect, PaletteInput, PaletteState, palette_update, parse_slash_line,
    state_update,
};
use talaria_core::models::{
    ConfirmKey, ModelCommandOrEffort, ModelsEffect, ModelsInput, ModelsState, NoticeLevel,
    PickerKey, PickerView, SavedModels, parse_effort_command, parse_model_command,
};
use talaria_core::mutation::Confirmation;
use talaria_core::rewind::{
    CallFailure, DiffCommand, DiffSource, DoubleEsc, Reply, RewindEffect, RewindInput, RewindKey,
    RewindState, RewindView, RollbackCommand, parse_diff as parse_diff_command, parse_rollback,
    parse_undo, text as rewind_text, update as rewind_update,
};
use talaria_core::session::{
    Cell, Effect, Event, NoticeKind, PromptKey, PromptKind, PromptState, SessionId, SessionState,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::commands::{
    AppAction, CommandReply, CommandService, no_session_notice, outcome_actions,
};
use crate::connection::{ConnectionService, OpenConnection, find_git_toplevel};
use crate::failure::{self, LaunchFailure};
use crate::launch::{
    Belongs, Launch, ListRow, MAX_HELD_PROMPTS, RESUME_HISTORY_CELLS, SessionRequest, StartMode,
    belongs, merge_rows, new_request_id, plain, plan, workspace_key,
};
use crate::prompts::{MaskedRequestInfo, PromptService, ServerRequestPolicy, policy_for};
use crate::sessions::{
    DetachOutcome, ExecuteOutcome, OpenError, OpenedSession, SessionInfoSummary, SessionService,
    accept_event, submission_id,
};

/// How long a quit waits for a create or resume that is still in flight.
pub const QUIT_WAIT: Duration = Duration::from_secs(5);
/// The bound passed to the prompt service's deny before an interrupt (it caps it there too).
pub const DENY_WAIT: Duration = Duration::from_secs(2);
/// Titles in the resume separator are cut to this many characters, with no ellipsis.
pub const TITLE_CHARS: usize = 60;
/// Unsent prompts printed after a quit are cut to this many characters.
const HELD_PRINT_CHARS: usize = 80;
/// At most this many ready messages are applied after the one that woke the loop, before
/// the screen is drawn.
const MESSAGE_BATCH: usize = 64;

pub const SESSION_GONE_NOTICE: &str = "This session no longer exists on the gateway.";
pub const STILL_WORKING: &str = "The agent is still working on it in the background.";
pub const HELD_FULL_NOTICE: &str =
    "Too many prompts are waiting for Hermes. Wait a moment, then send this one again.";
pub const REFRESHED_NOTICE: &str = "Reconnected. The transcript was refreshed.";
pub const STALE_SUBSCRIPTION: &str =
    "Hermes no longer knew this connection's subscription; the session is unaffected.";
const LINK_STOPPED_NOTICE: &str = "This prompt was not sent: the connection to Hermes has stopped.";
const REFRESH_FAILED_NOTICE: &str = "The transcript could not be refreshed after the reconnect.";

/// What the link is doing, as the screen shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkView {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

/// What a person asked for, after the screen routed a key or a paste.
pub enum UserAction {
    /// A key for the open rewind overlay. Issue #233.
    Rewind(RewindKey),
    /// How many list rows the rewind overlay shows.
    RewindViewport(u16),
    /// Esc reached the composer. `eligible` when the composer was empty and idle: two eligible
    /// presses close together open the rewind overlay.
    Escape {
        eligible: bool,
    },
    OpenModels,
    ModelPicker(PickerKey),
    ModelConfirm(ConfirmKey),
    /// The composer sent this text.
    Submit(String),
    /// Stop the running turn (Esc, or Ctrl+C on a card or an empty draft).
    Interrupt,
    /// `/quit` or the composer's exit request.
    Quit,
    /// An approval card was answered.
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    /// A clarification card was answered.
    Clarify {
        key: PromptKey,
        answer: String,
    },
    /// A masked card was answered; an empty value declines it.
    Masked {
        request_id: String,
        value: Zeroizing<String>,
    },
    /// A line the screen wants shown.
    Notice(String),
    /// The draft changed, or a completion-list key was pressed. Issue #223.
    Palette(PaletteInput),
}

/// Everything the screen draws from, borrowed for one draw.
pub struct View<'a> {
    pub state: Option<&'a SessionState>,
    pub session_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub info: Option<&'a SessionInfoSummary>,
    pub link: LinkView,
    pub profile: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub masked: &'a [MaskedRequestInfo],
    pub held: usize,
    /// The slash palette under the composer.
    pub palette: &'a PaletteState,
    /// The rewind overlay, while it is open. Issue #233.
    pub rewind: Option<RewindView<'a>>,
}

impl View<'_> {
    /// A turn is running in the session.
    #[must_use]
    pub fn turn_running(&self) -> bool {
        self.state
            .is_some_and(|state| *state.turn() == talaria_core::session::TurnState::Running)
    }
}

/// The terminal side of the loop.
pub trait Screen: ComposerDeadline + StatusHost {
    /// Synchronize model overlays and cards without passing terminal state into the app.
    fn sync_models(
        &mut self,
        _picker: Option<PickerView<'_>>,
        _confirmation: Option<&Confirmation>,
    ) {
    }
    fn model_list_height(&self) -> usize {
        10
    }
    /// One raw input: a key, a paste, a resize, or a status command result.
    type Input: Send + 'static;
    /// Route one input and redraw; returns what the person asked for.
    fn input(&mut self, input: Self::Input, view: &View<'_>, now: Instant) -> Vec<UserAction>;
    /// Draw the live region.
    fn draw(&mut self, view: &View<'_>);
    /// Write plain lines into the scrollback, once.
    fn commit_lines(&mut self, lines: Vec<String>);
    /// Render cells into the scrollback, once.
    fn commit_cells(&mut self, cells: &[Cell]);
    /// Put text back into the composer.
    fn load_draft(&mut self, text: &str);
    /// Give the terminal back. Later draws write nothing.
    fn restore(&mut self);
}

pub trait ComposerDeadline {
    fn tick(&mut self, now: Instant);
    fn next_tick(&self) -> Option<Instant>;
}

pub trait StatusHost {
    fn on_tick(&mut self, now: Instant);
    fn next_wake(&self, now: Instant) -> Option<Instant>;
    fn shutdown(&mut self) -> impl Future<Output = ()> + Send;
}

/// A session opened by the background start-up task.
pub struct Ready {
    pub connection: OpenConnection,
    pub service: SessionService,
    pub opened: OpenedSession,
    /// The "works in another folder" notice of a resume by id.
    pub notice: Option<String>,
}

pub struct AppState {
    pub models: ModelsState,
    model_service: Option<Arc<crate::models::ModelService>>,
    model_notice_counter: u64,
    model_warnings: BTreeSet<String>,
    pub opened: Option<OpenedSession>,
    pub held: VecDeque<String>,
    pub launch_id: String,
    pub prompt_counter: u64,
    pub quitting: bool,
    pub committed_cells: usize,
    pub link_failed: bool,
    pub session_gone: bool,
    /// The profile the resume hint names.
    pub profile: String,
    pub link_state: LinkView,
    /// An interrupt sequence is in flight; further presses send nothing.
    pub interrupting: bool,
    /// The title of the row picked from the `--resume` list.
    pub list_title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub masked: Vec<MaskedRequestInfo>,
    /// Composer draft. The slash palette reads it. Issue #223.
    pub draft: String,
    /// Slash palette. Closed until a slash draft opens it. Issue #223.
    pub palette: PaletteState,
    /// The catalogue store for this connection.
    pub commands: CommandsState,
    /// Commands Talaria runs itself.
    pub local: LocalCommandTable,
    /// The rewind overlay and the typed rewind commands. Issue #233.
    rewind: RewindState,
    /// Two quick eligible Esc presses open the rewind overlay.
    double_esc: DoubleEsc,
    /// The `/retry` submission still waiting for its outcome, with its text.
    retry: Option<(String, String)>,
    open_keys: Vec<PromptKey>,
    counter: Arc<AtomicU64>,
    service: Option<SessionService>,
    link: Option<Link>,
    prompts: Option<Arc<Mutex<PromptService>>>,
    pending: Option<JoinHandle<Result<Ready, LaunchFailure>>>,
    command_service: Option<CommandService>,
}

impl AppState {
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        let launch_id = launch_id.into();
        Self {
            models: ModelsState::new(&launch_id, Ok(SavedModels::empty())),
            model_service: None,
            model_notice_counter: 0,
            model_warnings: BTreeSet::new(),
            opened: None,
            held: VecDeque::new(),
            launch_id,
            prompt_counter: 0,
            quitting: false,
            committed_cells: 0,
            link_failed: false,
            session_gone: false,
            profile: "default".to_string(),
            link_state: LinkView::Connecting,
            interrupting: false,
            list_title: None,
            model: None,
            effort: None,
            masked: Vec::new(),
            draft: String::new(),
            palette: PaletteState::closed(),
            commands: CommandsState::default(),
            local: LocalCommandTable::builtin(),
            rewind: RewindState::default(),
            double_esc: DoubleEsc::default(),
            retry: None,
            open_keys: Vec::new(),
            counter: Arc::new(AtomicU64::new(0)),
            service: None,
            link: None,
            prompts: None,
            pending: None,
            command_service: None,
        }
    }

    /// The view the screen draws from.
    #[must_use]
    pub fn view(&self) -> View<'_> {
        let opened = self.opened.as_ref();
        View {
            state: opened.map(|opened| &opened.state),
            session_id: opened.map(|opened| opened.session_id.as_str()),
            title: opened
                .and_then(|opened| opened.title.as_deref())
                .or(self.list_title.as_deref()),
            info: opened.map(|opened| &opened.info),
            link: self.link_state,
            profile: &self.profile,
            model: self
                .model
                .as_deref()
                .or_else(|| opened.and_then(|opened| opened.info.model.as_deref())),
            effort: self.effort.as_deref(),
            masked: &self.masked,
            held: self.held.len(),
            palette: &self.palette,
            rewind: self.rewind.is_open().then(|| self.rewind.view(wall_ms())),
        }
    }

    fn next_submission_id(&mut self) -> String {
        self.prompt_counter = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        submission_id(&self.launch_id, self.prompt_counter)
    }

    fn has_user_message(&self) -> bool {
        self.opened.as_ref().is_some_and(|opened| {
            opened
                .state
                .cells()
                .iter()
                .any(|cell| matches!(cell, Cell::User(_)))
        })
    }
}

/// One input to [`update`].
#[allow(clippy::large_enum_variant)]
pub enum AppMessage {
    Models(ModelsInput),
    User(UserAction),
    Link(LinkEvent),
    /// The session is open: commit its history, track it, send held prompts.
    Opened {
        opened: OpenedSession,
        notice: Option<String>,
    },
    SubmitDone {
        submission_id: String,
        outcome: ExecuteOutcome,
    },
    InterruptDone {
        outcome: ExecuteOutcome,
        notices: Vec<String>,
    },
    PromptDone {
        events: Vec<Event>,
        notices: Vec<String>,
        masked: Option<Vec<MaskedRequestInfo>>,
    },
    Recovered(Result<OpenedSession, OpenError>),
    /// A key while Talaria is in the foreground. Issue #223 routes the open palette here.
    ForegroundKey(UserAction),
    /// A catalogue fetch or a completion finished.
    Commands(CommandReply),
    /// A slash command finished; `typed` is the line as it was run.
    CommandDone {
        typed: String,
        outcome: CommandOutcome,
    },
    /// A rewind call's reply, or another rewind input. Issue #233.
    Rewind(talaria_core::rewind::RewindInput),
}

/// What the executor does after one update. Ranges index the open session's cells.
pub enum AppEffect {
    Models(ModelsEffect),
    /// Run one rewind call; its reply comes back as [`AppMessage::Rewind`]. Issue #233.
    Rewind {
        ticket: u64,
        call: talaria_core::rewind::Call,
    },
    CommitLines(Vec<String>),
    CommitCells(Range<usize>),
    Redraw,
    LoadDraft(String),
    /// `prompt.submit`.
    Send(Effect),
    /// Deny open approvals, then `session.interrupt`.
    Interrupt(Effect),
    /// An approval or clarification answer, with the note to attach first.
    Respond {
        effect: Effect,
        note: Option<(PromptKey, String)>,
    },
    AnswerMasked {
        request_id: String,
        value: Zeroizing<String>,
    },
    ServerRequest(ServerRequest),
    ConnectionOpened(u64),
    PromptsChanged(Vec<PromptKey>),
    Track {
        session_id: String,
        cursor: Cursor,
    },
    Resync {
        session_id: String,
        cursor: Cursor,
    },
    Recover(ReplayGap),
    Quit,
    /// Run a Hermes slash command (`slash.exec`) in the background.
    RunCommand {
        session_id: String,
        typed: String,
    },
    /// `commands.catalog`, after `after`.
    FetchCatalog {
        request_id: u64,
        session_id: Option<String>,
        after: Duration,
    },
    /// `complete.slash` for the draft, once it has been still for `after`.
    RequestCompletion {
        request_id: u64,
        text: String,
        after: Duration,
        session_id: Option<String>,
    },
    /// Drop the pending completion request.
    CancelCompletion,
    /// Report the foreground pane state (issue 228; stub: never produced yet).
    ReportPane(talaria_core::attention::PaneReport),
}

fn notice_lines(text: &str) -> AppEffect {
    AppEffect::CommitLines(text.lines().map(|line| format!("\u{b7} {line}")).collect())
}

fn model_card_blocked(state: &AppState) -> bool {
    !state.masked.is_empty()
        || state
            .opened
            .as_ref()
            .is_some_and(|s| s.state.open_prompts().any(|p| p.state == PromptState::Open))
}

fn sync_models<S: Screen>(state: &mut AppState, screen: &mut S) {
    state.models.set_list_height(screen.model_list_height());
    screen.sync_models(
        state.models.view(),
        if model_card_blocked(state) {
            None
        } else {
            state.models.confirmation()
        },
    );
}

fn models_input(state: &mut AppState, input: ModelsInput) -> Vec<AppEffect> {
    let current = state.opened.as_ref().map(|s| s.session_id.as_str());
    match &input {
        ModelsInput::OptionsLoaded { session_id, result }
            if Some(session_id.as_str()) == current =>
        {
            state.model = Some(result.model.clone());
            if let Some(opened) = &mut state.opened {
                opened.info.provider = Some(result.provider.clone());
            }
        }
        ModelsInput::EffortRead { session_id, value } if Some(session_id.as_str()) == current => {
            state.effort = Some(value.clone().unwrap_or_else(|| "medium".into()));
        }
        ModelsInput::SessionEvent {
            session_id,
            payload,
            type_name,
        } if Some(session_id.as_str()) == current
            && type_name == "session.updated"
            && payload.get("status").and_then(Value::as_str) == Some("confirmation_required") =>
        {
            if let Some(model) = payload.get("committed_model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
        }
        ModelsInput::SessionEvent {
            session_id,
            payload,
            type_name,
        } if Some(session_id.as_str()) == current && type_name == "session.info" => {
            if let Some(model) = payload.get("model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
            if let Some(provider) = payload.get("provider").and_then(Value::as_str)
                && let Some(opened) = &mut state.opened
            {
                opened.info.provider = Some(provider.into());
            }
            if let Some(effort) = payload
                .get("reasoning_effort")
                .or_else(|| payload.get("reasoning"))
                .and_then(Value::as_str)
            {
                state.effort = Some(effort.into());
            }
        }
        ModelsInput::MutationReturned {
            request,
            outcome: talaria_core::mutation::MutationOutcome::Committed(receipt),
        } if Some(request.session_id.as_str()) == current => {
            if let Some(model) = receipt.get("model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
            if let Some(effort) = receipt.get("effort").and_then(Value::as_str) {
                state.effort = Some(effort.into());
            }
            if let Some(provider) = receipt.get("provider").and_then(Value::as_str)
                && let Some(opened) = &mut state.opened
            {
                opened.info.provider = Some(provider.into());
            }
        }
        _ => {}
    }
    state
        .models
        .update(input)
        .into_iter()
        .map(AppEffect::Models)
        .collect()
}

/// How a foreground rebuild was caused.
pub enum RebuildMode {
    /// Gap recovery, unchanged.
    AfterGap,
    /// A rewind or a restore finished. `notice` is committed and nothing above it is reprinted.
    AfterRewind { notice: Vec<String> },
}

/// Replace the foreground session from a fresh snapshot.
///
/// `AfterGap` calls the existing gap recovery. `AfterRewind` is issue 233.
pub fn rebuild_foreground(
    state: &mut AppState,
    snapshot: Value,
    mode: RebuildMode,
) -> Vec<AppEffect> {
    match mode {
        RebuildMode::AfterGap => rebuild_after_gap(state, snapshot),
        RebuildMode::AfterRewind { notice } => rebuild_after_rewind(state, snapshot, notice),
    }
}

fn rebuild_after_gap(state: &mut AppState, snapshot: Value) -> Vec<AppEffect> {
    let result =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => Ok(crate::sessions::OpenedSession::from_snapshot(decoded)),
            Err(_) => Err(crate::sessions::OpenError::Link(
                "the session reply did not decode".to_string(),
            )),
        };
    recovered(state, result)
}

/// Replace the foreground state after a rewind or a restore without printing anything again.
///
/// Cells already in scrollback are matched in order and skipped; the notice is committed, then
/// any final cell this pane never showed (another window's new turn), once.
fn rebuild_after_rewind(
    state: &mut AppState,
    snapshot: Value,
    notice: Vec<String>,
) -> Vec<AppEffect> {
    let fresh =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => crate::sessions::OpenedSession::from_snapshot(decoded),
            Err(_) => {
                return vec![notice_lines(&rewind_text::reload_failed(
                    "the session reply did not decode",
                ))];
            }
        };
    let old = state.opened.take();
    let shown: &[Cell] = old.as_ref().map_or(&[], |old| {
        &old.state.cells()[..state.committed_cells.min(old.state.cells().len())]
    });
    let cells = fresh.state.cells();
    let total = final_prefix(cells);
    let mut position = 0;
    let mut unseen: Vec<usize> = Vec::new();
    for (index, cell) in cells[..total].iter().enumerate() {
        match shown[position..].iter().position(|seen| seen == cell) {
            Some(offset) => position += offset + 1,
            None => unseen.push(index),
        }
    }
    let mut effects = Vec::new();
    if !notice.is_empty() {
        effects.push(AppEffect::CommitLines(notice));
    }
    let mut start = 0;
    while start < unseen.len() {
        let mut end = start + 1;
        while end < unseen.len() && unseen[end] == unseen[end - 1] + 1 {
            end += 1;
        }
        effects.push(AppEffect::CommitCells(unseen[start]..unseen[end - 1] + 1));
        start = end;
    }
    state.committed_cells = total;
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    let mut fresh = fresh;
    if fresh.title.is_none() {
        fresh.title = old.and_then(|old| old.title);
    }
    state.opened = Some(fresh);
    effects
}

/// Wall-clock milliseconds, for the double Esc window and the prompts' ages.
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// One of the five rewind commands, typed or picked. Issue #233.
fn rewind_command(state: &mut AppState, action: LocalAction, argument: &str) -> Vec<AppEffect> {
    let name = match action {
        LocalAction::Rewind => "rewind",
        LocalAction::Undo => "undo",
        LocalAction::Retry => "retry",
        LocalAction::Rollback => "rollback",
        _ => "diff",
    };
    if state.link_failed {
        return vec![notice_lines(&format!(
            "Talaria is not connected to Hermes, so /{name} did not run."
        ))];
    }
    let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
        return vec![notice_lines(&no_session_notice(name))];
    };
    let turn_running = *opened.state.turn() == talaria_core::session::TurnState::Running;
    let input = match action {
        LocalAction::Rewind => RewindInput::Open { turn_running },
        LocalAction::Undo => match parse_undo(argument) {
            Ok(n) => RewindInput::Undo { n, turn_running },
            Err(()) => return vec![notice_lines(rewind_text::UNDO_USAGE)],
        },
        LocalAction::Retry => RewindInput::Retry { turn_running },
        LocalAction::Rollback => match parse_rollback(argument) {
            RollbackCommand::List => RewindInput::OpenCheckpoints,
            RollbackCommand::Diff(target) => RewindInput::OpenDiff(DiffSource::Checkpoint(target)),
            RollbackCommand::Restore {
                target,
                file_path,
                all,
            } => RewindInput::TypedRestore {
                target,
                file_path,
                all,
                turn_running,
            },
            RollbackCommand::BadTarget(target) => {
                return vec![notice_lines(&rewind_text::no_such_checkpoint(&target))];
            }
            RollbackCommand::Usage => return vec![notice_lines(rewind_text::ROLLBACK_USAGE)],
        },
        _ => match parse_diff_command(argument) {
            DiffCommand::Session { stat_only } => {
                RewindInput::OpenDiff(DiffSource::Session { stat_only })
            }
            DiffCommand::GitView => return vec![notice_lines(rewind_text::GIT_DIFF_HINT)],
        },
    };
    rewind_input(state, input)
}

/// Apply one rewind input and carry out what it asks for.
fn rewind_input(state: &mut AppState, input: RewindInput) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in rewind_update(&mut state.rewind, input, wall_ms()) {
        match effect {
            RewindEffect::Call { ticket, call } => effects.push(AppEffect::Rewind { ticket, call }),
            RewindEffect::Rebuild { snapshot, notice } => effects.extend(rebuild_foreground(
                state,
                snapshot,
                RebuildMode::AfterRewind { notice },
            )),
            RewindEffect::ReturnPrompt { text, attachments } => {
                effects.extend(return_prompt(state, text, attachments));
            }
            RewindEffect::Submit(text) => {
                let submission_id = state.next_submission_id();
                state.retry = Some((submission_id.clone(), text.clone()));
                effects.extend(reduce(
                    state,
                    Event::Submit {
                        text,
                        submission_id,
                    },
                ));
            }
            RewindEffect::Notice(_, text) => effects.push(notice_lines(&text)),
        }
    }
    effects.push(AppEffect::Redraw);
    effects
}

/// Put a rewound prompt back, but only into an empty composer; a draft is never overwritten.
fn return_prompt(state: &mut AppState, text: String, attachments: bool) -> Vec<AppEffect> {
    let mut effects = if !state.draft.is_empty() {
        vec![notice_lines(&rewind_text::draft_kept(&text))]
    } else if text.is_empty() {
        Vec::new()
    } else {
        load_draft(state, text)
    };
    if attachments {
        effects.push(notice_lines(rewind_text::ATTACHMENTS_NOT_RESTORED));
    }
    effects
}

/// Apply one message. No input or output: the effects say what to do.
pub fn update(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let mut effects = match message {
        AppMessage::Models(input) => models_input(state, input),
        AppMessage::User(action) => user_action(state, action),
        AppMessage::Link(event) => link_event(state, event),
        AppMessage::Opened { opened, notice } => opened_session(state, opened, notice),
        AppMessage::SubmitDone {
            submission_id,
            outcome,
        } => submit_done(state, submission_id, outcome),
        AppMessage::InterruptDone { outcome, notices } => interrupt_done(state, outcome, notices),
        AppMessage::PromptDone {
            events,
            notices,
            masked,
        } => {
            if let Some(masked) = masked {
                state.masked = masked;
            }
            let mut effects: Vec<AppEffect> =
                notices.iter().map(|text| notice_lines(text)).collect();
            for event in events {
                effects.extend(reduce(state, event));
            }
            effects
        }
        AppMessage::Recovered(result) => recovered(state, result),
        AppMessage::Rewind(input) => rewind_input(state, input),
        AppMessage::ForegroundKey(action) => foreground_key(state, action),
        AppMessage::Commands(CommandReply::Commands(input)) => commands_input(state, input),
        AppMessage::Commands(CommandReply::Palette(input)) => {
            let (effects, _) = palette(state, input);
            effects
        }
        AppMessage::CommandDone { typed, outcome } => command_done(state, &typed, outcome),
    };
    if model_card_blocked(state) {
        effects.extend(models_input(state, ModelsInput::CardOpened));
    }
    effects.extend(commit_ready(state));
    effects.push(AppEffect::Redraw);
    effects
}

/// Feed the reducer and turn its effects into executor effects.
fn reduce(state: &mut AppState, event: Event) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_mut() else {
        return Vec::new();
    };
    let produced = talaria_core::session::update(&mut opened.state, event);
    route(state, produced, None)
}

fn route(
    state: &mut AppState,
    produced: Vec<Effect>,
    mut note: Option<(PromptKey, String)>,
) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            Effect::SendPrompt { .. } => effects.push(AppEffect::Send(effect)),
            Effect::Interrupt { .. } => {
                if !state.interrupting {
                    state.interrupting = true;
                    effects.push(AppEffect::Interrupt(effect));
                }
            }
            Effect::RespondApproval { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => effects.push(AppEffect::Respond {
                effect,
                note: note.take(),
            }),
        }
    }
    effects
}

fn user_action(state: &mut AppState, action: UserAction) -> Vec<AppEffect> {
    match action {
        UserAction::Rewind(key) => rewind_input(state, RewindInput::Key(key)),
        UserAction::RewindViewport(rows) => rewind_input(state, RewindInput::Viewport(rows)),
        UserAction::Escape { eligible } => {
            if state.double_esc.press(wall_ms(), eligible) && state.opened.is_some() {
                rewind_input(
                    state,
                    RewindInput::Open {
                        turn_running: false,
                    },
                )
            } else {
                Vec::new()
            }
        }
        UserAction::OpenModels => models_input(
            state,
            ModelsInput::OpenPicker {
                session_id: state.opened.as_ref().map(|s| s.session_id.clone()),
                profile: state.profile.clone(),
            },
        ),
        UserAction::ModelPicker(key) => models_input(state, ModelsInput::Picker(key)),
        UserAction::ModelConfirm(key) => models_input(state, ModelsInput::Confirm(key)),
        UserAction::Submit(text) => {
            // The composer has taken the draft.
            let mut effects = if state.draft.is_empty() && !state.palette.open {
                Vec::new()
            } else {
                palette_input(
                    state,
                    PaletteInput::DraftChanged {
                        text: String::new(),
                        caret: 0,
                    },
                )
            };
            effects.extend(submit(state, text));
            effects
        }
        UserAction::Interrupt => {
            if state.interrupting {
                return Vec::new();
            }
            reduce(state, Event::Interrupt)
        }
        UserAction::Quit => {
            state.quitting = true;
            vec![AppEffect::Quit]
        }
        UserAction::Approve { key, choice, note } => {
            let Some(opened) = state.opened.as_mut() else {
                return Vec::new();
            };
            let note = note.map(|note| (key.clone(), note));
            let produced = talaria_core::session::update(
                &mut opened.state,
                Event::AnswerApproval { key, choice },
            );
            route(state, produced, note)
        }
        UserAction::Clarify { key, answer } => reduce(state, Event::AnswerClarify { key, answer }),
        UserAction::Masked { request_id, value } => {
            vec![AppEffect::AnswerMasked { request_id, value }]
        }
        UserAction::Notice(text) => vec![notice_lines(&text)],
        UserAction::Palette(input) => palette_input(state, input),
    }
}

/// Keys in the foreground: an open palette takes Enter and Esc before the composer and the
/// turn do, so Esc closes the palette and never interrupts a running turn.
fn foreground_key(state: &mut AppState, action: UserAction) -> Vec<AppEffect> {
    if !state.palette.open {
        return user_action(state, action);
    }
    match action {
        UserAction::Interrupt => palette_input(state, PaletteInput::Close),
        UserAction::Submit(text) => palette_enter(state, text),
        other => user_action(state, other),
    }
}

/// Apply one palette input and turn its completion request into an executor effect. The
/// draft and run effects come back for the caller, which knows what the composer holds.
fn palette(state: &mut AppState, input: PaletteInput) -> (Vec<AppEffect>, Vec<PaletteEffect>) {
    let pending = state.palette.request_id;
    let ctx = PaletteContext {
        catalog: state.commands.catalog.as_ref(),
        catalog_status: &state.commands.status,
        local: &state.local,
        refused: &state.commands.refused,
    };
    let produced = palette_update(&mut state.palette, input, &ctx);
    let mut effects = Vec::new();
    let mut rest = Vec::new();
    let mut requested = false;
    for effect in produced {
        match effect {
            PaletteEffect::RequestCompletion {
                request_id,
                text,
                after,
            } => {
                requested = true;
                effects.push(AppEffect::RequestCompletion {
                    request_id,
                    text,
                    after,
                    session_id: state.commands.bound_session.clone(),
                });
            }
            PaletteEffect::Redraw => {}
            other => rest.push(other),
        }
    }
    if pending != 0 && !requested && state.palette.request_id != pending {
        effects.insert(0, AppEffect::CancelCompletion);
    }
    (effects, rest)
}

/// The composer will hold `text`: load it and let the palette follow.
fn load_draft(state: &mut AppState, text: String) -> Vec<AppEffect> {
    state.draft.clone_from(&text);
    let caret = text.len();
    let (mut effects, _) = palette(
        state,
        PaletteInput::DraftChanged {
            text: text.clone(),
            caret,
        },
    );
    effects.insert(0, AppEffect::LoadDraft(text));
    effects
}

/// Follow what a palette key asked for: run a line (the composer is left empty), or put a
/// completed draft into the composer.
fn follow(state: &mut AppState, produced: Vec<PaletteEffect>) -> Vec<AppEffect> {
    let mut replace = None;
    for effect in produced {
        match effect {
            PaletteEffect::Run { line } => {
                state.draft.clear();
                let (mut effects, _) = palette(
                    state,
                    PaletteInput::DraftChanged {
                        text: String::new(),
                        caret: 0,
                    },
                );
                effects.extend(submit(state, line));
                return effects;
            }
            PaletteEffect::ReplaceDraft { text } => replace = Some(text),
            PaletteEffect::RequestCompletion { .. } | PaletteEffect::Redraw => {}
        }
    }
    replace.map_or_else(Vec::new, |text| load_draft(state, text))
}

fn palette_input(state: &mut AppState, input: PaletteInput) -> Vec<AppEffect> {
    if let PaletteInput::DraftChanged { text, .. } = &input {
        state.draft.clone_from(text);
    }
    let (mut effects, produced) = palette(state, input);
    effects.extend(follow(state, produced));
    effects
}

/// Enter while the palette is open. The composer has already taken the draft, so when the
/// palette neither runs nor completes anything the draft goes back.
fn palette_enter(state: &mut AppState, text: String) -> Vec<AppEffect> {
    let (mut effects, produced) = palette(state, PaletteInput::Enter);
    if produced.is_empty() {
        effects.push(AppEffect::LoadDraft(text));
        return effects;
    }
    effects.extend(follow(state, produced));
    effects
}

/// Apply one catalogue store input and turn its effects into executor effects.
fn commands_input(state: &mut AppState, input: CommandsInput) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in state_update(&mut state.commands, input) {
        match effect {
            CommandsEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            } => effects.push(AppEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            }),
            CommandsEffect::CatalogUpdated => {
                let (refreshed, _) = palette(state, PaletteInput::CatalogChanged);
                effects.extend(refreshed);
            }
        }
    }
    effects
}

/// The catalogue store follows the connection only once the session service exists.
fn commands_follow(state: &mut AppState, input: CommandsInput) -> Vec<AppEffect> {
    if state.service.is_none() {
        return Vec::new();
    }
    commands_input(state, input)
}

/// Apply a finished command's outcome: notices into the transcript, a directive's prompt
/// through the normal submission path with a fresh id, a refused bare name dimmed.
fn command_done(state: &mut AppState, typed: &str, outcome: CommandOutcome) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for action in outcome_actions(outcome, typed) {
        match action {
            AppAction::Event(Event::SubmitCommand { text, display, .. }) => {
                let submission_id = state.next_submission_id();
                effects.extend(reduce(
                    state,
                    Event::SubmitCommand {
                        text,
                        display,
                        submission_id,
                    },
                ));
            }
            AppAction::Event(Event::Notice { kind, text }) if state.opened.is_some() => {
                effects.extend(reduce(state, Event::Notice { kind, text }));
            }
            AppAction::Event(Event::Notice { text, .. }) => effects.push(notice_lines(&text)),
            AppAction::Event(event) => effects.extend(reduce(state, event)),
            AppAction::LoadDraft(text) => effects.extend(load_draft(state, text)),
            AppAction::Refused { name } => {
                effects.extend(commands_input(state, CommandsInput::Refused { name }));
            }
            AppAction::Key(action) => effects.extend(user_action(state, action)),
            AppAction::CatalogUpdated => {
                let (refreshed, _) = palette(state, PaletteInput::CatalogChanged);
                effects.extend(refreshed);
            }
        }
    }
    effects
}

/// A slash line that names a Hermes command: run it through the session's gateway, or say why
/// it cannot run. Nothing is sent without a session.
fn run_command(state: &mut AppState, typed: String) -> Vec<AppEffect> {
    let name = parse_slash_line(&typed)
        .map(|line| line.name)
        .unwrap_or_default();
    if state.link_failed {
        return vec![notice_lines(&format!(
            "Talaria is not connected to Hermes, so /{name} did not run."
        ))];
    }
    let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
        return vec![notice_lines(&no_session_notice(&name))];
    };
    vec![AppEffect::RunCommand {
        session_id: opened.session_id.clone(),
        typed,
    }]
}

/// A local command this client runs itself: the model picker's own commands, which the
/// local table names and the loop turns into one model input.
fn local_command(state: &mut AppState, action: LocalAction, argument: &str) -> Vec<AppEffect> {
    let command = match action {
        LocalAction::Model => parse_model_command(argument).map(ModelCommandOrEffort::Model),
        LocalAction::Effort => parse_effort_command(argument).map(ModelCommandOrEffort::Effort),
        LocalAction::Rewind
        | LocalAction::Undo
        | LocalAction::Retry
        | LocalAction::Rollback
        | LocalAction::Diff => return rewind_command(state, action, argument),
        _ => return Vec::new(),
    };
    models_input(
        state,
        ModelsInput::Command {
            session_id: state.opened.as_ref().map(|s| s.session_id.clone()),
            profile: state.profile.clone(),
            command,
        },
    )
}

fn submit(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    match slash_line(&text) {
        Slash::Quit => {
            state.quitting = true;
            return vec![AppEffect::Quit];
        }
        Slash::Local(action, argument) => return local_command(state, action, &argument),
        Slash::Notice(notice) => {
            return match state.opened {
                Some(_) => reduce(
                    state,
                    Event::Notice {
                        kind: NoticeKind::Error,
                        text: notice,
                    },
                ),
                None => vec![notice_lines(&notice)],
            };
        }
        Slash::Command(line) => return run_command(state, line),
        Slash::Nothing => return Vec::new(),
        Slash::NotSlash => {}
    }
    if state.session_gone || state.link_failed {
        let notice = if state.session_gone {
            SESSION_GONE_NOTICE
        } else {
            LINK_STOPPED_NOTICE
        };
        return vec![notice_lines(notice), AppEffect::LoadDraft(text)];
    }
    if state.opened.is_none() {
        return match hold_prompt(state, &text) {
            Ok(()) => Vec::new(),
            Err(notice) => vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
        };
    }
    send_text(state, text)
}

fn send_text(state: &mut AppState, text: String) -> Vec<AppEffect> {
    let submission_id = state.next_submission_id();
    reduce(
        state,
        Event::Submit {
            text,
            submission_id,
        },
    )
}

fn event_position(raw: &Value) -> (Option<&str>, Option<u64>) {
    (
        raw.get("replay_epoch").and_then(Value::as_str),
        raw.get("seq").and_then(Value::as_u64),
    )
}

fn is_open_session(state: &AppState, session_id: &str) -> bool {
    state
        .opened
        .as_ref()
        .is_some_and(|opened| opened.session_id == session_id)
}

fn link_event(state: &mut AppState, event: LinkEvent) -> Vec<AppEffect> {
    match event {
        LinkEvent::State(LinkState::Connected { generation }) => {
            state.link_state = LinkView::Connected;
            let mut effects = vec![AppEffect::ConnectionOpened(generation)];
            if let Some(opened) = state.opened.as_mut() {
                let produced = note_connection(&mut opened.state, generation);
                effects.extend(route(state, produced, None));
            }
            effects.extend(commands_follow(state, CommandsInput::Connected));
            effects
        }
        LinkEvent::State(LinkState::Reconnecting { .. }) => {
            state.link_state = LinkView::Reconnecting;
            Vec::new()
        }
        LinkEvent::State(LinkState::Closed) => {
            state.link_state = LinkView::Closed;
            Vec::new()
        }
        event @ LinkEvent::State(LinkState::Failed { .. }) => {
            state.link_state = LinkView::Failed;
            state.link_failed = true;
            link_notice(&event)
                .map(|text| vec![notice_lines(&text)])
                .unwrap_or_default()
        }
        LinkEvent::Inbound { item, .. } => match item {
            // A skill root moved: refetch the catalogue. It is not a session event.
            Inbound::Event(inbound)
                if inbound.raw.get("type").and_then(Value::as_str) == Some("skills.changed") =>
            {
                commands_follow(state, CommandsInput::SkillsChanged)
            }
            Inbound::Event(inbound) => {
                let Some(opened) = state.opened.as_mut() else {
                    return Vec::new();
                };
                let session = inbound.raw.get("session_id").and_then(Value::as_str);
                let (epoch, seq) = event_position(&inbound.raw);
                if session != Some(opened.session_id.as_str())
                    || !accept_event(&opened.cursor, epoch, seq)
                {
                    return Vec::new();
                }
                let session_id = opened.session_id.clone();
                let type_name = inbound
                    .raw
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let payload = inbound.raw.get("payload").cloned().unwrap_or(Value::Null);
                if let Some(sequence) = seq {
                    opened.cursor.last_sequence = opened.cursor.last_sequence.max(sequence);
                }
                // `session.updated` still reaches the session reducer, which keeps the replay
                // cursor moving; a rewind from another window also rebuilds. Issue #233.
                let rewound = (type_name == "session.updated"
                    && payload.get("operation").and_then(Value::as_str) == Some("rewind"))
                .then(|| RewindInput::SessionUpdated {
                    operation: "rewind".to_string(),
                    revision: payload.get("revision").and_then(Value::as_i64),
                    rewound_count: payload.get("rewound_count").and_then(Value::as_u64),
                });
                let produced =
                    apply_inbound(&mut opened.state, &session_id, inbound.frame, inbound.raw);
                let mut effects = route(state, produced, None);
                if let Some(input) = rewound {
                    effects.extend(rewind_input(state, input));
                }
                effects.extend(models_input(
                    state,
                    ModelsInput::SessionEvent {
                        session_id,
                        type_name,
                        payload,
                    },
                ));
                effects
            }
            Inbound::ServerRequest(request) => vec![AppEffect::ServerRequest(request)],
            Inbound::Malformed(_) | Inbound::Closed(_) => Vec::new(),
        },
        LinkEvent::Gap(gap) => {
            if is_open_session(state, &gap.session_id) {
                vec![AppEffect::Recover(gap)]
            } else {
                Vec::new()
            }
        }
        LinkEvent::Reattached {
            session_id,
            subscription_id,
        } => {
            if let Some(opened) = state
                .opened
                .as_mut()
                .filter(|opened| opened.session_id == session_id)
            {
                opened.subscription_id = subscription_id;
            }
            Vec::new()
        }
        event @ LinkEvent::SessionGone { .. } => {
            let LinkEvent::SessionGone { session_id } = &event else {
                return Vec::new();
            };
            if !is_open_session(state, session_id) {
                return Vec::new();
            }
            state.session_gone = true;
            link_notice(&event)
                .map(|text| vec![notice_lines(&text)])
                .unwrap_or_default()
        }
    }
}

fn opened_session(
    state: &mut AppState,
    opened: OpenedSession,
    notice: Option<String>,
) -> Vec<AppEffect> {
    state.model = opened.info.model.clone();
    state.effort = opened.effort.clone();
    let mut effects = models_input(
        state,
        ModelsInput::ForegroundChanged {
            session_id: Some(opened.session_id.clone()),
        },
    );
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: opened.session_id.clone(),
            revision: Some(opened.revision),
            execution_generation: Some(opened.execution_generation),
        },
    ));
    for text in &opened.launch_notices {
        if !text.starts_with("~/.talaria/models.json could not be read (")
            || state.model_warnings.insert(text.clone())
        {
            effects.push(notice_lines(text));
        }
    }
    let title = session_title(
        opened.title.as_deref(),
        state.list_title.as_deref(),
        opened.state.cells(),
    );
    let (header, range) = resume_header(opened.state.cells(), &title);
    if !header.is_empty() {
        effects.push(AppEffect::CommitLines(header));
        effects.push(AppEffect::CommitCells(range.clone()));
    }
    state.committed_cells = range.end;
    effects.push(AppEffect::Track {
        session_id: opened.session_id.clone(),
        cursor: opened.cursor.clone(),
    });
    let session_id = opened.session_id.clone();
    state.opened = Some(opened);
    state.link_state = LinkView::Connected;
    effects.extend(commands_follow(
        state,
        CommandsInput::ForegroundSession(Some(session_id)),
    ));
    if let Some(notice) = notice.filter(|notice| !notice.is_empty()) {
        effects.push(notice_lines(&notice));
    }
    while let Some(text) = state.held.pop_front() {
        effects.extend(send_text(state, text));
    }
    effects
}

fn submit_done(state: &mut AppState, id: String, outcome: ExecuteOutcome) -> Vec<AppEffect> {
    let retried = state
        .retry
        .take_if(|(retry_id, _)| *retry_id == id)
        .map(|(_, text)| text);
    let mut effects = submit_outcome(state, id, outcome.clone());
    // `/retry` already removed the prompt, so a send that did not go through returns it.
    if let Some(text) = retried {
        match outcome {
            ExecuteOutcome::Refused { reason } => {
                effects.push(notice_lines(&rewind_text::resend_failed(&reason)));
                effects.extend(return_prompt(state, text, false));
            }
            ExecuteOutcome::OutcomeUnknown => effects.extend(return_prompt(state, text, false)),
            ExecuteOutcome::Sent | ExecuteOutcome::VoiceStopped => {}
        }
    }
    effects
}

fn submit_outcome(state: &mut AppState, id: String, outcome: ExecuteOutcome) -> Vec<AppEffect> {
    let (reason, notice) = match outcome {
        ExecuteOutcome::Sent => return Vec::new(),
        ExecuteOutcome::Refused { reason } => ("refused", failure::prompt_refused_notice(&reason)),
        ExecuteOutcome::OutcomeUnknown => ("connection dropped", failure::prompt_unknown_notice()),
        ExecuteOutcome::VoiceStopped => ("voice stop", failure::voice_stop_notice()),
    };
    let mut effects = reduce(
        state,
        Event::SubmitFailed {
            submission_id: id,
            reason: reason.to_string(),
        },
    );
    effects.push(notice_lines(&notice));
    effects
}

fn interrupt_done(
    state: &mut AppState,
    outcome: ExecuteOutcome,
    notices: Vec<String>,
) -> Vec<AppEffect> {
    state.interrupting = false;
    let mut effects: Vec<AppEffect> = notices.iter().map(|text| notice_lines(text)).collect();
    let reason = match outcome {
        ExecuteOutcome::Sent => return effects,
        ExecuteOutcome::Refused { reason } => reason,
        ExecuteOutcome::OutcomeUnknown | ExecuteOutcome::VoiceStopped => {
            "connection_dropped".to_string()
        }
    };
    effects.extend(reduce(state, Event::InterruptFailed));
    effects.push(notice_lines(&failure::interrupt_refused_notice(&reason)));
    effects
}

fn recovered(state: &mut AppState, result: Result<OpenedSession, OpenError>) -> Vec<AppEffect> {
    let fresh = match result {
        Ok(fresh) => fresh,
        Err(OpenError::NotFound) => {
            state.session_gone = true;
            return vec![notice_lines(SESSION_GONE_NOTICE)];
        }
        Err(_) => return vec![notice_lines(REFRESH_FAILED_NOTICE)],
    };
    let Some(old) = state.opened.take() else {
        return Vec::new();
    };
    let shown = state.committed_cells;
    let mut effects = Vec::new();
    let keeps_prefix = fresh.state.cells().len() >= shown
        && old.state.cells().len() >= shown
        && fresh.state.cells()[..shown] == old.state.cells()[..shown];
    if !keeps_prefix {
        let cells = fresh.state.cells();
        let total = final_prefix(cells);
        let start = total.saturating_sub(RESUME_HISTORY_CELLS);
        let mut lines = vec!["\u{2500}\u{2500} Transcript refreshed \u{2500}\u{2500}".to_string()];
        if start > 0 {
            lines.push(format!("({start} earlier messages not shown)"));
        }
        effects.push(AppEffect::CommitLines(lines));
        effects.push(AppEffect::CommitCells(start..total));
        state.committed_cells = total;
    }
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    let mut fresh = fresh;
    if fresh.title.is_none() {
        fresh.title = old.title;
    }
    state.opened = Some(fresh);
    effects.push(notice_lines(REFRESHED_NOTICE));
    effects
}

/// Commit cells that became final, and tell the prompt service when the open prompts changed.
fn commit_ready(state: &mut AppState) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let mut effects = Vec::new();
    let ready = opened.state.final_prefix_len();
    if ready > state.committed_cells {
        effects.push(AppEffect::CommitCells(state.committed_cells..ready));
    }
    state.committed_cells = state
        .committed_cells
        .max(ready)
        .min(opened.state.cells().len());
    let open: Vec<PromptKey> = opened
        .state
        .open_prompts()
        .map(|prompt| prompt.key.clone())
        .collect();
    if open != state.open_keys {
        state.open_keys.clone_from(&open);
        effects.push(AppEffect::PromptsChanged(open));
    }
    effects
}

fn final_prefix(cells: &[Cell]) -> usize {
    cells.iter().take_while(|cell| cell.is_final()).count()
}

fn cut(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// The separator title: the session's title, else the picked list row's, else the first line
/// of the first user message, else `untitled session`; cut to [`TITLE_CHARS`].
#[must_use]
pub fn session_title(info_title: Option<&str>, list_title: Option<&str>, cells: &[Cell]) -> String {
    let first_user = cells.iter().find_map(|cell| match cell {
        Cell::User(user) => user.text.lines().next().map(str::to_string),
        _ => None,
    });
    [
        info_title.map(str::to_string),
        list_title.map(str::to_string),
        first_user,
    ]
    .into_iter()
    .flatten()
    .map(|title| plain(&title))
    .find(|title| !title.trim().is_empty())
    .map_or_else(
        || "untitled session".to_string(),
        |title| cut(&title, TITLE_CHARS),
    )
}

/// The separator and marker for an opened session, and the cells to commit after them. A new
/// session with no cells gets nothing.
#[must_use]
pub fn resume_header(cells: &[Cell], title: &str) -> (Vec<String>, Range<usize>) {
    let total = final_prefix(cells);
    if total == 0 {
        return (Vec::new(), 0..0);
    }
    let start = total.saturating_sub(RESUME_HISTORY_CELLS);
    let mut lines = vec![format!(
        "\u{2500}\u{2500} Resumed: {title} \u{2500}\u{2500}"
    )];
    if start > 0 {
        lines.push(format!("({start} earlier messages not shown)"));
    }
    (lines, start..total)
}

/// One plain line for a cell, its first line of text.
fn cell_line(cell: &Cell) -> String {
    let text = match cell {
        Cell::User(user) => user.text.clone(),
        Cell::Assistant(assistant) => assistant.text.clone(),
        Cell::Reasoning(reasoning) => reasoning.text.clone(),
        Cell::Tool(tool) => tool.name.clone(),
        Cell::Notice(notice) => notice.text.clone(),
        Cell::Prompt(prompt) => prompt.command.clone().unwrap_or_default(),
    };
    plain(text.lines().next().unwrap_or_default())
}

/// The resumed history as plain lines: separator, marker, then one line per shown cell.
#[must_use]
pub fn resume_history(cells: &[Cell], title: &str) -> Vec<String> {
    let (mut lines, range) = resume_header(cells, title);
    lines.extend(cells[range].iter().map(cell_line));
    lines
}

/// The notice for a session resumed by id whose folder is another one.
#[must_use]
pub fn other_folder_notice(cwd: &str) -> String {
    format!(
        "This session works in {}, not in the current folder.",
        plain(cwd)
    )
}

/// [`other_folder_notice`] only when the session's folder is known and outside the workspace.
#[must_use]
pub fn folder_notice(session_cwd: Option<&str>, workspace: &Path) -> Option<String> {
    match (session_cwd, belongs(session_cwd, workspace)) {
        (Some(cwd), Belongs::No) => Some(other_folder_notice(cwd)),
        _ => None,
    }
}

/// The notices of `~/.talaria/config.toml` under `home`. The key map's own notices are added
/// by the screen, which owns the key map.
#[must_use]
pub fn startup_notices(home: &Path) -> Vec<String> {
    crate::config::load_user_config(home).notices
}

/// Run the composer's timer when it is due.
pub fn drive_composer(deadline: &mut impl ComposerDeadline, now: Instant) {
    if deadline.next_tick().is_some_and(|due| due <= now) {
        deadline.tick(now);
    }
}

/// The status command's last step at quit: offer the final document, then stop the command and
/// wait for it. A redaction check that would let `api_key` through is never used to run it.
pub async fn drive_status(host: &mut impl StatusHost, is_credential_name: fn(&str) -> bool) {
    if is_credential_name("api_key") {
        host.on_tick(Instant::now());
    }
    host.shutdown().await;
}

/// A session event for the reducer, with all of `params` as `raw`.
pub fn apply_inbound(
    state: &mut SessionState,
    session_id: &str,
    frame: EventFrame,
    raw: Value,
) -> Vec<Effect> {
    talaria_core::session::update(
        state,
        Event::Session {
            session_id: SessionId::new(session_id),
            frame,
            raw,
        },
    )
}

/// A new connection, for the reducer.
pub fn note_connection(state: &mut SessionState, generation: u64) -> Vec<Effect> {
    talaria_core::session::update(state, Event::ConnectionOpened { generation })
}

/// Keep a prompt typed before the session exists; the eleventh is refused with a notice.
pub fn hold_prompt(state: &mut AppState, text: &str) -> Result<(), String> {
    if state.held.len() >= MAX_HELD_PROMPTS {
        return Err(HELD_FULL_NOTICE.to_string());
    }
    state.held.push_back(text.to_string());
    Ok(())
}

/// What a submitted line is, before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
    /// `/quit` or `/exit`: detach and leave.
    Quit,
    /// A local command this client runs itself, with its action and its argument.
    Local(LocalAction, String),
    /// A local command refused locally, with the notice to show.
    Notice(String),
    /// A Hermes command, as typed. It goes to `slash.exec`.
    Command(String),
    /// A bare `/`: nothing runs.
    Nothing,
    /// Ordinary text for the model.
    NotSlash,
}

/// Sort a submitted line: local commands first (they win over Hermes commands of the same
/// name and send nothing), then Hermes commands, then prose. The grammar is
/// [`talaria_core::commands::parse_slash_line`].
#[must_use]
pub fn slash_line(text: &str) -> Slash {
    let Some(line) = parse_slash_line(text) else {
        return Slash::NotSlash;
    };
    if line.name.is_empty() {
        return Slash::Nothing;
    }
    if let Some(command) = LocalCommandTable::builtin().find(&line.name) {
        if !command.takes_argument && !line.arg.is_empty() {
            return Slash::Notice(format!("/{} does not take an argument.", line.name));
        }
        return match command.action {
            LocalAction::Quit => Slash::Quit,
            _ => Slash::Local(command.action, line.arg),
        };
    }
    Slash::Command(text.to_string())
}

/// The immediate answer this client writes for a server request of kind `name`: the
/// `-32601` refusal for a kind it does not support, or `Null` when a card holds the request
/// for the person.
pub async fn answer_server_request(name: &str, params: Value) -> Result<Value, String> {
    if !params.is_object() {
        return Err("a server request carries an object of parameters".to_string());
    }
    Ok(match policy_for(name) {
        ServerRequestPolicy::Masked(_) => Value::Null,
        ServerRequestPolicy::Refuse => json!({
            "code": -32601,
            "message": format!("talaria does not support server request: {name}"),
        }),
    })
}

/// The notice for a link event the person must see.
#[must_use]
pub fn link_notice(event: &LinkEvent) -> Option<String> {
    match event {
        LinkEvent::SessionGone { .. } => Some(SESSION_GONE_NOTICE.to_string()),
        LinkEvent::State(LinkState::Failed { reason, next_step }) => Some(format!(
            "{}\nNext step: {}",
            plain(reason),
            plain(next_step)
        )),
        _ => None,
    }
}

/// Leave the session: untrack, detach without ending it, close the link. Returns the detach
/// outcome, the exit code and the text for standard output (the resume hint, only when the
/// session has a user message). After `SessionGone` or a failed link nothing is detached and
/// the exit code is 1; the hint follows only a failed link.
pub async fn quit(state: &mut AppState) -> (DetachOutcome, u8, String) {
    state.quitting = true;
    let broken = state.session_gone || state.link_failed;
    let mut outcome = DetachOutcome::Skipped;
    if !broken
        && let (Some(opened), Some(service)) = (state.opened.as_ref(), state.service.as_ref())
    {
        if let Some(link) = &state.link {
            link.untrack(&opened.session_id);
        }
        outcome = service.detach(opened).await;
    }
    if let Some(link) = state.link.take() {
        link.close().await;
    }
    let code = u8::from(broken);
    let mut hint = String::new();
    if !state.session_gone
        && state.has_user_message()
        && let Some(opened) = &state.opened
    {
        if opened.state.turn() == &talaria_core::session::TurnState::Running {
            hint.push_str(STILL_WORKING);
            hint.push('\n');
        }
        hint.push_str(&crate::launch::exit_hint(
            &opened.stored_session_id,
            &state.profile,
        ));
    }
    (outcome, code, hint)
}

/// Quit while the create or resume is in flight: wait up to [`QUIT_WAIT`] and detach a session
/// that arrives late. Never a hint: no prompt was sent.
pub async fn quit_while_opening(state: &mut AppState) -> (DetachOutcome, u8, String) {
    state.quitting = true;
    let Some(mut pending) = state.pending.take() else {
        return (DetachOutcome::Skipped, 0, String::new());
    };
    match tokio::time::timeout(QUIT_WAIT, &mut pending).await {
        Ok(Ok(Ok(ready))) => {
            ready.connection.link.untrack(&ready.opened.session_id);
            let outcome = ready.service.detach(&ready.opened).await;
            ready.connection.link.close().await;
            (outcome, 0, String::new())
        }
        Ok(_) => (DetachOutcome::Skipped, 0, String::new()),
        Err(_) => {
            pending.abort();
            (DetachOutcome::Skipped, 0, String::new())
        }
    }
}

/// Deny open approvals first (the prompt service bounds that wait), then interrupt, even when
/// the deny failed.
async fn interrupt_sequence(
    service: SessionService,
    prompts: Option<Arc<Mutex<PromptService>>>,
    snapshot: SessionState,
    effect: Effect,
) -> (ExecuteOutcome, Vec<String>) {
    let mut notices = Vec::new();
    let open_approval = snapshot
        .open_prompts()
        .any(|prompt| prompt.kind == PromptKind::Approval && prompt.state == PromptState::Open);
    if let (true, Some(prompts), Some(client), Effect::Interrupt { session_id, .. }) =
        (open_approval, &prompts, service.client(), &effect)
    {
        let denied = prompts
            .lock()
            .await
            .deny_before_interrupt(client, &snapshot, session_id.as_str(), DENY_WAIT)
            .await;
        notices.extend(denied.into_iter().map(|notice| notice.text));
    }
    (service.execute(effect).await, notices)
}

/// Stop the running turn now, waiting for the answer. Without a session, a running turn or a
/// connection nothing is sent.
pub async fn interrupt(state: &mut AppState) -> ExecuteOutcome {
    let refused = |reason: &str| ExecuteOutcome::Refused {
        reason: reason.to_string(),
    };
    if state.interrupting {
        return refused("interrupt_in_flight");
    }
    let (Some(opened), Some(service)) = (state.opened.as_mut(), state.service.clone()) else {
        return refused("no_session");
    };
    let effect = talaria_core::session::update(&mut opened.state, Event::Interrupt)
        .into_iter()
        .find(|effect| matches!(effect, Effect::Interrupt { .. }));
    let Some(effect) = effect else {
        return refused("no_running_turn");
    };
    let snapshot = opened.state.clone();
    let (outcome, _) = interrupt_sequence(service, state.prompts.clone(), snapshot, effect).await;
    if outcome != ExecuteOutcome::Sent
        && let Some(opened) = state.opened.as_mut()
    {
        talaria_core::session::update(&mut opened.state, Event::InterruptFailed);
    }
    outcome
}

/// Refuse a `--record` path that exists or whose folder does not, before any drawing.
pub fn check_record_path(path: &Path) -> Result<(), LaunchFailure> {
    if path.symlink_metadata().is_ok() {
        return Err(failure::record_exists());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if parent.is_dir() {
        Ok(())
    } else {
        Err(failure::record_unwritable())
    }
}

/// Create the recording file (never overwriting one).
pub fn create_recorder(path: &Path) -> Result<Recorder, LaunchFailure> {
    Recorder::create(path, RecorderOptions::default()).map_err(|error| match error {
        RecordError::AlreadyExists => failure::record_exists(),
        _ => failure::record_unwritable(),
    })
}

/// Connect for the launch: discovery, profile, link.
pub async fn connect(launch: &Launch, home: &Path) -> Result<OpenConnection, LaunchFailure> {
    let recorder = launch.record.as_deref().map(create_recorder).transpose()?;
    ConnectionService::open(launch, home, recorder).await
}

/// The `--resume` list: live rows, then stored rows when that call works, merged.
pub async fn list_sessions(
    connection: &OpenConnection,
    launch_id: &str,
) -> Result<Vec<ListRow>, LaunchFailure> {
    let service = SessionService::with_client(launch_id, connection.link.client());
    let live = service.list("live").await.map_err(LaunchFailure::from)?;
    let stored = service.list("stored").await.unwrap_or_default();
    Ok(merge_rows(live, stored))
}

fn open_failure(error: OpenError, launch: &Launch) -> LaunchFailure {
    match (error, &launch.mode) {
        (OpenError::NotFound | OpenError::NotInFolder { .. }, StartMode::Continue) => {
            failure::nothing_to_continue()
        }
        (OpenError::NotFound, StartMode::Resume(Some(id))) => {
            failure::resume_not_found(id.trim(), &launch.profile)
        }
        (OpenError::NotFound, _) => failure::other_refusal("not_found"),
        (OpenError::NotInFolder { .. }, _) => failure::nothing_to_continue(),
        (OpenError::FolderUnknown, _) => failure::folder_unknown(),
        (OpenError::Refused { reason }, _) => failure::refusal(&reason),
        (OpenError::Unknown, StartMode::New) => failure::create_lost_twice(),
        (OpenError::Unknown, _) => {
            failure::link_failure("the connection dropped before Hermes answered")
        }
        (OpenError::Link(what), _) => failure::link_failure(&what),
    }
}

async fn open_session(
    launch: &Launch,
    connection: &OpenConnection,
    git: Option<JoinHandle<Option<PathBuf>>>,
    launch_id: &str,
) -> Result<(SessionService, OpenedSession, Option<String>), LaunchFailure> {
    let request =
        plan(launch, new_request_id(), &connection.describe).map_err(LaunchFailure::from)?;
    let service = SessionService::with_client(launch_id, connection.link.client())
        .with_create_defaults(
            &connection.profile_name,
            &connection.describe.session_create.parameters,
        );
    let git_toplevel = match git {
        Some(handle) => handle.await.ok().flatten(),
        None => None,
    };
    let workspace = workspace_key(&launch.folder, git_toplevel.as_deref());
    let (request, by_id) = match request {
        SessionRequest::Latest { .. } => (
            SessionRequest::Latest {
                workspace: workspace.clone(),
            },
            false,
        ),
        request @ SessionRequest::ById { .. } => (request, true),
        request => (request, false),
    };
    let opened = service
        .open(request)
        .await
        .map_err(|error| open_failure(error, launch))?;
    let notice = if by_id {
        folder_notice(opened.info.cwd.as_deref(), &workspace)
    } else {
        None
    };
    Ok((service, opened, notice))
}

/// Background start-up: connect (unless the `--resume` list already did), plan, open. `git`
/// runs at the same time as the connection for continue and resume.
pub async fn connect_and_open(
    launch: Launch,
    home: PathBuf,
    launch_id: String,
    preopened: Option<OpenConnection>,
) -> Result<Ready, LaunchFailure> {
    let git = matches!(
        launch.mode,
        StartMode::Continue | StartMode::Resume(Some(_))
    )
    .then(|| {
        let folder = launch.folder.clone();
        tokio::task::spawn_blocking(move || find_git_toplevel(&folder))
    });
    let connection = match preopened {
        Some(connection) => connection,
        None => connect(&launch, &home).await?,
    };
    match open_session(&launch, &connection, git, &launch_id).await {
        Ok((service, opened, notice)) => Ok(Ready {
            connection,
            service,
            opened,
            notice,
        }),
        Err(failure) => {
            connection.link.close().await;
            Err(failure)
        }
    }
}

/// What the process prints after the terminal is restored, and its exit code.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

/// What `run` needs beyond the launch.
pub struct RunSetup {
    /// The Hermes home.
    pub home: PathBuf,
    pub launch_id: String,
    /// The connection the `--resume` list opened, reused here.
    pub preopened: Option<OpenConnection>,
    pub list_title: Option<String>,
    /// Configuration and key map notices, committed with the first frame.
    pub notices: Vec<String>,
}

fn held_text(held: &VecDeque<String>) -> String {
    if held.is_empty() {
        return String::new();
    }
    let mut text = if held.len() == 1 {
        "1 typed prompt was never sent:\n".to_string()
    } else {
        format!("{} typed prompts were never sent:\n", held.len())
    };
    for prompt in held {
        text.push_str("  ");
        text.push_str(&cut(&plain(prompt), HELD_PRINT_CHARS));
        text.push('\n');
    }
    text
}

/// The executor side of the loop: everything it needs to start a call.
struct Executor {
    sender: mpsc::UnboundedSender<AppMessage>,
    /// Prompts go through one worker, one call at a time, so they reach the gateway in the
    /// order they were submitted.
    prompts: mpsc::UnboundedSender<(SessionService, Effect)>,
    model_files: mpsc::UnboundedSender<ModelFileJob>,
}

enum ModelFileJob {
    Save(Arc<crate::models::ModelService>, SavedModels),
    Flush(tokio::sync::oneshot::Sender<bool>),
}

impl Executor {
    fn new(sender: mpsc::UnboundedSender<AppMessage>) -> Self {
        let (prompts, mut queue) = mpsc::unbounded_channel::<(SessionService, Effect)>();
        let results = sender.clone();
        tokio::spawn(async move {
            while let Some((service, effect)) = queue.recv().await {
                let Effect::SendPrompt { submission_id, .. } = &effect else {
                    continue;
                };
                let submission_id = submission_id.clone();
                let outcome = service.execute(effect).await;
                if results
                    .send(AppMessage::SubmitDone {
                        submission_id,
                        outcome,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let (model_files, mut files) = mpsc::unbounded_channel();
        let results = sender.clone();
        tokio::spawn(async move {
            let mut failed = false;
            while let Some(job) = files.recv().await {
                match job {
                    ModelFileJob::Save(service, saved) => {
                        let inputs = service.execute(ModelsEffect::SaveFile(saved)).await;
                        failed = inputs
                            .iter()
                            .any(|input| matches!(input, ModelsInput::FileFailed { .. }));
                        for input in inputs {
                            let _ = results.send(AppMessage::Models(input));
                        }
                    }
                    ModelFileJob::Flush(reply) => {
                        let _ = reply.send(failed);
                    }
                }
            }
        });
        Self {
            sender,
            prompts,
            model_files,
        }
    }

    async fn flush_model_files(&self) -> bool {
        let (reply, waiting) = tokio::sync::oneshot::channel();
        if self.model_files.send(ModelFileJob::Flush(reply)).is_err() {
            return true;
        }
        !matches!(
            tokio::time::timeout(QUIT_WAIT, waiting).await,
            Ok(Ok(false))
        )
    }

    fn send_later(&self, task: impl Future<Output = AppMessage> + Send + 'static) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.send(task.await);
        });
    }

    fn prompt_task<F, Fut>(&self, prompts: &Arc<Mutex<PromptService>>, task: F)
    where
        F: FnOnce(Arc<Mutex<PromptService>>) -> Fut + Send + 'static,
        Fut: Future<Output = (Vec<Event>, Vec<String>)> + Send + 'static,
    {
        let prompts = Arc::clone(prompts);
        self.send_later(async move {
            let (events, notices) = task(Arc::clone(&prompts)).await;
            let masked = Some(prompts.lock().await.masked_requests());
            AppMessage::PromptDone {
                events,
                notices,
                masked,
            }
        });
    }
}

fn notice_texts(notices: Vec<crate::prompts::PromptNotice>) -> Vec<String> {
    notices.into_iter().map(|notice| notice.text).collect()
}

/// Apply one message and run its effects. Returns whether the screen needs a draw.
fn apply<S: Screen>(
    state: &mut AppState,
    message: AppMessage,
    screen: &mut S,
    executor: &Executor,
) -> bool {
    let mut redraw = false;
    for effect in update(state, message) {
        match effect {
            AppEffect::Rewind { ticket, call } => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned();
                let session_id = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone());
                executor.send_later(async move {
                    let input = match (client, session_id) {
                        (Some(client), Some(session_id)) => {
                            crate::rewind::RewindService::new(client, session_id)
                                .execute(ticket, call)
                                .await
                        }
                        _ => RewindInput::Reply {
                            ticket,
                            reply: Reply::Failed(CallFailure::NotSent(
                                "no session is open".to_string(),
                            )),
                        },
                    };
                    AppMessage::Rewind(input)
                });
            }
            AppEffect::Models(effect) => match effect {
                ModelsEffect::OpenOverlay | ModelsEffect::CloseOverlay => redraw = true,
                ModelsEffect::Notice { level, text, .. } => {
                    if text.starts_with("~/.talaria/models.json could not be read (")
                        && !state.model_warnings.insert(text.clone())
                    {
                        continue;
                    }
                    state.model_notice_counter = state.model_notice_counter.saturating_add(1);
                    screen.commit_cells(&[Cell::Notice(talaria_core::session::NoticeCell {
                        id: talaria_core::session::CellId::new(
                            u64::MAX - state.model_notice_counter,
                        ),
                        kind: match level {
                            NoticeLevel::Info => talaria_core::session::NoticeKind::Diagnostic,
                            NoticeLevel::Warning => talaria_core::session::NoticeKind::Warning,
                            NoticeLevel::Error => talaria_core::session::NoticeKind::Error,
                        },
                        text,
                        detail: None,
                    })]);
                }
                ModelsEffect::SaveFile(saved) => {
                    if let Some(service) = state.model_service.clone() {
                        let _ = executor
                            .model_files
                            .send(ModelFileJob::Save(service, saved));
                    }
                }
                effect => {
                    if let Some(service) = state.model_service.clone() {
                        let sender = executor.sender.clone();
                        tokio::spawn(async move {
                            for input in service.execute(effect).await {
                                let _ = sender.send(AppMessage::Models(input));
                            }
                        });
                    }
                }
            },
            AppEffect::CommitLines(lines) => screen.commit_lines(lines),
            AppEffect::CommitCells(range) => {
                if let Some(opened) = &state.opened {
                    let cells = opened.state.cells();
                    let end = range.end.min(cells.len());
                    screen.commit_cells(&cells[range.start.min(end)..end]);
                }
            }
            AppEffect::Redraw => redraw = true,
            AppEffect::LoadDraft(text) => screen.load_draft(&text),
            AppEffect::Send(effect) => {
                let Some(service) = state.service.clone() else {
                    continue;
                };
                let _ = executor.prompts.send((service, effect));
            }
            AppEffect::Interrupt(effect) => {
                let (Some(service), Some(opened)) = (state.service.clone(), &state.opened) else {
                    state.interrupting = false;
                    continue;
                };
                let task = interrupt_sequence(
                    service,
                    state.prompts.clone(),
                    opened.state.clone(),
                    effect,
                );
                executor.send_later(async move {
                    let (outcome, notices) = task.await;
                    AppMessage::InterruptDone { outcome, notices }
                });
            }
            AppEffect::Respond { effect, note } => {
                let (Some(prompts), Some(client)) = (
                    &state.prompts,
                    state
                        .service
                        .as_ref()
                        .and_then(SessionService::client)
                        .cloned(),
                ) else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let mut prompts = prompts.lock().await;
                    if let Some((key, note)) = note {
                        prompts.set_note(&key, note);
                    }
                    let executed = prompts.execute(&client, effect).await;
                    (executed.events, notice_texts(executed.notices))
                });
            }
            AppEffect::AnswerMasked { request_id, value } => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let result = prompts.lock().await.answer_masked(&request_id, value).await;
                    let notices = match result {
                        Ok(()) => Vec::new(),
                        Err(_) => vec![
                            "The answer could not be sent; Hermes may have moved on.".to_string(),
                        ],
                    };
                    (Vec::new(), notices)
                });
            }
            AppEffect::ServerRequest(request) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_server_request(request).await;
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::ConnectionOpened(generation) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_connection_replaced(generation);
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::PromptsChanged(open) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_prompts_changed(&open);
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::Track { session_id, cursor } => {
                if let Some(link) = &state.link {
                    link.track(&session_id, cursor);
                }
            }
            AppEffect::Resync { session_id, cursor } => {
                if let Some(link) = &state.link {
                    link.resync(&session_id, cursor);
                }
            }
            AppEffect::Recover(gap) => {
                let Some(service) = state.service.clone() else {
                    continue;
                };
                executor
                    .send_later(async move { AppMessage::Recovered(service.recover(gap).await) });
            }
            AppEffect::Quit => state.quitting = true,
            AppEffect::RunCommand { session_id, typed } => {
                let Some(commands) = &state.command_service else {
                    continue;
                };
                let mut runner = commands.runner(state.commands.catalog.clone());
                executor.send_later(async move {
                    let outcome = runner.dispatch(&session_id, &typed).await;
                    AppMessage::CommandDone { typed, outcome }
                });
            }
            AppEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            } => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.start_fetch(request_id, session_id, after);
                }
            }
            AppEffect::RequestCompletion {
                request_id,
                text,
                after,
                session_id,
            } => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.request_completion(request_id, text, after, session_id);
                }
            }
            AppEffect::CancelCompletion => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.cancel_completion();
                }
            }
            // #228: the build reports this to Herdr; never produced yet.
            AppEffect::ReportPane(_) => {}
        }
    }
    redraw
}

/// Take the opened session's handles into the state and start forwarding link events.
fn install(state: &mut AppState, ready: Ready, executor: &Executor) -> AppMessage {
    let Ready {
        connection,
        service,
        opened,
        notice,
    } = ready;
    let OpenConnection {
        link,
        mut inbound,
        profile_name,
        ..
    } = connection;
    state.profile = profile_name;
    let mut commands = CommandService::new(link.client());
    if let Some(mut replies) = commands.take_replies() {
        let sender = executor.sender.clone();
        tokio::spawn(async move {
            while let Some(reply) = replies.recv().await {
                if sender.send(AppMessage::Commands(reply)).is_err() {
                    break;
                }
            }
        });
    }
    state.command_service = Some(commands);
    state.link = Some(link);
    state.service = Some(service);
    state.model_service = Some(Arc::new(crate::models::ModelService::new(
        state
            .service
            .as_ref()
            .and_then(SessionService::client)
            .expect("an installed connection has a client")
            .clone(),
        crate::config::models_path().unwrap_or_default(),
    )));
    let counter = Arc::clone(&state.counter);
    let launch_id = state.launch_id.clone();
    state.prompts = Some(Arc::new(Mutex::new(PromptService::new(Box::new(
        move || submission_id(&launch_id, counter.fetch_add(1, Ordering::SeqCst) + 1),
    )))));
    let sender = executor.sender.clone();
    tokio::spawn(async move {
        while let Some(event) = inbound.recv().await {
            if sender.send(AppMessage::Link(event)).is_err() {
                break;
            }
        }
    });
    AppMessage::Opened { opened, notice }
}

fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// What ended the loop.
enum Ending {
    Quit,
    Signal(u8),
    Failed(LaunchFailure),
    InputClosed,
}

/// Run one launch: start-up in the background, the loop, and the quit. The first frame is
/// drawn before anything touches the network. The screen is restored before this returns;
/// the caller prints the outcome.
pub async fn run<S: Screen>(
    launch: Launch,
    mut setup: RunSetup,
    screen: &mut S,
    mut inputs: mpsc::UnboundedReceiver<S::Input>,
) -> Outcome {
    let mut state = AppState::new(setup.launch_id.clone());
    state.models = ModelsState::new(&setup.launch_id, crate::config::load_models());
    state
        .models
        .set_display_zone(crate::config::model_display_zone());
    if let Some(warning) = state.models.take_saved_warning() {
        state.model_warnings.insert(warning.clone());
        setup.notices.push(warning);
    }
    state.profile = launch.profile.trim().to_lowercase();
    state.list_title = setup.list_title;
    state.model = launch.model.clone();
    state.effort = launch.effort.clone();
    sync_models(&mut state, screen);
    screen.draw(&state.view());
    if !setup.notices.is_empty() {
        screen.commit_lines(
            setup
                .notices
                .iter()
                .map(|notice| format!("\u{b7} {notice}"))
                .collect(),
        );
        screen.draw(&state.view());
    }

    let (sender, mut messages) = mpsc::unbounded_channel();
    let executor = Executor::new(sender);
    let mut pending = Some(tokio::spawn(connect_and_open(
        launch,
        setup.home,
        setup.launch_id,
        setup.preopened,
    )));
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).ok();

    let ending = loop {
        let now = Instant::now();
        let deadline = earliest(screen.next_tick(), screen.next_wake(now));
        let mut redraw = false;
        let mut woke_on_message = false;
        tokio::select! {
            input = inputs.recv() => {
                let Some(input) = input else { break Ending::InputClosed };
                sync_models(&mut state, screen);
                let actions = screen.input(input, &state.view(), Instant::now());
                for action in actions {
                    redraw |= apply(&mut state, AppMessage::ForegroundKey(action), screen, &executor);
                }
            }
            Some(message) = messages.recv() => {
                redraw |= apply(&mut state, message, screen, &executor);
                woke_on_message = true;
            }
            joined = async { pending.as_mut().expect("guarded by the branch condition").await },
                if pending.is_some() => {
                pending = None;
                match joined {
                    Ok(Ok(ready)) => {
                        let message = install(&mut state, ready, &executor);
                        redraw |= apply(&mut state, message, screen, &executor);
                    }
                    Ok(Err(failure)) => break Ending::Failed(failure),
                    Err(_) => break Ending::Failed(failure::link_failure("the start-up task stopped")),
                }
            }
            () = async {
                match deadline {
                    Some(due) => tokio::time::sleep_until(due.into()).await,
                    None => std::future::pending().await,
                }
            } => {
                let now = Instant::now();
                drive_composer(screen, now);
                screen.on_tick(now);
                redraw = true;
            }
            Some(()) = async { match terminate.as_mut() { Some(stream) => stream.recv().await, None => std::future::pending().await } } => {
                break Ending::Signal(143);
            }
            Some(()) = async { match hangup.as_mut() { Some(stream) => stream.recv().await, None => std::future::pending().await } } => {
                break Ending::Signal(129);
            }
        }
        // A fast stream queues deltas faster than a frame is drawn. Keys that are already
        // waiting go first, so Esc is never held behind the stream, and ready messages are
        // applied together and drawn once instead of one frame each.
        let mut drained = 0;
        loop {
            while !state.quitting {
                let Ok(input) = inputs.try_recv() else { break };
                sync_models(&mut state, screen);
                let actions = screen.input(input, &state.view(), Instant::now());
                for action in actions {
                    redraw |= apply(
                        &mut state,
                        AppMessage::ForegroundKey(action),
                        screen,
                        &executor,
                    );
                }
            }
            if !woke_on_message || state.quitting || drained == MESSAGE_BATCH {
                break;
            }
            let Ok(message) = messages.try_recv() else {
                break;
            };
            redraw |= apply(&mut state, message, screen, &executor);
            drained += 1;
        }
        if state.quitting {
            break Ending::Quit;
        }
        if redraw {
            sync_models(&mut state, screen);
            screen.draw(&state.view());
            screen.on_tick(Instant::now());
        }
    };

    let mut outcome = Outcome::default();
    match ending {
        Ending::Quit | Ending::InputClosed => {
            let (detached, code) = if pending.is_some() {
                state.pending = pending.take();
                let (detached, code, _) = quit_while_opening(&mut state).await;
                (detached, code)
            } else {
                let (detached, code, hint) = quit(&mut state).await;
                outcome.stdout = hint;
                (detached, code)
            };
            outcome.code = code;
            if detached == DetachOutcome::StaleSubscription {
                outcome.stderr.push_str(STALE_SUBSCRIPTION);
                outcome.stderr.push('\n');
            }
        }
        Ending::Signal(code) => {
            if let Some(handle) = pending.take() {
                handle.abort();
            }
            if let Some(link) = state.link.take() {
                link.close().await;
            }
            outcome.code = code;
        }
        Ending::Failed(failure) => {
            outcome.code = failure.exit_code;
            outcome.stderr.push_str(&failure.render());
        }
    }
    if executor.flush_model_files().await {
        outcome
            .stderr
            .push_str("Some model choices could not be saved before exit.\n");
    }
    drive_status(screen, hermes_client::is_suspicious_key).await;
    screen.restore();
    outcome.stderr.push_str(&held_text(&state.held));
    outcome
}
