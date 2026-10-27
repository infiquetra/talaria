//! Request parameters for the configuration session.

use serde_json::{Value, json};
use talaria_core::configure::{CreateSpec, END_REASON, MAX_TURNS};

pub fn create_params(spec: &CreateSpec) -> Value {
    let mut params = json!({"request_id":spec.request_id,"toolsets":spec.toolsets,
        "hidden":true,"ignore_rules":true,"max_turns":MAX_TURNS});
    if let (Some(provider), Some(model)) = (&spec.provider, &spec.model) {
        params["provider"] = json!(provider);
        params["model"] = json!(model);
    }
    params
}

pub fn submit_params(session_id: &str, text: &str, submission_id: &str) -> Value {
    json!({"session_id":session_id,"text":text,"submission_id":submission_id})
}

pub fn interrupt_params(session_id: &str, execution_generation: i64) -> Value {
    json!({"session_id":session_id,"execution_generation":execution_generation})
}

pub fn detach_params(session_id: &str, subscription_id: &str) -> Value {
    json!({"session_id":session_id,"subscription_id":subscription_id,"end_reason":END_REASON})
}
