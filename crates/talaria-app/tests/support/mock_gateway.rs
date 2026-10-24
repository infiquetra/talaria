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
}

fn fixture_text(fixture: Fixture) -> &'static str {
    match fixture {
        Fixture::Approval => APPROVAL_LOG,
        Fixture::Clarification => CLARIFICATION_LOG,
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

impl MockGateway {
    /// Start the mock for the recorded session and a client attached to it.
    pub async fn start() -> (Self, Client, InboundStream) {
        Self::start_with_session(SESSION_ID).await
    }

    /// Start the mock for this session id and a client attached to it.
    pub async fn start_with_session(session_id: &str) -> (Self, Client, InboundStream) {
        let (parts, server) = socket_pair().await;
        let (commands, receiver) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                session_id: session_id.to_owned(),
                connection: 1,
                commands: commands.clone(),
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
            }),
            changed: Notify::new(),
        });
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
                "id-0002",
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

    /// Write an event whose whole `params` object the test supplies.
    pub async fn send_event_params(&self, params: Value) {
        if let Some(seq) = params["seq"].as_u64() {
            let mut s = lock(&self.shared);
            if s.next_seq <= seq {
                s.next_seq = seq + 1;
            }
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
            "replay_epoch": "id-0002",
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
    if state.silent_methods.contains(method) {
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
