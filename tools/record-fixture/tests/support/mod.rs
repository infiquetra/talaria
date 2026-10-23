//! A scripted Hermes for the record-fixture tests.
//!
//! Two pieces. `Gateway` is an in-process WebSocket server on 127.0.0.1: it sends
//! `gateway.ready`, answers the opening handshake (`client.capabilities`,
//! `runtime.describe`) and `config.get`, and hands every other request to a script. It logs every
//! frame the client sends so a test can assert the exact keys of each request. `MockHome` adds a
//! Hermes home directory with a control socket (`identify`, `session-ticket`) so that
//! discovery and ticket minting work against it.
//!
//! This is a small copy of the approach in `crates/hermes-client/tests/support/mod.rs`; test
//! support cannot be imported across crates.

#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use hermes_client::{
    Client, ConnectOptions, Endpoint, InboundStream, Ticket, connect_with, handshake,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderValue;

pub const INSTANCE_ID: &str = "0123456789abcdef0123456789abcdef";
pub const SESSION_ID: &str = "sess-1";
pub const SUBSCRIPTION_ID: &str = "sub-1";
pub const MODEL: &str = "minimax-m3.1-flash";
pub const PROVIDER: &str = "ollama-cloud";

pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Sixteen random hex digits, different on every call.
pub fn random_hex() -> String {
    format!("{:016x}", rand::random::<u64>())
}

// ---------------------------------------------------------------------------------------------
// What the gateway saw
// ---------------------------------------------------------------------------------------------

/// One text frame the client sent.
#[derive(Clone, Debug)]
pub struct Seen {
    pub frame: Value,
}

impl Seen {
    pub fn method(&self) -> Option<&str> {
        self.frame.get("method").and_then(Value::as_str)
    }

    pub fn id(&self) -> Value {
        self.frame.get("id").cloned().unwrap_or(Value::Null)
    }

    pub fn params(&self) -> &Value {
        self.frame.get("params").unwrap_or(&Value::Null)
    }

    /// The sorted key names of `params`.
    pub fn param_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .params()
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        keys.sort();
        keys
    }
}

// ---------------------------------------------------------------------------------------------
// Frames the gateway sends
// ---------------------------------------------------------------------------------------------

pub fn result_frame(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn error_frame(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub fn event_frame(seq: u64, kind: &str, payload: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": kind,
            "session_id": SESSION_ID,
            "seq": seq,
            "payload": payload,
            "execution_generation": 1,
            "replay_epoch": "e1",
            "authority_epoch": 3
        }
    })
}

fn session_ref() -> Value {
    json!({"profile_id": "profile-1", "session_id": SESSION_ID})
}

/// `CanonicalSnapshot` for a `session.create`, echoing the requested provider, model and cwd.
/// `provider` overrides the echoed provider (to make an attempt fail the provenance check).
pub fn snapshot(params: &Value, provider_override: Option<&str>) -> Value {
    json!({
        "session_id": SESSION_ID,
        "stored_session_id": "stored-1",
        "messages": [],
        "message_count": 0,
        "running": false,
        "authority_epoch": 3,
        "replay_epoch": "e1",
        "last_sequence": 0,
        "subscription_id": SUBSCRIPTION_ID,
        "revision": 1,
        "execution_generation": 1,
        "pending": [],
        "prompts": [],
        "info": {
            "desktop_protocol": "1",
            "provider": provider_override.map_or_else(|| params["provider"].clone(), |p| json!(p)),
            "model": params["model"].clone(),
            "cwd": params["cwd"].clone()
        }
    })
}

/// `AdmissionStatus` for a `prompt.submit`.
pub fn admission(sequence: u64, status: &str) -> Value {
    json!({
        "admission_id": format!("adm-{sequence}"),
        "ref": session_ref(),
        "sequence": sequence,
        "status": status,
        "outcome": null,
        "authority_epoch": 3,
        "execution_generation": 1
    })
}

fn session_handle() -> Value {
    json!({
        "ref": session_ref(),
        "instance_id": INSTANCE_ID,
        "authority_epoch": 3,
        "revision": 2,
        "execution_generation": 1,
        "execution_state": "idle"
    })
}

// ---------------------------------------------------------------------------------------------
// The script
// ---------------------------------------------------------------------------------------------

/// What a script returns for one request: frames to send, in order.
pub struct Ctx {
    frames: Vec<Value>,
    seq: u64,
    id: Value,
}

impl Ctx {
    /// Answer the current request.
    pub fn reply(&mut self, result: Value) {
        self.frames.push(result_frame(&self.id, result));
    }

    /// Refuse the current request.
    pub fn refuse(&mut self, code: i64, message: &str) {
        self.frames.push(error_frame(&self.id, code, message));
    }

    /// Send an event; `seq` counts up over the whole connection.
    pub fn event(&mut self, kind: &str, payload: Value) {
        self.seq += 1;
        self.frames.push(event_frame(self.seq, kind, payload));
    }

    /// Send any frame as is.
    pub fn raw(&mut self, frame: Value) {
        self.frames.push(frame);
    }
}

/// A script: look at one request, push frames onto the context, return `true` when handled.
/// A request it declines gets the default answer for that method.
pub type Script = Box<dyn FnMut(&Seen, &mut Ctx) -> bool + Send + 'static>;

/// Default answers for the methods the driver uses. `provider_override` changes the provider the
/// snapshot reports.
fn default_answer(seen: &Seen, ctx: &mut Ctx, provider_override: Option<&str>) {
    let params = seen.params();
    match seen.method().unwrap_or("") {
        "session.create" => ctx.reply(snapshot(params, provider_override)),
        "session.detach" => ctx.reply(json!({
            "session_id": SESSION_ID,
            "subscription_id": SUBSCRIPTION_ID,
            "detached": true
        })),
        "session.interrupt" => ctx.reply(session_handle()),
        "approval.respond" | "clarify.respond" => ctx.reply(json!({
            "status": "resolved",
            "prompt_id": params["prompt_id"].clone()
        })),
        "prompt.submit" => ctx.reply(admission(1, "started")),
        _ => ctx.refuse(-32601, "method not found"),
    }
}

/// A script that only uses the default answers.
pub fn plain_script() -> Script {
    Box::new(|_, _| false)
}

/// A script whose `session.create` snapshot reports a different provider than asked for.
pub fn wrong_provider_script() -> Script {
    Box::new(|seen, ctx| {
        if seen.method() == Some("session.create") {
            let snapshot = snapshot(seen.params(), Some("some-other-provider"));
            ctx.reply(snapshot);
            return true;
        }
        false
    })
}

// ---------------------------------------------------------------------------------------------
// The WebSocket gateway
// ---------------------------------------------------------------------------------------------

struct GatewayState {
    seen: Mutex<Vec<Seen>>,
    script: Mutex<Script>,
    ctx_seq: Mutex<u64>,
    profile_id: String,
    approvals_mode: Value,
}

pub struct Gateway {
    origin: String,
    state: Arc<GatewayState>,
    task: JoinHandle<()>,
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Gateway {
    /// Start a gateway. `profile_id` is what `runtime.describe` reports (it must equal the id
    /// of the ticket's profile when a real ticket is used). `approvals_mode` is the JSON answer
    /// of `config.get {key: "approvals.mode"}` under `value`.
    pub async fn start(profile_id: &str, approvals_mode: Value, script: Script) -> Gateway {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|_| panic!("mock gateway bind failed"));
        let port = listener
            .local_addr()
            .unwrap_or_else(|_| panic!("mock gateway port missing"))
            .port();
        let state = Arc::new(GatewayState {
            seen: Mutex::new(Vec::new()),
            script: Mutex::new(script),
            ctx_seq: Mutex::new(0),
            profile_id: profile_id.to_owned(),
            approvals_mode,
        });
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let state = Arc::clone(&task_state);
                tokio::spawn(async move {
                    serve_connection(stream, state).await;
                });
            }
        });
        Gateway {
            origin: format!("http://127.0.0.1:{port}"),
            state,
            task,
        }
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// `127.0.0.1:<port>`: the gateway host and port.
    pub fn authority(&self) -> &str {
        self.origin.trim_start_matches("http://")
    }

    /// Every frame the client sent, in order.
    pub fn seen(&self) -> Vec<Seen> {
        lock(&self.state.seen).clone()
    }

    /// The method of every request the client sent, in order.
    pub fn methods(&self) -> Vec<String> {
        self.seen()
            .iter()
            .filter_map(|seen| seen.method().map(str::to_owned))
            .collect()
    }

    /// Every request of one method, in order.
    pub fn calls(&self, method: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.method() == Some(method))
            .collect()
    }
}

#[allow(clippy::result_large_err)]
fn echo_protocol(_request: &Request, mut response: Response) -> Result<Response, ErrorResponse> {
    response.headers_mut().insert(
        tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static("hermes-gateway-v1"),
    );
    Ok(response)
}

async fn serve_connection(stream: tokio::net::TcpStream, state: Arc<GatewayState>) {
    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, echo_protocol).await else {
        return;
    };
    let ready = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "gateway.ready",
            "payload": {
                "skin": {"name": "default"},
                "change_events": true,
                "replay_epoch": "e1",
                "heartbeat": true,
                "session_authority": true
            }
        }
    });
    if socket.send(Message::text(ready.to_string())).await.is_err() {
        return;
    }
    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            if matches!(message, Message::Close(_)) {
                break;
            }
            continue;
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let seen = Seen { frame };
        lock(&state.seen).push(seen.clone());
        if seen.method().is_none() {
            continue;
        }
        let mut ctx = Ctx {
            frames: Vec::new(),
            seq: *lock(&state.ctx_seq),
            id: seen.id(),
        };
        match seen.method().unwrap_or("") {
            "client.capabilities" => ctx.reply(json!({
                "server_requests": ["approval"],
                "declines_not_shown": true
            })),
            "runtime.describe" => ctx.reply(json!({
                "instance_id": INSTANCE_ID,
                "profile_id": state.profile_id,
                "authority_epoch": 3,
                "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"],
                "session_create": {"sources": ["cli"], "parameters": ["text"]}
            })),
            "config.get" => ctx.reply(json!({"value": state.approvals_mode})),
            _ => {
                let handled = (lock(&state.script))(&seen, &mut ctx);
                if !handled {
                    default_answer(&seen, &mut ctx, None);
                }
            }
        }
        *lock(&state.ctx_seq) = ctx.seq;
        for frame in ctx.frames {
            if socket.send(Message::text(frame.to_string())).await.is_err() {
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A connected client for run_scenario tests
// ---------------------------------------------------------------------------------------------

/// A real `hermes_client::Client` and its inbound stream, connected to a `Gateway` through the
/// normal `connect_with`, `handshake` and `Client::start` path.
pub struct Rig {
    pub client: Client,
    pub inbound: InboundStream,
    pub gateway: Gateway,
    _dir: tempfile::TempDir,
}

impl Rig {
    pub async fn new(script: Script) -> Rig {
        let gateway = Gateway::start("profile-1", json!("manual"), script).await;
        let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("temp dir failed"));
        let home = std::fs::canonicalize(dir.path()).unwrap_or_else(|_| panic!("canonical failed"));
        let identify = identify_value(&home, gateway.origin(), json!(null), &[], &[]);
        let endpoint = Endpoint::from_identify(&identify, &home)
            .unwrap_or_else(|err| panic!("endpoint failed: {err}"));
        let ticket = Ticket::parse("t".repeat(43)).unwrap_or_else(|_| panic!("ticket failed"));
        let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
            .await
            .unwrap_or_else(|err| panic!("connect failed: {err}"));
        handshake(&mut connection)
            .await
            .unwrap_or_else(|err| panic!("handshake failed: {err}"));
        let (client, inbound) = Client::start(connection.into_parts());
        Rig {
            client,
            inbound,
            gateway,
            _dir: dir,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A Hermes home with a control socket
// ---------------------------------------------------------------------------------------------

/// The `identify` result object for a ready gateway. `served` names profiles besides `default`
/// (each at `<home>/profiles/<name>`); `parked` is `(name, reason)`.
pub fn identify_value(
    home: &Path,
    origin: &str,
    code_sha: Value,
    served: &[String],
    parked: &[(String, String)],
) -> Value {
    let home_text = home.to_string_lossy().into_owned();
    let mut entries = vec![json!({"profile_id": home_text, "home": home_text})];
    for name in served {
        let path = format!("{home_text}/profiles/{name}");
        entries.push(json!({"profile_id": path, "home": path}));
    }
    let parked: serde_json::Map<String, Value> = parked
        .iter()
        .map(|(name, reason)| (name.clone(), json!(reason)))
        .collect();
    json!({
        "protocol": 1,
        "kind": "gateway",
        "pid": 4242,
        "start_time": 1790000000.5,
        "hermes_home": home_text,
        "profile": "default",
        "supervisor": "none",
        "launch_supervisor": "manual",
        "code_sha": code_sha,
        "code_version": "0.22.4",
        "instance_id": INSTANCE_ID,
        "runtime_protocol": 1,
        "authority_epoch": 3,
        "state": "ready",
        "api_origin": origin,
        "served_profiles": entries,
        "parked_profiles": parked,
        "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"]
    })
}

/// What the home reports.
pub struct HomeSpec {
    /// `identify`'s `code_sha`; `None` is JSON null.
    pub code_sha: Option<String>,
    /// Profiles served besides `default`.
    pub served: Vec<String>,
    /// Parked profiles, `(name, reason)`.
    pub parked: Vec<(String, String)>,
    /// What `config.get {key: "approvals.mode"}` returns under `value`.
    pub approvals_mode: Value,
}

impl HomeSpec {
    /// A gateway at the pinned commit serving `fixture`, in manual approvals mode.
    pub fn good() -> HomeSpec {
        HomeSpec {
            code_sha: Some(hermes_contract::pinned::COMMIT.to_owned()),
            served: vec!["fixture".to_owned()],
            parked: Vec::new(),
            approvals_mode: json!("manual"),
        }
    }
}

pub struct MockHome {
    dir: tempfile::TempDir,
    canonical: PathBuf,
    pub gateway: Gateway,
    control: JoinHandle<()>,
    verbs: Arc<Mutex<Vec<String>>>,
}

impl Drop for MockHome {
    fn drop(&mut self) {
        self.control.abort();
    }
}

fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|_| panic!("mode change failed"));
}

/// A temporary directory standing in for the Hermes home: private, no gateway in it.
pub fn empty_home() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("t")
        .rand_bytes(3)
        .tempdir()
        .unwrap_or_else(|_| panic!("temp home failed"));
    set_mode(dir.path(), 0o700);
    dir
}

impl MockHome {
    pub async fn start(spec: HomeSpec, script: Script) -> MockHome {
        let dir = empty_home();
        let canonical =
            std::fs::canonicalize(dir.path()).unwrap_or_else(|_| panic!("canonical failed"));
        let fixture_id = format!("{}/profiles/fixture", canonical.display());
        for name in &spec.served {
            let path = canonical.join("profiles").join(name);
            std::fs::create_dir_all(&path).unwrap_or_else(|_| panic!("profile dir failed"));
            set_mode(&canonical.join("profiles"), 0o700);
            set_mode(&path, 0o700);
        }
        let gateway = Gateway::start(&fixture_id, spec.approvals_mode.clone(), script).await;
        let identify = identify_value(
            &canonical,
            gateway.origin(),
            spec.code_sha.map_or(Value::Null, Value::String),
            &spec.served,
            &spec.parked,
        );
        let socket = canonical.join("gateway.sock");
        if socket.as_os_str().len() > 100 {
            panic!("TMPDIR is too long for a control socket");
        }
        let listener =
            UnixListener::bind(&socket).unwrap_or_else(|_| panic!("control bind failed"));
        set_mode(&socket, 0o600);
        let verbs = Arc::new(Mutex::new(Vec::new()));
        let task_verbs = Arc::clone(&verbs);
        let control = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let identify = identify.clone();
                let verbs = Arc::clone(&task_verbs);
                tokio::spawn(async move {
                    let _ = control_request(stream, identify, verbs).await;
                });
            }
        });
        MockHome {
            dir,
            canonical,
            gateway,
            control,
            verbs,
        }
    }

    /// The Hermes home, canonical.
    pub fn path(&self) -> &Path {
        &self.canonical
    }

    /// Every control verb received, in order.
    pub fn verbs(&self) -> Vec<String> {
        lock(&self.verbs).clone()
    }
}

async fn control_request(
    mut stream: tokio::net::UnixStream,
    identify: Value,
    verbs: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < 64 * 1024 {
        match stream.read(&mut byte).await {
            Ok(0) | Err(_) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
        }
    }
    let request: Value = serde_json::from_slice(&line).unwrap_or(Value::Null);
    let verb = request["verb"].as_str().unwrap_or("").to_owned();
    lock(&verbs).push(verb.clone());
    let reply = match verb.as_str() {
        "identify" => json!({
            "ok": true,
            "protocol": 1,
            "id": request["id"],
            "result": identify
        }),
        "session-ticket" => json!({
            "ok": true,
            "protocol": 1,
            "id": request["id"],
            "result": {
                "ticket": format!("{}{}{}", random_hex(), random_hex(), random_hex()),
                "expires_in_seconds": 30,
                "instance_id": request["params"]["instance_id"],
                "profile_id": request["params"]["profile_id"],
                "runtime_protocol": 1
            }
        }),
        _ => json!({
            "ok": false,
            "protocol": 1,
            "id": request["id"],
            "error": "unknown verb"
        }),
    };
    stream.write_all(reply.to_string().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// A stub `hermes` program
// ---------------------------------------------------------------------------------------------

/// A directory holding an executable `hermes` that appends a line to `log` when it runs.
pub struct StubHermes {
    dir: tempfile::TempDir,
}

impl StubHermes {
    pub fn new() -> StubHermes {
        let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("stub dir failed"));
        let program = dir.path().join("hermes");
        let body = format!(
            "#!/bin/sh\necho called >> \"{}\"\n",
            dir.path().join("log").display()
        );
        std::fs::write(&program, body).unwrap_or_else(|_| panic!("stub write failed"));
        set_mode(&program, 0o700);
        StubHermes { dir }
    }

    /// The value for a `PATH` that finds the stub first.
    pub fn path_with_stub(&self) -> std::ffi::OsString {
        let mut paths = vec![self.dir.path().to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        std::env::join_paths(paths).unwrap_or_else(|_| panic!("PATH join failed"))
    }

    /// True when the stub ran at least once.
    pub fn was_run(&self) -> bool {
        self.dir.path().join("log").exists()
    }
}
