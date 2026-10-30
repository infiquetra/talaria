//! The peek session: one extra link used to read, reply, stop or answer a background session.
//!
//! A peek attaches the session with `session.resume` on a profile-scoped link of its own, so
//! its events never reach the foreground reducer and the fleet link stays free of
//! attachments. Server requests the peek cannot show are ignored, never refused: Hermes writes
//! them to every attached viewer and an error answer would skip them for all of them.
//!
//! The card intent lives in the terminal crate. This service takes [`CardAnswer`], which has
//! the same fields, so this crate does not depend on the terminal crate.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use hermes_client::liveness::{LinkEvent, LinkState};
use hermes_client::{CallError, CallOptions, Client, Inbound};
use hermes_contract::events::Notification;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_core::attention::NeedSummary;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::SubscribedView;
use talaria_core::fleet_view::{PeekStatus, ReplyOutcome, StopOutcome};
use talaria_core::session::{self, Effect, Event, PromptKey, SessionId, SessionState, TurnState};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::connection::{ConnectionService, OpenConnection};
use crate::launch::Launch;
use crate::prompts::{
    MaskedKind, MaskedRequestInfo, PromptNotice, PromptService, ServerRequestPolicy, policy_for,
};
use crate::sessions::{OpenedSession, detach_params, resume_params_id};

/// Why a peek did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekError {
    Connect(String),
    NotFound,
    Refused { reason: String },
    TooLarge,
    Timeout,
    Dropped,
}

impl PeekError {
    /// The panel text for this failure.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            PeekError::NotFound => "This session no longer exists on the gateway.".to_owned(),
            PeekError::TooLarge => {
                "This session's history is larger than Talaria accepts in one message (8 MiB)."
                    .to_owned()
            }
            other => format!("Could not open this session: {}.", other.reason()),
        }
    }

    /// The reason word or phrase, as the not-sent texts name it.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            PeekError::Connect(reason) | PeekError::Refused { reason } => reason.clone(),
            PeekError::NotFound => "not_found".to_owned(),
            PeekError::TooLarge => "the reply was too large".to_owned(),
            PeekError::Timeout => "no reply".to_owned(),
            PeekError::Dropped => "connection dropped".to_owned(),
        }
    }
}

/// What changed for the overlay after one link event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekUpdate {
    Unchanged,
    Changed { status: PeekStatus },
}

/// An answer for the open card, with the same fields as the terminal's prompt intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardAnswer {
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    Clarify {
        key: PromptKey,
        answer: String,
    },
    /// An empty value means declined.
    Masked {
        request_id: String,
        value: String,
    },
}

/// The three registry calls the peek makes. The real registry handle has no test constructor,
/// so tests pass a fake; [`crate::fleet::FleetHandle`] forwards them.
pub trait FleetSink: Send + Sync {
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView);
    fn observe_unsubscribed(&self, key: RowKey);
    fn acknowledge(&self, key: RowKey);
}

impl FleetSink for crate::fleet::FleetHandle {
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView) {
        crate::fleet::FleetHandle::observe_subscribed(self, key, view);
    }

    fn observe_unsubscribed(&self, key: RowKey) {
        crate::fleet::FleetHandle::observe_unsubscribed(self, key);
    }

    fn acknowledge(&self, key: RowKey) {
        crate::fleet::FleetHandle::acknowledge(self, key);
    }
}

/// A sink for a launch without the fleet service: the registry hears nothing.
pub struct NoFleet;

impl FleetSink for NoFleet {
    fn observe_subscribed(&self, _key: RowKey, _view: SubscribedView) {}
    fn observe_unsubscribed(&self, _key: RowKey) {}
    fn acknowledge(&self, _key: RowKey) {}
}

/// Timeouts for resume, ordinary calls, the deny that runs before a stop, and close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeekTimeouts {
    pub resume: Duration,
    pub call: Duration,
    pub deny: Duration,
    pub close: Duration,
}

impl Default for PeekTimeouts {
    fn default() -> Self {
        Self {
            resume: Duration::from_secs(120),
            call: hermes_client::rpc::DEFAULT_CALL_TIMEOUT,
            deny: Duration::from_secs(2),
            close: Duration::from_secs(2),
        }
    }
}

impl PeekTimeouts {
    /// The same timeouts, each divided by 20, for tests that use a real clock.
    #[must_use]
    pub fn scaled_for_tests() -> Self {
        let base = Self::default();
        Self {
            resume: base.resume / 20,
            call: base.call / 20,
            deny: base.deny / 20,
            close: base.close / 20,
        }
    }
}

type IdSource = Arc<Mutex<Box<dyn FnMut() -> String + Send>>>;

fn next_id(source: &IdSource) -> String {
    (source.lock().unwrap_or_else(PoisonError::into_inner))()
}

/// The link of one profile.
struct PeekLink {
    profile: String,
    connection: OpenConnection,
    client: Client,
    /// The link's events went to the app's pump.
    events_taken: bool,
}

/// The attached session.
struct PeekSession {
    key: RowKey,
    opened: OpenedSession,
    state: SessionState,
    prompts: PromptService,
    last_view: Option<SubscribedView>,
}

/// One peek at a time, with its link and the replies whose outcome is not known.
pub struct PeekService {
    launch: Launch,
    home: PathBuf,
    fleet: Arc<dyn FleetSink>,
    next_submission_id: IdSource,
    timeouts: PeekTimeouts,
    link: Option<PeekLink>,
    session: Option<PeekSession>,
    /// Text and submission id of each reply whose outcome is unknown, by row.
    unconfirmed: HashMap<RowKey, (String, String)>,
    /// Notices of the last stop or answer, for the app to show.
    notices: Vec<PromptNotice>,
}

impl PeekService {
    #[must_use]
    pub fn new(
        launch: Launch,
        home: PathBuf,
        fleet: Arc<dyn FleetSink>,
        next_submission_id: Box<dyn FnMut() -> String + Send>,
        timeouts: PeekTimeouts,
    ) -> Self {
        Self {
            launch,
            home,
            fleet,
            next_submission_id: Arc::new(Mutex::new(next_submission_id)),
            timeouts,
            link: None,
            session: None,
            unconfirmed: HashMap::new(),
            notices: Vec::new(),
        }
    }

    /// The key of the attached session.
    #[must_use]
    pub fn key(&self) -> Option<&RowKey> {
        self.session.as_ref().map(|session| &session.key)
    }

    /// Detach any other peek, then resume `key`. A link of the same profile is reused.
    pub async fn open(&mut self, key: RowKey) -> Result<(), PeekError> {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.key == key)
        {
            return Ok(());
        }
        self.detach().await;
        if self
            .link
            .as_ref()
            .is_some_and(|link| link.profile != key.profile)
        {
            self.close_link().await;
        }
        if self.link.is_none() {
            let mut launch = self.launch.clone();
            launch.profile = key.profile.clone();
            let home = self.home.clone();
            self.link = Some(connect(launch, home, self.timeouts.resume).await?);
        }
        let opened = match self.resume(&key.session_id).await {
            Ok(opened) => opened,
            Err(error) => {
                // A link that failed its read is not reused.
                if matches!(error, PeekError::TooLarge | PeekError::Dropped) {
                    self.close_link().await;
                }
                return Err(error);
            }
        };
        self.adopt(key, opened);
        Ok(())
    }

    async fn resume(&mut self, session_id: &str) -> Result<OpenedSession, PeekError> {
        let Some(link) = &mut self.link else {
            return Err(PeekError::Dropped);
        };
        let result = link
            .client
            .call_raw(
                "session.resume",
                resume_params_id(session_id),
                CallOptions {
                    timeout: Some(self.timeouts.resume),
                },
            )
            .await;
        match result {
            Ok(value) => serde_json::from_value::<CanonicalSnapshot>(value)
                .map(OpenedSession::from_snapshot)
                .map_err(|_| PeekError::Refused {
                    reason: "the session reply did not decode".to_owned(),
                }),
            Err(CallError::Rpc(error)) if error.reason.as_deref() == Some("not_found") => {
                Err(PeekError::NotFound)
            }
            Err(CallError::Unknown { .. }) => {
                // An over-limit reply closes the connection as an unreadable read.
                if read_failed(&mut link.connection.inbound).await {
                    Err(PeekError::TooLarge)
                } else {
                    Err(PeekError::Dropped)
                }
            }
            Err(CallError::TimedOut { .. }) => Err(PeekError::Timeout),
            Err(error) => Err(PeekError::Refused {
                reason: call_reason(&error, "session.resume"),
            }),
        }
    }

    fn adopt(&mut self, key: RowKey, opened: OpenedSession) {
        if let Some(link) = &self.link {
            link.connection
                .link
                .track(&opened.session_id, opened.cursor.clone());
        }
        let ids = Arc::clone(&self.next_submission_id);
        let mut prompts = PromptService::new(Box::new(move || next_id(&ids)));
        let state = opened.state.clone();
        let open: Vec<PromptKey> = state
            .open_prompts()
            .map(|prompt| prompt.key.clone())
            .collect();
        let _ = prompts.on_prompts_changed(&open);
        self.session = Some(PeekSession {
            key,
            opened,
            state,
            prompts,
            last_view: None,
        });
        self.report();
    }

    /// Build a peek from a recorded resume reply, with no link. Replays drive it with
    /// [`PeekService::on_link_event`].
    pub fn replay_snapshot(&mut self, key: RowKey, snapshot: Value) -> Result<(), PeekError> {
        let opened = serde_json::from_value::<CanonicalSnapshot>(snapshot)
            .map(OpenedSession::from_snapshot)
            .map_err(|_| PeekError::Refused {
                reason: "the session reply did not decode".to_owned(),
            })?;
        self.session = None;
        self.adopt(key, opened);
        Ok(())
    }

    /// Tell the registry about a changed view of the attached session.
    fn report(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        let masked: Vec<(String, NeedSummary)> = session
            .prompts
            .masked_requests()
            .iter()
            .map(|request| (request.id.clone(), masked_summary(request.kind)))
            .collect();
        let mut view = SubscribedView::from_state(&session.state, &masked);
        if view.title.is_none() {
            view.title.clone_from(&session.opened.title);
        }
        if session.last_view.as_ref() != Some(&view) {
            session.last_view = Some(view.clone());
            self.fleet.observe_subscribed(session.key.clone(), view);
        }
    }

    #[must_use]
    pub fn state(&self) -> Option<(&RowKey, &SessionState)> {
        self.session
            .as_ref()
            .map(|session| (&session.key, &session.state))
    }

    #[must_use]
    pub fn prompts(&self) -> Option<&PromptService> {
        self.session.as_ref().map(|session| &session.prompts)
    }

    /// The masked requests the peek's card shows.
    #[must_use]
    pub fn masked_requests(&self) -> Vec<MaskedRequestInfo> {
        self.prompts()
            .map(PromptService::masked_requests)
            .unwrap_or_default()
    }

    /// The panel status of the attached session.
    #[must_use]
    pub fn status(&self) -> Option<PeekStatus> {
        let session = self.session.as_ref()?;
        Some(PeekStatus::Open {
            has_open_prompt: session.state.open_prompts().next().is_some()
                || !session.prompts.masked_requests().is_empty(),
            running: *session.state.turn() == TurnState::Running,
        })
    }

    /// The link's events, for the app's pump: `Some` once per link, the first time.
    pub fn take_events(&mut self) -> Option<mpsc::Receiver<LinkEvent>> {
        let link = self.link.as_mut().filter(|link| !link.events_taken)?;
        link.events_taken = true;
        let (_, empty) = mpsc::channel(1);
        Some(std::mem::replace(&mut link.connection.inbound, empty))
    }

    /// Notices of the last stop or answer.
    pub fn take_notices(&mut self) -> Vec<PromptNotice> {
        std::mem::take(&mut self.notices)
    }

    /// Reduce one link event. Server requests the peek cannot show are ignored.
    pub fn on_link_event(&mut self, ev: LinkEvent) -> PeekUpdate {
        match ev {
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            } => {
                let Some(session) = &mut self.session else {
                    return PeekUpdate::Unchanged;
                };
                let id = event.raw["session_id"].as_str().unwrap_or_default();
                if id != session.opened.session_id {
                    return PeekUpdate::Unchanged;
                }
                if !accept(&mut session.opened.cursor, &event.raw) {
                    return PeekUpdate::Unchanged;
                }
                if let Notification::RequestCancel(payload) = &event.frame.notification {
                    let _ = session.prompts.on_request_cancel(
                        &payload.id,
                        &payload.method,
                        &payload.reason,
                    );
                }
                let _ = session::update(
                    &mut session.state,
                    Event::Session {
                        session_id: SessionId::new(id),
                        frame: event.frame,
                        raw: event.raw,
                    },
                );
                let open: Vec<PromptKey> = session
                    .state
                    .open_prompts()
                    .map(|prompt| prompt.key.clone())
                    .collect();
                let _ = session.prompts.on_prompts_changed(&open);
                self.changed()
            }
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(request),
                ..
            } => {
                let method = request_method(&request.request);
                let ours = self.session.as_ref().is_some_and(|session| {
                    request
                        .session_id
                        .as_deref()
                        .is_none_or(|id| id == session.opened.session_id)
                });
                match (policy_for(method), &mut self.session) {
                    (ServerRequestPolicy::Masked(_), Some(session)) if ours => {
                        // The masked path registers the card without waiting on anything.
                        if !run_ready(session.prompts.on_server_request(request)) {
                            return PeekUpdate::Unchanged;
                        }
                        self.changed()
                    }
                    // Another viewer may answer it; an error here would skip it for everyone.
                    _ => {
                        request.responder.ignore();
                        PeekUpdate::Unchanged
                    }
                }
            }
            LinkEvent::State(LinkState::Connected { generation }) => {
                if let Some(session) = &mut self.session {
                    let _ = session.prompts.on_connection_replaced(generation);
                }
                self.changed()
            }
            _ => PeekUpdate::Unchanged,
        }
    }

    fn changed(&mut self) -> PeekUpdate {
        self.report();
        match self.status() {
            Some(status) => PeekUpdate::Changed { status },
            None => PeekUpdate::Unchanged,
        }
    }

    /// Send a reply on the peeked session.
    pub async fn reply(&mut self, text: String) -> ReplyOutcome {
        let submission_id = next_id(&self.next_submission_id);
        let Some(session) = &mut self.session else {
            return ReplyOutcome::NotAttached {
                reason: "no session is open".to_owned(),
            };
        };
        let key = session.key.clone();
        let effects = session::update(
            &mut session.state,
            Event::Submit {
                display: text.clone(),
                text: text.clone(),
                submission_id: submission_id.clone(),
                attachments: Vec::new(),
            },
        );
        let has_send = effects
            .iter()
            .any(|effect| matches!(effect, Effect::SendPrompt { .. }));
        let session_id = session.opened.session_id.clone();
        if !has_send {
            return ReplyOutcome::Refused {
                reason: "not_sent".to_owned(),
            };
        }
        let outcome = match self.link.as_ref().map(|link| link.client.clone()) {
            Some(client) => {
                submit_on(
                    client,
                    self.timeouts.call,
                    &session_id,
                    &text,
                    &submission_id,
                )
                .await
                .outcome
            }
            None => ReplyOutcome::Unknown {
                why: "connection dropped".to_owned(),
            },
        };
        if let (ReplyOutcome::Refused { reason }, Some(session)) = (&outcome, &mut self.session) {
            let _ = session::update(
                &mut session.state,
                Event::SubmitFailed {
                    submission_id: submission_id.clone(),
                    reason: reason.clone(),
                },
            );
        }
        if matches!(outcome, ReplyOutcome::Unknown { .. }) {
            self.unconfirmed.insert(key, (text, submission_id));
        }
        self.report();
        outcome
    }

    /// Send the stored unconfirmed reply again, with the same text and submission id.
    pub async fn resend_unconfirmed(&mut self, key: &RowKey) -> ReplyOutcome {
        let Some((text, submission_id)) = self.unconfirmed.get(key).cloned() else {
            return ReplyOutcome::Refused {
                reason: "nothing to resend".to_owned(),
            };
        };
        let Some(session_id) = self
            .session
            .as_ref()
            .filter(|session| session.key == *key)
            .map(|session| session.opened.session_id.clone())
        else {
            return ReplyOutcome::NotAttached {
                reason: "the session is not open".to_owned(),
            };
        };
        let attempt = match self.link.as_ref().map(|link| link.client.clone()) {
            Some(client) => {
                submit_on(
                    client,
                    self.timeouts.call,
                    &session_id,
                    &text,
                    &submission_id,
                )
                .await
            }
            None => SubmitAttempt {
                outcome: ReplyOutcome::Unknown {
                    why: "connection dropped".to_owned(),
                },
                not_connected: false,
            },
        };
        if !attempt.not_connected && !matches!(attempt.outcome, ReplyOutcome::Unknown { .. }) {
            self.unconfirmed.remove(key);
        }
        attempt.outcome
    }
}

/// Open a profile-scoped link for `launch`'s profile, never starting a gateway.
async fn connect(
    mut launch: Launch,
    home: PathBuf,
    timeout: Duration,
) -> Result<PeekLink, PeekError> {
    launch.no_ensure = true;
    launch.record = None;
    let profile = launch.profile.clone();
    let opened = tokio::time::timeout(timeout, ConnectionService::open(&launch, &home, None))
        .await
        .map_err(|_| PeekError::Timeout)?
        .map_err(|failure| PeekError::Connect(failure.what))?;
    let client = opened.link.client();
    Ok(PeekLink {
        profile,
        connection: opened,
        client,
        events_taken: false,
    })
}

struct SubmitAttempt {
    outcome: ReplyOutcome,
    not_connected: bool,
}

/// `prompt.submit` with exactly the three keys, and its outcome.
async fn submit_on(
    client: Client,
    timeout: Duration,
    session_id: &str,
    text: &str,
    submission_id: &str,
) -> SubmitAttempt {
    {
        let params =
            json!({"session_id": session_id, "text": text, "submission_id": submission_id});
        let result = client
            .call_raw(
                "prompt.submit",
                params,
                CallOptions {
                    timeout: Some(timeout),
                },
            )
            .await;
        let not_connected = matches!(&result, Err(CallError::NotConnected));
        let outcome = match result {
            Ok(reply) if reply.get("voice_stopped") == Some(&Value::Bool(true)) => {
                ReplyOutcome::VoiceStopped
            }
            Ok(reply) => match reply["status"].as_str() {
                Some("queued") => ReplyOutcome::Queued,
                Some("terminal") => ReplyOutcome::AlreadyRan,
                Some("unknown") => ReplyOutcome::StateUnknown,
                _ => ReplyOutcome::Sent,
            },
            Err(CallError::TimedOut { .. }) => ReplyOutcome::Unknown {
                why: "no reply".to_owned(),
            },
            Err(CallError::Unknown { .. }) => ReplyOutcome::Unknown {
                why: "connection dropped".to_owned(),
            },
            Err(error) => ReplyOutcome::Refused {
                reason: call_reason(&error, "prompt.submit"),
            },
        };
        SubmitAttempt {
            outcome,
            not_connected,
        }
    }
}

impl PeekService {
    /// Deny open approvals, then interrupt with the session's generation.
    pub async fn stop(&mut self) -> StopOutcome {
        let Some(link) = &self.link else {
            return StopOutcome::NotAttached {
                reason: "no session is open".to_owned(),
            };
        };
        let client = link.client.clone();
        let deny = self.timeouts.deny;
        let call = self.timeouts.call;
        let Some(session) = &mut self.session else {
            return StopOutcome::NotAttached {
                reason: "no session is open".to_owned(),
            };
        };
        if *session.state.turn() != TurnState::Running {
            return StopOutcome::NothingRunning;
        }
        let session_id = session.opened.session_id.clone();
        let notices = session
            .prompts
            .deny_before_interrupt(&client, &session.state, &session_id, deny)
            .await;
        self.notices.extend(notices);
        let Some(session) = &mut self.session else {
            return StopOutcome::NotAttached {
                reason: "no session is open".to_owned(),
            };
        };
        let effects = session::update(&mut session.state, Event::Interrupt);
        let Some((session_id, generation)) = effects.into_iter().find_map(|effect| match effect {
            Effect::Interrupt {
                session_id,
                execution_generation,
            } => Some((session_id, execution_generation)),
            _ => None,
        }) else {
            // The reducer already holds an interrupt for this turn.
            return StopOutcome::Stopping;
        };
        let result = client
            .call_raw(
                "session.interrupt",
                json!({"session_id": session_id.as_str(), "execution_generation": generation}),
                CallOptions {
                    timeout: Some(call),
                },
            )
            .await;
        let outcome = match result {
            Ok(_) => return StopOutcome::Stopping,
            Err(CallError::TimedOut { .. }) => StopOutcome::Unknown {
                why: "no reply".to_owned(),
            },
            Err(CallError::Unknown { .. }) => StopOutcome::Unknown {
                why: "connection dropped".to_owned(),
            },
            Err(error) => StopOutcome::Refused {
                reason: call_reason(&error, "session.interrupt"),
            },
        };
        if let Some(session) = &mut self.session {
            let _ = session::update(&mut session.state, Event::InterruptFailed);
        }
        outcome
    }

    /// Answer the open card. Returns the notices of a failed answer.
    pub async fn answer(&mut self, intent: CardAnswer) -> Vec<PromptNotice> {
        let Some(link) = &self.link else {
            return vec![PromptNotice {
                session_id: None,
                text: "could not answer: no session is open".to_owned(),
            }];
        };
        let client = link.client.clone();
        let Some(session) = &mut self.session else {
            return Vec::new();
        };
        let event = match intent {
            CardAnswer::Masked { request_id, value } => {
                return match session
                    .prompts
                    .answer_masked(&request_id, Zeroizing::new(value))
                    .await
                {
                    Ok(()) => {
                        self.report();
                        Vec::new()
                    }
                    Err(_) => vec![PromptNotice {
                        session_id: Some(session.opened.session_id.clone()),
                        text: "could not answer: the request is no longer open".to_owned(),
                    }],
                };
            }
            CardAnswer::Approve { key, choice, note } => {
                if let Some(note) = note {
                    session.prompts.set_note(&key, note);
                }
                Event::AnswerApproval { key, choice }
            }
            CardAnswer::Clarify { key, answer } => Event::AnswerClarify { key, answer },
        };
        let effects = session::update(&mut session.state, event);
        let mut notices = Vec::new();
        let mut follow = Vec::new();
        for effect in effects {
            let executed = session.prompts.execute(&client, effect).await;
            notices.extend(executed.notices);
            follow.extend(executed.events);
        }
        let mut notes = Vec::new();
        for event in follow {
            if let Event::Submit { text, .. } = &event {
                notes.push(text.clone());
                continue;
            }
            let _ = session::update(&mut session.state, event);
        }
        // A note typed beside an approval goes out as a reply after the answer.
        for text in notes {
            let _ = self.reply(text).await;
        }
        self.report();
        notices
    }

    /// Detach the attached session without an end reason; keep the link.
    pub async fn detach(&mut self) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        // Open masked requests are left for other viewers, never answered.
        let _ = session.prompts.on_connection_replaced(u64::MAX);
        if let Some(link) = &self.link {
            link.connection.link.untrack(&session.opened.session_id);
            let params = detach_params(&session.opened.session_id, &session.opened.subscription_id);
            let _ = link
                .client
                .call_raw(
                    "session.detach",
                    params,
                    CallOptions {
                        timeout: Some(self.timeouts.close),
                    },
                )
                .await;
        }
        self.fleet.observe_unsubscribed(session.key);
    }

    async fn close_link(&mut self) {
        if let Some(link) = self.link.take() {
            let _ = tokio::time::timeout(self.timeouts.close, link.connection.link.close()).await;
        }
    }

    /// Detach without an end reason and drop the link, within the close timeout.
    pub async fn close(&mut self) {
        let budget = self.timeouts.close;
        let _ = tokio::time::timeout(budget, self.detach()).await;
        // Past the budget the session is dropped anyway; the gateway ends the subscription
        // with the connection.
        if let Some(session) = self.session.take() {
            self.fleet.observe_unsubscribed(session.key);
        }
        self.close_link().await;
    }

    /// Forget every unconfirmed reply: the overlay that knew about them has closed.
    pub fn forget_unconfirmed(&mut self) {
        self.unconfirmed.clear();
    }
}

/// The wire name of a server request, for the masked kinds the peek can show.
fn request_method(request: &hermes_contract::server_requests::ServerRequest) -> &'static str {
    use hermes_contract::server_requests::ServerRequest as Request;
    match request {
        Request::Sudo(_) => "sudo",
        Request::Secret(_) => "secret",
        Request::VaultCode(_) => "vault.code",
        Request::VaultUnlockPrompt(_) => "vault.unlock_prompt",
        // Every other kind is ignored, whatever its name.
        _ => "",
    }
}

/// Keep an event once: a replayed one at or below the cursor is a repeat.
fn accept(cursor: &mut hermes_client::replay::Cursor, raw: &Value) -> bool {
    let epoch = raw["replay_epoch"].as_str();
    let seq = raw["seq"].as_u64();
    match (epoch, seq) {
        (Some(epoch), Some(seq)) => {
            if epoch == cursor.replay_epoch && seq <= cursor.last_sequence {
                return false;
            }
            if epoch != cursor.replay_epoch {
                cursor.replay_epoch = epoch.to_owned();
            }
            cursor.last_sequence = seq;
            true
        }
        _ => true,
    }
}

/// Poll a future once. The masked branch of the prompt service registers its card without
/// awaiting, so it is ready at the first poll.
fn run_ready<F: Future>(future: F) -> bool {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    matches!(future.as_mut().poll(&mut context), Poll::Ready(_))
}

/// True when the link reports an unreadable read soon after a call was lost.
async fn read_failed(events: &mut mpsc::Receiver<LinkEvent>) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
    while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
        if let LinkEvent::State(LinkState::Reconnecting { reason, .. }) = event {
            return reason == "read failed";
        }
    }
    false
}

/// `data.reason`, else `unknown method <method>` for -32601, else `error <code>`.
fn call_reason(error: &CallError, method: &str) -> String {
    match error {
        CallError::Rpc(rpc) => match (&rpc.reason, rpc.code) {
            (Some(reason), _) => reason.clone(),
            (None, -32601) => format!("unknown method {method}"),
            (None, code) => format!("error {code}"),
        },
        CallError::TimedOut { .. } => "no reply".to_owned(),
        CallError::Unknown { .. } => "connection dropped".to_owned(),
        CallError::NotConnected => "not connected".to_owned(),
        CallError::Encode | CallError::InvalidTimeout => "not sent".to_owned(),
        CallError::BadResult { .. } => "the reply did not decode".to_owned(),
    }
}

fn masked_summary(kind: MaskedKind) -> NeedSummary {
    match kind {
        MaskedKind::Sudo => NeedSummary::Sudo,
        MaskedKind::Secret => NeedSummary::Secret,
        MaskedKind::VaultCode => NeedSummary::VaultCode,
        MaskedKind::VaultUnlockPrompt => NeedSummary::VaultUnlock,
    }
}
