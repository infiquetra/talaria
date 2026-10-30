//! The fleet task: one long-lived host-scoped link, its schedule, and the published snapshot.
//!
//! The registry sits behind one lock shared by the handle and the task, so an owner's report
//! (`observe_subscribed`, `set_foreground`) is in the snapshot when the call returns. Network
//! work runs only in the task; the handle forwards the effects it gets back.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hermes_client::liveness::{DialFailure, Link, LinkConfig, LinkEvent, LinkState};
use hermes_client::{
    CallError as WireError, CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy,
    Inbound, ProfileLookupError, discover_with,
};
use serde_json::{Value, json};
use talaria_core::attention::AlertKind;
use talaria_core::fleet::{
    CallError, FleetCounts, FleetEffect, FleetInput, FleetRegistry, FleetRow, LiveItem, NeedId,
    NeedItem, Outcome, PolledApproval, ProfileStatus, RosterRow, RowKey, SubscribedView, update,
};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::config::FleetConfig;
use super::host_dial::{DialReport, HostDial, lock};
use super::schedule::{CLASS_COLD, CLASS_HOT, CLASS_PROMPT, CLASS_ROUND, Schedule, Work};
use crate::side_connection::SideScope;

/// Most diagnostic texts a snapshot carries.
const DIAGNOSTIC_CAP: usize = 20;
/// Events the probe reads back from the latest sequence.
const PROBE_TAIL: u64 = 64;
/// How long closing waits for the task.
const CLOSE_WAIT: Duration = Duration::from_millis(200);
/// First-dial ladder: the link's own ladder takes over once a dial has succeeded.
const FIRST_DIAL_FLOOR: Duration = Duration::from_secs(1);
const FIRST_DIAL_CAP: Duration = Duration::from_secs(30);
/// The reason shown while a profile-only link is dropped to try host scope again.
const RETRY_HOST: &str = "retrying host scope";

/// What the link is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetLinkState {
    /// Before the first connection.
    Connecting,
    /// The link is up and its first round of reads has answered.
    Connected,
    /// The link dropped and will try again.
    Reconnecting {
        /// What a person can read.
        reason: String,
    },
    /// The link stopped trying.
    Failed {
        /// What a person can read.
        reason: String,
    },
}

/// One published view of the fleet.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetSnapshot {
    /// Raised by one each time a snapshot is published.
    pub version: u64,
    /// Host scope, or the one profile in view.
    pub scope: SideScope,
    /// Link state.
    pub link: FleetLinkState,
    /// Profiles in served order, then parked.
    pub profiles: Vec<(String, ProfileStatus)>,
    /// Rows in canonical order.
    pub rows: Vec<FleetRow>,
    /// Queue items, ascending sequence.
    pub queue: Vec<NeedItem>,
    /// Background counts.
    pub counts: FleetCounts,
    /// Distinct diagnostic texts, oldest first.
    pub diagnostics: Vec<String>,
    /// Seconds since 1970 when this snapshot was published.
    pub as_of: f64,
}

/// A published snapshot, or an attention the foreground owner should hear.
#[derive(Clone, Debug)]
pub enum FleetNotice {
    /// The snapshot changed.
    Changed(Arc<FleetSnapshot>),
    /// An attention effect.
    Attention {
        /// Which alert.
        kind: AlertKind,
        /// Row.
        key: RowKey,
        /// Queue item, when the alert is a need opening.
        need: Option<NeedId>,
        /// The row is foreground.
        foreground: bool,
    },
}

/// Why a one-shot list stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetListError {
    /// No gateway is running for this Hermes home.
    NoGateway,
    /// The ticket, the socket, or the handshake failed.
    Attach(String),
    /// `--profile` names nothing being served.
    UnknownProfile {
        /// How many profiles are served.
        served: usize,
    },
    /// `--profile` names a parked profile.
    ParkedProfile {
        /// The gateway's reason.
        reason: String,
    },
}

type Notify = Arc<dyn Fn(FleetNotice) + Send + Sync>;

/// What the handle sends the task.
enum Command {
    /// Effects of an input the handle applied.
    Effects(Vec<FleetEffect>),
    /// Discovery and a full round now.
    Refresh,
    /// Stop.
    Close,
}

/// State both sides read: the registry and what was last published.
struct Published {
    registry: FleetRegistry,
    link: FleetLinkState,
    scope: SideScope,
    service_diagnostics: Vec<String>,
    snapshot: Arc<FleetSnapshot>,
    core_version: u64,
}

/// The registry, the latest snapshot, and the way out to the consumer and the task.
struct Hub {
    state: Mutex<Published>,
    notify: Notify,
    commands: mpsc::UnboundedSender<Command>,
}

impl Hub {
    /// Apply one input and return the network effects for the caller to schedule.
    fn apply(&self, input: FleetInput) -> Vec<FleetEffect> {
        let (effects, changed) = {
            let mut state = lock(&self.state);
            let effects = update(&mut state.registry, input);
            (effects, publish(&mut state))
        };
        if let Some(snapshot) = changed {
            (self.notify)(FleetNotice::Changed(snapshot));
        }
        let mut network = Vec::new();
        for effect in effects {
            match effect {
                FleetEffect::Attention {
                    kind,
                    key,
                    need,
                    foreground,
                } => (self.notify)(FleetNotice::Attention {
                    kind,
                    key,
                    need,
                    foreground,
                }),
                FleetEffect::Diagnostic(_) => {}
                other => network.push(other),
            }
        }
        network
    }

    /// Apply an input from the handle; its network effects go to the task.
    fn apply_from_handle(&self, input: FleetInput) {
        let effects = self.apply(input);
        if !effects.is_empty() {
            let _ = self.commands.send(Command::Effects(effects));
        }
    }

    fn set_link(&self, link: FleetLinkState, scope: Option<SideScope>) {
        let changed = {
            let mut state = lock(&self.state);
            state.link = link;
            if let Some(scope) = scope {
                state.scope = scope;
            }
            publish(&mut state)
        };
        if let Some(snapshot) = changed {
            (self.notify)(FleetNotice::Changed(snapshot));
        }
    }

    fn note(&self, text: String) {
        let changed = {
            let mut state = lock(&self.state);
            if state.service_diagnostics.contains(&text) {
                return;
            }
            state.service_diagnostics.push(text);
            publish(&mut state)
        };
        if let Some(snapshot) = changed {
            (self.notify)(FleetNotice::Changed(snapshot));
        }
    }

    fn snapshot(&self) -> Arc<FleetSnapshot> {
        Arc::clone(&lock(&self.state).snapshot)
    }

    fn hot_profiles(&self) -> Vec<String> {
        lock(&self.state).registry.hot_profiles()
    }
}

/// Publish a new snapshot when something a reader can see changed.
fn publish(state: &mut Published) -> Option<Arc<FleetSnapshot>> {
    let diagnostics = diagnostics_of(state);
    let old = &state.snapshot;
    let unchanged = state.registry.version() == state.core_version
        && old.link == state.link
        && old.scope == state.scope
        && old.diagnostics == diagnostics;
    if unchanged {
        return None;
    }
    state.core_version = state.registry.version();
    let snapshot = Arc::new(FleetSnapshot {
        version: old.version + 1,
        scope: state.scope.clone(),
        link: state.link.clone(),
        profiles: state
            .registry
            .profiles()
            .into_iter()
            .map(|(name, status)| (name.to_owned(), status.clone()))
            .collect(),
        rows: state.registry.rows().into_iter().cloned().collect(),
        queue: state.registry.queue().into_iter().cloned().collect(),
        counts: state.registry.counts(),
        diagnostics,
        as_of: now_secs(),
    });
    state.snapshot = Arc::clone(&snapshot);
    Some(snapshot)
}

fn diagnostics_of(state: &Published) -> Vec<String> {
    let mut all: Vec<String> = state
        .registry
        .diagnostics()
        .into_iter()
        .map(str::to_owned)
        .collect();
    all.extend(state.service_diagnostics.iter().cloned());
    let extra = all.len().saturating_sub(DIAGNOSTIC_CAP);
    all.drain(..extra);
    all
}

/// Seconds since 1970. The service stamps inputs; the core reads no clock.
fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

/// The running fleet task.
pub struct FleetService;

impl FleetService {
    /// Spawn the fleet task and return at once. No frame is sent on the caller.
    pub fn start(
        home: PathBuf,
        foreground_profile: String,
        cfg: FleetConfig,
        notify: impl Fn(FleetNotice) + Send + Sync + 'static,
    ) -> FleetHandle {
        let (commands, receiver) = mpsc::unbounded_channel();
        let scope = HostDial::new(home.clone(), foreground_profile.clone()).scope();
        let hub = Arc::new(Hub {
            state: Mutex::new(Published {
                registry: FleetRegistry::new(),
                link: FleetLinkState::Connecting,
                scope: scope.clone(),
                service_diagnostics: Vec::new(),
                snapshot: Arc::new(FleetSnapshot {
                    version: 0,
                    scope,
                    link: FleetLinkState::Connecting,
                    profiles: Vec::new(),
                    rows: Vec::new(),
                    queue: Vec::new(),
                    counts: FleetCounts::default(),
                    diagnostics: Vec::new(),
                    as_of: now_secs(),
                }),
                core_version: 0,
            }),
            notify: Arc::new(notify),
            commands,
        });
        let task = FleetTask::new(Arc::clone(&hub), home, foreground_profile, cfg, receiver);
        let join = tokio::spawn(task.run());
        FleetHandle { hub, join }
    }
}

/// Commands into the fleet task, and the latest snapshot.
pub struct FleetHandle {
    hub: Arc<Hub>,
    join: tokio::task::JoinHandle<()>,
}

/// A cloneable reporting endpoint for an independently owned fleet backend.
#[derive(Clone)]
pub struct FleetReporter {
    hub: Arc<Hub>,
}

impl FleetReporter {
    /// Publish a complete external-session observation.
    pub fn observe_external(&self, session: talaria_core::fleet::ExternalSession) {
        self.hub.apply_from_handle(FleetInput::External(session));
    }

    /// Remove a session after its external owner has ended it.
    pub fn remove_external(&self, key: RowKey) {
        self.hub
            .apply_from_handle(FleetInput::RemoveExternal { key });
    }

    /// Keep a stopped process's rows until the operator acknowledges them.
    pub fn external_exited(&self, key: RowKey) {
        self.hub
            .apply_from_handle(FleetInput::ExternalExited { key });
    }

    /// Publish one sanitized diagnostic.
    pub fn diagnostic(&self, text: &str) {
        self.hub
            .note(talaria_core::text::sanitize(text).into_owned());
    }

    /// Read the current fleet snapshot.
    pub fn snapshot(&self) -> Arc<FleetSnapshot> {
        self.hub.snapshot()
    }
}

impl FleetHandle {
    /// Create a reporting endpoint that does not own the fleet task.
    pub fn reporter(&self) -> FleetReporter {
        FleetReporter {
            hub: Arc::clone(&self.hub),
        }
    }

    /// The latest snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<FleetSnapshot> {
        self.hub.snapshot()
    }

    /// An owner is reporting this session.
    pub fn observe_subscribed(&self, key: RowKey, view: SubscribedView) {
        self.hub.apply_from_handle(FleetInput::Subscribed {
            key,
            view,
            at: now_secs(),
        });
    }

    /// The owner let go.
    pub fn observe_unsubscribed(&self, key: RowKey) {
        self.hub.apply_from_handle(FleetInput::Unsubscribed {
            key,
            at: now_secs(),
        });
    }

    /// The foreground session changed.
    pub fn set_foreground(&self, key: Option<RowKey>) {
        self.hub.apply_from_handle(FleetInput::Foreground {
            key,
            at: now_secs(),
        });
    }

    /// The person acknowledged a done or failed row.
    pub fn acknowledge(&self, key: RowKey) {
        self.hub.apply_from_handle(FleetInput::Acknowledge {
            key,
            at: now_secs(),
        });
    }

    /// Discovery and a full round now. Under a profile-only fallback the link is dialed again
    /// so host scope is tried once more; the snapshot says so at once.
    pub fn refresh_now(&self) {
        if matches!(self.hub.snapshot().scope, SideScope::ProfileOnly { .. }) {
            self.hub.set_link(
                FleetLinkState::Reconnecting {
                    reason: RETRY_HOST.into(),
                },
                None,
            );
        }
        let _ = self.hub.commands.send(Command::Refresh);
    }

    /// Stop, waiting at most 200 ms for the task.
    pub async fn close(self) {
        let _ = self.hub.commands.send(Command::Close);
        let mut join = self.join;
        if tokio::time::timeout(CLOSE_WAIT, &mut join).await.is_err() {
            join.abort();
        }
    }
}

/// One-shot read for `talaria fleet list`: discover, attach, list once, close.
pub async fn list_once(home: &Path, cfg: &FleetConfig) -> Result<FleetSnapshot, FleetListError> {
    let profile = check_fleet_profile(home, cfg).await?;
    let handle = FleetService::start(home.to_path_buf(), profile, cfg.clone(), |_| {});
    let started = Instant::now();
    let deadline = cfg.connect_timeout + cfg.call_timeout;
    let mut last_reason = String::from("no connection");
    let result = loop {
        let snapshot = handle.snapshot();
        match &snapshot.link {
            FleetLinkState::Connected => break Ok((*snapshot).clone()),
            FleetLinkState::Failed { reason } => break Err(FleetListError::Attach(reason.clone())),
            FleetLinkState::Reconnecting { reason } => last_reason.clone_from(reason),
            FleetLinkState::Connecting => {}
        }
        let elapsed = started.elapsed();
        let connecting = !matches!(snapshot.link, FleetLinkState::Connected);
        if elapsed >= deadline || (connecting && elapsed >= cfg.connect_timeout && !linked(&handle))
        {
            break Err(FleetListError::Attach(last_reason));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    handle.close().await;
    result
}

/// True once the link has connected, even while its first round is still answering.
fn linked(handle: &FleetHandle) -> bool {
    !lock(&handle.hub.state).registry.profiles().is_empty()
}

/// The profile the ticket names: `--profile`, else `default`. An unserved or parked
/// `--profile` is refused before any ticket is minted; no gateway is [`FleetListError::NoGateway`].
pub async fn check_fleet_profile(home: &Path, cfg: &FleetConfig) -> Result<String, FleetListError> {
    let options = DiscoverOptions {
        ensure: EnsurePolicy::Never,
        ..DiscoverOptions::default()
    };
    let endpoint = discover_with(home, &options)
        .await
        .map_err(|error| match error {
            DiscoveryError::NoGateway => FleetListError::NoGateway,
            other => FleetListError::Attach(other.to_string()),
        })?;
    let name = cfg.only_profile.clone().unwrap_or_else(|| "default".into());
    match endpoint.resolve_profile(&name) {
        Ok(_) => Ok(name.trim().to_lowercase()),
        Err(error) if cfg.only_profile.is_none() => Err(FleetListError::Attach(error.to_string())),
        Err(ProfileLookupError::Parked { reason, .. }) => {
            Err(FleetListError::ParkedProfile { reason })
        }
        Err(ProfileLookupError::NeverDiscovered { .. }) => Err(FleetListError::UnknownProfile {
            served: endpoint.profiles().len(),
        }),
    }
}

/// The connection a reply belongs to: which link, and its channel generation.
type Tag = (u64, u64);

/// Served profile names and parked profiles with their reasons, as discovery reported them.
type Discovered = (Vec<String>, Vec<(String, String)>);

/// What a spawned read reports.
enum Done {
    Call {
        work: Work,
        tag: Tag,
        request: u64,
        outcome: Result<Value, CallError>,
    },
    Discovery(Result<Discovered, String>),
}

/// The first round after a connect: the link is published as connected once it answered.
struct Gate {
    waiting: HashSet<Work>,
    until: Instant,
}

struct Probe {
    id: u64,
    deadline: Instant,
}

struct FleetTask {
    hub: Arc<Hub>,
    home: PathBuf,
    foreground: String,
    cfg: FleetConfig,
    liveness_gap: Duration,
    commands: mpsc::UnboundedReceiver<Command>,
    link: Option<Link>,
    events: Option<mpsc::Receiver<LinkEvent>>,
    reports: Arc<Mutex<Option<DialReport>>>,
    link_epoch: u64,
    tag: Option<Tag>,
    client: Option<Client>,
    in_scope: Vec<String>,
    schedule: Schedule,
    calls: JoinSet<Done>,
    request: u64,
    /// Session ids in the latest successful liveness reply of each profile, and its connection.
    live: HashMap<String, (Tag, HashSet<String>)>,
    gate: Option<Gate>,
    /// The scope a connect learned, published with the link once the first round answered.
    pending_scope: Option<SideScope>,
    probes: HashMap<RowKey, Probe>,
    deferred: HashMap<RowKey, Instant>,
    coalesce_at: Option<Instant>,
    trailing: HashMap<String, Instant>,
    next_hot: Instant,
    next_cold: Instant,
    cold_cursor: usize,
    next_backstop: Instant,
    refused_noted: bool,
    discovering: bool,
    closing: bool,
}

impl FleetTask {
    fn new(
        hub: Arc<Hub>,
        home: PathBuf,
        foreground: String,
        cfg: FleetConfig,
        commands: mpsc::UnboundedReceiver<Command>,
    ) -> Self {
        let now = Instant::now();
        let liveness_gap = cfg.liveness_gap.min(cfg.hot_poll / 2);
        Self {
            hub,
            home,
            foreground,
            schedule: Schedule::new(cfg.max_in_flight, cfg.suspect_in_flight),
            liveness_gap,
            commands,
            link: None,
            events: None,
            reports: Arc::new(Mutex::new(None)),
            link_epoch: 0,
            tag: None,
            client: None,
            in_scope: Vec::new(),
            calls: JoinSet::new(),
            request: 0,
            live: HashMap::new(),
            gate: None,
            pending_scope: None,
            probes: HashMap::new(),
            deferred: HashMap::new(),
            coalesce_at: None,
            trailing: HashMap::new(),
            next_hot: now + cfg.hot_poll,
            next_cold: now + cfg.cold_poll,
            cold_cursor: 0,
            next_backstop: now + cfg.backstop,
            refused_noted: false,
            discovering: false,
            closing: false,
            cfg,
        }
    }

    async fn run(mut self) {
        let mut attempt: u32 = 0;
        while !self.closing {
            // A close sent before the task first ran wins over dialing.
            self.drain_commands();
            if self.closing {
                break;
            }
            if self.link.is_none() {
                match self.dial().await {
                    Some(Ok(())) => attempt = 0,
                    Some(Err(DialFailure::GiveUp { reason, .. })) => {
                        self.hub.set_link(FleetLinkState::Failed { reason }, None);
                        self.idle_until_close().await;
                        break;
                    }
                    Some(Err(DialFailure::Retry { reason, .. })) => {
                        self.hub
                            .set_link(FleetLinkState::Reconnecting { reason }, None);
                        attempt = attempt.saturating_add(1);
                        self.wait_retry(attempt).await;
                        continue;
                    }
                    None => break,
                }
            }
            self.serve().await;
        }
        if let Some(link) = self.link.take() {
            let _ = tokio::time::timeout(CLOSE_WAIT, link.close()).await;
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            self.command(command);
        }
    }

    /// Start a link. `None` means a close arrived while dialing.
    async fn dial(&mut self) -> Option<Result<(), DialFailure>> {
        let dial = HostDial::new(self.home.clone(), self.foreground.clone());
        self.reports = dial.reports();
        let start = Link::start(dial, LinkConfig::default());
        tokio::pin!(start);
        loop {
            tokio::select! {
                started = &mut start => {
                    return Some(started.map(|(link, events)| {
                        self.link_epoch += 1;
                        self.link = Some(link);
                        self.events = Some(events);
                    }));
                }
                command = self.commands.recv() => match command {
                    Some(Command::Close) | None => {
                        self.closing = true;
                        return None;
                    }
                    Some(other) => self.command(other),
                },
            }
        }
    }

    async fn wait_retry(&mut self, attempt: u32) {
        let exponent = attempt.saturating_sub(1).min(5);
        let delay = (FIRST_DIAL_FLOOR * 2u32.pow(exponent)).min(FIRST_DIAL_CAP);
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => return,
                command = self.commands.recv() => match command {
                    Some(Command::Close) | None => {
                        self.closing = true;
                        return;
                    }
                    Some(Command::Refresh) => return,
                    Some(other) => self.command(other),
                },
            }
        }
    }

    async fn idle_until_close(&mut self) {
        while let Some(command) = self.commands.recv().await {
            if matches!(command, Command::Close) {
                break;
            }
        }
        self.closing = true;
    }

    /// Serve one link until it ends, is replaced, or the task closes.
    async fn serve(&mut self) {
        loop {
            self.pump();
            let wake = self.next_wake();
            let Some(events) = self.events.as_mut() else {
                return;
            };
            tokio::select! {
                command = self.commands.recv() => match command {
                    Some(command) => {
                        self.command(command);
                        if self.closing || self.link.is_none() {
                            return;
                        }
                    }
                    None => {
                        self.closing = true;
                        return;
                    }
                },
                event = events.recv() => match event {
                    Some(event) => {
                        if !self.link_event(event).await {
                            return;
                        }
                    }
                    None => {
                        self.drop_link("the link ended").await;
                        return;
                    }
                },
                Some(done) = self.calls.join_next(), if !self.calls.is_empty() => {
                    if let Ok(done) = done {
                        self.done(done);
                    }
                }
                () = tokio::time::sleep_until(wake.into()) => self.timers(),
            }
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Close => self.closing = true,
            Command::Effects(effects) => self.effects(effects),
            Command::Refresh => {
                let profile_only = matches!(
                    lock(&self.reports).as_ref().map(|report| &report.scope),
                    Some(SideScope::ProfileOnly { .. })
                );
                if profile_only && self.link.is_some() {
                    // Dropped here; `serve` returns and the outer loop dials host scope again.
                    self.disconnect(RETRY_HOST);
                } else {
                    self.backstop();
                }
            }
        }
    }

    /// Forget the link without waiting; its task closes on its own.
    fn disconnect(&mut self, reason: &str) {
        if let Some(link) = self.link.take() {
            tokio::spawn(link.close());
        }
        self.events = None;
        self.lost(reason);
    }

    async fn drop_link(&mut self, reason: &str) {
        if let Some(link) = self.link.take() {
            let _ = tokio::time::timeout(CLOSE_WAIT, link.close()).await;
        }
        self.events = None;
        self.lost(reason);
    }

    fn lost(&mut self, reason: &str) {
        self.tag = None;
        self.client = None;
        self.gate = None;
        self.live.clear();
        self.probes.clear();
        self.deferred.clear();
        self.schedule.forget(|_| false);
        self.refused_noted = false;
        let _ = self
            .hub
            .apply(FleetInput::ConnectionLost { at: now_secs() });
        self.hub.set_link(
            FleetLinkState::Reconnecting {
                reason: reason.to_owned(),
            },
            None,
        );
    }

    /// Returns false when the link has ended.
    async fn link_event(&mut self, event: LinkEvent) -> bool {
        match event {
            LinkEvent::State(LinkState::Connected { generation }) => {
                self.connected(generation);
                true
            }
            LinkEvent::State(LinkState::Reconnecting { reason, .. }) => {
                self.lost(&reason);
                true
            }
            LinkEvent::State(LinkState::Failed { reason, .. }) => {
                self.lost(&reason);
                self.hub.set_link(FleetLinkState::Failed { reason }, None);
                self.idle_until_close().await;
                false
            }
            LinkEvent::State(LinkState::Closed) => false,
            LinkEvent::Inbound { item, .. } => {
                self.inbound(item);
                true
            }
            LinkEvent::Gap(_) | LinkEvent::Reattached { .. } | LinkEvent::SessionGone { .. } => {
                true
            }
        }
    }

    fn connected(&mut self, generation: u64) {
        let now = Instant::now();
        let tag = (self.link_epoch, generation);
        self.tag = Some(tag);
        self.client = self.link.as_ref().map(Link::client);
        self.refused_noted = false;
        let report = lock(&self.reports).clone();
        let (scope, served, parked) = match report {
            Some(report) => (report.scope, report.served, report.parked),
            None => (
                SideScope::ProfileOnly {
                    profile: self.foreground.clone(),
                    why: super::host_dial::HOST_REFUSED.into(),
                },
                vec![self.foreground.clone()],
                Vec::new(),
            ),
        };
        let (served, parked) = self.narrow(served, parked);
        self.in_scope = match &scope {
            SideScope::Host { .. } => served.clone(),
            SideScope::ProfileOnly { profile, .. } => vec![profile.clone()],
        };
        let _ = self.hub.apply(FleetInput::Profiles {
            served,
            parked,
            in_scope: self.in_scope.clone(),
            at: now_secs(),
        });
        let mut waiting = HashSet::new();
        for profile in self.in_scope.clone() {
            self.schedule
                .trigger(Work::Roster(profile.clone()), CLASS_ROUND, now);
            self.schedule
                .trigger(Work::Liveness(profile.clone()), CLASS_ROUND, now);
            waiting.insert(Work::Roster(profile.clone()));
            waiting.insert(Work::Liveness(profile));
        }
        self.gate = Some(Gate {
            waiting,
            until: now + self.cfg.call_timeout,
        });
        self.pending_scope = Some(scope);
        self.next_hot = now + self.cfg.hot_poll;
        self.next_cold = now + self.cold_step();
        self.next_backstop = now + self.cfg.backstop;
        self.open_gate_if_done(now);
    }

    /// `--profile` narrows the served list to that one name.
    fn narrow(
        &self,
        served: Vec<String>,
        parked: Vec<(String, String)>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        match &self.cfg.only_profile {
            None => (served, parked),
            Some(only) => (
                served.into_iter().filter(|name| name == only).collect(),
                parked
                    .into_iter()
                    .filter(|(name, _)| name == only)
                    .collect(),
            ),
        }
    }

    fn open_gate_if_done(&mut self, now: Instant) {
        let Some(gate) = &self.gate else {
            return;
        };
        if !gate.waiting.is_empty() && now < gate.until {
            return;
        }
        self.gate = None;
        let scope = self.pending_scope.take();
        self.hub.set_link(FleetLinkState::Connected, scope);
    }

    fn inbound(&mut self, item: Inbound) {
        match item {
            Inbound::Event(event) => {
                if event.raw["type"].as_str() == Some("sessions.changed") && self.tag.is_some() {
                    let at = Instant::now() + self.cfg.coalesce;
                    self.coalesce_at.get_or_insert(at);
                }
            }
            Inbound::ServerRequest(request) => {
                // The fleet link attaches to no session, so Hermes sends it no server request.
                // One that arrives anyway is refused at once, whatever its kind.
                let kind = serde_json::to_value(&request.request)
                    .ok()
                    .and_then(|value| value.as_object()?.keys().next().cloned())
                    .unwrap_or_else(|| "unknown".to_owned());
                let responder = request.responder;
                let message = format!("talaria does not support server request: {kind}");
                tokio::spawn(async move {
                    let _ = responder.fail(-32601, &message).await;
                });
                if !self.refused_noted {
                    self.refused_noted = true;
                    self.hub.note(format!(
                        "the gateway sent a {kind} server request to the fleet link; it was refused"
                    ));
                }
            }
            Inbound::Malformed(_) | Inbound::Closed(_) => {}
        }
    }

    fn effects(&mut self, effects: Vec<FleetEffect>) {
        let now = Instant::now();
        for effect in effects {
            match effect {
                FleetEffect::FetchApprovals { key } => self.fetch_approvals(key, now),
                FleetEffect::ProbeOutcome { key, probe_id } => {
                    self.probes.insert(
                        key.clone(),
                        Probe {
                            id: probe_id,
                            deadline: now + self.cfg.probe_deadline,
                        },
                    );
                    if self.guard(&key) {
                        self.schedule
                            .trigger(Work::ProbeHead { key, probe_id }, CLASS_PROMPT, now);
                    } else {
                        self.probed(key, probe_id, Outcome::Unknown, false);
                    }
                }
                FleetEffect::Attention { .. } | FleetEffect::Diagnostic(_) => {}
            }
        }
    }

    fn fetch_approvals(&mut self, key: RowKey, now: Instant) {
        if !self.guard(&key) {
            return;
        }
        let work = Work::Approvals(key.clone());
        if let Some(gate) = &mut self.gate {
            gate.waiting.insert(work.clone());
        }
        match self.schedule.started(&work) {
            Some(started) if now.saturating_duration_since(started) < self.cfg.approvals_gap => {
                self.deferred
                    .entry(key)
                    .or_insert(started + self.cfg.approvals_gap);
            }
            _ => self.schedule.trigger(work, CLASS_PROMPT, now),
        }
    }

    /// Approval and event reads go only to sessions in the latest liveness reply of the
    /// current connection: reading a session that is not live restores it.
    fn guard(&self, key: &RowKey) -> bool {
        let Some(tag) = self.tag else {
            return false;
        };
        self.live
            .get(&key.profile)
            .is_some_and(|(seen, ids)| *seen == tag && ids.contains(&key.session_id))
    }

    fn probed(&mut self, key: RowKey, probe_id: u64, outcome: Outcome, found_complete: bool) {
        if !self
            .probes
            .get(&key)
            .is_some_and(|probe| probe.id == probe_id)
        {
            return;
        }
        self.probes.remove(&key);
        let effects = self.hub.apply(FleetInput::OutcomeProbed {
            key,
            probe_id,
            outcome,
            found_complete,
            at: now_secs(),
        });
        self.effects(effects);
    }

    /// Start every pending read a free slot allows.
    fn pump(&mut self) {
        let (Some(tag), Some(client)) = (self.tag, self.client.clone()) else {
            return;
        };
        let now = Instant::now();
        while let Some((work, _suspect)) = self.schedule.next(now) {
            self.request += 1;
            let request = self.request;
            let (method, params) = self.call_for(&work);
            let timeout = self.cfg.call_timeout;
            let client = client.clone();
            self.calls.spawn(async move {
                let outcome = client
                    .call_raw(
                        method,
                        params,
                        CallOptions {
                            timeout: Some(timeout),
                        },
                    )
                    .await
                    .map_err(call_error);
                Done::Call {
                    work,
                    tag,
                    request,
                    outcome,
                }
            });
        }
    }

    fn call_for(&self, work: &Work) -> (&'static str, Value) {
        match work {
            Work::Roster(profile) => (
                "session.list",
                json!({"profile": profile, "scope": "live", "limit": 200}),
            ),
            Work::Liveness(profile) => ("session.active_list", json!({"profile": profile})),
            Work::Approvals(key) => (
                "approval.pending",
                json!({"session_id": key.session_id, "profile": key.profile}),
            ),
            Work::ProbeHead { key, .. } => (
                "session.events.since",
                json!({"session_id": key.session_id, "profile": key.profile, "last_sequence": 0}),
            ),
            Work::ProbeTail {
                key, epoch, from, ..
            } => (
                "session.events.since",
                json!({
                    "session_id": key.session_id,
                    "profile": key.profile,
                    "last_sequence": from,
                    "replay_epoch": epoch,
                }),
            ),
        }
    }

    fn done(&mut self, done: Done) {
        let now = Instant::now();
        match done {
            Done::Discovery(result) => {
                self.discovering = false;
                if let Ok((served, parked)) = result
                    && self.tag.is_some()
                {
                    let (served, parked) = self.narrow(served, parked);
                    if let Some(SideScope::Host { .. }) =
                        lock(&self.reports).as_ref().map(|report| &report.scope)
                    {
                        self.in_scope = served.clone();
                    }
                    let _ = self.hub.apply(FleetInput::Profiles {
                        served,
                        parked,
                        in_scope: self.in_scope.clone(),
                        at: now_secs(),
                    });
                }
                self.round(now);
            }
            Done::Call {
                work,
                tag,
                request,
                outcome,
            } => {
                self.schedule.finish(&work, now);
                if Some(tag) != self.tag {
                    // A reply from a replaced connection never changes state.
                    return;
                }
                let profile = work.profile().to_owned();
                match &outcome {
                    Err(CallError::NoReply) => self.schedule.set_suspect(&profile, true),
                    Err(_) => {}
                    Ok(_) => self.schedule.set_suspect(&profile, false),
                }
                self.reply(work.clone(), tag, request, outcome, now);
                if let Some(gate) = &mut self.gate {
                    gate.waiting.remove(&work);
                }
                self.open_gate_if_done(now);
            }
        }
    }

    fn reply(
        &mut self,
        work: Work,
        tag: Tag,
        request: u64,
        outcome: Result<Value, CallError>,
        now: Instant,
    ) {
        let at = now_secs();
        let input = match (work, outcome) {
            (Work::Roster(profile), Ok(result)) => match result["sessions"].as_array() {
                Some(rows) => {
                    let decoded: Vec<RosterRow> =
                        rows.iter().filter_map(RosterRow::from_value).collect();
                    FleetInput::Roster {
                        profile,
                        request,
                        malformed: rows.len() - decoded.len(),
                        rows: decoded,
                        at,
                    }
                }
                None => FleetInput::RosterFailed {
                    profile,
                    request,
                    error: CallError::Refused("bad result".into()),
                    at,
                },
            },
            (Work::Roster(profile), Err(error)) => FleetInput::RosterFailed {
                profile,
                request,
                error,
                at,
            },
            (Work::Liveness(profile), Ok(result)) => match result["sessions"].as_array() {
                Some(items) => {
                    let decoded: Vec<LiveItem> =
                        items.iter().filter_map(LiveItem::from_value).collect();
                    let ids = decoded.iter().map(|item| item.session_id.clone()).collect();
                    self.live.insert(profile.clone(), (tag, ids));
                    FleetInput::Liveness {
                        profile,
                        request,
                        malformed: items.len() - decoded.len(),
                        items: decoded,
                        at,
                    }
                }
                None => FleetInput::LivenessFailed {
                    profile,
                    request,
                    error: CallError::Refused("bad result".into()),
                    at,
                },
            },
            (Work::Liveness(profile), Err(error)) => FleetInput::LivenessFailed {
                profile,
                request,
                error,
                at,
            },
            (Work::Approvals(key), Ok(result)) => FleetInput::Approvals {
                key,
                request,
                approvals: approvals_of(&result),
                at,
            },
            (Work::Approvals(key), Err(error)) => FleetInput::ApprovalsFailed {
                key,
                request,
                error,
                at,
            },
            (Work::ProbeHead { key, probe_id }, outcome) => {
                self.probe_head(key, probe_id, outcome, now);
                return;
            }
            (Work::ProbeTail { key, probe_id, .. }, outcome) => {
                let (outcome, found) = match outcome {
                    Ok(result) => tail_outcome(&result),
                    Err(_) => (Outcome::Unknown, false),
                };
                self.probed(key, probe_id, outcome, found);
                return;
            }
        };
        let effects = self.hub.apply(input);
        self.effects(effects);
    }

    fn probe_head(
        &mut self,
        key: RowKey,
        probe_id: u64,
        outcome: Result<Value, CallError>,
        now: Instant,
    ) {
        if !self
            .probes
            .get(&key)
            .is_some_and(|probe| probe.id == probe_id)
        {
            return;
        }
        let Ok(result) = outcome else {
            self.probed(key, probe_id, Outcome::Unknown, false);
            return;
        };
        let latest = result["latest_seq"].as_u64().unwrap_or(0);
        let epoch = result["replay_epoch"]
            .as_str()
            .or_else(|| result["epoch"].as_str())
            .map(str::to_owned);
        match epoch {
            Some(epoch) if latest > 0 && self.guard(&key) => {
                let from = latest.saturating_sub(PROBE_TAIL);
                self.schedule.trigger(
                    Work::ProbeTail {
                        key,
                        probe_id,
                        epoch,
                        from,
                    },
                    CLASS_PROMPT,
                    now,
                );
            }
            _ => self.probed(key, probe_id, Outcome::Unknown, false),
        }
    }

    /// Discovery for the profile list, then a full round. The round runs when discovery ends.
    fn backstop(&mut self) {
        if self.tag.is_none() || self.discovering {
            return;
        }
        self.discovering = true;
        let home = self.home.clone();
        self.calls.spawn(async move {
            let options = DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            };
            Done::Discovery(
                discover_with(&home, &options)
                    .await
                    .map(|endpoint| {
                        (
                            endpoint
                                .profiles()
                                .iter()
                                .map(|served| served.name.clone())
                                .collect(),
                            endpoint.parked().to_vec(),
                        )
                    })
                    .map_err(|error| error.to_string()),
            )
        });
    }

    fn round(&mut self, now: Instant) {
        for profile in self.in_scope.clone() {
            self.schedule
                .trigger(Work::Roster(profile.clone()), CLASS_ROUND, now);
            self.schedule
                .trigger(Work::Liveness(profile), CLASS_ROUND, now);
        }
    }

    fn cold_step(&self) -> Duration {
        let hot: HashSet<String> = self.hub.hot_profiles().into_iter().collect();
        let cold = self
            .in_scope
            .iter()
            .filter(|profile| !hot.contains(*profile))
            .count()
            .max(1);
        let count = u32::try_from(cold).unwrap_or(u32::MAX);
        (self.cfg.cold_poll / count).max(Duration::from_millis(1))
    }

    fn next_wake(&self) -> Instant {
        let mut wake = self.next_hot.min(self.next_cold).min(self.next_backstop);
        if let Some(at) = self.coalesce_at {
            wake = wake.min(at);
        }
        if let Some(gate) = &self.gate {
            wake = wake.min(gate.until);
        }
        for at in self
            .trailing
            .values()
            .chain(self.deferred.values())
            .chain(self.probes.values().map(|probe| &probe.deadline))
        {
            wake = wake.min(*at);
        }
        wake
    }

    fn timers(&mut self) {
        let now = Instant::now();
        if self.tag.is_none() {
            self.next_hot = now + self.cfg.hot_poll;
            self.next_cold = now + self.cfg.cold_poll;
            self.next_backstop = now + self.cfg.backstop;
            return;
        }
        let expired: Vec<(RowKey, u64)> = self
            .probes
            .iter()
            .filter(|(_, probe)| probe.deadline <= now)
            .map(|(key, probe)| (key.clone(), probe.id))
            .collect();
        for (key, probe_id) in expired {
            self.schedule.forget(|work| match work {
                Work::ProbeHead { key: probe, .. } | Work::ProbeTail { key: probe, .. } => {
                    *probe != key
                }
                _ => true,
            });
            self.probed(key, probe_id, Outcome::Unknown, false);
        }
        let due: Vec<RowKey> = self
            .deferred
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in due {
            self.deferred.remove(&key);
            if self.guard(&key) {
                self.schedule
                    .trigger(Work::Approvals(key), CLASS_PROMPT, now);
            }
        }
        if self.coalesce_at.is_some_and(|at| at <= now) {
            self.coalesce_at = None;
            for profile in self.in_scope.clone() {
                self.schedule
                    .trigger(Work::Liveness(profile.clone()), CLASS_ROUND, now);
                let roster = Work::Roster(profile.clone());
                match self.schedule.started(&roster) {
                    Some(started)
                        if now.saturating_duration_since(started) < self.cfg.roster_gap =>
                    {
                        self.trailing
                            .entry(profile)
                            .or_insert(started + self.cfg.roster_gap);
                    }
                    _ => self.schedule.trigger(roster, CLASS_ROUND, now),
                }
            }
        }
        let trailing: Vec<String> = self
            .trailing
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(profile, _)| profile.clone())
            .collect();
        for profile in trailing {
            self.trailing.remove(&profile);
            self.schedule
                .trigger(Work::Roster(profile), CLASS_ROUND, now);
        }
        if self.next_hot <= now {
            self.next_hot = now + self.cfg.hot_poll;
            for profile in self.hub.hot_profiles() {
                if self.in_scope.contains(&profile) {
                    self.poll(profile, CLASS_HOT, now);
                }
            }
        }
        if self.next_cold <= now {
            let hot: HashSet<String> = self.hub.hot_profiles().into_iter().collect();
            let cold: Vec<String> = self
                .in_scope
                .iter()
                .filter(|profile| !hot.contains(*profile))
                .cloned()
                .collect();
            if !cold.is_empty() {
                let profile = cold[self.cold_cursor % cold.len()].clone();
                self.cold_cursor = self.cold_cursor.wrapping_add(1);
                self.poll(profile, CLASS_COLD, now);
            }
            self.next_cold = now + self.cold_step();
        }
        if self.next_backstop <= now {
            self.next_backstop = now + self.cfg.backstop;
            self.backstop();
        }
        self.open_gate_if_done(now);
    }

    fn poll(&mut self, profile: String, class: u8, now: Instant) {
        let work = Work::Liveness(profile);
        if !self.schedule.within(&work, self.liveness_gap, now) {
            self.schedule.trigger(work, class, now);
        }
    }
}

/// Map a failed call. A timeout or a dropped connection is an unknown outcome, never success.
fn call_error(error: WireError) -> CallError {
    match error {
        WireError::Rpc(rpc) => match (rpc.code, rpc.reason.as_deref()) {
            (-32601, _) => CallError::UnknownMethod,
            (5001, _) => CallError::StorageUnavailable,
            (_, Some("profile_mismatch")) => CallError::ProfileMismatch,
            (_, Some("not_found")) => CallError::NotFound,
            (_, Some("storage_unavailable")) => CallError::StorageUnavailable,
            (_, Some("unknown_method")) => CallError::UnknownMethod,
            (_, Some(reason)) => CallError::Refused(reason.to_owned()),
            (code, None) => CallError::Refused(format!("code {code}")),
        },
        WireError::TimedOut { .. } => CallError::NoReply,
        WireError::Unknown { .. } | WireError::NotConnected => CallError::ConnectionDropped,
        WireError::Encode | WireError::InvalidTimeout => {
            CallError::Refused("invalid_params".into())
        }
        WireError::BadResult { .. } => CallError::Refused("bad result".into()),
    }
}

fn approvals_of(result: &Value) -> Vec<PolledApproval> {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    result["approvals"]
        .as_array()
        .map(|approvals| {
            approvals
                .iter()
                .map(|approval| PolledApproval {
                    prompt_id: text(&approval["request_id"]).filter(|id| !id.is_empty()),
                    command: text(&approval["command"]),
                    description: text(&approval["description"]),
                    choices: approval["choices"]
                        .as_array()
                        .map(|choices| choices.iter().filter_map(text).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The outcome of the last finished turn in an events reply.
fn tail_outcome(result: &Value) -> (Outcome, bool) {
    let flag = |name: &str| result[name].as_bool().unwrap_or(false);
    if flag("truncated") || flag("snapshot_required") {
        return (Outcome::Unknown, false);
    }
    let complete = result["events"].as_array().and_then(|events| {
        events
            .iter()
            .rev()
            .find(|event| event["type"].as_str() == Some("message.complete"))
    });
    match complete {
        Some(event) => (
            Outcome::from_turn_status(event["payload"]["status"].as_str()),
            true,
        ),
        None => (Outcome::Unknown, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_reads_the_last_completion() {
        let result = json!({"truncated": false, "snapshot_required": false, "events": [
            {"type": "message.complete", "payload": {"status": "complete"}},
            {"type": "message.delta", "payload": {}},
            {"type": "message.complete", "payload": {"status": "error"}},
        ]});
        assert_eq!(tail_outcome(&result), (Outcome::Failed, true));
        assert_eq!(
            tail_outcome(&json!({"truncated": true, "events": []})),
            (Outcome::Unknown, false)
        );
        assert_eq!(
            tail_outcome(&json!({"events": [{"type": "message.delta"}]})),
            (Outcome::Unknown, false)
        );
    }

    #[test]
    fn approvals_read_null_ids_as_absent() {
        let result = json!({"approvals": [
            {"request_id": "a1", "command": "ls", "description": null, "choices": ["once", 3]},
            {"request_id": null, "command": null},
        ]});
        let read = approvals_of(&result);
        assert_eq!(read[0].prompt_id.as_deref(), Some("a1"));
        assert_eq!(read[0].choices, vec!["once".to_string()]);
        assert_eq!(read[1].prompt_id, None);
        assert!(approvals_of(&json!({})).is_empty());
    }
}
