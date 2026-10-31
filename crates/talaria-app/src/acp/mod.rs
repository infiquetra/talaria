//! Agent subprocess ownership and protocol transport.

mod client;
mod config;
mod fixture_trace;
pub(crate) mod manager;
mod powers;
mod service;

pub(crate) use config::parse_section;

pub use client::replay_recording;
pub use config::{AcpAgentEntry, AcpConfig};
#[doc(hidden)]
pub use fixture_trace::{
    FixtureTraceGuard, event as trace_fixture_event, guard as trace_fixture_guard,
};
pub use service::{AcpError, AcpService, PermissionResponder, ProbeReport, probe_agent};
