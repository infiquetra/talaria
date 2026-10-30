//! Owned agent processes and sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use talaria_core::fleet::acp::{
    AcpCell, AcpMapping, AcpSessionKey, PermissionOption, PermissionRequest,
};
use talaria_core::fleet::{
    ExternalNeed, ExternalOption, ExternalSession, FleetBackend, NeedSlot, Outcome, RowKey,
    RowState,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use super::client::{Initialization, Transport, decode_update, read_output, safe_text};
use super::powers::{PowerEvent, PowerManager};
use super::{AcpAgentEntry, AcpConfig};
use crate::fleet::{FleetHandle, FleetReporter, FleetSnapshot};

const SESSION_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionResponder {
    pub generation: u64,
    pub request_id: String,
    pub key: AcpSessionKey,
    pub tool_call_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReport {
    pub agent_name: String,
    pub protocol_version: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AcpError(pub String);

type Reply<T> = oneshot::Sender<Result<T, AcpError>>;
type SessionViews = HashMap<String, AcpMapping>;
type Observer = Arc<dyn Fn(AcpSessionKey, AcpMapping) + Send + Sync>;

struct WorkerHandle {
    sender: mpsc::UnboundedSender<WorkerCommand>,
    views: watch::Receiver<Arc<SessionViews>>,
    join: tokio::task::JoinHandle<()>,
}

/// Owns agent workers, each of which owns its process, pipes and session state.
pub struct AcpService {
    config: AcpConfig,
    fleet: Option<FleetHandle>,
    reporter: FleetReporter,
    workers: HashMap<String, WorkerHandle>,
    next_generation: u64,
    observer: Observer,
}

impl AcpService {
    pub fn start(cfg: AcpConfig, fleet: FleetHandle) -> AcpService {
        let reporter = fleet.reporter();
        AcpService {
            config: cfg,
            fleet: Some(fleet),
            reporter,
            workers: HashMap::new(),
            next_generation: 0,
            observer: Arc::new(|_, _| {}),
        }
    }

    pub(crate) fn attach(cfg: AcpConfig, reporter: FleetReporter, observer: Observer) -> Self {
        Self {
            config: cfg,
            fleet: None,
            reporter,
            workers: HashMap::new(),
            next_generation: 0,
            observer,
        }
    }

    pub async fn new_session(
        &mut self,
        agent_id: &str,
        cwd: &Path,
        first_prompt: &str,
    ) -> Result<AcpSessionKey, AcpError> {
        if !cwd.is_dir() {
            return Err(AcpError("The working directory does not exist.".into()));
        }
        let cwd = cwd
            .canonicalize()
            .map_err(|_| AcpError("The working directory cannot be read.".into()))?;
        let entry = self
            .config
            .agents
            .iter()
            .find(|entry| entry.id == agent_id)
            .cloned()
            .ok_or_else(|| AcpError("Agent is not configured.".into()))?;
        if self
            .workers
            .get(agent_id)
            .is_some_and(|worker| worker.sender.is_closed())
        {
            self.workers.remove(agent_id);
        }
        if !self.workers.contains_key(agent_id) {
            let (transport, init) = Transport::launch(&entry, true).await?;
            self.next_generation += 1;
            let (sender, commands) = mpsc::unbounded_channel();
            let (view_sender, views) = watch::channel(Arc::new(HashMap::new()));
            let worker = Worker::new(
                entry.clone(),
                self.next_generation,
                transport,
                init,
                self.reporter.clone(),
                commands,
                view_sender,
                Arc::clone(&self.observer),
            );
            let join = tokio::spawn(worker.run());
            self.workers.insert(
                agent_id.to_owned(),
                WorkerHandle {
                    sender,
                    views,
                    join,
                },
            );
        }
        let worker = self
            .workers
            .get(agent_id)
            .ok_or_else(|| AcpError("Agent could not start.".into()))?;
        let (reply, receive) = oneshot::channel();
        worker
            .sender
            .send(WorkerCommand::New {
                cwd,
                prompt: first_prompt.into(),
                reply,
            })
            .map_err(|_| AcpError("Agent process exited.".into()))?;
        tokio::time::timeout(SESSION_TIMEOUT, receive)
            .await
            .map_err(|_| AcpError("Agent session creation timed out.".into()))?
            .map_err(|_| AcpError("Agent process exited.".into()))?
    }

    pub async fn prompt(&mut self, key: &AcpSessionKey, text: &str) -> Result<(), AcpError> {
        let (reply, receive) = oneshot::channel();
        self.send(
            key,
            WorkerCommand::Prompt {
                key: key.clone(),
                text: text.into(),
                reply,
            },
        )?;
        receive
            .await
            .map_err(|_| AcpError("Agent process exited.".into()))?
    }

    pub async fn cancel(&mut self, key: &AcpSessionKey) -> Result<(), AcpError> {
        let (reply, receive) = oneshot::channel();
        self.send(
            key,
            WorkerCommand::Cancel {
                key: key.clone(),
                reply,
            },
        )?;
        tokio::time::timeout(CANCEL_TIMEOUT + Duration::from_secs(1), receive)
            .await
            .map_err(|_| AcpError("agent did not confirm cancellation".into()))?
            .map_err(|_| AcpError("Agent process exited.".into()))?
    }

    pub async fn answer_permission(
        &mut self,
        responder: &PermissionResponder,
        option_id: &str,
    ) -> Result<(), AcpError> {
        let (reply, receive) = oneshot::channel();
        self.send(
            &responder.key,
            WorkerCommand::Answer {
                responder: responder.clone(),
                option_id: option_id.into(),
                reply,
            },
        )?;
        receive
            .await
            .map_err(|_| AcpError("Agent process exited.".into()))?
    }

    fn send(&self, key: &AcpSessionKey, command: WorkerCommand) -> Result<(), AcpError> {
        self.workers
            .get(&key.agent_id)
            .ok_or_else(|| AcpError("Agent session is unavailable.".into()))?
            .sender
            .send(command)
            .map_err(|_| AcpError("Agent process exited.".into()))
    }

    pub fn transcript(&self, key: &AcpSessionKey) -> Vec<AcpCell> {
        self.workers
            .get(&key.agent_id)
            .and_then(|worker| {
                worker
                    .views
                    .borrow()
                    .get(&key.session_id)
                    .map(|view| view.transcript.clone())
            })
            .unwrap_or_default()
    }

    pub fn fleet_snapshot(&self) -> Arc<FleetSnapshot> {
        self.reporter.snapshot()
    }

    pub(crate) fn acknowledge(&self, key: &AcpSessionKey) {
        let _ = self.send(key, WorkerCommand::Acknowledge { key: key.clone() });
    }

    pub async fn close(&mut self) {
        let mut closing = tokio::task::JoinSet::new();
        for (_, worker) in self.workers.drain() {
            closing.spawn(async move {
                let (reply, receive) = oneshot::channel();
                let _ = worker.sender.send(WorkerCommand::Close { reply });
                let mut join = worker.join;
                if tokio::time::timeout(CLOSE_TIMEOUT + Duration::from_secs(3), receive)
                    .await
                    .is_err()
                {
                    join.abort();
                }
                if tokio::time::timeout(Duration::from_secs(3), &mut join)
                    .await
                    .is_err()
                {
                    join.abort();
                }
            });
        }
        while closing.join_next().await.is_some() {}
        if let Some(fleet) = self.fleet.take() {
            fleet.close().await;
        }
    }
}

/// Initialize an agent without creating or authenticating a session.
pub async fn probe_agent(entry: &AcpAgentEntry) -> Result<ProbeReport, AcpError> {
    let (mut transport, init) = Transport::launch(entry, false).await?;
    transport.terminate().await;
    Ok(ProbeReport {
        agent_name: init.agent_name,
        protocol_version: 1,
    })
}

enum WorkerCommand {
    New {
        cwd: PathBuf,
        prompt: String,
        reply: Reply<AcpSessionKey>,
    },
    Prompt {
        key: AcpSessionKey,
        text: String,
        reply: Reply<()>,
    },
    Cancel {
        key: AcpSessionKey,
        reply: Reply<()>,
    },
    Answer {
        responder: PermissionResponder,
        option_id: String,
        reply: Reply<()>,
    },
    Acknowledge {
        key: AcpSessionKey,
    },
    Close {
        reply: oneshot::Sender<()>,
    },
}

struct Creation {
    cwd: PathBuf,
    prompt: String,
    params: Value,
    retried: bool,
    reply: Reply<AcpSessionKey>,
}

enum Pending {
    Create(Creation),
    Authenticate(Creation),
    Prompt(String),
}

struct Permission {
    id: Value,
    options: Vec<PermissionOption>,
}

struct Session {
    key: AcpSessionKey,
    cwd: PathBuf,
    title: String,
    mapping: AcpMapping,
    permissions: HashMap<String, Permission>,
    prompting: bool,
    cancelling: bool,
    cancel_deadline: Option<Instant>,
    cancel_reply: Option<Reply<()>>,
}

struct Worker {
    entry: AcpAgentEntry,
    generation: u64,
    transport: Transport,
    auth_methods: Vec<Value>,
    fleet: FleetReporter,
    commands: mpsc::UnboundedReceiver<WorkerCommand>,
    views: watch::Sender<Arc<SessionViews>>,
    sessions: HashMap<String, Session>,
    pending: HashMap<u64, Pending>,
    next_id: u64,
    close_when_empty: bool,
    stderr_open: bool,
    observer: Observer,
    powers: PowerManager,
    power_tx: mpsc::UnboundedSender<PowerEvent>,
    power_rx: mpsc::UnboundedReceiver<PowerEvent>,
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    fn new(
        entry: AcpAgentEntry,
        generation: u64,
        transport: Transport,
        init: Initialization,
        fleet: FleetReporter,
        commands: mpsc::UnboundedReceiver<WorkerCommand>,
        views: watch::Sender<Arc<SessionViews>>,
        observer: Observer,
    ) -> Self {
        let (power_tx, power_rx) = mpsc::unbounded_channel();
        Self {
            entry,
            generation,
            transport,
            auth_methods: init.auth_methods,
            fleet,
            commands,
            views,
            sessions: HashMap::new(),
            pending: HashMap::new(),
            next_id: 1,
            close_when_empty: false,
            stderr_open: true,
            observer,
            powers: PowerManager::default(),
            power_tx,
            power_rx,
        }
    }

    async fn run(mut self) {
        let mut ticks = tokio::time::interval(Duration::from_millis(100));
        enum Event {
            Command(Option<WorkerCommand>),
            Frame(Result<Option<Value>, AcpError>),
            Diagnostic(bool),
            Power(Option<PowerEvent>),
            Tick,
        }
        loop {
            let event = tokio::select! {
                command = self.commands.recv() => Event::Command(command),
                frame = read_output(&mut self.transport.output, &mut self.transport.read_buffer) => Event::Frame(frame),
                changed = self.transport.stderr.changed(), if self.stderr_open => Event::Diagnostic(changed.is_ok()),
                power = self.power_rx.recv() => Event::Power(power),
                _ = ticks.tick() => Event::Tick,
            };
            match event {
                Event::Command(Some(WorkerCommand::Close { reply })) => {
                    self.shutdown().await;
                    let _ = reply.send(());
                    return;
                }
                Event::Command(Some(command)) => self.command(command).await,
                Event::Command(None) => {
                    self.shutdown().await;
                    return;
                }
                Event::Frame(Ok(Some(frame))) => self.frame(frame).await,
                Event::Frame(Ok(None)) => {
                    let status = match tokio::time::timeout(
                        Duration::from_secs(1),
                        self.transport.child.wait(),
                    )
                    .await
                    {
                        Ok(status) => status.ok(),
                        Err(_) => {
                            self.transport.terminate().await;
                            None
                        }
                    };
                    let reason = status.and_then(|s| s.code()).map_or_else(
                        || "agent process exited (signal)".to_owned(),
                        |code| format!("agent process exited (code {code})"),
                    );
                    self.fail(&reason);
                    return;
                }
                Event::Frame(Err(error)) => {
                    self.fail(&error.0);
                    self.transport.terminate().await;
                    return;
                }
                Event::Diagnostic(true) => {
                    let text = self.transport.stderr.borrow_and_update().clone();
                    if !text.is_empty() {
                        self.fleet.diagnostic(&text);
                    }
                }
                Event::Diagnostic(false) => self.stderr_open = false,
                Event::Power(Some(power)) => {
                    if let Some(cell) = power.cell {
                        if let Some(session) = self.sessions.get_mut(&power.session_id) {
                            session.mapping.transcript.push(cell);
                        }
                        self.publish(&power.session_id);
                    }
                    if power.id.is_null() {
                        continue;
                    }
                    match power.result {
                        Ok(result) => {
                            let _ = self
                                .transport
                                .send(&json!({"jsonrpc":"2.0","id":power.id,"result":result}))
                                .await;
                        }
                        Err(error) => {
                            self.fleet.diagnostic(&error.message);
                            self.error(power.id, error.code, &error.message).await;
                        }
                    }
                }
                Event::Power(None) => {}
                Event::Tick => {
                    if self.sessions.values().any(|session| {
                        session
                            .cancel_deadline
                            .is_some_and(|at| Instant::now() >= at)
                    }) {
                        self.fail("agent did not confirm cancellation");
                        self.transport.terminate().await;
                        return;
                    }
                    if self.sessions.is_empty()
                        && !self.pending.is_empty()
                        && self.pending.values().all(|pending| match pending {
                            Pending::Create(creation) | Pending::Authenticate(creation) => {
                                creation.reply.is_closed()
                            }
                            Pending::Prompt(_) => false,
                        })
                    {
                        self.transport.terminate().await;
                        return;
                    }
                }
            }
            if self.close_when_empty && self.sessions.is_empty() && self.pending.is_empty() {
                self.transport.terminate().await;
                return;
            }
        }
    }

    async fn command(&mut self, command: WorkerCommand) {
        match command {
            WorkerCommand::New { cwd, prompt, reply } => {
                let params = json!({"cwd":cwd,"mcpServers":[]});
                let creation = Creation {
                    cwd,
                    prompt,
                    params: params.clone(),
                    retried: false,
                    reply,
                };
                let id = self.next();
                self.pending.insert(id, Pending::Create(creation));
                if let Err(error) = self.request(id, "session/new", params).await {
                    self.reject_pending(id, error);
                }
            }
            WorkerCommand::Prompt { key, text, reply } => {
                let result = self.begin_prompt(&key.session_id, &text).await;
                let _ = reply.send(result);
            }
            WorkerCommand::Cancel { key, reply } => self.cancel(&key.session_id, reply).await,
            WorkerCommand::Answer {
                responder,
                option_id,
                reply,
            } => {
                let result = self.answer(responder, &option_id).await;
                let _ = reply.send(result);
            }
            WorkerCommand::Acknowledge { key } => {
                if let Some(session) = self.sessions.get_mut(&key.session_id)
                    && matches!(session.mapping.state, RowState::Done | RowState::Failed)
                {
                    session.mapping.state = RowState::Idle;
                    session.mapping.outcome = None;
                    self.publish(&key.session_id);
                }
            }
            WorkerCommand::Close { .. } => {}
        }
    }

    fn next(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    async fn request(&mut self, id: u64, method: &str, params: Value) -> Result<(), AcpError> {
        self.transport
            .send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await
    }

    async fn begin_prompt(&mut self, session_id: &str, text: &str) -> Result<(), AcpError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError("Agent session is unavailable.".into()))?;
        if session.prompting {
            return Err(AcpError("Agent is already working.".into()));
        }
        session.prompting = true;
        session.cancelling = false;
        session.mapping.start_turn();
        self.publish(session_id);
        let id = self.next();
        self.pending.insert(id, Pending::Prompt(session_id.into()));
        let result = self
            .request(
                id,
                "session/prompt",
                json!({"sessionId":session_id,"prompt":[{"type":"text","text":text}]}),
            )
            .await;
        if result.is_err() {
            self.pending.remove(&id);
            self.fail_session(session_id, "Agent prompt could not be sent.");
        }
        result
    }

    async fn cancel(&mut self, session_id: &str, reply: Reply<()>) {
        let Some(session) = self.sessions.get_mut(session_id) else {
            let _ = reply.send(Err(AcpError("Agent session is unavailable.".into())));
            return;
        };
        if !session.prompting {
            let _ = reply.send(Ok(()));
            return;
        }
        if session.cancelling {
            let _ = reply.send(Err(AcpError("Cancellation is already pending.".into())));
            return;
        }
        session.cancelling = true;
        session.cancel_deadline = Some(Instant::now() + CANCEL_TIMEOUT);
        session.cancel_reply = Some(reply);
        self.cancel_permissions(session_id).await;
        if self.transport.send(&json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session_id}})).await.is_err() {
            self.fail_session(session_id, "Could not cancel agent turn.");
        }
    }

    async fn cancel_permissions(&mut self, session_id: &str) {
        let ids = if let Some(session) = self.sessions.get_mut(session_id) {
            session.mapping.needs.clear();
            session
                .permissions
                .drain()
                .map(|(_, permission)| permission.id)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for id in ids {
            let _ = self
                .transport
                .send(
                    &json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}}),
                )
                .await;
        }
        self.publish(session_id);
    }

    async fn answer(
        &mut self,
        responder: PermissionResponder,
        option_id: &str,
    ) -> Result<(), AcpError> {
        if responder.generation != self.generation {
            self.fleet.diagnostic("already settled");
            return Err(AcpError("already settled".into()));
        }
        let session = self
            .sessions
            .get_mut(&responder.key.session_id)
            .ok_or_else(|| AcpError("Agent session is unavailable.".into()))?;
        let Some(permission) = session.permissions.get(&responder.request_id) else {
            self.fleet.diagnostic("already settled");
            return Err(AcpError("already settled".into()));
        };
        if !permission
            .options
            .iter()
            .any(|option| option.id == option_id)
        {
            return Err(AcpError("Permission option is unavailable.".into()));
        }
        let Some(permission) = session.permissions.remove(&responder.request_id) else {
            return Err(AcpError("already settled".into()));
        };
        let identity = format!("{}:{}", self.generation, responder.request_id);
        session
            .mapping
            .needs
            .retain(|need| need.responder != identity);
        if session.mapping.needs.is_empty() {
            session.mapping.state = if session.prompting {
                RowState::Working
            } else {
                RowState::Done
            };
        }
        let result = self.transport.send(&json!({"jsonrpc":"2.0","id":permission.id,"result":{"outcome":{"outcome":"selected","optionId":option_id}}})).await;
        self.publish(&responder.key.session_id);
        result
    }

    async fn frame(&mut self, frame: Value) {
        if let Some(method) = frame.get("method").and_then(Value::as_str) {
            let method = method.to_owned();
            let params = frame.get("params").cloned().unwrap_or(Value::Null);
            if method == "session/update" {
                if let Some(id) = params.get("sessionId").and_then(Value::as_str)
                    && let Some(session) = self.sessions.get_mut(id)
                {
                    if let Some(update) = params.get("update") {
                        for event in decode_update(&session.key, update) {
                            session.mapping.apply(event);
                        }
                    }
                    self.publish(id);
                }
            } else if let Some(id) = frame.get("id").cloned() {
                if method == "session/request_permission" {
                    self.permission(id, params).await;
                } else {
                    self.power_request(id, &method, params).await;
                }
            }
            return;
        }
        let Some(id) = frame.get("id").and_then(Value::as_u64) else {
            return;
        };
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        match pending {
            Pending::Create(creation) => self.created(creation, frame).await,
            Pending::Authenticate(creation) => {
                if let Some(error) = frame.get("error") {
                    let _ = creation.reply.send(Err(AcpError(format!(
                        "Authentication failed: {}",
                        safe_text(error["message"].as_str().unwrap_or("refused"))
                    ))));
                    self.close_when_empty = true;
                } else {
                    let params = creation.params.clone();
                    let id = self.next();
                    self.pending.insert(id, Pending::Create(creation));
                    if let Err(error) = self.request(id, "session/new", params).await {
                        self.reject_pending(id, error);
                    }
                }
            }
            Pending::Prompt(session_id) => {
                if self
                    .sessions
                    .get(&session_id)
                    .is_some_and(|session| !session.permissions.is_empty())
                {
                    self.cancel_permissions(&session_id).await;
                }
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.prompting = false;
                    session.cancelling = false;
                    session.cancel_deadline = None;
                    if let Some(error) = frame.get("error") {
                        let text =
                            safe_text(error["message"].as_str().unwrap_or("Agent prompt failed."));
                        session.mapping.state = RowState::Failed;
                        session.mapping.outcome = Some(Outcome::Failed);
                        session
                            .mapping
                            .transcript
                            .push(AcpCell::Notice(text.clone()));
                        session.mapping.activity = Some(text);
                        if let Some(reply) = session.cancel_reply.take() {
                            let _ = reply.send(Err(AcpError("Agent prompt failed.".into())));
                        }
                    } else if let Some(reason) =
                        frame.pointer("/result/stopReason").and_then(Value::as_str)
                    {
                        session.mapping.finish(reason);
                        if let Some(reply) = session.cancel_reply.take() {
                            let _ = reply.send(Ok(()));
                        }
                    } else {
                        session.mapping.state = RowState::Failed;
                        session.mapping.outcome = Some(Outcome::Failed);
                        if let Some(reply) = session.cancel_reply.take() {
                            let _ = reply
                                .send(Err(AcpError("Malformed agent prompt response.".into())));
                        }
                    }
                }
                self.publish(&session_id);
            }
        }
    }

    async fn created(&mut self, mut creation: Creation, frame: Value) {
        if let Some(error) = frame.get("error") {
            let auth_required = error.get("code").and_then(Value::as_i64) == Some(-32000);
            let method = self
                .entry
                .auth_method
                .as_ref()
                .filter(|method| {
                    self.auth_methods.iter().any(|item| {
                        item.get("id").and_then(Value::as_str) == Some(method.as_str())
                            && item.get("type").and_then(Value::as_str) != Some("terminal")
                            && item
                                .pointer("/_meta/terminal-auth/type")
                                .and_then(Value::as_str)
                                != Some("terminal")
                    })
                })
                .cloned();
            if auth_required
                && !creation.retried
                && let Some(method) = method
            {
                creation.retried = true;
                let id = self.next();
                self.pending.insert(id, Pending::Authenticate(creation));
                if let Err(error) = self
                    .request(id, "authenticate", json!({"methodId":method}))
                    .await
                {
                    self.reject_pending(id, error);
                }
                return;
            }
            let message = safe_text(
                error["message"]
                    .as_str()
                    .unwrap_or("Agent refused the session."),
            );
            let _ = creation.reply.send(Err(AcpError(message)));
            self.close_when_empty = true;
            return;
        }
        let Some(session_id) = frame
            .pointer("/result/sessionId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
        else {
            let _ = creation
                .reply
                .send(Err(AcpError("Agent did not provide a session id.".into())));
            self.close_when_empty = true;
            return;
        };
        if self.sessions.contains_key(&session_id) {
            let _ = creation
                .reply
                .send(Err(AcpError("Agent reused an existing session id.".into())));
            return;
        }
        if creation.reply.is_closed() {
            let id = self.next();
            let _ = self
                .request(id, "session/close", json!({"sessionId":session_id}))
                .await;
            self.close_when_empty = true;
            return;
        }
        let key = AcpSessionKey {
            agent_id: self.entry.id.clone(),
            session_id: session_id.clone(),
        };
        let title = if creation.prompt.is_empty() {
            self.entry.display_name.clone()
        } else {
            safe_text(&creation.prompt)
        };
        self.sessions.insert(
            session_id.clone(),
            Session {
                key: key.clone(),
                cwd: creation.cwd,
                title,
                mapping: AcpMapping::default(),
                permissions: HashMap::new(),
                prompting: false,
                cancelling: false,
                cancel_deadline: None,
                cancel_reply: None,
            },
        );
        self.publish(&session_id);
        if !creation.prompt.is_empty()
            && let Err(error) = self.begin_prompt(&session_id, &creation.prompt).await
        {
            let _ = creation.reply.send(Err(error));
            return;
        }
        let _ = creation.reply.send(Ok(key));
    }

    async fn permission(&mut self, id: Value, params: Value) {
        let Some(session_id) = params
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            self.error(id, -32602, "Malformed permission request.")
                .await;
            return;
        };
        let Some(session) = self.sessions.get_mut(&session_id) else {
            self.error(id, -32602, "Unknown agent session.").await;
            return;
        };
        if session.cancelling {
            let _ = self
                .transport
                .send(
                    &json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}}),
                )
                .await;
            return;
        }
        let Some(request_id) = request_id(&id) else {
            return;
        };
        if session.permissions.contains_key(&request_id) {
            self.fleet
                .diagnostic("Agent reused an open permission request id.");
            self.error(id, -32600, "Permission request id is already open.")
                .await;
            return;
        }
        let tool_call_id = params
            .pointer("/toolCall/toolCallId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let title = params
            .pointer("/toolCall/title")
            .and_then(Value::as_str)
            .unwrap_or("tool")
            .to_owned();
        let options = params
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                Some(PermissionOption {
                    id: option.get("optionId")?.as_str()?.to_owned(),
                    name: option.get("name")?.as_str()?.to_owned(),
                    kind: option.get("kind")?.as_str()?.to_owned(),
                })
            })
            .filter(|option| {
                !option.id.is_empty() && !option.name.is_empty() && !option.kind.is_empty()
            })
            .collect::<Vec<_>>();
        if options.is_empty() {
            self.fleet
                .diagnostic("Permission request has no usable options");
            self.error(id, -32602, "Permission request has no usable options.")
                .await;
            return;
        }
        let responder = format!("{}:{}", self.generation, request_id);
        session.permissions.insert(
            request_id,
            Permission {
                id,
                options: options.clone(),
            },
        );
        session.mapping.request_permission(PermissionRequest {
            key: session.key.clone(),
            responder,
            tool_call_id,
            tool_title: title,
            options,
        });
        self.publish(&session_id);
    }

    async fn power_request(&mut self, id: Value, method: &str, params: Value) {
        let Some(session_id) = params.get("sessionId").and_then(Value::as_str) else {
            self.error(id, -32602, "talaria: missing session id").await;
            return;
        };
        let Some(session) = self.sessions.get(session_id) else {
            self.error(id, -32602, "talaria: unknown session").await;
            return;
        };
        if let Err(error) = self.powers.dispatch(
            &self.entry,
            session_id,
            &session.cwd,
            id.clone(),
            method,
            params.clone(),
            &self.power_tx,
        ) {
            self.fleet.diagnostic(&error.message);
            self.error(id, error.code, &error.message).await;
        }
    }

    async fn error(&mut self, id: Value, code: i64, message: &str) {
        let _ = self
            .transport
            .send(&json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
            .await;
    }

    fn reject_pending(&mut self, id: u64, error: AcpError) {
        if let Some(pending) = self.pending.remove(&id) {
            match pending {
                Pending::Create(creation) | Pending::Authenticate(creation) => {
                    let _ = creation.reply.send(Err(error));
                }
                Pending::Prompt(_) => {}
            }
        }
        self.close_when_empty = true;
    }

    fn publish(&self, session_id: &str) {
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        let usage = session.mapping.usage.as_ref();
        let needs = session
            .mapping
            .needs
            .iter()
            .map(|need| ExternalNeed {
                slot: NeedSlot::External(need.responder.clone()),
                text: format!("{}: {}", self.entry.display_name, need.tool_title),
                options: need
                    .options
                    .iter()
                    .map(|option| ExternalOption {
                        id: option.id.clone(),
                        name: option.name.clone(),
                        kind: option.kind.clone(),
                    })
                    .collect(),
            })
            .collect();
        self.fleet.observe_external(ExternalSession {
            key: RowKey {
                profile: format!("acp:{}", self.entry.id),
                session_id: session_id.into(),
            },
            backend: FleetBackend::Acp,
            agent: self.entry.display_name.clone(),
            title: session
                .mapping
                .title
                .clone()
                .unwrap_or_else(|| session.title.clone()),
            model: session.mapping.model.clone(),
            activity: session.mapping.activity.clone(),
            state: session.mapping.state,
            outcome: session.mapping.outcome,
            used: usage.map(|u| u.used),
            size: usage.map(|u| u.size),
            cost: usage.and_then(|u| u.cost_amount.zip(u.cost_currency.clone())),
            needs,
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |duration| duration.as_secs_f64()),
        });
        for diagnostic in &session.mapping.diagnostics {
            self.fleet.diagnostic(diagnostic);
        }
        let views = self
            .sessions
            .iter()
            .map(|(id, session)| (id.clone(), session.mapping.clone()))
            .collect();
        self.views.send_replace(Arc::new(views));
        (self.observer)(session.key.clone(), session.mapping.clone());
    }

    fn fail_session(&mut self, session_id: &str, reason: &str) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.mapping.state = RowState::Failed;
            session.mapping.outcome = Some(Outcome::Failed);
            session.mapping.activity = Some(safe_text(reason));
            session.mapping.needs.clear();
            session.permissions.clear();
            session.prompting = false;
            if let Some(reply) = session.cancel_reply.take() {
                let _ = reply.send(Err(AcpError(safe_text(reason))));
            }
        }
        self.fleet.diagnostic(reason);
        self.publish(session_id);
    }

    fn fail(&mut self, reason: &str) {
        let ids = self
            .sessions
            .iter()
            .filter(|(_, session)| session.prompting || !session.permissions.is_empty())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.fail_session(&id, reason);
        }
        for id in self.sessions.keys() {
            self.fleet.external_exited(RowKey {
                profile: format!("acp:{}", self.entry.id),
                session_id: id.clone(),
            });
        }
        for (_, pending) in self.pending.drain() {
            match pending {
                Pending::Create(creation) | Pending::Authenticate(creation) => {
                    let _ = creation.reply.send(Err(AcpError(safe_text(reason))));
                }
                Pending::Prompt(_) => {}
            }
        }
    }

    async fn shutdown(&mut self) {
        let sessions = self.sessions.keys().cloned().collect::<Vec<_>>();
        for session_id in &sessions {
            if let Some(session) = self.sessions.get_mut(session_id) {
                session.cancelling = true;
            }
            self.cancel_permissions(session_id).await;
        }
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, async {
            let mut pending = std::collections::HashSet::new();
            for session_id in &sessions {
                let id = self.next();
                if self
                    .request(id, "session/close", json!({"sessionId":session_id}))
                    .await
                    .is_ok()
                {
                    pending.insert(id);
                }
            }
            while !pending.is_empty() {
                match self.transport.read().await {
                    Ok(Some(frame)) => {
                        if let Some(id) = frame.get("id").and_then(Value::as_u64) {
                            pending.remove(&id);
                        }
                        self.frame(frame).await;
                    }
                    _ => break,
                }
            }
        })
        .await;
        for session_id in sessions {
            self.powers.close_session(&session_id);
            self.fleet.remove_external(RowKey {
                profile: format!("acp:{}", self.entry.id),
                session_id: session_id.clone(),
            });
            self.sessions.remove(&session_id);
        }
        self.transport.terminate().await;
    }
}

fn request_id(id: &Value) -> Option<String> {
    match id {
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(format!("string:{value}")),
        _ => None,
    }
}
