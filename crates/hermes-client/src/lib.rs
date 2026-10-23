//! Attach to the local Hermes gateway.
//!
//! This crate finds the gateway, mints one ticket, opens the session socket,
//! and finishes the opening exchange. It does not create a session.
//!
//! Consumers continue on the same socket with [`Connection::into_parts`].
//! Automated callers select [`EnsurePolicy::Never`] when discovering a gateway.

#![forbid(unsafe_code)]

pub mod connect;
pub mod discovery;
pub mod ticket;

pub use connect::{
    Closed, ConnectError, ConnectOptions, Connection, ConnectionParts, Handshake, HandshakeError,
    HandshakeOptions, HandshakeTimings, connect, connect_with, format_failure, handshake,
    handshake_with, ws_url,
};
pub use discovery::{
    DiscoverOptions, DiscoveryError, Endpoint, EnsureError, EnsurePolicy, ExitKind, GatewayState,
    NodeKind, NodeProblem, PointerProblem, ProfileId, ProfileLookupError, ReportedState,
    ServedProfile, default_home, discover, discover_with, home_hash,
};
pub use ticket::{
    Purpose, Scope, ScopeError, Ticket, TicketError, TicketOptions, mint_ticket, mint_ticket_with,
};
