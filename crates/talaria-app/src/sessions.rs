//! Session calls: create, resume, submit, interrupt, recover, and detach.

use std::path::Path;
use std::time::Duration;

use hermes_client::Client;
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_client::rpc::CallError;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use talaria_core::session::{Effect, SessionState};

use crate::launch::{ListRow, NewSession, SessionRequest};

/// `session.detach` gets two seconds; a quit must not wait longer for it.
pub const DETACH_TIMEOUT: Duration = Duration::from_secs(2);

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
    /// The id the resume hint prints.
    pub stored_session_id: String,
    pub subscription_id: String,
    pub info: SessionInfoSummary,
    /// `info.title`, when the gateway sent one.
    pub title: Option<String>,
    pub cursor: Cursor,
}

impl OpenedSession {
    /// Build the session from a create or resume result.
    #[must_use]
    pub fn from_snapshot(snapshot: CanonicalSnapshot) -> Self {
        let _ = snapshot;
        todo!("binary")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    NotFound,
    NotInFolder {
        session_cwd: String,
    },
    /// `-c` found a session whose folder the gateway did not report; it was detached again.
    FolderUnknown,
    Refused {
        reason: String,
    },
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
    /// Nothing was detached: there was no session, or the session or link was already gone.
    Skipped,
}

#[derive(Clone)]
pub struct SessionService {
    launch_id: String,
    client: Option<Client>,
}

impl SessionService {
    /// A service with no connection. Every call fails as not connected.
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        Self {
            launch_id: launch_id.into(),
            client: None,
        }
    }

    /// A service that calls through `client`, the link's one client.
    #[must_use]
    pub fn with_client(launch_id: impl Into<String>, client: Client) -> Self {
        Self {
            launch_id: launch_id.into(),
            client: Some(client),
        }
    }

    #[must_use]
    pub fn launch_id(&self) -> &str {
        &self.launch_id
    }

    #[must_use]
    pub fn client(&self) -> Option<&Client> {
        self.client.as_ref()
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

    /// `session.list` for one scope, read loosely: rows without a session id are skipped.
    pub async fn list(&self, scope: &str) -> Result<Vec<ListRow>, CallError> {
        let _ = scope;
        todo!("binary")
    }
}

/// The rows of a `session.list` reply.
#[must_use]
pub fn list_rows(reply: &Value) -> Vec<ListRow> {
    let _ = reply;
    todo!("binary")
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
