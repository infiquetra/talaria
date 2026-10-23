//! Generated Hermes protocol types and canonical event payloads.

pub mod canonical_events;
pub mod events;
mod generated;
pub mod method;
pub mod overrides;
pub mod pinned;

pub use canonical_events::{
    ApprovalRequest, ApprovalSettled, ClarifyRequest, ClarifySettled, SessionReplayGap,
};
pub use generated::{methods, schemas, server_requests};

/// Methods that the canonical Hermes socket serves for an interactive connection.
///
/// ```
/// use hermes_contract::{methods, native::NativeMethod};
/// fn needs_native<M: NativeMethod>() {}
/// needs_native::<methods::canonical::prompt_submit::Marker>();
/// ```
///
/// ```compile_fail,E0277
/// use hermes_contract::{methods, native::NativeMethod};
/// fn needs_native<M: NativeMethod>() {}
/// needs_native::<methods::legacy::session_compress::Marker>();
/// ```
pub use generated::native;
