//! Issue 230, scenarios for fences, request keys, classification and exact retry.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use talaria_core::mutation::{
    Confirmation, Fences, MutationLedger, MutationOutcome, MutationRequest, classify_error,
    classify_result,
};

fn payload(entries: &[(&str, Value)]) -> Map<String, Value> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

fn request(id: &str, operation: &str, body: Map<String, Value>) -> MutationRequest {
    MutationRequest {
        session_id: "sess".to_string(),
        request_id: id.to_string(),
        expected_revision: 12,
        expected_generation: Some(4),
        operation: operation.to_string(),
        payload: body,
    }
}

#[test]
fn adopt_is_monotonic() {
    let mut fences = Fences::default();
    fences.adopt(Some(5), Some(2));
    fences.adopt(Some(4), Some(3));
    fences.adopt(None, None);
    assert_eq!(
        (fences.revision, fences.execution_generation),
        (Some(5), Some(3))
    );
}

#[test]
fn observe_reads_info_and_updated_for_this_session_only() {
    let mut fences = Fences::default();
    let partial = json!({
        "stored_session_id": "sess",
        "pending": [],
        "authority_epoch": 1,
        "desktop_protocol": "hermes-gateway-v1",
        "running": false,
        "execution_generation": 6,
        "revision": 16
    });
    fences.observe("sess", "session.info", &partial);
    assert_eq!(fences.ready(), Some((16, 6)));

    fences.observe(
        "sess",
        "session.updated",
        &json!({"session_id": "other", "revision": 99, "execution_generation": 9}),
    );
    assert_eq!(fences.ready(), Some((16, 6)));

    fences.observe(
        "sess",
        "session.updated",
        &json!({"session_id": "sess", "revision": "9", "execution_generation": 8}),
    );
    assert_eq!(
        (fences.revision, fences.execution_generation),
        (Some(16), Some(8))
    );

    fences.observe(
        "sess",
        "session.info",
        &json!({"session_id": "sess", "model": "glm-5", "provider": "ollama-cloud"}),
    );
    assert_eq!(fences.ready(), Some((16, 8)));
}

#[test]
fn request_params_have_exact_keys() {
    let mut ledger = MutationLedger::new("L");
    {
        let fences = ledger.fences_mut("sess");
        fences.revision = Some(12);
        fences.execution_generation = Some(4);
    }
    let first = ledger
        .build("sess", "model", payload(&[("model", json!("glm-5"))]), true)
        .expect("fences are known");
    assert_eq!(first.request_id, "L-m1");
    let keys: BTreeSet<_> = first
        .to_params()
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "session_id".to_string(),
            "request_id".to_string(),
            "expected_revision".to_string(),
            "expected_generation".to_string(),
            "operation".to_string(),
            "payload".to_string(),
        ])
    );

    let second = ledger
        .build(
            "sess",
            "reasoning",
            payload(&[("effort", Value::Null)]),
            false,
        )
        .expect("fences are known");
    assert_eq!(second.request_id, "L-m2");
    let without: BTreeSet<_> = second
        .to_params()
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        without,
        BTreeSet::from([
            "session_id".to_string(),
            "request_id".to_string(),
            "expected_revision".to_string(),
            "operation".to_string(),
            "payload".to_string(),
        ])
    );
}

#[test]
fn classify_results_and_errors() {
    let committed = classify_result(&json!({
        "session_id": "sess",
        "revision": 13,
        "execution_generation": 5,
        "operation": "model",
        "model": "glm-5",
        "provider": "ollama-cloud",
        "effort": "xhigh"
    }));
    assert!(matches!(committed, MutationOutcome::Committed(_)));

    let deferred = classify_result(&json!({
        "session_id": "sess",
        "operation": "model",
        "status": "deferred",
        "model": "glm-5",
        "revision": 12,
        "execution_generation": 4
    }));
    assert!(matches!(deferred, MutationOutcome::Deferred(_)));

    let confirmation = classify_result(&json!({
        "session_id": "sess",
        "operation": "model",
        "status": "confirmation_required",
        "confirm": "tok-1",
        "target_model": "glm-5",
        "target_provider": "ollama-cloud",
        "confirm_message": "This model costs more.",
        "payload": {"model": "glm-5", "provider": "ollama-cloud", "reasoning": "xhigh"}
    }));
    match confirmation {
        MutationOutcome::NeedsConfirmation(Confirmation {
            token,
            message,
            target_model,
            target_provider,
            pick,
        }) => {
            assert_eq!(token, "tok-1");
            assert_eq!(message, "This model costs more.");
            assert_eq!(target_model, "glm-5");
            assert_eq!(target_provider.as_deref(), Some("ollama-cloud"));
            assert_eq!(pick.get("reasoning").and_then(Value::as_str), Some("xhigh"));
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }

    let preview = classify_result(&json!({
        "session_id": "sess",
        "operation": "compress",
        "status": "preview"
    }));
    assert!(matches!(
        preview,
        MutationOutcome::OtherStatus { status, .. } if status == "preview"
    ));

    let fence = classify_error(
        4001,
        "revision_conflict",
        Some(&json!({"reason": "revision_conflict", "revision": 15, "execution_generation": 6})),
    );
    assert_eq!(
        fence,
        MutationOutcome::FenceRefused {
            reason: "revision_conflict".to_string(),
            revision: Some(15),
            execution_generation: Some(6),
        }
    );

    let refused = classify_error(
        4001,
        "model_resolution_failed",
        Some(&json!({"reason": "model_resolution_failed"})),
    );
    assert!(matches!(
        refused,
        MutationOutcome::Refused { reason, rpc_code: 4001, .. } if reason == "model_resolution_failed"
    ));

    let storage = classify_error(
        5001,
        "storage_unavailable",
        Some(&json!({"reason": "storage_unavailable"})),
    );
    assert!(matches!(
        storage,
        MutationOutcome::Refused { rpc_code: 5001, .. }
    ));

    let unknown_method = classify_error(
        -32601,
        "unknown method",
        Some(&json!({"reason": "unknown_method"})),
    );
    assert!(matches!(
        unknown_method,
        MutationOutcome::Refused {
            rpc_code: -32601,
            ..
        }
    ));

    assert_eq!(
        classify_result(&Value::String("no".into())),
        MutationOutcome::Unreadable
    );
}

#[test]
fn unknown_outcome_keeps_exact_request() {
    let mut ledger = MutationLedger::new("L");
    {
        let fences = ledger.fences_mut("sess");
        fences.revision = Some(12);
        fences.execution_generation = Some(4);
    }
    let body = payload(&[("model", json!("glm-5"))]);
    let first = request("L-m1", "model", body.clone());
    ledger.settle(&first, &MutationOutcome::Unknown);

    let again = ledger
        .build("sess", "model", body, true)
        .expect("kept request");
    assert_eq!(again.request_id, "L-m1");
    assert_eq!(again.expected_revision, 12);
    assert_eq!(again.expected_generation, Some(4));

    let other = ledger
        .build(
            "sess",
            "model",
            payload(&[("model", json!("other-model"))]),
            true,
        )
        .expect("new request");
    assert_eq!(other.request_id, "L-m2");
    let reused = ledger
        .build("sess", "model", payload(&[("model", json!("glm-5"))]), true)
        .expect("the kept request was discarded");
    assert_ne!(reused.request_id, "L-m1");
}

#[test]
fn fence_retry_once_at_the_larger_fences() {
    let mut ledger = MutationLedger::new("L");
    {
        let fences = ledger.fences_mut("sess");
        fences.revision = Some(12);
        fences.execution_generation = Some(4);
    }
    let original = request("L-m1", "model", payload(&[("model", json!("glm-5"))]));
    let refusal = MutationOutcome::FenceRefused {
        reason: "revision_conflict".to_string(),
        revision: Some(15),
        execution_generation: Some(6),
    };
    let retry = ledger
        .retry_after_fence(&original, &refusal)
        .expect("one retry");
    assert_eq!(retry.expected_revision, 15);
    assert_eq!(retry.expected_generation, Some(6));
    assert_ne!(retry.request_id, original.request_id);
    assert!(ledger.retry_after_fence(&retry, &refusal).is_none());

    let mut higher = MutationLedger::new("L");
    {
        let fences = higher.fences_mut("sess");
        fences.revision = Some(17);
        fences.execution_generation = Some(7);
    }
    let held = request("L-m3", "model", payload(&[("model", json!("glm-5"))]));
    let retry = higher
        .retry_after_fence(&held, &refusal)
        .expect("retry at the held fences");
    assert_eq!(retry.expected_revision, 17);
    assert_eq!(retry.expected_generation, Some(7));
}
