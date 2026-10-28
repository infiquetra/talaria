//! In-process event feed. Terminal views attach on their own connection.
use hermes_contract::{events::EventFrame, schemas::CanonicalSnapshot};

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AgentFeedItem {
    Event {
        profile: String,
        session_id: String,
        request_id: String,
        label: String,
        frame: EventFrame,
    },
    Gap {
        profile: String,
        session_id: String,
        reason: String,
    },
    Resynced {
        profile: String,
        session_id: String,
        snapshot: Box<CanonicalSnapshot>,
    },
    Gone {
        profile: String,
        session_id: String,
    },
}

pub type AgentFeedReceiver = tokio::sync::mpsc::UnboundedReceiver<AgentFeedItem>;
