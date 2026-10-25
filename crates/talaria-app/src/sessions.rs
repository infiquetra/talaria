//! Session calls: create, resume, submit, interrupt, recover, and detach.
//!
//! Requests are built by hand with exactly the keys Talaria sends and go out through
//! `Client::call_raw`, so the wire form is fixed here; results are read into the contract types.

use std::path::Path;
use std::time::Duration;

use hermes_client::Client;
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_client::rpc::{CallError, CallOptions};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Map, Value, json};
use talaria_core::session::{Effect, SessionState};

use crate::launch::{Belongs, NewSession, SESSION_SOURCE, SessionRequest, belongs};

/// `session.detach` gets two seconds; a quit must not wait longer for it.
pub const DETACH_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a create retry waits for the link to reconnect after a lost reply.
const RECONNECT_WAIT: Duration = Duration::from_secs(30);

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
    pub revision: i64,
    pub execution_generation: i64,
    pub launch_notices: Vec<String>,
    pub effort: Option<String>,
}

impl OpenedSession {
    /// Build the session from a create or resume result.
    #[must_use]
    pub fn from_snapshot(snapshot: CanonicalSnapshot) -> Self {
        let revision = snapshot.revision;
        let execution_generation = snapshot.execution_generation;
        let effort = snapshot.info.reasoning_effort.clone();
        let cursor = Cursor {
            replay_epoch: snapshot.replay_epoch.clone(),
            last_sequence: u64::try_from(snapshot.last_sequence).unwrap_or(0),
        };
        let info = SessionInfoSummary {
            model: snapshot.info.model.clone(),
            provider: snapshot.info.provider.clone(),
            cwd: snapshot.info.cwd.clone(),
            branch: snapshot.info.branch.clone(),
        };
        let title = snapshot
            .info
            .title
            .clone()
            .filter(|title| !title.trim().is_empty());
        let session_id = snapshot.session_id.clone();
        let stored_session_id = if snapshot.stored_session_id.is_empty() {
            snapshot.session_id.clone()
        } else {
            snapshot.stored_session_id.clone()
        };
        let subscription_id = snapshot.subscription_id.clone();
        Self {
            state: SessionState::from_snapshot(snapshot),
            session_id,
            stored_session_id,
            subscription_id,
            info,
            title,
            cursor,
            revision,
            execution_generation,
            launch_notices: Vec::new(),
            effort,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The gateway answered `not_found`.
    NotFound,
    /// `-c` found a session of another folder; it was detached again.
    NotInFolder { session_cwd: String },
    /// `-c` found a session whose folder the gateway did not report; it was detached again.
    FolderUnknown,
    /// Any other refusal, by its `data.reason`.
    Refused { reason: String },
    /// The create reply was lost twice.
    Unknown,
    /// No usable answer: not connected, timed out, or a result that did not decode.
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

/// The effect executor for one session's calls. Cloning is cheap; clones share the client.
#[derive(Clone)]
pub struct SessionService {
    launch_id: String,
    client: Option<Client>,
    create_defaults: Option<(String, crate::models::OfferedCreateKeys)>,
}

impl SessionService {
    /// A service with no connection. Every call fails as not connected.
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        Self {
            launch_id: launch_id.into(),
            client: None,
            create_defaults: None,
        }
    }

    /// A service that calls through `client`, the link's one client.
    #[must_use]
    pub fn with_client(launch_id: impl Into<String>, client: Client) -> Self {
        Self {
            launch_id: launch_id.into(),
            client: Some(client),
            create_defaults: None,
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

    /// Reuse the launch handshake's profile and offered create fields.
    #[must_use]
    pub fn with_create_defaults(mut self, profile: &str, parameters: &[String]) -> Self {
        self.create_defaults = Some((
            profile.into(),
            crate::models::OfferedCreateKeys(parameters.iter().cloned().collect()),
        ));
        self
    }

    async fn call(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, CallError> {
        let Some(client) = &self.client else {
            return Err(CallError::NotConnected);
        };
        client
            .call_raw(method, params, CallOptions { timeout })
            .await
    }

    async fn snapshot(&self, method: &str, params: Value) -> Result<OpenedSession, OpenError> {
        let value = self.call(method, params, None).await.map_err(open_error)?;
        snapshot_from(value)
    }

    /// Create, continue, or resume. `-c` checks the session's folder against the workspace and
    /// detaches a session that is outside it or whose folder is unknown.
    pub async fn open(&self, request: SessionRequest) -> Result<OpenedSession, OpenError> {
        let SessionRequest::New(new) = request else {
            return self.open_planned(request).await;
        };
        let loaded = tokio::task::spawn_blocking(crate::config::load_models).await;
        let saved = match loaded {
            Ok(Ok(saved)) if !saved.defaults.is_empty() => saved,
            Ok(Err(notice)) => {
                let mut opened = self.open_planned(SessionRequest::New(new)).await?;
                opened.launch_notices.push(notice);
                return Ok(opened);
            }
            _ => return self.open_planned(SessionRequest::New(new)).await,
        };
        let (profile, offered) = match &self.create_defaults {
            Some(context) => context.clone(),
            None => {
                let Some(client) = &self.client else {
                    return self.open_planned(SessionRequest::New(new)).await;
                };
                let describe = client
                    .call::<hermes_contract::methods::canonical::runtime_describe::Marker>(
                        hermes_contract::schemas::RuntimeDescribeParams(serde_json::Map::new()),
                    )
                    .await
                    .map_err(open_error)?;
                (
                    crate::config::served_model_profile(&describe.profile_id).to_owned(),
                    crate::models::OfferedCreateKeys(
                        describe.session_create.parameters.into_iter().collect(),
                    ),
                )
            }
        };
        let flags = crate::models::FlagsGiven {
            model_or_provider: new.model.is_some() || new.provider.is_some(),
            effort: new.reasoning.is_some(),
        };
        let (mut opened, notices) = crate::models::open_with_saved_default(
            self,
            new,
            saved.defaults.get(&profile),
            flags,
            &offered,
        )
        .await?;
        opened.launch_notices.extend(notices);
        Ok(opened)
    }

    /// Execute a plan whose saved fields have already been checked.
    pub(crate) async fn open_planned(
        &self,
        request: SessionRequest,
    ) -> Result<OpenedSession, OpenError> {
        match request {
            SessionRequest::New(new) => self.create(&new).await,
            SessionRequest::Latest { workspace } => {
                let opened = self
                    .snapshot("session.resume", resume_params_latest(&workspace))
                    .await?;
                match belongs(opened.info.cwd.as_deref(), &workspace) {
                    Belongs::Yes => Ok(opened),
                    outcome => {
                        let _ = self.detach(&opened).await;
                        Err(match outcome {
                            Belongs::No => OpenError::NotInFolder {
                                session_cwd: opened.info.cwd.clone().unwrap_or_default(),
                            },
                            _ => OpenError::FolderUnknown,
                        })
                    }
                }
            }
            SessionRequest::ById { session_id } => {
                self.snapshot("session.resume", resume_params_id(&session_id))
                    .await
            }
            SessionRequest::Pick => Err(OpenError::Link(
                "a session must be chosen from the list first".to_string(),
            )),
        }
    }

    /// `session.create`; after a lost reply, once more with the same request once the link is
    /// back. A second loss fails.
    async fn create(&self, new: &NewSession) -> Result<OpenedSession, OpenError> {
        let params = create_params(new);
        match self.call("session.create", params.clone(), None).await {
            Ok(value) => return snapshot_from(value),
            Err(CallError::Unknown { .. }) => {}
            Err(error) => return Err(open_error(error)),
        }
        let deadline = tokio::time::Instant::now() + RECONNECT_WAIT;
        loop {
            match self.call("session.create", params.clone(), None).await {
                Ok(value) => return snapshot_from(value),
                Err(CallError::NotConnected) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(CallError::Unknown { .. }) => return Err(OpenError::Unknown),
                Err(error) => return Err(open_error(error)),
            }
        }
    }

    /// Run a send-prompt or interrupt effect. An unknown outcome is never sent again.
    pub async fn execute(&self, effect: Effect) -> ExecuteOutcome {
        let result = match effect {
            Effect::SendPrompt {
                session_id,
                text,
                submission_id,
            } => {
                let params = submit_params(session_id.as_str(), &text, &submission_id);
                match self.call("prompt.submit", params, None).await {
                    Ok(reply) if reply.get("voice_stopped") == Some(&Value::Bool(true)) => {
                        return ExecuteOutcome::VoiceStopped;
                    }
                    other => other.map(drop),
                }
            }
            Effect::Interrupt {
                session_id,
                execution_generation,
            } => {
                let params = json!({
                    "session_id": session_id.as_str(),
                    "execution_generation": execution_generation,
                });
                self.call("session.interrupt", params, None).await.map(drop)
            }
            Effect::RespondApproval { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => {
                return ExecuteOutcome::Refused {
                    reason: "not_a_session_call".to_string(),
                };
            }
            Effect::RunShell { .. } | Effect::AskSide { .. } => {
                todo!("#225")
            }
        };
        match result {
            Ok(()) => ExecuteOutcome::Sent,
            Err(error) if error.outcome_unknown() => ExecuteOutcome::OutcomeUnknown,
            Err(CallError::Rpc(error)) => ExecuteOutcome::Refused {
                reason: error.reason.unwrap_or_else(|| "refused".to_string()),
            },
            Err(CallError::NotConnected) => ExecuteOutcome::Refused {
                reason: "not_connected".to_string(),
            },
            // The gateway did the work but its receipt did not decode.
            Err(CallError::BadResult { .. }) => ExecuteOutcome::Sent,
            Err(_) => ExecuteOutcome::Refused {
                reason: "not_sent".to_string(),
            },
        }
    }

    /// After a replay gap: resume the session again for a fresh snapshot.
    pub async fn recover(&self, gap: ReplayGap) -> Result<OpenedSession, OpenError> {
        self.snapshot("session.resume", resume_params_id(&gap.session_id))
            .await
    }

    /// `session.detach` with exactly the two ids, within [`DETACH_TIMEOUT`].
    pub async fn detach(&self, opened: &OpenedSession) -> DetachOutcome {
        let params = detach_params(&opened.session_id, &opened.subscription_id);
        match self
            .call("session.detach", params, Some(DETACH_TIMEOUT))
            .await
        {
            Ok(reply) if reply.get("detached") == Some(&Value::Bool(false)) => {
                DetachOutcome::StaleSubscription
            }
            Ok(_) => DetachOutcome::Detached,
            Err(_) => DetachOutcome::Failed,
        }
    }

    /// `session.list` for one scope, read loosely: rows without a session id are skipped.
    pub async fn list(&self, scope: &str) -> Result<Vec<crate::launch::ListRow>, CallError> {
        let params = json!({"scope": scope, "limit": crate::launch::LIST_ROWS});
        let reply = self.call("session.list", params, None).await?;
        Ok(list_rows(&reply))
    }
}

fn open_error(error: CallError) -> OpenError {
    match error {
        CallError::Rpc(error) => match error.reason.as_deref() {
            Some("not_found") => OpenError::NotFound,
            reason => OpenError::Refused {
                reason: reason.unwrap_or("refused").to_string(),
            },
        },
        CallError::Unknown { .. } => OpenError::Unknown,
        other => OpenError::Link(other.to_string()),
    }
}

fn snapshot_from(value: Value) -> Result<OpenedSession, OpenError> {
    serde_json::from_value::<CanonicalSnapshot>(value)
        .map(OpenedSession::from_snapshot)
        .map_err(|_| OpenError::Link("the session reply did not decode".to_string()))
}

/// The rows of a `session.list` reply.
#[must_use]
pub fn list_rows(reply: &Value) -> Vec<crate::launch::ListRow> {
    let rows = reply
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.iter()
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
            let session_id = text("session_id").or_else(|| text("id"))?;
            Some(crate::launch::ListRow {
                session_id,
                title: text("title").filter(|title| !title.trim().is_empty()),
                started_at: row.get("started_at").and_then(Value::as_f64),
                model: text("model"),
                message_count: row
                    .get("message_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                running: row.get("running").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

/// `session.create`: `request_id`, `source`, `cwd`, and the options that were given.
#[must_use]
pub fn create_params(new: &NewSession) -> Value {
    let mut params = Map::new();
    params.insert("request_id".into(), Value::from(new.request_id.as_str()));
    let source = if new.source.is_empty() {
        SESSION_SOURCE
    } else {
        new.source.as_str()
    };
    params.insert("source".into(), Value::from(source));
    params.insert("cwd".into(), Value::from(new.cwd.display().to_string()));
    for (key, value) in [
        ("model", &new.model),
        ("provider", &new.provider),
        ("reasoning", &new.reasoning),
    ] {
        if let Some(value) = value {
            params.insert(key.into(), Value::from(value.as_str()));
        }
    }
    Value::Object(params)
}

/// `session.resume` for the newest `tui` session of a workspace.
#[must_use]
pub fn resume_params_latest(workspace: &Path) -> Value {
    json!({
        "latest": SESSION_SOURCE,
        "workspace": workspace.display().to_string(),
        "display_projection": true,
    })
}

/// `session.resume` for one id, trimmed.
#[must_use]
pub fn resume_params_id(session_id: &str) -> Value {
    json!({"session_id": session_id.trim(), "display_projection": true})
}

/// `session.detach` with exactly the session and subscription ids, which leaves the session
/// running.
#[must_use]
pub fn detach_params(session_id: &str, subscription_id: &str) -> Value {
    json!({"session_id": session_id, "subscription_id": subscription_id})
}

/// `prompt.submit` with exactly `session_id`, `text` and `submission_id`.
#[must_use]
pub fn submit_params(session_id: &str, text: &str, submission_id: &str) -> Value {
    json!({"session_id": session_id, "text": text, "submission_id": submission_id})
}

/// `talaria-<launch id>-<n>`, `n` counting from 1 within a launch.
#[must_use]
pub fn submission_id(launch_id: &str, n: u64) -> String {
    format!("talaria-{launch_id}-{n}")
}

/// Keep an event whose epoch differs, or whose sequence is above the cursor.
#[must_use]
pub fn accept_event(cursor: &Cursor, replay_epoch: Option<&str>, seq: Option<u64>) -> bool {
    match (replay_epoch, seq) {
        (Some(epoch), Some(seq)) => epoch != cursor.replay_epoch || seq > cursor.last_sequence,
        _ => true,
    }
}
