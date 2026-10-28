//! Workflow viewer I/O. The app channel owns every asynchronous result.
use crate::app::AppMessage;
use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::{
    methods::{canonical, legacy},
    native::NativeMethod,
};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::{
    journal::{JournalError, JournalReader, RunDir},
    progress::{
        ControlOutcome, ControlRequest, RunControl, blobs,
        live::{self, GatewayFailure, ProgressEffect, ProgressInput, ProgressState},
    },
};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, thiserror::Error)]
pub enum ProgressError {
    #[error("{0}")]
    Journal(#[from] JournalError),
    #[error("workflow viewer task failed")]
    Task,
}
pub struct ProgressService {
    pub run_dir: RunDir,
    pub offset: u64,
    client: Option<Client>,
    sender: Option<UnboundedSender<AppMessage>>,
}
impl ProgressService {
    pub fn new(run_dir: RunDir) -> Self {
        Self {
            run_dir,
            offset: 0,
            client: None,
            sender: None,
        }
    }
    pub fn set_connection(&mut self, client: Option<Client>, sender: UnboundedSender<AppMessage>) {
        self.client = client;
        self.sender = Some(sender);
    }
    pub async fn poll_once(&mut self, state: &mut ProgressState) -> Result<(), ProgressError> {
        let dir = self.run_dir.clone();
        let offset = self.offset;
        let result = tokio::task::spawn_blocking(move || {
            let (lines, offset) =
                JournalReader::read_from(&dir.path().join("journal.jsonl"), offset)?;
            Ok::<_, JournalError>((lines, offset, RunControl::pending(&dir)))
        })
        .await
        .map_err(|_| ProgressError::Task)?;
        let result = result
            .map_err(|e| {
                talaria_workflow::progress::tail::absolute_error(e, state.progress.last_seq)
            })
            .and_then(|(lines, offset, pending)| {
                talaria_workflow::progress::tail::check_sequence(&lines, state.progress.last_seq)?;
                Ok((lines, offset, pending))
            });
        match result {
            Ok((lines, offset, pending)) => {
                self.offset = offset;
                live::update(state, ProgressInput::Journal(lines));
                live::update(state, ProgressInput::Controls(pending));
            }
            Err(error) => {
                live::update(state, ProgressInput::Error(error.to_string()));
                return Err(error.into());
            }
        }
        Ok(())
    }
    pub async fn execute(
        &mut self,
        state: &mut ProgressState,
        effects: Vec<ProgressEffect>,
    ) -> Result<(), ProgressError> {
        let stop = effects.iter().any(|e| {
            matches!(
                e,
                ProgressEffect::Interrupt { .. } | ProgressEffect::CancelQueued { .. }
            )
        });
        if let Some(sender) = self.sender.clone() {
            let client = self.client.clone();
            let dir = self.run_dir.clone();
            let at_ms = state.now_ms;
            let keys: HashMap<String, String> = state
                .progress
                .agents
                .iter()
                .flat_map(|a| {
                    [a.session_id.clone(), a.stored_session_id.clone()]
                        .into_iter()
                        .flatten()
                        .map(|s| (s, a.key.clone()))
                })
                .collect();
            tokio::spawn(async move {
                // Sequential attach is intentional: opening a run never fans resume calls out.
                for effect in effects {
                    for input in perform(&dir, client.as_ref(), effect, &keys, at_ms).await {
                        let _ = sender.send(AppMessage::WorkflowProgress {
                            run: dir.path().to_path_buf(),
                            input,
                        });
                    }
                }
            });
        } else {
            // The offline harness has no application loop. It still uses the same file writer.
            for effect in effects {
                let keymap = state
                    .progress
                    .agents
                    .iter()
                    .filter_map(|a| Some((a.session_id.clone()?, a.key.clone())))
                    .collect();
                let inputs = perform(
                    &self.run_dir,
                    self.client.as_ref(),
                    effect,
                    &keymap,
                    state.now_ms,
                )
                .await;
                for input in inputs {
                    live::update(state, input);
                }
            }
            if stop {
                self.poll_once(state).await?;
            }
        }
        Ok(())
    }
    pub async fn selected_prompt(&self, state: &mut ProgressState) -> Result<(), ProgressError> {
        let Some(key) = state.selected_agent_key() else {
            return Ok(());
        };
        let Some(a) = state.progress.agent(key) else {
            return Ok(());
        };
        if let Some(text) = &a.prompt {
            live::update(
                state,
                ProgressInput::PromptLoaded(text.lines().next().unwrap_or_default().into()),
            );
        } else if let Some(reference) = a.prompt_ref.clone() {
            let dir = self.run_dir.clone();
            let text = tokio::task::spawn_blocking(move || blobs::load_text(&dir, &reference))
                .await
                .map_err(|_| ProgressError::Task)?
                .map(|t| t.lines().next().unwrap_or_default().to_string())
                .unwrap_or_else(|_| "(prompt unavailable — journal blob missing)".into());
            live::update(state, ProgressInput::PromptLoaded(text));
        }
        Ok(())
    }
}
async fn typed<M: NativeMethod>(client: &Client, params: Value) -> Result<Value, CallError>
where
    M::Params: serde::de::DeserializeOwned,
    M::Result: serde::Serialize,
{
    let params = serde_json::from_value(params).map_err(|_| CallError::Encode)?;
    client
        .call_with::<M>(
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await
        .and_then(|r| serde_json::to_value(r).map_err(|_| CallError::Encode))
}
async fn perform(
    dir: &RunDir,
    client: Option<&Client>,
    effect: ProgressEffect,
    keys: &HashMap<String, String>,
    at_ms: i64,
) -> Vec<ProgressInput> {
    if let ProgressEffect::WriteControl(id, request) = effect {
        let dir = dir.clone();
        let result = tokio::task::spawn_blocking(move || {
            let outcome = match request {
                ControlRequest::Pause => RunControl::request_pause(&dir, at_ms),
                ControlRequest::Resume => RunControl::request_resume(&dir),
                ControlRequest::Stop => RunControl::request_stop(&dir, at_ms),
                ControlRequest::Restart { key } => RunControl::request_restart(&dir, &key, at_ms),
            };
            (outcome, RunControl::pending(&dir))
        })
        .await;
        let (outcome, controls) = match result {
            Ok((outcome, controls)) => (
                outcome.unwrap_or_else(|e| ControlOutcome::Refused {
                    reason: e.to_string(),
                }),
                Some(controls),
            ),
            Err(_) => (
                ControlOutcome::Refused {
                    reason: "control request task failed".into(),
                },
                None,
            ),
        };
        let mut inputs = vec![ProgressInput::ControlDone(id, outcome)];
        if let Some(controls) = controls {
            inputs.push(ProgressInput::Controls(controls));
        }
        return inputs;
    }
    let session = match &effect {
        ProgressEffect::Attach { session_id }
        | ProgressEffect::AttachAndReconcile { session_id }
        | ProgressEffect::OpenSession { session_id }
        | ProgressEffect::Steer { session_id, .. }
        | ProgressEffect::Interrupt { session_id, .. }
        | ProgressEffect::CancelQueued { session_id, .. }
        | ProgressEffect::CheckStopped { session_id, .. }
        | ProgressEffect::SubSteer { session_id, .. }
        | ProgressEffect::SubInterrupt { session_id, .. } => session_id.clone(),
        _ => String::new(),
    };
    let key = keys.get(&session).cloned().unwrap_or_default();
    if let ProgressEffect::CheckStopped {
        session_id,
        admission_id,
        attempt,
    } = effect
    {
        let outcome =
            if let Some(client) = client {
                typed::<canonical::prompt_receipt::Marker>(client, json!({
                "session_id": session_id, "admission_id": admission_id, "include_result": true,
            })).await.ok().filter(|r| r["status"] == "terminal")
                .and_then(|r| r["outcome"].as_str().map(str::to_owned))
            } else {
                None
            };
        return vec![ProgressInput::StopReceipt {
            session_id,
            admission_id,
            attempt,
            outcome,
        }];
    }
    let Some(client) = client else {
        return vec![ProgressInput::GatewayResponse {
            session_id: session.clone(),
            key,
            result: Err(GatewayFailure {
                reason: "gateway disconnected — showing the journal".into(),
                stale_generation: false,
                disconnected: true,
            }),
        }];
    };
    let reconcile = matches!(effect, ProgressEffect::AttachAndReconcile { .. });
    let open = matches!(effect, ProgressEffect::OpenSession { .. });
    match effect {
        ProgressEffect::Attach { session_id }
        | ProgressEffect::AttachAndReconcile { session_id }
        | ProgressEffect::OpenSession { session_id } => {
            // Keep the request minimal; the generated resume params serialise default flags.
            let reply = client
                .call_raw(
                    "session.resume",
                    json!({"session_id":session_id}),
                    CallOptions {
                        timeout: Some(Duration::from_secs(10)),
                    },
                )
                .await
                .and_then(|v| {
                    serde_json::from_value::<canonical::session_resume::Result>(v).map_err(|_| {
                        CallError::BadResult {
                            method: "session.resume".into(),
                            reason: "result did not decode",
                        }
                    })
                })
                .and_then(|v| serde_json::to_value(v).map_err(|_| CallError::Encode));
            match reply {
                Ok(snapshot) => {
                    let runtime_id = snapshot["session_id"]
                        .as_str()
                        .unwrap_or(&session_id)
                        .to_string();
                    let mut inputs = vec![ProgressInput::Attached {
                        session_id,
                        runtime_id: runtime_id.clone(),
                        generation: snapshot["execution_generation"].as_i64(),
                    }];
                    if let Ok(hydrated) = serde_json::from_value(snapshot.clone()) {
                        inputs.push(ProgressInput::Hydrated {
                            session_id: runtime_id.clone(),
                            snapshot: hydrated,
                        });
                    }
                    if reconcile {
                        match typed::<legacy::subagent_list::Marker>(
                            client,
                            json!({"session_id":runtime_id}),
                        )
                        .await
                        .and_then(|v| serde_json::from_value(v).map_err(|_| CallError::Encode))
                        {
                            Ok(list) => inputs.push(ProgressInput::Reconciled {
                                session_id: runtime_id.clone(),
                                list,
                            }),
                            Err(e) => inputs.push(ProgressInput::GatewayResponse {
                                session_id: runtime_id.clone(),
                                key: key.clone(),
                                result: Err(failure(e)),
                            }),
                        }
                    }
                    if open {
                        inputs.push(ProgressInput::Opened(runtime_id));
                    }
                    inputs
                }
                Err(e) => vec![ProgressInput::GatewayResponse {
                    session_id: session.clone(),
                    key,
                    result: Err(failure(e)),
                }],
            }
        }
        effect => {
            let result = match effect {
                ProgressEffect::Steer {
                    session_id,
                    generation,
                    text,
                } => typed::<canonical::session_steer::Marker>(
                    client,
                    json!({"session_id":session_id,"execution_generation":generation,"text":text}),
                )
                .await,
                ProgressEffect::Interrupt {
                    session_id,
                    generation,
                } => {
                    typed::<canonical::session_interrupt::Marker>(
                        client,
                        json!({"session_id":session_id,"execution_generation":generation}),
                    )
                    .await
                }
                ProgressEffect::CancelQueued {
                    session_id,
                    admission_id,
                } => {
                    typed::<canonical::prompt_cancel::Marker>(
                        client,
                        json!({"session_id":session_id,"admission_id":admission_id}),
                    )
                    .await
                }
                ProgressEffect::SubSteer {
                    session_id,
                    subagent_id,
                    text,
                } => {
                    typed::<legacy::subagent_steer::Marker>(
                        client,
                        json!({"session_id":session_id,"subagent_id":subagent_id,"text":text}),
                    )
                    .await
                }
                ProgressEffect::SubInterrupt {
                    session_id,
                    subagent_id,
                } => {
                    typed::<legacy::subagent_interrupt::Marker>(
                        client,
                        json!({"session_id":session_id,"subagent_id":subagent_id}),
                    )
                    .await
                }
                _ => return vec![],
            };
            vec![ProgressInput::GatewayResponse {
                session_id: session.clone(),
                key,
                result: result.map_err(failure),
            }]
        }
    }
}
fn failure(error: CallError) -> GatewayFailure {
    let stale_generation = matches!(&error,CallError::Rpc(e) if e.code==4001&&e.reason.as_deref()==Some("stale_generation"));
    let disconnected = matches!(error, CallError::NotConnected | CallError::Unknown { .. });
    let reason = match error {
        CallError::Rpc(e) => e.reason.unwrap_or(e.message),
        CallError::TimedOut { .. } => "no answer — the journal has not moved".into(),
        _ => "gateway request failed".into(),
    };
    GatewayFailure {
        reason: reason
            .chars()
            .filter(|c| !c.is_control())
            .take(240)
            .collect(),
        stale_generation,
        disconnected,
    }
}

/// One state per run survives closing the overlay. No discovery is performed here.
#[derive(Default)]
pub struct ProgressViews {
    pub picker: Option<talaria_workflow::runner::ProgressPicker>,
    pub states: HashMap<PathBuf, ProgressState>,
    pub active: Option<PathBuf>,
    pub offsets: HashMap<PathBuf, u64>,
    pub polling: std::collections::HashSet<PathBuf>,
    pub last_poll_ms: u64,
    pub profiles: HashMap<PathBuf, String>,
    pub scoped: std::collections::HashSet<PathBuf>,
    pub last_tick_ms: u64,
}
impl ProgressViews {
    pub(crate) fn next_tick_ms(&self) -> Option<u64> {
        let view = self.states.get(self.active.as_ref()?)?;
        view.error
            .is_none()
            .then_some(self.last_tick_ms.max(self.last_poll_ms).saturating_add(250))
    }
}

pub(crate) fn load_picker() -> talaria_workflow::runner::ProgressPicker {
    talaria_workflow::runner::ProgressPicker::load(&talaria_workflow::runner::runs_dir(None))
}
pub struct OpenedRun {
    pub path: PathBuf,
    pub profile: String,
    pub state: ProgressState,
    pub offset: u64,
}
pub struct PollBatch {
    pub lines: Vec<talaria_workflow::journal::RecordLine>,
    pub offset: u64,
    pub controls: talaria_workflow::progress::PendingControls,
}
pub(crate) fn open_run(path: PathBuf) -> Result<OpenedRun, String> {
    let dir = match RunDir::open(&path) {
        Ok(dir) => dir,
        Err(error) => {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("workflow")
                .to_string();
            let mut state =
                ProgressState::new(talaria_workflow::progress::RunProgress::empty(name));
            state.error = Some(error.to_string());
            return Ok(OpenedRun {
                path,
                profile: String::new(),
                state,
                offset: 0,
            });
        }
    };
    let (state, offset) = match JournalReader::read_from(&path.join("journal.jsonl"), 0) {
        Ok((lines, offset)) => (
            ProgressState::new(talaria_workflow::progress::RunProgress::from_records(
                dir.manifest(),
                &lines,
            )),
            offset,
        ),
        Err(error) => {
            let mut state = ProgressState::new(
                talaria_workflow::progress::RunProgress::from_records(dir.manifest(), &[]),
            );
            state.error = Some(error.to_string());
            (state, 0)
        }
    };
    Ok(OpenedRun {
        profile: dir.manifest().profile.clone(),
        path,
        state,
        offset,
    })
}
pub(crate) fn poll_run(
    path: &std::path::Path,
    offset: u64,
    last_seq: u64,
) -> Result<PollBatch, String> {
    let dir = RunDir::open(path).map_err(|e| e.to_string())?;
    let (lines, offset) = JournalReader::read_from(&path.join("journal.jsonl"), offset)
        .map_err(|e| talaria_workflow::progress::tail::absolute_error(e, last_seq).to_string())?;
    talaria_workflow::progress::tail::check_sequence(&lines, last_seq)
        .map_err(|e| e.to_string())?;
    Ok(PollBatch {
        lines,
        offset,
        controls: RunControl::pending(&dir),
    })
}
pub(crate) fn spawn_effects(
    path: PathBuf,
    connection: WorkflowConnection,
    sender: UnboundedSender<AppMessage>,
    effects: Vec<ProgressEffect>,
    keys: HashMap<String, String>,
    at_ms: i64,
) {
    tokio::spawn(async move {
        let target = path.clone();
        let dir = tokio::task::spawn_blocking(move || RunDir::open(&target)).await;
        if let Ok(Ok(dir)) = dir {
            let local_only = !effects.is_empty()
                && effects
                    .iter()
                    .all(|e| matches!(e, ProgressEffect::WriteControl(..)));
            let client = match if local_only {
                Ok(None)
            } else {
                connection.client(&dir, &sender).await
            } {
                Ok(client) => client,
                Err(reason) => {
                    let _ = sender.send(AppMessage::WorkflowProgress {
                        run: path,
                        input: ProgressInput::Error(reason),
                    });
                    return;
                }
            };
            for effect in effects {
                for input in perform(&dir, client.as_ref(), effect, &keys, at_ms).await {
                    let _ = sender.send(AppMessage::WorkflowProgress {
                        run: path.clone(),
                        input,
                    });
                }
            }
        }
    });
}

/// Cached I/O resources only; reducer state stays on the application channel.
#[derive(Default)]
pub(crate) struct WorkflowConnections {
    runs: tokio::sync::Mutex<HashMap<PathBuf, ScopedConnection>>,
}
struct ScopedConnection {
    client: Client,
    inbound: tokio::task::JoinHandle<()>,
}
impl Drop for ScopedConnection {
    fn drop(&mut self) {
        self.inbound.abort();
    }
}
pub(crate) struct WorkflowConnection {
    pub home: PathBuf,
    pub foreground_profile: String,
    pub fallback: Option<Client>,
    pub cache: Arc<WorkflowConnections>,
}
impl WorkflowConnection {
    async fn client(
        &self,
        dir: &RunDir,
        sender: &UnboundedSender<AppMessage>,
    ) -> Result<Option<Client>, String> {
        let profile = &dir.manifest().profile;
        if profile.is_empty() || profile == &self.foreground_profile {
            return Ok(self.fallback.clone());
        }
        let mut runs = self.cache.runs.lock().await;
        if let Some(cached) = runs.get(dir.path())
            && !cached.inbound.is_finished()
        {
            let _ = sender.send(AppMessage::WorkflowScoped(dir.path().to_path_buf()));
            let _ = sender.send(AppMessage::WorkflowProgress {
                run: dir.path().to_path_buf(),
                input: ProgressInput::Connected(true),
            });
            return Ok(Some(cached.client.clone()));
        }
        runs.remove(dir.path());
        let endpoint = match hermes_client::discover_with(
            &self.home,
            &hermes_client::DiscoverOptions {
                ensure: hermes_client::EnsurePolicy::Never,
                ..Default::default()
            },
        )
        .await
        {
            Ok(endpoint) => endpoint,
            // In-process clients and replay tests have no discovery socket.
            Err(_) => return Ok(self.fallback.clone()),
        };
        let resolved = endpoint
            .resolve_profile(profile)
            .map_err(|e| e.to_string())?;
        let ticket = hermes_client::mint_ticket(
            &self.home,
            &endpoint,
            &resolved,
            hermes_client::Purpose::Interactive,
            hermes_client::Scope::profile(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let mut connection = hermes_client::connect(&endpoint, ticket)
            .await
            .map_err(|e| e.to_string())?;
        hermes_client::handshake(&mut connection)
            .await
            .map_err(|e| e.to_string())?;
        let (client, mut inbound) = Client::start(connection.into_parts());
        let run = dir.path().to_path_buf();
        let _ = sender.send(AppMessage::WorkflowScoped(run.clone()));
        let _ = sender.send(AppMessage::WorkflowProgress {
            run: run.clone(),
            input: ProgressInput::Connected(true),
        });
        let tx = sender.clone();
        let task = tokio::spawn(async move {
            while let Some(item) = inbound.next().await {
                for input in workflow_inbound(item) {
                    if tx
                        .send(AppMessage::WorkflowProgress {
                            run: run.clone(),
                            input,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        runs.insert(
            dir.path().to_path_buf(),
            ScopedConnection {
                client: client.clone(),
                inbound: task,
            },
        );
        Ok(Some(client))
    }
}
fn workflow_inbound(item: hermes_client::Inbound) -> Vec<ProgressInput> {
    match item {
        hermes_client::Inbound::Event(event) => {
            let Some(session_id) = event.frame.session_id else {
                return vec![];
            };
            let mut inputs = vec![];
            if let Some(generation) = event.frame.execution_generation {
                inputs.push(ProgressInput::Generation {
                    session_id: session_id.clone(),
                    generation,
                });
            }
            inputs.push(
                if let Some(event) = talaria_core::subagents::SubagentEvent::from_notification(
                    &event.frame.notification,
                ) {
                    ProgressInput::Subagent { session_id, event }
                } else {
                    ProgressInput::Gateway {
                        session_id,
                        event: event.frame.notification,
                    }
                },
            );
            inputs
        }
        hermes_client::Inbound::Closed(_) => vec![ProgressInput::Connected(false)],
        _ => vec![],
    }
}

#[cfg(test)]
#[path = "../tests/support/mock_gateway.rs"]
mod progress_mock_gateway;

#[cfg(test)]
mod connection_tests {
    use super::*;
    use progress_mock_gateway::StandaloneGateway;

    fn run(root: &std::path::Path) -> RunDir {
        let folder = root.join("01JB2X9Q4M8Z7K3T5V6W8Y0A1C");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("manifest.json"), serde_json::to_vec(&json!({
            "v":1,"run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C","created_at_ms":1000,
            "workflow":{"name":"review-branch","script_sha256":"a".repeat(64)},
            "args":null,"started_at_ms":1,"seed":4242,"profile":"fixture",
            "on_unknown":"wait","talaria_version":"0.7.0-dev","hermes":{},"roster":{},"approval_policy":{}
        })).unwrap()).unwrap();
        RunDir::open(&folder).unwrap()
    }

    #[test]
    fn foreign_workflow_reuses_profile_ticket_and_forwards_inbound() {
        let home = tempfile::tempdir().unwrap();
        let runs = tempfile::tempdir().unwrap();
        let dir = run(runs.path());
        let gateway = StandaloneGateway::start(home.path());
        gateway.block_on(async {
            let binding = WorkflowConnection {
                home: home.path().to_path_buf(),
                foreground_profile: "default".into(),
                fallback: None,
                cache: Arc::default(),
            };
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let client = binding.client(&dir, &tx).await.unwrap().unwrap();
            assert!(matches!(
                rx.recv().await.unwrap(),
                AppMessage::WorkflowScoped(_)
            ));
            assert!(matches!(
                rx.recv().await.unwrap(),
                AppMessage::WorkflowProgress {
                    input: ProgressInput::Connected(true),
                    ..
                }
            ));
            let _ = binding.client(&dir, &tx).await.unwrap().unwrap();
            assert_eq!(gateway.tickets_issued(), 1);
            let ticket = gateway.ticket_records().remove(0);
            assert_eq!(ticket.scope, "profile");
            assert_eq!(
                ticket.profile_id,
                home.path()
                    .join("profiles/fixture")
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
            );
            assert_eq!(ticket.purpose, "interactive");
            let snapshot = client
                .call_raw(
                    "session.resume",
                    json!({"session_id":progress_mock_gateway::SESSION_ID}),
                    CallOptions::default(),
                )
                .await
                .unwrap();
            assert!(snapshot["session_id"].is_string());
            gateway
                .send_event("message.delta", json!({"text":"fixture-only"}))
                .await;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let AppMessage::WorkflowProgress {
                        run,
                        input: ProgressInput::Gateway { session_id, .. },
                    } = rx.recv().await.unwrap()
                    {
                        assert_eq!(run, dir.path());
                        assert_eq!(session_id, progress_mock_gateway::SESSION_ID);
                        break;
                    }
                }
            })
            .await
            .unwrap();
            client.close().await;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if matches!(
                        rx.recv().await.unwrap(),
                        AppMessage::WorkflowProgress {
                            input: ProgressInput::Connected(false),
                            ..
                        }
                    ) {
                        break;
                    }
                }
            })
            .await
            .unwrap();
        });
    }

    #[tokio::test]
    async fn absent_discovery_uses_mock_client() {
        let runs = tempfile::tempdir().unwrap();
        let dir = run(runs.path());
        let (gateway, client, _inbound) = progress_mock_gateway::MockGateway::start().await;
        let home = tempfile::tempdir().unwrap();
        let binding = WorkflowConnection {
            home: home.path().to_path_buf(),
            foreground_profile: "default".into(),
            fallback: Some(client),
            cache: Arc::default(),
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let fallback = binding.client(&dir, &tx).await.unwrap().unwrap();
        fallback
            .call_raw(
                "session.resume",
                json!({"session_id":progress_mock_gateway::SESSION_ID}),
                CallOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(gateway.requests("session.resume").len(), 1);
    }

    #[test]
    fn discovered_gateway_ticket_refusal_never_uses_foreground_client() {
        let home = tempfile::tempdir().unwrap();
        let runs = tempfile::tempdir().unwrap();
        let dir = run(runs.path());
        let gateway = StandaloneGateway::start(home.path());
        gateway.set_refuse_tickets(true);
        gateway.block_on(async {
            let (_fallback_gateway, fallback, _inbound) =
                progress_mock_gateway::MockGateway::start().await;
            let binding = WorkflowConnection {
                home: home.path().to_path_buf(),
                foreground_profile: "default".into(),
                fallback: Some(fallback),
                cache: Arc::default(),
            };
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            assert!(binding.client(&dir, &tx).await.is_err());
        });
    }
}
