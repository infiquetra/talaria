//! One headless run as a pure state machine.

use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

use crate::failure::LaunchFailure;
use crate::headless::output::{
    ErrorText, HeadlessResult, RunStatus, StopPoint, Usage, effective_outcome, judge,
    neutralise_line, usage_from_result,
};

/// What asked the run to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    Timeout,
    Signal(i32),
    Record,
    OutputClosed,
}

/// How the submit call answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitReply {
    Admitted {
        admission_id: String,
        status: String,
        execution_generation: Option<i64>,
    },
    VoiceStopped,
    Refused {
        reason: Option<String>,
    },
    Unknown,
}

/// How a finished call is handed to the machine.
#[derive(Debug, Clone, PartialEq)]
pub enum CallReply {
    Ok(Value),
    Refused { reason: Option<String> },
    Failed,
}

/// Every event the machine can take.
#[derive(Debug, Clone, PartialEq)]
pub enum HeadlessInput {
    Opened {
        session_id: String,
        stored_session_id: String,
        profile: String,
        model: Option<String>,
        provider: Option<String>,
        open_prompts: Vec<OpenPrompt>,
        unknown_admissions: usize,
    },
    OpenFailed(LaunchFailure),
    SubmitReplied(SubmitReply),
    Event {
        admission_id: Option<String>,
        execution_generation: Option<i64>,
        event_type: String,
        payload: Value,
    },
    ServerRequest {
        request_key: u64,
        kind: String,
    },
    ReceiptRead(CallReply),
    CancelReplied(CallReply),
    InterruptReplied(CallReply),
    Gap,
    Recovered {
        own_pending_status: Option<String>,
        own_pending_generation: Option<i64>,
        open_prompts: Vec<OpenPrompt>,
    },
    LinkFailed {
        reason: String,
        next_step: String,
    },
    SessionGone,
    Stop(StopCause),
    SettleDeadline,
    StopBudgetExpired,
}

/// One open approval or question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPrompt {
    pub prompt_id: String,
    pub kind: String,
    pub admission_id: Option<String>,
    pub execution_generation: Option<i64>,
}

/// What the service must do next.
#[derive(Debug, Clone, PartialEq)]
pub enum HeadlessEffect {
    Submit {
        text: String,
    },
    ReadReceipt,
    Cancel,
    Interrupt {
        execution_generation: i64,
    },
    DenyApproval {
        execution_generation: i64,
        prompt_id: String,
    },
    SkipClarify {
        execution_generation: i64,
        prompt_id: String,
    },
    RefuseServerRequest {
        request_key: u64,
    },
    Recover,
    Observed {
        event_type: String,
        payload: Value,
    },
    StartSettleTimer,
    StartStopBudget,
    Detach,
    Finish(HeadlessResult),
    ExitNow(HeadlessResult),
}

/// The machine. Owns the phase and everything learned so far.
#[derive(Debug)]
pub struct HeadlessRun {
    text: String,
    phase: Phase,
    result: HeadlessResult,
    opened: bool,
    admission: Option<String>,
    generation: Option<i64>,
    started: bool,
    completion: Option<Value>,
    held: Vec<HeadlessInput>,
    requests: Vec<(u64, String)>,
    prompts: BTreeMap<String, OpenPrompt>,
    answered: HashSet<(String, i64)>,
    gap_pending: bool,
    reading: ReadKind,
    cancel_sent: bool,
    interrupted: bool,
    stop: Option<StopCause>,
    stop_applies: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Starting,
    Submitting,
    Waiting,
    Reading,
    Stopping,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadKind {
    Completion,
    Gap,
    Settle,
    Generation,
}

const USAGE_WARNING: &str = "Hermes did not return the turn's usage figures.";
const STORED_WARNING: &str =
    "The answer was read from Hermes's stored copy, in which secrets are redacted.";
const BUDGET_WARNING: &str = "Talaria stopped waiting for Hermes; the turn may still be running.";

fn piece(text: &str) -> String {
    neutralise_line(text).chars().take(200).collect()
}

/// The result when a deadline or signal wins before opening a session.
pub fn stopped_before_submit(cause: StopCause) -> HeadlessResult {
    let mut run = HeadlessRun::new(String::new());
    run.stop = Some(cause);
    run.stop_result(StopPoint::BeforeSubmit)
}

/// A launch failure before any session has opened.
pub fn failed_before_open(failure: LaunchFailure) -> HeadlessResult {
    let mut run = HeadlessRun::new(String::new());
    run.result.status = RunStatus::NotStarted;
    run.result.exit_code = 4;
    run.result.error = Some(ErrorText {
        what: failure.what,
        next_step: failure.next_step,
    });
    run.result
}

impl HeadlessRun {
    pub fn new(text: String) -> Self {
        Self {
            text,
            phase: Phase::Starting,
            result: HeadlessResult {
                status: RunStatus::Unknown,
                exit_code: 1,
                text: String::new(),
                session_id: None,
                profile: None,
                model: None,
                provider: None,
                admission_id: None,
                duration_ms: 0,
                outcome: None,
                turn_exit_reason: None,
                usage: Usage::default(),
                warnings: Vec::new(),
                error: None,
                resume_hint: None,
            },
            opened: false,
            admission: None,
            generation: None,
            started: false,
            completion: None,
            held: Vec::new(),
            requests: Vec::new(),
            prompts: BTreeMap::new(),
            answered: HashSet::new(),
            gap_pending: false,
            reading: ReadKind::Completion,
            cancel_sent: false,
            interrupted: false,
            stop: None,
            stop_applies: false,
        }
    }

    pub(crate) fn admission_id(&self) -> Option<&str> {
        self.admission.as_deref()
    }

    pub fn step(&mut self, input: HeadlessInput) -> Vec<HeadlessEffect> {
        if self.phase == Phase::Done {
            return match input {
                HeadlessInput::Stop(cause) if self.stop.is_some() => self.stop(cause),
                _ => Vec::new(),
            };
        }
        match input {
            HeadlessInput::Opened {
                session_id: _,
                stored_session_id,
                profile,
                model,
                provider,
                open_prompts,
                unknown_admissions,
            } => {
                self.opened = true;
                self.result.session_id = Some(stored_session_id);
                self.result.profile = Some(profile);
                self.result.model = model;
                self.result.provider = provider;
                if self.stop.is_some() {
                    return self.finish_stop(StopPoint::BeforeSubmit, true);
                }
                if !open_prompts.is_empty() {
                    return self.fail(
                        RunStatus::InputRequired,
                        3,
                        "this session is waiting for an answer to a question or approval.".into(),
                        format!(
                            "Answer it in talaria --resume {}, then run the command again.",
                            self.id()
                        ),
                        false,
                        true,
                    );
                }
                if unknown_admissions > 0 {
                    return self.fail(RunStatus::InputRequired, 3,
                        "Hermes holds an earlier turn of this session whose outcome is unknown, so a new prompt would never run.".into(),
                        format!("Resolve it with hermes sessions discard {} --yes, or use another session.", self.id()), false, true);
                }
                self.phase = Phase::Submitting;
                vec![HeadlessEffect::Submit {
                    text: self.text.clone(),
                }]
            }
            HeadlessInput::OpenFailed(failure) => {
                if self.stop.is_some() {
                    self.finish_stop(StopPoint::BeforeSubmit, false)
                } else {
                    self.fail(
                        RunStatus::NotStarted,
                        4,
                        failure.what,
                        failure.next_step,
                        false,
                        false,
                    )
                }
            }
            HeadlessInput::SubmitReplied(reply) => self.submitted(reply),
            HeadlessInput::Event {
                admission_id,
                execution_generation,
                event_type,
                payload,
            } => self.event(admission_id, execution_generation, event_type, payload),
            HeadlessInput::ServerRequest { request_key, kind } => {
                if self.phase == Phase::Submitting {
                    self.requests.push((request_key, kind));
                    Vec::new()
                } else {
                    self.server_request(request_key, kind)
                }
            }
            HeadlessInput::ReceiptRead(reply) => self.receipt(reply),
            HeadlessInput::CancelReplied(reply) => self.cancelled(reply),
            HeadlessInput::InterruptReplied(_) => Vec::new(),
            HeadlessInput::Gap => {
                if self.phase == Phase::Submitting {
                    self.gap_pending = true;
                    Vec::new()
                } else if self.phase == Phase::Reading {
                    Vec::new()
                } else {
                    vec![HeadlessEffect::Recover]
                }
            }
            HeadlessInput::Recovered {
                own_pending_status,
                own_pending_generation,
                open_prompts,
            } => {
                if own_pending_status.as_deref() == Some("started") {
                    self.started = true;
                }
                if self.generation.is_none() {
                    self.generation = own_pending_generation;
                }
                self.prompts = open_prompts
                    .into_iter()
                    .map(|p| (p.prompt_id.clone(), p))
                    .collect();
                let mut effects = self.answer_prompts();
                match own_pending_status.as_deref() {
                    Some("unknown") => effects.extend(self.lost_end()),
                    None => {
                        self.reading = ReadKind::Gap;
                        effects.push(HeadlessEffect::ReadReceipt);
                    }
                    _ => {}
                }
                if self.stop.is_some() && self.phase != Phase::Done && own_pending_status.is_some()
                {
                    effects.extend(self.stop_waiting());
                }
                effects
            }
            HeadlessInput::LinkFailed { reason, next_step } => {
                self.link_failed(Some((reason, next_step)))
            }
            HeadlessInput::SessionGone => self.link_failed(None),
            HeadlessInput::Stop(cause) => self.stop(cause),
            HeadlessInput::SettleDeadline => {
                if self.stop.is_some() && self.completion.is_none() {
                    self.reading = ReadKind::Settle;
                    vec![HeadlessEffect::ReadReceipt]
                } else {
                    Vec::new()
                }
            }
            HeadlessInput::StopBudgetExpired => {
                self.warn(BUDGET_WARNING.into());
                if self.completion.is_some() {
                    self.settled(None, None, false)
                } else {
                    self.finish_stop(self.stop_point(), false)
                }
            }
        }
    }

    fn id(&self) -> &str {
        self.result.session_id.as_deref().unwrap_or("")
    }

    fn warn(&mut self, warning: String) {
        if !self.result.warnings.contains(&warning) {
            self.result.warnings.push(warning);
        }
    }

    fn fail(
        &mut self,
        status: RunStatus,
        code: u8,
        what: String,
        next_step: String,
        resume: bool,
        detach: bool,
    ) -> Vec<HeadlessEffect> {
        self.result.status = status;
        self.result.exit_code = code;
        self.result.error = Some(ErrorText { what, next_step });
        self.result.resume_hint = resume.then(|| {
            crate::launch::exit_hint(
                self.id(),
                self.result.profile.as_deref().unwrap_or("default"),
            )
        });
        self.finish(detach)
    }

    fn finish(&mut self, detach: bool) -> Vec<HeadlessEffect> {
        self.phase = Phase::Done;
        let mut effects = Vec::new();
        if detach && self.opened {
            effects.push(HeadlessEffect::Detach);
        }
        effects.push(HeadlessEffect::Finish(self.result.clone()));
        effects
    }

    fn unknown(&mut self, what: &str, detach: bool) -> Vec<HeadlessEffect> {
        self.fail(
            RunStatus::Unknown,
            1,
            what.into(),
            "Check the session before sending it again.".into(),
            self.opened,
            detach,
        )
    }

    fn lost_end(&mut self) -> Vec<HeadlessEffect> {
        self.unknown(
            "Hermes lost the end of this turn; its outcome is unknown.",
            true,
        )
    }

    fn submitted(&mut self, reply: SubmitReply) -> Vec<HeadlessEffect> {
        if self.phase != Phase::Submitting {
            return Vec::new();
        }
        match reply {
            SubmitReply::Admitted {
                admission_id,
                status,
                execution_generation,
            } => {
                self.result.admission_id = Some(admission_id.clone());
                self.admission = Some(admission_id);
                self.started |= status == "started";
                if self.generation.is_none() {
                    self.generation = execution_generation;
                }
                if status == "unknown" {
                    return if self.stop.is_some() {
                        self.finish_stop(StopPoint::WhileSending, false)
                    } else {
                        self.unknown("Hermes could not tell whether the prompt ran.", false)
                    };
                }
                self.phase = Phase::Waiting;
                let mut effects = Vec::new();
                for event in std::mem::take(&mut self.held) {
                    effects.extend(self.step(event));
                }
                if self.gap_pending && self.completion.is_none() {
                    effects.push(HeadlessEffect::Recover);
                }
                effects.extend(self.answer_prompts());
                for (key, kind) in std::mem::take(&mut self.requests) {
                    effects.extend(self.server_request(key, kind));
                }
                if self.completion.is_none() && status == "terminal" {
                    self.reading = ReadKind::Gap;
                    effects.push(HeadlessEffect::ReadReceipt);
                } else if self.completion.is_none() && self.stop.is_some() {
                    effects.extend(self.stop_waiting());
                }
                effects
            }
            SubmitReply::Unknown => {
                if self.stop.is_some() {
                    self.finish_stop(StopPoint::WhileSending, false)
                } else {
                    self.unknown("the connection dropped while the prompt was being sent; it may be running.", false)
                }
            }
            SubmitReply::Refused { reason } => {
                if self.stop.is_some() {
                    return self.finish_stop(StopPoint::BeforeSubmit, true);
                }
                self.fail(
                    RunStatus::NotStarted,
                    4,
                    format!(
                        "Hermes refused the prompt ({}).",
                        piece(reason.as_deref().unwrap_or("no reason given"))
                    ),
                    "Run the command again; if it keeps failing, report the reason.".into(),
                    false,
                    true,
                )
            }
            SubmitReply::VoiceStopped => {
                if self.stop.is_some() {
                    return self.finish_stop(StopPoint::BeforeSubmit, true);
                }
                self.fail(RunStatus::NotStarted, 4,
                    "Hermes took the prompt as the stop phrase of a voice chat on this session, so nothing ran.".into(),
                    "Send a different text, or use another session.".into(), false, true)
            }
        }
    }

    fn event(
        &mut self,
        admission: Option<String>,
        generation: Option<i64>,
        kind: String,
        payload: Value,
    ) -> Vec<HeadlessEffect> {
        if matches!(
            kind.as_str(),
            "approval.request" | "clarify.request" | "approval.settled" | "clarify.settled"
        ) {
            let Some(id) = payload.get("prompt_id").and_then(Value::as_str) else {
                return Vec::new();
            };
            if kind.ends_with(".settled") {
                self.prompts.remove(id);
                return Vec::new();
            }
            self.prompts.insert(
                id.into(),
                OpenPrompt {
                    prompt_id: id.into(),
                    kind: kind.split('.').next().unwrap_or("").into(),
                    admission_id: admission,
                    execution_generation: payload
                        .get("execution_generation")
                        .and_then(Value::as_i64)
                        .or(generation),
                },
            );
            return self.answer_prompts();
        }
        if self.phase == Phase::Submitting {
            if admission.is_some() && !self.gap_pending {
                if self.held.len() == MAX_HELD_EVENTS {
                    self.held.clear();
                    self.gap_pending = true;
                } else {
                    self.held.push(HeadlessInput::Event {
                        admission_id: admission,
                        execution_generation: generation,
                        event_type: kind,
                        payload,
                    });
                }
            }
            return Vec::new();
        }
        if admission.is_none() || admission != self.admission {
            return Vec::new();
        }
        match kind.as_str() {
            "message.delta" | "tool.start" | "tool.complete" => vec![HeadlessEffect::Observed {
                event_type: kind,
                payload,
            }],
            "message.start" => {
                self.started = true;
                if self.generation.is_none() {
                    self.generation = generation;
                }
                let mut effects = self.answer_prompts();
                if self.stop.is_some() && !self.interrupted {
                    effects.extend(self.stop_waiting());
                }
                effects
            }
            "message.complete" if self.completion.is_none() => {
                if !self.started
                    && self.cancel_sent
                    && payload.get("outcome").and_then(Value::as_str) == Some("cancelled")
                {
                    self.result.outcome = Some("cancelled".into());
                    return if self.stop.is_some() {
                        self.finish_stop(StopPoint::Withdrawn, true)
                    } else {
                        self.withdrawn()
                    };
                }
                self.result.text = payload
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| payload.get("content").and_then(Value::as_str))
                    .unwrap_or("")
                    .into();
                if let Some(warning) = payload.get("warning").and_then(Value::as_str) {
                    self.warn(piece(warning));
                }
                self.completion = Some(payload);
                self.result.outcome = self.completion.as_ref().and_then(|p| {
                    effective_outcome(
                        p.get("outcome").and_then(Value::as_str),
                        None,
                        p.get("status").and_then(Value::as_str),
                    )
                });
                self.phase = Phase::Reading;
                self.reading = ReadKind::Completion;
                vec![HeadlessEffect::ReadReceipt]
            }
            _ => Vec::new(),
        }
    }

    fn answer_prompts(&mut self) -> Vec<HeadlessEffect> {
        if self.admission.is_none() || self.phase == Phase::Done {
            return Vec::new();
        }
        let mut effects = Vec::new();
        let mut foreign = false;
        for prompt in self.prompts.values().cloned().collect::<Vec<_>>() {
            let own = prompt
                .admission_id
                .as_ref()
                .is_some_and(|id| Some(id) == self.admission.as_ref())
                || prompt
                    .execution_generation
                    .is_some_and(|g| Some(g) == self.generation);
            if !own {
                foreign = true;
                continue;
            }
            let Some(g) = self.generation.or(prompt.execution_generation) else {
                continue;
            };
            self.prompts.remove(&prompt.prompt_id);
            if !self.answered.insert((prompt.prompt_id.clone(), g)) {
                continue;
            }
            match prompt.kind.as_str() {
                "approval" => {
                    effects.push(HeadlessEffect::DenyApproval {
                        execution_generation: g,
                        prompt_id: prompt.prompt_id,
                    });
                    self.warn(
                        "Hermes asked this headless turn for approval; Talaria declined it.".into(),
                    );
                }
                "clarify" => {
                    effects.push(HeadlessEffect::SkipClarify {
                        execution_generation: g,
                        prompt_id: prompt.prompt_id,
                    });
                    self.warn(
                        "Hermes asked this headless turn a question; Talaria skipped it.".into(),
                    );
                }
                _ => {}
            }
        }
        if foreign && !self.started && self.phase == Phase::Waiting && !self.cancel_sent {
            self.cancel_sent = true;
            effects.push(HeadlessEffect::Cancel);
        }
        effects
    }

    fn server_request(&mut self, key: u64, kind: String) -> Vec<HeadlessEffect> {
        if !self.started || self.completion.is_some() {
            return Vec::new();
        }
        self.warn(format!(
            "Hermes asked for {}, which talaria -p cannot answer; Talaria declined it.",
            piece(&kind)
        ));
        vec![HeadlessEffect::RefuseServerRequest { request_key: key }]
    }

    fn withdrawn(&mut self) -> Vec<HeadlessEffect> {
        self.result.outcome = Some("cancelled".into());
        self.fail(RunStatus::InputRequired, 3,
            "another turn of this session is waiting for an answer, so this prompt was withdrawn before it ran.".into(),
            format!("Answer it in talaria --resume {}, then run the command again.", self.id()), false, true)
    }

    fn cancelled(&mut self, reply: CallReply) -> Vec<HeadlessEffect> {
        if self.completion.is_some() {
            return Vec::new();
        }
        match reply {
            CallReply::Ok(value) if value.get("status").and_then(Value::as_str) == Some("terminal") => {
                if value.get("outcome").and_then(Value::as_str) == Some("cancelled") && !self.started {
                    self.result.outcome = Some("cancelled".into());
                    if self.stop.is_some() { self.finish_stop(StopPoint::Withdrawn, true) } else { self.withdrawn() }
                } else { self.reading = ReadKind::Gap; vec![HeadlessEffect::ReadReceipt] }
            }
            CallReply::Ok(value) if value.get("status").and_then(Value::as_str) == Some("started") => {
                self.started = true;
                if self.generation.is_none() { self.generation = value.get("execution_generation").and_then(Value::as_i64); }
                if self.stop.is_some() { self.phase = Phase::Stopping; vec![HeadlessEffect::StartSettleTimer] } else { Vec::new() }
            }
            CallReply::Refused { reason } if reason.as_deref() == Some("stale_generation") => {
                self.started = true;
                if self.stop.is_some() { self.stop_waiting() } else { Vec::new() }
            }
            _ if self.stop.is_some() => self.finish_stop(StopPoint::WhileSending, true),
            _ => self.fail(RunStatus::Unknown, 1,
                "the prompt could not be withdrawn from behind another turn's question; it may still run.".into(),
                format!("Answer the question in talaria --resume {}, then check the session before sending it again.", self.id()), true, true),
        }
    }

    fn receipt(&mut self, reply: CallReply) -> Vec<HeadlessEffect> {
        if self.reading == ReadKind::Generation {
            if let CallReply::Ok(value) = &reply {
                if value.get("status").and_then(Value::as_str) == Some("terminal") {
                    self.reading = ReadKind::Gap;
                    return self.receipt(reply);
                }
                self.generation = value.get("execution_generation").and_then(Value::as_i64);
            }
            return if self.generation.is_some() {
                self.stop_waiting()
            } else {
                self.finish_stop(StopPoint::WhileSending, true)
            };
        }
        let terminal = match &reply {
            CallReply::Ok(value)
                if value.get("status").and_then(Value::as_str) == Some("terminal") =>
            {
                Some(value)
            }
            _ => None,
        };
        if self.completion.is_some() {
            return self.settled(
                terminal
                    .and_then(|v| v.get("result"))
                    .filter(|v| v.is_object()),
                terminal
                    .and_then(|v| v.get("outcome"))
                    .and_then(Value::as_str),
                true,
            );
        }
        if self.reading == ReadKind::Settle {
            if let Some(receipt) = terminal
                && let Some(result) = receipt.get("result").filter(|v| v.is_object())
            {
                self.result.text = result
                    .get("final_response")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into();
                self.warn(STORED_WARNING.into());
                return self.settled(
                    Some(result),
                    receipt.get("outcome").and_then(Value::as_str),
                    true,
                );
            }
            self.warn(USAGE_WARNING.into());
            return self.finish_stop(StopPoint::AfterStart, true);
        }
        if let Some(receipt) = terminal {
            let outcome = receipt.get("outcome").and_then(Value::as_str);
            if let Some(result) = receipt.get("result").filter(|v| v.is_object()) {
                self.result.text = result
                    .get("final_response")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into();
                self.warn(STORED_WARNING.into());
                return self.settled(Some(result), outcome, true);
            }
            if matches!(outcome, Some("cancelled" | "interrupted" | "failed")) {
                return self.settled(None, outcome, true);
            }
            self.lost_end()
        } else if matches!(reply, CallReply::Ok(_)) {
            Vec::new()
        } else {
            self.unknown("events were lost and Hermes's record of the turn could not be read; its outcome is unknown.", true)
        }
    }

    fn settled(
        &mut self,
        result: Option<&Value>,
        receipt_outcome: Option<&str>,
        detach: bool,
    ) -> Vec<HeadlessEffect> {
        let completion = self.completion.clone().unwrap_or(Value::Null);
        let status = completion.get("status").and_then(Value::as_str);
        let outcome = effective_outcome(
            completion.get("outcome").and_then(Value::as_str),
            receipt_outcome,
            status,
        );
        self.result.outcome = outcome.clone();
        if let Some(result) = result {
            self.result.usage = usage_from_result(result);
            self.result.turn_exit_reason = result
                .get("turn_exit_reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(value) = result.get("model").and_then(Value::as_str) {
                self.result.model = Some(value.into());
            }
            if let Some(value) = result.get("provider").and_then(Value::as_str) {
                self.result.provider = Some(value.into());
            }
        } else {
            self.warn(USAGE_WARNING.into());
        }
        let own_stop = self.stop.filter(|_| self.stop_applies).map(|cause| {
            (
                cause,
                if self.started {
                    StopPoint::AfterStart
                } else {
                    StopPoint::Withdrawn
                },
            )
        });
        let (state, code) = judge(own_stop, status, outcome.as_deref(), result);
        let stop_code = self.stop.map(|cause| match cause {
            StopCause::Timeout => 124,
            StopCause::Signal(signal) => (128 + signal) as u8,
            StopCause::Record => 130,
            StopCause::OutputClosed => 141,
        });
        if self.stop_applies
            && stop_code == Some(code)
            && !matches!(outcome.as_deref(), Some("completed" | "failed"))
        {
            return self.finish_stop(
                if self.started {
                    StopPoint::AfterStart
                } else {
                    StopPoint::Withdrawn
                },
                detach,
            );
        }
        self.result.status = state;
        self.result.exit_code = code;
        if code == 0 {
            self.result.error = None;
            return self.finish(detach);
        }
        let (what, next, resume) = match state {
            RunStatus::Interrupted if !self.started && !self.cancel_sent => (
                "someone else withdrew the prompt before it ran; nothing ran.".into(),
                "Run the command again if you still need it.".into(),
                false,
            ),
            RunStatus::Interrupted => (
                "the turn was interrupted before it finished.".into(),
                "Continue the session if you need the rest.".into(),
                true,
            ),
            RunStatus::Unknown => (
                "Hermes did not report how the turn ended; its outcome is unknown.".into(),
                "Check the session before sending it again.".into(),
                true,
            ),
            _ => {
                let detail = [
                    completion.get("failure_reason"),
                    result.and_then(|r| r.get("error")),
                    result.and_then(|r| r.get("failure_reason")),
                    result.and_then(|r| r.get("turn_exit_reason")),
                ]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|s| !s.trim().is_empty())
                .unwrap_or("no reason given");
                (
                    format!("the turn failed: {}.", piece(detail)),
                    "Continue the session to see what happened.".into(),
                    true,
                )
            }
        };
        self.fail(state, code, what, next, resume, detach)
    }

    fn stop(&mut self, cause: StopCause) -> Vec<HeadlessEffect> {
        if self.stop.is_some() {
            self.stop = Some(cause);
            let mut result = self.stop_result(self.stop_point());
            result.error = Some(ErrorText {
                what:
                    "stopped again before the first stop finished; the turn may still be running."
                        .into(),
                next_step: "Check the session before sending it again.".into(),
            });
            if self.opened {
                result.resume_hint = Some(crate::launch::exit_hint(
                    self.id(),
                    self.result.profile.as_deref().unwrap_or("default"),
                ));
            }
            self.phase = Phase::Done;
            return vec![HeadlessEffect::ExitNow(result)];
        }
        self.stop = Some(cause);
        self.stop_applies = self.phase != Phase::Reading;
        let mut effects = vec![HeadlessEffect::StartStopBudget];
        if self.phase == Phase::Waiting {
            effects.extend(self.stop_waiting());
        }
        effects
    }

    fn stop_waiting(&mut self) -> Vec<HeadlessEffect> {
        self.phase = Phase::Stopping;
        if self.started {
            if let Some(g) = self.generation {
                if self.interrupted {
                    return Vec::new();
                }
                self.interrupted = true;
                vec![
                    HeadlessEffect::Interrupt {
                        execution_generation: g,
                    },
                    HeadlessEffect::StartSettleTimer,
                ]
            } else {
                self.reading = ReadKind::Generation;
                vec![HeadlessEffect::ReadReceipt]
            }
        } else if !self.cancel_sent {
            self.cancel_sent = true;
            vec![HeadlessEffect::Cancel]
        } else {
            Vec::new()
        }
    }

    fn stop_point(&self) -> StopPoint {
        if !self.opened {
            StopPoint::BeforeSubmit
        } else if self.started {
            StopPoint::AfterStart
        } else {
            StopPoint::WhileSending
        }
    }

    fn stop_result(&self, point: StopPoint) -> HeadlessResult {
        let mut result = self.result.clone();
        let cause = self.stop.unwrap_or(StopCause::Timeout);
        let (prefix, next, code) = match cause {
            StopCause::Timeout => (
                "the time limit passed",
                match point {
                    StopPoint::BeforeSubmit => {
                        "Raise --timeout, or check the gateway with talaria."
                    }
                    StopPoint::WhileSending => "Check the session before sending it again.",
                    StopPoint::Withdrawn => "Raise --timeout, or check what holds the session.",
                    _ => "Raise --timeout, or continue the session.",
                },
                124,
            ),
            StopCause::Record => (
                "stopped by an interrupt record",
                "Continue the session if you need the rest.",
                130,
            ),
            StopCause::OutputClosed => (
                "standard output was closed",
                "Continue the session if you need the rest.",
                141,
            ),
            StopCause::Signal(signal) => (
                "stopped by a signal",
                "Continue the session if you need the rest.",
                (128 + signal) as u8,
            ),
        };
        let suffix = match point {
            StopPoint::BeforeSubmit => " before the prompt was sent; nothing ran.",
            StopPoint::WhileSending => " while the prompt was being sent; it may be running.",
            StopPoint::Withdrawn => " before the prompt ran; it was withdrawn and nothing ran.",
            StopPoint::AfterStart => "; the turn was stopped.",
        };
        result.status = if code == 124 {
            RunStatus::TimedOut
        } else {
            RunStatus::Interrupted
        };
        result.exit_code = code;
        result.error = Some(ErrorText {
            what: format!("{prefix}{suffix}"),
            next_step: next.into(),
        });
        result.resume_hint = (self.opened
            && matches!(point, StopPoint::WhileSending | StopPoint::AfterStart))
        .then(|| {
            crate::launch::exit_hint(
                self.id(),
                self.result.profile.as_deref().unwrap_or("default"),
            )
        });
        result
    }

    fn finish_stop(&mut self, point: StopPoint, detach: bool) -> Vec<HeadlessEffect> {
        self.result = self.stop_result(point);
        self.finish(detach)
    }

    fn link_failed(&mut self, failure: Option<(String, String)>) -> Vec<HeadlessEffect> {
        if self.completion.is_some() {
            return self.settled(None, None, false);
        }
        if self.stop.is_some() {
            self.warn(piece(
                failure
                    .as_ref()
                    .map(|(reason, _)| reason.as_str())
                    .unwrap_or("Hermes no longer knows this session."),
            ));
            return self.finish_stop(self.stop_point(), false);
        }
        if !self.opened {
            return self.fail(
                RunStatus::NotStarted,
                4,
                failure
                    .map(|(reason, _)| piece(&reason))
                    .unwrap_or("Hermes no longer knows this session.".into()),
                "Check the gateway with talaria, then check the session.".into(),
                false,
                false,
            );
        }
        if let Some((_, next)) = failure {
            self.fail(RunStatus::Unknown, 1, "the connection to Hermes was lost before the turn finished; it may still be running.".into(), piece(&next), true, false)
        } else {
            self.fail(
                RunStatus::Unknown,
                1,
                "Hermes no longer knows this session; the turn's outcome is unknown.".into(),
                "Look for the session with talaria --resume before sending the prompt again."
                    .into(),
                false,
                false,
            )
        }
    }
}

/// Most events held while the submit reply is missing.
pub const MAX_HELD_EVENTS: usize = 4096;

#[cfg(test)]
mod tests {
    use super::{
        CallReply, HeadlessEffect, HeadlessInput, HeadlessRun, OpenPrompt, StopCause, SubmitReply,
    };
    use crate::headless::output::HeadlessResult;
    use serde_json::json;

    fn opened() -> HeadlessInput {
        HeadlessInput::Opened {
            session_id: "local-session-1".to_string(),
            stored_session_id: "local-session-1".to_string(),
            profile: "fixture".to_string(),
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            open_prompts: Vec::new(),
            unknown_admissions: 0,
        }
    }

    fn admitted(status: &str) -> HeadlessInput {
        HeadlessInput::SubmitReplied(SubmitReply::Admitted {
            admission_id: "adm-1".to_string(),
            status: status.to_string(),
            execution_generation: None,
        })
    }

    fn event(admission: Option<&str>, kind: &str, payload: serde_json::Value) -> HeadlessInput {
        HeadlessInput::Event {
            admission_id: admission.map(str::to_string),
            execution_generation: Some(7),
            event_type: kind.to_string(),
            payload,
        }
    }

    fn start(admission: &str) -> HeadlessInput {
        event(Some(admission), "message.start", json!({}))
    }

    fn completion(admission: &str) -> HeadlessInput {
        event(
            Some(admission),
            "message.complete",
            json!({
                "text": "5555",
                "content": "5555",
                "admission_id": admission,
                "outcome": "completed",
                "status": "complete",
            }),
        )
    }

    fn finish_of(effects: &[HeadlessEffect]) -> Option<&HeadlessResult> {
        effects.iter().find_map(|effect| match effect {
            HeadlessEffect::Finish(done) | HeadlessEffect::ExitNow(done) => Some(done),
            _ => None,
        })
    }

    #[test]
    fn happy_path_effects_in_order() {
        let mut run = HeadlessRun::new("hello".to_string());
        assert_eq!(
            run.step(opened()),
            vec![HeadlessEffect::Submit {
                text: "hello".to_string()
            }]
        );
        assert!(run.step(admitted("queued")).is_empty());
        assert!(run.step(start("adm-1")).is_empty());
        assert_eq!(
            run.step(completion("adm-1")),
            vec![HeadlessEffect::ReadReceipt]
        );
        let effects = run.step(HeadlessInput::ReceiptRead(CallReply::Ok(json!({
            "admission_id": "adm-1",
            "status": "terminal",
            "outcome": "completed",
            "result": { "completed": true, "final_response": "5555" },
        }))));
        assert!(effects.contains(&HeadlessEffect::Detach));
        let done = finish_of(&effects).expect("a finish");
        assert_eq!(done.exit_code, 0);
        assert_eq!(done.text, "5555");
    }

    #[test]
    fn events_before_submit_reply_are_held() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        // Another turn's events before the reply are held, never acted on.
        assert!(run.step(start("adm-9")).is_empty());
        assert!(run.step(completion("adm-9")).is_empty());
        // A foreign approval before the reply is not held: once the reply
        // lands, our queued prompt is withdrawn for it.
        let effects = run.step(event(
            Some("adm-9"),
            "approval.request",
            json!({ "prompt_id": "p-1", "execution_generation": 3 }),
        ));
        assert!(effects.is_empty());
        let effects = run.step(admitted("queued"));
        assert!(effects.contains(&HeadlessEffect::Cancel));
    }

    #[test]
    fn other_admissions_never_finish_the_run() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        assert!(run.step(completion("adm-9")).is_empty());
        assert!(run.step(start("adm-9")).is_empty());
    }

    #[test]
    fn submit_reply_kinds() {
        for (reply, code) in [
            (
                SubmitReply::Refused {
                    reason: Some("busy".to_string()),
                },
                4,
            ),
            (SubmitReply::Refused { reason: None }, 4),
            (SubmitReply::VoiceStopped, 4),
            (SubmitReply::Unknown, 1),
        ] {
            let mut run = HeadlessRun::new("hello".to_string());
            let _ = run.step(opened());
            let effects = run.step(HeadlessInput::SubmitReplied(reply));
            let done = finish_of(&effects).expect("a finish");
            assert_eq!(done.exit_code, code);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
            );
        }
        // An unknown status also ends unknown with no second submit.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let effects = run.step(admitted("unknown"));
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn blocked_session_refused_before_submit() {
        let with_prompt = HeadlessInput::Opened {
            session_id: "s".to_string(),
            stored_session_id: "s".to_string(),
            profile: "fixture".to_string(),
            model: None,
            provider: None,
            open_prompts: vec![OpenPrompt {
                prompt_id: "p-1".to_string(),
                kind: "approval".to_string(),
                admission_id: None,
                execution_generation: Some(3),
            }],
            unknown_admissions: 0,
        };
        let with_unknown = HeadlessInput::Opened {
            session_id: "s".to_string(),
            stored_session_id: "s".to_string(),
            profile: "fixture".to_string(),
            model: None,
            provider: None,
            open_prompts: Vec::new(),
            unknown_admissions: 1,
        };
        for input in [with_prompt, with_unknown] {
            let mut run = HeadlessRun::new("hello".to_string());
            let effects = run.step(input);
            assert!(effects.contains(&HeadlessEffect::Detach));
            assert_eq!(finish_of(&effects).expect("a finish").exit_code, 3);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
            );
        }
    }

    #[test]
    fn foreign_prompt_while_queued_cancels_ours() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(event(
            Some("adm-9"),
            "approval.request",
            json!({ "prompt_id": "p-9", "execution_generation": 3 }),
        ));
        assert_eq!(effects, vec![HeadlessEffect::Cancel]);
        // The cancel's own settlement arrives before the reply.
        let effects = run.step(event(
            Some("adm-1"),
            "message.complete",
            json!({
                "text": "",
                "content": "",
                "admission_id": "adm-1",
                "outcome": "cancelled",
            }),
        ));
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 3);
    }

    #[test]
    fn prompt_for_own_turn_is_declined() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(HeadlessInput::SubmitReplied(SubmitReply::Admitted {
            admission_id: "adm-1".to_string(),
            status: "started".to_string(),
            execution_generation: Some(7),
        }));
        let effects = run.step(event(
            Some("adm-1"),
            "approval.request",
            json!({ "prompt_id": "p-1", "execution_generation": 7 }),
        ));
        assert!(effects.contains(&HeadlessEffect::DenyApproval {
            execution_generation: 7,
            prompt_id: "p-1".to_string(),
        }));
        let effects = run.step(event(
            Some("adm-1"),
            "clarify.request",
            json!({ "prompt_id": "c-1", "execution_generation": 7 }),
        ));
        assert!(effects.contains(&HeadlessEffect::SkipClarify {
            execution_generation: 7,
            prompt_id: "c-1".to_string(),
        }));
    }

    #[test]
    fn server_requests_by_turn_state() {
        // Queued: another viewer's turn owns the request; leave it alone.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::ServerRequest {
            request_key: 1,
            kind: "sudo".to_string(),
        });
        assert!(!effects.contains(&HeadlessEffect::RefuseServerRequest { request_key: 1 }));
        assert!(!effects.contains(&HeadlessEffect::Cancel));
        // Started: our turn owns it; refuse at once.
        let _ = run.step(start("adm-1"));
        let effects = run.step(HeadlessInput::ServerRequest {
            request_key: 2,
            kind: "sudo".to_string(),
        });
        assert!(effects.contains(&HeadlessEffect::RefuseServerRequest { request_key: 2 }));
    }

    #[test]
    fn stop_interrupts_started_cancels_queued() {
        // Started with a known generation: interrupt it.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(start("adm-1"));
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(15)));
        assert!(effects.contains(&HeadlessEffect::StartStopBudget));
        assert!(effects.contains(&HeadlessEffect::Interrupt {
            execution_generation: 7
        }));
        // Queued: cancel it instead.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::Stop(StopCause::Timeout));
        assert!(effects.contains(&HeadlessEffect::Cancel));
    }

    #[test]
    fn settle_wait_and_stop_budget() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(start("adm-1"));
        let _ = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        // No settlement before the deadline: the receipt is read once.
        let effects = run.step(HeadlessInput::SettleDeadline);
        assert!(effects.contains(&HeadlessEffect::ReadReceipt));
        // An expired budget finishes with the stop code and no detach.
        let effects = run.step(HeadlessInput::StopBudgetExpired);
        let done = finish_of(&effects).expect("a finish");
        assert_eq!(done.exit_code, 130);
        assert!(!effects.contains(&HeadlessEffect::Detach));
    }

    #[test]
    fn second_stop_exits_now() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(HeadlessInput::Stop(StopCause::Timeout));
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        let exited = effects
            .iter()
            .find_map(|effect| match effect {
                HeadlessEffect::ExitNow(done) => Some(done),
                _ => None,
            })
            .expect("an immediate exit");
        assert_eq!(exited.exit_code, 130);
        assert!(!effects.contains(&HeadlessEffect::Detach));
    }

    #[test]
    fn gap_rules() {
        // A queued row keeps waiting for the completion.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        assert_eq!(run.step(HeadlessInput::Gap), vec![HeadlessEffect::Recover]);
        let effects = run.step(HeadlessInput::Recovered {
            own_pending_status: Some("queued".to_string()),
            own_pending_generation: None,
            open_prompts: Vec::new(),
        });
        assert!(finish_of(&effects).is_none());
        // An unknown row ends unknown.
        let effects = run.step(HeadlessInput::Recovered {
            own_pending_status: Some("unknown".to_string()),
            own_pending_generation: None,
            open_prompts: Vec::new(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn link_failure_by_phase() {
        // Before the session opened: nothing ran.
        let mut run = HeadlessRun::new("hello".to_string());
        let effects = run.step(HeadlessInput::LinkFailed {
            reason: "gone".to_string(),
            next_step: "retry".to_string(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 4);
        // After the submit: the prompt may be running.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::LinkFailed {
            reason: "gone".to_string(),
            next_step: "retry".to_string(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
        // A gone session after the submit is unknown too.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::SessionGone);
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn stop_before_and_during_submit() {
        // Before the session opened: no submit follows the stop.
        let mut run = HeadlessRun::new("hello".to_string());
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        assert!(effects.contains(&HeadlessEffect::StartStopBudget));
        let effects = run.step(opened());
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
        );
        assert!(effects.contains(&HeadlessEffect::Detach));
        // During the submit: the reply is waited for, nothing is cancelled.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let effects = run.step(HeadlessInput::Stop(StopCause::Timeout));
        assert!(!effects.contains(&HeadlessEffect::Cancel));
        assert!(!effects.contains(&HeadlessEffect::Interrupt {
            execution_generation: 7
        }));
    }
    #[test]
    fn observes_only_own_admission() {
        let mut run = HeadlessRun::new("hello".into());
        let _ = run.step(opened());
        let _ = run.step(admitted("started"));
        for kind in ["message.delta", "tool.start", "tool.complete"] {
            let payload = json!({"text":"own", "is_error":true, "tool_call_id":"t"});
            assert!(
                run.step(event(Some("adm-9"), kind, payload.clone()))
                    .is_empty()
            );
            assert_eq!(
                run.step(event(Some("adm-1"), kind, payload.clone())),
                vec![HeadlessEffect::Observed {
                    event_type: kind.into(),
                    payload
                }]
            );
        }
        assert!(
            run.step(event(
                Some("adm-1"),
                "thinking.delta",
                json!({"text":"hidden"})
            ))
            .is_empty()
        );
    }

    #[test]
    fn held_events_observed_after_reply_in_order() {
        let mut run = HeadlessRun::new("hello".into());
        let _ = run.step(opened());
        let early = [
            ("message.delta", json!({"text":"a"})),
            ("tool.start", json!({"tool_call_id":"t"})),
            (
                "tool.complete",
                json!({"tool_call_id":"t", "is_error":true}),
            ),
        ];
        for (kind, payload) in &early {
            assert!(
                run.step(event(Some("adm-1"), kind, payload.clone()))
                    .is_empty()
            );
        }
        assert!(
            run.step(event(
                Some("adm-9"),
                "message.delta",
                json!({"text":"foreign"})
            ))
            .is_empty()
        );
        let effects = run.step(admitted("started"));
        let observed: Vec<_> = effects
            .into_iter()
            .filter_map(|effect| match effect {
                HeadlessEffect::Observed {
                    event_type,
                    payload,
                } => Some((event_type, payload)),
                _ => None,
            })
            .collect();
        assert_eq!(
            observed,
            early
                .into_iter()
                .map(|(kind, payload)| (kind.to_string(), payload))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn record_and_output_closed_stops() {
        for (cause, code) in [(StopCause::Record, 130), (StopCause::OutputClosed, 141)] {
            let mut run = HeadlessRun::new("hello".into());
            let _ = run.step(opened());
            let _ = run.step(admitted("started"));
            let _ = run.step(start("adm-1"));
            let effects = run.step(HeadlessInput::Stop(cause));
            assert!(effects.contains(&HeadlessEffect::Interrupt {
                execution_generation: 7
            }));
            let _ = run.step(event(
                Some("adm-1"),
                "message.complete",
                json!({"text":"", "outcome":"cancelled", "status":"interrupted"}),
            ));
            let done = run.step(HeadlessInput::ReceiptRead(CallReply::Ok(
                json!({"status":"terminal", "outcome":"cancelled", "result":{"interrupted":true}}),
            )));
            let result = finish_of(&done).expect("stopped result");
            assert_eq!(result.exit_code, code);
            assert_eq!(
                result.status,
                crate::headless::output::RunStatus::Interrupted
            );
            let mut queued = HeadlessRun::new("hello".into());
            let _ = queued.step(opened());
            let _ = queued.step(admitted("queued"));
            assert!(
                queued
                    .step(HeadlessInput::Stop(cause))
                    .contains(&HeadlessEffect::Cancel)
            );
        }
    }
}
