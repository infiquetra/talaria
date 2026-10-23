//! Mock gateway and the live-test precondition check.
//!
//! The mock speaks the Hermes v0.22.4 control line and session socket. It does
//! not start a real gateway.

#![allow(dead_code)]

pub mod peer;

use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use hermes_client::{
    DiscoverOptions, DiscoveryError, Endpoint, EnsurePolicy, ProfileId, ProfileLookupError,
    default_home, discover_with,
};
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf,
};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

pub const INSTANCE_ID: &str = "0123456789abcdef0123456789abcdef";
pub const CODE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

static UMASK: Once = Once::new();

fn install_umask() {
    UMASK.call_once(|| {
        let _ = rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    });
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub fn set_mode(path: &Path, mode: u32) {
    let permissions = std::fs::Permissions::from_mode(mode);
    if std::fs::set_permissions(path, permissions).is_err() {
        panic!("mode change failed");
    }
}

pub struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    pub fn new() -> Self {
        install_umask();
        let dir = tempfile::Builder::new()
            .prefix("t")
            .rand_bytes(3)
            .tempdir()
            .unwrap_or_else(|_| panic!("temp home failed"));
        set_mode(dir.path(), 0o700);
        let socket = dir.path().join("gateway.sock");
        if socket.as_os_str().len() > 100 {
            panic!("TMPDIR is too long for a control socket");
        }
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn canonical(&self) -> String {
        match std::fs::canonicalize(self.dir.path()) {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(_) => panic!("canonical home failed"),
        }
    }
}

pub fn ensure_fixture(home: &Path) {
    let parent = home.join("profiles");
    let fixture = parent.join("fixture");
    if std::fs::create_dir_all(&fixture).is_err() {
        panic!("fixture directory failed");
    }
    set_mode(&parent, 0o700);
    set_mode(&fixture, 0o700);
}

pub fn write_program(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    if std::fs::write(&path, body).is_err() {
        panic!("script write failed");
    }
    set_mode(&path, 0o700);
    path
}

pub fn bind_stale(path: &Path, mode: u32) {
    if std::os::unix::net::UnixListener::bind(path).is_err() {
        panic!("stale socket setup failed");
    }
    set_mode(path, mode);
}

pub struct DeleteOnDrop(pub PathBuf);

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn canonical_temp() -> PathBuf {
    match std::fs::canonicalize(std::env::temp_dir()) {
        Ok(path) => path,
        Err(_) => panic!("TMPDIR is too long for a control socket"),
    }
}

#[derive(Clone)]
struct Issued {
    value: String,
    used: bool,
    expired: bool,
}

#[derive(Clone)]
pub enum ControlReply {
    Line(String),
    Close,
    Silence,
    Bytes {
        len: usize,
        newline: bool,
    },
    Lines(String, String),
    Issue {
        profile_id: String,
        instance_id: String,
        expired: bool,
    },
}

#[derive(Clone)]
pub enum Echo {
    Version1,
    Ticket,
    Text(String),
    Absent,
}

#[derive(Clone)]
pub enum WsStep {
    Wait(Duration),
    Send(String),
    Binary,
    Close { code: u16, reason: String },
    Take,
    Drop,
}

#[derive(Clone)]
pub enum WsPlan {
    Idle,
    Talk {
        echo: Echo,
        steps: Vec<WsStep>,
        guard: bool,
    },
    Status(u16),
    Drop,
    Silence,
}

#[derive(Clone, Debug)]
pub struct WsSeen {
    pub path: String,
    pub host: String,
    pub origin: Option<String>,
    pub protocols: Vec<String>,
}

struct Shared {
    requests: Mutex<Vec<String>>,
    connections: AtomicUsize,
    issued: Mutex<Vec<Issued>>,
    seen: Mutex<Option<WsSeen>>,
    frames: Mutex<Vec<String>>,
    attempts: AtomicUsize,
}

pub struct AttachMock {
    control: tokio::task::JoinHandle<()>,
    ws: tokio::task::JoinHandle<()>,
    shared: Arc<Shared>,
    origin: String,
}

impl Drop for AttachMock {
    fn drop(&mut self) {
        self.control.abort();
        self.ws.abort();
    }
}

impl AttachMock {
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn requests(&self) -> Vec<String> {
        lock(&self.shared.requests).clone()
    }

    pub fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    pub fn attempts(&self) -> usize {
        self.shared.attempts.load(Ordering::SeqCst)
    }

    pub fn seen(&self) -> Option<WsSeen> {
        lock(&self.shared.seen).clone()
    }

    pub fn frames(&self) -> Vec<String> {
        lock(&self.shared.frames).clone()
    }

    pub fn issued(&self) -> Vec<String> {
        lock(&self.shared.issued)
            .iter()
            .map(|item| item.value.clone())
            .collect()
    }
}

pub async fn serve<F>(socket: &Path, replies_for: F, plan: WsPlan) -> AttachMock
where
    F: FnOnce(&str) -> Vec<ControlReply>,
{
    install_umask();
    let shared = Arc::new(Shared {
        requests: Mutex::new(Vec::new()),
        connections: AtomicUsize::new(0),
        issued: Mutex::new(Vec::new()),
        seen: Mutex::new(None),
        frames: Mutex::new(Vec::new()),
        attempts: AtomicUsize::new(0),
    });
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(_) => panic!("tcp bind failed"),
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(_) => panic!("tcp port missing"),
    };
    let origin = format!("http://127.0.0.1:{port}");
    let replies = replies_for(&origin);
    let ws_shared = Arc::clone(&shared);
    let ws = tokio::spawn(async move {
        run_ws(listener, plan, ws_shared).await;
    });
    let control = spawn_control(socket, replies, Arc::clone(&shared));
    AttachMock {
        control,
        ws,
        shared,
        origin,
    }
}

fn spawn_control(
    socket: &Path,
    replies: Vec<ControlReply>,
    shared: Arc<Shared>,
) -> tokio::task::JoinHandle<()> {
    if let Some(parent) = socket.parent()
        && !parent.is_dir()
    {
        panic!("control directory missing");
    }
    if std::fs::remove_file(socket).is_err() && socket.exists() {
        panic!("socket reset failed");
    }
    let listener = match UnixListener::bind(socket) {
        Ok(listener) => listener,
        Err(_) => panic!("control bind failed"),
    };
    set_mode(socket, 0o600);
    tokio::spawn(async move {
        run_control(listener, replies, shared).await;
    })
}

async fn run_control(listener: UnixListener, mut replies: Vec<ControlReply>, shared: Arc<Shared>) {
    replies.reverse();
    // Each control request owns its connection; a silent peer must not block
    // the next request. Dropping this set also cancels silent connections.
    let mut connections = tokio::task::JoinSet::new();
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            break;
        };
        shared.connections.fetch_add(1, Ordering::SeqCst);
        let reply = replies.pop().unwrap_or(ControlReply::Close);
        let shared = Arc::clone(&shared);
        connections.spawn(async move {
            let _ = handle_control(&mut stream, reply, &shared).await;
        });
    }
}

async fn handle_control(
    stream: &mut tokio::net::UnixStream,
    reply: ControlReply,
    shared: &Shared,
) -> std::io::Result<()> {
    let line = read_line(stream, 64 * 1024).await?;
    if !line.is_empty() {
        lock(&shared.requests).push(line);
    }
    match reply {
        ControlReply::Silence => std::future::pending::<()>().await,
        ControlReply::Close => {}
        ControlReply::Line(text) => {
            stream.write_all(text.as_bytes()).await?;
            stream.write_all(b"\n").await?;
        }
        ControlReply::Lines(first, second) => {
            stream.write_all(first.as_bytes()).await?;
            stream.write_all(b"\n").await?;
            stream.write_all(second.as_bytes()).await?;
            stream.write_all(b"\n").await?;
        }
        ControlReply::Bytes { len, newline } => {
            let chunk = vec![b'a'; 4096];
            let mut left = len;
            while left > 0 {
                let n = left.min(chunk.len());
                stream.write_all(&chunk[..n]).await?;
                left -= n;
            }
            if newline {
                stream.write_all(b"\n").await?;
            }
        }
        ControlReply::Issue {
            profile_id,
            instance_id,
            expired,
        } => {
            let value = token(lock(&shared.issued).len() as u64 + 1);
            lock(&shared.issued).push(Issued {
                value: value.clone(),
                used: false,
                expired,
            });
            let text = serde_json::json!({
                "ok": true,
                "protocol": 1,
                "id": 2,
                "result": {
                    "ticket": value,
                    "expires_in_seconds": 30,
                    "instance_id": instance_id,
                    "profile_id": profile_id,
                    "runtime_protocol": 1
                }
            })
            .to_string();
            stream.write_all(text.as_bytes()).await?;
            stream.write_all(b"\n").await?;
        }
    }
    Ok(())
}

async fn read_line(stream: &mut tokio::net::UnixStream, limit: usize) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if buf.len() >= limit {
            break;
        }
        match stream.read(&mut byte).await {
            Ok(0) | Err(_) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => buf.push(byte[0]),
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn token(n: u64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let mut out = String::with_capacity(43);
    let mut state = n.wrapping_add(0x9E37);
    for _ in 0..43 {
        let index = (state as usize) % ALPHABET.len();
        out.push(ALPHABET[index] as char);
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
    }
    out
}

async fn run_ws(listener: TcpListener, plan: WsPlan, shared: Arc<Shared>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        shared.attempts.fetch_add(1, Ordering::SeqCst);
        if serve_ws(stream, plan.clone(), &shared).await.is_err() {
            continue;
        }
    }
}

async fn serve_ws(mut stream: TcpStream, plan: WsPlan, shared: &Shared) -> std::io::Result<()> {
    if matches!(plan, WsPlan::Idle) {
        return Ok(());
    }
    if matches!(plan, WsPlan::Drop) {
        return Ok(());
    }
    let raw = read_http(&mut stream).await?;
    record_request(&raw, &shared.seen);
    let ticket = ticket_from_raw(&raw);
    if matches!(plan, WsPlan::Silence) {
        std::future::pending::<()>().await;
    }
    if let WsPlan::Status(status) = plan {
        let body = refusal(status, &ticket);
        stream.write_all(body.as_bytes()).await?;
        return Ok(());
    }
    let WsPlan::Talk { echo, steps, guard } = plan else {
        return Ok(());
    };
    if guard && reject_ticket(&ticket, &shared.issued) {
        let body = refusal(403, &ticket);
        stream.write_all(body.as_bytes()).await?;
        return Ok(());
    }
    let replay = Replay {
        buf: raw.into_bytes(),
        pos: 0,
        inner: stream,
    };
    let callback_echo = echo.clone();
    // Tungstenite fixes this callback's error type to an unboxed HTTP response.
    #[allow(clippy::result_large_err)]
    let callback =
        move |request: &Request, response: Response| set_echo(request, response, &callback_echo);
    let socket = tokio_tungstenite::accept_hdr_async(replay, callback)
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "upgrade"))?;
    run_steps(socket, steps, &shared.frames).await;
    Ok(())
}

fn reject_ticket(ticket: &str, issued: &Mutex<Vec<Issued>>) -> bool {
    let mut store = lock(issued);
    match store.iter_mut().find(|item| item.value == ticket) {
        Some(item) if !item.used && !item.expired => {
            item.used = true;
            false
        }
        _ => true,
    }
}

fn refusal(status: u16, ticket: &str) -> String {
    format!(
        "HTTP/1.1 {status} refused {ticket}\r\nX-Ticket: {ticket}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ticket}",
        ticket.len()
    )
}

async fn read_http(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    let marker = b"\r\n\r\n";
    let mut matched = 0usize;
    while buf.len() < 64 * 1024 {
        match stream.read(&mut byte).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                buf.push(byte[0]);
                if byte[0] == marker[matched] {
                    matched += 1;
                    if matched == marker.len() {
                        break;
                    }
                } else {
                    matched = usize::from(byte[0] == marker[0]);
                }
            }
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn record_request(raw: &str, seen: &Mutex<Option<WsSeen>>) {
    let path = raw
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string();
    let mut host = String::new();
    let mut origin = None;
    let mut protocols = Vec::new();
    for line in raw.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host = value.to_string();
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("sec-websocket-protocol") {
            protocols = value
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect();
        }
    }
    *lock(seen) = Some(WsSeen {
        path,
        host,
        origin,
        protocols,
    });
}

fn ticket_from_raw(raw: &str) -> String {
    for line in raw.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("sec-websocket-protocol") {
            return ticket_from_header(value);
        }
    }
    String::new()
}

fn ticket_from_header(value: &str) -> String {
    value
        .split(',')
        .filter_map(|part| part.trim().strip_prefix("hermes-gateway-ticket."))
        .next()
        .unwrap_or("")
        .to_string()
}

// Tungstenite requires this unboxed error response in its upgrade callback.
#[allow(clippy::result_large_err)]
fn set_echo(
    request: &Request,
    mut response: Response,
    echo: &Echo,
) -> Result<Response, ErrorResponse> {
    let name = http::header::SEC_WEBSOCKET_PROTOCOL;
    match echo {
        Echo::Absent => {
            response.headers_mut().remove(name);
        }
        Echo::Version1 => insert_header(&mut response, "hermes-gateway-v1"),
        Echo::Text(text) => insert_header(&mut response, text),
        Echo::Ticket => {
            let header = request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let chosen = header
                .split(',')
                .map(str::trim)
                .find(|part| part.starts_with("hermes-gateway-ticket."))
                .unwrap_or("hermes-gateway-v1");
            insert_header(&mut response, chosen);
        }
    }
    Ok(response)
}

fn insert_header(response: &mut Response, value: &str) {
    let Ok(header) = http::HeaderValue::from_str(value) else {
        return;
    };
    response
        .headers_mut()
        .insert(http::header::SEC_WEBSOCKET_PROTOCOL, header);
}

async fn run_steps<S>(mut socket: S, steps: Vec<WsStep>, frames: &Mutex<Vec<String>>)
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + Unpin,
{
    for step in steps {
        match step {
            WsStep::Wait(duration) => {
                let deadline = tokio::time::Instant::now() + duration;
                loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match tokio::time::timeout(remaining, socket.next()).await {
                        Ok(Some(Ok(Message::Text(text)))) => lock(frames).push(text.to_string()),
                        Ok(Some(Ok(_))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
                    }
                }
            }
            WsStep::Send(text) => {
                if socket
                    .send(Message::Text(Utf8Bytes::from(text)))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            WsStep::Binary => {
                if socket.send(Message::binary(vec![1u8, 2, 3])).await.is_err() {
                    break;
                }
            }
            WsStep::Close { code, reason } => {
                let frame = CloseFrame {
                    code: CloseCode::from(code),
                    reason: Utf8Bytes::from(reason),
                };
                let _ = socket.send(Message::Close(Some(frame))).await;
                break;
            }
            WsStep::Take => {
                if let Ok(Some(Ok(Message::Text(text)))) =
                    tokio::time::timeout(Duration::from_secs(2), socket.next()).await
                {
                    lock(frames).push(text.to_string());
                }
            }
            WsStep::Drop => break,
        }
    }
}

struct Replay<S> {
    buf: Vec<u8>,
    pos: usize,
    inner: S,
}

impl<S: AsyncRead + Unpin> AsyncRead for Replay<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.pos < this.buf.len() {
            let n = std::cmp::min(buf.remaining(), this.buf.len() - this.pos);
            buf.put_slice(&this.buf[this.pos..this.pos + n]);
            this.pos += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Replay<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

pub fn ready_identify(home: &str, origin: &str) -> serde_json::Value {
    let fixture = format!("{home}/profiles/fixture");
    serde_json::json!({
        "protocol": 1,
        "kind": "gateway",
        "pid": 4242,
        "start_time": 1790000000.5,
        "hermes_home": home,
        "profile": "default",
        "supervisor": "none",
        "launch_supervisor": "manual",
        "code_sha": CODE_SHA,
        "code_version": "0.22.4",
        "instance_id": INSTANCE_ID,
        "runtime_protocol": 1,
        "authority_epoch": 3,
        "state": "ready",
        "api_origin": origin,
        "served_profiles": [
            {"profile_id": home, "home": home},
            {"profile_id": fixture, "home": fixture}
        ],
        "parked_profiles": {},
        "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"]
    })
}

pub fn pad_json(mut value: serde_json::Value, width: usize) -> String {
    let Some(object) = value.as_object_mut() else {
        panic!("identify document is not an object");
    };
    object.insert("pad".to_string(), serde_json::Value::String(String::new()));
    let base = serde_json::to_string(&value).unwrap_or_else(|_| panic!("identify json"));
    if base.len() > width {
        panic!("identify document exceeds the control limit");
    }
    let extra = width - base.len();
    value.as_object_mut().expect("object").insert(
        "pad".to_string(),
        serde_json::Value::String("a".repeat(extra)),
    );
    let text = serde_json::to_string(&value).unwrap_or_else(|_| panic!("identify json"));
    if text.len() != width {
        panic!("padded identify has the wrong size");
    }
    text
}

pub fn frame_ready() -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "gateway.ready",
            "payload": {
                "skin": {"name": "default"},
                "change_events": true,
                "replay_epoch": "0123abcd",
                "heartbeat": true,
                "session_authority": true
            }
        }
    })
    .to_string()
}

pub fn frame_capabilities() -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"server_requests": ["approval.request"], "declines_not_shown": true}
    })
    .to_string()
}

pub fn frame_describe(profile_id: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "result": {
            "instance_id": INSTANCE_ID,
            "profile_id": profile_id,
            "authority_epoch": 3,
            "capabilities": ["session-authority-v1", "durable-admission-v1", "event-replay-v1"],
            "session_create": {"sources": ["local"], "parameters": ["text"]}
        }
    })
    .to_string()
}

pub fn success_steps(profile_id: &str) -> Vec<WsStep> {
    vec![
        WsStep::Wait(Duration::from_millis(100)),
        WsStep::Send(frame_ready()),
        WsStep::Take,
        WsStep::Send(frame_capabilities()),
        WsStep::Take,
        WsStep::Send(frame_describe(profile_id)),
    ]
}

pub fn assert_plain_text(text: &str) {
    assert!(!text.is_empty(), "empty text");
    assert!(
        !text.contains("hermes-gateway-ticket"),
        "ticket marker in text"
    );
    if looks_like_path(text) {
        panic!("absolute path in text");
    }
}

pub fn assert_step(step: &str) {
    assert_plain_text(step);
    assert!(step.ends_with('.'), "step must end with a full stop");
}

fn looks_like_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'/' {
            continue;
        }
        let boundary = index == 0
            || matches!(
                bytes[index - 1],
                b' ' | b'\n' | b'\t' | b'"' | b'\'' | b'(' | b'[' | b'=' | b',' | b':'
            );
        if !boundary || bytes.get(index + 1) == Some(&b'/') {
            continue;
        }
        if bytes
            .get(index + 1)
            .is_some_and(|next| !next.is_ascii_whitespace())
        {
            return true;
        }
    }
    false
}

pub struct LiveEnv {
    pub home: PathBuf,
    pub endpoint: Endpoint,
    pub fixture: ProfileId,
}

// The pre-build gateway may predate the pinned identity contract. Only an
// authenticated, well-formed reply proving a different revision may skip it.
// This probe never relaxes discovery, starts a gateway, or accepts an endpoint.
// Pointer-based homes retain the original discovery failure.
async fn incompatible_live_revision(home: &Path) -> Option<String> {
    let home = home.canonicalize().ok()?;
    let uid = rustix::process::getuid().as_raw();
    let metadata = std::fs::symlink_metadata(&home).ok()?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o002 != 0 {
        return None;
    }
    let socket = home.join("gateway.sock");
    let metadata = std::fs::symlink_metadata(&socket).ok()?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return None;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(socket).await.ok()?;
        if stream.peer_cred().ok()?.uid() != uid {
            return None;
        }
        stream
            .write_all(b"{\"protocol\":1,\"verb\":\"identify\",\"id\":1}\n")
            .await
            .ok()?;
        let mut line = Vec::new();
        BufReader::new(stream.take(524_290))
            .read_until(b'\n', &mut line)
            .await
            .ok()?;
        if line.last() != Some(&b'\n') || line.len() > 524_289 {
            return None;
        }
        let reply: serde_json::Value = serde_json::from_slice(&line).ok()?;
        if reply["ok"] != true || reply["protocol"] != 1 || reply["id"] != 1 {
            return None;
        }
        let result = reply.get("result")?.as_object()?;
        let sha = result.get("code_sha")?.as_str()?;
        // Restrict the only peer text included in a skip reason to a revision.
        if !(7..=40).contains(&sha.len()) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        hermes_contract::pinned::live_skip_reason(Some(sha))
    })
    .await
    .ok()
    .flatten()
}

pub async fn live_preconditions() -> Result<LiveEnv, String> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => {
            return Err("no Hermes gateway is running for this home".to_string());
        }
        Err(DiscoveryError::InvalidIdentity) => {
            if let Some(reason) = incompatible_live_revision(&home).await {
                return Err(reason);
            }
            let err = DiscoveryError::InvalidIdentity;
            panic!("discovery failed: {err} | {}", err.next_step());
        }
        Err(err) => panic!("discovery failed: {err} | {}", err.next_step()),
    };
    let fixture = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(ProfileLookupError::NeverDiscovered { name }) => {
            return Err(format!(
                "the gateway never discovered a profile named {name}"
            ));
        }
        Err(err) => panic!("profile lookup failed: {err} | {}", err.next_step()),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        return Err(reason);
    }
    Ok(LiveEnv {
        home,
        endpoint,
        fixture,
    })
}

pub fn report_skip(prefix: &str, reason: &str) {
    println!("{prefix}: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live gateway required");
    }
}
