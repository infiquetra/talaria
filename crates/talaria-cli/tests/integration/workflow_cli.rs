#[cfg(feature = "workflow-mock")]
include!("workflow_cli_mock.rs");

mod diagnostics {
    use serde_json::{Value, json};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, UnixListener};

    const INSTANCE: &str = "0123456789abcdef0123456789abcdef";

    fn hostile() -> String {
        format!(
            "alpha\n{esc}[2J\n{esc}]52;c;dGVzdA==\u{7}\n\u{9b}2J\nbefore\rafter\n\u{2028}line\u{2029}sep",
            esc = '\u{1b}'
        )
    }

    fn assert_sanitized(text: &str) {
        assert!(
            text.contains("alpha\n"),
            "intentional newline was not preserved: {text:?}"
        );
        assert!(
            text.contains("\u{241b}[2J"),
            "ESC stand-in missing: {text:?}"
        );
        assert!(
            text.contains("\u{241b}]52;c;dGVzdA==\u{2407}"),
            "OSC 52 stand-in missing: {text:?}"
        );
        assert!(
            text.contains("\u{2039}U+009B\u{203a}2J"),
            "C1 stand-in missing: {text:?}"
        );
        assert!(
            text.contains("before\u{240d}after"),
            "CR stand-in missing: {text:?}"
        );
        assert!(
            text.contains("\u{2039}U+2028\u{203a}line\u{2039}U+2029\u{203a}sep"),
            "Zl/Zp stand-in missing: {text:?}"
        );
        assert!(
            !text.contains('\u{240a}'),
            "newline was replaced with a stand-in: {text:?}"
        );
        for character in text.chars() {
            let code = u32::from(character);
            if character == '\n' || character == '\t' {
                continue;
            }
            assert!(
                code > 0x1f
                    && code != 0x7f
                    && !(0x80..=0x9f).contains(&code)
                    && character != '\u{2028}'
                    && character != '\u{2029}',
                "raw control U+{code:04X} in {text:?}"
            );
        }
    }

    #[derive(Clone, Copy)]
    enum Mode {
        Detail,
        Warning,
    }

    struct Gateway {
        home: PathBuf,
        log: Arc<Mutex<Vec<String>>>,
        _dir: tempfile::TempDir,
        shutdown: Option<tokio::sync::watch::Sender<bool>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for Gateway {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(true);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    impl Gateway {
        fn start(mode: Mode) -> Self {
            let dir = tempfile::Builder::new()
                .prefix("t")
                .rand_bytes(4)
                .tempdir_in("/tmp")
                .expect("temp home");
            let home = dir.path().canonicalize().expect("canonical home");
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))
                .expect("home mode");
            let socket = home.join("gateway.sock");
            assert!(
                socket.as_os_str().len() < 100,
                "control socket path is too long"
            );
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
            let log = Arc::new(Mutex::new(Vec::new()));
            let server_log = Arc::clone(&log);
            let server_home = home.clone();
            let thread = std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("runtime");
                runtime.block_on(serve(
                    server_home,
                    socket,
                    mode,
                    server_log,
                    shutdown_rx,
                    ready_tx,
                ));
            });
            let _port = ready_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("gateway ready");
            Self {
                home,
                log,
                _dir: dir,
                shutdown: Some(shutdown_tx),
                thread: Some(thread),
            }
        }

        fn requests(&self) -> Vec<String> {
            self.log
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone()
        }
    }

    async fn serve(
        home: PathBuf,
        socket: PathBuf,
        mode: Mode,
        log: Arc<Mutex<Vec<String>>>,
        shutdown: tokio::sync::watch::Receiver<bool>,
        ready: std::sync::mpsc::Sender<u16>,
    ) {
        let tcp = TcpListener::bind("127.0.0.1:0").await.expect("tcp bind");
        let port = tcp.local_addr().expect("tcp port").port();
        let _ = std::fs::remove_file(&socket);
        let control = UnixListener::bind(&socket).expect("control bind");
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .expect("socket mode");
        let origin = format!("http://127.0.0.1:{port}");
        let _ = ready.send(port);
        let tasks = Arc::new(AtomicU64::new(0));
        let http_shutdown = shutdown.clone();
        let control_shutdown = shutdown.clone();
        let http_log = Arc::clone(&log);
        let http_home = home.clone();
        tokio::select! {
            _ = shutdown_changed(shutdown) => {}
            _ = async {
                tokio::join!(
                    accept_http(tcp, mode, tasks, http_log, http_home, http_shutdown),
                    accept_control(control, origin, home, control_shutdown),
                );
            } => {}
        }
    }

    async fn shutdown_changed(mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let _ = shutdown.changed().await;
    }

    async fn accept_http(
        listener: TcpListener,
        mode: Mode,
        tasks: Arc<AtomicU64>,
        log: Arc<Mutex<Vec<String>>>,
        profile: PathBuf,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let tasks = Arc::clone(&tasks);
                    let log = Arc::clone(&log);
                    let profile = profile.display().to_string();
                    tokio::spawn(async move {
                        if let Err(error) = dispatch_tcp(stream, mode, &tasks, &log, profile).await {
                            push(&log, format!("tcp: {error}"));
                        }
                    });
                }
            }
        }
    }

    async fn accept_control(
        listener: UnixListener,
        origin: String,
        home: PathBuf,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let origin = origin.clone();
                    let home = home.display().to_string();
                    tokio::spawn(async move {
                        let _ = handle_control(stream, origin, home).await;
                    });
                }
            }
        }
    }

    fn push(log: &Mutex<Vec<String>>, line: String) {
        log.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(line);
    }

    async fn dispatch_tcp(
        stream: tokio::net::TcpStream,
        mode: Mode,
        tasks: &AtomicU64,
        log: &Mutex<Vec<String>>,
        profile: String,
    ) -> std::io::Result<()> {
        let mut peeked = [0u8; 4096];
        let count = stream.peek(&mut peeked).await?;
        let preview = String::from_utf8_lossy(&peeked[..count]);
        if preview.contains("/api/ws") {
            push(log, "WS /api/ws".into());
            serve_ws(stream, profile)
                .await
                .map_err(std::io::Error::other)?;
            return Ok(());
        }
        serve_http(stream, mode, tasks, log).await
    }

    async fn serve_http(
        mut stream: tokio::net::TcpStream,
        mode: Mode,
        tasks: &AtomicU64,
        log: &Mutex<Vec<String>>,
    ) -> std::io::Result<()> {
        let header = read_headers(&mut stream).await?;
        let length = content_length(&header);
        let mut body = vec![0u8; length];
        if length > 0 {
            stream.read_exact(&mut body).await?;
        }
        let request = header.lines().next().unwrap_or("").to_string();
        push(log, request.clone());
        let mut parts = request.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        stream
            .write_all(&http_reply(method, path, mode, tasks))
            .await
    }

    async fn read_headers(stream: &mut tokio::net::TcpStream) -> std::io::Result<String> {
        let mut bytes = Vec::new();
        let mut one = [0u8; 1];
        let marker = b"\r\n\r\n";
        let mut matched = 0usize;
        while bytes.len() < 65_536 {
            let count = stream.read(&mut one).await?;
            if count == 0 {
                break;
            }
            bytes.push(one[0]);
            if one[0] == marker[matched] {
                matched += 1;
                if matched == marker.len() {
                    break;
                }
            } else {
                matched = usize::from(one[0] == marker[0]);
            }
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn content_length(header: &str) -> usize {
        header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().ok())
                    .flatten()
            })
            .unwrap_or(0)
    }

    fn http_reply(method: &str, path: &str, mode: Mode, tasks: &AtomicU64) -> Vec<u8> {
        let route = path.split('?').next().unwrap_or(path);
        if route.ends_with("/orchestration") {
            return match mode {
                Mode::Detail => json_bytes(400, &json!({"detail": hostile()})),
                Mode::Warning => json_bytes(
                    200,
                    &json!({
                        "default_assignee": "",
                        "resolved_default_assignee": "default",
                        "active_profile": "default"
                    }),
                ),
            };
        }
        if route.ends_with("/board") {
            return json_bytes(
                200,
                &json!({
                    "columns": [],
                    "tenants": [],
                    "assignees": [],
                    "latest_event_id": 0
                }),
            );
        }
        if route.ends_with("/dispatch") || route.contains("/comments") || route.ends_with("/link") {
            return json_bytes(200, &json!({"ok": true}));
        }
        if route.contains("/tasks") {
            let id = route
                .rsplit('/')
                .next()
                .filter(|part| {
                    part.starts_with('t') && part[1..].chars().all(|c| c.is_ascii_digit())
                })
                .map(str::to_owned)
                .unwrap_or_else(|| format!("t{}", tasks.fetch_add(1, Ordering::SeqCst) + 1));
            let task = task_json(&id, if method == "PATCH" { "done" } else { "ready" });
            return if method == "POST" {
                json_bytes(200, &json!({"task": task, "warning": hostile()}))
            } else if method == "GET" && route.contains("/tasks/") {
                json_bytes(
                    200,
                    &json!({
                        "task": task,
                        "comments": [],
                        "events": [],
                        "attachments": [],
                        "links": {"parents": [], "children": []},
                        "link_tasks": [],
                        "child_results": [],
                        "runs": []
                    }),
                )
            } else {
                json_bytes(200, &json!({"task": task}))
            };
        }
        json_bytes(
            404,
            &json!({"detail": format!("unhandled {method} {path}")}),
        )
    }

    fn task_json(id: &str, status: &str) -> Value {
        json!({
            "id": id,
            "title": "card",
            "body": "",
            "status": status,
            "priority": 0,
            "skills": [],
            "link_counts": {"parents": 0, "children": 0},
            "comment_count": 0,
            "progress": {"done": 0, "total": 0}
        })
    }

    fn json_bytes(status: u16, body: &Value) -> Vec<u8> {
        let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
        let reason = if status == 200 { "OK" } else { "Error" };
        let header = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            payload.len()
        );
        let mut bytes = header.into_bytes();
        bytes.extend(payload);
        bytes
    }

    async fn handle_control(
        stream: tokio::net::UnixStream,
        origin: String,
        home: String,
    ) -> std::io::Result<()> {
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let request: Value = serde_json::from_str(line.trim()).unwrap_or_else(|_| json!({}));
        let id = request.get("id").cloned().unwrap_or_else(|| json!(1));
        let reply = if request["verb"] == "session-ticket" {
            static TICKETS: AtomicU64 = AtomicU64::new(1);
            json!({
                "ok": true,
                "protocol": 1,
                "id": id,
                "result": {
                    "ticket": format!("tkt{}", TICKETS.fetch_add(1, Ordering::SeqCst)),
                    "expires_in_seconds": 30,
                    "instance_id": INSTANCE,
                    "profile_id": request["params"]["profile_id"].as_str().unwrap_or(""),
                    "runtime_protocol": 1
                }
            })
        } else {
            json!({
                "ok": true,
                "protocol": 1,
                "id": id,
                "result": {
                    "runtime_protocol": 1,
                    "state": "ready",
                    "supervisor": "none",
                    "authority_epoch": 3,
                    "instance_id": INSTANCE,
                    "api_origin": origin,
                    "capabilities": ["session-authority-v1"],
                    "served_profiles": [{"profile_id": home, "home": home}],
                    "parked_profiles": {}
                }
            })
        };
        let mut bytes = serde_json::to_vec(&reply).unwrap_or_default();
        bytes.push(b'\n');
        reader.get_mut().write_all(&bytes).await
    }

    async fn serve_ws(stream: tokio::net::TcpStream, profile: String) -> Result<(), String> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

        #[allow(clippy::result_large_err)]
        let callback = |_request: &Request, mut response: Response| {
            response.headers_mut().insert(
                http::header::SEC_WEBSOCKET_PROTOCOL,
                http::HeaderValue::from_static("hermes-gateway-v1"),
            );
            Ok(response)
        };
        let mut socket = tokio_tungstenite::accept_hdr_async(stream, callback)
            .await
            .map_err(|error| error.to_string())?;
        let ready = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "gateway.ready",
                "payload": {
                    "skin": {"name": "default"},
                    "change_events": true,
                    "replay_epoch": "0123abcd",
                    "session_authority": true
                }
            }
        });
        socket
            .send(Message::Text(ready.to_string().into()))
            .await
            .map_err(|error| error.to_string())?;
        while let Some(message) = socket.next().await {
            let message = message.map_err(|error| error.to_string())?;
            let Message::Text(text) = message else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let Some(id) = value.get("id").cloned() else {
                continue;
            };
            let method = value["method"].as_str().unwrap_or("");
            let result = match method {
                "runtime.describe" => json!({
                    "instance_id": INSTANCE,
                    "profile_id": profile,
                    "authority_epoch": 3,
                    "capabilities": [
                        "session-authority-v1",
                        "durable-admission-v1",
                        "event-replay-v1"
                    ],
                    "session_create": {"sources": ["local"], "parameters": ["text"]}
                }),
                "model.options" => json!({
                    "model": "m-small",
                    "provider": "alpha",
                    "providers": [{
                        "slug": "alpha",
                        "name": "Alpha",
                        "authenticated": true,
                        "models": ["m-small"]
                    }]
                }),
                _ => json!({"server_requests": [], "declines_not_shown": true}),
            };
            let reply = json!({"jsonrpc": "2.0", "id": id, "result": result});
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn compile(home: &Path, script: &Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_talaria"))
            .args([
                "workflow",
                "compile",
                "--board",
                "lab",
                "--profile",
                "default",
                "--no-dispatch",
            ])
            .arg(script)
            .env("HERMES_HOME", home)
            .env("TALARIA_CONFIG_DIR", home)
            .env("HOME", home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("compile command")
    }

    fn script(home: &Path) -> PathBuf {
        let path = home.join("hostile.js");
        std::fs::write(
            &path,
            "export const meta = {name: 'hostile', description: 'one call'};\nawait agent('read the source', {label: 'read'});\n",
        )
        .expect("script");
        path
    }

    #[test]
    fn workflow_cli_sanitizes_gateway_compile_diagnostics() {
        let detail = Gateway::start(Mode::Detail);
        let source = script(&detail.home);
        let failed = compile(&detail.home, &source);
        let stderr = String::from_utf8_lossy(&failed.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&failed.stdout).into_owned();
        assert!(
            stderr.contains("cannot read orchestration settings"),
            "stderr={stderr:?} requests={:?}",
            detail.requests()
        );
        assert_sanitized(&stderr);
        assert_sanitized_or_empty(&stdout);

        let warning = Gateway::start(Mode::Warning);
        let source = script(&warning.home);
        let compiled = compile(&warning.home, &source);
        let stdout = String::from_utf8_lossy(&compiled.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&compiled.stderr).into_owned();
        assert!(
            compiled.status.success(),
            "stdout={stdout:?} stderr={stderr:?} requests={:?}",
            warning.requests()
        );
        assert!(
            stdout.contains("compiled "),
            "compile report missing: {stdout:?}"
        );
        assert_sanitized(&stdout);
        assert_sanitized_or_empty(&stderr);
    }

    fn assert_sanitized_or_empty(text: &str) {
        if text.is_empty() {
            return;
        }
        for character in text.chars() {
            let code = u32::from(character);
            if character == '\n' || character == '\t' {
                continue;
            }
            assert!(
                code > 0x1f
                    && code != 0x7f
                    && !(0x80..=0x9f).contains(&code)
                    && character != '\u{2028}'
                    && character != '\u{2029}',
                "raw control U+{code:04X} in {text:?}"
            );
        }
    }
}
