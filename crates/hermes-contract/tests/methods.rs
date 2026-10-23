//! Tests 32 to 34. Method modules, shared schemas, and server requests.

use hermes_contract::events::{DecodeError, ServerRequest, ServerRequestFrame};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

fn schema_name(reference: &Json) -> String {
    reference["$ref"]
        .as_str()
        .expect("$ref")
        .trim_start_matches("#/components/schemas/")
        .to_string()
}

fn differing_shared_names() -> BTreeSet<String> {
    let document: Json =
        serde_json::from_str(include_str!("../vendor/gateway-contract.openrpc.json"))
            .expect("vendored contract");
    let mut legacy = std::collections::BTreeMap::new();
    for method in document["methods"].as_array().expect("methods") {
        let name = method["name"].as_str().expect("name").to_string();
        let pair = (
            schema_name(&method["params"][0]["schema"]),
            schema_name(&method["result"]["schema"]),
        );
        legacy.insert(name, pair);
    }
    let mut different = BTreeSet::new();
    for method in document["x-canonical-methods"]
        .as_array()
        .expect("canonical")
    {
        let name = method["name"].as_str().expect("name");
        let Some(left) = legacy.get(name) else {
            continue;
        };
        let right = (
            schema_name(&method["params"][0]["schema"]),
            schema_name(&method["result"]["schema"]),
        );
        if left != &right {
            different.insert(name.to_string());
        }
    }
    different
}

macro_rules! assert_distinct {
    ($module:ident) => {{
        let params_differ =
            std::any::type_name::<hermes_contract::methods::legacy::$module::Params>()
                != std::any::type_name::<hermes_contract::methods::canonical::$module::Params>();
        let result_differ =
            std::any::type_name::<hermes_contract::methods::legacy::$module::Result>()
                != std::any::type_name::<hermes_contract::methods::canonical::$module::Result>();
        assert!(
            params_differ || result_differ,
            "{} shares both Params and Result",
            stringify!($module)
        );
    }};
}

#[test]
fn names_and_transport_modules() {
    assert_eq!(
        hermes_contract::methods::canonical::prompt_submit::NAME,
        "prompt.submit"
    );
    assert_eq!(
        hermes_contract::methods::legacy::display_switch_sandbox_image::NAME,
        "display.switchSandboxImage"
    );
    assert_eq!(
        hermes_contract::methods::canonical::runtime_describe::NAME,
        "runtime.describe"
    );
    let generated = include_str!("../src/generated/methods.rs");
    assert_eq!(
        generated.matches("pub mod runtime_describe").count(),
        1,
        "runtime.describe exists only in the canonical catalogue"
    );
}

#[test]
fn shared_names_have_distinct_parameter_types() {
    let expected: BTreeSet<String> = [
        "approval.respond",
        "bot_relay.deliver",
        "config.get",
        "config.set",
        "groups.approve",
        "groups.create",
        "groups.retry",
        "image.attach_bytes",
        "profiles.list",
        "prompt.submit",
        "session.create",
        "session.events.since",
        "session.interrupt",
        "session.list",
        "session.redirect",
        "session.resume",
        "session.steer",
        "shell.exec",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(differing_shared_names(), expected);
    assert_distinct!(approval_respond);
    assert_distinct!(bot_relay_deliver);
    assert_distinct!(config_get);
    assert_distinct!(config_set);
    assert_distinct!(groups_approve);
    assert_distinct!(groups_create);
    assert_distinct!(groups_retry);
    assert_distinct!(image_attach_bytes);
    assert_distinct!(profiles_list);
    assert_distinct!(prompt_submit);
    assert_distinct!(session_create);
    assert_distinct!(session_events_since);
    assert_distinct!(session_interrupt);
    assert_distinct!(session_list);
    assert_distinct!(session_redirect);
    assert_distinct!(session_resume);
    assert_distinct!(session_steer);
    assert_distinct!(shell_exec);
    assert_eq!(
        std::any::type_name::<hermes_contract::methods::legacy::clarify_lock::Params>(),
        std::any::type_name::<hermes_contract::methods::canonical::clarify_lock::Params>()
    );
    assert_eq!(
        std::any::type_name::<hermes_contract::methods::legacy::clarify_lock::Result>(),
        std::any::type_name::<hermes_contract::methods::canonical::clarify_lock::Result>()
    );
}

#[test]
fn server_request_names_and_unknown_method() {
    assert_eq!(
        hermes_contract::server_requests::vault_save_login::NAME,
        "vault.save_login"
    );
    let unknown = ServerRequestFrame::from_request(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "not.a.real.method",
        "params": {"a": 1}
    }))
    .expect("an unlisted method is Unknown");
    assert!(matches!(unknown.request, ServerRequest::Unknown(_)));

    let bad = ServerRequestFrame::from_request(&json!({
        "jsonrpc": "2.0",
        "id": "req",
        "method": "vault.save_login",
        "params": {}
    }));
    let error: DecodeError = bad.expect_err("vault.save_login requires its params");
    assert!(
        error.event_type.contains("vault.save_login") || error.reason.contains("vault.save_login"),
        "{error:?}"
    );
}
