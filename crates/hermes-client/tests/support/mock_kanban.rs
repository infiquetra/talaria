//! In-process board for compile and view tests.
//!
//! One request per connection. The events upgrade speaks the socket handshake
//! and records client text frames.

#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

const COLUMNS: &[&str] = &[
    "triage",
    "todo",
    "scheduled",
    "ready",
    "running",
    "blocked",
    "review",
    "done",
];

#[derive(Clone, Debug)]
pub struct MockOptions {
    pub parents_open: bool,
    pub default_assignee: String,
    pub tail_burst: bool,
    pub unauthorized: bool,
    pub fail_task_post: Option<u64>,
}

impl Default for MockOptions {
    fn default() -> Self {
        Self {
            parents_open: false,
            default_assignee: String::new(),
            tail_burst: false,
            unauthorized: false,
            fail_task_post: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub body: Value,
    pub ticket: String,
}

struct Inner {
    options: MockOptions,
    tasks: Mutex<Vec<Value>>,
    hits: Mutex<Vec<Hit>>,
    deduped: AtomicU64,
    pings: AtomicU64,
    task_posts: AtomicU64,
    failed_once: AtomicBool,
    next_id: AtomicU64,
    drop_sockets: AtomicBool,
    dropped: Notify,
    links: Mutex<Vec<(String, String)>>,
    comments: Mutex<Vec<(String, String, String)>>,
}

pub struct MockKanban {
    origin: String,
    inner: Arc<Inner>,
}

impl MockKanban {
    pub async fn start(options: MockOptions) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let tasks = load_tasks();
        let inner = Arc::new(Inner {
            options,
            tasks: Mutex::new(tasks),
            hits: Mutex::new(Vec::new()),
            deduped: AtomicU64::new(0),
            pings: AtomicU64::new(0),
            task_posts: AtomicU64::new(0),
            failed_once: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            drop_sockets: AtomicBool::new(false),
            dropped: Notify::new(),
            links: Mutex::new(Vec::new()),
            comments: Mutex::new(Vec::new()),
        });
        let shared = Arc::clone(&inner);
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    break;
                };
                let shared = Arc::clone(&shared);
                tokio::spawn(async move {
                    let _ = serve(socket, shared).await;
                });
            }
        });
        Self {
            origin: format!("http://127.0.0.1:{port}"),
            inner,
        }
    }

    pub fn origin(&self) -> String {
        self.origin.clone()
    }

    pub fn hits(&self) -> Vec<Hit> {
        self.inner.hits.lock().expect("hits").clone()
    }

    pub fn deduped(&self) -> u64 {
        self.inner.deduped.load(Ordering::SeqCst)
    }

    pub fn pings(&self) -> u64 {
        self.inner.pings.load(Ordering::SeqCst)
    }

    pub fn drop_sockets(&self) {
        self.inner.drop_sockets.store(true, Ordering::SeqCst);
        self.inner.dropped.notify_waiters();
    }

    pub fn task_count(&self) -> usize {
        self.inner.tasks.lock().expect("tasks").len()
    }

    /// Finish the running fixture card and move the first ready card to running.
    pub fn advance(&self) {
        let mut tasks = self.inner.tasks.lock().expect("tasks");
        if let Some(task) = tasks.iter_mut().find(|task| task["status"] == "running") {
            if let Some(object) = task.as_object_mut() {
                object.insert("status".into(), json!("done"));
                object.insert("latest_summary".into(), json!("mock result"));
                object.insert("result".into(), json!("mock result"));
            }
        }
        if let Some(task) = tasks.iter_mut().find(|task| task["status"] == "ready") {
            if let Some(object) = task.as_object_mut() {
                object.insert("status".into(), json!("running"));
            }
        }
    }
}

fn load_tasks() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/kanban/board.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let value: Value = serde_json::from_slice(&bytes).expect("fixture json");
    value["tasks"].as_array().expect("tasks").clone()
}

async fn serve(mut socket: TcpStream, inner: Arc<Inner>) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut temp = [0u8; 4096];
    let header_end = loop {
        let n = socket.read(&mut temp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&temp[..n]);
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > 1024 * 1024 {
            return Ok(());
        }
    };
    let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = header.split("\r\n");
    let request = lines.next().unwrap_or("");
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let body_at = header_end + 4;
    while buf.len() < body_at + length {
        let n = socket.read(&mut temp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&temp[..n]);
    }
    let body_bytes = if buf.len() >= body_at + length {
        buf[body_at..body_at + length].to_vec()
    } else {
        Vec::new()
    };
    if headers
        .get("upgrade")
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    {
        return upgrade(socket, &target, &headers, inner).await;
    }
    let (path, query) = split_target(&target);
    let body = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
    };
    let ticket = headers
        .get("x-hermes-gateway-ticket")
        .cloned()
        .unwrap_or_default();
    inner.hits.lock().expect("hits").push(Hit {
        method: method.clone(),
        path: path.clone(),
        query: query.clone(),
        body: body.clone(),
        ticket: ticket.clone(),
    });
    let (status, payload) = respond(&inner, &method, &path, &query, &body, &ticket);
    let bytes = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
    let response = format!(
        "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        reason(status),
        bytes.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.write_all(&bytes).await?;
    Ok(())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

fn respond(
    inner: &Inner,
    method: &str,
    path: &str,
    query: &BTreeMap<String, String>,
    body: &Value,
    ticket: &str,
) -> (u16, Value) {
    if inner.options.unauthorized || ticket.is_empty() {
        return (401, json!({"detail": "Unauthorized"}));
    }
    let board = query.get("board").map(String::as_str).unwrap_or("");
    let board_scoped = !matches!(
        path,
        "/api/plugins/kanban/boards"
            | "/api/plugins/kanban/workflow"
            | "/api/plugins/kanban/orchestration"
    );
    if board_scoped && board != "default" && board != "mock" {
        return (404, json!({"detail": "unknown board"}));
    }
    match (method, path) {
        ("GET", "/api/plugins/kanban/boards") => (200, boards_body()),
        ("GET", "/api/plugins/kanban/workflow") => (200, workflow_body()),
        ("GET", "/api/plugins/kanban/orchestration") => (200, orchestration_body(inner)),
        ("GET", "/api/plugins/kanban/board") => {
            let archived = query
                .get("include_archived")
                .is_some_and(|v| v == "1" || v == "true");
            (200, board_body(inner, archived))
        }
        ("GET", path) if path.starts_with("/api/plugins/kanban/tasks/") => {
            let id = path.rsplit('/').next().unwrap_or("");
            task_body(inner, id)
        }
        ("POST", "/api/plugins/kanban/tasks") => create_task(inner, body),
        ("PATCH", path) if path.starts_with("/api/plugins/kanban/tasks/") => {
            let id = path.rsplit('/').next().unwrap_or("");
            patch_task(inner, id, body)
        }
        ("POST", path) if path.contains("/comments") => comment(inner, path, body),
        ("POST", "/api/plugins/kanban/links") => link(inner, body),
        ("POST", "/api/plugins/kanban/dispatch") => (200, json!({"ok": true})),
        _ => (404, json!({"detail": "unknown board"})),
    }
}

fn boards_body() -> Value {
    json!({
        "boards": [
            {"slug": "default", "counts": {}, "total": 8, "is_current": true},
            {"slug": "mock", "counts": {}, "total": 8, "is_current": false}
        ],
        "current": "default"
    })
}

fn workflow_body() -> Value {
    json!({
        "columns": [
            {"key": "triage", "label": "Triage", "icon": "◇", "drag_target": true},
            {"key": "todo", "label": "Todo", "icon": "◻", "drag_target": true},
            {"key": "scheduled", "label": "Scheduled", "icon": "⏱", "drag_target": false},
            {"key": "ready", "label": "Ready", "icon": "▶", "drag_target": true},
            {"key": "running", "label": "Running", "icon": "●", "drag_target": false},
            {"key": "blocked", "label": "Blocked", "icon": "⊘", "drag_target": true},
            {"key": "review", "label": "Review", "icon": "◎", "drag_target": false},
            {"key": "done", "label": "Done", "icon": "✓", "drag_target": true}
        ],
        "manual": {
            "triage": ["ready", "todo"],
            "todo": ["ready", "scheduled", "triage"],
            "scheduled": ["ready", "todo", "triage"],
            "ready": ["blocked", "done", "review", "scheduled", "todo", "triage"],
            "running": ["blocked", "done", "ready", "review", "scheduled", "todo", "triage"],
            "blocked": ["done", "ready", "scheduled", "todo", "triage"],
            "review": ["done", "ready", "todo", "triage"],
            "done": ["ready", "todo", "triage"],
            "archived": ["ready", "todo", "triage"]
        },
        "archived": "archived"
    })
}

fn orchestration_body(inner: &Inner) -> Value {
    json!({
        "orchestrator_profile": "",
        "default_assignee": inner.options.default_assignee,
        "auto_decompose": true,
        "auto_promote_children": true,
        "resolved_orchestrator_profile": "default",
        "resolved_default_assignee": "default",
        "active_profile": "default"
    })
}

fn board_body(inner: &Inner, archived: bool) -> Value {
    let tasks = inner.tasks.lock().expect("tasks");
    let mut columns: BTreeMap<&str, Vec<Value>> =
        COLUMNS.iter().map(|name| (*name, Vec::new())).collect();
    for task in tasks.iter() {
        let status = task["status"].as_str().unwrap_or("");
        if status == "archived" && !archived {
            continue;
        }
        let name = if COLUMNS.contains(&status) {
            status
        } else if status == "archived" {
            continue;
        } else {
            "todo"
        };
        if let Some(bucket) = columns.get_mut(name) {
            bucket.push(task.clone());
        }
    }
    if archived {
        let archived_tasks: Vec<_> = tasks
            .iter()
            .filter(|task| task["status"] == "archived")
            .cloned()
            .collect();
        columns.entry("todo").or_default().extend(archived_tasks);
    }
    let columns = COLUMNS
        .iter()
        .map(|name| json!({"name": name, "tasks": columns.get(name).cloned().unwrap_or_default()}))
        .collect::<Vec<_>>();
    json!({
        "columns": columns,
        "tenants": [],
        "assignees": ["fixture"],
        "latest_event_id": 123,
        "now": 1793000000
    })
}

fn task_body(inner: &Inner, id: &str) -> (u16, Value) {
    let tasks = inner.tasks.lock().expect("tasks");
    let Some(task) = tasks.iter().find(|task| task["id"] == id) else {
        return (404, json!({"detail": "unknown board"}));
    };
    let links = inner.links.lock().expect("links");
    let parents: Vec<_> = links
        .iter()
        .filter(|(_, child)| child == id)
        .map(|(parent, _)| parent.clone())
        .collect();
    let children: Vec<_> = links
        .iter()
        .filter(|(parent, _)| parent == id)
        .map(|(_, child)| child.clone())
        .collect();
    (
        200,
        json!({
            "task": task,
            "comments": [],
            "events": [],
            "attachments": [],
            "links": {"parents": parents, "children": children},
            "link_tasks": [],
            "child_results": [],
            "runs": []
        }),
    )
}

fn create_task(inner: &Inner, body: &Value) -> (u16, Value) {
    let n = inner.task_posts.fetch_add(1, Ordering::SeqCst) + 1;
    if inner.options.fail_task_post == Some(n) && !inner.failed_once.swap(true, Ordering::SeqCst) {
        return (500, json!({"detail": "create failed"}));
    }
    let key = body["idempotency_key"].as_str().unwrap_or("");
    let mut tasks = inner.tasks.lock().expect("tasks");
    if let Some(existing) = tasks.iter().find(|task| {
        task["idempotency_key"] == key && task["status"] != "archived" && !key.is_empty()
    }) {
        inner.deduped.fetch_add(1, Ordering::SeqCst);
        return (200, json!({"task": existing}));
    }
    let id = format!("t_made_{}", inner.next_id.fetch_add(1, Ordering::SeqCst));
    let mut task = body.clone();
    if let Some(object) = task.as_object_mut() {
        object.insert("id".into(), json!(id));
        object.entry("status").or_insert(json!("ready"));
        object.entry("assignee").or_insert(Value::Null);
    }
    tasks.push(task.clone());
    drop(tasks);
    if let Some(parents) = body["parents"].as_array() {
        let mut links = inner.links.lock().expect("links");
        for parent in parents {
            if let Some(parent) = parent.as_str() {
                links.push((parent.to_string(), id.clone()));
            }
        }
    }
    (200, json!({"task": task}))
}

fn patch_task(inner: &Inner, id: &str, body: &Value) -> (u16, Value) {
    let status = body["status"].as_str().unwrap_or("");
    if status == "done" {
        let summary = body["summary"].as_str().unwrap_or("").trim();
        let result = body["result"].as_str().unwrap_or("").trim();
        let tasks = inner.tasks.lock().expect("tasks");
        let stored = tasks.iter().find(|task| task["id"] == id);
        let stored_result = stored
            .and_then(|task| task["result"].as_str())
            .unwrap_or("")
            .trim();
        if summary.is_empty() && result.is_empty() && stored_result.is_empty() {
            return (
                400,
                json!({"detail": format!("completion blocked: {id} has no result or summary evidence")}),
            );
        }
        if inner.options.parents_open {
            return (
                409,
                json!({"detail": format!("cannot move {id} to 'done': unsatisfied parent dependencies: t_triage (triage); complete the parents first (done or archived)")}),
            );
        }
    }
    let mut tasks = inner.tasks.lock().expect("tasks");
    let Some(task) = tasks.iter_mut().find(|task| task["id"] == id) else {
        return (404, json!({"detail": "unknown board"}));
    };
    if let Some(object) = task.as_object_mut() {
        if let Some(status) = body.get("status") {
            object.insert("status".into(), status.clone());
        }
        if let Some(summary) = body.get("summary") {
            object.insert("latest_summary".into(), summary.clone());
        }
    }
    (200, json!({"task": task}))
}

fn comment(inner: &Inner, path: &str, body: &Value) -> (u16, Value) {
    let id = path
        .trim_start_matches("/api/plugins/kanban/tasks/")
        .trim_end_matches("/comments")
        .trim_matches('/')
        .to_string();
    let text = body["body"].as_str().unwrap_or("").to_string();
    let author = body["author"].as_str().unwrap_or("dashboard").to_string();
    inner
        .comments
        .lock()
        .expect("comments")
        .push((id, author, text));
    (200, json!({"ok": true}))
}

fn link(inner: &Inner, body: &Value) -> (u16, Value) {
    let parent = body["parent_id"].as_str().unwrap_or("").to_string();
    let child = body["child_id"].as_str().unwrap_or("").to_string();
    inner.links.lock().expect("links").push((parent, child));
    (200, json!({"ok": true}))
}

async fn upgrade(
    mut socket: TcpStream,
    target: &str,
    headers: &BTreeMap<String, String>,
    inner: Arc<Inner>,
) -> std::io::Result<()> {
    let ticket = headers
        .get("x-hermes-gateway-ticket")
        .cloned()
        .unwrap_or_default();
    if inner.options.unauthorized || ticket.is_empty() {
        let body = br#"{"detail":"Unauthorized"}"#;
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(response.as_bytes()).await?;
        socket.write_all(body).await?;
        return Ok(());
    }
    let key = headers
        .get("sec-websocket-key")
        .cloned()
        .unwrap_or_default();
    let accept = ws_accept(&key);
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    socket.write_all(response.as_bytes()).await?;
    let (_path, query) = split_target(target);
    inner.hits.lock().expect("hits").push(Hit {
        method: "WS".into(),
        path: "/api/plugins/kanban/events".into(),
        query,
        body: Value::Null,
        ticket: ticket.clone(),
    });
    if inner.options.tail_burst {
        socket
            .write_all(&text_frame(
                r#"{"events":[{"id":124,"task_id":"t_ready","kind":"status"}],"cursor":124}"#,
            ))
            .await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        socket
            .write_all(&text_frame(
                r#"{"events":[{"id":125,"task_id":"t_running","kind":"status"}],"cursor":125}"#,
            ))
            .await?;
    }
    let dropped = inner.dropped.notified();
    tokio::pin!(dropped);
    let mut buf = Vec::new();
    let mut temp = [0u8; 1024];
    loop {
        if inner.drop_sockets.load(Ordering::SeqCst) {
            break;
        }
        tokio::select! {
            _ = &mut dropped => break,
            read = socket.read(&mut temp) => {
                let n = read?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&temp[..n]);
                while let Some(text) = pop_text(&mut buf) {
                    if text.contains("\"ping\"") {
                        inner.pings.fetch_add(1, Ordering::SeqCst);
                    }
                    inner.hits.lock().expect("hits").push(Hit {
                        method: "WS".into(),
                        path: "/api/plugins/kanban/events".into(),
                        query: BTreeMap::new(),
                        body: serde_json::from_str(&text).unwrap_or(Value::String(text)),
                        ticket: ticket.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn text_frame(payload: &str) -> Vec<u8> {
    let bytes = payload.as_bytes();
    let mut out = vec![0x81];
    if bytes.len() < 126 {
        out.push(bytes.len() as u8);
    } else {
        out.push(126);
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    }
    out.extend_from_slice(bytes);
    out
}

fn pop_text(buf: &mut Vec<u8>) -> Option<String> {
    if buf.len() < 2 {
        return None;
    }
    let masked = buf[1] & 0x80 != 0;
    let mut len = (buf[1] & 0x7f) as usize;
    let mut offset = 2;
    if len == 126 {
        if buf.len() < 4 {
            return None;
        }
        len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        offset = 4;
    }
    let mask_len = if masked { 4 } else { 0 };
    if buf.len() < offset + mask_len + len {
        return None;
    }
    let mask = if masked {
        buf[offset..offset + 4].to_vec()
    } else {
        vec![0, 0, 0, 0]
    };
    let start = offset + mask_len;
    let data: Vec<u8> = buf[start..start + len]
        .iter()
        .enumerate()
        .map(|(i, byte)| byte ^ mask[i % 4])
        .collect();
    buf.drain(..start + len);
    String::from_utf8(data).ok()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn split_target(target: &str) -> (String, BTreeMap<String, String>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut map = BTreeMap::new();
    if !query.is_empty() {
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            map.insert(key.to_string(), value.to_string());
        }
    }
    (path.to_string(), map)
}

fn ws_accept(key: &str) -> String {
    let mut data = key.as_bytes().to_vec();
    data.extend_from_slice(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64(&sha1(&data))
}

fn sha1(message: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;
    let bit_len = (message.len() as u64).saturating_mul(8);
    let mut data = message.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in data.chunks(64) {
        let mut words = [0u32; 80];
        for i in 0..16 {
            words[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().expect("word"));
        }
        for i in 16..80 {
            words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(words[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, word) in [h0, h1, h2, h3, h4].into_iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn base64(bytes: &[u8]) -> String {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        out.push(ALPHA[((n >> 18) & 63) as usize] as char);
        out.push(ALPHA[((n >> 12) & 63) as usize] as char);
        out.push(ALPHA[((n >> 6) & 63) as usize] as char);
        out.push(ALPHA[(n & 63) as usize] as char);
        i += 3;
    }
    if bytes.len() - i == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(ALPHA[((n >> 18) & 63) as usize] as char);
        out.push(ALPHA[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if bytes.len() - i == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(ALPHA[((n >> 18) & 63) as usize] as char);
        out.push(ALPHA[((n >> 12) & 63) as usize] as char);
        out.push(ALPHA[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}
