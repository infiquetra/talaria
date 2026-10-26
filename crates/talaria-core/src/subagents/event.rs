//! One subagent event, decoded from a gateway notification. Issue #242.

use hermes_contract::events::Notification;
use hermes_contract::schemas::SubagentEventPayload;

/// Which of the six `subagent.*` notifications an event came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentEventKind {
    SpawnRequested,
    Start,
    Progress,
    Thinking,
    Tool,
    Complete,
}

/// A decoded `subagent.*` notification, ready for the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentEvent {
    pub kind: SubagentEventKind,
    pub payload: SubagentEventPayload,
}

impl SubagentEvent {
    /// Decode the six `subagent.*` variants; `None` for every other
    /// notification or a payload that fails its contract shape.
    #[must_use]
    pub fn from_notification(_n: &Notification) -> Option<SubagentEvent> {
        todo!("#242")
    }
}
