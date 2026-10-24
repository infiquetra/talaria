//! Mock gateway for the reconnect tests.
//!
//! It speaks the attach handshake and the replay methods.
//! It does not implement `session.create` or `prompt.submit`.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hermes_client::liveness::{Dial, DialFailure, Dialed};
use hermes_client::{
    ConnectOptions, Endpoint, HandshakeOptions, ProfileLookupError, Ticket, connect_with,
    handshake_with,
};
use serde_json::{Map, Value, json};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

const INSTANCE_ID: &str = "mock-instance-p1-03";
const PROFILE_ID: &str = "mock-profile-p1-03";
const AUTHORITY: u64 = 7;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Tests in `reconnect.rs` hold this for the whole test so socket tests do not overlap.
pub async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    SERIAL.lock().await
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub method: String,
    pub params: Value,
    pub connection: u64,
}

struct Event {
    seq: u64,
    epoch: String,
    session_id: String,
    kind: String,
    payload: Value,
}

struct Session {
    epoch: String,
    seq: u64,
    ring: VecDeque<Event>,
    watermark: u64,
}

struct State {
    process_epoch: String,
    heartbeat: Option<bool>,
    capabilities: Vec<String>,
    ring_max: usize,
    tickets: HashMap<String, bool>,
    ticket_seq: u64,
    upgrades: u64,
    upgrade_tickets: Vec<String>,
    sessions: HashMap<String, Session>,
    session_seq: u64,
    redirects: HashMap<String, String>,
    subscribers: HashMap<u64, HashMap<String, String>>,
    requests: Vec<RecordedRequest>,
    ignore_pings: bool,
    hang_upgrades: bool,
    close_after: Option<Duration>,
    delays: Vec<(String, String, Duration)>,
    next_conn: u64,
    sub_seq: u64,
    outs: HashMap<u64, mpsc::UnboundedSender<String>>,
}

struct Inner {
    state: Mutex<State>,
    commands: broadcast::Sender<Cmd>,
}

#[derive(Clone, Copy)]
enum Cmd {
    Kill,
    Close(u16),
    Silence,
}

/// A loopback gateway. Cheap to clone. The accept task lives until the last clone drops.
#[derive(Clone)]
pub struct MockGateway {
    inner: Arc<Inner>,
    port: u16,
}

impl MockGateway {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock gateway");
        let port = listener.local_addr().expect("mock port").port();
        let (commands, _) = broadcast::channel(32);
        let inner = Arc::new(Inner {
            commands,
            state: Mutex::new(State {
                process_epoch: "proc-1".to_owned(),
                heartbeat: Some(true),
                capabilities: vec![
                    "event-replay-v1".to_owned(),
                    "session-authority-v1".to_owned(),
                ],
                ring_max: 512,
                tickets: HashMap::new(),
                ticket_seq: 1,
                upgrades: 0,
                upgrade_tickets: Vec::new(),
                sessions: HashMap::new(),
                session_seq: 1,
                redirects: HashMap::new(),
                subscribers: HashMap::new(),
                requests: Vec::new(),
                ignore_pings: false,
                hang_upgrades: false,
                close_after: None,
                delays: Vec::new(),
                next_conn: 1,
                sub_seq: 0,
                outs: HashMap::new(),
            }),
        });
        let accept = Arc::clone(&inner);
        tokio::spawn(async move { accept_loop(listener, accept).await });
        Self { inner, port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn issue_ticket(&self) -> String {
        let mut state = self.lock();
        state.ticket_seq += 1;
        let value = ticket_text(state.ticket_seq);
        state.tickets.insert(value.clone(), false);
        value
    }

    pub fn add_session(&self) -> String {
        let mut state = self.lock();
        let n = state.session_seq;
        state.session_seq += 1;
        let id = format!("s{n}");
        state.sessions.insert(
            id.clone(),
            Session {
                epoch: format!("e{n}"),
                seq: 0,
                ring: VecDeque::new(),
                watermark: 0,
            },
        );
        id
    }

    pub fn publish(&self, session: &str, kind: &str, payload: Value) {
        let frames = {
            let mut state = self.lock();
            let ring_max = state.ring_max;
            let Some(found) = state.sessions.get_mut(session) else {
                return;
            };
            found.seq += 1;
            let event = Event {
                seq: found.seq,
                epoch: found.epoch.clone(),
                session_id: session.to_owned(),
                kind: kind.to_owned(),
                payload,
            };
            found.ring.push_back(clone_event(&event));
            while found.ring.len() > ring_max {
                if let Some(dropped) = found.ring.pop_front() {
                    found.watermark = dropped.seq.saturating_add(1);
                }
            }
            let text = event_frame(&event, true);
            fanout(&state, session, text)
        };
        let _ = frames;
    }

    pub fn overflow(&self, session: &str) {
        let mut state = self.lock();
        let Some(found) = state.sessions.get(session) else {
            return;
        };
        let text = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "session.replay_gap",
                "session_id": session,
                "replay_epoch": found.epoch,
                "payload": {"replay_epoch": found.epoch, "latest_seq": found.seq}
            }
        })
        .to_string();
        let _ = fanout(&state, session, text);
        for subs in state.subscribers.values_mut() {
            subs.remove(session);
        }
    }

    pub fn restart(&self, epoch: &str) {
        let mut state = self.lock();
        state.process_epoch = epoch.to_owned();
        for (id, session) in &mut state.sessions {
            session.epoch = format!("{epoch}:{id}");
            session.seq = 0;
            session.ring.clear();
            session.watermark = 0;
        }
        state.subscribers.clear();
    }

    pub fn redirect(&self, from: &str, to: &str) {
        self.lock().redirects.insert(from.to_owned(), to.to_owned());
    }

    pub fn kill(&self) {
        let _ = self.inner.commands.send(Cmd::Kill);
    }

    pub fn close(&self, code: u16) {
        let _ = self.inner.commands.send(Cmd::Close(code));
    }

    pub fn silence(&self) {
        let _ = self.inner.commands.send(Cmd::Silence);
    }

    pub fn ignore_pings(&self) {
        self.lock().ignore_pings = true;
    }

    pub fn hang_upgrades(&self) {
        self.lock().hang_upgrades = true;
    }

    pub fn close_after(&self, delay: Duration) {
        self.lock().close_after = Some(delay);
    }

    pub fn clear_close_after(&self) {
        self.lock().close_after = None;
    }

    pub fn delay_replies(&self, method: &str, session: &str, delay: Duration) {
        self.lock()
            .delays
            .push((method.to_owned(), session.to_owned(), delay));
    }

    /// Forget every `delay_replies` rule. Replies already scheduled still arrive late.
    pub fn clear_delays(&self) {
        self.lock().delays.clear();
    }

    /// Answer upgrades again after `hang_upgrades()`. Streams already held stay unanswered.
    pub fn allow_upgrades(&self) {
        self.lock().hang_upgrades = false;
    }

    /// Forget a session: later resume and since calls for it are refused as `not_found`.
    pub fn remove_session(&self, session: &str) {
        let mut state = self.lock();
        state.sessions.remove(session);
        for subs in state.subscribers.values_mut() {
            subs.remove(session);
        }
    }

    pub fn set_ring_max(&self, max: usize) {
        self.lock().ring_max = max;
    }

    pub fn set_capabilities(&self, capabilities: Vec<String>) {
        self.lock().capabilities = capabilities;
    }

    pub fn set_heartbeat(&self, heartbeat: Option<bool>) {
        self.lock().heartbeat = heartbeat;
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.lock().requests.clone()
    }

    pub fn upgrades(&self) -> u64 {
        self.lock().upgrades
    }

    pub fn distinct_upgrade_tickets(&self) -> usize {
        self.lock()
            .upgrade_tickets
            .iter()
            .collect::<HashSet<_>>()
            .len()
    }

    pub fn text_contains_ticket(&self, text: &str) -> bool {
        let state = self.lock();
        state.tickets.keys().any(|ticket| text.contains(ticket))
            || state
                .upgrade_tickets
                .iter()
                .any(|ticket| text.contains(ticket))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().expect("mock lock")
    }
}

fn fanout(state: &State, session: &str, text: String) -> usize {
    let mut sent = 0;
    for (conn, subs) in &state.subscribers {
        if subs.contains_key(session)
            && let Some(tx) = state.outs.get(conn)
            && tx.send(text.clone()).is_ok()
        {
            sent += 1;
        }
    }
    sent
}

fn clone_event(event: &Event) -> Event {
    Event {
        seq: event.seq,
        epoch: event.epoch.clone(),
        session_id: event.session_id.clone(),
        kind: event.kind.clone(),
        payload: event.payload.clone(),
    }
}

fn event_frame(event: &Event, with_seq: bool) -> String {
    let mut params = Map::new();
    params.insert("type".to_owned(), json!(event.kind));
    params.insert("session_id".to_owned(), json!(event.session_id));
    params.insert("replay_epoch".to_owned(), json!(event.epoch));
    if with_seq {
        params.insert("seq".to_owned(), json!(event.seq));
    }
    params.insert("payload".to_owned(), event.payload.clone());
    json!({"jsonrpc": "2.0", "method": "event", "params": params}).to_string()
}

fn ticket_text(n: u64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let mut state = n.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    let mut out = String::with_capacity(43);
    for _ in 0..43 {
        out.push(ALPHABET[(state as usize) % ALPHABET.len()] as char);
        state = state.wrapping_mul(0x5851_F42D_4C95_7F2D).wrapping_add(1);
    }
    out
}

async fn accept_loop(listener: TcpListener, inner: Arc<Inner>) {
    let mut hung: Vec<tokio::net::TcpStream> = Vec::new();
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let hang = inner.state.lock().expect("mock lock").hang_upgrades;
        if hang {
            hung.push(stream);
            continue;
        }
        let inner = Arc::clone(&inner);
        tokio::spawn(async move {
            if let Ok((socket, conn)) = upgrade(stream, &inner).await {
                session(socket, conn, inner).await;
            }
        });
    }
}

#[allow(clippy::result_large_err)]
async fn upgrade(
    stream: tokio::net::TcpStream,
    inner: &Inner,
) -> Result<
    (
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        u64,
    ),
    (),
> {
    let shared = Arc::new(Mutex::new(None::<u64>));
    let slot = Arc::clone(&shared);
    let gate = &inner.state;
    let callback =
        move |request: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
            if request.uri().path() != "/api/ws" {
                return Err(reject(404));
            }
            let header = request
                .headers()
                .get("Sec-WebSocket-Protocol")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let ticket = header
                .split(',')
                .filter_map(|part| part.trim().strip_prefix("hermes-gateway-ticket."))
                .next()
                .unwrap_or("");
            let mut state = gate.lock().expect("mock lock");
            match state.tickets.get_mut(ticket) {
                Some(used) if !*used && !ticket.is_empty() => {
                    *used = true;
                    state.upgrades += 1;
                    state.upgrade_tickets.push(ticket.to_owned());
                    let id = state.next_conn;
                    state.next_conn += 1;
                    *slot.lock().expect("conn slot") = Some(id);
                }
                _ => return Err(reject(403)),
            }
            let value = http::HeaderValue::from_static("hermes-gateway-v1");
            response
                .headers_mut()
                .insert(http::header::SEC_WEBSOCKET_PROTOCOL, value);
            Ok(response)
        };
    let socket = tokio_tungstenite::accept_hdr_async(stream, callback)
        .await
        .map_err(|_| ())?;
    let conn = shared.lock().expect("conn slot").take().ok_or(())?;
    Ok((socket, conn))
}

fn reject(status: u16) -> ErrorResponse {
    Response::builder()
        .status(status)
        .body(None)
        .expect("reject response")
}

async fn session(
    socket: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    conn: u64,
    inner: Arc<Inner>,
) {
    let mut commands = inner.commands.subscribe();
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    {
        let mut state = inner.state.lock().expect("mock lock");
        state.outs.insert(conn, out_tx);
        state.subscribers.entry(conn).or_default();
    }
    let ready = ready_frame(&inner);
    if sink
        .send(Message::Text(Utf8Bytes::from(ready)))
        .await
        .is_err()
    {
        return;
    }
    let (timer_tx, mut timer_rx) = tokio::sync::oneshot::channel();
    let mut timer_tx = Some(timer_tx);
    let mut keep_timer: Option<tokio::sync::oneshot::Sender<()>> = None;
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Ok(Cmd::Kill) => break,
                    Ok(Cmd::Silence) => {
                        std::future::pending::<()>().await;
                    }
                    Ok(Cmd::Close(code)) => {
                        let frame = tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: CloseCode::from(code),
                            reason: Utf8Bytes::from(String::new()),
                        };
                        let _ = sink.send(Message::Close(Some(frame))).await;
                        break;
                    }
                    Err(_) => {}
                }
            }
            outbound = out_rx.recv() => {
                let Some(text) = outbound else { break };
                if sink.send(Message::Text(Utf8Bytes::from(text))).await.is_err() {
                    break;
                }
            }
            incoming = stream.next() => {
                let Some(Ok(Message::Text(text))) = incoming else { break };
                let reply = handle_text(&inner, conn, text.as_str());
                if reply.handshake_done
                    && let Some(sender) = timer_tx.take()
                {
                    let close_after = inner.state.lock().expect("mock lock").close_after;
                    if let Some(delay) = close_after {
                        tokio::spawn(async move {
                            tokio::time::sleep(delay).await;
                            let _ = sender.send(());
                        });
                    } else {
                        // Dropping the sender would complete the receiver and close the socket.
                        keep_timer = Some(sender);
                    }
                }
                if let Some(body) = reply.body {
                    if let Some(wait) = reply.delay {
                        // A delayed reply must not stall the rest of the connection: pings,
                        // other sessions' replies and live frames keep flowing meanwhile.
                        let tx = inner.state.lock().expect("mock lock").outs.get(&conn).cloned();
                        if let Some(tx) = tx {
                            tokio::spawn(async move {
                                tokio::time::sleep(wait).await;
                                let _ = tx.send(body);
                            });
                        }
                    } else if sink.send(Message::Text(Utf8Bytes::from(body))).await.is_err() {
                        break;
                    }
                }
            }
            _ = &mut timer_rx => break,
        }
    }
    drop(keep_timer);
    let mut state = inner.state.lock().expect("mock lock");
    state.outs.remove(&conn);
    state.subscribers.remove(&conn);
}

struct Reply {
    body: Option<String>,
    delay: Option<Duration>,
    handshake_done: bool,
}

fn ready_frame(inner: &Inner) -> String {
    let state = inner.state.lock().expect("mock lock");
    let mut payload = json!({
        "skin": {"name": "mock"},
        "change_events": true,
        "replay_epoch": state.process_epoch,
        "session_authority": true,
    });
    if let Some(heartbeat) = state.heartbeat {
        payload["heartbeat"] = json!(heartbeat);
    }
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": "gateway.ready", "payload": payload}
    })
    .to_string()
}

fn handle_text(inner: &Inner, conn: u64, text: &str) -> Reply {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            return Reply {
                body: None,
                delay: None,
                handshake_done: false,
            };
        }
    };
    let method = value["method"].as_str().unwrap_or("").to_owned();
    let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    {
        let mut state = inner.state.lock().expect("mock lock");
        state.requests.push(RecordedRequest {
            method: method.clone(),
            params: params.clone(),
            connection: conn,
        });
    }
    if method == "gateway.ping" {
        let ignore = inner.state.lock().expect("mock lock").ignore_pings;
        let body = if ignore {
            None
        } else {
            Some(json!({"jsonrpc": "2.0", "id": id, "result": {"ok": true}}).to_string())
        };
        return Reply {
            body,
            delay: None,
            handshake_done: false,
        };
    }
    if method == "client.capabilities" {
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"server_requests": [], "declines_not_shown": true}
        })
        .to_string();
        return Reply {
            body: Some(body),
            delay: None,
            handshake_done: false,
        };
    }
    if method == "runtime.describe" {
        let state = inner.state.lock().expect("mock lock");
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "instance_id": INSTANCE_ID,
                "profile_id": PROFILE_ID,
                "authority_epoch": AUTHORITY,
                "capabilities": state.capabilities,
                "session_create": {"sources": ["local"], "parameters": ["text"]}
            }
        })
        .to_string();
        return Reply {
            body: Some(body),
            delay: None,
            handshake_done: true,
        };
    }
    let delay = reply_delay(inner, &method, &params);
    let body = match method.as_str() {
        "session.resume" => resume(inner, conn, &id, &params),
        "session.events.since" => since(inner, &id, &params),
        "session.list" => {
            if let Some(error) = extra_keys(
                &params,
                &["scope", "limit", "include_hidden", "profile", "title"],
            ) {
                error_body(&id, 4001, "invalid_params", "invalid_params", Some(error))
            } else {
                json!({"jsonrpc": "2.0", "id": id, "result": {"sessions": [], "scope": "live"}})
                    .to_string()
            }
        }
        "session.detach" => {
            if let Some(error) = extra_keys(&params, &["session_id", "subscription_id"]) {
                error_body(&id, 4001, "invalid_params", "invalid_params", Some(error))
            } else {
                json!({"jsonrpc": "2.0", "id": id, "result": {}}).to_string()
            }
        }
        _ => error_body(&id, -32601, "method not found", "unknown_method", None),
    };
    Reply {
        body: Some(body),
        delay,
        handshake_done: false,
    }
}

fn reply_delay(inner: &Inner, method: &str, params: &Value) -> Option<Duration> {
    let session = params["session_id"].as_str().unwrap_or("");
    inner
        .state
        .lock()
        .expect("mock lock")
        .delays
        .iter()
        .find(|(name, id, _)| name == method && id == session)
        .map(|(_, _, delay)| *delay)
}

const RESUME_KEYS: &[&str] = &[
    "api_key",
    "cols",
    "defer_history",
    "display_projection",
    "editor",
    "latest",
    "omit_messages",
    "profile",
    "session_id",
    "source",
    "title",
    "workspace",
];

const SINCE_KEYS: &[&str] = &[
    "last_seen",
    "last_sequence",
    "profile",
    "replay_epoch",
    "session_id",
];

fn resume(inner: &Inner, conn: u64, id: &Value, params: &Value) -> String {
    if let Some(fields) = extra_keys(params, RESUME_KEYS) {
        return error_body(id, 4001, "invalid_params", "invalid_params", Some(fields));
    }
    let Some(requested) = params["session_id"].as_str() else {
        return error_body(
            id,
            4001,
            "invalid_params",
            "invalid_params",
            Some(vec!["session_id".to_owned()]),
        );
    };
    let mut state = inner.state.lock().expect("mock lock");
    let resolved = state
        .redirects
        .get(requested)
        .cloned()
        .unwrap_or_else(|| requested.to_owned());
    if !state.sessions.contains_key(&resolved) {
        return error_body(id, 4001, "not_found", "not_found", None);
    }
    state.sub_seq += 1;
    let next_sub = state.sub_seq;
    let subscription = {
        let subs = state.subscribers.entry(conn).or_default();
        if let Some(existing) = subs.get(&resolved) {
            existing.clone()
        } else {
            // A retired subscription must not come back under its old id (fact R24).
            let fresh = format!("sub-{conn}-{next_sub}");
            subs.insert(resolved.clone(), fresh.clone());
            fresh
        }
    };
    let session = state.sessions.get(&resolved).expect("session");
    snapshot(&resolved, session, &subscription, id)
}

fn snapshot(session_id: &str, session: &Session, subscription: &str, id: &Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "authority_epoch": 1,
            "execution_generation": 1,
            "info": {"desktop_protocol": "1"},
            "last_sequence": session.seq,
            "message_count": 0,
            "messages": [],
            "pending": [],
            "prompts": [],
            "replay_epoch": session.epoch,
            "revision": 0,
            "running": false,
            "session_id": session_id,
            "stored_session_id": session_id,
            "subscription_id": subscription
        }
    })
    .to_string()
}

fn since(inner: &Inner, id: &Value, params: &Value) -> String {
    if let Some(fields) = extra_keys(params, SINCE_KEYS) {
        return error_body(id, 4001, "invalid_params", "invalid_params", Some(fields));
    }
    let Some(session_id) = params["session_id"].as_str() else {
        return error_body(
            id,
            4001,
            "invalid_params",
            "invalid_params",
            Some(vec!["session_id".to_owned()]),
        );
    };
    // A missing or null epoch is allowed and always forces a snapshot (fact R7); a
    // non-string epoch is refused (fact R8).
    let epoch = match &params["replay_epoch"] {
        Value::String(text) => Some(text.as_str()),
        Value::Null => None,
        _ => {
            return error_body(
                id,
                4001,
                "invalid_params",
                "invalid_params",
                Some(vec!["replay_epoch".to_owned()]),
            );
        }
    };
    let Some(cursor) = params["last_sequence"].as_u64() else {
        return error_body(
            id,
            4001,
            "invalid_params",
            "invalid_params",
            Some(vec!["last_sequence".to_owned()]),
        );
    };
    let state = inner.state.lock().expect("mock lock");
    let Some(session) = state.sessions.get(session_id) else {
        return error_body(id, 4001, "not_found", "not_found", None);
    };
    let forced =
        epoch != Some(session.epoch.as_str()) || cursor > session.seq || cursor < session.watermark;
    let events: Vec<Value> = if forced {
        Vec::new()
    } else {
        session
            .ring
            .iter()
            .filter(|event| event.seq > cursor)
            .map(|event| {
                json!({
                    "payload": event.payload,
                    "replay_epoch": event.epoch,
                    "seq": event.seq,
                    "session_id": event.session_id,
                    "type": event.kind
                })
            })
            .collect()
    };
    let last_sequence = events
        .last()
        .and_then(|event| event["seq"].as_u64())
        .unwrap_or(cursor);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "count": events.len(),
            "epoch": session.epoch,
            "events": events,
            "last_sequence": last_sequence,
            "latest_seq": session.seq,
            "replay_epoch": session.epoch,
            "snapshot_required": forced,
            "truncated": forced
        }
    })
    .to_string()
}

fn extra_keys(params: &Value, allowed: &[&str]) -> Option<Vec<String>> {
    let Some(object) = params.as_object() else {
        return Some(vec!["params".to_owned()]);
    };
    let mut unknown: Vec<String> = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .cloned()
        .collect();
    unknown.sort();
    if unknown.is_empty() {
        None
    } else {
        Some(unknown)
    }
}

fn error_body(
    id: &Value,
    code: i64,
    message: &str,
    reason: &str,
    fields: Option<Vec<String>>,
) -> String {
    let mut data = json!({"reason": reason});
    if let Some(fields) = fields {
        data["fields"] = json!(fields);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message, "data": data}
    })
    .to_string()
}

enum Script {
    Failure(DialFailure),
    Parked(String),
}

/// `Dial` for the mock. One fresh ticket and handshake per attempt.
pub struct MockDial {
    gateway: MockGateway,
    home: tempfile::TempDir,
    calls: u64,
    scripts: Vec<(u64, Script)>,
}

impl MockDial {
    pub fn new(gateway: &MockGateway) -> Self {
        Self {
            gateway: gateway.clone(),
            home: tempfile::TempDir::new().expect("mock home"),
            calls: 0,
            scripts: Vec::new(),
        }
    }

    /// The `calls`-th dial (1-based) returns `failure` and does not open a socket.
    pub fn fail_on(&mut self, call: u64, failure: DialFailure) {
        self.scripts.push((call, Script::Failure(failure)));
    }

    /// The `calls`-th dial maps a parked profile named `name` through `DialFailure::from`.
    pub fn fail_parked_on(&mut self, call: u64, name: &str) {
        self.scripts.push((call, Script::Parked(name.to_owned())));
    }

    pub fn home_path(&self) -> PathBuf {
        self.home.path().to_path_buf()
    }

    pub fn dials(&self) -> u64 {
        self.calls
    }
}

impl Dial for MockDial {
    fn dial(
        &mut self,
        generation: u64,
    ) -> impl Future<Output = Result<Dialed, DialFailure>> + Send {
        self.calls += 1;
        let call = self.calls;
        let script = self
            .scripts
            .iter()
            .position(|(index, _)| *index == call)
            .map(|index| self.scripts.remove(index).1);
        let gateway = self.gateway.clone();
        let home = self.home.path().to_path_buf();
        async move {
            if let Some(script) = script {
                return match script {
                    Script::Failure(failure) => Err(failure),
                    Script::Parked(name) => Err(DialFailure::from(ProfileLookupError::Parked {
                        name,
                        reason: "held".to_owned(),
                    })),
                };
            }
            attach(gateway, home, generation).await
        }
    }
}

async fn attach(
    gateway: MockGateway,
    home: PathBuf,
    _generation: u64,
) -> Result<Dialed, DialFailure> {
    let canonical = home.canonicalize().unwrap_or(home.clone());
    let origin = format!("http://127.0.0.1:{}", gateway.port());
    let identify = json!({
        "runtime_protocol": 1,
        "state": "ready",
        "capabilities": ["session-authority-v1"],
        "authority_epoch": AUTHORITY,
        "instance_id": INSTANCE_ID,
        "api_origin": origin,
        "supervisor": "none",
        "served_profiles": [{"home": canonical, "profile_id": PROFILE_ID}]
    });
    let endpoint = Endpoint::from_identify(&identify, &home).map_err(as_retry)?;
    let secret = gateway.issue_ticket();
    let ticket = Ticket::parse(secret).map_err(as_retry)?;
    let mut connection = connect_with(
        &endpoint,
        ticket,
        &ConnectOptions {
            timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(as_retry)?;
    let handshake = handshake_with(
        &mut connection,
        &HandshakeOptions {
            ready_timeout: Duration::from_secs(5),
            call_timeout: Duration::from_secs(5),
        },
    )
    .await
    .map_err(as_retry)?;
    Ok(Dialed {
        process_epoch: handshake.ready.replay_epoch,
        heartbeat: handshake.ready.heartbeat == Some(true),
        capabilities: handshake.describe.capabilities,
        parts: connection.into_parts(),
    })
}

fn as_retry(error: impl std::fmt::Display) -> DialFailure {
    DialFailure::Retry {
        reason: error.to_string(),
        next_step: "Try again.".to_owned(),
    }
}
