//! Hand-built parameters and reply readers for a background launch.
//!
//! Step 0 does not build a frame. The allowed keys are request id, source, cwd,
//! model, provider, reasoning, profile, session id, text, submission id, subscription
//! id, and an end reason. Nothing else is named here.

use std::time::Duration;

use serde_json::Value;

use crate::bg::machine::{CreatedSession, NewSessionSpec, SubmitReply};

/// How long the service may spend reading a summary it already holds.
pub const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `session.create` may take.
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long `prompt.submit` may take.
pub const SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long `session.detach` may take.
pub const DETACH_TIMEOUT: Duration = Duration::from_secs(2);

/// Parameters for `session.create`. `profile` is set only when the link routes one.
#[must_use]
pub fn create_params(spec: &NewSessionSpec, profile: Option<&str>) -> Value {
    let _ = (spec, profile);
    todo!("create params")
}

/// Parameters for `prompt.submit`. `profile` is set only when the link routes one.
#[must_use]
pub fn submit_params(
    session_id: &str,
    text: &str,
    submission_id: &str,
    profile: Option<&str>,
) -> Value {
    let _ = (session_id, text, submission_id, profile);
    todo!("submit params")
}

/// Parameters for `session.detach`. `profile` is set only when the link routes one.
#[must_use]
pub fn detach_params(
    session_id: &str,
    subscription_id: &str,
    end_reason: Option<&str>,
    profile: Option<&str>,
) -> Value {
    let _ = (session_id, subscription_id, end_reason, profile);
    todo!("detach params")
}

/// Read a create reply. Missing `session_id` or `subscription_id` is an error.
pub fn read_created(snapshot: &Value) -> Result<CreatedSession, String> {
    let _ = snapshot;
    todo!("read created")
}

/// Read a submit reply. Anything other than an admission or a voice stop is an error.
pub fn read_submit(reply: &Value) -> Result<SubmitReply, String> {
    let _ = reply;
    todo!("read submit")
}
