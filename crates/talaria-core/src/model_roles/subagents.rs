//! Delegation patch: only the keys that changed.

use serde_json::{Map, Value, json};

use super::fallbacks::{entry_values, parse_entries};
use super::{ChainMode, RoleRoute, SubagentRoles};
use crate::models::Effort;

/// `{"delegation": {..}}` holding only the keys that differ. A route change writes `model`
/// and `provider` together. Leaving a direct endpoint writes the route again and blanks the
/// endpoint's three keys, so a later endpoint never inherits a stale protocol.
pub fn subagent_patch(before: &SubagentRoles, after: &SubagentRoles) -> Value {
    let mut delegation = Map::new();
    let leaves_endpoint = has_endpoint(before) && !has_endpoint(after);
    if before.route != after.route || leaves_endpoint {
        let (provider, model) = route_pair(&after.route);
        delegation.insert("provider".to_owned(), Value::String(provider));
        delegation.insert("model".to_owned(), Value::String(model));
    }
    if leaves_endpoint {
        for key in ["base_url", "api_key", "api_mode"] {
            delegation.insert(key.to_owned(), Value::String(String::new()));
        }
    }
    if before.effort != after.effort {
        let wire = after.effort.map_or("", Effort::as_wire);
        delegation.insert(
            "reasoning_effort".to_owned(),
            Value::String(wire.to_owned()),
        );
    }
    if before.chain != after.chain {
        delegation.insert("fallback_providers".to_owned(), chain_value(&after.chain));
    }
    json!({ "delegation": delegation })
}

pub(super) fn chain_value(chain: &ChainMode) -> Value {
    match chain {
        ChainMode::Inherit => Value::Null,
        ChainMode::Disabled => Value::Array(Vec::new()),
        ChainMode::Own(entries) => Value::Array(entry_values(entries)),
    }
}

pub(super) fn route_pair(route: &RoleRoute) -> (String, String) {
    match route {
        RoleRoute::Inherit => (String::new(), String::new()),
        RoleRoute::Pinned { provider, model } | RoleRoute::Partial { provider, model } => {
            (provider.clone(), model.clone())
        }
    }
}

pub(super) fn has_endpoint(roles: &SubagentRoles) -> bool {
    roles.direct_endpoint.is_some()
        || roles.endpoint_keys.values().any(|value| match value {
            Value::String(text) => !text.is_empty(),
            Value::Null => false,
            _ => true,
        })
}

/// Reads the `delegation` map of the raw file; absent keys take Hermes's defaults.
pub(super) fn from_raw(delegation: &Map<String, Value>) -> SubagentRoles {
    let word = |key: &str| {
        delegation
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let provider = word("provider");
    let model = word("model");
    let route = match (provider.is_empty(), model.is_empty()) {
        (true, true) => RoleRoute::Inherit,
        (false, false) => RoleRoute::Pinned { provider, model },
        _ => RoleRoute::Partial { provider, model },
    };
    let chain = match delegation.get("fallback_providers") {
        Some(Value::Array(items)) if items.is_empty() => ChainMode::Disabled,
        Some(Value::Array(items)) => ChainMode::Own(parse_entries(items)),
        _ => ChainMode::Inherit,
    };
    let base_url = word("base_url");
    let mut endpoint_keys = Map::new();
    for key in ["base_url", "api_key", "api_mode"] {
        if let Some(value) = delegation.get(key) {
            endpoint_keys.insert(key.to_owned(), value.clone());
        }
    }
    SubagentRoles {
        route,
        effort: Effort::parse(&word("reasoning_effort")),
        chain,
        direct_endpoint: (!base_url.is_empty()).then_some(base_url),
        endpoint_keys,
    }
}
