#![cfg(feature = "mock")]
mod builtins_common;
use builtins_common::{completed, message, run};
use serde_json::Value;
fn args(s: &str) -> Option<Value> {
    Some(serde_json::from_str(s).unwrap())
}
#[tokio::test(flavor = "current_thread")]
async fn nothing_readable_skips_synthesis() {
    let (out, _, backend) = run(
        "research",
        args(include_str!(
            "data/builtins/research/nothing-readable.args.json"
        )),
        include_str!("data/builtins/research/nothing-readable.mock.json"),
        true,
    )
    .await;
    let result = completed(out);
    let expected: Value = serde_json::from_str(include_str!(
        "data/builtins/research/nothing-readable.expected.json"
    ))
    .unwrap();
    assert_eq!(result, expected);
    assert_eq!(result["status"], "no-sources");
    assert!(result["answer"].is_null());
    assert_eq!(result["claims"].as_array().unwrap().len(), 0);
    assert_eq!(result["sources_read"].as_array().unwrap().len(), 1);
    assert_eq!(result["sources_read"][0]["accessible"], false);
    assert!(!backend.requests().iter().any(|r| r.label == "synthesize"));
}
#[tokio::test(flavor = "current_thread")]
async fn synthesizer_failure_fails_the_run() {
    let (out, _, backend) = run(
        "research",
        args(include_str!("data/builtins/research/synth-fails.args.json")),
        include_str!("data/builtins/research/synth-fails.mock.json"),
        true,
    )
    .await;
    assert_eq!(
        message(out),
        "research: synthesizer failed; no answer produced"
    );
    assert!(backend.requests().iter().any(|r| r.label == "synthesize"));
}
#[tokio::test(flavor = "current_thread")]
async fn unavailable_lists_only_requested_kinds() {
    let (out, _, backend) = run(
        "research",
        args(include_str!("data/builtins/research/web-off.args.json")),
        include_str!("data/builtins/research/web-off.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["unavailable"], serde_json::json!([]));
    assert!(backend.requests().iter().all(|r| {
        r.toolsets
            .as_ref()
            .is_none_or(|t| !t.iter().any(|v| v == "web"))
    }));
}
#[tokio::test(flavor = "current_thread")]
async fn code_off_starts_no_worktree_agent() {
    let (out, _, backend) = run(
        "research",
        args(include_str!("data/builtins/research/code-off.args.json")),
        include_str!("data/builtins/research/code-off.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert!(
        !value["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "code")
    );
    assert!(backend.requests().iter().all(|r| r.isolation.is_none()));
}

#[tokio::test(flavor = "current_thread")]
async fn happy_path_drops_unread_claims() {
    let (out, _, backend) = run(
        "research",
        args(include_str!("data/builtins/research/happy.args.json")),
        include_str!("data/builtins/research/happy.mock.json"),
        true,
    )
    .await;
    let expected: Value =
        serde_json::from_str(include_str!("data/builtins/research/happy.expected.json")).unwrap();
    let result = completed(out);
    assert_eq!(result, expected);
    assert_eq!(
        backend
            .requests()
            .iter()
            .filter(|r| r.label.starts_with("read:"))
            .count(),
        1
    );
    assert_eq!(
        result["removed_claims"][0]["reason"],
        "cites a source that was not read"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn web_unavailable_still_answers_from_code() {
    let params = serde_json::json!({"question":"What is present?", "maxReads":1});
    let (out, _, _) = run(
        "research",
        Some(params),
        include_str!("data/builtins/research/no-web.mock.json"),
        true,
    )
    .await;
    let result = completed(out);
    let expected: Value =
        serde_json::from_str(include_str!("data/builtins/research/no-web.expected.json")).unwrap();
    assert_eq!(result, expected);
    assert_eq!(result["unavailable"], serde_json::json!(["web"]));
    assert_eq!(result["status"], "answered");
    assert!(
        result["sources_read"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| source["kind"] == "code")
    );
}
