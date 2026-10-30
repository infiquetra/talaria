//! Agent subprocess ownership and protocol transport.

mod client;
mod config;
mod service;

pub use client::replay_recording;
pub use config::{AcpAgentEntry, AcpConfig};
pub use service::{AcpError, AcpService, PermissionResponder, ProbeReport, probe_agent};
