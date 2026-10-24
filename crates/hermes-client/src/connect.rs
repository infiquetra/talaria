//! Open the session socket and finish the opening handshake.
//!
//! TLS is refused. The socket is a plain TCP connection to the loopback origin.

use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use hermes_contract::{
    methods::{canonical::runtime_describe, legacy::client_capabilities},
    native::NativeMethod,
};
use serde_json::Value;
use tokio_tungstenite::tungstenite::{
    Error as WsError, Message,
    client::IntoClientRequest,
    error::ProtocolError,
    protocol::{WebSocketConfig, frame::coding::CloseCode},
};

use crate::discovery::{Endpoint, ProfileId};
use crate::ticket::Ticket;
use hermes_contract::schemas::{
    ClientCapabilitiesResult, GatewayReadyPayload, RuntimeDescribeResult,
};

pub type WsStreamInner = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;
pub type WsSink = SplitSink<WsStreamInner, Message>;
pub type WsStream = SplitStream<WsStreamInner>;

/// `http` becomes `ws` and gains `/api/ws`. `https` becomes `wss`.
pub fn ws_url(api_origin: &str) -> String {
    format!("{}/api/ws", api_origin.replacen("http", "ws", 1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectOptions {
    pub timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectError {
    UpgradeRejected { status: Option<u16> },
    SubprotocolMismatch,
    WrongPurpose,
    Timeout,
    TlsNotSupported,
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UpgradeRejected {
                status: Some(status),
            } => {
                write!(f, "the gateway refused the connection (HTTP {status})")
            }
            Self::UpgradeRejected { status: None } => {
                f.write_str("the gateway refused the connection")
            }
            Self::SubprotocolMismatch => {
                f.write_str("the gateway answered with a different protocol")
            }
            Self::WrongPurpose => f.write_str(
                "the ticket was made for dashboard requests, not for the session socket",
            ),
            Self::Timeout => f.write_str("the gateway did not finish the connection in time."),
            Self::TlsNotSupported => {
                f.write_str("the gateway address uses HTTPS, which Talaria does not open.")
            }
            Self::Io(_) => f.write_str("the connection to the gateway failed."),
        }
    }
}

impl ConnectError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::UpgradeRejected { .. } => {
                "The gateway refused the connection. Try again with a new ticket; if it keeps failing, run `hermes gateway status`."
            }
            Self::SubprotocolMismatch => {
                "Update or restart Hermes and Talaria so their versions match."
            }
            Self::WrongPurpose => "Report a bug.",
            Self::Timeout | Self::Io(_) => "Try again.",
            Self::TlsNotSupported => {
                "Talaria supports only a gateway on this machine over plain HTTP."
            }
        }
    }
}

pub async fn connect(endpoint: &Endpoint, ticket: Ticket) -> Result<Connection, ConnectError> {
    connect_with(endpoint, ticket, &ConnectOptions::default()).await
}

pub async fn connect_with(
    endpoint: &Endpoint,
    ticket: Ticket,
    options: &ConnectOptions,
) -> Result<Connection, ConnectError> {
    if ticket.purpose() != crate::ticket::Purpose::Interactive {
        return Err(ConnectError::WrongPurpose);
    }
    if endpoint.api_origin().starts_with("https://") {
        return Err(ConnectError::TlsNotSupported);
    }
    tokio::time::timeout(options.timeout, async {
        let authority = endpoint
            .api_origin()
            .strip_prefix("http://")
            .ok_or(ConnectError::Io(std::io::ErrorKind::InvalidInput))?;
        let address: std::net::SocketAddr = authority
            .parse()
            .map_err(|_| ConnectError::Io(std::io::ErrorKind::InvalidInput))?;
        let socket = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|err| ConnectError::Io(err.kind()))?;
        let mut request = ws_url(endpoint.api_origin())
            .into_client_request()
            .map_err(map_upgrade_error)?;
        let protocols = format!(
            "hermes-gateway-v1, hermes-gateway-ticket.{}",
            ticket.expose()
        );
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            protocols
                .parse()
                .map_err(|_| ConnectError::Io(std::io::ErrorKind::InvalidInput))?,
        );
        let config = WebSocketConfig::default()
            .max_message_size(Some(8 * 1024 * 1024))
            .max_frame_size(Some(8 * 1024 * 1024));
        let (socket, response) =
            tokio_tungstenite::client_async_with_config(request, socket, Some(config))
                .await
                .map_err(map_upgrade_error)?;
        if response
            .headers()
            .get("Sec-WebSocket-Protocol")
            .and_then(|h| h.to_str().ok())
            != Some("hermes-gateway-v1")
        {
            return Err(ConnectError::SubprotocolMismatch);
        }
        let (sink, stream) = socket.split();
        let (instance_id, profile_id) = match ticket.identity() {
            Some((instance, profile)) => (instance.to_owned(), Some(profile.clone())),
            None => (endpoint.instance_id().to_owned(), None),
        };
        Ok(Connection {
            sink,
            stream,
            instance_id,
            profile_id,
            next_request_id: 1,
            buffered: Vec::new(),
        })
    })
    .await
    .map_err(|_| ConnectError::Timeout)?
}

fn map_upgrade_error(error: WsError) -> ConnectError {
    match error {
        WsError::Http(response) => ConnectError::UpgradeRejected {
            status: Some(response.status().as_u16()),
        },
        WsError::Protocol(ProtocolError::SecWebSocketSubProtocolError(_)) => {
            ConnectError::SubprotocolMismatch
        }
        WsError::Io(err) => ConnectError::Io(err.kind()),
        _ => ConnectError::Io(std::io::ErrorKind::InvalidData),
    }
}

/// The two failure lines the example prints.
pub fn format_failure(step: &str, message: &str, next_step: &str) -> String {
    format!("ATTACH FAILED at {step}: {message}\nNext step: {next_step}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandshakeOptions {
    pub ready_timeout: Duration,
    pub call_timeout: Duration,
}

impl Default for HandshakeOptions {
    fn default() -> Self {
        Self {
            ready_timeout: Duration::from_secs(15),
            call_timeout: Duration::from_secs(15),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Handshake {
    pub ready: GatewayReadyPayload,
    pub describe: RuntimeDescribeResult,
    pub server_requests: Option<ClientCapabilitiesResult>,
    pub timings: HandshakeTimings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandshakeTimings {
    pub ready: Duration,
    pub capabilities: Duration,
    pub describe: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Closed {
    Overloaded,
    Dropped,
}

impl std::fmt::Display for Closed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overloaded => {
                f.write_str("the gateway closed the connection because it could not keep up")
            }
            Self::Dropped => f.write_str("the connection ended during the handshake"),
        }
    }
}

impl Closed {
    pub fn next_step(self) -> &'static str {
        match self {
            Self::Overloaded => "Try again; if it repeats, run `hermes gateway status`.",
            Self::Dropped => "Try again.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandshakeError {
    ReadyTimeout,
    CallTimeout { method: &'static str },
    BadReady,
    BadFrame,
    NotSessionAuthority,
    IdentityChanged,
    BadDescribe,
    DescribeFailed { code: i64, reason: Option<String> },
    Closed(Closed),
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadyTimeout => f.write_str("the gateway did not send gateway.ready in time."),
            Self::CallTimeout { method } => {
                write!(f, "the gateway did not answer {method} in time.")
            }
            Self::BadReady => {
                f.write_str("the gateway sent a gateway.ready frame Talaria could not use.")
            }
            Self::BadFrame => f.write_str("the gateway sent a frame Talaria could not read."),
            Self::NotSessionAuthority => {
                f.write_str("the gateway did not offer a session-authority connection.")
            }
            Self::IdentityChanged => {
                f.write_str("the gateway identity changed during the handshake.")
            }
            Self::BadDescribe => {
                f.write_str("the gateway sent a runtime.describe reply Talaria could not use.")
            }
            Self::DescribeFailed { .. } => f.write_str("the gateway refused runtime.describe."),
            Self::Closed(closed) => write!(f, "{closed}"),
            Self::Io(_) => f.write_str("the handshake connection failed."),
        }
    }
}

impl HandshakeError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::ReadyTimeout | Self::CallTimeout { .. } | Self::Io(_) => "Try again.",
            Self::BadReady | Self::BadFrame | Self::BadDescribe | Self::DescribeFailed { .. } => {
                "Update Hermes and try again."
            }
            Self::NotSessionAuthority => "Restart the gateway and try again.",
            Self::IdentityChanged => "Discover the gateway again.",
            Self::Closed(closed) => closed.next_step(),
        }
    }
}

enum FrameError {
    Closed(Closed),
    Io(std::io::ErrorKind),
}

/// An open session socket after the WebSocket upgrade.
///
/// Every text frame this unit sends goes through `send_text`. Every text
/// frame it reads goes through `next_text`. A recording tap for the handshake
/// frames belongs on `send_text` and `next_text`.
pub struct Connection {
    sink: WsSink,
    stream: WsStream,
    instance_id: String,
    profile_id: Option<ProfileId>,
    next_request_id: u64,
    buffered: Vec<String>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Connection")
    }
}

impl Connection {
    pub fn into_parts(self) -> ConnectionParts {
        ConnectionParts {
            sink: self.sink,
            stream: self.stream,
            instance_id: self.instance_id,
            profile_id: self.profile_id,
            next_request_id: self.next_request_id,
            buffered: self.buffered,
        }
    }

    async fn send_text(&mut self, text: String) -> Result<(), FrameError> {
        self.sink
            .send(Message::Text(text.into()))
            .await
            .map_err(frame_error)
    }

    async fn next_text(&mut self) -> Result<Option<String>, FrameError> {
        loop {
            match self.stream.next().await {
                Some(Ok(Message::Text(text))) => return Ok(Some(text.to_string())),
                Some(Ok(Message::Close(frame))) => {
                    return Err(FrameError::Closed(
                        if frame.is_some_and(|frame| frame.code == CloseCode::Error) {
                            Closed::Overloaded
                        } else {
                            Closed::Dropped
                        },
                    ));
                }
                Some(Ok(_)) => {}
                Some(Err(err)) => return Err(frame_error(err)),
                None => return Ok(None),
            }
        }
    }
}

/// What the next unit needs to keep reading the same socket.
pub struct ConnectionParts {
    pub sink: WsSink,
    pub stream: WsStream,
    pub instance_id: String,
    pub profile_id: Option<ProfileId>,
    pub next_request_id: u64,
    pub buffered: Vec<String>,
}

impl ConnectionParts {
    /// Build parts from a socket pair. The identity is empty.
    #[doc(hidden)]
    pub fn for_test(
        sink: WsSink,
        stream: WsStream,
        next_request_id: u64,
        buffered: Vec<String>,
    ) -> Self {
        Self {
            sink,
            stream,
            instance_id: String::new(),
            profile_id: None,
            next_request_id,
            buffered,
        }
    }
}

pub async fn handshake(conn: &mut Connection) -> Result<Handshake, HandshakeError> {
    handshake_with(conn, &HandshakeOptions::default()).await
}

pub async fn handshake_with(
    conn: &mut Connection,
    options: &HandshakeOptions,
) -> Result<Handshake, HandshakeError> {
    let started = tokio::time::Instant::now();
    let ready = tokio::time::timeout(options.ready_timeout, async {
        loop {
            let (raw, value) = read_frame(conn).await?;
            if value["method"] == "event" && value["params"]["type"] == "gateway.ready" {
                let ready: GatewayReadyPayload =
                    serde_json::from_value(value["params"]["payload"].clone())
                        .map_err(|_| HandshakeError::BadReady)?;
                if ready.session_authority != Some(true) {
                    return Err(HandshakeError::NotSessionAuthority);
                }
                return Ok(ready);
            }
            if value.get("method").is_some() {
                conn.buffered.push(raw);
            } else if !is_reply(&value) {
                return Err(HandshakeError::BadFrame);
            }
        }
    })
    .await
    .map_err(|_| HandshakeError::ReadyTimeout)??;
    let ready_time = started.elapsed();
    let started = tokio::time::Instant::now();
    let caps = call::<client_capabilities::Marker>(
        conn,
        1,
        client_capabilities::Params {
            server_requests: true,
        },
        options.call_timeout,
    )
    .await;
    let server_requests = match caps {
        Ok(value) if value.get("error").is_none() => {
            serde_json::from_value(value["result"].clone()).ok()
        }
        Ok(_) => None,
        Err(HandshakeError::CallTimeout { .. }) => None,
        Err(err) => return Err(err),
    };
    let capabilities_time = started.elapsed();
    let started = tokio::time::Instant::now();
    let value = call::<runtime_describe::Marker>(
        conn,
        2,
        hermes_contract::schemas::RuntimeDescribeParams(serde_json::Map::new()),
        options.call_timeout,
    )
    .await?;
    if let Some(error) = value.get("error") {
        let code = error["code"].as_i64().ok_or(HandshakeError::BadDescribe)?;
        let reason = error["data"]["reason"]
            .as_str()
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 80
                    && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .map(str::to_owned);
        return Err(HandshakeError::DescribeFailed { code, reason });
    }
    // Keep a required JSON value distinct from a missing field, which serde's
    // generic JSON value decoder otherwise accepts as null.
    if value["result"].get("session_create").is_none() {
        return Err(HandshakeError::BadDescribe);
    }
    let describe: RuntimeDescribeResult =
        serde_json::from_value(value["result"].clone()).map_err(|_| HandshakeError::BadDescribe)?;
    if describe.instance_id != conn.instance_id
        || conn
            .profile_id
            .as_ref()
            .is_some_and(|profile| profile.as_ref() != describe.profile_id)
    {
        return Err(HandshakeError::IdentityChanged);
    }
    if conn.profile_id.is_none() {
        conn.profile_id = Some(ProfileId::from(describe.profile_id.clone()));
    }
    Ok(Handshake {
        ready,
        describe,
        server_requests,
        timings: HandshakeTimings {
            ready: ready_time,
            capabilities: capabilities_time,
            describe: started.elapsed(),
        },
    })
}

fn frame_error(error: WsError) -> FrameError {
    match error {
        WsError::Io(err) => FrameError::Io(err.kind()),
        _ => FrameError::Closed(Closed::Dropped),
    }
}

impl From<FrameError> for HandshakeError {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Closed(closed) => Self::Closed(closed),
            FrameError::Io(kind) => Self::Io(kind),
        }
    }
}

async fn read_frame(conn: &mut Connection) -> Result<(String, Value), HandshakeError> {
    let raw = conn
        .next_text()
        .await?
        .ok_or(HandshakeError::Closed(Closed::Dropped))?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| HandshakeError::BadFrame)?;
    if !value.is_object() {
        return Err(HandshakeError::BadFrame);
    }
    Ok((raw, value))
}

fn is_reply(value: &Value) -> bool {
    value.get("id").is_some() && (value.get("result").is_some() || value.get("error").is_some())
}

async fn call<M: NativeMethod>(
    conn: &mut Connection,
    id: u64,
    params: M::Params,
    timeout: Duration,
) -> Result<Value, HandshakeError> {
    #[derive(serde::Serialize)]
    struct Request<P> {
        jsonrpc: &'static str,
        id: u64,
        method: &'static str,
        params: P,
    }
    tokio::time::timeout(timeout, async {
        let request = Request {
            jsonrpc: "2.0",
            id,
            method: M::NAME,
            params,
        };
        conn.next_request_id = id + 1;
        let text = serde_json::to_string(&request).map_err(|_| HandshakeError::BadFrame)?;
        conn.send_text(text).await?;
        loop {
            let (raw, value) = read_frame(conn).await?;
            if value.get("method").is_some() {
                conn.buffered.push(raw);
                continue;
            }
            if value["id"].as_u64() == Some(id) {
                // The method decides whether a matched but malformed reply is usable.
                return Ok(value);
            }
            if !is_reply(&value) {
                return Err(HandshakeError::BadFrame);
            }
        }
    })
    .await
    .map_err(|_| HandshakeError::CallTimeout { method: M::NAME })?
}
