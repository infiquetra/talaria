use super::{PROMPT, Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderScript {
    Ready,
    Slow,
    Todo,
    TodoFollowup,
}
impl From<&str> for ProviderScript {
    fn from(s: &str) -> Self {
        match s {
            "slow" => Self::Slow,
            "todo" => Self::Todo,
            "todo-followup" => Self::TodoFollowup,
            _ => Self::Ready,
        }
    }
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ProviderStats {
    pub turn_requests: u64,
    pub turn_requests_peak_min: u64,
    pub aux_requests: u64,
    pub unexpected_routes: u64,
}
#[derive(Default)]
struct Counters {
    requests: AtomicU64,
    active: AtomicU64,
    peak: AtomicU64,
    auxiliary: AtomicU64,
    unexpected: AtomicU64,
}
pub struct LoopbackProvider {
    pub base_url: String,
    counts: Arc<Counters>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for LoopbackProvider {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl LoopbackProvider {
    pub async fn start(script: impl Into<ProviderScript>) -> std::io::Result<Self> {
        let script = script.into();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://127.0.0.1:{}/v1", listener.local_addr()?.port());
        let counts = Arc::new(Counters::default());
        let shared = Arc::clone(&counts);
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break; };
                        let counts = Arc::clone(&shared);
                        connections.spawn(async move {
                            let _ = tokio::time::timeout(Duration::from_secs(90), serve(stream, script, counts)).await;
                        });
                    },
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            base_url,
            counts,
            task,
        })
    }
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn reset_peak_sync(&self) {
        self.counts
            .peak
            .store(self.counts.active.load(Ordering::SeqCst), Ordering::SeqCst);
    }
    pub async fn reset_peak(&self) {
        self.reset_peak_sync();
    }
    pub fn stats_sync(&self) -> ProviderStats {
        ProviderStats {
            turn_requests: self.counts.requests.load(Ordering::SeqCst),
            turn_requests_peak_min: self.counts.peak.load(Ordering::SeqCst),
            aux_requests: self.counts.auxiliary.load(Ordering::SeqCst),
            unexpected_routes: self.counts.unexpected.load(Ordering::SeqCst),
        }
    }
    pub async fn stats(&self) -> ProviderStats {
        self.stats_sync()
    }
}
struct InFlight(Arc<Counters>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}
async fn serve(
    stream: TcpStream,
    script: ProviderScript,
    counts: Arc<Counters>,
) -> std::io::Result<()> {
    let mut stream = BufReader::new(stream);
    let mut first = String::new();
    stream.read_line(&mut first).await?;
    let valid_route = first
        .split_whitespace()
        .take(2)
        .eq(["POST", "/v1/chat/completions"]);
    let mut len = 0;
    let mut chunked = false;
    let mut header_bytes = first.len();
    loop {
        let mut line = String::new();
        stream.read_line(&mut line).await?;
        header_bytes += line.len();
        if header_bytes > 65536 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| std::io::ErrorKind::InvalidData)?;
            }
            if k.eq_ignore_ascii_case("transfer-encoding") {
                chunked = v.trim().eq_ignore_ascii_case("chunked");
            }
        }
    }
    if !valid_route {
        counts.unexpected.fetch_add(1, Ordering::SeqCst);
        stream
            .get_mut()
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let mut size = String::new();
            stream.read_line(&mut size).await?;
            let n = usize::from_str_radix(size.trim().split(';').next().unwrap_or(""), 16)
                .map_err(|_| std::io::ErrorKind::InvalidData)?;
            if n == 0 {
                break;
            }
            if body.len() + n > 8 * 1024 * 1024 {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            let old = body.len();
            body.resize(old + n, 0);
            stream.read_exact(&mut body[old..]).await?;
            let mut crlf = [0; 2];
            stream.read_exact(&mut crlf).await?;
        }
    } else {
        if len > 8 * 1024 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        body.resize(len, 0);
        stream.read_exact(&mut body).await?;
    }
    let request: Value =
        serde_json::from_slice(&body).map_err(|_| std::io::ErrorKind::InvalidData)?;
    let last = request["messages"].as_array().and_then(|a| a.last());
    let turn = request["stream"] == true
        && last.is_some_and(|m| {
            m["role"] == "tool"
                || m["role"] == "user" && m["content"].as_str().is_some_and(|s| s.contains(PROMPT))
        });
    let socket = stream.get_mut();
    if !turn {
        counts.auxiliary.fetch_add(1, Ordering::SeqCst);
        let response=json!({"id":"local","object":"chat.completion","created":1,"model":"talaria-probe-stub","choices":[{"index":0,"message":{"role":"assistant","content":"probe"},"finish_reason":"stop"}]}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await?;
        return Ok(());
    }
    counts.requests.fetch_add(1, Ordering::SeqCst);
    let active = counts.active.fetch_add(1, Ordering::SeqCst) + 1;
    counts.peak.fetch_max(active, Ordering::SeqCst);
    let _flight = InFlight(counts);
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n").await?;
    let tool = script == ProviderScript::Todo && last.is_some_and(|m| m["role"] != "tool");
    if tool {
        tokio::time::sleep(Duration::from_millis(25)).await;
        chunk(socket,json!({"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"probe-todo","type":"function","function":{"name":"todo_list","arguments":"{\"todos\":[{\"id\":\"1\",\"content\":\"probe\",\"status\":\"pending\"}]}"}}]}),Value::Null).await?;
    } else {
        let (n, delay) = if script == ProviderScript::Slow {
            (40, 500)
        } else {
            (20, 25)
        };
        for _ in 0..n {
            chunk(socket, json!({"content":"R"}), Value::Null).await?;
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
    chunk(
        socket,
        json!({}),
        json!(if tool { "tool_calls" } else { "stop" }),
    )
    .await?;
    socket.write_all(b"data: [DONE]\n\n").await?;
    socket.shutdown().await
}
async fn chunk(socket: &mut TcpStream, delta: Value, finish: Value) -> std::io::Result<()> {
    let frame=json!({"id":"local","object":"chat.completion.chunk","created":1,"model":"talaria-probe-stub","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}).to_string();
    socket
        .write_all(format!("data: {frame}\n\n").as_bytes())
        .await
}
