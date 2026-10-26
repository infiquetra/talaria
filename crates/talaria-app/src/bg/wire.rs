//! Hand-built parameters and reply readers for a background launch.
//!
//! The allowed keys are request id, source, cwd,
//! model, provider, reasoning, profile, session id, text, submission id, subscription
//! id, and an end reason. Nothing else is named here.

use std::time::Duration;

use serde_json::{Map, Value, json};

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
    let mut params = Map::new();
    params.insert("request_id".to_owned(), json!(spec.request_id));
    params.insert("source".to_owned(), json!("tui"));
    params.insert("cwd".to_owned(), json!(spec.cwd.to_string_lossy()));
    insert_some(&mut params, "model", spec.model.as_deref());
    insert_some(&mut params, "provider", spec.provider.as_deref());
    insert_some(&mut params, "reasoning", spec.reasoning.as_deref());
    insert_some(&mut params, "profile", profile);
    Value::Object(params)
}

/// Parameters for `prompt.submit`. `profile` is set only when the link routes one.
#[must_use]
pub fn submit_params(
    session_id: &str,
    text: &str,
    submission_id: &str,
    profile: Option<&str>,
) -> Value {
    let mut params = Map::new();
    params.insert("session_id".to_owned(), json!(session_id));
    params.insert("text".to_owned(), json!(text));
    params.insert("submission_id".to_owned(), json!(submission_id));
    insert_some(&mut params, "profile", profile);
    Value::Object(params)
}

/// Parameters for `session.detach`. `profile` is set only when the link routes one.
#[must_use]
pub fn detach_params(
    session_id: &str,
    subscription_id: &str,
    end_reason: Option<&str>,
    profile: Option<&str>,
) -> Value {
    let mut params = Map::new();
    params.insert("session_id".to_owned(), json!(session_id));
    params.insert("subscription_id".to_owned(), json!(subscription_id));
    insert_some(&mut params, "end_reason", end_reason);
    insert_some(&mut params, "profile", profile);
    Value::Object(params)
}

/// Read a create reply. Missing `session_id` or `subscription_id` is an error.
pub fn read_created(snapshot: &Value) -> Result<CreatedSession, String> {
    let object = snapshot
        .as_object()
        .ok_or_else(|| "the create reply is not an object".to_owned())?;
    let session_id = non_empty(object.get("session_id"))
        .ok_or_else(|| "the create reply has no session_id".to_owned())?;
    let subscription_id = non_empty(object.get("subscription_id"))
        .ok_or_else(|| "the create reply has no subscription_id".to_owned())?;
    let info = object.get("info");
    Ok(CreatedSession {
        session_id,
        stored_session_id: non_empty(object.get("stored_session_id")),
        subscription_id,
        info_cwd: non_empty(info.and_then(|info| info.get("cwd"))),
        model: non_empty(info.and_then(|info| info.get("model"))),
    })
}

/// Read a submit reply. Anything other than an admission or a voice stop is an error.
pub fn read_submit(reply: &Value) -> Result<SubmitReply, String> {
    let object = reply
        .as_object()
        .ok_or_else(|| "the submit reply is not an object".to_owned())?;
    if object.get("voice_stopped") == Some(&Value::Bool(true)) {
        return Ok(SubmitReply::VoiceStopped);
    }
    match (
        non_empty(object.get("admission_id")),
        non_empty(object.get("status")),
    ) {
        (Some(admission_id), Some(status)) => Ok(SubmitReply::Admitted {
            admission_id,
            status,
        }),
        _ => Err("the submit reply is neither an admission nor a voice stop".to_owned()),
    }
}

fn insert_some(params: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        params.insert(key.to_owned(), json!(value));
    }
}

/// A string value that is present and not empty.
fn non_empty(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
