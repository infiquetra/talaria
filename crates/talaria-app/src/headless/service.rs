//! Running one headless turn against the gateway.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

use hermes_client::liveness::{Link, LinkEvent, LinkState};
use hermes_client::replay::ReplayGap;
use hermes_client::rpc::{CallError, CallOptions};
use hermes_client::{Client, Inbound, Responder};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use tokio::sync::mpsc::Receiver;
use tokio::task::JoinSet;

use crate::failure::{self, LaunchFailure};
use crate::headless::PrintOptions;
use crate::headless::output::HeadlessResult;
use crate::headless::run::{
    CallReply, HeadlessEffect, HeadlessInput, HeadlessRun, OpenPrompt, StopCause,
    failed_before_open, stopped_before_submit,
};
use crate::headless::wire::{self, HeadlessStartError};
use crate::launch::{Launch, StartMode, new_launch_id, new_request_id, workspace_key};
use crate::sessions::{OpenError, OpenedSession, SessionService, resume_params_id, submission_id};

/// Time limits for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadlessTimings {
    pub receipt: Duration,
    pub cancel: Duration,
    pub interrupt: Duration,
    pub settle: Duration,
    pub detach: Duration,
    pub stop_budget: Duration,
}

impl Default for HeadlessTimings {
    fn default() -> Self {
        Self {
            receipt: Duration::from_secs(10),
            cancel: Duration::from_secs(5),
            interrupt: Duration::from_secs(2),
            settle: Duration::from_secs(10),
            detach: Duration::from_secs(2),
            stop_budget: Duration::from_secs(25),
        }
    }
}

impl HeadlessTimings {
    /// Every limit of the default, divided by one hundred, for fast tests.
    pub fn scaled_for_tests() -> Self {
        let limits = Self::default();
        Self {
            receipt: limits.receipt / 100,
            cancel: limits.cancel / 100,
            interrupt: limits.interrupt / 100,
            settle: limits.settle / 100,
            detach: limits.detach / 100,
            stop_budget: limits.stop_budget / 100,
        }
    }
}

/// Open the session, run one finite turn, detach, and report what happened.
pub async fn run_headless(
    launch: &Launch,
    options: &PrintOptions,
    text: String,
    home: &Path,
    started: Instant,
    mut stops: Receiver<StopCause>,
    timings: HeadlessTimings,
) -> HeadlessResult {
    let _ = options; // The caller starts the process deadline before reading standard input.
    let recorder = match launch
        .record
        .as_deref()
        .map(crate::app::create_recorder)
        .transpose()
    {
        Ok(recorder) => recorder,
        Err(error) => return timed(failed_before_open(error), started),
    };
    let recording = Recording(recorder);
    let connection = tokio::select! {
        biased;
        Some(cause) = stops.recv() => return timed(stopped_before_submit(cause), started),
        result = crate::connection::ConnectionService::open(launch, home, recording.0.clone()) => match result {
            Ok(connection) => connection,
            Err(error) => return timed(failed_before_open(error), started),
        }
    };
    let mut request = match wire::plan_headless(launch, new_request_id(), &connection.describe) {
        Ok(request) => request,
        Err(error) => {
            let failure = match error {
                HeadlessStartError::Plan(failure) => failure,
                HeadlessStartError::OneshotMissing => LaunchFailure { what: "this Hermes gateway does not offer the session source headless mode needs (oneshot).".into(), next_step: "Update Hermes to a release that offers it.".into(), exit_code: 4 },
            };
            connection.link.close().await;
            return timed(failed_before_open(failure), started);
        }
    };
    if let crate::launch::SessionRequest::Latest { workspace } = &mut request {
        let folder = launch.folder.clone();
        let git =
            tokio::task::spawn_blocking(move || crate::connection::find_git_toplevel(&folder));
        let top = tokio::select! {
            Some(cause) = stops.recv() => {
                connection.link.close().await;
                return timed(stopped_before_submit(cause), started);
            }
            result = git => result.ok().flatten(),
        };
        *workspace = workspace_key(&launch.folder, top.as_deref());
    }
    let service = SessionService::with_client(new_launch_id(), connection.link.client());
    let mut machine = HeadlessRun::new(text);
    let mut budget = None;
    let mut stops_open = true;
    let opening_service = service.clone();
    let opening_client = connection.link.client();
    let opening = async move {
        let opened = opening_service.open(request).await?;
        Ok::<_, OpenError>(refresh(&opening_client, opened, timings.receipt).await)
    };
    tokio::pin!(opening);
    let (opened, snapshot) = loop {
        tokio::select! {
            biased;
            cause = stops.recv(), if stops_open => match cause {
                Some(cause) => {
                    let effects = machine.step(HeadlessInput::Stop(cause));
                    if let Some(result) = immediate(effects, &mut budget, timings.stop_budget) {
                        connection.link.close().await;
                        return timed(result, started);
                    }
                }
                None => stops_open = false,
            },
            _ = wait_deadline(budget) => {
                let effects = machine.step(HeadlessInput::StopBudgetExpired);
                if let Some(result) = immediate(effects, &mut budget, timings.stop_budget) {
                    connection.link.close().await;
                    return timed(result, started);
                }
            },
            result = &mut opening => match result {
                Ok(pair) => break pair,
                Err(error) => {
                    let effects = machine.step(HeadlessInput::OpenFailed(open_failure(error, launch)));
                    connection.link.close().await;
                    if let Some(result) = immediate(effects, &mut budget, timings.stop_budget) { return timed(result, started); }
                    return timed(failed_before_open(failure::link_failure("the session could not be opened")), started);
                }
            },
        }
    };
    let input = opened_input(&opened, snapshot.as_ref().ok(), &connection.profile_name);
    let mut effects = machine.step(input);
    if snapshot.is_err() {
        effects = machine.step(HeadlessInput::OpenFailed(failure::link_failure(
            "the session snapshot could not be refreshed",
        )));
        effects.insert(0, HeadlessEffect::Detach);
    }
    connection
        .link
        .track(&opened.session_id, opened.cursor.clone());
    let mut engine = Engine {
        link: Some(connection.link),
        inbound: connection.inbound,
        service,
        machine,
        opened,
        jobs: JoinSet::new(),
        responders: HashMap::new(),
        next_request: 0,
        effects: effects.into(),
        budget,
        settle: None,
        final_result: None,
        detaching: false,
        gap: None,
        timings,
        inbound_open: true,
    };
    let result = engine.drive(&mut stops, stops_open).await;
    for (_, (_, responder)) in engine.responders.drain() {
        responder.ignore();
    }
    engine.jobs.abort_all();
    if let Some(link) = engine.link.take() {
        link.close().await;
    }
    timed(result, started)
}

struct Recording(Option<hermes_client::record::Recorder>);

impl Drop for Recording {
    fn drop(&mut self) {
        if let Some(recorder) = self.0.take() {
            let _ = recorder.finish();
        }
    }
}

fn timed(mut result: HeadlessResult, started: Instant) -> HeadlessResult {
    result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    result
}

async fn wait_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

fn immediate(
    effects: Vec<HeadlessEffect>,
    budget: &mut Option<tokio::time::Instant>,
    limit: Duration,
) -> Option<HeadlessResult> {
    for effect in effects {
        match effect {
            HeadlessEffect::StartStopBudget => *budget = Some(tokio::time::Instant::now() + limit),
            HeadlessEffect::Finish(result) | HeadlessEffect::ExitNow(result) => {
                return Some(result);
            }
            _ => {}
        }
    }
    None
}

type Snapshot = (OpenedSession, Result<Value, CallError>);

// Refresh raw pending rows without changing the shared session service's projection.
async fn refresh(client: &Client, mut opened: OpenedSession, limit: Duration) -> Snapshot {
    let reply = client
        .call_raw(
            "session.resume",
            resume_params_id(&opened.session_id),
            CallOptions {
                timeout: Some(limit),
            },
        )
        .await;
    let snapshot = match reply {
        Ok(raw) => match serde_json::from_value::<CanonicalSnapshot>(raw.clone()) {
            Ok(snapshot) if snapshot.session_id == opened.session_id => {
                opened = OpenedSession::from_snapshot(snapshot);
                Ok(raw)
            }
            _ => Err(CallError::BadResult {
                method: "session.resume".into(),
                reason: "the snapshot did not decode",
            }),
        },
        Err(error) => Err(error),
    };
    (opened, snapshot)
}

fn prompts(snapshot: Option<&Value>) -> Vec<OpenPrompt> {
    snapshot
        .and_then(|v| v.get("prompts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(OpenPrompt {
                prompt_id: row.get("prompt_id")?.as_str()?.into(),
                kind: row.get("kind")?.as_str()?.into(),
                admission_id: row
                    .get("admission_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                execution_generation: row.get("execution_generation").and_then(Value::as_i64),
            })
        })
        .collect()
}

fn opened_input(opened: &OpenedSession, snapshot: Option<&Value>, profile: &str) -> HeadlessInput {
    HeadlessInput::Opened {
        session_id: opened.session_id.clone(),
        stored_session_id: opened.stored_session_id.clone(),
        profile: profile.into(),
        model: opened.info.model.clone(),
        provider: opened.info.provider.clone(),
        open_prompts: prompts(snapshot),
        unknown_admissions: snapshot
            .and_then(|v| v.get("pending"))
            .and_then(Value::as_array)
            .map_or(0, |rows| {
                rows.iter()
                    .filter(|row| row.get("status").and_then(Value::as_str) == Some("unknown"))
                    .count()
            }),
    }
}

fn open_failure(error: OpenError, launch: &Launch) -> LaunchFailure {
    match (error, &launch.mode) {
        (OpenError::NotFound | OpenError::NotInFolder { .. }, StartMode::Continue) => {
            failure::nothing_to_continue()
        }
        (OpenError::NotFound, StartMode::Resume(Some(id))) => {
            failure::resume_not_found(id, &launch.profile)
        }
        (OpenError::FolderUnknown, _) => failure::folder_unknown(),
        (OpenError::Refused { reason }, _) => failure::refusal(&reason),
        (OpenError::Unknown, StartMode::New) => failure::create_lost_twice(),
        (OpenError::Link(what), _) => failure::link_failure(&what),
        _ => failure::link_failure("the session could not be opened"),
    }
}

enum JobReply {
    Input(HeadlessInput),
    Snapshot(Box<Snapshot>),
    RecoveryFailed(OpenError),
    Detached,
    Ignored,
}

struct Engine {
    link: Option<Link>,
    inbound: Receiver<LinkEvent>,
    service: SessionService,
    machine: HeadlessRun,
    opened: OpenedSession,
    jobs: JoinSet<JobReply>,
    responders: HashMap<u64, (String, Responder)>,
    next_request: u64,
    effects: VecDeque<HeadlessEffect>,
    budget: Option<tokio::time::Instant>,
    settle: Option<tokio::time::Instant>,
    final_result: Option<HeadlessResult>,
    detaching: bool,
    gap: Option<ReplayGap>,
    timings: HeadlessTimings,
    inbound_open: bool,
}

impl Engine {
    fn feed(&mut self, input: HeadlessInput) {
        self.effects.extend(self.machine.step(input));
    }

    async fn drive(
        &mut self,
        stops: &mut Receiver<StopCause>,
        mut stops_open: bool,
    ) -> HeadlessResult {
        loop {
            while let Some(effect) = self.effects.pop_front() {
                if let HeadlessEffect::ExitNow(result) = effect {
                    return result;
                }
                self.execute(effect);
            }
            if !self.detaching
                && let Some(result) = self.final_result.take()
            {
                return result;
            }
            tokio::select! {
                biased;
                cause = stops.recv(), if stops_open => match cause { Some(cause) => self.feed(HeadlessInput::Stop(cause)), None => stops_open = false },
                _ = wait_deadline(self.budget) => {
                    self.budget = None;
                    if let Some(mut result) = self.final_result.take() {
                        result.warnings.push("Talaria stopped waiting for Hermes; the turn may still be running.".into());
                        return result;
                    }
                    self.detaching = false;
                    self.feed(HeadlessInput::StopBudgetExpired);
                },
                _ = wait_deadline(self.settle) => { self.settle = None; self.feed(HeadlessInput::SettleDeadline); },
                event = self.inbound.recv(), if self.inbound_open => match event {
                    Some(event) => self.link_event(event),
                    None => { self.inbound_open = false; self.feed(HeadlessInput::LinkFailed { reason: "the connection closed".into(), next_step: "Check the gateway with talaria, then check the session.".into() }); },
                },
                result = self.jobs.join_next(), if !self.jobs.is_empty() => match result {
                    Some(Ok(result)) => self.job(result),
                    Some(Err(_)) => self.feed(HeadlessInput::LinkFailed { reason: "a gateway call stopped".into(), next_step: "Check the gateway with talaria, then check the session.".into() }),
                    None => {},
                },
            }
        }
    }

    fn execute(&mut self, effect: HeadlessEffect) {
        let is_approval = matches!(&effect, HeadlessEffect::DenyApproval { .. });
        let Some(client) = self.service.client().cloned() else {
            return;
        };
        let session = self.opened.session_id.clone();
        match effect {
            HeadlessEffect::Submit { text } => {
                let submission = submission_id(self.service.launch_id(), 1);
                self.jobs.spawn(async move {
                    let reply = client
                        .call_raw(
                            "prompt.submit",
                            wire::submit_params_finite(&session, &text, &submission),
                            CallOptions::default(),
                        )
                        .await;
                    let reply = match reply {
                        Ok(value)
                            if value.get("voice_stopped").and_then(Value::as_bool)
                                == Some(true) =>
                        {
                            crate::headless::run::SubmitReply::VoiceStopped
                        }
                        Ok(value) => match (
                            value.get("admission_id").and_then(Value::as_str),
                            value.get("status").and_then(Value::as_str),
                        ) {
                            (Some(id), Some(status)) => {
                                crate::headless::run::SubmitReply::Admitted {
                                    admission_id: id.into(),
                                    status: status.into(),
                                    execution_generation: value
                                        .get("execution_generation")
                                        .and_then(Value::as_i64),
                                }
                            }
                            _ => crate::headless::run::SubmitReply::Unknown,
                        },
                        Err(CallError::Rpc(error)) => crate::headless::run::SubmitReply::Refused {
                            reason: error.reason,
                        },
                        Err(_) => crate::headless::run::SubmitReply::Unknown,
                    };
                    JobReply::Input(HeadlessInput::SubmitReplied(reply))
                });
            }
            HeadlessEffect::ReadReceipt
            | HeadlessEffect::Cancel
            | HeadlessEffect::Interrupt { .. } => {
                let admission = self.machine.admission_id().unwrap_or("").to_string();
                let (method, params, timeout) = match &effect {
                    HeadlessEffect::ReadReceipt => (
                        "prompt.receipt",
                        wire::receipt_params(&session, &admission),
                        self.timings.receipt,
                    ),
                    HeadlessEffect::Cancel => (
                        "prompt.cancel",
                        wire::cancel_params(&session, &admission),
                        self.timings.cancel,
                    ),
                    HeadlessEffect::Interrupt {
                        execution_generation,
                    } => (
                        "session.interrupt",
                        wire::interrupt_params(&session, *execution_generation),
                        self.timings.interrupt,
                    ),
                    _ => return,
                };
                self.jobs.spawn(async move {
                    let reply = call_reply(
                        client
                            .call_raw(
                                method,
                                params,
                                CallOptions {
                                    timeout: Some(timeout),
                                },
                            )
                            .await,
                    );
                    JobReply::Input(match effect {
                        HeadlessEffect::ReadReceipt => HeadlessInput::ReceiptRead(reply),
                        HeadlessEffect::Cancel => HeadlessInput::CancelReplied(reply),
                        _ => HeadlessInput::InterruptReplied(reply),
                    })
                });
            }
            HeadlessEffect::DenyApproval {
                execution_generation,
                prompt_id,
            }
            | HeadlessEffect::SkipClarify {
                execution_generation,
                prompt_id,
            } => {
                let (method, params) = if is_approval {
                    (
                        "approval.respond",
                        wire::approval_deny_params(&session, execution_generation, &prompt_id),
                    )
                } else {
                    (
                        "clarify.respond",
                        wire::clarify_skip_params(&session, execution_generation, &prompt_id),
                    )
                };
                self.jobs.spawn(async move {
                    let _ = client
                        .call_raw(
                            method,
                            params,
                            CallOptions {
                                timeout: Some(Duration::from_secs(300)),
                            },
                        )
                        .await;
                    JobReply::Ignored
                });
            }
            HeadlessEffect::RefuseServerRequest { request_key } => {
                if let Some((kind, responder)) = self.responders.remove(&request_key) {
                    self.jobs.spawn(async move {
                        let _ = responder
                            .fail(
                                -32601,
                                &format!("talaria -p cannot answer server request: {kind}"),
                            )
                            .await;
                        JobReply::Ignored
                    });
                }
            }
            HeadlessEffect::Recover => {
                if let Some(gap) = self.gap.take() {
                    let service = self.service.clone();
                    let limit = self.timings.receipt;
                    self.jobs.spawn(async move {
                        match service.recover(gap).await {
                            Ok(opened) => {
                                JobReply::Snapshot(Box::new(refresh(&client, opened, limit).await))
                            }
                            Err(error) => JobReply::RecoveryFailed(error),
                        }
                    });
                }
            }
            HeadlessEffect::StartSettleTimer => {
                self.settle = Some(tokio::time::Instant::now() + self.timings.settle)
            }
            HeadlessEffect::StartStopBudget => {
                self.budget = Some(tokio::time::Instant::now() + self.timings.stop_budget)
            }
            HeadlessEffect::Detach => {
                let service = self.service.clone();
                let opened = self.opened.clone();
                let timeout = self.timings.detach;
                self.detaching = true;
                self.jobs.spawn(async move {
                    let _ = tokio::time::timeout(timeout, service.detach(&opened)).await;
                    JobReply::Detached
                });
            }
            HeadlessEffect::Finish(result) => self.final_result = Some(result),
            HeadlessEffect::ExitNow(_) => {}
        }
    }

    fn job(&mut self, reply: JobReply) {
        match reply {
            JobReply::Input(input) => self.feed(input),
            JobReply::Snapshot(pair) => {
                let (opened, snapshot) = *pair;
                let Ok(snapshot) = snapshot else {
                    self.feed(HeadlessInput::LinkFailed {
                        reason: "the session snapshot could not be read".into(),
                        next_step: "Check the session before sending it again.".into(),
                    });
                    return;
                };
                let row = snapshot
                    .get("pending")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|row| {
                        row.get("admission_id").and_then(Value::as_str)
                            == self.machine.admission_id()
                    });
                let input = HeadlessInput::Recovered {
                    own_pending_status: row
                        .and_then(|r| r.get("status"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    own_pending_generation: row
                        .and_then(|r| r.get("execution_generation"))
                        .and_then(Value::as_i64),
                    open_prompts: prompts(Some(&snapshot)),
                };
                if let Some(link) = &self.link {
                    link.resync(&opened.session_id, opened.cursor.clone());
                }
                self.opened = opened;
                self.feed(input);
            }
            JobReply::RecoveryFailed(OpenError::NotFound) => self.feed(HeadlessInput::SessionGone),
            JobReply::RecoveryFailed(_) => self.feed(HeadlessInput::LinkFailed {
                reason: "the session could not be recovered".into(),
                next_step: "Check the session before sending it again.".into(),
            }),
            JobReply::Detached => self.detaching = false,
            JobReply::Ignored => {}
        }
    }

    fn link_event(&mut self, event: LinkEvent) {
        match event {
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            } if event.frame.session_id.as_deref() == Some(&self.opened.session_id) => {
                let raw = event.raw;
                let payload = raw.get("payload").cloned().unwrap_or(Value::Null);
                self.feed(HeadlessInput::Event {
                    admission_id: raw
                        .get("admission_id")
                        .and_then(Value::as_str)
                        .or_else(|| payload.get("admission_id").and_then(Value::as_str))
                        .map(str::to_string),
                    execution_generation: raw
                        .get("execution_generation")
                        .and_then(Value::as_i64)
                        .or_else(|| payload.get("execution_generation").and_then(Value::as_i64)),
                    event_type: raw.get("type").and_then(Value::as_str).unwrap_or("").into(),
                    payload,
                });
            }
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(request),
                ..
            } => {
                if request.session_id.as_deref() != Some(&self.opened.session_id) {
                    request.responder.ignore();
                    return;
                }
                let kind = request_kind(&request.request).to_string();
                self.next_request += 1;
                self.responders
                    .insert(self.next_request, (kind.clone(), request.responder));
                self.feed(HeadlessInput::ServerRequest {
                    request_key: self.next_request,
                    kind,
                });
            }
            LinkEvent::Gap(gap) if gap.session_id == self.opened.session_id => {
                self.gap = Some(gap);
                self.feed(HeadlessInput::Gap);
            }
            LinkEvent::Reattached {
                session_id,
                subscription_id,
                ..
            } if session_id == self.opened.session_id => {
                self.opened.subscription_id = subscription_id
            }
            LinkEvent::SessionGone { session_id } if session_id == self.opened.session_id => {
                self.feed(HeadlessInput::SessionGone)
            }
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                self.feed(HeadlessInput::LinkFailed { reason, next_step })
            }
            LinkEvent::State(LinkState::Closed) => self.feed(HeadlessInput::LinkFailed {
                reason: "the connection closed".into(),
                next_step: "Check the gateway with talaria, then check the session.".into(),
            }),
            _ => {}
        }
    }
}

fn request_kind(request: &hermes_contract::server_requests::ServerRequest) -> &str {
    use hermes_contract::server_requests::{self as names, ServerRequest};
    match request {
        ServerRequest::Approval(_) => names::approval::NAME,
        ServerRequest::Clarify(_) => names::clarify::NAME,
        ServerRequest::DisplayInstallSudo(_) => names::display_install_sudo::NAME,
        ServerRequest::PreviewAct(_) => names::preview_act::NAME,
        ServerRequest::PreviewRead(_) => names::preview_read::NAME,
        ServerRequest::Secret(_) => names::secret::NAME,
        ServerRequest::SetupBeat(_) => names::setup_beat::NAME,
        ServerRequest::Sudo(_) => names::sudo::NAME,
        ServerRequest::TerminalRead(_) => names::terminal_read::NAME,
        ServerRequest::ThemeApply(_) => names::theme_apply::NAME,
        ServerRequest::ThemePick(_) => names::theme_pick::NAME,
        ServerRequest::Tour(_) => names::tour::NAME,
        ServerRequest::VaultCode(_) => names::vault_code::NAME,
        ServerRequest::VaultSaveLogin(_) => names::vault_save_login::NAME,
        ServerRequest::VaultUnlockPrompt(_) => names::vault_unlock_prompt::NAME,
        ServerRequest::WindowRead(_) => names::window_read::NAME,
        ServerRequest::Unknown(raw) => raw
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
    }
}

fn call_reply(reply: Result<Value, CallError>) -> CallReply {
    match reply {
        Ok(value) => CallReply::Ok(value),
        Err(CallError::Rpc(error)) => CallReply::Refused {
            reason: error.reason,
        },
        Err(_) => CallReply::Failed,
    }
}
