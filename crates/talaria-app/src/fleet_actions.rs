//! Run the fleet overlay's effects.
//!
//! The overlay's own work (closing it, opening a row, acknowledging, the foreground row's
//! reply and stop) happens on the app state. Peek work runs on one worker task that owns the
//! [`PeekService`]: the effects of a batch reach it in order, a reply or a stop for a row
//! whose peek is still opening waits for that open, and the peek link's events are reduced
//! there too, so the app loop never waits on the network.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hermes_client::liveness::LinkEvent;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet_view::{
    FleetViewEffect, FleetViewInput, FleetViewState, PeekStatus, ReplyOutcome, StopOutcome,
};
use talaria_core::session::{SessionState, TurnState};
use tokio::sync::{mpsc, oneshot};

use crate::app::{AppEffect, AppMessage, AppState};
use crate::launch::Launch;
use crate::peek::{CardAnswer, FleetSink, PeekService, PeekTimeouts, PeekUpdate};
use crate::prompts::{MaskedRequestInfo, PromptNotice};
use crate::sessions::ExecuteOutcome;

/// Peek frames are sent to the screen at most this often.
const FRAME_GAP: Duration = Duration::from_millis(33);

/// What the screen draws for the peeked session.
#[derive(Clone, Debug)]
pub struct PeekFrame {
    pub key: RowKey,
    pub state: SessionState,
    pub masked: Vec<MaskedRequestInfo>,
}

/// What the worker tells the app between effect results.
pub enum PeekReport {
    /// The peeked session changed.
    Frame(Box<PeekFrame>),
    /// The peek closed.
    Gone,
    /// An overlay input, such as a new peek status.
    Input(FleetViewInput),
    /// Notices of a stop or an answer.
    Notices(Vec<PromptNotice>),
}

enum Job {
    Effect(FleetViewEffect, oneshot::Sender<Vec<FleetViewInput>>),
    Answer(CardAnswer, oneshot::Sender<Vec<PromptNotice>>),
}

/// The handle of the peek worker. Dropping it closes the peek.
pub struct PeekWorker {
    jobs: mpsc::UnboundedSender<Job>,
}

/// What a worker needs from the launch.
pub struct PeekSetup {
    pub launch: Launch,
    pub home: PathBuf,
    pub fleet: Arc<dyn FleetSink>,
    pub next_submission_id: Box<dyn FnMut() -> String + Send>,
    pub timeouts: PeekTimeouts,
    pub reports: Option<mpsc::UnboundedSender<PeekReport>>,
}

impl PeekWorker {
    /// Spawn the worker. It ends when this handle is dropped, closing the peek first.
    #[must_use]
    pub fn start(setup: PeekSetup) -> Self {
        let (jobs, receiver) = mpsc::unbounded_channel();
        let service = PeekService::new(
            setup.launch,
            setup.home,
            setup.fleet,
            setup.next_submission_id,
            setup.timeouts,
        );
        tokio::spawn(work(service, receiver, setup.reports));
        Self { jobs }
    }

    /// Queue one peek effect; the receiver gets the inputs for the overlay.
    pub fn run(&self, effect: FleetViewEffect) -> oneshot::Receiver<Vec<FleetViewInput>> {
        let (reply, receiver) = oneshot::channel();
        let _ = self.jobs.send(Job::Effect(effect, reply));
        receiver
    }

    /// Queue one card answer; the receiver gets the notices of a failed answer.
    pub fn answer(&self, intent: CardAnswer) -> oneshot::Receiver<Vec<PromptNotice>> {
        let (reply, receiver) = oneshot::channel();
        let _ = self.jobs.send(Job::Answer(intent, reply));
        receiver
    }
}

async fn work(
    mut peek: PeekService,
    mut jobs: mpsc::UnboundedReceiver<Job>,
    reports: Option<mpsc::UnboundedSender<PeekReport>>,
) {
    let mut events: Option<mpsc::Receiver<LinkEvent>> = None;
    let mut last_frame = Instant::now() - FRAME_GAP;
    let mut frame_due = false;
    loop {
        let wait = if frame_due {
            (last_frame + FRAME_GAP).saturating_duration_since(Instant::now())
        } else {
            Duration::from_secs(3600)
        };
        tokio::select! {
            job = jobs.recv() => {
                let Some(job) = job else { break };
                match job {
                    Job::Effect(effect, reply) => {
                        let inputs = run_effect(&mut peek, effect).await;
                        let _ = reply.send(inputs);
                    }
                    Job::Answer(intent, reply) => {
                        let notices = peek.answer(intent).await;
                        let _ = reply.send(notices);
                    }
                }
                if let Some(taken) = peek.take_events() {
                    events = Some(taken);
                }
                frame_due = true;
                if peek.state().is_none() {
                    send(&reports, PeekReport::Gone);
                    frame_due = false;
                }
                let notices = peek.take_notices();
                if !notices.is_empty() {
                    send(&reports, PeekReport::Notices(notices));
                }
            }
            event = async {
                match events.as_mut() {
                    Some(events) => events.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                let Some(event) = event else {
                    events = None;
                    continue;
                };
                if let PeekUpdate::Changed { status } = peek.on_link_event(event)
                    && let Some(key) = peek.key().cloned()
                {
                    send(&reports, PeekReport::Input(FleetViewInput::Peek { key, status }));
                    frame_due = true;
                }
            }
            () = tokio::time::sleep(wait), if frame_due => {}
        }
        if frame_due && last_frame.elapsed() >= FRAME_GAP {
            frame_due = false;
            last_frame = Instant::now();
            if let Some((key, state)) = peek.state() {
                send(
                    &reports,
                    PeekReport::Frame(Box::new(PeekFrame {
                        key: key.clone(),
                        state: state.clone(),
                        masked: peek.masked_requests(),
                    })),
                );
            }
        }
    }
    peek.close().await;
}

fn send(reports: &Option<mpsc::UnboundedSender<PeekReport>>, report: PeekReport) {
    if let Some(reports) = reports {
        let _ = reports.send(report);
    }
}

/// Make sure the peek is on `key`, opening it when needed. `Err` is the reason it failed.
async fn ensure(peek: &mut PeekService, key: &RowKey) -> Result<(), String> {
    if peek.key() == Some(key) {
        return Ok(());
    }
    peek.open(key.clone()).await.map_err(|error| error.reason())
}

async fn run_effect(peek: &mut PeekService, effect: FleetViewEffect) -> Vec<FleetViewInput> {
    match effect {
        FleetViewEffect::OpenPeek(key) => {
            let status = match peek.open(key.clone()).await {
                Ok(()) => peek.status().unwrap_or(PeekStatus::Opening),
                Err(error) => PeekStatus::Failed { text: error.text() },
            };
            vec![FleetViewInput::Peek { key, status }]
        }
        FleetViewEffect::ClosePeek => {
            peek.detach().await;
            vec![FleetViewInput::PeekClosed]
        }
        FleetViewEffect::SendReply { key, text } => {
            let outcome = match ensure(peek, &key).await {
                Ok(()) => peek.reply(text).await,
                Err(reason) => ReplyOutcome::NotAttached { reason },
            };
            vec![FleetViewInput::ReplyDone { key, outcome }]
        }
        FleetViewEffect::ResendReply(key) => {
            let outcome = match ensure(peek, &key).await {
                Ok(()) => peek.resend_unconfirmed(&key).await,
                Err(reason) => ReplyOutcome::NotAttached { reason },
            };
            vec![FleetViewInput::ReplyDone { key, outcome }]
        }
        FleetViewEffect::Stop(key) => {
            let outcome = match ensure(peek, &key).await {
                Ok(()) => peek.stop().await,
                Err(reason) => StopOutcome::NotAttached { reason },
            };
            vec![FleetViewInput::StopDone { key, outcome }]
        }
        FleetViewEffect::Close => {
            peek.close().await;
            peek.forget_unconfirmed();
            vec![FleetViewInput::PeekClosed]
        }
        // Handled on the app state before an effect reaches the worker.
        FleetViewEffect::Acknowledge(_)
        | FleetViewEffect::Open(_)
        | FleetViewEffect::FocusAnswer
        | FleetViewEffect::Review(_) => Vec::new(),
    }
}

/// The panel status of the foreground session.
#[must_use]
pub fn foreground_status(state: &SessionState, masked: bool) -> PeekStatus {
    PeekStatus::Open {
        has_open_prompt: state.open_prompts().next().is_some() || masked,
        running: *state.turn() == TurnState::Running,
    }
}

/// The scrollback notice for each reply still unsettled when the overlay closes.
#[must_use]
pub fn unsettled_notices(view: &FleetViewState) -> Vec<String> {
    view.unsettled_replies()
        .into_iter()
        .map(|(_, title, why)| {
            format!(
                "The reply to \"{title}\" may not have been sent ({why}). Open the session to check before sending it again."
            )
        })
        .collect()
}

/// A foreground reply's result in the overlay's words.
#[must_use]
pub fn reply_outcome(outcome: &ExecuteOutcome, status: Option<&str>) -> ReplyOutcome {
    match outcome {
        ExecuteOutcome::Sent => match status {
            Some("queued") => ReplyOutcome::Queued,
            Some("terminal") => ReplyOutcome::AlreadyRan,
            Some("unknown") => ReplyOutcome::StateUnknown,
            _ => ReplyOutcome::Sent,
        },
        ExecuteOutcome::VoiceStopped => ReplyOutcome::VoiceStopped,
        ExecuteOutcome::Refused { reason } => ReplyOutcome::Refused {
            reason: reason.clone(),
        },
        ExecuteOutcome::OutcomeUnknown => ReplyOutcome::Unknown {
            why: "no reply".to_owned(),
        },
    }
}

/// A foreground stop's result in the overlay's words.
#[must_use]
pub fn stop_outcome(outcome: &ExecuteOutcome) -> StopOutcome {
    match outcome {
        ExecuteOutcome::Sent | ExecuteOutcome::VoiceStopped => StopOutcome::Stopping,
        ExecuteOutcome::Refused { reason } if reason == "no_running_turn" => {
            StopOutcome::NothingRunning
        }
        ExecuteOutcome::Refused { reason } => StopOutcome::Refused {
            reason: reason.clone(),
        },
        ExecuteOutcome::OutcomeUnknown => StopOutcome::Unknown {
            why: "no reply".to_owned(),
        },
    }
}

/// Run one effect and wait for its result. The app loop runs the same work without waiting
/// (`AppEffect::FleetPeek`); tests and callers outside the loop use this.
pub async fn execute(app: &mut AppState, effect: FleetViewEffect) -> Vec<FleetViewInput> {
    let mut inputs = Vec::new();
    for produced in crate::app::fleet_effect(app, effect) {
        match produced {
            AppEffect::FleetPeek(effect) => {
                let worker = crate::app::peek_worker(app);
                if let Ok(result) = worker.run(effect).await {
                    inputs.extend(result);
                }
            }
            AppEffect::FleetForeground(job) => {
                inputs.extend(crate::app::run_foreground(app, job).await);
            }
            AppEffect::FleetInput(input) => inputs.push(input),
            _ => {}
        }
    }
    inputs
}

/// Open the overlay exactly as `/fleet` does, then put the cursor on `key` and peek it.
pub async fn open_fleet_view_at(app: &mut AppState, key: RowKey) {
    let effects = crate::app::open_fleet_view(app);
    let _ = effects;
    let Some(view) = app.fleet.as_mut() else {
        return;
    };
    let produced = view.open_at(key);
    for effect in produced {
        let inputs = execute(app, effect).await;
        for input in inputs {
            let _ = crate::app::update(app, AppMessage::FleetView(input));
        }
    }
}
