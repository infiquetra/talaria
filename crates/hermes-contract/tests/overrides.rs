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
