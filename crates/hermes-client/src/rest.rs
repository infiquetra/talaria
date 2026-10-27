//! One single-use credential and one loopback connection per dashboard request.

use std::{
    future::Future,
    io,
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use http_body_util::{BodyExt, Full};
use hyper::{
    Request,
    body::{Body, Bytes},
    client::conn::http1,
};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};

use crate::{Endpoint, ProfileId, Purpose, Scope, Ticket, TicketError, mint_ticket};

pub trait TicketMinter: Send + Sync {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>>;
}

pub struct NativeHttpMinter {
    home: PathBuf,
    endpoint: Arc<Endpoint>,
    profile: ProfileId,
}
impl NativeHttpMinter {
    pub fn new(home: PathBuf, endpoint: Arc<Endpoint>, profile: ProfileId) -> Self {
        Self {
            home,
            endpoint,
            profile,
        }
    }
}
impl TicketMinter for NativeHttpMinter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(mint_ticket(
            &self.home,
            &self.endpoint,
            &self.profile,
            Purpose::NativeHttp,
            Scope::profile(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Post,
    Delete,
}
impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}
#[derive(Clone, Debug)]
pub struct RestOptions {
    pub get_deadline: Duration,
    pub write_deadline: Duration,
    pub max_body: usize,
}
impl Default for RestOptions {
    fn default() -> Self {
        Self {
            get_deadline: Duration::from_secs(15),
            write_deadline: Duration::from_secs(30),
            max_body: 8 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug)]
pub struct RequestOpts {
    pub ticket: bool,
    pub deadline: Option<Duration>,
}
impl Default for RequestOpts {
    fn default() -> Self {
        Self {
            ticket: true,
            deadline: None,
        }
    }
}

pub struct RestClient {
    address: SocketAddr,
    authority: String,
    minter: Arc<dyn TicketMinter>,
    options: RestOptions,
}
impl std::fmt::Debug for RestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RestClient(<loopback>)")
    }
}
impl std::fmt::Display for RestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hermes loopback HTTP client")
    }
}

impl RestClient {
    pub fn new(
        api_origin: &str,
        minter: Arc<dyn TicketMinter>,
        options: RestOptions,
    ) -> Result<Self, RestError> {
        if api_origin.starts_with("https:") {
            return Err(RestError::TlsNotSupported);
        }
        // Parse the literal authority, without URL host normalization or DNS.
        let authority = api_origin
            .strip_prefix("http://")
            .ok_or(RestError::NotLoopback)?;
        let address: SocketAddr = authority.parse().map_err(|_| RestError::NotLoopback)?;
        if !matches!(address.ip(), std::net::IpAddr::V4(ip) if ip == std::net::Ipv4Addr::LOCALHOST)
            && !matches!(address.ip(), std::net::IpAddr::V6(ip) if ip == std::net::Ipv6Addr::LOCALHOST)
        {
            return Err(RestError::NotLoopback);
        }
        if authority != address.to_string() || address.port() == 0 {
            return Err(RestError::NotLoopback);
        }
        Ok(Self {
            address,
            authority: authority.to_owned(),
            minter,
            options,
        })
    }

    pub async fn request(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<&Value>,
        opts: RequestOpts,
    ) -> Result<Value, RestError> {
        let bytes = self
            .request_bytes(method, path_and_query, body, opts)
            .await?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| RestError::BadBody("invalid JSON".into()))
    }

    pub(crate) async fn request_bytes(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        opts: RequestOpts,
    ) -> Result<Vec<u8>, RestError> {
        validate_path(path)?;
        let body = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| RestError::BadBody("invalid request JSON".into()))?;
        if body
            .as_ref()
            .is_some_and(|bytes| bytes.len() > self.options.max_body)
        {
            return Err(RestError::BodyTooLarge);
        }
        let deadline = opts.deadline.unwrap_or(if method == Method::Get {
            self.options.get_deadline
        } else {
            self.options.write_deadline
        });
        let sent = Arc::new(AtomicBool::new(false));
        let mut ticket = if opts.ticket {
            Some(self.minter.mint().await.map_err(RestError::Ticket)?)
        } else {
            None
        };
        let operation = async {
            for attempt in 0..=1 {
                if attempt > 0 {
                    ticket = Some(self.minter.mint().await.map_err(RestError::Ticket)?);
                }
                let (status, bytes) = self
                    .attempt(method, path, body.as_deref(), ticket.as_ref(), sent.clone())
                    .await?;
                if status == 401 && opts.ticket && attempt == 0 {
                    continue;
                }
                return map_reply(
                    method,
                    path,
                    status,
                    bytes,
                    ticket.as_ref().map(Ticket::expose),
                );
            }
            Err(RestError::Unauthorized)
        };
        tokio::time::timeout(deadline, operation)
            .await
            .unwrap_or_else(|_| {
                Err(RestError::Timeout {
                    sent: sent.load(Ordering::Relaxed),
                })
            })
    }

    async fn attempt(
        &self,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
        ticket: Option<&Ticket>,
        sent: Arc<AtomicBool>,
    ) -> Result<(u16, Vec<u8>), RestError> {
        let stream = TcpStream::connect(self.address)
            .await
            .map_err(RestError::Connect)?;
        let io = TokioIo::new(CountWrites {
            stream,
            sent: sent.clone(),
        });
        let (mut sender, connection) = http1::handshake(io).await.map_err(|_| dropped(&sent))?;
        let connection = AbortConnection(tokio::spawn(async move {
            let _ = connection.await;
        }));
        let mut request = Request::builder()
            .method(method.as_str())
            .uri(path)
            .header("host", &self.authority)
            .header("accept", "application/json")
            .header("connection", "close");
        if let Some(ticket) = ticket {
            request = request.header("x-hermes-gateway-ticket", ticket.expose());
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .header("content-length", body.len());
        }
        let request = request
            .body(Full::new(Bytes::copy_from_slice(body.unwrap_or_default())))
            .map_err(|_| RestError::BadBody("invalid request".into()))?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| dropped(&sent))?;
        let status = response.status().as_u16();
        let mut incoming = response.into_body();
        if incoming.size_hint().lower() > self.options.max_body as u64 {
            return Err(RestError::BodyTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(frame) = incoming.frame().await {
            let frame = frame.map_err(|_| dropped(&sent))?;
            if let Ok(data) = frame.into_data() {
                if data.len() > self.options.max_body.saturating_sub(bytes.len()) {
                    return Err(RestError::BodyTooLarge);
                }
                bytes.extend_from_slice(&data);
            }
        }
        drop(connection);
        Ok((status, bytes))
    }

    pub async fn get_json(&self, path: &str) -> Result<Value, RestError> {
        self.request(Method::Get, path, None, RequestOpts::default())
            .await
    }
    pub async fn put_json(&self, path: &str, body: &Value) -> Result<Value, RestError> {
        self.request(Method::Put, path, Some(body), RequestOpts::default())
            .await
    }
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value, RestError> {
        self.request(Method::Post, path, Some(body), RequestOpts::default())
            .await
    }
    pub async fn delete_json(&self, path: &str) -> Result<Value, RestError> {
        self.request(Method::Delete, path, None, RequestOpts::default())
            .await
    }
    #[doc(hidden)]
    pub async fn probe_ticket_reuse(&self, path: &str) -> Result<(u16, u16), RestError> {
        validate_path(path)?;
        let sent = Arc::new(AtomicBool::new(false));
        let ticket = self.minter.mint().await.map_err(RestError::Ticket)?;
        let operation = async {
            let (first, _) = self
                .attempt(Method::Get, path, None, Some(&ticket), sent.clone())
                .await?;
            let (second, _) = self
                .attempt(Method::Get, path, None, Some(&ticket), sent.clone())
                .await?;
            Ok((first, second))
        };
        tokio::time::timeout(self.options.get_deadline, operation)
            .await
            .unwrap_or_else(|_| {
                Err(RestError::Timeout {
                    sent: sent.load(Ordering::Relaxed),
                })
            })
    }
}

fn validate_path(path: &str) -> Result<(), RestError> {
    let uri: http::Uri = path
        .parse()
        .map_err(|_| RestError::BadBody("invalid route".into()))?;
    if !path.starts_with('/')
        || path.starts_with("//")
        || uri.scheme().is_some()
        || uri.authority().is_some()
        || path.contains('#')
    {
        return Err(RestError::BadBody("invalid route".into()));
    }
    Ok(())
}
fn map_reply(
    method: Method,
    path: &str,
    status: u16,
    bytes: Vec<u8>,
    ticket: Option<&str>,
) -> Result<Vec<u8>, RestError> {
    let detail = || {
        serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
            v.get("detail").and_then(Value::as_str).map(|text| {
                let text = ticket.map_or_else(
                    || text.to_owned(),
                    |ticket| text.replace(ticket, "<redacted>"),
                );
                text.chars().take(200).collect::<String>()
            })
        })
    };
    match status {
        200..=299 => Ok(bytes),
        401 => Err(RestError::Unauthorized),
        403 => Err(RestError::Forbidden {
            detail: detail().unwrap_or_default(),
        }),
        404 => Err(RestError::NotFound {
            route: format!(
                "{} {}",
                method.as_str(),
                path.split('?').next().unwrap_or(path)
            ),
        }),
        400..=499 => Err(RestError::Refused {
            status,
            detail: detail(),
        }),
        500..=599 => Err(RestError::Server { status }),
        _ => Err(RestError::BadBody("unexpected HTTP status".into())),
    }
}
fn dropped(sent: &AtomicBool) -> RestError {
    RestError::Dropped {
        sent: sent.load(Ordering::Relaxed),
    }
}
struct AbortConnection(tokio::task::JoinHandle<()>);
impl Drop for AbortConnection {
    fn drop(&mut self) {
        self.0.abort();
    }
}
struct CountWrites {
    stream: TcpStream,
    sent: Arc<AtomicBool>,
}
impl AsyncRead for CountWrites {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buffer)
    }
}
impl AsyncWrite for CountWrites {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, bytes);
        if matches!(result, Poll::Ready(Ok(n)) if n > 0) {
            self.sent.store(true, Ordering::Relaxed);
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[derive(thiserror::Error)]
pub enum RestError {
    #[error("origin must be loopback")]
    NotLoopback,
    #[error("HTTPS is not supported")]
    TlsNotSupported,
    #[error("could not authenticate")]
    Ticket(TicketError),
    #[error("connection failed")]
    Connect(io::Error),
    #[error("request timed out")]
    Timeout { sent: bool },
    #[error("connection dropped")]
    Dropped { sent: bool },
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden { detail: String },
    #[error("route not found")]
    NotFound { route: String },
    #[error("request refused")]
    Refused { status: u16, detail: Option<String> },
    #[error("server error")]
    Server { status: u16 },
    #[error("response body too large")]
    BodyTooLarge,
    #[error("invalid response body")]
    BadBody(String),
}
impl std::fmt::Debug for RestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ticket(_) => f.write_str("Ticket(<redacted>)"),
            Self::Connect(_) => f.write_str("Connect"),
            Self::Timeout { sent } => f.debug_struct("Timeout").field("sent", sent).finish(),
            Self::Dropped { sent } => f.debug_struct("Dropped").field("sent", sent).finish(),
            Self::Forbidden { .. } => f.write_str("Forbidden(<redacted>)"),
            Self::NotFound { .. } => f.write_str("NotFound"),
            Self::Refused { status, .. } => {
                f.debug_struct("Refused").field("status", status).finish()
            }
            Self::Server { status } => f.debug_struct("Server").field("status", status).finish(),
            Self::BadBody(_) => f.write_str("BadBody"),
            _ => std::fmt::Display::fmt(self, f),
        }
    }
}
impl RestError {
    pub fn outcome_unknown(&self) -> bool {
        matches!(
            self,
            Self::Timeout { sent: true } | Self::Dropped { sent: true }
        )
    }
}
