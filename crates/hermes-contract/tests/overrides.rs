//! Test 37. Canonical prompt.submit needs a submission id and must not be unattended.

use hermes_contract::methods::canonical::prompt_submit::Params;
use hermes_contract::overrides::{check_prompt_submit, prompt_submit_params};

#[test]
fn prompt_submit_requires_submission_or_input_id() {
    let mut missing: Params = prompt_submit_params("session", "hello", "sub-1");
    missing.submission_id = None;
    missing.input_id = None;
    assert!(check_prompt_submit(&missing).is_err());

    missing.submission_id = Some(String::new());
    assert!(check_prompt_submit(&missing).is_err());

    missing.input_id = Some("input-1".to_string());
    missing.submission_id = None;
    assert!(check_prompt_submit(&missing).is_ok());

    missing.input_id = Some(String::new());
    missing.submission_id = Some("sub-1".to_string());
    assert!(check_prompt_submit(&missing).is_ok());
}

#[test]
fn constructor_sets_submission_id_and_never_unattended() {
    let params = prompt_submit_params("session", "hello", "sub-1");
    let value = serde_json::to_value(&params).expect("params serialise");
    assert_eq!(value["submission_id"], "sub-1");
    match value.get("unattended") {
        None => {}
        Some(flag) => assert_eq!(flag, &serde_json::json!(false)),
    }
    let mut unattended = value;
    unattended["unattended"] = serde_json::json!(true);
    let bad: Params = serde_json::from_value(unattended).expect("unattended true still decodes");
    assert!(check_prompt_submit(&bad).is_err());
}

#[test]
fn transcript_rows_read_archived_truncated_as_a_boolean_or_absent() {
    use hermes_contract::overrides::{TranscriptMessageOverride, transcript_archived_truncated};
    use hermes_contract::schemas::TranscriptMessage;

    let cases = [
        (Some(serde_json::json!(true)), Some(true)),
        (Some(serde_json::json!(false)), Some(false)),
        (None, None),
        (Some(serde_json::json!("yes")), None),
        (Some(serde_json::json!(1)), None),
        (Some(serde_json::Value::Null), None),
    ];
    for (flag, expected) in cases {
        let mut row = serde_json::json!({"role": "user", "text": "hello"});
        if let Some(flag) = &flag {
            row["archived_truncated"] = flag.clone();
        }
        let message: TranscriptMessage =
            serde_json::from_value(row.clone()).expect("transcript row decodes");
        assert_eq!(message.archived_truncated, expected, "{flag:?}");
        assert_eq!(
            transcript_archived_truncated(&message),
            expected,
            "{flag:?}"
        );
        assert_eq!(message.role, "user");
        let fields: TranscriptMessageOverride =
            serde_json::from_value(row).expect("override fields decode");
        assert_eq!(fields.archived_truncated, expected, "{flag:?}");
    }
}

#[test]
fn archived_truncated_is_written_only_when_present() {
    use hermes_contract::overrides::TranscriptMessageOverride;
    use hermes_contract::schemas::TranscriptMessage;

    let message = TranscriptMessage::try_from(
        TranscriptMessage::builder()
            .role("assistant")
            .archived_truncated(Some(true)),
    )
    .expect("builder supplies every required field");
    let value = serde_json::to_value(&message).expect("row serialises");
    assert_eq!(value["archived_truncated"], serde_json::json!(true));
    let back: TranscriptMessage = serde_json::from_value(value).expect("row round-trips");
    assert_eq!(back, message);

    let plain = serde_json::to_value(TranscriptMessageOverride::default()).expect("serialises");
    assert_eq!(plain, serde_json::json!({}));
}

#[test]
fn mutation_receipts_keep_recorded_projections_and_confirmation_fields() {
    use hermes_contract::methods::canonical::session_mutate;
    use serde_json::{Value, json};

    let recording = include_str!("../../../fixtures/frames/model-switch.jsonl");
    let mut receipts = 0;
    for line in recording.lines() {
        let row: Value = serde_json::from_str(line).expect("recorded row");
        let Some(result) = row.pointer("/frame/result") else {
            continue;
        };
        if result.get("operation").is_none() {
            continue;
        }
        let typed: session_mutate::Result =
            serde_json::from_value(result.clone()).expect("receipt");
        assert_eq!(
            serde_json::to_value(typed).expect("encoded receipt"),
            *result
        );
        receipts += 1;
    }
    assert_eq!(receipts, 3);

    // These are the extra fields returned by the pinned model preparation handler.
    let result = json!({
        "session_id": "session", "operation": "model", "status": "confirmation_required",
        "confirm": "token", "confirm_message": "Confirm the switch.",
        "target_model": "glm-5", "target_provider": "ollama-cloud",
        "model": "glm-5", "provider": "ollama-cloud", "effort": "high",
        "execution_generation": 4,
        "warnings": [{"kind": "cost", "title": "Cost", "message": "Confirm the switch."}],
    });
    let typed: session_mutate::Result =
        serde_json::from_value(result.clone()).expect("confirmation");
    assert_eq!(
        serde_json::to_value(typed).expect("encoded confirmation"),
        result
    );
}

#[test]
fn catalogue_refresh_is_omitted_until_requested() {
    use hermes_contract::methods::legacy::model_options;
    use serde_json::json;

    let mut params = model_options::Params {
        session_id: Some("session".into()),
        explicit_only: true,
        include_unconfigured: true,
        ..Default::default()
    };
    let normal = serde_json::to_value(&params).expect("catalogue params");
    assert_eq!(
        normal,
        json!({"session_id": "session", "explicit_only": true, "include_unconfigured": true})
    );
    params.refresh = true;
    let refreshed = serde_json::to_value(params).expect("refresh params");
    assert_eq!(refreshed["refresh"], json!(true));

    let profile = model_options::Params {
        explicit_only: true,
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(profile).expect("profile params"),
        json!({"explicit_only": true})
    );
}
