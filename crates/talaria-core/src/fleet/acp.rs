//! Pure projection of an agent session into fleet state.

use super::{Outcome, RowState};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AcpSessionKey {
    pub agent_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcpUsage {
    pub used: u64,
    pub size: u64,
    pub cost_amount: Option<f64>,
    pub cost_currency: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpCell {
    Text(String),
    Thought(String),
    Tool {
        id: String,
        title: String,
        body: String,
        status: Option<String>,
    },
    Plan(String),
    Notice(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AcpUpdate {
    MessageChunk {
        key: AcpSessionKey,
        text: String,
    },
    ThoughtChunk {
        key: AcpSessionKey,
        text: String,
    },
    ToolCall {
        key: AcpSessionKey,
        id: String,
        title: String,
    },
    ToolCallPatch {
        key: AcpSessionKey,
        id: String,
        title: Option<String>,
        body: Option<String>,
        status: Option<String>,
    },
    Plan {
        key: AcpSessionKey,
        summary: String,
    },
    AvailableCommands {
        key: AcpSessionKey,
        names: Vec<String>,
    },
    Mode {
        key: AcpSessionKey,
        name: String,
    },
    ConfigOption {
        key: AcpSessionKey,
        name: String,
        value: String,
    },
    SessionInfo {
        key: AcpSessionKey,
        title: String,
    },
    Notice {
        key: AcpSessionKey,
        text: String,
    },
    Compaction {
        key: AcpSessionKey,
        text: String,
    },
    Usage {
        key: AcpSessionKey,
        usage: AcpUsage,
    },
    UsageFields {
        key: AcpSessionKey,
        used: Option<u64>,
        size: Option<u64>,
        cost_amount: Option<f64>,
        cost_currency: Option<String>,
    },
    Unknown {
        key: AcpSessionKey,
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub key: AcpSessionKey,
    pub responder: String,
    pub tool_call_id: String,
    pub tool_title: String,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcpMapping {
    pub transcript: Vec<AcpCell>,
    pub usage: Option<AcpUsage>,
    pub title: Option<String>,
    pub state: RowState,
    pub outcome: Option<Outcome>,
    pub diagnostics: Vec<String>,
    pub needs: Vec<PermissionRequest>,
}

impl Default for AcpMapping {
    fn default() -> Self {
        Self {
            transcript: Vec::new(),
            usage: None,
            title: None,
            state: RowState::Idle,
            outcome: None,
            diagnostics: Vec::new(),
            needs: Vec::new(),
        }
    }
}

impl AcpMapping {
    pub fn start_turn(&mut self) {
        todo!("add agent session mapping")
    }
    pub fn apply(&mut self, _update: AcpUpdate) {
        todo!("add agent session mapping")
    }
    pub fn finish(&mut self, _stop_reason: &str) {
        todo!("add agent session mapping")
    }
    pub fn request_permission(&mut self, _request: PermissionRequest) {
        todo!("add agent session mapping")
    }
}
