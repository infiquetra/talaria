//! Tests 22 to 30. Recorded event frames decode, or name the fields they lack.

use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::schemas::GatewayReadyPayload;
use proptest::prelude::*;
use serde_json::{Value as Json, json};

fn recorded() -> Vec<Json> {
    include_str!("fixtures/live_two_turns.frames.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame line is JSON"))
        .collect()
}

fn event_params() -> Vec<Json> {
    recorded()
        .into_iter()
        .filter(|value| value.get("method").and_then(Json::as_str) == Some("event"))
        .map(|value| value["params"].clone())
        .collect()
}

fn params_of(kind: &str) -> Vec<Json> {
    event_params()
        .into_iter()
        .filter(|params| params.get("type").and_then(Json::as_str) == Some(kind))
        .collect()
}

fn assert_known(frame: &EventFrame, kind: &str) {
    assert_eq!(frame.type_name(), Some(kind));
    assert!(
        !matches!(frame.notification, Notification::Unknown(_)),
        "{kind} decoded as Unknown"
    );
}

#[test]
fn recorded_gateway_ready_names_the_missing_fields() {
    let frames = params_of("gateway.ready");
    assert_eq!(frames.len(), 1);
    let error =
        EventFrame::from_params(&frames[0]).expect_err("recorded gateway.ready is incomplete");
    assert_eq!(error.event_type, "gateway.ready");
    assert_eq!(
        error.missing_fields,
        vec!["replay_epoch".to_string(), "skin".to_string()]
    );
}

#[test]
fn completed_gateway_ready_decodes() {
    let params: Json = serde_json::from_str(include_str!("fixtures/gateway_ready_completed.json"))
        .expect("completed fixture");
    let frame = EventFrame::from_params(&params).expect("completed gateway.ready decodes");
    assert!(matches!(frame.notification, Notification::GatewayReady(_)));
    assert_eq!(frame.type_name(), Some("gateway.ready"));
    let payload: GatewayReadyPayload =
        serde_json::from_value(params["payload"].clone()).expect("GatewayReadyPayload is public");
    assert_eq!(payload.replay_epoch, "replay-epoch-placeholder");
    assert!(payload.change_events);
}

#[test]
fn message_delta_text_and_null_text_decode() {
    let frames = params_of("message.delta");
    assert_eq!(frames.len(), 6);
    let mut saw_null = false;
    let mut saw_text = false;
    for params in &frames {
        let frame = EventFrame::from_params(params).expect("message.delta decodes");
        match &frame.notification {
            Notification::MessageDelta(payload) => {
                let text = serde_json::to_value(&payload.text).expect("text serialises");
                assert_eq!(text, params["payload"]["text"]);
                saw_null |= text.is_null();
                saw_text |= text.is_string();
            }
            other => panic!("expected MessageDelta, got {other:?}"),
        }
    }
    assert!(saw_null, "the recording has a null text");
    assert!(saw_text, "the recording has a string text");

    let mut bad = frames[0].clone();
    bad["payload"]["text"] = json!(5);
    let error = EventFrame::from_params(&bad).expect_err("text: 5 is a type error");
    assert_eq!(error.event_type, "message.delta");
    assert!(error.missing_fields.is_empty());
}

#[test]
fn payloads_with_extra_fields_decode() {
    for kind in ["tool.start", "tool.complete", "message.complete"] {
        let frames = params_of(kind);
        assert!(!frames.is_empty(), "{kind}");
        for params in frames {
            let frame = EventFrame::from_params(&params)
                .unwrap_or_else(|error| panic!("{kind} should ignore extra fields, got {error:?}"));
            assert_known(&frame, kind);
        }
    }
}

#[test]
fn approval_request_and_settled_decode_in_order() {
    let request = params_of("approval.request");
    let settled = params_of("approval.settled");
    assert_eq!(request.len(), 1);
    assert_eq!(settled.len(), 1);
    let request_frame = EventFrame::from_params(&request[0]).expect("approval.request");
    let settled_frame = EventFrame::from_params(&settled[0]).expect("approval.settled");
    assert_eq!(request_frame.execution_generation, Some(2));
    assert_eq!(settled_frame.execution_generation, Some(2));
    match &request_frame.notification {
        Notification::ApprovalRequest(payload) => {
            assert_eq!(
                payload.choices.as_deref().expect("choices"),
                ["once", "deny", "session", "always"].as_slice()
            );
            assert_eq!(payload.execution_generation, Some(2));
            assert_eq!(payload.timeout_s, Some(60.0));
            assert_eq!(payload.prompt_id, "id-0006");
            match &settled_frame.notification {
                Notification::ApprovalSettled(done) => {
                    assert_eq!(done.prompt_id, payload.prompt_id);
                    assert_eq!(done.execution_generation, Some(2));
                }
                other => panic!("expected ApprovalSettled, got {other:?}"),
            }
        }
        other => panic!("expected ApprovalRequest, got {other:?}"),
    }
}

#[test]
fn agent_step_is_unknown_and_never_an_error() {
    let frames = params_of("agent.step");
    assert_eq!(frames.len(), 4);
    for params in frames {
        let frame = EventFrame::from_params(&params).expect("agent.step is not an error");
        assert!(matches!(frame.notification, Notification::Unknown(_)));
        assert_eq!(frame.type_name(), Some("agent.step"));
    }
}

#[test]
fn known_type_with_bad_payload_is_an_error_not_unknown() {
    let mut bad = params_of("message.delta").remove(0);
    bad["payload"]["text"] = json!(5);
    let error = EventFrame::from_params(&bad).expect_err("a bad known payload is an error");
    assert_eq!(error.event_type, "message.delta");
    assert!(error.missing_fields.is_empty());
}

#[test]
fn every_recorded_event_frame_has_the_expected_outcome() {
    let expected_ok = [
        "sessions.changed",
        "platforms.changed",
        "session.info",
        "message.start",
        "thinking.delta",
        "tool.generating",
        "message.delta",
        "tool.start",
        "tool.complete",
        "session.usage",
        "message.complete",
    ];
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for params in event_params() {
        let kind = params["type"].as_str().expect("type").to_string();
        *counts.entry(kind.clone()).or_default() += 1;
        match kind.as_str() {
            "gateway.ready" => {
                let error = EventFrame::from_params(&params).expect_err("gateway.ready");
                assert!(error.missing_fields.iter().any(|field| field == "skin"));
                assert!(
                    error
                        .missing_fields
                        .iter()
                        .any(|field| field == "replay_epoch")
                );
            }
            "approval.request" => {
                let frame = EventFrame::from_params(&params).expect("approval.request");
                assert!(matches!(
                    frame.notification,
                    Notification::ApprovalRequest(_)
                ));
            }
            "approval.settled" => {
                let frame = EventFrame::from_params(&params).expect("approval.settled");
                assert!(matches!(
                    frame.notification,
                    Notification::ApprovalSettled(_)
                ));
            }
            "agent.step" => {
                let frame = EventFrame::from_params(&params).expect("agent.step");
                assert!(matches!(frame.notification, Notification::Unknown(_)));
            }
            other if expected_ok.contains(&other) => {
                let frame = EventFrame::from_params(&params)
                    .unwrap_or_else(|error| panic!("{other} should decode, got {error:?}"));
                assert_known(&frame, other);
            }
            other => panic!("unexpected recorded event {other}"),
        }
    }
    let expected_counts = [
        ("agent.step", 4),
        ("thinking.delta", 12),
        ("session.info", 10),
        ("sessions.changed", 8),
        ("message.delta", 6),
        ("tool.start", 3),
        ("tool.complete", 3),
        ("tool.generating", 3),
        ("session.usage", 4),
        ("platforms.changed", 4),
        ("message.start", 2),
        ("message.complete", 2),
        ("approval.request", 1),
        ("approval.settled", 1),
        ("gateway.ready", 1),
    ];
    for (kind, count) in expected_counts {
        assert_eq!(counts.get(kind).copied(), Some(count), "{kind}");
    }
    assert_eq!(counts.values().sum::<usize>(), 64);
}

#[test]
fn unlisted_events_decode_as_unknown() {
    for kind in ["session.updated", "session.tools_changed", "message.user"] {
        let params = json!({"type": kind, "payload": {}});
        let frame = EventFrame::from_params(&params).expect(kind);
        assert!(matches!(frame.notification, Notification::Unknown(_)));
        assert_eq!(frame.type_name(), Some(kind));
    }
}

#[test]
fn voice_interrupted_accepts_absent_null_or_empty_payload() {
    let absent = json!({"type": "voice.interrupted"});
    let null_payload = json!({"type": "voice.interrupted", "payload": null});
    let empty = json!({"type": "voice.interrupted", "payload": {}});
    for params in [absent, null_payload, empty] {
        let frame = EventFrame::from_params(&params).expect("voice.interrupted");
        assert!(matches!(frame.notification, Notification::VoiceInterrupted));
        assert_eq!(frame.type_name(), Some("voice.interrupted"));
    }
}

fn arb_json() -> impl Strategy<Value = Json> {
    let leaf = prop_oneof![
        Just(Json::Null),
        any::<bool>().prop_map(Json::Bool),
        any::<i32>().prop_map(Json::from),
        "[a-z]{0,6}".prop_map(Json::String),
    ];
    leaf.prop_recursive(4, 16, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(Json::Array),
            prop::collection::btree_map("[a-z]{1,4}", inner, 0..3)
                .prop_map(|map| { Json::Object(map.into_iter().collect()) }),
        ]
    })
}

fn notification_payload(notification: &Notification) -> Json {
    let tagged = serde_json::to_value(notification)
        .expect("Notification serialises as an externally tagged enum");
    match tagged {
        Json::Object(map) if map.len() == 1 => map.into_values().next().expect("variant payload"),
        other => other,
    }
}

fn without_key(value: &Json, key: &str) -> Json {
    let mut copy = value.clone();
    if let Some(object) = copy.as_object_mut() {
        object.remove(key);
    }
    copy
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn extra_keys_never_change_decoding(
        suffix in "[a-z0-9]{1,12}",
        extra in arb_json(),
    ) {
        let key = format!("x_talaria_extra_{suffix}");
        for params in event_params() {
            let Ok(baseline) = EventFrame::from_params(&params) else {
                continue;
            };
            let mut with_params = params.clone();
            with_params[key.as_str()] = extra.clone();
            let decoded = EventFrame::from_params(&with_params)
                .expect("an extra params key still decodes");
            assert_eq!(
                std::mem::discriminant(&decoded.notification),
                std::mem::discriminant(&baseline.notification)
            );
            let unknown = matches!(baseline.notification, Notification::Unknown(_));
            if unknown {
                assert!(matches!(decoded.notification, Notification::Unknown(_)));
            } else {
                assert_eq!(
                    without_key(&notification_payload(&decoded.notification), &key),
                    without_key(&notification_payload(&baseline.notification), &key)
                );
            }

            if let Some(object) = params.get("payload").and_then(Json::as_object) {
                let mut payload = object.clone();
                payload.insert(key.clone(), extra.clone());
                let mut with_payload = params.clone();
                with_payload["payload"] = Json::Object(payload);
                let decoded = EventFrame::from_params(&with_payload)
                    .expect("an extra payload key still decodes");
                assert_eq!(
                    std::mem::discriminant(&decoded.notification),
                    std::mem::discriminant(&baseline.notification)
                );
                if unknown {
                    assert!(matches!(decoded.notification, Notification::Unknown(_)));
                } else {
                    assert_eq!(
                        without_key(&notification_payload(&decoded.notification), &key),
                        without_key(&notification_payload(&baseline.notification), &key)
                    );
                }
            }
        }
    }
}
