//! Attach to the local Hermes gateway and exchange typed messages.
//!
//! This crate finds the gateway, mints one ticket, opens the session socket,
//! and finishes the opening exchange. It does not create a session.
//!
//! Consumers continue on the same socket with [`Connection::into_parts`].
//! Automated callers select [`EnsurePolicy::Never`] when discovering a gateway.
//! [`Client::start`] consumes the connection parts once. Calls and the unbounded
//! inbound stream then share one actor; reconnect through [`Client::replace_connection`].
//! [`liveness::Link`] manages heartbeat, reconnect, and replay on that same client.
//! On a [`liveness::LinkEvent::Gap`], rebuild the session from a fresh snapshot
//! and install its cursor with [`liveness::Link::resync`]. Replayed inbound events
//! retain the original parameter object in [`InboundEvent::raw`].

#![forbid(unsafe_code)]

pub mod channel;
pub mod config_api;
pub mod connect;
pub mod discovery;
pub mod gateway_api;
pub mod liveness;
pub mod record;
pub mod redact;
pub mod replay;
pub mod replay_source;
pub mod rest;
pub mod rpc;
pub mod ticket;

pub use channel::{
    ChannelStats, Client, ClientClosed, CloseReason, ClosedConnection, Decoded, Generation,
    Inbound, InboundEvent, InboundStream, MalformedFrame, MalformedKind, ReplyFrame, RespondError,
    Responder, ServerRequest, decode_frame, write_frame,
};
pub use connect::{
    Closed, ConnectError, ConnectOptions, Connection, ConnectionParts, Handshake, HandshakeError,
    HandshakeOptions, HandshakeTimings, connect, connect_with, format_failure, handshake,
    handshake_with, ws_url,
};
pub use discovery::{
    DiscoverOptions, DiscoveryError, Endpoint, EnsureError, EnsurePolicy, ExitKind, GatewayState,
    NodeKind, NodeProblem, PointerProblem, ProfileId, ProfileLookupError, ReadVerb, ReportedState,
    ServedProfile, control_query, default_home, discover, discover_with, home_hash,
};
pub use record::{
    FRAME_LOG_VERSION, GatewayIdentity, RecordError, RecordSummary, Recorder, RecorderOptions,
};
pub use redact::{
    REDACTED, Redacted, Redaction, Redactor, ScanHit, is_suspicious_key, local_forbidden_literals,
    redact_url, scan_for_secrets, scan_literals,
};
pub use replay_source::{
    FrameLogHeader, ReplayError, ReplayFrame, ReplayItem, ReplaySource, Speed,
};
pub use rpc::{
    APPROVAL_RESPOND_TIMEOUT, CallError, CallOptions, DEFAULT_CALL_TIMEOUT, FrameClass, RpcError,
    classify, default_timeout, encode_request,
};
pub use ticket::{
    Purpose, Scope, ScopeError, Ticket, TicketError, TicketOptions, mint_ticket, mint_ticket_with,
};
