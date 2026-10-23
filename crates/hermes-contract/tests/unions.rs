//! Test 39. Received discriminated unions decode to the variant their discriminator names.
//!
//! Entry points are the ones live code uses. Events go through `EventFrame::from_params`.
//! Connector-policy results go through `serde_json::from_value` of the generated result
//! type. `GoalSnapshot` is also decoded directly. Round trip is a subset check: typify
//! may add a default the input did not carry.

use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::schemas::{
    Body, CanonicalCorrectionAck, CanonicalPendingPrompt, ConnectorPolicyGetResult,
    ConnectorPolicySetResult, Effective, GoalSnapshot, GoalSnapshotWaitBarrier, Owner,
    PromptResponseResult, ProviderLimit, ProviderUsageWindow, SessionAutomationParams,
};
use serde::Serialize;
use serde_json::{Value as Json, json};

fn assert_subset(input: &Json, output: &Json) {
    match (input, output) {
        (Json::Object(left), Json::Object(right)) => {
            for (key, value) in left {
                let got = right
                    .get(key)
                    .unwrap_or_else(|| panic!("missing {key} in {output}"));
                assert_subset(value, got);
            }
        }
        (Json::Array(left), Json::Array(right)) => {
            assert_eq!(left.len(), right.len(), "{input} vs {output}");
            for (left_item, right_item) in left.iter().zip(right) {
                assert_subset(left_item, right_item);
            }
        }
        _ => assert_eq!(input, output),
    }
}

fn assert_round_trip(input: &Json, value: &impl Serialize) {
    let output = serde_json::to_value(value).expect("round trip");
    assert_subset(input, &output);
}

fn get_result(effective: Json, body: Option<Json>) -> Json {
    let layers = match body {
        Some(body) => json!([{
            "kind": "org",
            "revision": "layer-rev",
            "body": body
        }]),
        None => json!([]),
    };
    json!({
        "effective": effective,
        "layers": layers
    })
}

fn unrestricted() -> Json {
    json!({"mode": "unrestricted", "version": 1, "revision": "rev", "issued_at_ms": 1})
}

fn deny_all() -> Json {
    json!({"mode": "deny-all", "version": 1, "revision": "rev", "issued_at_ms": 1})
}

fn allow_effective() -> Json {
    json!({
        "mode": "allow",
        "version": 1,
        "revision": "rev",
        "issued_at_ms": 1,
        "connectors": [],
        "tools": {}
    })
}

fn deny_effective() -> Json {
    json!({
        "mode": "deny",
        "version": 1,
        "revision": "rev",
        "issued_at_ms": 1,
        "disabled_connectors": [],
        "tools": {}
    })
}

#[test]
fn connector_policy_body_decodes_each_mode() {
    let bodies = [
        json!({"mode": "unrestricted"}),
        json!({"mode": "deny-all"}),
        json!({"mode": "allow", "connectors": [], "tools": {}}),
        json!({"mode": "deny", "disabled_connectors": [], "tools": {}}),
    ];
    for body in bodies {
        let parsed: ConnectorPolicyGetResult =
            serde_json::from_value(get_result(unrestricted(), Some(body.clone())))
                .unwrap_or_else(|error| panic!("body {body} failed: {error}"));
        let decoded = &parsed.layers[0].body;
        match body["mode"].as_str() {
            Some("unrestricted") => assert!(matches!(decoded, Body::UnrestrictedBody(_))),
            Some("deny-all") => assert!(matches!(decoded, Body::DenyAllBody(_))),
            Some("allow") => assert!(matches!(decoded, Body::AllowBody(_))),
            Some("deny") => assert!(matches!(decoded, Body::DenyBody(_))),
            other => panic!("unexpected mode {other:?}"),
        }
        assert_round_trip(&body, decoded);
    }
}

#[test]
fn connector_policy_effective_decodes_each_mode() {
    let forms = [
        unrestricted(),
        deny_all(),
        allow_effective(),
        deny_effective(),
    ];
    for effective in forms {
        let got: ConnectorPolicyGetResult =
            serde_json::from_value(get_result(effective.clone(), None))
                .unwrap_or_else(|error| panic!("get effective {effective} failed: {error}"));
        let set: ConnectorPolicySetResult = serde_json::from_value(json!({
            "effective": effective,
            "revision": "rev"
        }))
        .unwrap_or_else(|error| panic!("set effective {effective} failed: {error}"));
        assert_effective(effective["mode"].as_str(), &got.effective, &effective);
        assert_effective(effective["mode"].as_str(), &set.effective, &effective);
    }
}

fn assert_effective(mode: Option<&str>, decoded: &Effective, input: &Json) {
    match mode {
        Some("unrestricted") => assert!(matches!(decoded, Effective::Unrestricted(_))),
        Some("deny-all") => assert!(matches!(decoded, Effective::DenyAll(_))),
        Some("allow") => assert!(matches!(decoded, Effective::Allow(_))),
        Some("deny") => assert!(matches!(decoded, Effective::Deny(_))),
        other => panic!("unexpected mode {other:?}"),
    }
    assert_round_trip(input, decoded);
}

fn connection_params(owner: Json) -> Json {
    json!({
        "type": "connection.update",
        "payload": {
            "op_id": "op-1",
            "seq": 1,
            "deadline_at": 1.0,
            "settled": false,
            "targets": [],
            "owner": owner
        }
    })
}

#[test]
fn connection_update_owner_decodes_each_type() {
    let account = json!({"type": "account"});
    let session = json!({"type": "session", "session_id": "x"});
    for owner in [account, session] {
        let frame = EventFrame::from_params(&connection_params(owner.clone()))
            .unwrap_or_else(|error| panic!("owner {owner} failed: {error:?}"));
        match &frame.notification {
            Notification::ConnectionUpdate(payload) => {
                match owner["type"].as_str() {
                    Some("account") => assert!(matches!(payload.owner, Owner::AccountOwner(_))),
                    Some("session") => {
                        assert!(matches!(payload.owner, Owner::SessionOwner(_)));
                        let raw = serde_json::to_value(&payload.owner).expect("owner");
                        assert_eq!(raw["session_id"], "x");
                    }
                    other => panic!("unexpected owner type {other:?}"),
                }
                assert_round_trip(&owner, &payload.owner);
            }
            other => panic!("expected ConnectionUpdate, got {other:?}"),
        }
    }
}

fn goal(barrier: Json) -> Json {
    json!({
        "title": "goal",
        "status": "active",
        "turns_used": 0,
        "max_turns": 1,
        "contract": {},
        "subgoals": [],
        "gates": [],
        "wait_barrier": barrier
    })
}

fn control_params(barrier: Json) -> Json {
    json!({
        "type": "session.control.update",
        "payload": {
            "control": {
                "goal": goal(barrier),
                "heartbeat": null,
                "loop": null,
                "revision": "",
                "updated_at": 0
            }
        }
    })
}

fn assert_barrier(barrier: &Json, decoded: &GoalSnapshotWaitBarrier) {
    match barrier["type"].as_str() {
        Some("until") => assert!(matches!(decoded, GoalSnapshotWaitBarrier::Until(_))),
        Some("session" | "pid") => assert!(matches!(decoded, GoalSnapshotWaitBarrier::Target(_))),
        other => panic!("unexpected barrier {other:?}"),
    }
    assert_round_trip(barrier, decoded);
}

#[test]
fn goal_wait_barrier_decodes_each_type() {
    let barriers = [
        json!({"type": "until", "until_at": 1.5}),
        json!({"type": "session", "target": "child"}),
        json!({"type": "pid", "target": 7}),
    ];
    for barrier in barriers {
        let direct: GoalSnapshot = serde_json::from_value(goal(barrier.clone()))
            .unwrap_or_else(|error| panic!("GoalSnapshot {barrier} failed: {error}"));
        let decoded = direct.wait_barrier.as_ref().expect("wait_barrier");
        assert_barrier(&barrier, decoded);

        let frame = EventFrame::from_params(&control_params(barrier.clone()))
            .unwrap_or_else(|error| panic!("session.control.update {barrier} failed: {error:?}"));
        match &frame.notification {
            Notification::SessionControlUpdate(payload) => {
                let nested = payload
                    .control
                    .goal
                    .as_ref()
                    .and_then(|item| item.wait_barrier.as_ref())
                    .expect("event wait_barrier");
                assert_barrier(&barrier, nested);
            }
            other => panic!("expected SessionControlUpdate, got {other:?}"),
        }
    }
}

/// Inline enums whose titles collide across schemas must still decode every value the
/// contract lists, so each field is decoded from its generated schema type.
#[test]
fn inline_enums_with_colliding_titles_decode_all_contract_values() {
    for value in ["queued", "redirected", "rejected"] {
        let input = json!({
            "status": value,
            "text": "t",
            "execution_generation": 1,
            "authority_epoch": 1
        });
        let decoded: CanonicalCorrectionAck = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("CanonicalCorrectionAck.status {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
    for value in ["already_resolved", "resolved"] {
        let input = json!({"status": value, "prompt_id": "p"});
        let decoded: PromptResponseResult = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("PromptResponseResult.status {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
    for value in ["account", "models"] {
        let input = json!({"scope": value});
        let decoded: ProviderLimit = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("ProviderLimit.scope {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
    for value in ["account", "model"] {
        let input = json!({"label": "l", "used_percent": 1.5, "scope": value});
        let decoded: ProviderUsageWindow = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("ProviderUsageWindow.scope {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
    for value in ["approval", "clarify"] {
        let input = json!({
            "kind": value,
            "prompt_id": "p",
            "execution_generation": 1,
            "choices": []
        });
        let decoded: CanonicalPendingPrompt = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("CanonicalPendingPrompt.kind {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
    for value in ["goal", "heartbeat", "loop"] {
        let input = json!({"session_id": "s", "kind": value, "call": "c"});
        let decoded: SessionAutomationParams = serde_json::from_value(input.clone())
            .unwrap_or_else(|error| panic!("SessionAutomationParams.kind {value}: {error}"));
        assert_round_trip(&input, &decoded);
    }
}
