//! Typed Hermes gateway contract. Generated modules stay private.

pub mod canonical_events;
pub mod events;
mod generated;
pub mod method;
pub mod overrides;
pub mod pinned;

pub use canonical_events::{
    ApprovalRequest, ApprovalSettled, ClarifyRequest, ClarifySettled, SessionReplayGap,
};
pub use events::{DecodeError, EventFrame, ServerRequestFrame};
pub use method::Method;
pub use overrides::{OverrideError, check_prompt_submit, prompt_submit_params};
pub use pinned::{COMMIT, TAG, live_skip_reason};

/// Native methods are the ones the canonical socket serves for a full interactive grant.
///
/// ```
/// fn needs_native<M: hermes_contract::native::NativeMethod>() {}
///
/// fn accepts_prompt_submit() {
///     needs_native::<hermes_contract::methods::canonical::prompt_submit::Marker>();
/// }
/// ```
///
/// ```compile_fail,E0277
/// fn needs_native<M: hermes_contract::native::NativeMethod>() {}
///
/// fn rejects_session_compress() {
///     needs_native::<hermes_contract::methods::legacy::session_compress::Marker>();
/// }
/// ```
pub use generated::native;
pub use generated::{methods, schemas, server_requests};
