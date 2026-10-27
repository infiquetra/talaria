//! The context and effort patch that follows a main-model write.

use serde_json::Value;

use super::MainRoles;

pub fn main_config_patch(before: &MainRoles, after: &MainRoles) -> Option<Value> {
    let _ = (before, after);
    todo!("model roles stub")
}
