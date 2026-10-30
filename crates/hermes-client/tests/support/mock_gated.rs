//! Local gated gateway double. The approval endpoint and forced rejection of ticket
//! subprotocols stand in for the provider round trip and a non-loopback peer.
#![allow(dead_code)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{Message, handshake::derive_accept_key, protocol::Role};
use url::Url;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub auto_approve: bool,
    pub wrong_state: bool,
    pub ungated: bool,
    pub no_native: bool,
    pub refresh_expired: bool,
    pub refresh_down: bool,
    pub rotating: bool,
    pub expire_now: bool,
    pub no_authority: bool,
    pub refuse_upgrade: Option<u16>,
    pub echo_protocol: Option<String>,
    pub token_status: Option<u16>,
    pub token_drop: bool,
    pub token_stall: bool,
    pub ticket_unauthorized: usize,
    pub tls: bool,
    pub tls_files: Option<(std::path::PathBuf, std::path::PathBuf)>,
    pub listen_both: bool,
    pub ticket_snapshot: Option<std::path::PathBuf>,
    pub profiles: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Counts {
    pub status: usize,
    pub authorize: usize,
    pub token: usize,
    pub refresh: usize,
    pub tickets: usize,
    pub upgrades: usize,
    pub per_profile: HashMap<String, Vec<String>>,
    pub ticket_snapshot: Option<Vec<u8>>,
    pub log: Vec<String>,
    pub upgrade_headers: Vec<String>,
    pub protocols: Vec<String>,
}
struct Pending {
    challenge: String,
    redirect: String,
    state: String,
    expires: tokio::time::Instant,
}
struct Code {
    challenge: String,
    expires: tokio::time::Instant,
}
struct Inner {
    opts: Options,
    counts: Counts,
    pending: HashMap<String, Pending>,
    codes: HashMap<String, Code>,
    tickets: HashMap<String, tokio::time::Instant>,
    refresh_seen: HashSet<String>,
    counter: usize,
    revision: usize,
    current_access: String,
}
#[derive(Clone)]
pub struct MockGateway {
    addr: std::net::SocketAddr,
    inner: Arc<Mutex<Inner>>,
    tasks: Arc<Vec<JoinHandle<()>>>,
    root: Option<Vec<u8>>,
    kick: Arc<tokio::sync::Notify>,
    tls_dir: Option<Arc<tempfile::TempDir>>,
    addr_v6: Option<std::net::SocketAddr>,
}
impl MockGateway {
    pub async fn start(opts: Options) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock bind");
        let addr = listener.local_addr().expect("mock address");
        let (acceptor, root, tls_dir) = if let Some((cert_path, key_path)) = &opts.tls_files {
            let mut cert_reader =
                std::io::BufReader::new(std::fs::File::open(cert_path).expect("certificate file"));
            let certs = rustls_pemfile::certs(&mut cert_reader)
                .collect::<Result<Vec<_>, _>>()
                .expect("certificates");
            let root = certs.last().expect("certificate").to_vec();
            let mut key_reader =
                std::io::BufReader::new(std::fs::File::open(key_path).expect("key file"));
            let key = rustls_pemfile::private_key(&mut key_reader)
                .expect("private key syntax")
                .expect("private key");
            let config = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .expect("TLS server");
            (
                Some(tokio_rustls::TlsAcceptor::from(Arc::new(config))),
                Some(root),
                None,
            )
        } else if opts.tls {
            use rcgen::{
                BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer,
                KeyPair, KeyUsagePurpose,
            };
            use std::os::unix::fs::PermissionsExt;
            let mut ca_params =
                CertificateParams::new(Vec::<String>::new()).expect("CA parameters");
            ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            ca_params.key_usages.push(KeyUsagePurpose::KeyCertSign);
            let ca_key = KeyPair::generate().expect("CA key");
            let ca = ca_params.self_signed(&ca_key).expect("CA certificate");
            let issuer = Issuer::new(ca_params, ca_key);
            let mut leaf_params =
                CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()])
                    .expect("leaf parameters");
            leaf_params
                .extended_key_usages
                .push(ExtendedKeyUsagePurpose::ServerAuth);
            let leaf_key = KeyPair::generate().expect("server key");
            let leaf = leaf_params
                .signed_by(&leaf_key, &issuer)
                .expect("server certificate");
            let root = ca.der().to_vec();
            let directory = Arc::new(tempfile::tempdir().expect("TLS directory"));
            std::fs::write(directory.path().join("root.der"), &root).expect("write test root");
            std::fs::write(directory.path().join("server.der"), leaf.der())
                .expect("write test certificate");
            let key_path = directory.path().join("server-key.der");
            std::fs::write(&key_path, leaf_key.serialize_der()).expect("write test key");
            std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))
                .expect("protect test key");
            let key = rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into());
            let config = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![leaf.der().clone(), ca.der().clone()], key)
                .expect("TLS server");
            (
                Some(tokio_rustls::TlsAcceptor::from(Arc::new(config))),
                Some(root),
                Some(directory),
            )
        } else {
            (None, None, None)
        };
        let listener_v6 = if opts.listen_both {
            Some(
                TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, addr.port()))
                    .await
                    .expect("IPv6 mock bind"),
            )
        } else {
            None
        };
        let addr_v6 = listener_v6
            .as_ref()
            .map(|listener| listener.local_addr().expect("IPv6 address"));
        let inner = Arc::new(Mutex::new(Inner {
            opts,
            counts: Counts::default(),
            pending: HashMap::new(),
            codes: HashMap::new(),
            tickets: HashMap::new(),
            refresh_seen: HashSet::new(),
            counter: 0,
            revision: 1,
            current_access: "SENTINEL-access".into(),
        }));
        let kick = Arc::new(tokio::sync::Notify::new());
        let mut tasks = vec![serve_listener(
            listener,
            acceptor.clone(),
            inner.clone(),
            kick.clone(),
        )];
        if let Some(listener) = listener_v6 {
            tasks.push(serve_listener(
                listener,
                acceptor,
                inner.clone(),
                kick.clone(),
            ));
        }
        Self {
            addr,
            inner,
            tasks: Arc::new(tasks),
            root,
            kick,
            tls_dir,
            addr_v6,
        }
    }
    pub fn base(&self) -> Url {
        let scheme = if self.root.is_some() { "https" } else { "http" };
        Url::parse(&format!("{scheme}://{}/hermes", self.addr)).expect("mock URL")
    }
    pub fn base_v6(&self) -> Option<Url> {
        let scheme = if self.root.is_some() { "https" } else { "http" };
        self.addr_v6
            .map(|addr| Url::parse(&format!("{scheme}://{addr}/hermes")).expect("IPv6 URL"))
    }
    pub fn counts(&self) -> Counts {
        self.inner.lock().expect("mock state").counts.clone()
    }
    pub fn root(&self) -> &[u8] {
        self.root.as_deref().expect("TLS root")
    }
    pub fn tls_files(&self) -> &std::path::Path {
        self.tls_dir.as_ref().expect("TLS files").path()
    }
    pub fn set_ticket_unauthorized(&self, n: usize) {
        self.inner.lock().unwrap().opts.ticket_unauthorized = n;
    }
    pub fn drop_connections(&self) {
        self.kick.notify_waiters();
    }
    pub fn restart(&self) {
        self.inner.lock().unwrap().revision += 1;
        self.drop_connections();
    }
    pub fn set_refresh_expired(&self, on: bool) {
        self.inner.lock().unwrap().opts.refresh_expired = on;
    }
    pub fn set_refuse(&self, status: u16) {
        self.inner.lock().unwrap().opts.refuse_upgrade = Some(status);
    }
    pub fn set_echo(&self, protocol: Option<&str>) {
        self.inner.lock().unwrap().opts.echo_protocol = protocol.map(str::to_owned);
    }
}
impl Drop for MockGateway {
    fn drop(&mut self) {
        if Arc::strong_count(&self.tasks) == 1 {
            for task in self.tasks.iter() {
                task.abort();
            }
        }
    }
}
fn serve_listener(
    listener: TcpListener,
    acceptor: Option<tokio_rustls::TlsAcceptor>,
    shared: Arc<Mutex<Inner>>,
    kick: Arc<tokio::sync::Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let shared = shared.clone();
            let acceptor = acceptor.clone();
            let kick = kick.clone();
            tokio::spawn(async move {
                if let Some(acceptor) = acceptor {
                    if let Ok(stream) = acceptor.accept(socket).await {
                        serve(stream, shared, kick).await;
                    }
                } else {
                    serve(socket, shared, kick).await;
                }
            });
        }
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}
fn token_payload(expired: bool) -> Value {
    json!({"access_token":"SENTINEL-access", "refresh_token":"SENTINEL-refresh", "token_type":"Bearer", "expires_at":if expired { unix_now()-1 } else { unix_now()+43200 }, "provider":"basic", "user_id":"fixture"})
}
fn captured_frame(method: &str) -> Value {
    for file in [
        include_str!("../../../../fixtures/frames/plain-answer.jsonl"),
        include_str!("../../../../fixtures/frames/session-picker.jsonl"),
        include_str!("../../../../fixtures/frames/fleet-peek.jsonl"),
    ] {
        let mut request_id = None;
        for line in file.lines() {
            let Ok(row) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let frame = &row["frame"];
            if frame["method"] == method && row["dir"] == "out" {
                request_id = frame.get("id").cloned();
            }
            if request_id.is_some()
                && row["dir"] == "in"
                && frame.get("id") == request_id.as_ref()
                && frame.get("result").is_some()
            {
                return frame.clone();
            }
        }
    }
    panic!("recorded handshake or session response is missing")
}
fn ready() -> Value {
    include_str!("../../../../fixtures/frames/plain-answer.jsonl")
        .lines()
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .find(|v| v["frame"]["params"]["type"] == "gateway.ready")
        .expect("recorded ready")["frame"]
        .clone()
}
fn completion() -> Value {
    include_str!("../../../../fixtures/frames/plain-answer.jsonl")
        .lines()
        .filter_map(|s| serde_json::from_str::<Value>(s).ok())
        .find(|v| v["frame"]["params"]["type"] == "message.complete")
        .expect("recorded completion")["frame"]
        .clone()
}
async fn response<S: AsyncWrite + Unpin>(
    stream: &mut S,
    status: u16,
    headers: &[(&str, String)],
    body: &str,
) {
    let mut out = format!(
        "HTTP/1.1 {status} mock\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (key, value) in headers {
        out.push_str(&format!("{key}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    let _ = stream.write_all(out.as_bytes()).await;
}
async fn serve<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut stream: S,
    inner: Arc<Mutex<Inner>>,
    kick: Arc<tokio::sync::Notify>,
) {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") && head.len() < 64 * 1024 {
        let mut byte = [0];
        if stream.read_exact(&mut byte).await.is_err() {
            return;
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut words = first.split_whitespace();
    let method = words.next().unwrap_or("").to_ascii_uppercase();
    let target = words.next().unwrap_or("/");
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
        }
    }
    let size = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(1024 * 1024);
    let mut body = vec![0; size];
    if stream.read_exact(&mut body).await.is_err() {
        return;
    }
    let parsed = Url::parse(&format!("http://gateway.example.com{target}")).expect("mock target");
    let path = parsed
        .path()
        .strip_prefix("/hermes")
        .unwrap_or(parsed.path())
        .to_owned();
    let query: HashMap<String, String> = parsed
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let safe_query = if query.is_empty() {
        String::new()
    } else {
        format!(
            "?{}",
            parsed
                .query_pairs()
                .map(|(k, _)| format!("{k}=[redacted]"))
                .collect::<Vec<_>>()
                .join("&")
        )
    };
    {
        let mut state = inner.lock().unwrap();
        state
            .counts
            .log
            .push(format!("{method} {path}{safe_query}"));
    }
    if path == "/api/ws" {
        let (status, echo, accepted) = {
            let mut state = inner.lock().unwrap();
            state.counts.upgrades += 1;
            state.counts.upgrade_headers = headers.keys().cloned().collect();
            state.counts.protocols = headers
                .get("sec-websocket-protocol")
                .map(|p| p.split(',').map(|x| x.trim().to_owned()).collect())
                .unwrap_or_default();
            let valid = query
                .get("ticket")
                .and_then(|ticket| state.tickets.remove(ticket))
                .is_some_and(|until| until > tokio::time::Instant::now());
            let bad_subprotocol = state
                .counts
                .protocols
                .iter()
                .any(|p| p.starts_with("hermes-gateway-ticket."));
            (
                state
                    .opts
                    .refuse_upgrade
                    .unwrap_or(if valid && !bad_subprotocol { 101 } else { 403 }),
                state.opts.echo_protocol.clone(),
                valid && !bad_subprotocol,
            )
        };
        if status != 101 || !accepted {
            response(&mut stream, status, &[], "").await;
            return;
        }
        let Some(key) = headers.get("sec-websocket-key") else {
            response(&mut stream, 400, &[], "").await;
            return;
        };
        let mut handshake = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n",
            derive_accept_key(key.as_bytes())
        );
        if let Some(echo) = echo {
            handshake.push_str(&format!("Sec-WebSocket-Protocol: {echo}\r\n"));
        }
        handshake.push_str("\r\n");
        if stream.write_all(handshake.as_bytes()).await.is_err() {
            return;
        }
        let mut ws =
            tokio_tungstenite::WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
        let mut frame = ready();
        {
            let state = inner.lock().unwrap();
            if state.opts.no_authority {
                frame["params"]["payload"]["session_authority"] = json!(false);
            }
            frame["params"]["payload"]["replay_epoch"] =
                json!(format!("mock-epoch-{}", state.revision));
        }
        if ws.send(Message::text(frame.to_string())).await.is_err() {
            return;
        }
        loop {
            let text = tokio::select! { _ = kick.notified() => break, frame = ws.next() => match frame { Some(Ok(Message::Text(text))) => text, _ => break } };
            let Ok(request) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let name = request["method"].as_str().unwrap_or("");
            let profile = request["params"]["profile"]
                .as_str()
                .unwrap_or("default")
                .to_owned();
            let allowed = {
                let mut state = inner.lock().unwrap();
                state
                    .counts
                    .per_profile
                    .entry(profile.clone())
                    .or_default()
                    .push(name.to_owned());
                state.opts.profiles.is_empty() || state.opts.profiles.contains(&profile)
            };
            let reply = if !allowed {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":4001,"message":"profile mismatch","data":{"reason":"profile_mismatch"}}})
            } else if matches!(
                name,
                "client.capabilities"
                    | "runtime.describe"
                    | "session.list"
                    | "session.create"
                    | "prompt.submit"
                    | "session.resume"
            ) {
                let mut frame = captured_frame(name);
                frame["id"] = request["id"].clone();
                if name == "runtime.describe" {
                    let state = inner.lock().unwrap();
                    frame["result"]["instance_id"] =
                        json!(format!("mock-instance-{}", state.revision));
                    frame["result"]["profile_id"] = json!("fixture");
                }
                if name == "session.resume" {
                    // The recorder replaces this optional object with a redaction string.
                    // Omit the unavailable value rather than claiming it is a wire object.
                    if frame["result"]["info"]["launch_request"] == "[redacted]" {
                        frame["result"]["info"]
                            .as_object_mut()
                            .unwrap()
                            .remove("launch_request");
                    }
                    frame["result"]["session_id"] = request["params"]["session_id"].clone();
                    frame["result"]["stored_session_id"] = request["params"]["session_id"].clone();
                    frame["result"]["replay_epoch"] = json!("mock-epoch-1");
                }
                frame
            } else if name == "session.events.since" {
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"events":[],"latest_seq":0,"last_sequence":0,"epoch":"mock-epoch-1","replay_epoch":"mock-epoch-1","truncated":false,"snapshot_required":false,"count":0}})
            } else {
                json!({"jsonrpc":"2.0","id":request["id"],"result":{}})
            };
            if ws.send(Message::text(reply.to_string())).await.is_err() {
                break;
            }
            if name == "prompt.submit" {
                let _ = ws.send(Message::text(completion().to_string())).await;
            }
        }
        return;
    }
    let json_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let (status, response_headers, content, stall, drop_connection) = {
        let mut s = inner.lock().unwrap();
        match (method.as_str(), path.as_str()) {
            ("GET", "/api/status") => {
                s.counts.status += 1;
                let flows = if s.opts.ungated {
                    vec![]
                } else if s.opts.no_native {
                    vec!["cookie"]
                } else {
                    vec!["cookie", "native_pkce"]
                };
                (200, vec![], json!({"auth_required":!s.opts.ungated,"auth_providers":["basic"],"auth_flows":flows}).to_string(),false,false)
            }
            ("GET", "/auth/native/authorize") => {
                s.counts.authorize += 1;
                let redirect = query.get("redirect_uri").and_then(|v| Url::parse(v).ok());
                let valid = query
                    .get("code_challenge_method")
                    .is_some_and(|v| v.eq_ignore_ascii_case("S256"))
                    && query.get("code_challenge").is_some_and(|v| !v.is_empty())
                    && redirect.as_ref().is_some_and(|u| {
                        u.scheme() == "http"
                            && matches!(u.host_str(), Some("127.0.0.1" | "::1"))
                            && u.username().is_empty()
                            && u.password().is_none()
                            && u.fragment().is_none()
                    });
                if !valid {
                    (400, vec![], "bad authorize".into(), false, false)
                } else {
                    s.counter += 1;
                    let broker = format!("broker-{}", s.counter);
                    let pending = Pending {
                        challenge: query["code_challenge"].clone(),
                        redirect: query["redirect_uri"].clone(),
                        state: query.get("state").cloned().unwrap_or_default(),
                        expires: tokio::time::Instant::now() + Duration::from_secs(600),
                    };
                    if s.opts.auto_approve {
                        let (location, code) = issue_code(&mut s, pending);
                        (302, vec![("Location", location)], code, false, false)
                    } else {
                        s.pending.insert(broker.clone(), pending);
                        (
                            200,
                            vec![],
                            format!("/mock/approve?broker={broker}"),
                            false,
                            false,
                        )
                    }
                }
            }
            ("GET", "/mock/approve") => {
                if let Some(pending) = query
                    .get("broker")
                    .and_then(|v| s.pending.remove(v))
                    .filter(|p| p.expires > tokio::time::Instant::now())
                {
                    let (location, code) = issue_code(&mut s, pending);
                    (302, vec![("Location", location)], code, false, false)
                } else {
                    (400, vec![], String::new(), false, false)
                }
            }
            ("POST", "/auth/native/token") => {
                s.counts.token += 1;
                if s.opts.token_drop {
                    (0, vec![], String::new(), false, true)
                } else if s.opts.token_stall {
                    (0, vec![], String::new(), true, false)
                } else if let Some(code) = s.opts.token_status {
                    (code, vec![], "{}".into(), false, false)
                } else if !json_body["code"].is_string() || !json_body["code_verifier"].is_string()
                {
                    (422, vec![], "{}".into(), false, false)
                } else {
                    let found = s.codes.remove(json_body["code"].as_str().unwrap());
                    let verifier = json_body["code_verifier"].as_str().unwrap();
                    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                    if found.is_some_and(|c| {
                        c.expires > tokio::time::Instant::now() && c.challenge == challenge
                    }) {
                        (
                            200,
                            vec![],
                            token_payload(s.opts.expire_now).to_string(),
                            false,
                            false,
                        )
                    } else {
                        (
                            400,
                            vec![],
                            json!({"detail":"Invalid or expired authorization code."}).to_string(),
                            false,
                            false,
                        )
                    }
                }
            }
            ("POST", "/auth/native/refresh") => {
                s.counts.refresh += 1;
                let token = json_body["refresh_token"].as_str().unwrap_or("").to_owned();
                let reused = s.opts.rotating && !s.refresh_seen.insert(token);
                if s.opts.refresh_expired || reused {
                    (
                        401,
                        vec![],
                        json!({"error":"session_expired"}).to_string(),
                        false,
                        false,
                    )
                } else if s.opts.refresh_down {
                    (503, vec![], "{}".into(), false, false)
                } else {
                    let mut response = token_payload(s.opts.expire_now);
                    if s.opts.rotating {
                        s.counter += 1;
                        s.current_access = format!("SENTINEL-access-{}", s.counter);
                        response["access_token"] = json!(s.current_access);
                        response["refresh_token"] =
                            json!(format!("SENTINEL-refresh-{}", s.counter));
                    }
                    (200, vec![], response.to_string(), false, false)
                }
            }
            ("POST", "/api/auth/ws-ticket") => {
                s.counts.tickets += 1;
                s.counts.ticket_snapshot = s
                    .opts
                    .ticket_snapshot
                    .as_ref()
                    .and_then(|path| std::fs::read(path).ok());
                if s.opts.ticket_unauthorized > 0 {
                    s.opts.ticket_unauthorized -= 1;
                    (401, vec![], "{}".into(), false, false)
                } else if !headers
                    .get("authorization")
                    .is_some_and(|h| h == &format!("Bearer {}", s.current_access))
                {
                    (401, vec![], "{}".into(), false, false)
                } else {
                    s.counter += 1;
                    let ticket = format!("SENTINEL-ticket-{}", s.counter);
                    s.tickets.insert(
                        ticket.clone(),
                        tokio::time::Instant::now() + Duration::from_secs(30),
                    );
                    (
                        200,
                        vec![],
                        json!({"ticket":ticket,"ttl_seconds":30}).to_string(),
                        false,
                        false,
                    )
                }
            }
            ("GET", "/api/auth/me") => {
                if headers
                    .get("authorization")
                    .is_some_and(|h| h == &format!("Bearer {}", s.current_access))
                {
                    (200,vec![],json!({"user_id":"fixture","email":null,"display_name":"fixture","org_id":null,"provider":"basic","expires_at":unix_now()+43200}).to_string(),false,false)
                } else {
                    (401, vec![], "{}".into(), false, false)
                }
            }
            _ => (404, vec![], String::new(), false, false),
        }
    };
    if drop_connection {
        return;
    }
    if stall {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        return;
    }
    response(&mut stream, status, &response_headers, &content).await;
}
fn issue_code(s: &mut Inner, pending: Pending) -> (String, String) {
    s.counter += 1;
    let code = format!("SENTINEL-code-{}", s.counter);
    s.codes.insert(
        code.clone(),
        Code {
            challenge: pending.challenge,
            expires: tokio::time::Instant::now() + Duration::from_secs(120),
        },
    );
    let mut target = Url::parse(&pending.redirect).expect("redirect");
    target
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair(
            "state",
            if s.opts.wrong_state {
                "wrong"
            } else {
                &pending.state
            },
        );
    (target.to_string(), String::new())
}
