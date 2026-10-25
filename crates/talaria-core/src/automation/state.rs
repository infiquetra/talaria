//! The automation reducer: standing state in, effects out. Stubs.
//!
//! Behavior contract: issue 236.

use std::collections::BTreeSet;

/// What reached the automation state.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AutomationEvent {
    /// A foreground session attached; read everything once.
    Attached {
        /// The session id.
        session_id: String,
    },
    /// The foreground session went away with no replacement.
    Detached,
    /// A control read answered.
    ControlRead {
        /// The token from its effect.
        token: u64,
        /// The decoded view or the failure.
        result: Result<ControlView, ReadFailure>,
    },
    /// A `session.control.update` event arrived.
    ControlUpdated {
        /// The event's session id.
        session_id: String,
        /// The event payload's `control` value.
        raw: serde_json::Value,
    },
    /// A `status.update` note arrived.
    StatusNote {
        /// The event's session id.
        session_id: String,
        /// The note kind.
        kind: String,
        /// The note text.
        text: String,
    },
    /// A presented command finished, whatever its result.
    CommandFinished {
        /// The session the command was sent to.
        session_id: String,
        /// The canonical command name.
        canonical: String,
        /// What the command did.
        result: CommandResult,
    },
    /// A mode read answered.
    ModeRead {
        /// The session the read went to.
        session_id: String,
        /// Which mode was read.
        key: ModeKey,
        /// The token from its effect.
        token: u64,
        /// The configured value, if any, or the failure.
        result: Result<Option<String>, ReadFailure>,
    },
}

/// What a presented command did, from the submit path's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResult {
    /// The command ran and answered.
    Output,
    /// The command queued a turn.
    Submit,
    /// The outcome is unknown.
    Unknown,
    /// The command never ran.
    NotRun,
}

/// How a read failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadFailure {
    /// The gateway refused, with its reason.
    Refused(String),
    /// The method does not exist on this Hermes.
    MethodMissing,
    /// No reply before the deadline.
    TimedOut,
    /// The connection was lost or the reply did not decode as a value.
    Lost,
    /// A reply arrived that is not usable.
    BadResult,
}

/// Which session mode a read targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKey {
    /// The approval bypass.
    Yolo,
    /// The speed tier.
    Fast,
}

/// What the app does after one update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutomationEffect {
    /// Read the standing state.
    ReadControl {
        /// The session to read.
        session_id: String,
        /// The token the answer carries back.
        token: u64,
    },
    /// Read one session mode.
    ReadMode {
        /// The session to read.
        session_id: String,
        /// Which mode to read.
        key: ModeKey,
        /// The token the answer carries back.
        token: u64,
    },
    /// Show one transcript notice.
    Notice {
        /// The notice text.
        text: String,
    },
    /// The rows changed; draw again.
    Redraw,
}

/// A session mode value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeValue<T> {
    /// Never read.
    NotRead,
    /// Read cleanly.
    Known(T),
    /// A read failed after the value may have changed.
    Unknown,
}

/// The speed tier of a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FastMode {
    /// The normal tier.
    Normal,
    /// The fast tier.
    Fast,
    /// The ultrafast tier.
    Ultrafast,
}

/// Why a snapshot did not decode, for logs only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);

/// Whether the standing state is shown, and why not.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ControlStatus {
    /// Never read.
    #[default]
    NotRead,
    /// A read is in flight.
    Reading,
    /// Shown.
    Ready(ControlView),
    /// A refusal or a missing method hid the rows, with its reason.
    Unavailable(String),
}

/// The standing goal, loop and heartbeat of one session.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlView {
    /// The goal, if Hermes reports one.
    pub goal: Option<GoalView>,
    /// The loop, if Hermes reports one.
    pub loop_: Option<LoopView>,
    /// The heartbeat, if Hermes reports one.
    pub heartbeat: Option<HeartbeatView>,
    /// Hermes's revision of the three parts.
    pub revision: String,
}

/// A goal as the rows read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalView {
    /// The goal title.
    pub title: String,
    /// `active`, `paused` or `done`.
    pub status: String,
    /// Turns spent.
    pub turns_used: i64,
    /// The turn budget.
    pub max_turns: i64,
    /// How many subgoals stand.
    pub subgoals: usize,
    /// How many quality gates stand.
    pub gates: usize,
    /// Why it paused, if it did.
    pub paused_reason: Option<String>,
    /// What it waits on, if parked.
    pub wait: Option<WaitView>,
}

/// What a parked goal waits on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitView {
    /// A clock time, in epoch milliseconds.
    Until {
        /// The time, in epoch milliseconds.
        until_ms: u64,
        /// Why it waits.
        reason: String,
    },
    /// Another session.
    Session {
        /// The session it waits on.
        target: String,
        /// Why it waits.
        reason: String,
    },
    /// A process.
    Pid {
        /// The process it waits on.
        target: String,
        /// Why it waits.
        reason: String,
    },
}

/// A loop as the rows read it.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopView {
    /// The recurring prompt.
    pub prompt: String,
    /// `active`, `paused` or `done`.
    pub status: String,
    /// True for a self-paced loop.
    pub self_paced: bool,
    /// The cadence in seconds.
    pub interval_seconds: f64,
    /// The run cap, 0 for none.
    pub times: i64,
    /// Wakeups fired.
    pub ticks_fired: i64,
    /// The next wakeup, in epoch milliseconds.
    pub next_due_ms: u64,
    /// When it was created, in epoch milliseconds.
    pub created_ms: u64,
    /// True while a wakeup turn runs.
    pub awaiting_response: bool,
    /// True while an active goal holds it back.
    pub deferred_by_goal: bool,
    /// Why it paused, if it did.
    pub paused_reason: Option<String>,
}

/// A heartbeat as the rows read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeartbeatView {
    /// The recurring prompt.
    pub prompt: String,
    /// `active` or `paused`.
    pub status: String,
    /// The cadence in seconds.
    pub interval_seconds: i64,
    /// Pings sent.
    pub fire_count: i64,
    /// The next ping, in epoch milliseconds.
    pub next_due_ms: u64,
}

/// The automation state of one foreground session.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct AutomationState {
    session_id: Option<String>,
    control: ControlStatus,
    next_token: u64,
    pending_control: Option<u64>,
    yolo: ModeValue<bool>,
    fast: ModeValue<FastMode>,
    pending_mode: [Option<PendingMode>; 2],
    plan_noted: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct PendingMode {
    token: u64,
    after_command: bool,
}

#[allow(clippy::derivable_impls)]
impl<T> Default for ModeValue<T> {
    fn default() -> Self {
        ModeValue::NotRead
    }
}

impl AutomationState {
    /// Both mode values, for the rows and for sibling readers.
    pub fn modes(&self) -> (ModeValue<bool>, ModeValue<FastMode>) {
        let _ = self;
        todo!("#236")
    }
}

impl ControlView {
    /// Decode through the generated snapshot type, then check the epoch times.
    pub fn decode(value: &serde_json::Value) -> Result<ControlView, DecodeError> {
        let _ = value;
        todo!("#236")
    }
}

/// Fold one event into the state, answering the app's effects.
pub fn update(state: &mut AutomationState, event: AutomationEvent) -> Vec<AutomationEffect> {
    let _ = (state, event);
    todo!("#236")
}
