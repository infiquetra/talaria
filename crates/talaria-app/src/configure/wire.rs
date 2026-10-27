//! Request parameters for the configuration session.

use serde_json::Value;
use talaria_core::configure::CreateSpec;

pub fn create_params(_spec: &CreateSpec) -> Value {
    todo!()
}

pub fn submit_params(_session_id: &str, _text: &str, _submission_id: &str) -> Value {
    todo!()
}

pub fn interrupt_params(_session_id: &str, _execution_generation: i64) -> Value {
    todo!()
}

pub fn detach_params(_session_id: &str, _subscription_id: &str) -> Value {
    todo!()
}
