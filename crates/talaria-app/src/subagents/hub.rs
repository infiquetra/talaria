//! The subagent hub and its messages. Issue #242.

use hermes_client::{CallError, Client};
use hermes_contract::events::Notification;
use hermes_contract::schemas::{DelegationStatusResult, SubagentListResult, SubagentTailResult};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Clock, Spawning, SubagentCounts, SubagentTree};

use super::AppSender;

/// Settings from `[subagents]`: list poll seconds, tail poll milliseconds,
/// dock rows. Defaults 5, 2000, 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubagentsConfig {
    pub poll_s: u64,
    pub tail_poll_ms: u64,
    pub dock_rows: u16,
}

impl Default for SubagentsConfig {
    fn default() -> Self {
        Self {
            poll_s: 5,
            tail_poll_ms: 2000,
            dock_rows: 3,
        }
    }
}

/// One control the panel or `/agents` asks the hub to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubagentAction {
    Steer { id: String, text: String },
    Stop(String),
    StopAll,
    ConfirmStopAll,
    RequestPause,
    ConfirmPause(bool),
    Resume,
    OpenTail(String),
    CloseTail,
    Refresh,
}

/// What one control came back with. `goal` is the row's goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    SteerQueued {
        goal: String,
    },
    SteerRejected {
        goal: String,
    },
    StopRequested {
        goal: String,
    },
    AlreadyFinished {
        goal: String,
    },
    NotStarted {
        goal: String,
    },
    AlreadyStopping {
        goal: String,
    },
    StopAllSent {
        sent: u32,
        of: u32,
    },
    Paused(bool),
    Refused {
        method: &'static str,
        reason: String,
    },
    MethodMissing {
        method: &'static str,
    },
    Unknown {
        action: &'static str,
        secs: u64,
    },
    Lost {
        action: &'static str,
    },
    NotSent {
        action: &'static str,
    },
}

/// The hub's own messages, carried by `AppMessage::Subagents`.
#[derive(Clone, Debug)]
pub enum SubagentMsg {
    Listed {
        key: RowKey,
        generation: u64,
        result: Result<SubagentListResult, CallError>,
    },
    Tail {
        key: RowKey,
        id: String,
        result: Result<SubagentTailResult, CallError>,
    },
    Control {
        key: RowKey,
        outcome: ControlOutcome,
    },
    DelegationStatus {
        result: Result<DelegationStatusResult, CallError>,
    },
    PollTick {
        key: RowKey,
    },
    TailTick {
        key: RowKey,
        id: String,
    },
    ElapsedTick,
}

/// One tracker per attached session: tree, client, poll and tail state.
pub struct SubagentHub {
    /// Reserved for the trackers. The stub holds nothing yet.
    _reserved: (),
}

impl SubagentHub {
    #[must_use]
    pub fn new(_cfg: SubagentsConfig, _tx: AppSender) -> SubagentHub {
        todo!("#242")
    }

    pub fn attach(&mut self, _key: RowKey, _client: Client) {
        todo!("#242")
    }

    pub fn detach(&mut self, _key: &RowKey) {
        todo!("#242")
    }

    /// Feed one notification of an attached session. True when visible state
    /// changed. The hub also takes `message.start` here, to prune.
    pub fn on_notification(&mut self, _key: &RowKey, _n: &Notification, _now: Clock) -> bool {
        todo!("#242")
    }

    /// A reconnect or a resolved replay gap: list again, reset failures.
    pub fn on_reconnected(&mut self, _key: &RowKey) {
        todo!("#242")
    }

    /// Feed one of the hub's own messages. True when visible state changed.
    pub fn on_message(&mut self, _msg: SubagentMsg, _now: Clock) -> bool {
        todo!("#242")
    }

    /// Run one panel or `/agents` control.
    pub fn act(&mut self, _key: &RowKey, _action: SubagentAction, _now: Clock) {
        todo!("#242")
    }

    #[must_use]
    pub fn tree(&self, _key: &RowKey) -> Option<&SubagentTree> {
        todo!("#242")
    }

    #[must_use]
    pub fn counts(&self, _key: &RowKey) -> Option<SubagentCounts> {
        todo!("#242")
    }

    pub fn open_panel(&mut self, _key: RowKey) {
        todo!("#242")
    }

    pub fn close_panel(&mut self) {
        todo!("#242")
    }

    #[must_use]
    pub fn spawning(&self) -> Spawning {
        todo!("#242")
    }
}

/// The exact flash and notice text for one outcome.
#[must_use]
pub fn outcome_text(_o: &ControlOutcome) -> String {
    todo!("#242")
}
