//! Session calls: create, resume, submit, interrupt, recover, and detach.

use std::path::Path;

use hermes_client::replay::{Cursor, ReplayGap};
use serde_json::Value;
use talaria_core::session::{Effect, SessionState};

use crate::launch::{NewSession, SessionRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfoSummary {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub cwd: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenedSession {
    pub state: SessionState,
    pub session_id: String,
    pub subscription_id: String,
    pub info: SessionInfoSummary,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NotFound,
    NotInFolder { session_cwd: String },
    Refused { reason: String },
    Unknown,
    Link(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecuteOutcome {
    Sent,
    Refused { reason: String },
    OutcomeUnknown,
    VoiceStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachOutcome {
    Detached,
    StaleSubscription,
    Failed,
}

pub struct SessionService {
    launch_id: String,
}

impl SessionService {
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        Self {
            launch_id: launch_id.into(),
        }
    }

    #[must_use]
    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    pub async fn open(&self, request: SessionRequest) -> Result<OpenedSession, OpenError> {
        let _ = request;
        todo!("binary")
    }

    pub async fn execute(&self, effect: Effect) -> ExecuteOutcome {
        let _ = effect;
        todo!("binary")
    }

    pub async fn recover(&self, gap: ReplayGap) -> Result<OpenedSession, OpenError> {
        let _ = gap;
        todo!("binary")
    }

    pub async fn detach(&self, opened: &OpenedSession) -> DetachOutcome {
        let _ = opened;
        todo!("binary")
    }
}

#[must_use]
pub fn create_params(new: &NewSession) -> Value {
    let _ = new;
    todo!("binary")
}

#[must_use]
pub fn resume_params_latest(workspace: &Path) -> Value {
    let _ = workspace;
    todo!("binary")
}

#[must_use]
pub fn resume_params_id(session_id: &str) -> Value {
    let _ = session_id;
    todo!("binary")
}

#[must_use]
pub fn detach_params(session_id: &str, subscription_id: &str) -> Value {
    let _ = (session_id, subscription_id);
    todo!("binary")
}

#[must_use]
pub fn submit_params(session_id: &str, text: &str, submission_id: &str) -> Value {
    let _ = (session_id, text, submission_id);
    todo!("binary")
}

#[must_use]
pub fn submission_id(launch_id: &str, n: u64) -> String {
    let _ = (launch_id, n);
    todo!("binary")
}

/// Keep an event whose epoch differs, or whose sequence is above the cursor.
#[must_use]
pub fn accept_event(cursor: &Cursor, replay_epoch: Option<&str>, seq: Option<u64>) -> bool {
    let _ = (cursor, replay_epoch, seq);
    todo!("binary")
}
