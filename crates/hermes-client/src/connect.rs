//! Open the session socket and finish the opening handshake.
//!
//! TLS is refused. The socket is a plain TCP connection to the loopback origin.

use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use tokio_tungstenite::tungstenite::Message;

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
    let _ = api_origin;
    todo!("P1-01")
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for ConnectError {}

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
    let _ = (endpoint, ticket, options);
    todo!("P1-01")
}

/// The two failure lines the example prints.
pub fn format_failure(step: &str, message: &str, next_step: &str) -> String {
    let _ = (step, message, next_step);
    todo!("P1-01")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl std::error::Error for HandshakeError {}

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
        let _ = self;
        todo!("P1-01")
    }

    async fn send_text(&mut self, text: String) -> Result<(), FrameError> {
        let _ = text;
        todo!("P1-01")
    }

    async fn next_text(&mut self) -> Result<Option<String>, FrameError> {
        todo!("P1-01")
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
    let _ = (conn, options);
    todo!("P1-01")
}
