//! Workflow agents bind to ordinary profile-scoped, durable Hermes sessions.
pub mod brief;
pub mod feed;
pub mod frames;
pub mod launch;
#[cfg(feature = "mock")]
pub mod mock_wire;
pub mod policy;
pub mod port;
pub mod usage;
pub mod validate;
pub mod wire;
pub use crate::journal::*;
pub use brief::{brief, correction_message};
pub use feed::*;
pub use frames::{CREATE_KEYS, FrameError, create_params, submit_params};
pub use launch::{ExplicitLaunch, LaunchDefaults, WorktreeProvider};
pub use policy::SchemaTurnPolicy;
pub use port::*;
use std::sync::Arc;
pub use usage::{AgentUsage, call_usage};
pub use validate::{check_schema, extract_json_candidate, validate_output};
pub use wire::{
    Cursor, EventFrame, LinkWire, LinkWireFactory, Wire, WireError, WireEvent, WireFactory,
    accept_wire_fixture,
};
pub fn backend<G: AdmissionGate>(
    journal: Arc<RunJournal>,
    port: HermesPort,
    defaults: LaunchDefaults,
    gate: G,
    retry: RetryPolicy,
) -> JournaledBackend<HermesPort, SchemaTurnPolicy, ExplicitLaunch, G> {
    JournaledBackend::new(
        journal,
        port,
        SchemaTurnPolicy,
        ExplicitLaunch::new(defaults),
        gate,
        retry,
    )
}
