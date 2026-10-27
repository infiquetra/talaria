//! Delegation patch: only the keys that changed.

use serde_json::Value;

use super::SubagentRoles;

pub fn subagent_patch(before: &SubagentRoles, after: &SubagentRoles) -> Value {
    let _ = (before, after);
    todo!("model roles stub")
}
