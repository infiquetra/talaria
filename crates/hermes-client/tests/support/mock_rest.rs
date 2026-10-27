#![allow(dead_code)]

use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub fn deep_merge(base: &mut Value, patch: &Value) {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            match base.get_mut(key) {
                Some(old) if old.is_object() && value.is_object() => deep_merge(old, value),
                Some(old) if old.is_object() && value.is_null() => {}
                _ => {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *base = patch.clone();
    }
}

pub fn effective_config(defaults: &Value, raw: &Value) -> Value {
    let mut value = defaults.clone();
    deep_merge(&mut value, raw);
    normalize(&mut value);
    expand(&mut value);
    value
}
fn normalize(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        let model = object.get("model").cloned();
        if let Some(Value::Object(model)) = model {
            object.insert(
                "model".into(),
                model
                    .get("default")
                    .or_else(|| model.get("name"))
                    .cloned()
                    .unwrap_or(json!("")),
            );
            object.insert(
                "model_context_length".into(),
                model
                    .get("context_length")
                    .filter(|value| value.is_i64() || value.is_u64())
                    .cloned()
                    .unwrap_or(json!(0)),
            );
        } else {
            object.insert("model_context_length".into(), json!(0));
        }
    }
}
fn expand(value: &mut Value) {
    match value {
        Value::String(text) => {
            *text = expand_text(text);
        }
        Value::Array(items) => items.iter_mut().for_each(expand),
        Value::Object(object) => object.values_mut().for_each(expand),
        _ => {}
    }
}
fn expand_text(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let Some(end) = rest[start + 2..].find('}').map(|end| start + 2 + end) else {
            break;
        };
        result.push_str(&rest[..start]);
        let name = rest[start + 2..end].trim();
        let name = name.strip_prefix("env:").unwrap_or(name).trim();
        result.push_str(match name {
            "EXAMPLE_API_KEY" => "expanded-test-value-1111",
            "EXAMPLE_URL" => "http://expanded.invalid/expanded-test-value-2222",
            _ => &rest[start..=end],
        });
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/rest")
            .join(name),
    )
    .expect("recorded configuration fixture")
}

#[derive(Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl std::fmt::Debug for RecordedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordedRequest")
            .field("method", &self.method)
            .field("headers", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
type Route = (String, String);
type Handler = Arc<dyn Fn(&RecordedRequest) -> (u16, Value) + Send + Sync>;
#[derive(Default, Clone)]
struct Switch {
    failure: Option<(u16, Option<String>)>,
    first_401: bool,
    drop: bool,
    delay: Duration,
    body: Option<Vec<u8>>,
}
struct ExternalWrite {
    after: Route,
    before: Route,
    patch: Value,
    armed: bool,
}
struct State {
    requests: Vec<RecordedRequest>,
    tickets: BTreeSet<String>,
    defaults: Value,
    raw: Value,
    schema: Vec<u8>,
    switches: BTreeMap<Route, Switch>,
    routes: BTreeMap<Route, Handler>,
    managed: BTreeSet<String>,
    external: Option<ExternalWrite>,
}
pub struct MockRest {
    address: std::net::SocketAddr,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for MockRest {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl MockRest {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State {
            requests: Vec::new(),
            tickets: BTreeSet::new(),
            defaults: serde_json::from_slice(&fixture("config-defaults.json")).unwrap(),
            raw: serde_json::from_slice(&fixture("config-raw.json")).unwrap(),
            schema: fixture("config-schema.json"),
            switches: BTreeMap::new(),
            routes: BTreeMap::new(),
            managed: BTreeSet::new(),
            external: None,
        }));
        let served = state.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = served.clone();
                tokio::spawn(async move {
                    let _ = serve(stream, state).await;
                });
            }
        });
        Self {
            address,
            state,
            task,
        }
    }
    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }
    pub async fn requests(&self) -> Vec<RecordedRequest> {
        self.state.lock().unwrap().requests.clone()
    }
    pub fn with_route<F>(self, method: &str, path: &str, handler: F) -> Self
    where
        F: Fn(&RecordedRequest) -> (u16, Value) + Send + Sync + 'static,
    {
        self.state
            .lock()
            .unwrap()
            .routes
            .insert(route(method, path), Arc::new(handler));
        self
    }
    pub async fn fail_status(&self, method: &str, path: &str, status: u16, detail: Option<&str>) {
        self.state
            .lock()
            .unwrap()
            .switches
            .entry(route(method, path))
            .or_default()
            .failure = Some((status, detail.map(str::to_owned)));
    }
    pub async fn first_401(&self, method: &str, path: &str) {
        self.state
            .lock()
            .unwrap()
            .switches
            .entry(route(method, path))
            .or_default()
            .first_401 = true;
    }
    pub async fn drop_after_request(&self, method: &str, path: &str) {
        self.state
            .lock()
            .unwrap()
            .switches
            .entry(route(method, path))
            .or_default()
            .drop = true;
    }
    pub async fn delay(&self, method: &str, path: &str, duration: Duration) {
        self.state
            .lock()
            .unwrap()
            .switches
            .entry(route(method, path))
            .or_default()
            .delay = duration;
    }
    pub async fn response_body(&self, method: &str, path: &str, body: Vec<u8>) {
        self.state
            .lock()
            .unwrap()
            .switches
            .entry(route(method, path))
            .or_default()
            .body = Some(body);
    }
    pub async fn managed_keys(&self, keys: impl IntoIterator<Item = String>) {
        self.state.lock().unwrap().managed = keys.into_iter().collect();
    }
    pub async fn external_write(
        &self,
        after_method: &str,
        after_path: &str,
        before_method: &str,
        before_path: &str,
        patch: Value,
    ) {
        self.state.lock().unwrap().external = Some(ExternalWrite {
            after: route(after_method, after_path),
            before: route(before_method, before_path),
            patch,
            armed: false,
        });
    }
}
fn route(method: &str, path: &str) -> Route {
    (method.to_owned(), path.to_owned())
}

async fn serve(mut stream: TcpStream, state: Arc<Mutex<State>>) -> std::io::Result<()> {
    let request = read_request(&mut stream).await?;
    let (status, body, switch) = {
        let mut state = state.lock().unwrap();
        let key = route(&request.method, &request.path);
        if let Some(external) = &mut state.external
            && external.armed
            && external.before == key
        {
            let patch = external.patch.clone();
            state.external = None;
            deep_merge(&mut state.raw, &patch);
        }
        state.requests.push(request.clone());
        let switch = state.switches.entry(key.clone()).or_default().clone();
        if switch.first_401 {
            state.switches.entry(key.clone()).or_default().first_401 = false;
        }
        let (status, body) = respond(&request, &key, &switch, &mut state);
        if let Some(external) = &mut state.external
            && external.after == key
        {
            external.armed = true;
        }
        (status, body, switch)
    };
    if switch.drop {
        return Ok(());
    }
    if !switch.delay.is_zero() {
        tokio::time::sleep(switch.delay).await;
    }
    let head = format!(
        "HTTP/1.1 {status} response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await
}
fn respond(
    request: &RecordedRequest,
    key: &Route,
    switch: &Switch,
    state: &mut State,
) -> (u16, Vec<u8>) {
    let json_reply = |status, value: Value| (status, serde_json::to_vec(&value).unwrap());
    let path = request.path.split('?').next().unwrap();
    let public = matches!(path, "/api/config/schema" | "/api/config/defaults");
    let tickets: Vec<_> = request
        .headers
        .iter()
        .filter(|(name, _)| name == "x-hermes-gateway-ticket")
        .map(|(_, value)| value)
        .collect();
    if (!public && tickets.is_empty())
        || tickets.len() > 1
        || request.headers.iter().any(|(name, _)| name == "origin")
        || tickets
            .first()
            .is_some_and(|ticket| !state.tickets.insert((*ticket).clone()))
        || switch.first_401
    {
        return json_reply(401, json!({"detail":"Unauthorized"}));
    }
    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
    let has_profile = query_pairs(&request.path)
        .iter()
        .any(|(name, _)| name == "profile")
        || body.get("profile").is_some();
    if has_profile {
        return json_reply(403, json!({"detail":"profile_scope_mismatch"}));
    }
    if let Some((status, detail)) = &switch.failure {
        return json_reply(*status, json!({"detail":detail}));
    }
    if let Some(bytes) = &switch.body {
        return (200, bytes.clone());
    }
    if let Some(handler) = state.routes.get(key) {
        let (status, body) = handler(request);
        return json_reply(status, body);
    }
    match (request.method.as_str(), path) {
        ("GET", "/api/config/schema") => (200, state.schema.clone()),
        ("GET", "/api/config/defaults") => json_reply(200, state.defaults.clone()),
        ("GET", "/api/config") => {
            let mut value = if query_pairs(&request.path)
                .iter()
                .any(|(key, value)| key == "include_defaults" && value == "false")
            {
                let mut raw = state.raw.clone();
                normalize(&mut raw);
                raw
            } else {
                effective_config(&state.defaults, &state.raw)
            };
            if let Some(object) = value.as_object_mut() {
                object.retain(|key, _| !key.starts_with('_'));
            }
            json_reply(200, value)
        }
        ("PUT", "/api/config") => {
            let Some(mut patch) = body
                .get("config")
                .filter(|value| value.is_object())
                .cloned()
            else {
                return json_reply(422, json!({"detail":[]}));
            };
            for key in &state.managed {
                remove_key(&mut patch, key);
            }
            deep_merge(&mut state.raw, &patch);
            json_reply(200, json!({"ok":true}))
        }
        _ => json_reply(404, json!({"detail":"Not Found"})),
    }
}
fn remove_key(value: &mut Value, key: &str) {
    if let Some((first, rest)) = key.split_once('.') {
        if let Some(child) = value.get_mut(first) {
            remove_key(child, rest);
        }
    } else if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}
fn query_pairs(path: &str) -> Vec<(String, String)> {
    path.split_once('?').map_or_else(Vec::new, |(_, query)| {
        query
            .split('&')
            .map(|part| {
                let (name, value) = part.split_once('=').unwrap_or((part, ""));
                (decode_query(name), decode_query(value))
            })
            .collect()
    })
}
fn decode_query(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut input = text.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let pair = [
                input.next().unwrap_or_default(),
                input.next().unwrap_or_default(),
            ];
            if let Ok(text) = std::str::from_utf8(&pair)
                && let Ok(byte) = u8::from_str_radix(text, 16)
            {
                bytes.push(byte);
                continue;
            }
            bytes.extend_from_slice(&[b'%', pair[0], pair[1]]);
        } else {
            bytes.push(if byte == b'+' { b' ' } else { byte });
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
async fn read_request(stream: &mut TcpStream) -> std::io::Result<RecordedRequest> {
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(pos) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break pos + 4;
        }
        if bytes.len() > 64 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    };
    let text =
        std::str::from_utf8(&bytes[..header_end]).map_err(|_| std::io::ErrorKind::InvalidData)?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap_or_default().to_owned();
    let path = first.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect();
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > 16 * 1024 * 1024 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    while bytes.len() - header_end < length {
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    Ok(RecordedRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + length].to_vec(),
    })
}

#[derive(Default)]
pub struct CountingMinter(std::sync::atomic::AtomicUsize);
impl CountingMinter {
    pub fn count(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
impl hermes_client::rest::TicketMinter for CountingMinter {
    fn mint(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<hermes_client::Ticket, hermes_client::TicketError>,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            hermes_client::Ticket::parse(format!("TICKET-SENTINEL-{n}"))
        })
    }
}
