//! Scripted JSON-RPC peer for the channel tests.
//!
//! `Peer::pair` is a loopback WebSocket over a plain `TcpStream`
//! (`tokio_tungstenite::client_async`, not `connect_async`). Recorded replies and events are
//! copied from `crates/hermes-contract/tests/fixtures/live_two_turns.frames.jsonl`. No server
//! request was recorded, so `server_request` follows the contract schemas (`SudoRequestParams`
//! and the method name the test passes). A `request.cancel` event uses `RequestCancelPayload`.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use futures_util::{SinkExt, StreamExt};
use hermes_client::ConnectionParts;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

const FIXTURE: &str =
    include_str!("../../../hermes-contract/tests/fixtures/live_two_turns.frames.jsonl");

enum Incoming {
    Json(Value),
    NotJson,
}

enum Cmd {
    Hold,
    Release(oneshot::Sender<()>),
    Send {
        message: Message,
        done: oneshot::Sender<()>,
    },
    Close {
        code: u16,
        done: oneshot::Sender<()>,
    },
    Kill(oneshot::Sender<()>),
}

/// The server half of a loopback session socket.
pub struct Peer {
    cmds: mpsc::UnboundedSender<Cmd>,
    inbound: mpsc::UnboundedReceiver<Incoming>,
    next_srq: AtomicU64,
    json_frames: Arc<AtomicUsize>,
}

/// One recorded frame, parsed. Panics with a fixed message if the fixture is unreadable.
pub fn recorded_lines() -> Vec<Value> {
    FIXTURE
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).expect("recorded frame"))
        .collect()
}

/// The `result` object of the recorded reply with this integer id.
pub fn recorded_result(id: u64) -> Value {
    recorded_lines()
        .into_iter()
        .find(|frame| frame.get("id").and_then(Value::as_u64) == Some(id))
        .and_then(|frame| frame.get("result").cloned())
        .expect("recorded reply is missing")
}

/// The nth recorded event whose `params.type` equals `type_name`. The first is 0.
pub fn recorded_event(type_name: &str, nth: usize) -> Value {
    let mut seen = 0;
    for frame in recorded_lines() {
        if frame.pointer("/params/type").and_then(Value::as_str) == Some(type_name) {
            if seen == nth {
                return frame;
            }
            seen += 1;
        }
    }
    panic!("recorded event is missing")
}

impl Peer {
    /// Bind `127.0.0.1:0`, connect the client half, and accept the server half.
    ///
    /// The returned parts use next id 3 and an empty buffer.
    pub async fn pair() -> (ConnectionParts, Self) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback websocket pair failed");
        let address = listener
            .local_addr()
            .expect("loopback websocket pair failed");
        let client = tokio::spawn(async move {
            let stream = TcpStream::connect(address)
                .await
                .expect("loopback websocket pair failed");
            stream
                .set_nodelay(true)
                .expect("loopback websocket pair failed");
            let (socket, _) = tokio_tungstenite::client_async("ws://127.0.0.1/", stream)
                .await
                .expect("loopback websocket pair failed");
            socket
        });
        let (server_stream, _) = listener
            .accept()
            .await
            .expect("loopback websocket pair failed");
        server_stream
            .set_nodelay(true)
            .expect("loopback websocket pair failed");
        let server = tokio_tungstenite::accept_async(server_stream)
            .await
            .expect("loopback websocket pair failed");
        let client = client.await.expect("loopback websocket pair failed");
        let (sink, stream) = client.split();
        let parts = ConnectionParts::for_test(sink, stream, 3, Vec::new());
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let json_frames = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&json_frames);
        tokio::spawn(run_socket(server, cmd_rx, in_tx, counted));
        (
            parts,
            Self {
                cmds: cmd_tx,
                inbound: in_rx,
                next_srq: AtomicU64::new(1),
                json_frames,
            },
        )
    }

    /// JSON text frames the client has sent.
    ///
    /// A test that must show nothing was sent reads this. It is not a new scripted action.
    pub fn json_frames(&self) -> usize {
        self.json_frames.load(Ordering::SeqCst)
    }

    /// The next JSON frame the client sent. Panics with a fixed message, never with frame text.
    pub async fn recv_json(&mut self) -> Value {
        match self.inbound.recv().await {
            Some(Incoming::Json(value)) => value,
            Some(Incoming::NotJson) | None => panic!("peer expected a JSON text frame"),
        }
    }

    /// Send one text frame.
    pub async fn send_text(&self, text: &str) {
        self.send_message(Message::text(text)).await;
    }

    /// Send one binary frame. Test 24 uses valid UTF-8 JSON here.
    pub async fn send_binary(&self, bytes: &[u8]) {
        self.send_message(Message::Binary(bytes.to_vec().into()))
            .await;
    }

    /// Success reply. `id` is the value the client sent, unchanged.
    pub async fn reply_ok(&self, id: Value, result: Value) {
        let text = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .expect("reply encodes");
        self.send_text(&text).await;
    }

    /// Error reply. `reason` becomes `data.reason` when it is present.
    pub async fn reply_err(&self, id: Value, code: i64, message: &str, reason: Option<&str>) {
        let mut error = serde_json::json!({"code": code, "message": message});
        if let Some(reason) = reason {
            error["data"] = serde_json::json!({"reason": reason});
        }
        let text = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": error,
        }))
        .expect("error reply encodes");
        self.send_text(&text).await;
    }

    /// An `event` notification. Absent `session_id` and `seq` are left out of `params`.
    pub async fn event(
        &self,
        type_name: &str,
        session_id: Option<&str>,
        seq: Option<u64>,
        payload: Value,
    ) {
        let mut params = serde_json::Map::new();
        params.insert("type".to_string(), Value::String(type_name.to_string()));
        if let Some(session_id) = session_id {
            params.insert(
                "session_id".to_string(),
                Value::String(session_id.to_string()),
            );
        }
        if let Some(seq) = seq {
            params.insert("seq".to_string(), serde_json::json!(seq));
        }
        params.insert("payload".to_string(), payload);
        let text = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": params,
        }))
        .expect("event encodes");
        self.send_text(&text).await;
    }

    /// A server request. The id is `srq-` plus 12 lowercase hex digits.
    ///
    /// `session_id` is written into an object `params` when that key is absent.
    pub async fn server_request(&self, kind: &str, session_id: &str, mut params: Value) -> String {
        if let Some(object) = params.as_object_mut() {
            object
                .entry("session_id".to_string())
                .or_insert_with(|| Value::String(session_id.to_string()));
        }
        let n = self.next_srq.fetch_add(1, Ordering::SeqCst);
        let id = format!("srq-{n:012x}");
        let text = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": kind,
            "params": params,
        }))
        .expect("server request encodes");
        self.send_text(&text).await;
        id
    }

    /// Hold later outbound frames until [`release`](Self::release).
    pub fn hold(&self) {
        self.cmds.send(Cmd::Hold).expect("peer task ended");
    }

    /// Write the frames [`hold`](Self::hold) kept, in order.
    pub async fn release(&self) {
        let (done, ack) = oneshot::channel();
        self.cmds.send(Cmd::Release(done)).expect("peer task ended");
        ack.await.expect("peer task ended");
    }

    /// Send a close frame with this code.
    pub async fn close(&self, code: u16) {
        let (done, ack) = oneshot::channel();
        self.cmds
            .send(Cmd::Close { code, done })
            .expect("peer task ended");
        ack.await.expect("peer task ended");
    }

    /// Drop the socket. No close frame is sent.
    pub async fn kill(&self) {
        let (done, ack) = oneshot::channel();
        self.cmds.send(Cmd::Kill(done)).expect("peer task ended");
        ack.await.expect("peer task ended");
    }

    async fn send_message(&self, message: Message) {
        let (done, ack) = oneshot::channel();
        self.cmds
            .send(Cmd::Send { message, done })
            .expect("peer task ended");
        ack.await.expect("peer task ended");
    }
}

async fn run_socket(
    mut socket: WebSocketStream<TcpStream>,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
    inbound: mpsc::UnboundedSender<Incoming>,
    json_frames: Arc<AtomicUsize>,
) {
    let mut holding = false;
    let mut buffered: Vec<Message> = Vec::new();
    let mut reading = true;
    loop {
        tokio::select! {
            biased;
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Cmd::Hold => holding = true,
                    Cmd::Release(done) => {
                        holding = false;
                        for message in buffered.drain(..) {
                            if socket.send(message).await.is_err() {
                                break;
                            }
                        }
                        let _ = done.send(());
                    }
                    Cmd::Send { message, done } if holding => {
                        buffered.push(message);
                        let _ = done.send(());
                    }
                    Cmd::Send { message, done } => {
                        let _ = socket.send(message).await;
                        let _ = done.send(());
                    }
                    Cmd::Close { code, done } => {
                        let frame = Message::Close(Some(CloseFrame {
                            code: code.into(),
                            reason: "".into(),
                        }));
                        if holding {
                            buffered.push(frame);
                        } else {
                            let _ = socket.send(frame).await;
                        }
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
                    Some(Ok(Message::Text(text))) => deliver_text(&text, &inbound, &json_frames),
                    Some(Ok(Message::Binary(bytes))) => match std::str::from_utf8(&bytes) {
                        Ok(text) => deliver_text(text, &inbound, &json_frames),
                        Err(_) => {
                            let _ = inbound.send(Incoming::NotJson);
                        }
                    },
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | None => {
                        reading = false;
                        let _ = inbound.send(Incoming::NotJson);
                    }
                    Some(Err(_)) => {
                        reading = false;
                        let _ = inbound.send(Incoming::NotJson);
                    }
                }
            }
        }
    }
}

fn deliver_text(text: &str, inbound: &mpsc::UnboundedSender<Incoming>, json_frames: &AtomicUsize) {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => {
            json_frames.fetch_add(1, Ordering::SeqCst);
            let _ = inbound.send(Incoming::Json(value));
        }
        Err(_) => {
            let _ = inbound.send(Incoming::NotJson);
        }
    }
}
