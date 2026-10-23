//! Heartbeat, reconnect backoff, and the link that owns one client.
//!
//! Instants are [`tokio::time::Instant`]. The first connection is not retried.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::channel::{Client, Inbound};
use crate::connect::{ConnectError, Connection, ConnectionParts, HandshakeError};
use crate::discovery::{DiscoveryError, ProfileLookupError};
use crate::replay::{Cursor, GapReason, ReplayGap};
use crate::ticket::TicketError;

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
        let _ = (self, attempt);
        todo!("P1-03")
    }

    /// Uniform point in `[max(floor, ceiling / 2), ceiling]` selected by `draw`.
    ///
    /// `draw` is in `[0, 1]`. `0` is the bottom of the band and `1` is the ceiling.
    pub fn delay(&self, attempt: u32, draw: f64) -> Duration {
        let _ = (self, attempt, draw);
        todo!("P1-03")
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
}

impl HeartbeatClock {
    /// Store the interval, the deadline, and the instant the handshake finished.
    pub fn new(interval: Duration, deadline: Duration, connected_at: Instant) -> Self {
        Self {
            interval,
            deadline,
            connected_at,
        }
    }

    /// Remember the latest sign of life. An older instant is ignored.
    pub fn on_inbound(&mut self, at: Instant) {
        let _ = (self, at);
        todo!("P1-03")
    }

    /// Decide this tick. A tick later than two intervals only sends a ping.
    pub fn on_tick(&mut self, now: Instant) -> Tick {
        let _ = (self.interval, self.deadline, self.connected_at, now);
        todo!("P1-03")
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
        let _ = error;
        todo!("P1-03")
    }
}

impl From<ProfileLookupError> for DialFailure {
    fn from(error: ProfileLookupError) -> Self {
        let _ = error;
        todo!("P1-03")
    }
}

impl From<TicketError> for DialFailure {
    fn from(error: TicketError) -> Self {
        let _ = error;
        todo!("P1-03")
    }
}

impl From<ConnectError> for DialFailure {
    fn from(error: ConnectError) -> Self {
        let _ = error;
        todo!("P1-03")
    }
}

impl From<HandshakeError> for DialFailure {
    fn from(error: HandshakeError) -> Self {
        let _ = error;
        todo!("P1-03")
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
    hook: Option<Arc<dyn Fn(&mut Connection, u64) + Send + Sync>>,
    origin: Option<String>,
}

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
    pub fn on_connect(mut self, hook: Arc<dyn Fn(&mut Connection, u64) + Send + Sync>) -> Self {
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
    fn dial(
        &mut self,
        generation: u64,
    ) -> impl Future<Output = Result<Dialed, DialFailure>> + Send {
        let _ = (self.home.as_os_str(), &self.profile, generation);
        async { todo!("P1-03") }
    }
}

/// What the consumer sees the link doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// A handshake finished. The first generation is 1.
    Connected {
        /// P1-02 generation of the open socket.
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
pub enum LinkEvent {
    /// The link changed state.
    State(LinkState),
    /// One inbound item. `replayed` is true for events taken from a replay window.
    Inbound {
        /// P1-02 item, including its generation and raw params.
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
    _private: (),
}

impl std::fmt::Debug for Link {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (self, formatter);
        todo!("P1-03")
    }
}

impl Link {
    /// Dial once. The first event is `State(Connected { generation: 1 })`.
    ///
    /// A failed first dial is returned. It is not retried.
    pub async fn start(
        dial: impl Dial,
        cfg: LinkConfig,
    ) -> Result<(Self, mpsc::Receiver<LinkEvent>), DialFailure> {
        let _ = (dial, cfg);
        todo!("P1-03")
    }

    /// The one client. Clones stay valid across reconnects.
    pub fn client(&self) -> Client {
        let _ = self;
        todo!("P1-03")
    }

    /// Track `session_id` at `cursor`. Returns before the task applies it.
    pub fn track(&self, session_id: &str, cursor: Cursor) {
        let _ = (self, session_id, cursor);
        todo!("P1-03")
    }

    /// Stop tracking `session_id`. Sends nothing to the gateway.
    pub fn untrack(&self, session_id: &str) {
        let _ = (self, session_id);
        todo!("P1-03")
    }

    /// Install `cursor` after the consumer has rebuilt the session.
    pub fn resync(&self, session_id: &str, cursor: Cursor) {
        let _ = (self, session_id, cursor);
        todo!("P1-03")
    }

    /// A snapshot of the counters.
    pub fn stats(&self) -> LinkStats {
        let _ = self;
        todo!("P1-03")
    }

    /// Stop the link. Emits `State(Closed)` and then ends.
    pub async fn close(self) {
        let _ = self._private;
        todo!("P1-03")
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;
    use crate::connect::Closed;
    use crate::discovery::{EnsureError, NodeKind, NodeProblem, PointerProblem, ReportedState};

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
