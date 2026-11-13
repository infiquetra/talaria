//! Exact headless wire frames and the headless session plan.

use std::collections::BTreeSet;

use serde_json::{Value, json};
use talaria_app::headless::wire::{
    HeadlessStartError, ONESHOT_SOURCE, approval_deny_params, cancel_params, clarify_skip_params,
    interrupt_params, plan_headless, receipt_params, submit_params_finite,
};
use talaria_app::launch::{Launch, StartMode};

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn params_exact_keys() {
    let submit = submit_params_finite("local-session-1", "hello", "talaria-0123456789abcdef-1");
    assert_eq!(
        keys(&submit),
        BTreeSet::from(["session_id", "text", "submission_id", "finite"])
    );
    assert_eq!(submit["finite"], json!(true));
    assert!(submit.get("profile").is_none());
    assert!(submit.get("input_id").is_none());
    assert!(submit.get("unattended").is_none());
    let receipt = receipt_params("local-session-1", "adm-1");
    assert_eq!(
        keys(&receipt),
        BTreeSet::from(["session_id", "admission_id", "include_result"])
    );
    assert_eq!(receipt["include_result"], json!(true));
    let cancel = cancel_params("local-session-1", "adm-1");
    assert_eq!(
        keys(&cancel),
        BTreeSet::from(["session_id", "admission_id"])
    );
    let interrupt = interrupt_params("local-session-1", 7);
    assert_eq!(
        keys(&interrupt),
        BTreeSet::from(["session_id", "execution_generation"])
    );
    let deny = approval_deny_params("local-session-1", 7, "p-1");
    assert_eq!(
        keys(&deny),
        BTreeSet::from(["session_id", "execution_generation", "prompt_id", "choice"])
    );
    assert_eq!(deny["choice"], json!("deny"));
    let skip = clarify_skip_params("local-session-1", 7, "c-1");
    assert_eq!(
        keys(&skip),
        BTreeSet::from(["session_id", "execution_generation", "prompt_id", "answer"])
    );
    assert_eq!(skip["answer"], json!(""));
}

#[test]
fn describe_without_oneshot_is_not_started() {
    use hermes_contract::schemas::{RuntimeDescribeResult, SessionCreateDescriptor};
    use std::path::PathBuf;

    assert_eq!(ONESHOT_SOURCE, "oneshot");
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_string(),
        folder: PathBuf::from("/work/proj"),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let describe = RuntimeDescribeResult {
        authority_epoch: 1,
        capabilities: Vec::new(),
        instance_id: "instance".to_string(),
        profile_id: "fixture".to_string(),
        session_create: SessionCreateDescriptor {
            parameters: vec!["cwd".to_string()],
            sources: vec!["cli".to_string(), "tui".to_string()],
        },
        skin: None,
    };
    let error = plan_headless(
        &launch,
        "0123456789abcdef0123456789abcdef".to_string(),
        &describe,
    )
    .expect_err("oneshot is missing");
    assert!(matches!(error, HeadlessStartError::OneshotMissing));
}
