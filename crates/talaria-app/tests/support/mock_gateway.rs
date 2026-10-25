//! An in-process Hermes gateway for the prompt tests.
//!
//! A real `hermes_client::Client` talks to this mock over a loopback WebSocket, the same
//! transport hermes-client's own test peer uses. The mock plays the gateway side: it writes
//! server requests and events, answers `approval.respond`, `clarify.respond`, `prompt.submit`
//! and `session.interrupt`, and records everything the client sends with a monotonic stamp.
//!
//! Where the mock builds a frame it follows the vendored contract
//! (`crates/hermes-contract/vendor/gateway-contract.openrpc.json`) or the recorded frame logs
//! under `fixtures/frames/`. Server request params are filled from the contract: every
//! `required` property gets a placeholder of its declared type, and `session_id` is the test
//! session. Optional properties are added only when a test passes them.
//!
//! Waiting helpers use a real-time limit. Tests that pause the clock should not depend on them.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use hermes_client::{
    Client, ConnectionParts, Generation, Inbound, InboundEvent, InboundStream, ServerRequest,
};
use serde_json::{Map, Value, json};
use talaria_core::session::{Event, SessionId};
use tokio::net::TcpStream;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

/// The session every frame belongs to unless a test starts the mock with another one. It is the
/// session of the recorded frame logs.
pub const SESSION_ID: &str = "local-session-1";

/// The error code hermes-client's own tests use for gateway refusals that carry `data.reason`.
pub const REFUSAL_CODE: i64 = 4001;

/// How long a waiting helper waits before it gives up and panics.
pub const WAIT_LIMIT: Duration = Duration::from_secs(10);

const CONTRACT_TEXT: &str =
    include_str!("../../../hermes-contract/vendor/gateway-contract.openrpc.json");
const APPROVAL_LOG: &str = include_str!("../../../../fixtures/frames/approval.jsonl");
const CLARIFICATION_LOG: &str = include_str!("../../../../fixtures/frames/clarification.jsonl");
const PLAIN_ANSWER_LOG: &str = include_str!("../../../../fixtures/frames/plain-answer.jsonl");
const INTERRUPTED_LOG: &str = include_str!("../../../../fixtures/frames/interrupted.jsonl");
const RESUME_LOG: &str = include_str!("../../../talaria-core/tests/data/resume_snapshot.jsonl");
const OPEN_APPROVAL_SNAPSHOT: &str = include_str!("../data/open_approval.json");
const RUNNING_TURN_SNAPSHOT: &str = include_str!("../data/running_turn.json");

/// The sixteen server requests of the pinned Hermes release, written out by hand from the
/// Hermes facts so that a test can compare them with the contract's own list.
pub const KNOWN_SERVER_REQUEST_KINDS: [&str; 16] = [
    "approval",
    "clarify",
    "display.install.sudo",
    "preview.act",
    "preview.read",
    "secret",
    "setup_beat",
    "sudo",
    "terminal.read",
    "theme.apply",
    "theme.pick",
    "tour",
    "vault.code",
    "vault.save_login",
    "vault.unlock_prompt",
    "window.read",
];

// ---------------------------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------------------------

/// The vendored contract, parsed once.
pub fn contract() -> &'static Value {
    static PARSED: OnceLock<Value> = OnceLock::new();
    PARSED.get_or_init(|| {
        serde_json::from_str(CONTRACT_TEXT).unwrap_or_else(|_| panic!("the contract is not JSON"))
    })
}

/// The names in the contract's `x-server-requests`, in contract order.
pub fn contract_server_request_kinds() -> Vec<String> {
    contract()["x-server-requests"]
        .as_array()
        .unwrap_or_else(|| panic!("the contract has no x-server-requests"))
        .iter()
        .filter_map(|entry| entry["name"].as_str().map(str::to_owned))
        .collect()
}

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let mut current = schema;
    for _ in 0..16 {
        let Some(path) = current.get("$ref").and_then(Value::as_str) else {
            break;
        };
        let Some(pointer) = path.strip_prefix('#') else {
            break;
        };
        match root.pointer(pointer) {
            Some(target) => current = target,
            None => break,
        }
    }
    current
}

fn is_null_schema(root: &Value, schema: &Value) -> bool {
    resolve(root, schema).get("type").and_then(Value::as_str) == Some("null")
}

/// A value of the schema's declared type. Objects get their required properties only.
fn fill(root: &Value, schema: &Value, depth: usize) -> Value {
    if depth > 12 {
        return Value::Null;
    }
    let schema = resolve(root, schema);
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|options| options.first())
    {
        return first.clone();
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(options) = schema.get(key).and_then(Value::as_array) {
            let pick = options
                .iter()
                .find(|option| !is_null_schema(root, option))
                .or_else(|| options.first());
            return pick.map_or(Value::Null, |option| fill(root, option, depth + 1));
        }
    }
    let declared = match schema.get("type") {
        Some(Value::String(name)) => name.as_str(),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .find(|name| *name != "null")
            .unwrap_or("null"),
        _ if schema.get("properties").is_some() => "object",
        _ => "null",
    };
    match declared {
        "string" => json!("placeholder"),
        "integer" => json!(schema.get("minimum").and_then(Value::as_i64).unwrap_or(0)),
        "number" => json!(0),
        "boolean" => json!(false),
        "array" => {
            let wanted = schema
                .get("minItems")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            match schema.get("items") {
                Some(item) if wanted > 0 => {
                    Value::Array((0..wanted).map(|_| fill(root, item, depth + 1)).collect())
                }
                _ => json!([]),
            }
        }
        "object" => {
            let properties = schema.get("properties").and_then(Value::as_object);
            let mut object = Map::new();
            for name in schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let value = properties
                    .and_then(|all| all.get(name))
                    .map_or(Value::Null, |sub| fill(root, sub, depth + 1));
                object.insert(name.to_owned(), value);
            }
            Value::Object(object)
        }
        _ => Value::Null,
    }
}

/// Params for one server request kind, built from the contract: every required property is a
/// placeholder of its declared type and `session_id` is `session_id`. Panics for a name the
/// contract does not list.
pub fn server_request_params(kind: &str, session_id: &str) -> Value {
    let root = contract();
    let entry = root["x-server-requests"]
        .as_array()
        .and_then(|all| all.iter().find(|entry| entry["name"] == kind))
        .unwrap_or_else(|| panic!("the contract lists no server request of that name"));
    let mut params = fill(root, &entry["params"][0]["schema"], 0);
    if let Some(object) = params.as_object_mut() {
        object.insert("session_id".to_owned(), json!(session_id));
    }
    params
}

/// The result a canonical method returns, built from its result schema in `x-canonical-methods`.
/// `None` when the contract has no such canonical method.
pub fn canonical_result(method: &str, session_id: &str) -> Option<Value> {
    let root = contract();
    let entry = root["x-canonical-methods"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == method)?;
    let mut result = fill(root, &entry["result"]["schema"], 0);
    if let Some(session) = result.pointer_mut("/ref/session_id") {
        *session = json!(session_id);
    }
    Some(result)
}

// ---------------------------------------------------------------------------------------------
// The recorded frame logs
// ---------------------------------------------------------------------------------------------

/// Which recorded frame log to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixture {
    Approval,
    Clarification,
    PlainAnswer,
    Interrupted,
}

fn fixture_text(fixture: Fixture) -> &'static str {
    match fixture {
        Fixture::Approval => APPROVAL_LOG,
        Fixture::Clarification => CLARIFICATION_LOG,
        Fixture::PlainAnswer => PLAIN_ANSWER_LOG,
        Fixture::Interrupted => INTERRUPTED_LOG,
    }
}

fn fixture_entries(fixture: Fixture) -> Vec<Value> {
    fixture_text(fixture)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|_| panic!("a recorded line is not JSON"))
        })
        .filter(|entry: &Value| entry["kind"] == "frame")
        .collect()
}

/// Every recorded frame, both directions, in order.
pub fn fixture_frames(fixture: Fixture) -> Vec<Value> {
    fixture_entries(fixture)
        .into_iter()
        .map(|entry| entry["frame"].clone())
        .collect()
}

/// The recorded event notifications the gateway sent, in order, as recorded.
pub fn fixture_events(fixture: Fixture) -> Vec<Value> {
    fixture_entries(fixture)
        .into_iter()
        .filter(|entry| entry["dir"] == "in")
        .map(|entry| entry["frame"].clone())
        .filter(|frame| frame["method"] == "event")
        .collect()
}

/// The first recorded event of this type. Panics when the log has none.
pub fn fixture_event(fixture: Fixture, type_name: &str) -> Value {
    fixture_events(fixture)
        .into_iter()
        .find(|frame| frame["params"]["type"] == type_name)
        .unwrap_or_else(|| panic!("the recorded log has no event of that type"))
}

/// The params of the first request the recorded client sent with this method.
pub fn fixture_outbound_params(fixture: Fixture, method: &str) -> Option<Value> {
    fixture_entries(fixture)
        .into_iter()
        .filter(|entry| entry["dir"] == "out")
        .map(|entry| entry["frame"].clone())
        .find(|frame| frame["method"] == method)
        .map(|frame| frame["params"].clone())
}

// ---------------------------------------------------------------------------------------------
// What the mock does with the client's frames
// ---------------------------------------------------------------------------------------------

/// How the mock answers an `approval.respond` or `clarify.respond` call.
#[derive(Clone, Debug)]
pub enum Reply {
    /// `{status: "resolved", prompt_id}`.
    Resolved,
    /// `{status: "already_resolved", prompt_id}`.
    AlreadyResolved,
    /// An error reply with this `data.reason`: `stale_generation`, `invalid_params` or
    /// `permission_denied`.
    Error(&'static str),
    /// No reply at all.
    Silent,
    /// A `resolved` reply written after this long.
    ResolvedAfter(Duration),
}

/// Whether a recorded frame was a request or a response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// A call: it has a `method` and an `id`.
    Request,
    /// An answer to a server request: it has an `id` and a `result` or an `error`.
    Response,
}

/// One frame the mock read from the client.
#[derive(Clone, Debug)]
pub struct Seen {
    /// Position among everything read, from 0.
    pub index: usize,
    pub kind: FrameKind,
    /// The frame's `id`, unchanged.
    pub id: Value,
    /// The method of a request.
    pub method: Option<String>,
    /// A request's `params`; a response's `result` or `error` object.
    pub params: Value,
    /// True when a response carried `error`.
    pub is_error: bool,
    /// Monotonic stamp taken when the frame was read.
    pub at: Instant,
    /// Which socket carried it. The first is 1.
    pub connection: u64,
}

/// What a written frame was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrittenKind {
    ServerRequest,
    Event,
    Reply,
}

/// One frame the mock wrote.
#[derive(Clone, Debug)]
pub struct Written {
    pub kind: WrittenKind,
    /// A server request's `srq-` id, or a reply's echoed id as text.
    pub id: Option<String>,
    /// A server request's method; `event` for an event; the call's method for a reply.
    pub method: String,
    /// An event's `type`.
    pub event_type: Option<String>,
    /// Monotonic stamp taken just before the frame went to the socket.
    pub at: Instant,
    /// Which socket carried it. The first is 1.
    pub connection: u64,
}

struct Meta {
    kind: WrittenKind,
    id: Option<String>,
    method: String,
    event_type: Option<String>,
}

enum Cmd {
    Send {
        text: String,
        meta: Meta,
        done: Option<oneshot::Sender<()>>,
    },
    Close {
        code: u16,
        done: oneshot::Sender<()>,
    },
    Kill(oneshot::Sender<()>),
}

struct State {
    session_id: String,
    connection: u64,
    commands: mpsc::UnboundedSender<Cmd>,
    seen: Vec<Seen>,
    written: Vec<Written>,
    next_srq: u64,
    next_seq: u64,
    envelope_generation: i64,
    respond_script: VecDeque<Reply>,
    respond_default: Reply,
    auto_settle: bool,
    cancel_after_response: Option<String>,
    silent_methods: HashSet<String>,
    request_methods: HashMap<String, String>,
    silent_all: bool,
    kill_after: bool,
    /// `slash.exec` answers -32601. Nothing is remembered for the next command.
    gateway_without_slash_exec: bool,
    /// `command.dispatch` answers -32601. Used only when both methods are missing.
    gateway_without_command_dispatch: bool,
    /// A turn is running, so `/plan` answers with the busy text.
    turn_running: bool,
    sess: SessionSide,
    /// Checkpoint, diff and rewind answers. Issue #233.
    rewind: RewindSide,
}

struct Shared {
    state: Mutex<State>,
    changed: Notify,
}

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared
        .state
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// The gateway half of a loopback session socket. Cheap to clone; clones share one mock.
#[derive(Clone)]
pub struct MockGateway {
    shared: Arc<Shared>,
}

async fn socket_pair() -> (ConnectionParts, WebSocketStream<TcpStream>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(address)
            .await
            .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
        stream
            .set_nodelay(true)
            .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
        let (socket, _) = tokio_tungstenite::client_async("ws://127.0.0.1/", stream)
            .await
            .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
        socket
    });
    let (server_stream, _) = listener
        .accept()
        .await
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    server_stream
        .set_nodelay(true)
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    let server = tokio_tungstenite::accept_async(server_stream)
        .await
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    let client = client
        .await
        .unwrap_or_else(|_| panic!("loopback websocket pair failed"));
    let (sink, stream) = client.split();
    (
        ConnectionParts::for_test(sink, stream, 3, Vec::new()),
        server,
    )
}

fn new_shared(
    session_id: &str,
    commands: mpsc::UnboundedSender<Cmd>,
    connection: u64,
) -> Arc<Shared> {
    Arc::new(Shared {
        state: Mutex::new(State {
            session_id: session_id.to_owned(),
            connection,
            commands,
            seen: Vec::new(),
            written: Vec::new(),
            next_srq: 1,
            next_seq: 1,
            envelope_generation: 1,
            respond_script: VecDeque::new(),
            respond_default: Reply::Resolved,
            auto_settle: false,
            cancel_after_response: None,
            silent_methods: HashSet::new(),
            request_methods: HashMap::new(),
            silent_all: false,
            kill_after: false,
            gateway_without_slash_exec: false,
            gateway_without_command_dispatch: false,
            turn_running: false,
            sess: SessionSide::new(),
            rewind: RewindSide::default(),
        }),
        changed: Notify::new(),
    })
}

impl MockGateway {
    /// Start the mock for the recorded session and a client attached to it.
    pub async fn start() -> (Self, Client, InboundStream) {
        Self::start_with_session(SESSION_ID).await
    }

    /// Start the mock for this session id and a client attached to it.
    pub async fn start_with_session(session_id: &str) -> (Self, Client, InboundStream) {
        let (parts, server) = socket_pair().await;
        let (commands, receiver) = mpsc::unbounded_channel();
        let shared = new_shared(session_id, commands.clone(), 1);
        tokio::spawn(run_socket(
            server,
            receiver,
            commands.downgrade(),
            Arc::downgrade(&shared),
            1,
        ));
        drop(commands);
        let (client, inbound) = Client::start(parts);
        (Self { shared }, client, inbound)
    }

    /// The session id frames are written for.
    pub fn session_id(&self) -> String {
        lock(&self.shared).session_id.clone()
    }

    /// Open a new socket, give it to the client with `Client::replace_connection`, and make it
    /// the mock's current socket. The old socket is dropped. Frames recorded so far are kept.
    pub async fn replace_connection(&self, client: &Client) -> Generation {
        let (parts, server) = socket_pair().await;
        let (commands, receiver) = mpsc::unbounded_channel();
        let connection = {
            let mut state = lock(&self.shared);
            state.connection += 1;
            state.commands = commands.clone();
            state.connection
        };
        tokio::spawn(run_socket(
            server,
            receiver,
            commands.downgrade(),
            Arc::downgrade(&self.shared),
            connection,
        ));
        drop(commands);
        client
            .replace_connection(parts)
            .await
            .unwrap_or_else(|_| panic!("the client was closed"))
    }

    /// How many sockets the mock has had. The first is 1.
    pub fn connection_count(&self) -> u64 {
        lock(&self.shared).connection
    }

    /// Drop the current socket without a close frame.
    pub async fn kill_connection(&self) {
        let (done, ack) = oneshot::channel();
        self.command(Cmd::Kill(done));
        let _ = ack.await;
    }

    /// Send a close frame with this code on the current socket.
    pub async fn close_connection(&self, code: u16) {
        let (done, ack) = oneshot::channel();
        self.command(Cmd::Close { code, done });
        let _ = ack.await;
    }

    fn command(&self, command: Cmd) {
        let sender = lock(&self.shared).commands.clone();
        if sender.send(command).is_err() {
            panic!("the mock socket task ended");
        }
    }

    async fn write(&self, text: String, meta: Meta) {
        let (done, ack) = oneshot::channel();
        self.command(Cmd::Send {
            text,
            meta,
            done: Some(done),
        });
        let _ = ack.await;
    }

    // -----------------------------------------------------------------------------------------
    // Behaviour switches
    // -----------------------------------------------------------------------------------------

    /// The reply to every `approval.respond` and `clarify.respond` once the queue is empty.
    /// The default is [`Reply::Resolved`].
    pub fn set_respond_reply(&self, reply: Reply) {
        lock(&self.shared).respond_default = reply;
    }

    /// Queue a reply for the next `approval.respond` or `clarify.respond`. Queued replies are
    /// used one per call, in order, before the default.
    pub fn queue_respond_reply(&self, reply: Reply) {
        lock(&self.shared).respond_script.push_back(reply);
    }

    /// After a `resolved` or `already_resolved` reply, also write the matching `*.settled`
    /// event with the prompt id and generation of the call. Off by default.
    pub fn set_auto_settle(&self, on: bool) {
        lock(&self.shared).auto_settle = on;
    }

    /// After the client answers a server request, write `request.cancel` with this reason (for
    /// example `resolved`) for that request, as Hermes does. `None` turns it off, which is the
    /// default.
    pub fn publish_cancel_after_response(&self, reason: Option<&str>) {
        lock(&self.shared).cancel_after_response = reason.map(str::to_owned);
    }

    /// Never reply to this method (`prompt.submit`, `session.interrupt`, ...) while on.
    pub fn set_silent(&self, method: &str, on: bool) {
        let mut state = lock(&self.shared);
        if on {
            state.silent_methods.insert(method.to_owned());
        } else {
            state.silent_methods.remove(method);
        }
    }

    /// The `execution_generation` written on the envelope of built events. The default is 1.
    pub fn set_envelope_generation(&self, generation: i64) {
        lock(&self.shared).envelope_generation = generation;
    }

    // -----------------------------------------------------------------------------------------
    // Server requests
    // -----------------------------------------------------------------------------------------

    /// The kinds the mock can send: the contract's `x-server-requests`.
    pub fn server_request_kinds(&self) -> Vec<String> {
        contract_server_request_kinds()
    }

    /// Write one server request with contract-built params. Returns its `srq-` id.
    pub async fn server_request(&self, kind: &str) -> String {
        self.server_request_with(kind, json!({})).await
    }

    /// Write one server request. `extra` is an object merged over the contract-built params; a
    /// `null` value removes that key. Returns its `srq-` id.
    pub async fn server_request_with(&self, kind: &str, extra: Value) -> String {
        let (session, id) = {
            let mut state = lock(&self.shared);
            let id = format!("srq-{:012x}", state.next_srq);
            state.next_srq += 1;
            state.request_methods.insert(id.clone(), kind.to_owned());
            (state.session_id.clone(), id)
        };
        let mut params = server_request_params(kind, &session);
        if let (Some(target), Some(over)) = (params.as_object_mut(), extra.as_object()) {
            for (key, value) in over {
                if value.is_null() {
                    target.remove(key);
                } else {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        let text = json!({"jsonrpc": "2.0", "id": id, "method": kind, "params": params});
        self.write(
            text.to_string(),
            Meta {
                kind: WrittenKind::ServerRequest,
                id: Some(id.clone()),
                method: kind.to_owned(),
                event_type: None,
            },
        )
        .await;
        id
    }

    /// Write one server request of every kind the contract lists. Returns `(kind, id)` pairs.
    pub async fn every_server_request(&self) -> Vec<(String, String)> {
        let mut sent = Vec::new();
        for kind in self.server_request_kinds() {
            let id = self.server_request(&kind).await;
            sent.push((kind, id));
        }
        sent
    }

    // -----------------------------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------------------------

    /// Write an event with the envelope the recorded frame logs show. `seq` counts up from 1.
    /// Returns the sequence number used.
    pub async fn send_event(&self, type_name: &str, payload: Value) -> u64 {
        let (session, seq, generation, epoch) = {
            let mut state = lock(&self.shared);
            let seq = state.next_seq;
            state.next_seq += 1;
            (
                state.session_id.clone(),
                seq,
                state.envelope_generation,
                state.sess.replay_epoch.clone(),
            )
        };
        let generation = payload
            .get("execution_generation")
            .and_then(Value::as_i64)
            .unwrap_or(generation);
        let params = json!({
            "type": type_name,
            "session_id": session,
            "seq": seq,
            "replay_epoch": epoch,
            "authority_epoch": 1,
            "execution_generation": generation,
            "admission_id": "adm-1",
            "timestamp": 1_761_177_600.066,
            "payload": payload,
        });
        self.send_event_params(params).await;
        seq
    }

    /// `skills.changed` with an empty session id and no `seq`. Issue #223.
    pub async fn skills_changed(&self) {
        self.send_event_params(json!({
            "type": "skills.changed",
            "session_id": "",
            "payload": {},
        }))
        .await;
    }

    /// Write an event whose whole `params` object the test supplies.
    pub async fn send_event_params(&self, params: Value) {
        if let Some(seq) = params["seq"].as_u64() {
            let mut s = lock(&self.shared);
            if s.next_seq <= seq {
                s.next_seq = seq + 1;
            }
        }
        // Events the mock makes up afterwards belong to the stream just replayed.
        if let Some(epoch) = params["replay_epoch"].as_str() {
            lock(&self.shared).sess.replay_epoch = epoch.to_owned();
        }
        let event_type = params["type"].as_str().map(str::to_owned);
        let text = json!({"jsonrpc": "2.0", "method": "event", "params": params});
        self.write(
            text.to_string(),
            Meta {
                kind: WrittenKind::Event,
                id: None,
                method: "event".to_owned(),
                event_type,
            },
        )
        .await;
    }

    /// Write text as one frame, unchanged.
    pub async fn send_raw_text(&self, text: &str) {
        self.write(
            text.to_owned(),
            Meta {
                kind: WrittenKind::Event,
                id: None,
                method: "raw".to_owned(),
                event_type: None,
            },
        )
        .await;
    }

    /// `approval.request` with the shape of the Hermes facts: `kind`, `prompt_id`,
    /// `execution_generation`, `command`, `description` and `choices`.
    pub async fn approval_request(
        &self,
        prompt_id: &str,
        generation: i64,
        command: &str,
        choices: &[&str],
    ) -> u64 {
        self.send_event(
            "approval.request",
            json!({
                "kind": "approval",
                "prompt_id": prompt_id,
                "execution_generation": generation,
                "command": command,
                "description": "Remove pytest cache directory",
                "choices": choices,
            }),
        )
        .await
    }

    /// `approval.request` with a payload the test builds.
    pub async fn approval_request_with(&self, payload: Value) -> u64 {
        self.send_event("approval.request", payload).await
    }

    /// `clarify.request` with `kind`, `prompt_id`, `execution_generation`, `question`,
    /// `choices` and `multi_select`.
    pub async fn clarify_request(
        &self,
        prompt_id: &str,
        generation: i64,
        question: &str,
        choices: &[&str],
        multi_select: bool,
    ) -> u64 {
        self.send_event(
            "clarify.request",
            json!({
                "kind": "clarify",
                "prompt_id": prompt_id,
                "execution_generation": generation,
                "question": question,
                "choices": choices,
                "multi_select": multi_select,
            }),
        )
        .await
    }

    /// `clarify.request` with a payload the test builds.
    pub async fn clarify_request_with(&self, payload: Value) -> u64 {
        self.send_event("clarify.request", payload).await
    }

    /// `approval.settled` with `prompt_id` and `execution_generation`.
    pub async fn approval_settled(&self, prompt_id: &str, generation: i64) -> u64 {
        self.send_event(
            "approval.settled",
            json!({"prompt_id": prompt_id, "execution_generation": generation}),
        )
        .await
    }

    /// `clarify.settled` with `prompt_id` and `execution_generation`.
    pub async fn clarify_settled(&self, prompt_id: &str, generation: i64) -> u64 {
        self.send_event(
            "clarify.settled",
            json!({"prompt_id": prompt_id, "execution_generation": generation}),
        )
        .await
    }

    /// `request.cancel` with `id`, `method` and `reason`, all strings.
    pub async fn request_cancel(&self, id: &str, method: &str, reason: &str) -> u64 {
        self.send_event(
            "request.cancel",
            json!({"id": id, "method": method, "reason": reason}),
        )
        .await
    }

    /// Replay the first `count` recorded events of a frame log, as recorded except that the
    /// session id is the mock's. Returns how many were written.
    pub async fn replay_fixture(&self, fixture: Fixture, count: usize) -> usize {
        let session = self.session_id();
        let mut written = 0;
        for frame in fixture_events(fixture).into_iter().take(count) {
            let mut params = frame["params"].clone();
            params["session_id"] = json!(session);
            self.send_event_params(params).await;
            written += 1;
        }
        written
    }

    // -----------------------------------------------------------------------------------------
    // What the client sent
    // -----------------------------------------------------------------------------------------

    /// Everything read so far, in order.
    pub fn seen(&self) -> Vec<Seen> {
        lock(&self.shared).seen.clone()
    }

    /// How many frames have been read.
    pub fn frame_count(&self) -> usize {
        lock(&self.shared).seen.len()
    }

    /// Requests read with this method, in order.
    pub fn requests(&self, method: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|frame| {
                frame.kind == FrameKind::Request && frame.method.as_deref() == Some(method)
            })
            .collect()
    }

    /// Every response frame read (answers to server requests), in order.
    pub fn answers(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|frame| frame.kind == FrameKind::Response)
            .collect()
    }

    /// Response frames read with this id.
    pub fn answers_for(&self, id: &str) -> Vec<Seen> {
        self.answers()
            .into_iter()
            .filter(|frame| frame.id.as_str() == Some(id))
            .collect()
    }

    /// Everything written so far, in order.
    pub fn written(&self) -> Vec<Written> {
        lock(&self.shared).written.clone()
    }

    /// Server requests written so far, in order.
    pub fn written_server_requests(&self) -> Vec<Written> {
        self.written()
            .into_iter()
            .filter(|frame| frame.kind == WrittenKind::ServerRequest)
            .collect()
    }

    /// The stamp of the write of the server request with this id.
    pub fn written_at(&self, id: &str) -> Option<Instant> {
        self.written_server_requests()
            .into_iter()
            .find(|frame| frame.id.as_deref() == Some(id))
            .map(|frame| frame.at)
    }

    /// How long the client took to answer a server request: the stamp of its first response
    /// frame minus the stamp of the request's write. `None` until both exist.
    pub fn answer_delay(&self, id: &str) -> Option<Duration> {
        let written = self.written_at(id)?;
        let answered = self.answers_for(id).first()?.at;
        Some(answered.saturating_duration_since(written))
    }

    /// Panics when the client wrote more than one response frame with this id.
    pub fn assert_answered_at_most_once(&self, id: &str) {
        let count = self.answers_for(id).len();
        assert!(
            count <= 1,
            "{count} response frames were written for one request id"
        );
    }

    /// Panics when the client wrote more than one response frame with the same id, for any id.
    pub fn assert_no_duplicate_answers(&self) {
        let mut ids = HashSet::new();
        for frame in self.answers() {
            assert!(
                ids.insert(frame.id.to_string()),
                "a response frame id was written twice"
            );
        }
    }

    /// Panics when two request frames carry the same id.
    pub fn assert_no_duplicate_request_ids(&self) {
        let mut ids = HashSet::new();
        for frame in self.seen() {
            if frame.kind == FrameKind::Request {
                assert!(
                    ids.insert(frame.id.to_string()),
                    "a request frame id was written twice"
                );
            }
        }
    }

    /// Wait until the nth (from 0) request with this method has been read, and return it.
    pub async fn wait_for_request(&self, method: &str, nth: usize) -> Seen {
        let method = method.to_owned();
        self.wait_for("a request", move |state| {
            state
                .seen
                .iter()
                .filter(|frame| {
                    frame.kind == FrameKind::Request && frame.method.as_deref() == Some(&method)
                })
                .nth(nth)
                .cloned()
        })
        .await
    }

    /// Wait until the nth (from 0) response frame with this id has been read, and return it.
    pub async fn wait_for_answer(&self, id: &str, nth: usize) -> Seen {
        let id = id.to_owned();
        self.wait_for("a response", move |state| {
            state
                .seen
                .iter()
                .filter(|frame| frame.kind == FrameKind::Response && frame.id.as_str() == Some(&id))
                .nth(nth)
                .cloned()
        })
        .await
    }

    /// Wait until at least `count` frames of any kind have been read.
    pub async fn wait_for_frames(&self, count: usize) {
        self.wait_for("frames", move |state| {
            (state.seen.len() >= count).then_some(())
        })
        .await;
    }

    /// Wait for `quiet` of real time, then report whether any new frame was read meanwhile.
    pub async fn no_new_frames_within(&self, quiet: Duration) -> bool {
        let before = self.frame_count();
        tokio::time::sleep(quiet).await;
        self.frame_count() == before
    }

    async fn wait_for<T>(&self, what: &str, mut check: impl FnMut(&State) -> Option<T>) -> T {
        let deadline = tokio::time::Instant::now() + WAIT_LIMIT;
        loop {
            let notified = self.shared.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(found) = check(&lock(&self.shared)) {
                return found;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                panic!("the mock waited too long for {what}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The socket task
// ---------------------------------------------------------------------------------------------

async fn run_socket(
    mut socket: WebSocketStream<TcpStream>,
    mut commands: mpsc::UnboundedReceiver<Cmd>,
    own: mpsc::WeakUnboundedSender<Cmd>,
    shared: Weak<Shared>,
    connection: u64,
) {
    let mut reading = true;
    loop {
        tokio::select! {
            biased;
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Cmd::Send { text, meta, done } => {
                        if let Some(shared) = shared.upgrade() {
                            let at = Instant::now();
                            lock(&shared).written.push(Written {
                                kind: meta.kind,
                                id: meta.id,
                                method: meta.method,
                                event_type: meta.event_type,
                                at,
                                connection,
                            });
                        }
                        let _ = socket.send(Message::text(text)).await;
                        if let Some(done) = done {
                            let _ = done.send(());
                        }
                    }
                    Cmd::Close { code, done } => {
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::from(code),
                                reason: "".into(),
                            })))
                            .await;
                        let _ = done.send(());
                    }
                    Cmd::Kill(done) => {
                        drop(socket);
                        let _ = done.send(());
                        break;
                    }
                }
            }
            incoming = socket.next(), if reading => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let at = Instant::now();
                        let Some(shared) = shared.upgrade() else { break };
                        handle_frame(&shared, &own, connection, &text, at);
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        let at = Instant::now();
                        let Some(shared) = shared.upgrade() else { break };
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            handle_frame(&shared, &own, connection, text, at);
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => reading = false,
                }
            }
        }
    }
}

/// Record one frame the client wrote and queue whatever the mock writes back.
fn handle_frame(
    shared: &Arc<Shared>,
    own: &mpsc::WeakUnboundedSender<Cmd>,
    connection: u64,
    text: &str,
    at: Instant,
) {
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let method = frame
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let id = frame.get("id").cloned();
    let (kind, params, is_error) = match (&method, &id) {
        (Some(_), Some(_)) => (FrameKind::Request, frame["params"].clone(), false),
        (None, Some(_)) if frame.get("result").is_some() || frame.get("error").is_some() => {
            let is_error = frame.get("error").is_some();
            let body = if is_error {
                &frame["error"]
            } else {
                &frame["result"]
            };
            (FrameKind::Response, body.clone(), is_error)
        }
        _ => return,
    };
    let id = id.unwrap_or(Value::Null);
    let mut outgoing: Vec<(Option<Duration>, String, Meta)> = Vec::new();
    let kill;
    {
        let mut state = lock(shared);
        let index = state.seen.len();
        state.seen.push(Seen {
            index,
            kind,
            id: id.clone(),
            method: method.clone(),
            params: params.clone(),
            is_error,
            at,
            connection,
        });
        match (kind, method.as_deref()) {
            (FrameKind::Request, Some(name)) => {
                plan_request(&mut state, name, &id, &params, &mut outgoing);
            }
            (FrameKind::Response, _) => {
                if let (Some(reason), Some(answered)) =
                    (state.cancel_after_response.clone(), id.as_str())
                {
                    let request_method = state
                        .request_methods
                        .get(answered)
                        .cloned()
                        .unwrap_or_default();
                    outgoing.push((
                        None,
                        event_text(
                            &mut state,
                            "request.cancel",
                            json!({"id": answered, "method": request_method, "reason": reason}),
                        ),
                        Meta {
                            kind: WrittenKind::Event,
                            id: None,
                            method: "event".to_owned(),
                            event_type: Some("request.cancel".to_owned()),
                        },
                    ));
                }
            }
            _ => {}
        }
        kill = std::mem::take(&mut state.kill_after);
    }
    shared.changed.notify_waiters();
    for (delay, text, meta) in outgoing {
        match delay {
            None => {
                if let Some(sender) = own.upgrade() {
                    let _ = sender.send(Cmd::Send {
                        text,
                        meta,
                        done: None,
                    });
                }
            }
            Some(delay) => {
                let own = own.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    if let Some(sender) = own.upgrade() {
                        let _ = sender.send(Cmd::Send {
                            text,
                            meta,
                            done: None,
                        });
                    }
                });
            }
        }
    }
    if kill && let Some(sender) = own.upgrade() {
        let (done, _) = oneshot::channel();
        let _ = sender.send(Cmd::Kill(done));
    }
}

fn event_text(state: &mut State, type_name: &str, payload: Value) -> String {
    let seq = state.next_seq;
    state.next_seq += 1;
    let generation = payload
        .get("execution_generation")
        .and_then(Value::as_i64)
        .unwrap_or(state.envelope_generation);
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": type_name,
            "session_id": state.session_id,
            "seq": seq,
            "replay_epoch": state.sess.replay_epoch,
            "authority_epoch": 1,
            "execution_generation": generation,
            "admission_id": "adm-1",
            "timestamp": 1_761_177_600.066,
            "payload": payload,
        },
    })
    .to_string()
}

fn reply_meta(id: &Value, method: &str) -> Meta {
    Meta {
        kind: WrittenKind::Reply,
        id: Some(id.to_string()),
        method: method.to_owned(),
        event_type: None,
    }
}

fn plan_request(
    state: &mut State,
    method: &str,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if SESSION_METHODS.contains(&method) {
        plan_session_method(state, method, id, params, outgoing);
        return;
    }
    if state.silent_all || state.silent_methods.contains(method) {
        return;
    }
    if matches!(
        method,
        "commands.catalog" | "complete.slash" | "slash.exec" | "command.dispatch"
    ) {
        plan_slash(state, method, id, params, outgoing);
        return;
    }
    if method == "approval.respond" || method == "clarify.respond" {
        let reply = state
            .respond_script
            .pop_front()
            .unwrap_or_else(|| state.respond_default.clone());
        let prompt_id = params["prompt_id"].clone();
        let (delay, status) = match &reply {
            Reply::Silent => return,
            Reply::Error(reason) => {
                let text = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": REFUSAL_CODE,
                        "message": reason,
                        "data": {"reason": reason},
                    },
                })
                .to_string();
                outgoing.push((None, text, reply_meta(id, method)));
                return;
            }
            Reply::Resolved => (None, "resolved"),
            Reply::ResolvedAfter(delay) => (Some(*delay), "resolved"),
            Reply::AlreadyResolved => (None, "already_resolved"),
        };
        let text = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"status": status, "prompt_id": prompt_id},
        })
        .to_string();
        outgoing.push((delay, text, reply_meta(id, method)));
        if state.auto_settle {
            let settled = if method == "approval.respond" {
                "approval.settled"
            } else {
                "clarify.settled"
            };
            let text = event_text(
                state,
                settled,
                json!({
                    "prompt_id": params["prompt_id"],
                    "execution_generation": params["execution_generation"],
                }),
            );
            outgoing.push((
                delay,
                text,
                Meta {
                    kind: WrittenKind::Event,
                    id: None,
                    method: "event".to_owned(),
                    event_type: Some(settled.to_owned()),
                },
            ));
        }
        return;
    }
    if method != "clarify.respond" {
        state.sess.violations.push(Violation {
            method: method.to_owned(),
            reason: "method outside the mock's list".to_owned(),
            fields: Vec::new(),
        });
    }
    let text = match canonical_result(method, &state.session_id) {
        Some(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        None => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": "unknown method",
                "data": {"reason": "unknown_method"},
            },
        }),
    };
    outgoing.push((None, text.to_string(), reply_meta(id, method)));
}

// ---------------------------------------------------------------------------------------------
// Reading what the client delivers
// ---------------------------------------------------------------------------------------------

/// The next inbound item. Panics when nothing arrives within [`WAIT_LIMIT`].
pub async fn next_inbound(stream: &mut InboundStream) -> Inbound {
    match tokio::time::timeout(WAIT_LIMIT, stream.next()).await {
        Ok(Some(item)) => item,
        Ok(None) => panic!("the inbound stream ended"),
        Err(_) => panic!("no inbound item arrived in time"),
    }
}

/// The next inbound item, which must be an event.
pub async fn next_event(stream: &mut InboundStream) -> InboundEvent {
    match next_inbound(stream).await {
        Inbound::Event(event) => event,
        _ => panic!("the next inbound item was not an event"),
    }
}

/// The next inbound item, which must be a server request.
pub async fn next_server_request(stream: &mut InboundStream) -> ServerRequest {
    match next_inbound(stream).await {
        Inbound::ServerRequest(request) => request,
        _ => panic!("the next inbound item was not a server request"),
    }
}

/// The reducer input for an event the client delivered, built as the application builds it.
pub fn session_event(event: &InboundEvent) -> Event {
    Event::Session {
        session_id: SessionId::new(event.raw["session_id"].as_str().unwrap_or_default()),
        frame: event.frame.clone(),
        raw: event.raw.clone(),
    }
}

// ---------------------------------------------------------------------------------------------
// The session side: handshake methods, the six session methods, and the request checks
// ---------------------------------------------------------------------------------------------

/// The identity the mock's gateway reports.
pub const INSTANCE_ID: &str = "0123456789abcdef0123456789abcdef";
/// The code revision the mock's gateway reports.
pub const CODE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The handshake, session, finite-turn, and model methods the mock answers.
pub const SESSION_METHODS: [&str; 18] = [
    "runtime.describe",
    "client.capabilities",
    "session.create",
    "session.resume",
    "session.list",
    "session.detach",
    "prompt.submit",
    "session.interrupt",
    "prompt.receipt",
    "prompt.cancel",
    "model.options",
    "config.get",
    "config.set",
    "session.mutate",
    "rollback.list",
    "rollback.diff",
    "rollback.restore",
    "rollback.session_diff",
];

/// Finite-turn scenarios, enabled only by headless flow tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessScenario {
    Complete,
    BeforeReply,
    ReceiptFailure,
    StartedStop,
    QueuedStop,
    ServerRequest,
}

/// What the mock does with the next request of a method.
#[derive(Clone, Debug)]
pub enum Behavior {
    /// Answer as the gateway would.
    Normal,
    /// Read the frame and never answer.
    Silent,
    /// Answer after this long.
    Delay(Duration),
    /// Close the socket as soon as the frame is read. Nothing is done and nothing is answered.
    DropAfterFrame,
    /// Do the work (a prompt is admitted, a session is created), then close the socket without
    /// answering.
    DropBeforeReply,
    /// Answer with a refusal carrying this `data.reason`, without doing the work.
    Refuse(String),
    /// `prompt.submit` only: answer `{"voice_stopped": true}`. The one reply that is not checked
    /// against the contract.
    VoiceStopped,
}

/// A request the mock found wrong. Tests assert there are none.
#[derive(Clone, Debug)]
pub struct Violation {
    pub method: String,
    pub reason: String,
    /// The offending keys of an `invalid_params` answer.
    pub fields: Vec<String>,
}

/// One time the client took a subscription to a session (a create or a resume).
#[derive(Clone, Debug)]
pub struct AttachSeen {
    pub session_id: String,
    pub subscription_id: String,
    /// `session.create` or `session.resume`.
    pub via: String,
    pub at: Instant,
}

/// One `session.detach` the mock read.
#[derive(Clone, Debug)]
pub struct DetachSeen {
    pub session_id: String,
    pub subscription_id: String,
    /// True when the params carried the key `end_reason`, whatever its value.
    pub had_end_reason: bool,
    /// What the mock answered.
    pub detached: bool,
    pub at: Instant,
}

/// One prompt the mock admitted.
#[derive(Clone, Debug)]
pub struct Submission {
    pub session_id: String,
    pub text: String,
    pub submission_id: String,
    pub at: Instant,
}

struct SessionSide {
    headless: Option<HeadlessScenario>,
    profile_id: String,
    sources: Vec<String>,
    parameters: Vec<String>,
    create_template: Value,
    resume_snapshot: Value,
    resume_queue: VecDeque<Value>,
    latest_missing: bool,
    sessions: HashMap<String, Value>,
    request_ids: HashMap<String, String>,
    created: Vec<String>,
    attaches: Vec<AttachSeen>,
    live_subscriptions: HashSet<String>,
    gens: HashMap<String, i64>,
    replay_epoch: String,
    behaviors: HashMap<String, VecDeque<Behavior>>,
    defaults: HashMap<String, Behavior>,
    list_rows: HashMap<String, Vec<Value>>,
    list_errors: HashMap<String, String>,
    detached_override: Option<bool>,
    detaches: Vec<DetachSeen>,
    submissions: Vec<Submission>,
    admitted: u64,
    replay_once: HashMap<String, VecDeque<Vec<Value>>>,
    replay_always: HashMap<String, Vec<Value>>,
    violations: Vec<Violation>,
    describe_authority_epoch: u64,
    /// Scripted `model.options` result when the request names a session.
    options_with_session: Value,
    /// Scripted `model.options` result when the request has no session.
    options_profile: Value,
    /// When set, `model.options` is refused with this reason instead of a catalogue.
    options_fail: Option<String>,
    /// `config.get` value for the reasoning key. Null is a value the client can read.
    reasoning_value: Value,
    mutate_revision: i64,
    mutate_generation: i64,
    /// `normal`, `busy`, `guarded`, `guarded-twice`, `unresolvable`, or `drop`.
    mutate_mode: String,
    /// Stored mutate answers, keyed by `request_id`: the fingerprint and the result.
    receipts: HashMap<String, (Value, Value)>,
    /// `session.create` reasoning words that are refused.
    refused_efforts: Vec<String>,
    /// A `session.create` parameter name to omit from `runtime.describe`.
    describe_without: Option<String>,
    /// `normal`, `refuse`, or `drop` for `config.set`.
    config_set_mode: String,
    /// The handler did the work and the reply must not be written.
    suppress_reply: bool,
}

impl SessionSide {
    fn new() -> Self {
        Self {
            headless: None,
            profile_id: "fixture".to_owned(),
            sources: vec!["tui".to_owned(), "cli".to_owned()],
            parameters: [
                "source",
                "request_id",
                "cwd",
                "model",
                "provider",
                "reasoning",
            ]
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
            create_template: recorded_create_snapshot(),
            resume_snapshot: default_resume_snapshot(),
            resume_queue: VecDeque::new(),
            latest_missing: false,
            sessions: HashMap::new(),
            request_ids: HashMap::new(),
            created: Vec::new(),
            attaches: Vec::new(),
            live_subscriptions: HashSet::new(),
            gens: HashMap::new(),
            replay_epoch: "id-0002".to_owned(),
            behaviors: HashMap::new(),
            defaults: HashMap::new(),
            list_rows: HashMap::new(),
            list_errors: HashMap::new(),
            detached_override: None,
            detaches: Vec::new(),
            submissions: Vec::new(),
            admitted: 0,
            replay_once: HashMap::new(),
            replay_always: HashMap::new(),
            violations: Vec::new(),
            describe_authority_epoch: 3,
            options_with_session: json!({"model": "", "provider": "", "providers": []}),
            options_profile: json!({"model": "", "provider": "", "providers": []}),
            options_fail: None,
            reasoning_value: json!("high"),
            mutate_revision: 0,
            mutate_generation: 0,
            mutate_mode: "normal".to_owned(),
            receipts: HashMap::new(),
            refused_efforts: Vec::new(),
            describe_without: None,
            config_set_mode: "normal".to_owned(),
            suppress_reply: false,
        }
    }
}

/// The result of the first request with this method in a recorded frame log, found through the
/// request's id. Panics when the log has none.
fn recorded_result(log: &str, method: &str) -> Value {
    let frames: Vec<Value> = log
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|_| panic!("a recorded line is not JSON"))
        })
        .filter(|entry: &Value| entry["kind"] == "frame")
        .collect();
    let id = frames
        .iter()
        .find(|entry| entry["dir"] == "out" && entry["frame"]["method"] == method)
        .map(|entry| entry["frame"]["id"].clone())
        .unwrap_or_else(|| panic!("the recorded log has no such request"));
    frames
        .iter()
        .find(|entry| {
            entry["dir"] == "in"
                && entry["frame"]["id"] == id
                && entry["frame"].get("result").is_some()
        })
        .map(|entry| entry["frame"]["result"].clone())
        .unwrap_or_else(|| panic!("the recorded log has no such result"))
}

/// The `session.create` result of the recorded plain-answer log: session `local-session-1`,
/// stored id `stored-1`, `execution_generation` 1, folder `/work/proj`.
pub fn recorded_create_snapshot() -> Value {
    let mut snapshot = recorded_result(PLAIN_ANSWER_LOG, "session.create");
    // A live recording carries the ids, epoch and folder of the machine it was made on; the
    // mock serves the fixed ones the tests name.
    snapshot["session_id"] = json!("local-session-1");
    snapshot["stored_session_id"] = json!("stored-1");
    snapshot["subscription_id"] = json!("sub-1");
    snapshot["replay_epoch"] = json!("id-0002");
    snapshot["authority_epoch"] = json!(1);
    snapshot["revision"] = json!(1);
    snapshot["execution_generation"] = json!(1);
    snapshot["info"]["cwd"] = json!("/work/proj");
    // The recorder withholds some `info` objects as the text `[redacted]`, which is not the
    // shape the contract gives them; the mock leaves those keys out.
    if let Some(info) = snapshot["info"].as_object_mut() {
        info.retain(|_, value| value != "[redacted]");
    }
    snapshot
}

/// The `session.resume` result of talaria-core's paired resume snapshot, as recorded: three
/// messages and one open approval in `prompts`.
pub fn recorded_resume_snapshot() -> Value {
    recorded_result(RESUME_LOG, "session.resume")
}

/// The snapshot a resume answers unless a test sets another: the recorded one with no open
/// prompt and the folder `/work/proj`.
fn default_resume_snapshot() -> Value {
    let mut snapshot = recorded_resume_snapshot();
    snapshot["prompts"] = json!([]);
    snapshot["info"]["cwd"] = json!("/work/proj");
    snapshot
}

/// A snapshot file of this crate's test data: `open_approval.json` or `running_turn.json`.
pub fn data_snapshot(name: &str) -> Value {
    let text = match name {
        "open_approval.json" => OPEN_APPROVAL_SNAPSHOT,
        "running_turn.json" => RUNNING_TURN_SNAPSHOT,
        _ => panic!("unknown snapshot file"),
    };
    serde_json::from_str(text).unwrap_or_else(|_| panic!("a snapshot file is not JSON"))
}

/// One `session.list` row with every required field. `started_at` is seconds since the epoch.
pub fn list_row(
    session_id: &str,
    title: &str,
    started_at: Option<f64>,
    model: Option<&str>,
    message_count: u64,
    running: bool,
) -> Value {
    json!({
        "session_id": session_id,
        "id": session_id,
        "title": title,
        "source": "tui",
        "started_at": started_at,
        "message_count": message_count,
        "running": running,
        "model": model,
    })
}

fn param_schema(method: &str) -> Option<&'static Value> {
    let root = contract();
    ["x-canonical-methods", "methods"].iter().find_map(|list| {
        root[*list]
            .as_array()?
            .iter()
            .find(|entry| entry["name"] == method)
            .map(|entry| resolve(root, &entry["params"][0]["schema"]))
    })
}

/// The keys this client may send and the keys it must send, on top of the contract's own
/// closed set.
fn client_shape(method: &str, params: &Value) -> (Vec<&'static str>, Vec<&'static str>) {
    match method {
        "session.create" => (
            vec!["request_id", "source", "cwd"],
            vec![
                "request_id",
                "source",
                "cwd",
                "model",
                "provider",
                "reasoning",
            ],
        ),
        "session.resume" if params.get("latest").is_some() => (
            vec!["latest", "workspace"],
            vec!["latest", "workspace", "display_projection"],
        ),
        "session.resume" => (vec!["session_id"], vec!["session_id", "display_projection"]),
        "session.list" => (vec!["scope", "limit"], vec!["scope", "limit"]),
        "session.detach" => (
            vec!["session_id", "subscription_id"],
            vec!["session_id", "subscription_id", "end_reason"],
        ),
        "prompt.submit" => {
            let mut keys = vec!["session_id", "text", "submission_id"];
            if params.get("finite").is_some() {
                keys.push("finite");
            }
            (keys.clone(), keys)
        }
        "prompt.receipt" => (
            vec!["session_id", "admission_id", "include_result"],
            vec!["session_id", "admission_id", "include_result"],
        ),
        "prompt.cancel" => (
            vec!["session_id", "admission_id"],
            vec!["session_id", "admission_id"],
        ),
        "session.interrupt" => {
            let keys = vec!["session_id", "execution_generation"];
            (keys.clone(), keys)
        }
        "client.capabilities" => (Vec::new(), vec!["server_requests"]),
        "commands.catalog" => (Vec::new(), vec!["profile", "session_id"]),
        "complete.slash" => (Vec::new(), vec!["profile", "session_id", "text"]),
        "slash.exec" => (
            vec!["command", "session_id"],
            vec!["command", "profile", "session_id"],
        ),
        "command.dispatch" => (vec!["name"], vec!["arg", "name", "profile", "session_id"]),
        "model.options" => (
            Vec::new(),
            vec![
                "session_id",
                "explicit_only",
                "include_unconfigured",
                "refresh",
                "profile",
            ],
        ),
        "config.get" => (vec!["key"], vec!["session_id", "profile", "key", "cwd"]),
        "config.set" => (
            vec!["key", "value"],
            vec!["session_id", "profile", "key", "value", "scope"],
        ),
        "session.mutate" => (
            vec![
                "session_id",
                "request_id",
                "expected_revision",
                "operation",
                "payload",
            ],
            vec![
                "session_id",
                "request_id",
                "expected_revision",
                "expected_generation",
                "operation",
                "payload",
            ],
        ),
        "rollback.list" | "rollback.session_diff" => (vec!["session_id"], vec!["session_id"]),
        "rollback.diff" => (vec!["session_id", "hash"], vec!["session_id", "hash"]),
        "rollback.restore" => (
            vec!["session_id", "hash"],
            vec!["session_id", "hash", "file_path", "all"],
        ),
        _ => (Vec::new(), Vec::new()),
    }
}

/// The keys that make `params` wrong: not in the contract's closed key set, missing from the
/// contract's required keys, or outside the keys this client is meant to send. Empty when the
/// request is fine.
fn check_params(method: &str, params: &Value) -> Vec<String> {
    let empty = Map::new();
    let object = match params {
        Value::Object(object) => object,
        Value::Null => &empty,
        _ => return vec!["params".to_owned()],
    };
    let mut fields = std::collections::BTreeSet::new();
    if let Some(schema) = param_schema(method) {
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for key in object.keys() {
                if !properties.contains_key(key) {
                    fields.insert(key.clone());
                }
            }
            for (key, value) in object {
                let allowed = properties
                    .get(key)
                    .and_then(|property| property.get("enum"))
                    .and_then(Value::as_array);
                if let (Some(allowed), Some(_)) = (allowed, value.as_str())
                    && !allowed.contains(value)
                {
                    fields.insert(key.clone());
                }
            }
        }
        for name in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !object.contains_key(name) {
                fields.insert(name.to_owned());
            }
        }
    }
    let (required, allowed) = client_shape(method, params);
    if method == "prompt.submit"
        && object
            .get("finite")
            .is_some_and(|value| value != &json!(true))
    {
        fields.insert("finite".to_owned());
    }
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            fields.insert(key.clone());
        }
    }
    for name in required {
        if !object.contains_key(name) {
            fields.insert(name.to_owned());
        }
    }
    if let Some(latest) = object.get("latest")
        && !matches!(latest.as_str(), Some("tui" | "cli"))
    {
        fields.insert("latest".to_owned());
    }
    if let Some(projection) = object.get("display_projection")
        && !projection.is_boolean()
    {
        fields.insert("display_projection".to_owned());
    }
    if matches!(
        method,
        "commands.catalog" | "complete.slash" | "slash.exec" | "command.dispatch"
    ) {
        for (key, value) in object {
            if value.is_null() {
                fields.insert(key.clone());
            }
        }
    }
    fields.into_iter().collect()
}

/// Catalogue, completion, and slash execution. Issue #223.
fn unknown_method(id: &Value, method: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32601,
            "message": format!("unknown method: {method}"),
            "data": {"reason": "unknown_method"},
        },
    })
    .to_string()
}

fn plan_slash(
    state: &mut State,
    method: &str,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if state.silent_all || state.silent_methods.contains(method) {
        return;
    }
    let fields = check_params(method, params);
    if !fields.is_empty() {
        state.sess.violations.push(Violation {
            method: method.to_owned(),
            reason: "invalid_params".to_owned(),
            fields: fields.clone(),
        });
        outgoing.push((
            None,
            refusal_text(id, "invalid_params", Some(&fields)),
            reply_meta(id, method),
        ));
        return;
    }
    if matches!(method, "slash.exec" | "command.dispatch") {
        let session = params["session_id"].as_str();
        if session.is_some() && session != Some(state.session_id.as_str()) {
            outgoing.push((
                None,
                refusal_text(id, "not_found", None),
                reply_meta(id, method),
            ));
            return;
        }
    }
    if method == "slash.exec" && state.gateway_without_slash_exec {
        outgoing.push((None, unknown_method(id, method), reply_meta(id, method)));
        return;
    }
    if method == "command.dispatch" && state.gateway_without_command_dispatch {
        outgoing.push((None, unknown_method(id, method), reply_meta(id, method)));
        return;
    }
    if method == "slash.exec" {
        let command = params["command"].as_str().unwrap_or("");
        let name = command.split_whitespace().next().unwrap_or("");
        if name == "worktree" {
            let text = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": REFUSAL_CODE,
                    "message": "unsupported_command",
                    "data": {
                        "reason": "unsupported_command",
                        "detail": "Start in a fresh worktree with hermes -w, or use the classic CLI.",
                    },
                },
            })
            .to_string();
            outgoing.push((None, text, reply_meta(id, method)));
            return;
        }
        if name == "nonexistent-talaria-check" {
            let text = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": REFUSAL_CODE,
                    "message": "unsupported_command",
                    "data": {"reason": "unsupported_command"},
                },
            })
            .to_string();
            outgoing.push((None, text, reply_meta(id, method)));
            return;
        }
    }
    let result = slash_result(method, params, state.turn_running);
    let text = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
    outgoing.push((None, text, reply_meta(id, method)));
}

fn slash_result(method: &str, params: &Value, turn_running: bool) -> Value {
    match method {
        "commands.catalog" => json!({
            "pairs": [
                ["/new", "Start a new session (usage: /new [name])"],
                ["/help", "Show available commands (usage: /help [skills|<filter>])"],
                ["/version", "Show the Hermes version"],
                ["/plan", "Plan a task (usage: /plan <task>)"],
                ["/quit", "Leave (usage: /quit)"],
                ["/fixture-user-1", "Fixture user command 1"]
            ],
            "canon": {"/reset": "/new", "/new": "/new"},
            "commands": {
                "/new": {"argument_mode": "text"},
                "/help": {"argument_mode": "text"},
                "/plan": {"argument_mode": "text"},
                "/version": {}
            },
            "categories": [
                {"name": "Session", "pairs": [["/new", "Start a new session (usage: /new [name])"]]},
                {"name": "Info", "pairs": [
                    ["/help", "Show available commands (usage: /help [skills|<filter>])"],
                    ["/version", "Show the Hermes version"],
                    ["/plan", "Plan a task (usage: /plan <task>)"]
                ]},
                {"name": "Exit", "pairs": [["/quit", "Leave (usage: /quit)"]]},
                {"name": "User commands", "pairs": [["/fixture-user-1", "Fixture user command 1"]]}
            ],
            "skills": {"/fixture-skill-1": {"usage": 3, "origin": "local"}},
            "skill_count": 1,
            "warning": "fixture warning",
            "plugin_discovery_error": "fixture warning",
            "plugin_commands": {}
        }),
        "complete.slash" => {
            let text = params["text"].as_str().unwrap_or("");
            if text.contains(' ') {
                json!({
                    "items": [{
                        "text": "alice",
                        "display": "alice",
                        "meta": "a friendly assistant",
                        "kind": "command"
                    }],
                    "replace_from": 13
                })
            } else {
                json!({
                    "items": [{
                        "text": "help",
                        "display": "/help",
                        "meta": "Show available commands (usage: /help [skills|<filter>])",
                        "kind": "command"
                    }],
                    "replace_from": 1
                })
            }
        }
        "command.dispatch" => {
            json!({"type": "exec", "output": "Hermes Agent v0.22.4\nPython 3.12.0"})
        }
        _ => {
            let command = params["command"].as_str().unwrap_or("");
            let name = command.split_whitespace().next().unwrap_or("");
            if name == "plan" && turn_running {
                json!({
                    "type": "exec",
                    "output": "⏳ Agent is running — `/plan` can't run mid-turn. Wait for the current response or `/stop` first."
                })
            } else if name == "plan" {
                json!({
                    "type": "send",
                    "message": "[/plan — plan mode]\n\nTask to plan:\nwrite a haiku about rain\n",
                    "display": "/plan write a haiku about rain",
                    "notice": "Planning: write a haiku about rain"
                })
            } else if name == "fixture-skill-1" {
                json!({
                    "type": "skill",
                    "name": "fixture-skill-1",
                    "message": "[IMPORTANT: The user has invoked fixture-skill-1]\nFixture skill body.",
                    "display": "/fixture-skill-1 tidy"
                })
            } else if name == "version" {
                json!({"type": "exec", "output": "Hermes Agent v0.22.4\nPython 3.12.0"})
            } else {
                json!({"type": "exec", "output": "(no output)"})
            }
        }
    }
}

fn refusal_text(id: &Value, reason: &str, fields: Option<&[String]>) -> String {
    let mut data = json!({"reason": reason});
    if let Some(fields) = fields {
        data["fields"] = json!(fields);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": REFUSAL_CODE, "message": reason, "data": data},
    })
    .to_string()
}

fn event_meta(type_name: &str) -> Meta {
    Meta {
        kind: WrittenKind::Event,
        id: None,
        method: "event".to_owned(),
        event_type: Some(type_name.to_owned()),
    }
}

fn next_behavior(state: &mut State, method: &str) -> Behavior {
    state
        .sess
        .behaviors
        .get_mut(method)
        .and_then(VecDeque::pop_front)
        .or_else(|| state.sess.defaults.get(method).cloned())
        .unwrap_or(Behavior::Normal)
}

/// Plan the answer to one handshake or session request.
fn plan_session_method(
    state: &mut State,
    method: &str,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    let mut fields = check_params(method, params);
    if method == "prompt.submit"
        && state.sess.headless.is_some()
        && params.get("finite") != Some(&json!(true))
    {
        fields.push("finite".to_owned());
    }
    if !fields.is_empty() {
        state.sess.violations.push(Violation {
            method: method.to_owned(),
            reason: "invalid_params".to_owned(),
            fields: fields.clone(),
        });
    }
    let behavior = next_behavior(state, method);
    if state.silent_all
        || state.silent_methods.contains(method)
        || matches!(behavior, Behavior::Silent)
    {
        return;
    }
    if matches!(behavior, Behavior::DropAfterFrame) {
        state.kill_after = true;
        return;
    }
    if !fields.is_empty() {
        outgoing.push((
            None,
            refusal_text(id, "invalid_params", Some(&fields)),
            reply_meta(id, method),
        ));
        return;
    }
    let mut delay = None;
    match &behavior {
        Behavior::Delay(wait) => delay = Some(*wait),
        Behavior::Refuse(reason) => {
            outgoing.push((None, refusal_text(id, reason, None), reply_meta(id, method)));
            return;
        }
        _ => {}
    }
    let voice = matches!(behavior, Behavior::VoiceStopped);
    let result = match method {
        "runtime.describe" => Ok(describe_result(state)),
        "client.capabilities" => Ok(json!({
            "server_requests": contract_server_request_kinds(),
            "declines_not_shown": true,
        })),
        "session.create" => handle_create(state, params),
        "session.resume" => handle_resume(state, params),
        "session.list" => handle_list(state, params),
        "session.detach" => Ok(handle_detach(state, params)),
        "prompt.submit" => Ok(handle_submit(state, params, voice)),
        "prompt.receipt" => Ok(headless_receipt(state, params)),
        "prompt.cancel" => Ok(headless_receipt(state, params)),
        "model.options" => handle_model_options(state, params),
        "config.get" => handle_config_get(state, params),
        "config.set" => handle_config_set(state, params),
        "session.mutate" if params["operation"] == "rewind" => handle_rewind_mutate(state, params),
        "session.mutate" => handle_session_mutate(state, params),
        "rollback.list" => Ok(handle_rollback_list(state)),
        "rollback.diff" => Ok(handle_rollback_diff(state)),
        "rollback.restore" => handle_rollback_restore(state, params),
        "rollback.session_diff" => Ok(handle_rollback_session_diff(state)),
        _ => handle_interrupt(state, params),
    };
    if matches!(behavior, Behavior::DropBeforeReply) {
        state.rewind.after_reply.clear();
        state.kill_after = true;
        return;
    }
    let result = match result {
        Ok(result) => result,
        Err(reason) => {
            push_method_error(id, method, &reason, state, outgoing);
            return;
        }
    };
    if state.sess.suppress_reply {
        state.sess.suppress_reply = false;
        state.kill_after = true;
        return;
    }
    let text = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
    outgoing.push((delay, text, reply_meta(id, method)));
    flush_after_reply(state, delay, outgoing);
    if voice {
        return;
    }
    let frames = state
        .sess
        .replay_once
        .get_mut(method)
        .and_then(VecDeque::pop_front)
        .or_else(|| state.sess.replay_always.get(method).cloned());
    if let Some(frames) = frames {
        for (text, meta) in replay_texts(state, &frames) {
            outgoing.push((delay, text, meta));
        }
    }
    if method == "prompt.submit" && state.sess.headless == Some(HeadlessScenario::BeforeReply) {
        outgoing.rotate_left(1);
    }
    if state.sess.headless.is_some() && matches!(method, "prompt.cancel" | "session.interrupt") {
        let mut frames = headless_events();
        let mut frame = frames.pop().expect("recorded completion");
        frame["params"]["payload"]["text"] = json!("");
        frame["params"]["payload"]["content"] = json!("");
        frame["params"]["payload"]["outcome"] = json!("cancelled");
        frame["params"]["payload"]["status"] = json!("interrupted");
        if method == "prompt.cancel" {
            frame["params"]
                .as_object_mut()
                .expect("params")
                .remove("execution_generation");
            frame["params"]["payload"]
                .as_object_mut()
                .expect("payload")
                .remove("status");
        }
        for (text, meta) in replay_texts(state, &[frame]) {
            outgoing.insert(0, (delay, text, meta));
        }
    }
}

fn headless_receipt(state: &State, params: &Value) -> Value {
    let stopped = matches!(
        state.sess.headless,
        Some(HeadlessScenario::StartedStop | HeadlessScenario::QueuedStop)
    );
    let mut receipt = json!({
        "admission_id": params["admission_id"],
        "ref": {"profile_id": state.sess.profile_id, "session_id": params["session_id"]},
        "sequence": 1, "status": "terminal",
        "outcome": if stopped { "cancelled" } else { "completed" },
        "authority_epoch": 1,
        "execution_generation": state.sess.gens.get(params["session_id"].as_str().unwrap_or_default()),
    });
    if params["include_result"] == true {
        receipt["result"] = json!({
            "completed": !stopped, "failed": false, "partial": false,
            "interrupted": stopped, "final_response": if stopped { "" } else { "5555" },
            "turn_exit_reason": if stopped { "interrupted" } else { "text_response(stop)" },
            "input_tokens": 12, "output_tokens": 4, "total_tokens": 16, "api_calls": 1,
        });
    }
    receipt
}

/// The committed finite-turn recording, with the admission allocated by this mock.
pub fn headless_events() -> Vec<Value> {
    include_str!("../data/headless/finite_turn.frames.jsonl")
        .lines()
        .map(|line| {
            let mut frame: Value = serde_json::from_str(line).expect("recorded finite frame");
            frame["params"]["admission_id"] = json!("adm-1");
            if frame["params"]["type"] == "message.complete" {
                frame["params"]["payload"]["admission_id"] = json!("adm-1");
            }
            frame
        })
        .collect()
}

fn describe_result(state: &State) -> Value {
    let mut parameters = state.sess.parameters.clone();
    if let Some(missing) = &state.sess.describe_without {
        parameters.retain(|name| name != missing);
    }
    json!({
        "instance_id": INSTANCE_ID,
        "profile_id": state.sess.profile_id,
        "authority_epoch": state.sess.describe_authority_epoch,
        "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"],
        "session_create": {
            "sources": state.sess.sources,
            "parameters": parameters,
        },
    })
}

/// Give a snapshot to the client: fresh subscription after the first, generation and sequence
/// brought up to date, and the mock now writes frames for this session.
fn attach(state: &mut State, mut snapshot: Value, via: &str) -> Value {
    let session_id = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if !state.sess.attaches.is_empty() {
        snapshot["subscription_id"] = json!(format!("sub-{}", state.sess.attaches.len() + 1));
    }
    let subscription = snapshot["subscription_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    state.sess.live_subscriptions.insert(subscription.clone());
    let own_generation = snapshot["execution_generation"].as_i64().unwrap_or(1);
    let generation = *state
        .sess
        .gens
        .entry(session_id.clone())
        .or_insert(own_generation);
    snapshot["execution_generation"] = json!(generation);
    let last = snapshot["last_sequence"].as_u64().unwrap_or(0);
    state.next_seq = state.next_seq.max(last + 1);
    snapshot["last_sequence"] = json!(state.next_seq - 1);
    if let Some(epoch) = snapshot["replay_epoch"].as_str() {
        state.sess.replay_epoch = epoch.to_owned();
    }
    state.session_id = session_id.clone();
    state
        .sess
        .sessions
        .insert(session_id.clone(), snapshot.clone());
    state.sess.attaches.push(AttachSeen {
        session_id,
        subscription_id: subscription,
        via: via.to_owned(),
        at: Instant::now(),
    });
    snapshot
}

fn handle_create(state: &mut State, params: &Value) -> Result<Value, String> {
    if let Some(effort) = params.get("reasoning").and_then(Value::as_str)
        && state.sess.refused_efforts.iter().any(|item| item == effort)
    {
        return Err("field:reasoning".to_owned());
    }
    let request_id = params["request_id"].as_str().unwrap_or_default().to_owned();
    if let Some(session_id) = state.sess.request_ids.get(&request_id).cloned()
        && let Some(snapshot) = state.sess.sessions.get(&session_id).cloned()
    {
        return Ok(attach(state, snapshot, "session.create"));
    }
    let number = state.sess.created.len() + 1;
    let mut snapshot = state.sess.create_template.clone();
    if number > 1 {
        snapshot["session_id"] = json!(format!("local-session-{number}"));
        snapshot["stored_session_id"] = json!(format!("stored-{number}"));
    }
    snapshot["info"]["cwd"] = params["cwd"].clone();
    snapshot["info"]["profile_id"] = json!(state.sess.profile_id);
    for key in ["model", "provider"] {
        if let Some(value) = params.get(key).filter(|value| value.is_string()) {
            snapshot["info"][key] = value.clone();
        }
    }
    let session_id = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    state
        .sess
        .request_ids
        .insert(request_id, session_id.clone());
    state.sess.created.push(session_id);
    Ok(attach(state, snapshot, "session.create"))
}

fn handle_resume(state: &mut State, params: &Value) -> Result<Value, String> {
    let projection = params.get("display_projection") == Some(&Value::Bool(true));
    let snapshot = if let Some(queued) = state.sess.resume_queue.pop_front() {
        queued
    } else if params.get("latest").is_some() {
        if state.sess.latest_missing {
            return Err("not_found".to_owned());
        }
        state.sess.resume_snapshot.clone()
    } else {
        let wanted = params["session_id"].as_str().unwrap_or_default();
        if let Some(stored) = state.sess.sessions.get(wanted) {
            stored.clone()
        } else if state.sess.resume_snapshot["session_id"] == wanted {
            state.sess.resume_snapshot.clone()
        } else {
            return Err("not_found".to_owned());
        }
    };
    let mut reply = attach(state, snapshot, "session.resume");
    if projection {
        if reply["display_messages"].is_null() {
            reply["display_messages"] = reply["messages"].clone();
        }
    } else if let Some(object) = reply.as_object_mut() {
        object.remove("display_messages");
    }
    Ok(reply)
}

fn handle_list(state: &State, params: &Value) -> Result<Value, String> {
    let scope = params["scope"].as_str().unwrap_or("live");
    if let Some(reason) = state.sess.list_errors.get(scope) {
        return Err(reason.clone());
    }
    let limit = params["limit"].as_u64().unwrap_or(200) as usize;
    let rows: Vec<Value> = state
        .sess
        .list_rows
        .get(scope)
        .map(|rows| rows.iter().take(limit).cloned().collect())
        .unwrap_or_default();
    Ok(json!({"sessions": rows, "scope": scope}))
}

fn handle_detach(state: &mut State, params: &Value) -> Value {
    let session_id = params["session_id"].as_str().unwrap_or_default().to_owned();
    let subscription = params["subscription_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let known = state.sess.live_subscriptions.remove(&subscription);
    let detached = state.sess.detached_override.unwrap_or(known);
    state.sess.detaches.push(DetachSeen {
        session_id: session_id.clone(),
        subscription_id: subscription.clone(),
        had_end_reason: params.get("end_reason").is_some(),
        detached,
        at: Instant::now(),
    });
    json!({"session_id": session_id, "subscription_id": subscription, "detached": detached})
}

fn handle_submit(state: &mut State, params: &Value, voice: bool) -> Value {
    if voice {
        return json!({"voice_stopped": true});
    }
    let session_id = params["session_id"].as_str().unwrap_or_default().to_owned();
    let text = params["text"].as_str().unwrap_or_default().to_owned();
    state.sess.admitted += 1;
    let number = state.sess.admitted;
    state.sess.submissions.push(Submission {
        session_id: session_id.clone(),
        text: text.clone(),
        submission_id: params["submission_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        at: Instant::now(),
    });
    if let Some(snapshot) = state.sess.sessions.get_mut(&session_id) {
        let message = json!({"role": "user", "text": text});
        for key in ["messages", "display_messages"] {
            if let Some(list) = snapshot[key].as_array_mut() {
                list.push(message.clone());
            }
        }
        let count = snapshot["message_count"].as_u64().unwrap_or(0);
        snapshot["message_count"] = json!(count + 1);
    }
    json!({
        "admission_id": format!("adm-{number}"),
        "ref": {"profile_id": state.sess.profile_id, "session_id": session_id},
        "sequence": number,
        "status": if matches!(state.sess.headless, Some(HeadlessScenario::QueuedStop | HeadlessScenario::ServerRequest)) { "queued" } else { "started" },
        "outcome": null,
        "authority_epoch": 1,
        "execution_generation": if matches!(state.sess.headless, Some(HeadlessScenario::QueuedStop | HeadlessScenario::ServerRequest)) {
            None
        } else {
            Some(state.sess.gens.get(&session_id).copied().unwrap_or(1))
        },
    })
}

fn handle_interrupt(state: &mut State, params: &Value) -> Result<Value, String> {
    let session_id = params["session_id"].as_str().unwrap_or_default().to_owned();
    let current = state.sess.gens.get(&session_id).copied();
    if let Some(current) = current
        && params["execution_generation"].as_i64() != Some(current)
    {
        return Err("stale_generation".to_owned());
    }
    let mut result = canonical_result("session.interrupt", &session_id)
        .unwrap_or_else(|| panic!("the contract has no session.interrupt"));
    result["ref"]["profile_id"] = json!(state.sess.profile_id);
    result["instance_id"] = json!(INSTANCE_ID);
    result["authority_epoch"] = json!(1);
    result["execution_generation"] = json!(current.unwrap_or(1));
    Ok(result)
}

/// Turn recorded event frames into frames for the mock's session. Each is renumbered from the
/// mock's sequence counter. A turn start (`message.start`) raises the session's
/// `execution_generation` by one; that frame and the frames of the same recorded turn carry the
/// new number, in the envelope and in a payload number that matches the recorded one.
fn replay_texts(state: &mut State, frames: &[Value]) -> Vec<(String, Meta)> {
    let session_id = state.session_id.clone();
    let mut recorded_turn: Option<i64> = None;
    let mut out = Vec::new();
    for frame in frames {
        let mut params = frame["params"].clone();
        let type_name = params["type"].as_str().unwrap_or_default().to_owned();
        let recorded = params["execution_generation"].as_i64();
        if type_name == "message.start" {
            recorded_turn = recorded;
            let start = state.envelope_generation;
            *state.sess.gens.entry(session_id.clone()).or_insert(start) += 1;
            if let Some(stored) = state.sess.sessions.get_mut(&session_id) {
                stored["running"] = json!(true);
            }
        } else if recorded_turn.is_none() {
            recorded_turn = recorded;
        }
        if type_name == "message.complete"
            && let Some(stored) = state.sess.sessions.get_mut(&session_id)
        {
            stored["running"] = json!(false);
        }
        let start = state.envelope_generation;
        let current = *state.sess.gens.entry(session_id.clone()).or_insert(start);
        if recorded.is_some() && recorded == recorded_turn {
            params["execution_generation"] = json!(current);
        }
        if let Some(payload) = params["payload"].as_object_mut()
            && payload.get("execution_generation").and_then(Value::as_i64) == recorded_turn
            && recorded_turn.is_some()
        {
            payload.insert("execution_generation".to_owned(), json!(current));
        }
        params["session_id"] = json!(session_id);
        params["seq"] = json!(state.next_seq);
        state.next_seq += 1;
        params["replay_epoch"] = json!(state.sess.replay_epoch);
        let text = json!({"jsonrpc": "2.0", "method": "event", "params": params}).to_string();
        out.push((text, event_meta(&type_name)));
    }
    out
}

impl MockGateway {
    /// Enable finite frames without changing the interactive fixture defaults.
    pub fn configure_headless(&self, scenario: HeadlessScenario) {
        lock(&self.shared).sess.headless = Some(scenario);
        self.set_session_create_descriptor(
            &["cli", "tui", "gui", "acp", "tool", "oneshot"],
            &[
                "source",
                "request_id",
                "cwd",
                "model",
                "provider",
                "reasoning",
            ],
        );
        let mut frames = headless_events();
        match scenario {
            HeadlessScenario::StartedStop => {
                frames.truncate(1);
            }
            HeadlessScenario::QueuedStop | HeadlessScenario::ServerRequest => frames.clear(),
            _ => {}
        }
        self.replay_after("prompt.submit", frames);
        if scenario == HeadlessScenario::ReceiptFailure {
            self.set_behavior(
                "prompt.receipt",
                Behavior::Refuse("receipt_unavailable".to_owned()),
            );
        }
    }
    /// The profile id the mock reports in `runtime.describe` and in the refs it builds. The
    /// default is `fixture`; a standalone gateway sets it from the ticket it accepted.
    pub fn set_profile_id(&self, profile_id: &str) {
        lock(&self.shared).sess.profile_id = profile_id.to_owned();
    }

    /// What `runtime.describe` offers for `session.create`.
    pub fn set_session_create_descriptor(&self, sources: &[&str], parameters: &[&str]) {
        let mut state = lock(&self.shared);
        state.sess.sources = sources.iter().map(|name| (*name).to_owned()).collect();
        state.sess.parameters = parameters.iter().map(|name| (*name).to_owned()).collect();
    }

    /// The snapshot `session.create` answers, before the mock fills in the request's folder,
    /// model and provider. The default is the recorded plain-answer one.
    pub fn set_create_template(&self, snapshot: Value) {
        lock(&self.shared).sess.create_template = snapshot;
    }

    /// The snapshot `session.resume` answers for `latest` and for its own session id. The
    /// default is [`recorded_resume_snapshot`] with `prompts` emptied and `info.cwd` set to
    /// `/work/proj`.
    pub fn set_resume_snapshot(&self, snapshot: Value) {
        let mut state = lock(&self.shared);
        if let Some(id) = snapshot["session_id"].as_str() {
            let id = id.to_owned();
            state.sess.sessions.remove(&id);
        }
        state.sess.resume_snapshot = snapshot;
    }

    /// Set `info.cwd` of the resume snapshot; `None` removes the key.
    pub fn set_resume_cwd(&self, cwd: Option<&str>) {
        let mut state = lock(&self.shared);
        let id = state.sess.resume_snapshot["session_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        state.sess.sessions.remove(&id);
        match cwd {
            Some(cwd) => state.sess.resume_snapshot["info"]["cwd"] = json!(cwd),
            None => {
                if let Some(info) = state.sess.resume_snapshot["info"].as_object_mut() {
                    info.remove("cwd");
                }
            }
        }
    }

    /// Answer the next `session.resume` (of any kind) with this snapshot, once. Queued
    /// snapshots are used in order before the configured one.
    pub fn queue_resume_snapshot(&self, snapshot: Value) {
        lock(&self.shared).sess.resume_queue.push_back(snapshot);
    }

    /// While on, `session.resume` with `latest` answers `not_found`.
    pub fn set_latest_missing(&self, missing: bool) {
        lock(&self.shared).sess.latest_missing = missing;
    }

    /// The rows `session.list` answers for `live` or `stored`. Build them with [`list_row`].
    pub fn set_list_rows(&self, scope: &str, rows: Vec<Value>) {
        lock(&self.shared)
            .sess
            .list_rows
            .insert(scope.to_owned(), rows);
    }

    /// Make `session.list` for this scope answer a refusal with this reason; `None` clears it.
    pub fn set_list_error(&self, scope: &str, reason: Option<&str>) {
        let mut state = lock(&self.shared);
        match reason {
            Some(reason) => state
                .sess
                .list_errors
                .insert(scope.to_owned(), reason.to_owned()),
            None => state.sess.list_errors.remove(scope),
        };
    }

    /// Force the `detached` flag of every `session.detach` answer. `None` (the default) answers
    /// true for a subscription the mock handed out and still holds, false otherwise.
    pub fn set_detached(&self, detached: Option<bool>) {
        lock(&self.shared).sess.detached_override = detached;
    }

    /// Do this for the next request with this method, once. Queued behaviours are used in order
    /// before the method's default.
    pub fn queue_behavior(&self, method: &str, behavior: Behavior) {
        lock(&self.shared)
            .sess
            .behaviors
            .entry(method.to_owned())
            .or_default()
            .push_back(behavior);
    }

    /// Script the two `model.options` catalogues. Each must decode as a model catalogue.
    pub fn set_model_options(&self, with_session: Value, profile_view: Value) {
        let mut state = lock(&self.shared);
        state.sess.options_with_session = with_session;
        state.sess.options_profile = profile_view;
    }

    /// Refuse `model.options`. `None` answers with the scripted catalogue again.
    pub fn fail_model_options(&self, reason: Option<&str>) {
        lock(&self.shared).sess.options_fail = reason.map(str::to_owned);
    }

    /// The value `config.get` returns for `reasoning`.
    pub fn set_reasoning_value(&self, value: Value) {
        lock(&self.shared).sess.reasoning_value = value;
    }

    /// The revision and execution generation `session.mutate` currently holds.
    pub fn set_mutate_fences(&self, revision: i64, generation: i64) {
        let mut state = lock(&self.shared);
        state.sess.mutate_revision = revision;
        state.sess.mutate_generation = generation;
    }

    /// How `session.mutate` answers: normal, busy, guarded, guarded-twice, unresolvable, or drop.
    pub fn set_mutate_mode(&self, mode: &str) {
        lock(&self.shared).sess.mutate_mode = mode.to_owned();
    }

    /// How `config.set` answers: normal, refuse, or drop.
    pub fn set_config_set_mode(&self, mode: &str) {
        lock(&self.shared).sess.config_set_mode = mode.to_owned();
    }

    /// `session.create` requests whose `reasoning` is one of these words are refused.
    pub fn set_create_refusal_efforts(&self, efforts: &[&str]) {
        lock(&self.shared).sess.refused_efforts =
            efforts.iter().map(|effort| (*effort).to_owned()).collect();
    }

    /// Omit this key from `runtime.describe`'s `session_create.parameters`. `None` lists them all.
    pub fn set_describe_without(&self, key: Option<&str>) {
        lock(&self.shared).sess.describe_without = key.map(str::to_owned);
    }

    /// Do this for every request with this method once the queue is empty.
    pub fn set_behavior(&self, method: &str, behavior: Behavior) {
        lock(&self.shared)
            .sess
            .defaults
            .insert(method.to_owned(), behavior);
    }

    /// Answer every request with this method after this long; `None` clears it.
    pub fn set_delay(&self, method: &str, delay: Option<Duration>) {
        match delay {
            Some(delay) => self.set_behavior(method, Behavior::Delay(delay)),
            None => {
                lock(&self.shared).sess.defaults.remove(method);
            }
        }
    }

    /// Read every frame and answer none, including the handshake, while on.
    pub fn set_silent_all(&self, silent: bool) {
        lock(&self.shared).silent_all = silent;
    }

    /// `slash.exec` answers JSON-RPC -32601. The next command tries `slash.exec` again.
    pub fn set_gateway_without_slash_exec(&self, on: bool) {
        lock(&self.shared).gateway_without_slash_exec = on;
    }

    /// `command.dispatch` answers JSON-RPC -32601 as well.
    pub fn set_gateway_without_command_dispatch(&self, on: bool) {
        lock(&self.shared).gateway_without_command_dispatch = on;
    }

    /// While on, `/plan` is answered with the busy exec text.
    pub fn set_turn_running(&self, running: bool) {
        lock(&self.shared).turn_running = running;
    }

    /// After every successful answer to this method, write these recorded event frames (as
    /// [`fixture_events`] returns them) for the current session.
    pub fn replay_after(&self, method: &str, frames: Vec<Value>) {
        lock(&self.shared)
            .sess
            .replay_always
            .insert(method.to_owned(), frames);
    }

    /// Like [`replay_after`](Self::replay_after) but for the next request of the method only.
    pub fn queue_replay_after(&self, method: &str, frames: Vec<Value>) {
        lock(&self.shared)
            .sess
            .replay_once
            .entry(method.to_owned())
            .or_default()
            .push_back(frames);
    }

    /// Start counting generations for a session that no create or resume attached, so a
    /// `session.interrupt` for it is checked.
    pub fn track_generation(&self, session_id: &str, generation: i64) {
        lock(&self.shared)
            .sess
            .gens
            .insert(session_id.to_owned(), generation);
    }

    /// The session's current `execution_generation`, once it is tracked.
    pub fn generation(&self, session_id: &str) -> Option<i64> {
        lock(&self.shared).sess.gens.get(session_id).copied()
    }

    /// Every request the mock found wrong.
    pub fn violations(&self) -> Vec<Violation> {
        lock(&self.shared).sess.violations.clone()
    }

    /// Panics when the mock found any request wrong, naming the method and keys.
    pub fn assert_no_violations(&self) {
        let found = self.violations();
        assert!(found.is_empty(), "the mock found wrong requests: {found:?}");
    }

    /// The ids of the sessions `session.create` made, in order.
    pub fn created_sessions(&self) -> Vec<String> {
        lock(&self.shared).sess.created.clone()
    }

    /// Every create and resume answered, in order.
    pub fn attaches(&self) -> Vec<AttachSeen> {
        lock(&self.shared).sess.attaches.clone()
    }

    /// Every `session.detach` answered, in order.
    pub fn detaches(&self) -> Vec<DetachSeen> {
        lock(&self.shared).sess.detaches.clone()
    }

    /// Every prompt admitted, in order.
    pub fn submissions(&self) -> Vec<Submission> {
        lock(&self.shared).sess.submissions.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// The standalone gateway: a control socket and a loopback WebSocket for a spawned binary
// ---------------------------------------------------------------------------------------------

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode};

/// One request the control socket read.
#[derive(Clone, Debug)]
pub struct ControlSeen {
    /// `identify` or `session-ticket`.
    pub verb: String,
    /// Monotonic stamp taken when the connection was accepted.
    pub at: Instant,
}

struct IssuedTicket {
    value: String,
    profile_id: String,
    used: bool,
}

struct StandaloneInfo {
    home: String,
    fixture: String,
    origin: String,
    control: Mutex<Vec<ControlSeen>>,
    tickets: Mutex<Vec<IssuedTicket>>,
    websockets: Mutex<Vec<Instant>>,
    present: std::sync::atomic::AtomicBool,
}

fn plain<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn ticket_token(number: u64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let mut out = String::with_capacity(43);
    let mut state = number.wrapping_add(0x9E37);
    for _ in 0..43 {
        out.push(ALPHABET[(state as usize) % ALPHABET.len()] as char);
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
    }
    out
}

fn set_private(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|_| panic!("the mock could not set a file mode"));
}

/// A gateway for a spawned binary: it answers `identify` and `session-ticket` on a Unix control
/// socket named `gateway.sock` in the home, and serves the session WebSocket on a loopback port,
/// with every handler of [`MockGateway`]. All of the in-process mock's switches and accessors
/// are reached through `Deref`. The tests using it are plain threads, not async tasks, so the
/// gateway owns a runtime; the waiting helpers here block.
pub struct StandaloneGateway {
    mock: MockGateway,
    info: Arc<StandaloneInfo>,
    runtime: Option<tokio::runtime::Runtime>,
    home: PathBuf,
}

impl Deref for StandaloneGateway {
    type Target = MockGateway;

    fn deref(&self) -> &MockGateway {
        &self.mock
    }
}

impl StandaloneGateway {
    /// Serve a gateway that is ready now.
    pub fn start(home: &Path) -> Self {
        Self::build(home, None)
    }

    /// Serve a gateway that starts absent: no `gateway.sock` exists until the file `counter` has
    /// a line in it (the stand-in `hermes` of [`write_ensure_stand_in`] writes one), then the
    /// socket appears a moment later and the gateway stays ready.
    pub fn start_absent(home: &Path, counter: &Path) -> Self {
        Self::build(home, Some(counter.to_owned()))
    }

    fn build(home: &Path, appear_after: Option<PathBuf>) -> Self {
        let home = std::fs::canonicalize(home)
            .unwrap_or_else(|_| panic!("the mock's home folder is missing"));
        let fixture = home.join("profiles").join("fixture");
        std::fs::create_dir_all(&fixture)
            .unwrap_or_else(|_| panic!("the mock could not make the fixture profile"));
        set_private(&home.join("profiles"), 0o700);
        set_private(&fixture, 0o700);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap_or_else(|_| panic!("the mock could not start a runtime"));
        let (commands, _unused) = mpsc::unbounded_channel();
        let shared = new_shared(SESSION_ID, commands, 0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap_or_else(|_| panic!("the mock could not bind a port"));
        let port = listener
            .local_addr()
            .unwrap_or_else(|_| panic!("the mock has no port"))
            .port();
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|_| panic!("the mock could not set its port non-blocking"));
        let info = Arc::new(StandaloneInfo {
            home: home.to_string_lossy().into_owned(),
            fixture: fixture.to_string_lossy().into_owned(),
            origin: format!("http://127.0.0.1:{port}"),
            control: Mutex::new(Vec::new()),
            tickets: Mutex::new(Vec::new()),
            websockets: Mutex::new(Vec::new()),
            present: std::sync::atomic::AtomicBool::new(false),
        });
        lock(&shared).sess.profile_id = info.home.clone();
        let socket = home.join("gateway.sock");
        {
            let _guard = runtime.enter();
            let listener = TcpListener::from_std(listener)
                .unwrap_or_else(|_| panic!("the mock could not use its port"));
            runtime.spawn(accept_sockets(
                listener,
                Arc::clone(&shared),
                Arc::clone(&info),
            ));
            match appear_after {
                None => {
                    let control = bind_control(&socket);
                    info.present
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    runtime.spawn(serve_control(control, Arc::clone(&info)));
                }
                Some(counter) => {
                    let info = Arc::clone(&info);
                    runtime.spawn(async move {
                        loop {
                            let written = std::fs::read_to_string(&counter)
                                .map(|text| text.lines().any(|line| !line.is_empty()))
                                .unwrap_or(false);
                            if written {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(5)).await;
                        }
                        tokio::time::sleep(Duration::from_millis(60)).await;
                        let control = bind_control(&socket);
                        info.present
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        serve_control(control, info).await;
                    });
                }
            }
        }
        Self {
            mock: MockGateway { shared },
            info,
            runtime: Some(runtime),
            home,
        }
    }

    /// The canonical home folder this gateway serves.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// True once `gateway.sock` is bound.
    pub fn is_present(&self) -> bool {
        self.info.present.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The loopback origin the identify answer names.
    pub fn api_origin(&self) -> &str {
        &self.info.origin
    }

    /// Every control request read, in order.
    pub fn control_requests(&self) -> Vec<ControlSeen> {
        plain(&self.info.control).clone()
    }

    /// When the control socket first accepted a connection.
    pub fn first_control_connection_at(&self) -> Option<Instant> {
        plain(&self.info.control).first().map(|seen| seen.at)
    }

    /// When the WebSocket port first accepted a connection.
    pub fn first_websocket_connection_at(&self) -> Option<Instant> {
        plain(&self.info.websockets).first().copied()
    }

    /// How many connections the WebSocket port has accepted.
    pub fn websocket_connections(&self) -> usize {
        plain(&self.info.websockets).len()
    }

    /// How many tickets the control socket has issued.
    pub fn tickets_issued(&self) -> usize {
        plain(&self.info.tickets).len()
    }

    /// Run a future of the in-process mock's async API (`server_request`, `send_event`,
    /// `kill_connection`, ...) on the gateway's runtime and wait for it. Do not call it from
    /// inside an async task.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        match &self.runtime {
            Some(runtime) => runtime.block_on(future),
            None => panic!("the gateway is shut down"),
        }
    }

    /// Poll until the check holds or the limit passes. Returns whether it held.
    pub fn wait_until(&self, limit: Duration, check: impl Fn(&MockGateway) -> bool) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if check(&self.mock) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Poll until the nth (from 0) request with this method has been read and return it.
    pub fn wait_request(&self, method: &str, nth: usize, limit: Duration) -> Option<Seen> {
        self.wait_until(limit, |mock| mock.requests(method).len() > nth);
        self.mock.requests(method).into_iter().nth(nth)
    }
}

impl Drop for StandaloneGateway {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
        let _ = std::fs::remove_file(self.home.join("gateway.sock"));
    }
}

fn bind_control(socket: &Path) -> UnixListener {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)
        .unwrap_or_else(|_| panic!("the mock could not bind its control socket"));
    set_private(socket, 0o600);
    listener
}

async fn serve_control(listener: UnixListener, info: Arc<StandaloneInfo>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let at = Instant::now();
        let info = Arc::clone(&info);
        tokio::spawn(async move {
            let _ = answer_control(stream, info, at).await;
        });
    }
}

fn identify_result(info: &StandaloneInfo, epoch: u64) -> Value {
    json!({
        "protocol": 1,
        "kind": "gateway",
        "pid": 4242,
        "start_time": 1_790_000_000.5,
        "hermes_home": info.home,
        "profile": "default",
        "supervisor": "none",
        "launch_supervisor": "manual",
        "code_sha": CODE_SHA,
        "code_version": "0.22.4",
        "instance_id": INSTANCE_ID,
        "runtime_protocol": 1,
        "authority_epoch": epoch,
        "state": "ready",
        "api_origin": info.origin,
        "served_profiles": [
            {"profile_id": info.home, "home": info.home},
            {"profile_id": info.fixture, "home": info.fixture},
        ],
        "parked_profiles": {},
        "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"],
    })
}

async fn answer_control(
    stream: UnixStream,
    info: Arc<StandaloneInfo>,
    at: Instant,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(tokio::io::AsyncReadExt::take(stream, 64 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let request: Value = serde_json::from_str(line.trim()).unwrap_or(Value::Null);
    let verb = request["verb"].as_str().unwrap_or_default().to_owned();
    plain(&info.control).push(ControlSeen {
        verb: verb.clone(),
        at,
    });
    let id = request["id"].clone();
    let reply = match verb.as_str() {
        "identify" => json!({
            "ok": true,
            "protocol": 1,
            "id": id,
            "result": identify_result(&info, 3),
        }),
        "session-ticket" => issue_ticket(&info, &request),
        _ => json!({"ok": false, "protocol": 1, "id": id, "error": "unknown verb"}),
    };
    let mut stream = reader.into_inner().into_inner();
    stream.write_all(reply.to_string().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    Ok(())
}

fn issue_ticket(info: &StandaloneInfo, request: &Value) -> Value {
    let id = request["id"].clone();
    let params = &request["params"];
    let refuse = |text: &str| json!({"ok": false, "protocol": 1, "id": id, "error": text});
    if params["instance_id"] != INSTANCE_ID {
        return refuse("session-ticket: stale runtime instance");
    }
    let profile = params["profile_id"].as_str().unwrap_or_default();
    if profile != info.home && profile != info.fixture {
        return refuse("session-ticket: bootstrap binding rejected");
    }
    if !matches!(
        params["purpose"].as_str(),
        Some("interactive" | "native-http")
    ) || !matches!(params["scope"].as_str(), Some("profile" | "host"))
    {
        return refuse("session-ticket: invalid bootstrap parameters");
    }
    let mut tickets = plain(&info.tickets);
    let value = ticket_token(tickets.len() as u64 + 1);
    tickets.push(IssuedTicket {
        value: value.clone(),
        profile_id: profile.to_owned(),
        used: false,
    });
    json!({
        "ok": true,
        "protocol": 1,
        "id": id,
        "result": {
            "ticket": value,
            "expires_in_seconds": 30,
            "instance_id": INSTANCE_ID,
            "profile_id": profile,
            "runtime_protocol": 1,
        },
    })
}

async fn accept_sockets(listener: TcpListener, shared: Arc<Shared>, info: Arc<StandaloneInfo>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        plain(&info.websockets).push(Instant::now());
        let shared = Arc::clone(&shared);
        let info = Arc::clone(&info);
        tokio::spawn(serve_socket(stream, shared, info));
    }
}

async fn serve_socket(stream: TcpStream, shared: Arc<Shared>, info: Arc<StandaloneInfo>) {
    let _ = stream.set_nodelay(true);
    let profile = Arc::new(Mutex::new(String::new()));
    let callback = {
        let profile = Arc::clone(&profile);
        let info = Arc::clone(&info);
        let shared = Arc::clone(&shared);
        // Tungstenite fixes this callback's error type to an unboxed HTTP response.
        #[allow(clippy::result_large_err)]
        move |request: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
            let header = request
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let ticket = header
                .split(',')
                .filter_map(|part| part.trim().strip_prefix("hermes-gateway-ticket."))
                .next()
                .unwrap_or("");
            let accepted = {
                let mut tickets = plain(&info.tickets);
                match tickets
                    .iter_mut()
                    .find(|issued| issued.value == ticket && !issued.used)
                {
                    Some(issued) => {
                        issued.used = true;
                        *plain(&profile) = issued.profile_id.clone();
                        true
                    }
                    None => false,
                }
            };
            if !accepted {
                lock(&shared).sess.violations.push(Violation {
                    method: "websocket upgrade".to_owned(),
                    reason: "a ticket the mock did not issue, or already used".to_owned(),
                    fields: Vec::new(),
                });
                let mut refusal = ErrorResponse::new(Some(String::new()));
                *refusal.status_mut() = StatusCode::FORBIDDEN;
                return Err(refusal);
            }
            response.headers_mut().insert(
                SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static("hermes-gateway-v1"),
            );
            Ok(response)
        }
    };
    let Ok(socket) = tokio_tungstenite::accept_hdr_async(stream, callback).await else {
        return;
    };
    let (commands, receiver) = mpsc::unbounded_channel();
    let connection = {
        let mut state = lock(&shared);
        state.connection += 1;
        state.commands = commands.clone();
        state.sess.profile_id = plain(&profile).clone();
        state.connection
    };
    let ready = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "gateway.ready",
            "payload": {
                "skin": {"name": "default"},
                "change_events": true,
                "replay_epoch": lock(&shared).sess.replay_epoch,
                "heartbeat": true,
                "session_authority": true,
            },
        },
    });
    let _ = commands.send(Cmd::Send {
        text: ready.to_string(),
        meta: event_meta("gateway.ready"),
        done: None,
    });
    let weak = commands.downgrade();
    drop(commands);
    shared.changed.notify_waiters();
    run_socket(socket, receiver, weak, Arc::downgrade(&shared), connection).await;
}

// ---------------------------------------------------------------------------------------------
// Stand-in programs for process tests
// ---------------------------------------------------------------------------------------------

fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap_or_else(|_| panic!("the mock could not write a script"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|_| panic!("the mock could not set a script mode"));
    path
}

fn quoted(path: &Path) -> String {
    let text = path.to_string_lossy();
    assert!(!text.contains('\''), "a stand-in path holds a quote");
    format!("'{text}'")
}

/// Write a stand-in `hermes` into `bin_dir`. Run as `hermes gateway ensure --json` it appends
/// one line to `counter`, waits until `gateway.sock` exists in `home` (up to five seconds), then
/// prints a successful ensure result. Put `bin_dir` first on the binary's `PATH`, and start the
/// gateway with [`StandaloneGateway::start_absent`] on the same counter.
pub fn write_ensure_stand_in(bin_dir: &Path, counter: &Path, home: &Path) -> PathBuf {
    let socket = home.join("gateway.sock");
    let body = format!(
        "#!/bin/sh\n\
         echo ran >> {counter}\n\
         waited=0\n\
         while [ ! -S {socket} ] && [ \"$waited\" -lt 100 ]; do\n\
         sleep 0.05\n\
         waited=$((waited + 1))\n\
         done\n\
         if [ ! -S {socket} ]; then\n\
         exit 1\n\
         fi\n\
         printf '%s\\n' '{{\"state\":\"ready\",\"reason_code\":\"started\"}}'\n",
        counter = quoted(counter),
        socket = quoted(&socket),
    );
    write_script(bin_dir, "hermes", &body)
}

/// How many times the stand-in `hermes` ran: the lines in its counter file.
pub fn ensure_runs(counter: &Path) -> usize {
    std::fs::read_to_string(counter)
        .map(|text| text.lines().filter(|line| !line.is_empty()).count())
        .unwrap_or(0)
}

/// Write a stand-in program into `bin_dir` that appends one line to `log` for every call and
/// then runs `then` (shell text; `exit 1` makes a program that fails, an empty text succeeds
/// with no output).
pub fn write_logging_stand_in(bin_dir: &Path, name: &str, log: &Path, then: &str) -> PathBuf {
    let body = format!(
        "#!/bin/sh\necho \"$0 $*\" >> {log}\n{then}\n",
        log = quoted(log)
    );
    write_script(bin_dir, name, &body)
}

// ---------------------------------------------------------------------------------------------
// MockServer: the name earlier tests use for the standalone gateway
// ---------------------------------------------------------------------------------------------

/// The standalone gateway under its earlier name.
pub struct MockServer {
    gateway: StandaloneGateway,
}

impl MockServer {
    pub fn start(home: &Path) -> std::io::Result<Self> {
        Ok(Self {
            gateway: StandaloneGateway::start(home),
        })
    }

    /// The text of every frame the client has written to the session socket, one per line.
    /// Empty until a client connects and speaks.
    pub fn received(&self) -> Vec<u8> {
        let mut text = String::new();
        for frame in self.gateway.seen() {
            text.push_str(
                &json!({"method": frame.method, "id": frame.id, "params": frame.params})
                    .to_string(),
            );
            text.push('\n');
        }
        text.into_bytes()
    }
}

impl Deref for MockServer {
    type Target = StandaloneGateway;

    fn deref(&self) -> &StandaloneGateway {
        &self.gateway
    }
}

// Model catalogue, effort and fenced mutate handlers. Issue 230.
include!("mock_model.rs");
include!("mock_rewind.rs");
