//! Pure fleet reducer.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::{
    AlertKind, ExternalSession, FleetBackend, FleetRow, LiveItem, LiveStatus, NeedDetail, NeedId,
    NeedItem, NeedSlot, NeedSummary, Outcome, RosterRow, RowKey, RowState, SubscribedNeed,
    SubscribedTurn, SubscribedView,
};

/// `session.list` returns at most this many rows; a full page may hide more.
const ROSTER_LIMIT: usize = 200;
/// Gone rows and unacknowledged unlisted rows kept together.
const HELD_CAP: usize = 50;
/// Diagnostic texts kept.
const DIAGNOSTIC_CAP: usize = 20;

/// Why a fleet call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallError {
    /// No reply before the call timeout.
    NoReply,
    /// The connection dropped.
    ConnectionDropped,
    /// The gateway answered `profile_mismatch`.
    ProfileMismatch,
    /// The gateway answered storage unavailable.
    StorageUnavailable,
    /// The method is not implemented.
    UnknownMethod,
    /// The gateway answered `not_found`.
    NotFound,
    /// Any other refusal. The string is the reason word.
    Refused(String),
}

/// What the registry knows about one profile.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileStatus {
    /// A roster has not answered yet.
    Loading,
    /// The latest roster answered. `truncated` is 200 rows.
    Listed { truncated: bool },
    /// The latest roster or discovery failed.
    Failed { reason: String },
    /// Discovery says the profile is parked.
    Parked { reason: String },
    /// Host scope was refused, so this profile is outside the one profile in view.
    OutOfScope,
}

/// Counts of background rows. The foreground row is not included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FleetCounts {
    /// Background rows in needs-you.
    pub needs_you: usize,
    /// Queue items on background rows.
    pub needs_you_items: usize,
    /// Background rows working.
    pub working: usize,
    /// Background rows failed.
    pub failed: usize,
    /// Background rows done.
    pub done: usize,
    /// Background rows idle.
    pub idle: usize,
}

/// One approval from `approval.pending`.
#[derive(Clone, Debug, PartialEq)]
pub struct PolledApproval {
    /// Prompt id. `None` counts toward the waiting rule and is not its own item.
    pub prompt_id: Option<String>,
    /// Command text.
    pub command: Option<String>,
    /// Description text.
    pub description: Option<String>,
    /// Offered choices.
    pub choices: Vec<String>,
}

/// One input to [`update`].
#[derive(Clone, Debug)]
pub enum FleetInput {
    /// Discovery.
    Profiles {
        /// Served profile names, in served order.
        served: Vec<String>,
        /// Parked profiles and the reason for each.
        parked: Vec<(String, String)>,
        /// Profiles this link may read.
        in_scope: Vec<String>,
        /// Seconds since 1970.
        at: f64,
    },
    /// A successful roster.
    Roster {
        /// Profile the call named.
        profile: String,
        /// Request number.
        request: u64,
        /// Decoded rows.
        rows: Vec<RosterRow>,
        /// Rows `from_value` refused.
        malformed: usize,
        /// Seconds since 1970.
        at: f64,
    },
    /// A failed roster.
    RosterFailed {
        /// Profile the call named.
        profile: String,
        /// Request number.
        request: u64,
        /// Why it failed.
        error: CallError,
        /// Seconds since 1970.
        at: f64,
    },
    /// A successful liveness reply.
    Liveness {
        /// Profile the call named.
        profile: String,
        /// Request number.
        request: u64,
        /// Decoded items.
        items: Vec<LiveItem>,
        /// Items `from_value` refused.
        malformed: usize,
        /// Seconds since 1970.
        at: f64,
    },
    /// A failed liveness call.
    LivenessFailed {
        /// Profile the call named.
        profile: String,
        /// Request number.
        request: u64,
        /// Why it failed.
        error: CallError,
        /// Seconds since 1970.
        at: f64,
    },
    /// A successful `approval.pending` reply.
    Approvals {
        /// Row the call named.
        key: RowKey,
        /// Request number.
        request: u64,
        /// Approvals in the reply.
        approvals: Vec<PolledApproval>,
        /// Seconds since 1970.
        at: f64,
    },
    /// A failed `approval.pending` call.
    ApprovalsFailed {
        /// Row the call named.
        key: RowKey,
        /// Request number.
        request: u64,
        /// Why it failed.
        error: CallError,
        /// Seconds since 1970.
        at: f64,
    },
    /// An outcome probe finished.
    OutcomeProbed {
        /// Row the probe named.
        key: RowKey,
        /// Identity of this particular outcome check.
        probe_id: u64,
        /// Outcome the probe decided.
        outcome: Outcome,
        /// True when a `message.complete` was found, whatever its status.
        found_complete: bool,
        /// Seconds since 1970.
        at: f64,
    },
    /// An owner is reporting this session.
    Subscribed {
        /// Row.
        key: RowKey,
        /// The view.
        view: SubscribedView,
        /// Seconds since 1970.
        at: f64,
    },
    /// An independently owned session changed.
    External(ExternalSession),
    /// An external owner ended a session.
    RemoveExternal { key: RowKey },
    /// Retain a stopped external process's row until acknowledgement.
    ExternalExited { key: RowKey },
    /// The owner let go.
    Unsubscribed {
        /// Row.
        key: RowKey,
        /// Seconds since 1970.
        at: f64,
    },
    /// The foreground session changed. `None` clears it.
    Foreground {
        /// The new foreground row, if any.
        key: Option<RowKey>,
        /// Seconds since 1970.
        at: f64,
    },
    /// The person acknowledged a done or failed row.
    Acknowledge {
        /// Row.
        key: RowKey,
        /// Seconds since 1970.
        at: f64,
    },
    /// The link dropped.
    ConnectionLost {
        /// Seconds since 1970.
        at: f64,
    },
}

/// What the service should do after [`update`].
#[derive(Clone, Debug, PartialEq)]
pub enum FleetEffect {
    /// Read `approval.pending` for this row.
    FetchApprovals { key: RowKey },
    /// Probe how the turn ended.
    ProbeOutcome { key: RowKey, probe_id: u64 },
    /// Tell the foreground attention owner.
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
    /// One distinct diagnostic. The registry keeps at most 20.
    Diagnostic(String),
}

/// Why the registry asked for an outcome probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeReason {
    /// A running status turned idle.
    TurnEnded,
    /// The message count rose between two idle reads. `before` is the count it rose from.
    CountRose { before: Option<u64> },
}

/// One row and what the rules remember about it.
#[derive(Clone, Debug)]
struct RowEntry {
    row: FleetRow,
    /// A running status was read since the last outcome.
    turn_seen: bool,
    /// Message count of the previous applied liveness item.
    last_live_count: Option<u64>,
    /// Status in the latest successful liveness reply; `None` when the reply left it out.
    latest: Option<LiveStatus>,
    /// A probe was asked for and has not answered.
    probe: Option<(u64, ProbeReason)>,
    /// The view an owner holds.
    view: Option<SubscribedView>,
    /// The held view's turn was running, so a finished view is a finished turn.
    view_running: bool,
    /// Consecutive successful rosters that left the row out.
    misses: u8,
    /// The latest roster's `running` flag.
    roster_running: bool,
}

impl RowEntry {
    fn new(key: RowKey, at: f64, foreground: bool) -> RowEntry {
        RowEntry {
            row: FleetRow {
                key,
                title: String::new(),
                model: None,
                source: None,
                started_at: None,
                message_count: 0,
                state: RowState::Idle,
                state_since: at,
                outcome: None,
                foreground,
                subscribed: false,
                stale: false,
                gone: false,
                unlisted: false,
                activity: None,
                cost_usd: None,
                backend: super::FleetBackend::Hermes,
                agent: None,
                occupancy: None,
                cost: None,
            },
            turn_seen: false,
            last_live_count: None,
            latest: None,
            probe: None,
            view: None,
            view_running: false,
            misses: 0,
            roster_running: false,
        }
    }

    /// A working session the roster does not list has no visible row; it is still tracked so a
    /// need or a finished turn shows at once.
    fn visible(&self) -> bool {
        !(self.row.unlisted && self.row.state == RowState::Working)
    }

    fn held(&self) -> bool {
        self.row.gone || (self.row.unlisted && finished(self.row.state))
    }
}

fn finished(state: RowState) -> bool {
    matches!(state, RowState::Done | RowState::Failed)
}

fn running(state: RowState) -> bool {
    matches!(state, RowState::Working | RowState::NeedsYou)
}

/// Per-profile memory that is not visible.
#[derive(Clone, Debug, Default)]
struct ProfileMemo {
    roster_request: u64,
    liveness_request: u64,
    /// Latest successful liveness items, for rows a later roster creates.
    live: HashMap<String, LiveItem>,
    listed_once: bool,
    no_liveness: bool,
    no_approvals: bool,
    roster_malformed_noted: bool,
    live_malformed_noted: bool,
    unmatched_noted: bool,
}

/// The needs-you queue, ordered by `seq`.
#[derive(Clone, Debug, Default)]
struct Queue {
    items: BTreeMap<u64, NeedItem>,
    index: HashMap<NeedId, u64>,
    last_seq: u64,
    changed: bool,
}

impl Queue {
    fn next_seq(&mut self) -> u64 {
        self.last_seq += 1;
        self.last_seq
    }

    fn has_items(&self, key: &RowKey) -> bool {
        self.items.values().any(|item| item.id.key == *key)
    }

    fn get(&self, id: &NeedId) -> Option<&NeedItem> {
        self.index.get(id).and_then(|seq| self.items.get(seq))
    }

    fn insert(&mut self, item: NeedItem) {
        self.index.insert(item.id.clone(), item.seq);
        self.items.insert(item.seq, item);
        self.changed = true;
    }

    fn remove(&mut self, id: &NeedId) -> Option<NeedItem> {
        let seq = self.index.remove(id)?;
        self.changed = true;
        self.items.remove(&seq)
    }

    /// Change one item in place; marks the queue changed only when the item differs.
    fn modify(&mut self, id: &NeedId, change: impl FnOnce(&mut NeedItem)) {
        let Some(seq) = self.index.get(id) else {
            return;
        };
        if let Some(item) = self.items.get_mut(seq) {
            let before = item.clone();
            change(item);
            if *item != before {
                self.changed = true;
            }
        }
    }

    /// Remove a row's items that match; true when any went.
    fn remove_where(&mut self, key: &RowKey, keep_out: impl Fn(&NeedItem) -> bool) -> bool {
        let gone: Vec<NeedId> = self
            .items
            .values()
            .filter(|item| item.id.key == *key && keep_out(item))
            .map(|item| item.id.clone())
            .collect();
        for id in &gone {
            self.remove(id);
        }
        !gone.is_empty()
    }

    /// Non-exact items of a row: polled, downgraded and waiting placeholders.
    fn settle_polled(&mut self, key: &RowKey) -> bool {
        self.remove_where(key, |item| !item.exact)
    }

    fn for_row(&mut self, key: &RowKey, change: impl Fn(&mut NeedItem)) {
        let ids: Vec<NeedId> = self
            .items
            .values()
            .filter(|item| item.id.key == *key)
            .map(|item| item.id.clone())
            .collect();
        for id in ids {
            self.modify(&id, &change);
        }
    }

    fn has_named(&self, key: &RowKey) -> bool {
        self.items
            .values()
            .any(|item| item.id.key == *key && item.id.slot != NeedSlot::Waiting)
    }

    /// Add an item, taking a waiting placeholder's place when a named item first arrives.
    fn open(&mut self, mut item: NeedItem, effects: &mut Vec<FleetEffect>) {
        let placeholder = NeedId {
            key: item.id.key.clone(),
            slot: NeedSlot::Waiting,
        };
        if item.id.slot != NeedSlot::Waiting
            && let Some(old) = self.remove(&placeholder)
        {
            item.seq = old.seq;
            item.since = old.since;
            self.insert(item);
            return;
        }
        item.seq = self.next_seq();
        effects.push(FleetEffect::Attention {
            kind: AlertKind::NeedsYou,
            key: item.id.key.clone(),
            need: Some(item.id.clone()),
            foreground: item.foreground,
        });
        self.insert(item);
    }
}

/// Distinct diagnostic texts.
#[derive(Clone, Debug, Default)]
struct Diagnostics {
    kept: VecDeque<String>,
    seen: HashSet<String>,
}

impl Diagnostics {
    fn note(&mut self, text: String, effects: &mut Vec<FleetEffect>) {
        if !self.seen.insert(text.clone()) {
            return;
        }
        if self.kept.len() == DIAGNOSTIC_CAP {
            self.kept.pop_front();
        }
        self.kept.push_back(text.clone());
        effects.push(FleetEffect::Diagnostic(text));
    }
}

/// Rows, queue, and profile status. Pure.
#[derive(Clone, Debug, Default)]
pub struct FleetRegistry {
    rows: HashMap<RowKey, RowEntry>,
    queue: Queue,
    profiles: Vec<(String, ProfileStatus)>,
    memo: HashMap<String, ProfileMemo>,
    /// The latest discovery's in-scope profiles; `None` before the first discovery.
    scope: Option<HashSet<String>>,
    foreground: Option<RowKey>,
    approvals_request: HashMap<RowKey, u64>,
    diagnostics: Diagnostics,
    next_probe_id: u64,
    version: u64,
}

impl FleetRegistry {
    /// An empty registry.
    pub fn new() -> FleetRegistry {
        FleetRegistry::default()
    }

    /// Rows in canonical order.
    pub fn rows(&self) -> Vec<&FleetRow> {
        let mut first_seq: HashMap<&RowKey, u64> = HashMap::new();
        for item in self.queue.items.values() {
            first_seq.entry(&item.id.key).or_insert(item.seq);
        }
        let mut rows: Vec<&FleetRow> = self
            .rows
            .values()
            .filter(|entry| entry.visible())
            .map(|entry| &entry.row)
            .collect();
        rows.sort_by(|left, right| {
            rank(left.state)
                .cmp(&rank(right.state))
                .then_with(|| {
                    if left.state == RowState::NeedsYou {
                        let left_seq = first_seq.get(&left.key).copied().unwrap_or(u64::MAX);
                        let right_seq = first_seq.get(&right.key).copied().unwrap_or(u64::MAX);
                        left_seq.cmp(&right_seq)
                    } else {
                        right.state_since.total_cmp(&left.state_since)
                    }
                })
                .then_with(|| left.key.cmp(&right.key))
        });
        rows
    }

    /// One row.
    pub fn row(&self, key: &RowKey) -> Option<&FleetRow> {
        self.rows.get(key).map(|entry| &entry.row)
    }

    /// Queue items, ascending `seq`.
    pub fn queue(&self) -> Vec<&NeedItem> {
        self.queue.items.values().collect()
    }

    /// Background counts.
    pub fn counts(&self) -> FleetCounts {
        let mut counts = FleetCounts::default();
        for entry in self.rows.values() {
            if !entry.visible() || entry.row.foreground {
                continue;
            }
            match entry.row.state {
                RowState::NeedsYou => counts.needs_you += 1,
                RowState::Working => counts.working += 1,
                RowState::Failed => counts.failed += 1,
                RowState::Done => counts.done += 1,
                RowState::Idle => counts.idle += 1,
            }
        }
        counts.needs_you_items = self
            .queue
            .items
            .values()
            .filter(|item| !item.foreground)
            .count();
        counts
    }

    /// Served profiles in served order, then parked.
    pub fn profiles(&self) -> Vec<(&str, &ProfileStatus)> {
        self.profiles
            .iter()
            .map(|(name, status)| (name.as_str(), status))
            .collect()
    }

    /// Profiles with a needs-you or working row, or a probe in flight.
    pub fn hot_profiles(&self) -> Vec<String> {
        let mut hot: Vec<String> = self
            .rows
            .values()
            .filter(|entry| {
                entry.row.backend == FleetBackend::Hermes
                    && (running(entry.row.state) || entry.probe.is_some())
            })
            .map(|entry| entry.row.key.profile.clone())
            .collect::<HashSet<String>>()
            .into_iter()
            .collect();
        hot.sort();
        hot
    }

    /// Distinct diagnostic texts, oldest first, at most 20.
    pub fn diagnostics(&self) -> Vec<&str> {
        self.diagnostics.kept.iter().map(String::as_str).collect()
    }

    /// Rises by one exactly when rows, the queue, counts, or profiles changed.
    pub fn version(&self) -> u64 {
        self.version
    }

    fn in_scope(&self, profile: &str) -> bool {
        self.scope
            .as_ref()
            .is_none_or(|scope| scope.contains(profile))
    }

    fn ensure_profile(&mut self, profile: &str) {
        if !self.profiles.iter().any(|(name, _)| name == profile) {
            self.profiles
                .push((profile.to_string(), ProfileStatus::Loading));
        }
    }

    fn set_profile(&mut self, profile: &str, status: ProfileStatus) {
        self.ensure_profile(profile);
        if let Some((_, slot)) = self.profiles.iter_mut().find(|(name, _)| name == profile) {
            *slot = status;
        }
    }

    fn profile_status(&self, profile: &str) -> Option<&ProfileStatus> {
        self.profiles
            .iter()
            .find(|(name, _)| name == profile)
            .map(|(_, status)| status)
    }

    fn keys_of(&self, profile: &str) -> Vec<RowKey> {
        self.rows
            .keys()
            .filter(|key| key.profile == profile)
            .cloned()
            .collect()
    }

    /// Remove a row and its items silently.
    fn remove_row(&mut self, key: &RowKey, dirty: &mut bool) {
        if let Some(entry) = self.rows.remove(key)
            && entry.visible()
        {
            *dirty = true;
        }
        self.queue.remove_where(key, |_| true);
    }

    /// Insert a new row; its foreground flag follows the latest `Foreground`.
    fn create_row(&mut self, key: RowKey, at: f64, dirty: &mut bool) {
        let foreground = self.foreground.as_ref() == Some(&key);
        let entry = RowEntry::new(key.clone(), at, foreground);
        if entry.visible() {
            *dirty = true;
        }
        self.rows.insert(key, entry);
    }

    /// Drop the oldest gone and unacknowledged unlisted rows beyond the cap.
    fn cap_held(&mut self, dirty: &mut bool) {
        let mut held: Vec<(f64, RowKey)> = self
            .rows
            .values()
            .filter(|entry| entry.held())
            .map(|entry| (entry.row.state_since, entry.row.key.clone()))
            .collect();
        if held.len() <= HELD_CAP {
            return;
        }
        held.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        let extra = held.len() - HELD_CAP;
        for (_, key) in held.into_iter().take(extra) {
            self.remove_row(&key, dirty);
        }
    }
}

fn rank(state: RowState) -> u8 {
    match state {
        RowState::NeedsYou => 0,
        RowState::Working => 1,
        RowState::Failed => 2,
        RowState::Done => 3,
        RowState::Idle => 4,
    }
}

fn reason_text(error: &CallError) -> String {
    match error {
        CallError::NoReply => "no reply".into(),
        CallError::ConnectionDropped => "connection dropped".into(),
        CallError::ProfileMismatch => "no longer served by the gateway".into(),
        CallError::StorageUnavailable => "Hermes storage busy".into(),
        CallError::UnknownMethod => "unknown method".into(),
        CallError::NotFound => "not_found".into(),
        CallError::Refused(reason) => reason.clone(),
    }
}

/// Raise the stored request number; false when `request` is not above it.
fn bump(stored: &mut u64, request: u64) -> bool {
    if request <= *stored && *stored != 0 {
        return false;
    }
    *stored = request;
    true
}

/// Change a row and mark the update visible when the row was or is visible and differs.
fn touch<R>(
    rows: &mut HashMap<RowKey, RowEntry>,
    key: &RowKey,
    dirty: &mut bool,
    change: impl FnOnce(&mut RowEntry) -> R,
) -> Option<R> {
    let entry = rows.get_mut(key)?;
    let was_visible = entry.visible();
    let before = entry.row.clone();
    let result = change(entry);
    let is_visible = entry.visible();
    if (was_visible || is_visible) && (was_visible != is_visible || entry.row != before) {
        *dirty = true;
    }
    Some(result)
}

/// Move a row to a state. The foreground row never becomes done or failed.
fn set_state(entry: &mut RowEntry, state: RowState, at: f64) {
    let state = if entry.row.foreground && finished(state) {
        RowState::Idle
    } else {
        state
    };
    if entry.row.state == state {
        return;
    }
    if running(state) && !running(entry.row.state) {
        entry.row.outcome = None;
    }
    entry.row.state = state;
    entry.row.state_since = at;
}

/// Done or failed while an unacknowledged outcome is held, else idle.
fn rest_state(entry: &RowEntry) -> RowState {
    match entry.row.outcome {
        Some(_) if entry.row.foreground => RowState::Idle,
        Some(Outcome::Failed) => RowState::Failed,
        Some(_) => RowState::Done,
        None => RowState::Idle,
    }
}

/// Recompute after the row's items changed: needs-you exactly when it has an item.
fn refresh_state(entry: &mut RowEntry, queue: &Queue, at: f64) {
    if queue.has_items(&entry.row.key) {
        set_state(entry, RowState::NeedsYou, at);
        return;
    }
    if entry.row.state != RowState::NeedsYou {
        return;
    }
    let busy = match &entry.view {
        Some(view) => matches!(view.turn, SubscribedTurn::Running { .. }),
        None => {
            entry.probe.is_some()
                || entry
                    .latest
                    .as_ref()
                    .is_some_and(|status| *status != LiveStatus::Idle)
        }
    };
    let state = if busy {
        RowState::Working
    } else {
        rest_state(entry)
    };
    set_state(entry, state, at);
}

fn waiting_item(entry: &RowEntry, at: f64) -> NeedItem {
    NeedItem {
        id: NeedId {
            key: entry.row.key.clone(),
            slot: NeedSlot::Waiting,
        },
        seq: 0,
        since: at,
        detail: NeedDetail::Waiting,
        choices: Vec::new(),
        execution_generation: None,
        timeout_s: None,
        exact: false,
        foreground: entry.row.foreground,
        external_options: Vec::new(),
    }
}

fn finish_attention(entry: &RowEntry, outcome: Outcome) -> FleetEffect {
    FleetEffect::Attention {
        kind: if outcome == Outcome::Failed {
            AlertKind::Failed
        } else {
            AlertKind::Done
        },
        key: entry.row.key.clone(),
        need: None,
        foreground: entry.row.foreground,
    }
}

/// A row first seen idle shows how long it has been idle, from Hermes's own time.
fn seed_idle_since(entry: &mut RowEntry, first_sight: bool, last_active: Option<f64>, at: f64) {
    if first_sight
        && entry.row.state == RowState::Idle
        && let Some(since) = last_active.filter(|since| *since <= at)
    {
        entry.row.state_since = since;
    }
}

/// Apply one liveness status to an unsubscribed row (section "row state" of the rules).
#[allow(clippy::too_many_arguments)]
fn apply_status(
    entry: &mut RowEntry,
    status: LiveStatus,
    count: u64,
    no_approvals: bool,
    at: f64,
    queue: &mut Queue,
    diagnostics: &mut Diagnostics,
    next_probe_id: &mut u64,
    effects: &mut Vec<FleetEffect>,
) {
    let previous = entry.last_live_count.replace(count);
    entry.latest = Some(status.clone());
    if entry.view.is_some() {
        return;
    }
    apply_running(
        entry,
        status,
        count,
        previous,
        no_approvals,
        at,
        queue,
        diagnostics,
        next_probe_id,
        effects,
    );
}

#[allow(clippy::too_many_arguments)]
fn apply_running(
    entry: &mut RowEntry,
    status: LiveStatus,
    count: u64,
    previous: Option<u64>,
    no_approvals: bool,
    at: f64,
    queue: &mut Queue,
    diagnostics: &mut Diagnostics,
    next_probe_id: &mut u64,
    effects: &mut Vec<FleetEffect>,
) {
    let key = entry.row.key.clone();
    match status {
        LiveStatus::Waiting => {
            entry.turn_seen = true;
            entry.probe = None;
            if no_approvals {
                if !queue.has_items(&key) {
                    queue.open(waiting_item(entry, at), effects);
                }
            } else {
                effects.push(FleetEffect::FetchApprovals { key: key.clone() });
            }
            let state = if queue.has_items(&key) {
                RowState::NeedsYou
            } else {
                RowState::Working
            };
            set_state(entry, state, at);
        }
        LiveStatus::Idle => {
            queue.settle_polled(&key);
            if entry.turn_seen {
                entry.turn_seen = false;
                if entry.row.foreground {
                    set_state(entry, RowState::Idle, at);
                } else {
                    *next_probe_id += 1;
                    entry.probe = Some((*next_probe_id, ProbeReason::TurnEnded));
                    effects.push(FleetEffect::ProbeOutcome {
                        key,
                        probe_id: *next_probe_id,
                    });
                    refresh_state(entry, queue, at);
                }
            } else if previous.is_some_and(|before| count > before)
                && !entry.row.foreground
                && entry.probe.is_none()
            {
                *next_probe_id += 1;
                entry.probe = Some((*next_probe_id, ProbeReason::CountRose { before: previous }));
                effects.push(FleetEffect::ProbeOutcome {
                    key,
                    probe_id: *next_probe_id,
                });
            } else if entry.probe.is_none() {
                let state = rest_state(entry);
                set_state(entry, state, at);
            }
        }
        LiveStatus::Working | LiveStatus::Starting | LiveStatus::Other(_) => {
            if let LiveStatus::Other(word) = &status {
                diagnostics.note(
                    format!("unknown live status \"{word}\" in profile {}", key.profile),
                    effects,
                );
            }
            entry.turn_seen = true;
            entry.probe = None;
            queue.settle_polled(&key);
            let state = if queue.has_items(&key) {
                RowState::NeedsYou
            } else {
                RowState::Working
            };
            set_state(entry, state, at);
        }
    }
}

/// Apply one input. Reads no clock.
pub fn update(reg: &mut FleetRegistry, input: FleetInput) -> Vec<FleetEffect> {
    let mut effects = Vec::new();
    let mut dirty = false;
    let profiles_before = reg.profiles.clone();
    reg.queue.changed = false;
    match input {
        FleetInput::Profiles {
            served,
            parked,
            in_scope,
            at: _,
        } => on_profiles(reg, served, parked, in_scope, &mut dirty),
        FleetInput::Roster {
            profile,
            request,
            rows,
            malformed,
            at,
        } => on_roster(
            reg,
            &profile,
            request,
            rows,
            malformed,
            at,
            &mut effects,
            &mut dirty,
        ),
        FleetInput::RosterFailed {
            profile,
            request,
            error,
            at: _,
        } => on_roster_failed(reg, &profile, request, &error, &mut effects, &mut dirty),
        FleetInput::Liveness {
            profile,
            request,
            items,
            malformed,
            at,
        } => on_liveness(
            reg,
            &profile,
            request,
            items,
            malformed,
            at,
            &mut effects,
            &mut dirty,
        ),
        FleetInput::LivenessFailed {
            profile,
            request,
            error,
            at,
        } => on_liveness_failed(reg, &profile, request, &error, at, &mut effects, &mut dirty),
        FleetInput::Approvals {
            key,
            request,
            approvals,
            at,
        } => on_approvals(reg, &key, request, approvals, at, &mut effects, &mut dirty),
        FleetInput::ApprovalsFailed {
            key,
            request,
            error,
            at,
        } => on_approvals_failed(reg, &key, request, &error, at, &mut effects, &mut dirty),
        FleetInput::OutcomeProbed {
            key,
            probe_id,
            outcome,
            found_complete,
            at,
        } => on_probed(
            reg,
            &key,
            probe_id,
            outcome,
            found_complete,
            at,
            &mut effects,
            &mut dirty,
        ),
        FleetInput::Subscribed { key, view, at } => {
            on_subscribed(reg, key, view, at, &mut effects, &mut dirty)
        }
        FleetInput::External(session) => on_external(reg, session, &mut effects, &mut dirty),
        FleetInput::RemoveExternal { key } => {
            if reg
                .rows
                .get(&key)
                .is_some_and(|entry| entry.row.backend == FleetBackend::Acp)
            {
                reg.remove_row(&key, &mut dirty);
            }
        }
        FleetInput::ExternalExited { key } => {
            touch(&mut reg.rows, &key, &mut dirty, |entry| {
                if entry.row.backend == FleetBackend::Acp {
                    entry.row.gone = true;
                }
            });
        }
        FleetInput::Unsubscribed { key, at: _ } => on_unsubscribed(reg, &key, &mut dirty),
        FleetInput::Foreground { key, at } => on_foreground(reg, key, at, &mut dirty),
        FleetInput::Acknowledge { key, at } => on_acknowledge(reg, &key, at, &mut dirty),
        FleetInput::ConnectionLost { at: _ } => on_connection_lost(reg, &mut dirty),
    }
    if dirty || reg.queue.changed || reg.profiles != profiles_before {
        reg.version += 1;
    }
    effects
}

fn on_profiles(
    reg: &mut FleetRegistry,
    served: Vec<String>,
    parked: Vec<(String, String)>,
    in_scope: Vec<String>,
    dirty: &mut bool,
) {
    let scope: HashSet<String> = in_scope.into_iter().collect();
    let mut next: Vec<(String, ProfileStatus)> = Vec::new();
    for name in served {
        if next.iter().any(|(known, _)| *known == name) {
            continue;
        }
        let status = if scope.contains(&name) {
            match reg.profile_status(&name) {
                Some(ProfileStatus::Parked { .. } | ProfileStatus::OutOfScope) | None => {
                    ProfileStatus::Loading
                }
                Some(status) => status.clone(),
            }
        } else {
            ProfileStatus::OutOfScope
        };
        next.push((name, status));
    }
    for (name, reason) in parked {
        if !next.iter().any(|(known, _)| *known == name) {
            next.push((name, ProfileStatus::Parked { reason }));
        }
    }
    reg.profiles = next;
    let dropped: Vec<RowKey> = reg
        .rows
        .values()
        .filter(|entry| {
            entry.row.backend == FleetBackend::Hermes
                && !scope.contains(&entry.row.key.profile)
                && entry.view.is_none()
                && !entry.row.foreground
        })
        .map(|entry| entry.row.key.clone())
        .collect();
    for key in dropped {
        reg.remove_row(&key, dirty);
    }
    reg.memo.retain(|name, _| scope.contains(name));
    reg.scope = Some(scope);
}

#[allow(clippy::too_many_arguments)]
fn on_roster(
    reg: &mut FleetRegistry,
    profile: &str,
    request: u64,
    rows: Vec<RosterRow>,
    malformed: usize,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    if !reg.in_scope(profile) {
        return;
    }
    let memo = reg.memo.entry(profile.to_string()).or_default();
    if !bump(&mut memo.roster_request, request) {
        return;
    }
    memo.listed_once = true;
    let (no_liveness, no_approvals) = (memo.no_liveness, memo.no_approvals);
    if malformed > 0 && !memo.roster_malformed_noted {
        memo.roster_malformed_noted = true;
        reg.diagnostics.note(
            format!("{malformed} list rows in profile {profile} could not be read"),
            effects,
        );
    }
    reg.set_profile(
        profile,
        ProfileStatus::Listed {
            truncated: rows.len() >= ROSTER_LIMIT,
        },
    );
    let mut listed: HashSet<String> = HashSet::with_capacity(rows.len());
    for listed_row in rows {
        let key = RowKey {
            profile: profile.to_string(),
            session_id: listed_row.session_id.clone(),
        };
        listed.insert(listed_row.session_id.clone());
        let created = !reg.rows.contains_key(&key);
        if created {
            reg.create_row(key.clone(), at, dirty);
        }
        let kept = reg
            .memo
            .get(profile)
            .and_then(|memo| memo.live.get(&listed_row.session_id))
            .cloned();
        let FleetRegistry {
            rows,
            queue,
            diagnostics,
            next_probe_id,
            ..
        } = reg;
        touch(rows, &key, dirty, |entry| {
            let was_unlisted = entry.row.unlisted;
            let view_title = entry.view.as_ref().and_then(|view| view.title.clone());
            let view_model = entry.view.as_ref().and_then(|view| view.model.clone());
            entry.row.title = view_title.unwrap_or(listed_row.title);
            entry.row.model = view_model.or(listed_row.model);
            entry.row.source = listed_row.source;
            entry.row.started_at = listed_row.started_at;
            entry.row.message_count = listed_row.message_count;
            entry.row.unlisted = false;
            entry.row.gone = false;
            entry.row.stale = false;
            entry.misses = 0;
            entry.roster_running = listed_row.running;
            if no_liveness {
                let status = if listed_row.running {
                    LiveStatus::Working
                } else {
                    LiveStatus::Idle
                };
                let count = listed_row.message_count;
                apply_status(
                    entry,
                    status,
                    count,
                    true,
                    at,
                    queue,
                    diagnostics,
                    next_probe_id,
                    effects,
                );
            } else if created {
                if let Some(item) = kept {
                    apply_status(
                        entry,
                        item.status,
                        item.message_count,
                        no_approvals,
                        at,
                        queue,
                        diagnostics,
                        next_probe_id,
                        effects,
                    );
                    seed_idle_since(entry, true, item.last_active, at);
                }
            } else if was_unlisted
                && entry.view.is_none()
                && entry.latest == Some(LiveStatus::Waiting)
                && !no_approvals
                && !queue.has_items(&entry.row.key)
            {
                effects.push(FleetEffect::FetchApprovals {
                    key: entry.row.key.clone(),
                });
            }
        });
    }
    for key in reg.keys_of(profile) {
        if listed.contains(&key.session_id) {
            continue;
        }
        let Some(entry) = reg.rows.get_mut(&key) else {
            continue;
        };
        if entry.row.stale {
            let was_visible = entry.visible();
            entry.row.stale = false;
            if was_visible {
                *dirty = true;
            }
        }
        if entry.view.is_some() || entry.row.unlisted || entry.row.foreground {
            continue;
        }
        entry.misses = entry.misses.saturating_add(1);
        if entry.misses < 2 {
            continue;
        }
        if entry.row.outcome.is_some() && finished(entry.row.state) {
            if !entry.row.gone {
                entry.row.gone = true;
                *dirty = true;
            }
        } else {
            reg.remove_row(&key, dirty);
        }
    }
    reg.cap_held(dirty);
}

fn on_roster_failed(
    reg: &mut FleetRegistry,
    profile: &str,
    request: u64,
    error: &CallError,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    if !reg.in_scope(profile) {
        return;
    }
    let memo = reg.memo.entry(profile.to_string()).or_default();
    if !bump(&mut memo.roster_request, request) {
        return;
    }
    let reason = reason_text(error);
    reg.diagnostics.note(
        format!("session.list failed in profile {profile}: {reason}"),
        effects,
    );
    reg.set_profile(profile, ProfileStatus::Failed { reason });
    mark_stale(reg, profile, dirty);
}

fn mark_stale(reg: &mut FleetRegistry, profile: &str, dirty: &mut bool) {
    for key in reg.keys_of(profile) {
        touch(&mut reg.rows, &key, dirty, |entry| entry.row.stale = true);
    }
}

#[allow(clippy::too_many_arguments)]
fn on_liveness(
    reg: &mut FleetRegistry,
    profile: &str,
    request: u64,
    items: Vec<LiveItem>,
    malformed: usize,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    if !reg.in_scope(profile) {
        return;
    }
    let memo = reg.memo.entry(profile.to_string()).or_default();
    if !bump(&mut memo.liveness_request, request) {
        return;
    }
    if malformed > 0 && !memo.live_malformed_noted {
        memo.live_malformed_noted = true;
        reg.diagnostics.note(
            format!("{malformed} live items in profile {profile} could not be read"),
            effects,
        );
    }
    memo.live = items
        .iter()
        .map(|item| (item.session_id.clone(), item.clone()))
        .collect();
    let (listed_once, no_approvals) = (memo.listed_once, memo.no_approvals);
    reg.ensure_profile(profile);
    let clear_stale = matches!(
        reg.profile_status(profile),
        Some(ProfileStatus::Listed { .. })
    );
    let mut present: HashSet<String> = HashSet::with_capacity(items.len());
    let mut unmatched = 0usize;
    for item in items {
        let key = RowKey {
            profile: profile.to_string(),
            session_id: item.session_id.clone(),
        };
        present.insert(item.session_id.clone());
        if !reg.rows.contains_key(&key) {
            if item.status != LiveStatus::Waiting {
                if listed_once {
                    unmatched += 1;
                }
                continue;
            }
            reg.create_row(key.clone(), at, dirty);
            if let Some(entry) = reg.rows.get_mut(&key) {
                entry.row.unlisted = true;
                entry.row.title = item.title.clone();
                entry.row.model = item.model.clone();
                entry.row.message_count = item.message_count;
            }
        }
        let FleetRegistry {
            rows,
            queue,
            diagnostics,
            next_probe_id,
            ..
        } = reg;
        touch(rows, &key, dirty, |entry| {
            if clear_stale {
                entry.row.stale = false;
            }
            let first_sight = entry.last_live_count.is_none();
            apply_status(
                entry,
                item.status,
                item.message_count,
                no_approvals,
                at,
                queue,
                diagnostics,
                next_probe_id,
                effects,
            );
            seed_idle_since(entry, first_sight, item.last_active, at);
        });
    }
    if unmatched > 0
        && let Some(memo) = reg.memo.get_mut(profile)
        && !memo.unmatched_noted
    {
        memo.unmatched_noted = true;
        reg.diagnostics.note(
            format!("{unmatched} live sessions in {profile} are not in its list"),
            effects,
        );
    }
    for key in reg.keys_of(profile) {
        if present.contains(&key.session_id) {
            continue;
        }
        let FleetRegistry { rows, queue, .. } = reg;
        touch(rows, &key, dirty, |entry| {
            if clear_stale {
                entry.row.stale = false;
            }
            entry.latest = None;
            if entry.view.is_some() || !entry.turn_seen {
                return;
            }
            // The session left the live listing while a turn was seen: closed, so no probe.
            entry.turn_seen = false;
            entry.probe = None;
            queue.settle_polled(&entry.row.key);
            if entry.row.foreground {
                set_state(entry, RowState::Idle, at);
            } else {
                entry.row.outcome = Some(Outcome::Unknown);
                set_state(entry, RowState::Done, at);
                effects.push(finish_attention(entry, Outcome::Unknown));
            }
        });
    }
    let idle_unlisted: Vec<RowKey> = reg
        .rows
        .values()
        .filter(|entry| {
            entry.row.key.profile == profile
                && entry.row.unlisted
                && entry.view.is_none()
                && entry.row.state == RowState::Idle
                && entry.probe.is_none()
        })
        .map(|entry| entry.row.key.clone())
        .collect();
    for key in idle_unlisted {
        reg.remove_row(&key, dirty);
    }
    reg.cap_held(dirty);
}

fn on_liveness_failed(
    reg: &mut FleetRegistry,
    profile: &str,
    request: u64,
    error: &CallError,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    if !reg.in_scope(profile) {
        return;
    }
    let memo = reg.memo.entry(profile.to_string()).or_default();
    if !bump(&mut memo.liveness_request, request) {
        return;
    }
    if *error == CallError::UnknownMethod {
        memo.no_liveness = true;
        memo.no_approvals = true;
        reg.diagnostics.note(
            format!(
                "session.active_list is unknown in profile {profile}; its rows follow the list"
            ),
            effects,
        );
        for key in reg.keys_of(profile) {
            let FleetRegistry {
                rows,
                queue,
                diagnostics,
                next_probe_id,
                ..
            } = reg;
            touch(rows, &key, dirty, |entry| {
                let status = if entry.roster_running {
                    LiveStatus::Working
                } else {
                    LiveStatus::Idle
                };
                let count = entry.row.message_count;
                apply_status(
                    entry,
                    status,
                    count,
                    true,
                    at,
                    queue,
                    diagnostics,
                    next_probe_id,
                    effects,
                );
            });
        }
        return;
    }
    reg.diagnostics.note(
        format!(
            "session.active_list failed in profile {profile}: {}",
            reason_text(error)
        ),
        effects,
    );
    mark_stale(reg, profile, dirty);
}

/// The row an approvals reply may change: unsubscribed and last read waiting.
fn polled_waiting(reg: &FleetRegistry, key: &RowKey) -> bool {
    reg.rows
        .get(key)
        .is_some_and(|entry| entry.view.is_none() && entry.latest == Some(LiveStatus::Waiting))
}

fn on_approvals(
    reg: &mut FleetRegistry,
    key: &RowKey,
    request: u64,
    approvals: Vec<PolledApproval>,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    let stored = reg.approvals_request.entry(key.clone()).or_insert(0);
    if !bump(stored, request) || !polled_waiting(reg, key) {
        return;
    }
    let FleetRegistry {
        rows,
        queue,
        diagnostics,
        ..
    } = reg;
    touch(rows, key, dirty, |entry| {
        let mut named: HashSet<String> = HashSet::new();
        for approval in approvals {
            let Some(prompt_id) = approval.prompt_id else {
                diagnostics.note(
                    format!("an approval in profile {} has no request id", key.profile),
                    effects,
                );
                continue;
            };
            named.insert(prompt_id.clone());
            let id = NeedId {
                key: key.clone(),
                slot: NeedSlot::Prompt(prompt_id),
            };
            let detail = NeedDetail::Known(NeedSummary::Approval {
                command: approval.command.unwrap_or_default(),
                description: approval.description.unwrap_or_default(),
            });
            match queue.get(&id) {
                Some(item) if item.exact => {}
                Some(_) => queue.modify(&id, |item| {
                    item.detail = detail;
                    item.choices = approval.choices;
                }),
                None => queue.open(
                    NeedItem {
                        id,
                        seq: 0,
                        since: at,
                        detail,
                        choices: approval.choices,
                        execution_generation: None,
                        timeout_s: None,
                        exact: false,
                        foreground: entry.row.foreground,
                        external_options: Vec::new(),
                    },
                    effects,
                ),
            }
        }
        let removed = queue.remove_where(key, |item| {
            !item.exact
                && matches!(&item.id.slot, NeedSlot::Prompt(id) if !named.contains(id))
                && matches!(item.detail, NeedDetail::Known(NeedSummary::Approval { .. }))
        });
        if !queue.has_named(key) && !removed && !queue.has_items(key) {
            queue.open(waiting_item(entry, at), effects);
        }
        refresh_state(entry, queue, at);
    });
}

fn on_approvals_failed(
    reg: &mut FleetRegistry,
    key: &RowKey,
    request: u64,
    error: &CallError,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    let stored = reg.approvals_request.entry(key.clone()).or_insert(0);
    if !bump(stored, request) || !polled_waiting(reg, key) || *error == CallError::NotFound {
        return;
    }
    if *error == CallError::UnknownMethod {
        reg.memo
            .entry(key.profile.clone())
            .or_default()
            .no_approvals = true;
        reg.diagnostics.note(
            format!(
                "approval.pending is unknown in profile {}; waiting sessions show no detail",
                key.profile
            ),
            effects,
        );
    } else {
        reg.diagnostics.note(
            format!(
                "approval.pending failed in profile {}: {}",
                key.profile,
                reason_text(error)
            ),
            effects,
        );
    }
    let FleetRegistry { rows, queue, .. } = reg;
    touch(rows, key, dirty, |entry| {
        if !queue.has_items(key) {
            queue.open(waiting_item(entry, at), effects);
        }
        refresh_state(entry, queue, at);
    });
}

#[allow(clippy::too_many_arguments)]
fn on_probed(
    reg: &mut FleetRegistry,
    key: &RowKey,
    probe_id: u64,
    outcome: Outcome,
    found_complete: bool,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    let FleetRegistry { rows, queue, .. } = reg;
    touch(rows, key, dirty, |entry| {
        let Some((current, reason)) = entry.probe else {
            return;
        };
        if current != probe_id {
            return;
        }
        entry.probe = None;
        if entry.view.is_some() {
            return;
        }
        if matches!(reason, ProbeReason::CountRose { .. }) && !found_complete {
            return;
        }
        if entry.row.foreground {
            refresh_state(entry, queue, at);
            if entry.row.state != RowState::NeedsYou {
                set_state(entry, RowState::Idle, at);
            }
            return;
        }
        entry.row.outcome = Some(outcome);
        let state = if queue.has_items(key) {
            RowState::NeedsYou
        } else if outcome == Outcome::Failed {
            RowState::Failed
        } else {
            RowState::Done
        };
        set_state(entry, state, at);
        effects.push(finish_attention(entry, outcome));
    });
    let idle_unlisted = reg.rows.get(key).is_some_and(|entry| {
        entry.row.unlisted && entry.row.state == RowState::Idle && entry.probe.is_none()
    });
    if idle_unlisted {
        reg.remove_row(key, dirty);
    }
}

fn exact_item(key: &RowKey, need: &SubscribedNeed, at: f64, foreground: bool) -> NeedItem {
    NeedItem {
        id: NeedId {
            key: key.clone(),
            slot: need.slot.clone(),
        },
        seq: 0,
        since: at,
        detail: NeedDetail::Known(need.summary.clone()),
        choices: need.choices.clone(),
        execution_generation: need.execution_generation,
        timeout_s: need.timeout_s,
        exact: true,
        foreground,
        external_options: Vec::new(),
    }
}

fn on_external(
    reg: &mut FleetRegistry,
    session: ExternalSession,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    let key = session.key.clone();
    if !reg.rows.contains_key(&key) {
        reg.create_row(key.clone(), session.at, dirty);
    }
    let FleetRegistry { rows, queue, .. } = reg;
    touch(rows, &key, dirty, |entry| {
        let previous = entry.row.state;
        entry.row.backend = session.backend;
        entry.row.agent = Some(crate::text::sanitize(&session.agent).into_owned());
        entry.row.title = crate::text::sanitize(&session.title).into_owned();
        entry.row.model = session
            .model
            .as_deref()
            .map(|s| crate::text::sanitize(s).into_owned());
        entry.row.activity = session
            .activity
            .as_deref()
            .map(|s| crate::text::sanitize(s).into_owned());
        entry.row.occupancy = session.used.zip(session.size);
        entry.row.cost = session
            .cost
            .as_ref()
            .map(|(amount, currency)| (*amount, crate::text::sanitize(currency).into_owned()));
        entry.row.subscribed = true;
        entry.row.stale = false;
        entry.row.gone = false;
        entry.row.unlisted = false;
        let mut wanted = HashSet::new();
        for need in &session.needs {
            let id = NeedId {
                key: key.clone(),
                slot: need.slot.clone(),
            };
            wanted.insert(id.clone());
            let options = need
                .options
                .iter()
                .map(|option| super::ExternalOption {
                    id: option.id.clone(),
                    name: crate::text::sanitize(&option.name).into_owned(),
                    kind: crate::text::sanitize(&option.kind).into_owned(),
                })
                .collect::<Vec<_>>();
            let detail = NeedDetail::Known(NeedSummary::Question(
                crate::text::sanitize(&need.text).into_owned(),
            ));
            let choices = options
                .iter()
                .map(|option| option.name.clone())
                .collect::<Vec<_>>();
            if queue.get(&id).is_some() {
                queue.modify(&id, |held| {
                    held.detail = detail;
                    held.choices = choices;
                    held.external_options = options;
                });
            } else {
                queue.open(
                    NeedItem {
                        id,
                        seq: 0,
                        since: session.at,
                        detail,
                        choices,
                        execution_generation: None,
                        timeout_s: None,
                        exact: true,
                        foreground: entry.row.foreground,
                        external_options: options,
                    },
                    effects,
                );
            }
        }
        queue.remove_where(&key, |item| !wanted.contains(&item.id));
        let next = if queue.has_items(&key) {
            RowState::NeedsYou
        } else {
            session.state
        };
        set_state(entry, next, session.at);
        entry.row.outcome = session.outcome;
        if running(previous) && finished(next) && !entry.row.foreground {
            effects.push(finish_attention(
                entry,
                session.outcome.unwrap_or(Outcome::Unknown),
            ));
        }
    });
}

fn on_subscribed(
    reg: &mut FleetRegistry,
    key: RowKey,
    view: SubscribedView,
    at: f64,
    effects: &mut Vec<FleetEffect>,
    dirty: &mut bool,
) {
    if !reg.rows.contains_key(&key) {
        reg.create_row(key.clone(), at, dirty);
    }
    let FleetRegistry { rows, queue, .. } = reg;
    touch(rows, &key, dirty, |entry| {
        if entry.view.as_ref() == Some(&view) {
            return;
        }
        entry.row.subscribed = true;
        entry.probe = None;
        entry.row.activity = view.activity.clone();
        entry.row.cost_usd = view.cost_usd;
        if let Some(model) = &view.model {
            entry.row.model = Some(model.clone());
        }
        if let Some(title) = &view.title {
            entry.row.title = title.clone();
        }
        let foreground = entry.row.foreground;
        let mut wanted: HashSet<NeedId> = HashSet::new();
        for need in &view.needs {
            let item = exact_item(&key, need, at, foreground);
            wanted.insert(item.id.clone());
            if queue.get(&item.id).is_some() {
                queue.modify(&item.id, |held| {
                    held.detail = item.detail;
                    held.choices = item.choices;
                    held.execution_generation = item.execution_generation;
                    held.timeout_s = item.timeout_s;
                    held.exact = true;
                });
            } else {
                queue.open(item, effects);
            }
        }
        queue.remove_where(&key, |item| !wanted.contains(&item.id));
        let was_running = entry.view_running;
        let has_items = queue.has_items(&key);
        match &view.turn {
            SubscribedTurn::Running { started_at } => {
                entry.view_running = true;
                if has_items {
                    set_state(entry, RowState::NeedsYou, at);
                } else if entry.row.state != RowState::Working {
                    set_state(entry, RowState::Working, at);
                    if let Some(started) = started_at {
                        entry.row.state_since = *started;
                    }
                }
            }
            SubscribedTurn::Finished(outcome) => {
                entry.view_running = false;
                if was_running && !foreground {
                    entry.row.outcome = Some(*outcome);
                    effects.push(finish_attention(entry, *outcome));
                }
                let state = if has_items {
                    RowState::NeedsYou
                } else {
                    rest_state(entry)
                };
                set_state(entry, state, at);
            }
            SubscribedTurn::Idle => {
                entry.view_running = false;
                let state = if has_items {
                    RowState::NeedsYou
                } else {
                    rest_state(entry)
                };
                set_state(entry, state, at);
            }
        }
        entry.view = Some(view);
    });
}

fn on_unsubscribed(reg: &mut FleetRegistry, key: &RowKey, dirty: &mut bool) {
    let FleetRegistry { rows, queue, .. } = reg;
    touch(rows, key, dirty, |entry| {
        let Some(view) = entry.view.take() else {
            return;
        };
        entry.row.subscribed = false;
        entry.row.activity = None;
        entry.view_running = false;
        entry.turn_seen =
            matches!(view.turn, SubscribedTurn::Running { .. }) || !view.needs.is_empty();
        queue.for_row(key, |item| {
            item.exact = false;
            item.execution_generation = None;
        });
    });
}

fn on_foreground(reg: &mut FleetRegistry, key: Option<RowKey>, at: f64, dirty: &mut bool) {
    if reg.foreground == key {
        return;
    }
    if let Some(old) = reg.foreground.take() {
        touch(&mut reg.rows, &old, dirty, |entry| {
            entry.row.foreground = false
        });
        reg.queue.for_row(&old, |item| item.foreground = false);
    }
    reg.foreground = key.clone();
    if let Some(new) = key {
        // The foreground session always has a row, listed or not yet.
        if !reg.rows.contains_key(&new) {
            reg.create_row(new.clone(), at, dirty);
        }
        touch(&mut reg.rows, &new, dirty, |entry| {
            entry.row.foreground = true;
            if finished(entry.row.state) {
                entry.row.outcome = None;
                set_state(entry, RowState::Idle, at);
            }
        });
        reg.queue.for_row(&new, |item| item.foreground = true);
    }
}

fn on_acknowledge(reg: &mut FleetRegistry, key: &RowKey, at: f64, dirty: &mut bool) {
    let Some(entry) = reg.rows.get(key) else {
        return;
    };
    if entry.row.gone || (entry.row.unlisted && finished(entry.row.state)) {
        reg.remove_row(key, dirty);
        return;
    }
    touch(&mut reg.rows, key, dirty, |entry| {
        if finished(entry.row.state) {
            entry.row.outcome = None;
            set_state(entry, RowState::Idle, at);
        }
    });
}

fn on_connection_lost(reg: &mut FleetRegistry, dirty: &mut bool) {
    let keys: Vec<RowKey> = reg.rows.keys().cloned().collect();
    for key in keys {
        touch(&mut reg.rows, &key, dirty, |entry| {
            if entry.row.backend != FleetBackend::Hermes {
                return;
            }
            entry.row.stale = true;
            // The probe's reply belongs to the old connection; ask again at the next read.
            match entry.probe.take() {
                Some((_, ProbeReason::TurnEnded)) => entry.turn_seen = true,
                Some((_, ProbeReason::CountRose { before })) => entry.last_live_count = before,
                None => {}
            }
        });
    }
    for (_, status) in &mut reg.profiles {
        if matches!(status, ProfileStatus::Listed { .. }) {
            *status = ProfileStatus::Loading;
        }
    }
}
