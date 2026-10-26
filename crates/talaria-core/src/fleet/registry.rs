//! Pure fleet reducer.

use super::{AlertKind, FleetRow, LiveItem, NeedId, Outcome, RosterRow, RowKey, SubscribedView};

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
    ProbeOutcome { key: RowKey },
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

/// Rows, queue, and profile status. Pure.
pub struct FleetRegistry;

impl FleetRegistry {
    /// An empty registry.
    pub fn new() -> FleetRegistry {
        FleetRegistry
    }

    /// Rows in canonical order.
    pub fn rows(&self) -> Vec<&FleetRow> {
        todo!()
    }

    /// One row.
    pub fn row(&self, key: &RowKey) -> Option<&FleetRow> {
        let _ = key;
        todo!()
    }

    /// Queue items, ascending `seq`.
    pub fn queue(&self) -> Vec<&super::NeedItem> {
        todo!()
    }

    /// Background counts.
    pub fn counts(&self) -> FleetCounts {
        todo!()
    }

    /// Served profiles in served order, then parked.
    pub fn profiles(&self) -> Vec<(&str, &ProfileStatus)> {
        todo!()
    }

    /// Profiles with a needs-you or working row, or a probe in flight.
    pub fn hot_profiles(&self) -> Vec<String> {
        todo!()
    }

    /// Distinct diagnostic texts, oldest first, at most 20.
    pub fn diagnostics(&self) -> Vec<&str> {
        todo!()
    }

    /// Rises by one exactly when rows, the queue, counts, or profiles changed.
    pub fn version(&self) -> u64 {
        todo!()
    }
}

impl Default for FleetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply one input. Reads no clock.
pub fn update(reg: &mut FleetRegistry, input: FleetInput) -> Vec<FleetEffect> {
    let _ = (reg, input);
    todo!()
}
