//! Heartbeat, reconnect backoff, and the link that owns one client.
//!
//! Instants are [`tokio::time::Instant`]. The first connection is not retried.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::Instant;

use crate::channel::{Client, CloseReason, Generation, Inbound, InboundEvent, InboundStream};
use crate::connect::{ConnectError, Connection, ConnectionParts, HandshakeError};
use crate::discovery::{DiscoveryError, ProfileLookupError};
use crate::replay::{
    Cursor, GapReason, Out, ReplayFailure, ReplayGap, ReplayToken, ReplayTracker, ReplayWindow,
    Stamp,
};
use crate::ticket::TicketError;
use crate::{CallError, CallOptions};
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::schemas::{CanonicalEventsResult, CanonicalSnapshot};

/// Half-jitter ladder. `floor` is the shortest delay and `cap` is the longest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backoff {
    /// Shortest delay. Production is 1 s. Scaled tests use 10 ms.
    pub floor: Duration,
    /// Longest delay. Production is 30 s. Scaled tests use 300 ms.
    pub cap: Duration,
}

impl Backoff {
    /// `min(cap, floor × 2^(attempt - 1))`. `attempt` starts at 1.
    pub fn ceiling(&self, attempt: u32) -> Duration {
        self.floor
            .saturating_mul(
                1u32.checked_shl(attempt.saturating_sub(1))
                    .unwrap_or(u32::MAX),
            )
            .min(self.cap)
    }

    /// Uniform point in `[max(floor, ceiling / 2), ceiling]` selected by `draw`.
    ///
    /// `draw` is in `[0, 1]`. `0` is the bottom of the band and `1` is the ceiling.
    pub fn delay(&self, attempt: u32, draw: f64) -> Duration {
        let ceiling = self.ceiling(attempt);
        let bottom = self.floor.max(ceiling / 2).min(ceiling);
        let draw = if draw.is_finite() {
            draw.clamp(0.0, 1.0)
        } else {
            0.0
        };
        bottom + (ceiling - bottom).mul_f64(draw)
    }
}

/// How a reconnect delay picks its place in the half-open band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Jitter {
    /// Draw with `fastrand` when a delay is computed.
    Random,
    /// Tests pass a fixed draw in `[0, 1]`.
    Fixed(f64),
}

/// What one heartbeat tick decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Send `gateway.ping` and remember this instant.
    SendPing,
    /// The link has been silent long enough to reconnect.
    Dead,
}

/// One connection's ping clock. A new connection builds a new clock.
pub struct HeartbeatClock {
    interval: Duration,
    deadline: Duration,
    connected_at: Instant,
    last_frame: Instant,
    last_tick: Instant,
    last_ping: Option<Instant>,
}

impl HeartbeatClock {
    /// Store the interval, the deadline, and the instant the handshake finished.
    pub fn new(interval: Duration, deadline: Duration, connected_at: Instant) -> Self {
        Self {
            interval,
            deadline,
            connected_at,
            last_frame: connected_at,
            last_tick: connected_at,
            last_ping: None,
        }
    }

    /// Remember the latest sign of life. An older instant is ignored.
    pub fn on_inbound(&mut self, at: Instant) {
        self.last_frame = self.last_frame.max(at);
    }

    /// Decide this tick. A tick later than two intervals only sends a ping.
    pub fn on_tick(&mut self, now: Instant) -> Tick {
        let late = now.saturating_duration_since(self.last_tick) > self.interval.saturating_mul(2);
        self.last_tick = now;
        if !late
            && self.last_ping.is_some_and(|ping| ping > self.last_frame)
            && now.saturating_duration_since(self.last_frame.max(self.connected_at))
                >= self.deadline
        {
            return Tick::Dead;
        }
        self.last_ping = Some(now);
        Tick::SendPing
    }
}

/// Timings and bounds for one link.
#[derive(Clone, Debug)]
pub struct LinkConfig {
    /// How often a heartbeat ping is sent. Production is 15 s.
    pub ping_interval: Duration,
    /// Silence after a ping before the link is dead. Production is 45 s.
    pub ping_deadline: Duration,
    /// How long a connection must stay up before the backoff counter resets.
    pub stable_open: Duration,
    /// Budget for `session.resume` and for `session.events.since`.
    pub replay_timeout: Duration,
    /// Budget for one dial, including ticket, upgrade, and handshake.
    pub dial_timeout: Duration,
    /// Reconnect ladder.
    pub backoff: Backoff,
    /// Where in the half-open band a delay lands.
    pub jitter: Jitter,
    /// Frames held per session during replay.
    pub hold_limit: usize,
    /// Capacity of the event channel returned by [`Link::start`].
    pub event_buffer: usize,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(15),
            ping_deadline: Duration::from_secs(45),
            stable_open: Duration::from_secs(5),
            replay_timeout: Duration::from_secs(10),
            dial_timeout: Duration::from_secs(30),
            backoff: Backoff {
                floor: Duration::from_secs(1),
                cap: Duration::from_secs(30),
            },
            jitter: Jitter::Random,
            hold_limit: 10_000,
            event_buffer: 1_024,
        }
    }
}

impl LinkConfig {
    /// Production durations divided by 100, with [`Jitter::Fixed`]`(1.0)`.
    pub fn scaled_for_tests() -> Self {
        let base = Self::default();
        Self {
            ping_interval: base.ping_interval / 100,
            ping_deadline: base.ping_deadline / 100,
            stable_open: base.stable_open / 100,
            replay_timeout: base.replay_timeout / 100,
            dial_timeout: base.dial_timeout / 100,
            backoff: Backoff {
                floor: base.backoff.floor / 100,
                cap: base.backoff.cap / 100,
            },
            jitter: Jitter::Fixed(1.0),
            hold_limit: base.hold_limit,
            event_buffer: base.event_buffer,
        }
    }
}

/// A dial failed. `Retry` waits and tries again. `GiveUp` ends the link.
#[derive(Clone, PartialEq, Eq)]
pub enum DialFailure {
    /// Try again on the ladder.
    Retry {
        /// What a person can read. No ticket, path, or unexpected profile name.
        reason: String,
        /// What to do next.
        next_step: String,
    },
    /// Stop. The profile is gone, the gateway is incompatible, or auth was refused.
    GiveUp {
        /// What a person can read. No ticket, path, or unexpected profile name.
        reason: String,
        /// What to do next.
        next_step: String,
    },
}

impl DialFailure {
    /// The sentence stored on this failure.
    pub fn reason(&self) -> &str {
        match self {
            Self::Retry { reason, .. } | Self::GiveUp { reason, .. } => reason,
        }
    }

    /// The next step stored on this failure.
    pub fn next_step(&self) -> &str {
        match self {
            Self::Retry { next_step, .. } | Self::GiveUp { next_step, .. } => next_step,
        }
    }
}

impl std::fmt::Debug for DialFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry { reason, .. } => formatter
                .debug_struct("Retry")
                .field("reason", reason)
                .finish(),
            Self::GiveUp { reason, .. } => formatter
                .debug_struct("GiveUp")
                .field("reason", reason)
                .finish(),
        }
    }
}

impl From<DiscoveryError> for DialFailure {
    fn from(error: DiscoveryError) -> Self {
        let fatal = matches!(
            error,
            DiscoveryError::HomeNotServed
                | DiscoveryError::NamedProfileHome
                | DiscoveryError::Incompatible
                | DiscoveryError::InvalidState
                | DiscoveryError::InvalidApiOrigin
                | DiscoveryError::InvalidSupervisor
                | DiscoveryError::InvalidIdentity
                | DiscoveryError::BadPointer(_)
                | DiscoveryError::UnsafeNode { .. }
        );
        failure(fatal, error.to_string(), error.next_step().to_owned())
    }
}

impl From<ProfileLookupError> for DialFailure {
    fn from(error: ProfileLookupError) -> Self {
        let (ProfileLookupError::Parked { name, .. }
        | ProfileLookupError::NeverDiscovered { name }) = &error;
        let mut reason = error.to_string();
        let mut next_step = error.next_step().to_owned();
        if !matches!(name.as_str(), "default" | "fixture") && !name.is_empty() {
            reason = reason.replace(name, "<profile>");
            next_step = next_step.replace(name, "<profile>");
        }
        failure(true, reason, next_step)
    }
}

impl From<TicketError> for DialFailure {
    fn from(error: TicketError) -> Self {
        let fatal = matches!(
            error,
            TicketError::InvalidRequest
                | TicketError::ScopeMismatch
                | TicketError::Malformed
                | TicketError::NotAuthenticated
                | TicketError::BindingRejected
                | TicketError::UnsafeNode { .. }
        );
        failure(fatal, error.to_string(), error.next_step().to_owned())
    }
}

impl From<ConnectError> for DialFailure {
    fn from(error: ConnectError) -> Self {
        let fatal = matches!(
            error,
            ConnectError::SubprotocolMismatch
                | ConnectError::TlsNotSupported
                | ConnectError::WrongPurpose
        );
        failure(fatal, error.to_string(), error.next_step().to_owned())
    }
}

impl From<HandshakeError> for DialFailure {
    fn from(error: HandshakeError) -> Self {
        let fatal = matches!(
            error,
            HandshakeError::NotSessionAuthority
                | HandshakeError::BadReady
                | HandshakeError::BadDescribe
                | HandshakeError::BadFrame
        );
        failure(fatal, error.to_string(), error.next_step().to_owned())
    }
}

/// One fresh attach. `generation` is the generation the connection will receive.
pub trait Dial: Send + 'static {
    /// Discover, mint a ticket, upgrade, and finish the handshake once.
    fn dial(&mut self, generation: u64)
    -> impl Future<Output = Result<Dialed, DialFailure>> + Send;
}

/// What a successful dial hands the link.
pub struct Dialed {
    /// The open socket, after the handshake.
    pub parts: ConnectionParts,
    /// `gateway.ready` `replay_epoch` for this process.
    pub process_epoch: String,
    /// True only when `gateway.ready` advertised `heartbeat: true`.
    pub heartbeat: bool,
    /// `runtime.describe` capabilities.
    pub capabilities: Vec<String>,
}

/// The production dial. It runs the attach client and never prints a ticket.
pub struct GatewayDial {
    home: PathBuf,
    profile: String,
    hook: Option<ConnectHook>,
    origin: Option<String>,
}

/// Runs on each newly opened connection before its handshake.
pub type ConnectHook = Arc<dyn Fn(&mut Connection, u64) + Send + Sync>;

impl GatewayDial {
    /// Dial `profile` in `home` through the gateway's own API origin.
    pub fn new(home: PathBuf, profile: String) -> Self {
        Self {
            home,
            profile,
            hook: None,
            origin: None,
        }
    }

    /// Run `hook` on the open socket, with its generation, before the handshake.
    pub fn on_connect(mut self, hook: ConnectHook) -> Self {
        self.hook = Some(hook);
        self
    }

    /// Dial the fault relay's origin instead of the origin discovery reported.
    #[doc(hidden)]
    pub fn via_origin(mut self, origin: String) -> Self {
        self.origin = Some(origin);
        self
    }
}

impl std::fmt::Debug for GatewayDial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (&self.home, &self.profile, &self.hook, &self.origin);
        formatter.write_str("GatewayDial { .. }")
    }
}

impl Dial for GatewayDial {
    async fn dial(&mut self, generation: u64) -> Result<Dialed, DialFailure> {
        let endpoint = crate::discover_with(
            &self.home,
            &crate::DiscoverOptions {
                ensure: crate::EnsurePolicy::Never,
                ..crate::DiscoverOptions::default()
            },
        )
        .await?;
        let endpoint = match self.origin.as_deref() {
            Some(origin) => endpoint.with_api_origin(origin)?,
            None => endpoint,
        };
        let profile = endpoint.resolve_profile(&self.profile)?;
        let ticket = crate::mint_ticket(
            &self.home,
            &endpoint,
            &profile,
            crate::Purpose::Interactive,
            crate::Scope::profile(),
        )
        .await?;
        let mut connection = crate::connect(&endpoint, ticket).await?;
        if let Some(hook) = &self.hook {
            hook(&mut connection, generation);
        }
        let handshake = crate::handshake(&mut connection).await?;
        Ok(Dialed {
            parts: connection.into_parts(),
            process_epoch: handshake.ready.replay_epoch,
            heartbeat: handshake.ready.heartbeat == Some(true),
            capabilities: handshake.describe.capabilities,
        })
    }
}

/// What the consumer sees the link doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// A handshake finished. The first generation is 1.
    Connected {
        /// Channel generation of the open socket.
        generation: u64,
    },
    /// Waiting to dial again.
    Reconnecting {
        /// Attempt since the last stable connection. The first reconnect is 1.
        attempt: u32,
        /// How long this wait is.
        retry_in: Duration,
        /// Why the previous connection ended, or why the last dial failed.
        reason: String,
    },
    /// A dial returned [`DialFailure::GiveUp`].
    Failed {
        /// Why the link stopped.
        reason: String,
        /// What to do next.
        next_step: String,
    },
    /// `close` finished.
    Closed,
}

/// One item on the link's event channel.
#[allow(clippy::large_enum_variant)]
pub enum LinkEvent {
    /// The link changed state.
    State(LinkState),
    /// One inbound item. `replayed` is true for events taken from a replay window.
    Inbound {
        /// Channel item, including its generation and raw params.
        item: Inbound,
        /// True when a replay window produced this item.
        replayed: bool,
    },
    /// One session cannot be proven complete.
    Gap(ReplayGap),
    /// `session.resume` re-attached this session on the current connection.
    Reattached {
        /// Session that was resumed.
        session_id: String,
        /// Subscription the new connection received.
        subscription_id: String,
    },
    /// Resume or replay said `not_found`. The link has forgotten the session.
    SessionGone {
        /// Session that is gone.
        session_id: String,
    },
}

/// Counters for the life of one link.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkStats {
    /// Connections opened after the first.
    pub reconnects: u64,
    /// Events delivered from a replay window.
    pub replayed_events: u64,
    /// Tracked frames dropped because their sequence was already delivered.
    pub duplicates_dropped: u64,
    /// Gaps raised, by reason.
    pub gaps: BTreeMap<GapReason, u64>,
}

/// Handle to the link task. Dropping it does not close the link.
pub struct Link {
    client: Client,
    commands: mpsc::UnboundedSender<LinkCommand>,
    stop: watch::Sender<bool>,
    stats: Arc<Mutex<LinkStats>>,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Link { .. }")
    }
}

impl Link {
    /// Dial once. The first event is `State(Connected { generation: 1 })`.
    ///
    /// A failed first dial is returned. It is not retried.
    pub async fn start(
        mut dial: impl Dial,
        cfg: LinkConfig,
    ) -> Result<(Self, mpsc::Receiver<LinkEvent>), DialFailure> {
        let first = tokio::time::timeout(cfg.dial_timeout, dial.dial(1))
            .await
            .unwrap_or_else(|_| Err(dial_timeout()))?;
        let (client, inbound) = Client::start(first.parts);
        let (commands, receiver) = mpsc::unbounded_channel();
        let (events, rx) = mpsc::channel(cfg.event_buffer.max(1));
        let (stop, cancel) = watch::channel(false);
        let stats = Arc::new(Mutex::new(LinkStats::default()));
        let now = Instant::now();
        let mut tracker = ReplayTracker::new(cfg.hold_limit);
        tracker.on_new_connection(&first.process_epoch);
        let mut ticks = tokio::time::interval_at(now + cfg.ping_interval, cfg.ping_interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let task = LinkTask {
            client: client.clone(),
            inbound,
            commands: receiver,
            events,
            cancel,
            _keep_stop: stop.clone(),
            stats: stats.clone(),
            tracker,
            cfg,
            generation: Generation(1),
            connected: true,
            connected_at: now,
            heartbeat: first.heartbeat,
            clock: HeartbeatClock::new(Duration::ZERO, Duration::ZERO, now),
            ticks,
            attempt: 0,
            retry_at: None,
            replay: JoinSet::new(),
        };
        tokio::spawn(task.run(dial));
        Ok((
            Self {
                client,
                commands,
                stop,
                stats,
            },
            rx,
        ))
    }

    /// The one client. Clones stay valid across reconnects.
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Track `session_id` at `cursor`. Returns before the task applies it.
    pub fn track(&self, session_id: &str, cursor: Cursor) {
        let _ = self
            .commands
            .send(LinkCommand::Track(session_id.to_owned(), cursor));
    }

    /// Stop tracking `session_id`. Sends nothing to the gateway.
    pub fn untrack(&self, session_id: &str) {
        let _ = self
            .commands
            .send(LinkCommand::Untrack(session_id.to_owned()));
    }

    /// Install `cursor` after the consumer has rebuilt the session.
    pub fn resync(&self, session_id: &str, cursor: Cursor) {
        let _ = self
            .commands
            .send(LinkCommand::Resync(session_id.to_owned(), cursor));
    }

    /// A snapshot of the counters.
    pub fn stats(&self) -> LinkStats {
        self.stats.lock().expect("link stats poisoned").clone()
    }

    /// Stop the link. Emits `State(Closed)` and then ends.
    pub async fn close(self) {
        let _ = self.stop.send(true);
        self.client.close().await;
    }
}

fn failure(fatal: bool, reason: String, next_step: String) -> DialFailure {
    if fatal {
        DialFailure::GiveUp { reason, next_step }
    } else {
        DialFailure::Retry { reason, next_step }
    }
}

fn dial_timeout() -> DialFailure {
    failure(
        false,
        "dial timed out".into(),
        "Check the gateway with `hermes gateway status`.".into(),
    )
}

enum LinkCommand {
    Track(String, Cursor),
    Untrack(String),
    Resync(String, Cursor),
}

enum ReplayReply {
    PingDone,
    Attached {
        id: String,
        cursor: Cursor,
        token: ReplayToken,
        result: Result<Box<CanonicalSnapshot>, CallError>,
    },
    Window {
        id: String,
        token: ReplayToken,
        result: Result<(CanonicalEventsResult, serde_json::Value), CallError>,
    },
}

struct LinkTask {
    client: Client,
    inbound: InboundStream,
    commands: mpsc::UnboundedReceiver<LinkCommand>,
    events: mpsc::Sender<LinkEvent>,
    cancel: watch::Receiver<bool>,
    // Dropping the public handle deliberately leaves the link running.
    _keep_stop: watch::Sender<bool>,
    stats: Arc<Mutex<LinkStats>>,
    tracker: ReplayTracker<Inbound>,
    cfg: LinkConfig,
    generation: Generation,
    connected: bool,
    connected_at: Instant,
    heartbeat: bool,
    clock: HeartbeatClock,
    ticks: tokio::time::Interval,
    attempt: u32,
    retry_at: Option<Instant>,
    replay: JoinSet<ReplayReply>,
}

impl LinkTask {
    async fn emit(&mut self, event: LinkEvent) {
        if *self.cancel.borrow() {
            return;
        }
        tokio::select! {
            biased;
            _ = self.cancel.changed() => {},
            _ = self.events.send(event) => {},
        }
    }

    async fn outputs(&mut self, outputs: Vec<Out<Inbound>>) {
        self.stats
            .lock()
            .expect("link stats poisoned")
            .duplicates_dropped = self.tracker.duplicates_dropped();
        for output in outputs {
            let event = match output {
                Out::Deliver { item, replayed } => {
                    if replayed {
                        self.stats
                            .lock()
                            .expect("link stats poisoned")
                            .replayed_events += 1;
                    }
                    LinkEvent::Inbound { item, replayed }
                }
                Out::Gap(gap) => {
                    *self
                        .stats
                        .lock()
                        .expect("link stats poisoned")
                        .gaps
                        .entry(gap.reason)
                        .or_default() += 1;
                    LinkEvent::Gap(gap)
                }
                Out::Gone(session_id) => LinkEvent::SessionGone { session_id },
            };
            self.emit(event).await;
            if *self.cancel.borrow() {
                break;
            }
        }
    }

    fn restart_clock(&mut self) {
        self.connected_at = Instant::now();
        self.clock = HeartbeatClock::new(
            self.cfg.ping_interval,
            self.cfg.ping_deadline,
            self.connected_at,
        );
        self.ticks
            .reset_at(self.connected_at + self.cfg.ping_interval);
    }

    async fn retry(&mut self, reason: String) {
        self.attempt = self.attempt.saturating_add(1);
        let draw = match self.cfg.jitter {
            Jitter::Random => fastrand::f64(),
            Jitter::Fixed(draw) => draw,
        };
        let delay = self.cfg.backoff.delay(self.attempt, draw);
        self.emit(LinkEvent::State(LinkState::Reconnecting {
            attempt: self.attempt,
            retry_in: delay,
            reason,
        }))
        .await;
        self.retry_at = Some(Instant::now() + delay);
    }

    async fn lost(&mut self, reason: &str) {
        let stats = self.client.stats().await;
        let ended_at = if stats.generation == self.generation {
            stats
                .closed_at
                .map(Instant::from_std)
                .unwrap_or_else(Instant::now)
        } else {
            Instant::now()
        };
        if ended_at.saturating_duration_since(self.connected_at) >= self.cfg.stable_open {
            self.attempt = 0;
        }
        self.connected = false;
        self.replay.abort_all();
        self.tracker.on_connection_lost();
        self.retry(reason.to_owned()).await;
    }

    fn begin_replay(&mut self) {
        for (id, cursor, token) in self.tracker.begin_replay() {
            let client = self.client.clone();
            let timeout = self.cfg.replay_timeout;
            self.replay.spawn(async move {
                let result = resume_session(&client, &id, timeout).await.map(Box::new);
                ReplayReply::Attached {
                    id,
                    cursor,
                    token,
                    result,
                }
            });
        }
    }

    async fn replay_reply(&mut self, reply: ReplayReply) {
        if !matches!(reply, ReplayReply::PingDone) {
            // The channel can fail a pending call before the link consumes Closed.
            // Such an Unknown belongs to a lost replay, not a snapshot incident.
            let stats = self.client.stats().await;
            if !self.connected || stats.generation != self.generation || stats.closed_at.is_some() {
                return;
            }
        }
        match reply {
            ReplayReply::PingDone => {}
            ReplayReply::Attached {
                id,
                cursor,
                token,
                result,
            } => {
                if !self.tracker.token_is_current(&id, token) {
                    return;
                }
                match result {
                    Ok(snapshot) if snapshot.session_id == id => {
                        let outs = self
                            .tracker
                            .on_reattached(&id, token, &snapshot.replay_epoch);
                        self.outputs(outs).await;
                        if !self.tracker.token_is_current(&id, token) {
                            return;
                        }
                        self.emit(LinkEvent::Reattached {
                            session_id: id.clone(),
                            subscription_id: snapshot.subscription_id,
                        })
                        .await;
                        let Ok(last_sequence) = cursor.last_sequence.try_into() else {
                            let outs =
                                self.tracker
                                    .on_replay_failed(&id, token, ReplayFailure::Other);
                            self.outputs(outs).await;
                            return;
                        };
                        let client = self.client.clone();
                        let timeout = self.cfg.replay_timeout;
                        self.replay.spawn(async move {
                            let result = replay_events(
                                &client,
                                &id,
                                last_sequence,
                                &cursor.replay_epoch,
                                timeout,
                            )
                            .await;
                            ReplayReply::Window { id, token, result }
                        });
                    }
                    result => {
                        let why = replay_failure(result.err().as_ref());
                        let outs = self.tracker.on_replay_failed(&id, token, why);
                        self.outputs(outs).await;
                    }
                }
            }
            ReplayReply::Window { id, token, result } => {
                if !self.tracker.token_is_current(&id, token) {
                    return;
                }
                let window = result
                    .map_err(|err| replay_failure(Some(&err)))
                    .and_then(|(result, raw)| decode_window(result, raw, self.generation, &id));
                let outs = match window {
                    Ok(window) => self.tracker.on_replay_window(&id, token, window),
                    Err(why) => self.tracker.on_replay_failed(&id, token, why),
                };
                self.outputs(outs).await;
            }
        }
    }

    async fn run<D: Dial>(mut self, dial: D) {
        type DialFuture<D> = Pin<Box<dyn Future<Output = (D, Result<Dialed, DialFailure>)> + Send>>;
        let mut dial = Some(dial);
        let mut dialing: Option<DialFuture<D>> = None;
        let mut commands_open = true;
        self.restart_clock();
        self.emit(LinkEvent::State(LinkState::Connected { generation: 1 }))
            .await;
        while !*self.cancel.borrow() && !self.events.is_closed() {
            tokio::select! {
                _ = self.cancel.changed() => break,
                _ = self.events.closed() => break,
                command = self.commands.recv(), if commands_open => match command {
                    Some(LinkCommand::Track(id, cursor)) => self.tracker.track(&id, cursor),
                    Some(LinkCommand::Untrack(id)) => self.tracker.untrack(&id),
                    Some(LinkCommand::Resync(id, cursor)) => {
                        let outs = self.tracker.resync(&id, cursor);
                        self.outputs(outs).await;
                    }
                    None => commands_open = false,
                },
                item = self.inbound.next() => match item {
                    Some(Inbound::Closed(closed)) => {
                        if self.connected && closed.generation == self.generation {
                            let reason = match closed.reason {
                                CloseReason::RemoteClosed { .. } => Some("socket closed"),
                                CloseReason::Error(_) => Some("read failed"),
                                CloseReason::Overloaded => Some("gateway closed a slow reader"),
                                CloseReason::Replaced | CloseReason::ClientClosed => None,
                            };
                            if let Some(reason) = reason { self.lost(reason).await; }
                        }
                    }
                    Some(item) if self.connected => {
                        let generation = match &item {
                            Inbound::Event(event) => event.generation,
                            Inbound::ServerRequest(request) => request.generation,
                            Inbound::Malformed(frame) => frame.generation,
                            Inbound::Closed(_) => unreachable!("closed handled above"),
                        };
                        if generation != self.generation { continue; }
                        let stamp = match &item {
                            Inbound::Event(event) => Stamp {
                                session_id: event.frame.session_id.as_deref(), seq: event.frame.seq,
                                replay_epoch: event.frame.replay_epoch.as_deref(),
                                server_gap: matches!(event.frame.notification, Notification::SessionReplayGap(_)),
                            },
                            _ => Stamp { session_id: None, seq: None, replay_epoch: None, server_gap: false },
                        };
                        // The stamp borrows the event; own the small metadata before moving it.
                        let id = stamp.session_id.map(str::to_owned);
                        let epoch = stamp.replay_epoch.map(str::to_owned);
                        let stamp = Stamp { session_id: id.as_deref(), replay_epoch: epoch.as_deref(), ..stamp };
                        let gap_hint = match &item {
                            Inbound::Event(event) if stamp.server_gap => Some((
                                event.raw["payload"]["latest_seq"].as_u64(),
                                event.raw["payload"]["replay_epoch"].as_str().map(str::to_owned),
                            )),
                            _ => None,
                        };
                        let mut outs = self.tracker.on_live(stamp, item);
                        if let Some((seq, epoch)) = gap_hint {
                            for out in &mut outs {
                                if let Out::Gap(gap) = out {
                                    gap.latest_seq = seq;
                                    gap.replay_epoch = epoch.clone();
                                }
                            }
                        }
                        self.outputs(outs).await;
                    }
                    Some(_) => {},
                    None => break,
                },
                _ = self.ticks.tick(), if self.connected && self.heartbeat => {
                    let stats = self.client.stats().await;
                    if stats.generation == self.generation && let Some(at) = stats.last_frame_at {
                        self.clock.on_inbound(Instant::from_std(at));
                    }
                    match self.clock.on_tick(Instant::now()) {
                        Tick::Dead => self.lost("heartbeat timeout").await,
                        Tick::SendPing => {
                            let client = self.client.clone();
                            let timeout = self.cfg.ping_interval;
                            // Abort heartbeat calls along with this connection's replay work.
                            self.replay.spawn(async move {
                                let _ = client.call_raw("gateway.ping", serde_json::json!({}), CallOptions { timeout: Some(timeout) }).await;
                                ReplayReply::PingDone
                            });
                        }
                    }
                },
                _ = async { tokio::time::sleep_until(self.retry_at.expect("retry scheduled")).await }, if self.retry_at.is_some() => {
                    self.retry_at = None;
                    let mut next = dial.take().expect("one dial at a time");
                    let generation = self.generation.0 + 1;
                    let timeout = self.cfg.dial_timeout;
                    dialing = Some(Box::pin(async move {
                        let result = tokio::time::timeout(timeout, next.dial(generation)).await.unwrap_or_else(|_| Err(dial_timeout()));
                        (next, result)
                    }));
                },
                (next, result) = async { dialing.as_mut().expect("dialing").await }, if dialing.is_some() => {
                    dialing = None;
                    dial = Some(next);
                    match result {
                        Ok(open) => {
                            let Ok(generation) = self.client.replace_connection(open.parts).await else { break; };
                            self.generation = generation;
                            self.connected = true;
                            self.heartbeat = open.heartbeat;
                            self.restart_clock();
                            self.stats.lock().expect("link stats poisoned").reconnects += 1;
                            self.emit(LinkEvent::State(LinkState::Connected { generation: generation.0 })).await;
                            let outs = self.tracker.on_new_connection(&open.process_epoch);
                            let restarted = outs.iter().any(|out| matches!(out, Out::Gap(gap) if gap.reason == GapReason::BackendRestarted));
                            self.outputs(outs).await;
                            if !restarted {
                                if open.capabilities.iter().any(|cap| cap == "event-replay-v1") { self.begin_replay(); }
                                else {
                                    let outs = self.tracker.on_replay_unsupported();
                                    self.outputs(outs).await;
                                }
                            }
                        }
                        Err(DialFailure::Retry { reason, .. }) => self.retry(reason).await,
                        Err(DialFailure::GiveUp { reason, next_step }) => {
                            self.client.close().await;
                            self.emit(LinkEvent::State(LinkState::Failed { reason, next_step })).await;
                            if !*self.cancel.borrow() {
                                tokio::select! {
                                    _ = self.cancel.changed() => {},
                                    _ = self.events.closed() => {},
                                }
                            }
                            break;
                        },
                    }
                },
                Some(reply) = self.replay.join_next(), if !self.replay.is_empty() => {
                    if let Ok(reply) = reply { self.replay_reply(reply).await; }
                },
            }
        }
        // Dropping the dial future and JoinSet cancels work, including a hung upgrade.
        drop(dialing);
        self.replay.abort_all();
        self.client.close().await;
        let _ = self.events.send(LinkEvent::State(LinkState::Closed)).await;
    }
}

async fn resume_session(
    client: &Client,
    id: &str,
    timeout: Duration,
) -> Result<CanonicalSnapshot, CallError> {
    let raw = client
        .call_raw(
            "session.resume",
            serde_json::json!({"session_id": id}),
            CallOptions {
                timeout: Some(timeout),
            },
        )
        .await?;
    serde_json::from_value(raw).map_err(|_| CallError::BadResult {
        method: "session.resume".into(),
        reason: "result did not decode",
    })
}

async fn replay_events(
    client: &Client,
    id: &str,
    last_sequence: i64,
    replay_epoch: &str,
    timeout: Duration,
) -> Result<(CanonicalEventsResult, serde_json::Value), CallError> {
    let raw = client
        .call_raw(
            "session.events.since",
            serde_json::json!({
                "session_id": id,
                "last_sequence": last_sequence,
                "replay_epoch": replay_epoch,
            }),
            CallOptions {
                timeout: Some(timeout),
            },
        )
        .await?;
    let result = serde_json::from_value(raw.clone()).map_err(|_| CallError::BadResult {
        method: "session.events.since".into(),
        reason: "result did not decode",
    })?;
    Ok((result, raw))
}

fn replay_failure(error: Option<&CallError>) -> ReplayFailure {
    if matches!(error, Some(CallError::Rpc(error)) if error.reason.as_deref() == Some("not_found"))
    {
        ReplayFailure::NotFound
    } else {
        ReplayFailure::Other
    }
}

fn decode_window(
    result: CanonicalEventsResult,
    raw: serde_json::Value,
    generation: Generation,
    session_id: &str,
) -> Result<ReplayWindow<Inbound>, ReplayFailure> {
    if result.snapshot_required || result.truncated {
        return Ok(ReplayWindow::SnapshotRequired {
            latest_seq: result
                .latest_seq
                .try_into()
                .map_err(|_| ReplayFailure::Other)?,
        });
    }
    let raw_events = raw
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or(ReplayFailure::Other)?;
    let events = raw_events
        .iter()
        .cloned()
        .map(|raw| {
            let frame = EventFrame::from_params(&raw).map_err(|_| ReplayFailure::Other)?;
            if frame.session_id.as_deref() != Some(session_id) {
                return Err(ReplayFailure::Other);
            }
            let seq = frame.seq.ok_or(ReplayFailure::Other)?;
            let epoch = frame.replay_epoch.clone().ok_or(ReplayFailure::Other)?;
            Ok((
                seq,
                epoch,
                Inbound::Event(InboundEvent {
                    generation,
                    frame,
                    raw,
                }),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReplayWindow::Events(events))
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;
    use crate::connect::Closed;
    use crate::discovery::{EnsureError, NodeKind, NodeProblem, PointerProblem, ReportedState};

    #[test]
    fn replay_raw_preserves_extension_fields_and_nulls() {
        let event = serde_json::json!({
            "type": "message.delta", "session_id": "s", "seq": 5,
            "replay_epoch": "e", "payload": {"text": "hello", "future_payload": 7},
            "future_envelope": {"value": null}, "admission_id": null,
        });
        let raw = serde_json::json!({
            "events": [event.clone()], "latest_seq": 5, "last_sequence": 5,
            "epoch": "e", "replay_epoch": "e", "truncated": false,
            "snapshot_required": false, "count": 1,
        });
        let typed = serde_json::from_value(raw.clone()).expect("canonical replay result");
        let ReplayWindow::Events(mut events) =
            decode_window(typed, raw, Generation(3), "s").unwrap()
        else {
            panic!("expected events");
        };
        let (_, _, Inbound::Event(inbound)) = events.pop().expect("one event") else {
            panic!("expected inbound event");
        };
        assert_eq!(inbound.generation, Generation(3));
        assert_eq!(inbound.raw, event);
    }

    #[test]
    fn replay_decode_rejects_entire_window_on_invalid_or_foreign_event() {
        for bad in [
            serde_json::json!({"type": "message.delta", "session_id": "s", "seq": 6, "replay_epoch": "e", "payload": {}}),
            serde_json::json!({"type": "message.delta", "session_id": "other", "seq": 6, "replay_epoch": "e", "payload": {"text": "wrong session"}}),
        ] {
            let raw = serde_json::json!({
                "events": [{"type": "message.delta", "session_id": "s", "seq": 5, "replay_epoch": "e", "payload": {"text": "ok"}}, bad],
                "latest_seq": 6, "last_sequence": 6, "epoch": "e", "replay_epoch": "e",
                "truncated": false, "snapshot_required": false, "count": 2,
            });
            let typed = serde_json::from_value(raw.clone()).expect("canonical replay envelope");
            assert!(matches!(
                decode_window(typed, raw, Generation(1), "s"),
                Err(ReplayFailure::Other)
            ));
        }
    }

    fn production() -> Backoff {
        Backoff {
            floor: Duration::from_secs(1),
            cap: Duration::from_secs(30),
        }
    }

    #[test]
    fn backoff_ladder_top_and_bottom() {
        let backoff = production();
        let top = [1, 2, 4, 8, 16, 30, 30];
        for (index, seconds) in top.iter().enumerate() {
            let attempt = (index as u32) + 1;
            assert_eq!(backoff.delay(attempt, 1.0), Duration::from_secs(*seconds));
        }
        let bottom = [1, 1, 2, 4, 8, 15, 15];
        for (index, seconds) in bottom.iter().enumerate() {
            let attempt = (index as u32) + 1;
            assert_eq!(backoff.delay(attempt, 0.0), Duration::from_secs(*seconds));
        }
        let huge = backoff.delay(1_000, 1.0);
        assert!(huge <= Duration::from_secs(30));
        assert!(huge >= Duration::from_secs(1));
    }

    proptest::proptest! {
        #[test]
        fn backoff_delay_within_floor_and_cap(attempt in 1u32..64, draw in 0.0f64..=1.0) {
            let backoff = production();
            let delay = backoff.delay(attempt, draw);
            let ceiling = backoff.ceiling(attempt);
            proptest::prop_assert!(delay >= Duration::from_secs(1));
            proptest::prop_assert!(delay <= ceiling);
            proptest::prop_assert!(ceiling <= Duration::from_secs(30));
            proptest::prop_assert!(ceiling >= Duration::from_secs(1));
        }
    }

    fn clock(t0: Instant) -> HeartbeatClock {
        HeartbeatClock::new(Duration::from_secs(15), Duration::from_secs(45), t0)
    }

    #[test]
    fn pings_every_interval() {
        let t0 = Instant::now();
        let mut clock = clock(t0);
        for (frame, tick) in [(5, 15), (20, 30), (35, 45)] {
            clock.on_inbound(t0 + Duration::from_secs(frame));
            assert_eq!(
                clock.on_tick(t0 + Duration::from_secs(tick)),
                Tick::SendPing
            );
        }
    }

    #[test]
    fn streaming_without_pongs_stays_alive() {
        let t0 = Instant::now();
        let mut clock = clock(t0);
        let mut frame = 5u64;
        for tick in (15..=600).step_by(15) {
            while frame <= tick && frame <= 600 {
                clock.on_inbound(t0 + Duration::from_secs(frame));
                frame += 5;
            }
            assert_eq!(
                clock.on_tick(t0 + Duration::from_secs(tick)),
                Tick::SendPing,
                "tick at {tick}s"
            );
        }
    }

    #[test]
    fn silence_is_dead_between_45_and_60_seconds() {
        let t0 = Instant::now();
        let mut quiet = clock(t0);
        assert_eq!(quiet.on_tick(t0 + Duration::from_secs(15)), Tick::SendPing);
        assert_eq!(quiet.on_tick(t0 + Duration::from_secs(30)), Tick::SendPing);
        assert_eq!(quiet.on_tick(t0 + Duration::from_secs(45)), Tick::Dead);

        let mut paced = clock(t0);
        paced.on_inbound(t0 + Duration::from_secs(1));
        assert_eq!(paced.on_tick(t0 + Duration::from_secs(15)), Tick::SendPing);
        assert_eq!(paced.on_tick(t0 + Duration::from_secs(30)), Tick::SendPing);
        assert_eq!(paced.on_tick(t0 + Duration::from_secs(45)), Tick::SendPing);
        assert_eq!(paced.on_tick(t0 + Duration::from_secs(60)), Tick::Dead);
    }

    #[test]
    fn late_tick_after_sleep_does_not_kill_a_live_link() {
        let t0 = Instant::now();
        let mut clock = clock(t0);
        clock.on_inbound(t0 + Duration::from_secs(10));
        assert_eq!(clock.on_tick(t0 + Duration::from_secs(120)), Tick::SendPing);
        clock.on_inbound(t0 + Duration::from_secs(121));
        assert_eq!(clock.on_tick(t0 + Duration::from_secs(135)), Tick::SendPing);
    }

    #[test]
    fn late_tick_with_ping_outstanding_pings_before_judging() {
        let t0 = Instant::now();
        let mut dead = clock(t0);
        assert_eq!(dead.on_tick(t0 + Duration::from_secs(15)), Tick::SendPing);
        assert_eq!(dead.on_tick(t0 + Duration::from_secs(200)), Tick::SendPing);
        assert_eq!(dead.on_tick(t0 + Duration::from_secs(215)), Tick::Dead);

        let mut saved = clock(t0);
        assert_eq!(saved.on_tick(t0 + Duration::from_secs(15)), Tick::SendPing);
        assert_eq!(saved.on_tick(t0 + Duration::from_secs(200)), Tick::SendPing);
        saved.on_inbound(t0 + Duration::from_secs(201));
        assert_eq!(saved.on_tick(t0 + Duration::from_secs(215)), Tick::SendPing);
    }

    fn expect(error: impl Into<DialFailure>, give_up: bool, banned: Option<&str>) {
        let failure = error.into();
        let (reason, next_step) = match (give_up, &failure) {
            (true, DialFailure::GiveUp { reason, next_step }) => (reason, next_step),
            (false, DialFailure::Retry { reason, next_step }) => (reason, next_step),
            (true, DialFailure::Retry { .. }) => panic!("expected GiveUp, got {failure:?}"),
            (false, DialFailure::GiveUp { .. }) => panic!("expected Retry, got {failure:?}"),
        };
        assert!(!reason.is_empty(), "{failure:?}");
        assert!(!next_step.is_empty(), "{failure:?}");
        if let Some(name) = banned {
            assert!(reason.contains("<profile>"), "{reason}");
            assert!(!reason.contains(name), "{reason}");
            assert!(!next_step.contains(name), "{next_step}");
        }
    }

    #[test]
    fn gateway_dial_classifies_p1_01_errors() {
        let sample = DialFailure::Retry {
            reason: "dial timed out".to_owned(),
            next_step: "Check the gateway with `hermes gateway status`.".to_owned(),
        };
        assert_eq!(sample.reason(), "dial timed out");
        assert_eq!(
            sample.next_step(),
            "Check the gateway with `hermes gateway status`."
        );
        let rendered = format!("{sample:?}");
        assert!(rendered.contains("Retry"), "{rendered}");
        assert!(rendered.contains("dial timed out"), "{rendered}");
        assert!(!rendered.contains("hermes gateway status"), "{rendered}");

        let give_up = [
            DiscoveryError::HomeNotServed,
            DiscoveryError::NamedProfileHome,
            DiscoveryError::Incompatible,
            DiscoveryError::InvalidState,
            DiscoveryError::InvalidApiOrigin,
            DiscoveryError::InvalidSupervisor,
            DiscoveryError::InvalidIdentity,
            DiscoveryError::BadPointer(PointerProblem::NotAbsolute),
            DiscoveryError::UnsafeNode {
                node: NodeKind::Home,
                problem: NodeProblem::PeerIsAnotherUser,
            },
        ];
        for error in give_up {
            expect(error, true, None);
        }
        expect(
            ProfileLookupError::Parked {
                name: "p3probe".to_owned(),
                reason: "held".to_owned(),
            },
            true,
            Some("p3probe"),
        );
        expect(
            ProfileLookupError::NeverDiscovered {
                name: "p3probe".to_owned(),
            },
            true,
            Some("p3probe"),
        );
        expect(ConnectError::SubprotocolMismatch, true, None);
        expect(ConnectError::TlsNotSupported, true, None);
        expect(ConnectError::WrongPurpose, true, None);
        expect(HandshakeError::NotSessionAuthority, true, None);
        expect(HandshakeError::BadReady, true, None);
        expect(HandshakeError::BadDescribe, true, None);
        expect(HandshakeError::BadFrame, true, None);
        expect(TicketError::InvalidRequest, true, None);
        expect(TicketError::ScopeMismatch, true, None);
        expect(TicketError::Malformed, true, None);
        expect(TicketError::NotAuthenticated, true, None);
        expect(TicketError::BindingRejected, true, None);
        expect(
            TicketError::UnsafeNode {
                node: NodeKind::Socket,
                problem: NodeProblem::WrongOwner,
            },
            true,
            None,
        );

        expect(DiscoveryError::NoGateway, false, None);
        expect(DiscoveryError::ControlTimeout, false, None);
        expect(DiscoveryError::ReplyTooLarge, false, None);
        expect(DiscoveryError::InvalidReply, false, None);
        expect(
            DiscoveryError::ControlRefused {
                code: None,
                text: "no".to_owned(),
            },
            false,
            None,
        );
        expect(
            DiscoveryError::NotReady {
                state: ReportedState::Starting,
                idle_exit: false,
            },
            false,
            None,
        );
        expect(
            DiscoveryError::Ensure(EnsureError::NotInstalled),
            false,
            None,
        );
        expect(DiscoveryError::StillUnreachableAfterEnsure, false, None);
        expect(
            DiscoveryError::Io(ErrorKind::ConnectionRefused),
            false,
            None,
        );
        expect(TicketError::StaleInstance, false, None);
        expect(TicketError::CapacityExhausted, false, None);
        expect(TicketError::IdentityChanged, false, None);
        expect(TicketError::InvalidReply, false, None);
        expect(TicketError::Refused("no".to_owned()), false, None);
        expect(TicketError::ReplyTooLarge, false, None);
        expect(TicketError::Timeout, false, None);
        expect(TicketError::Io(ErrorKind::ConnectionRefused), false, None);
        expect(
            ConnectError::UpgradeRejected { status: Some(403) },
            false,
            None,
        );
        expect(ConnectError::Timeout, false, None);
        expect(ConnectError::Io(ErrorKind::ConnectionRefused), false, None);
        expect(HandshakeError::ReadyTimeout, false, None);
        expect(
            HandshakeError::CallTimeout {
                method: "runtime.describe",
            },
            false,
            None,
        );
        expect(HandshakeError::IdentityChanged, false, None);
        expect(
            HandshakeError::DescribeFailed {
                code: 1,
                reason: None,
            },
            false,
            None,
        );
        expect(HandshakeError::Closed(Closed::Dropped), false, None);
        expect(
            HandshakeError::Io(ErrorKind::ConnectionRefused),
            false,
            None,
        );

        let kept = DialFailure::from(ProfileLookupError::NeverDiscovered {
            name: "fixture".to_owned(),
        });
        match kept {
            DialFailure::GiveUp { reason, .. } => assert!(reason.contains("fixture"), "{reason}"),
            DialFailure::Retry { .. } => panic!("fixture is still a give-up"),
        }
    }
}
