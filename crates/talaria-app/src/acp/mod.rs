//! Agent subprocess ownership and protocol transport.

mod client;
mod config;
pub(crate) mod manager;
mod powers;
mod service;

pub(crate) use config::parse_section;

pub use client::replay_recording;
pub use config::{AcpAgentEntry, AcpConfig};
pub use service::{AcpError, AcpService, PermissionResponder, ProbeReport, probe_agent};
