#![cfg(feature = "mock")]
mod builtins_common;
use builtins_common::{completed, message, run};
#[tokio::test(flavor = "current_thread")]
async fn all_designers_fail() {
    let args = serde_json::from_str(include_str!(
        "data/builtins/ultracode/designers-fail.args.json"
    ))
    .unwrap();
    let (out, _, backend) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/designers-fail.mock.json"),
        true,
    )
    .await;
    assert_eq!(
        message(out),
        "ultracode: every designer failed; nothing to build"
    );
    assert!(!backend.requests().iter().any(|r| r.label == "implement"));
}
#[tokio::test(flavor = "current_thread")]
async fn null_implementer_stops_before_review() {
    let args = serde_json::from_str(include_str!(
        "data/builtins/ultracode/implementer-null.args.json"
    ))
    .unwrap();
    let (out, events, backend) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/implementer-null.mock.json"),
        true,
    )
    .await;
    let result = completed(out);
    assert_eq!(result["status"], "blocked");
    assert!(result["implementation"].is_null());
    assert!(result["review"].is_null());
    assert!(
        backend
            .requests()
            .iter()
            .all(|r| r.workflow_path.is_empty())
    );
    assert!(!events.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn happy_path_runs_review_as_child() {
    let args =
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.args.json")).unwrap();
    let (out, events, backend) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/happy.mock.json"),
        true,
    )
    .await;
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.expected.json")).unwrap();
    assert_eq!(completed(out), expected);
    let requests = backend.requests();
    assert!(requests.len() <= 23);
    let collector = requests
        .iter()
        .find(|r| r.label == "collect" && !r.workflow_path.is_empty())
        .unwrap();
    assert!(collector.prompt.contains(&"a".repeat(40)));
    assert!(collector.prompt.contains(&"b".repeat(40)));
    use sha2::{Digest, Sha256};
    let child_args = serde_json::json!({"base": "a".repeat(40), "head": "b".repeat(40)});
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&child_args).unwrap()));
    let child_path = vec![format!("name:review-branch@{}", &digest[..12])];
    assert_eq!(collector.workflow_path, child_path);
    assert!(events.iter().any(|event| matches!(event, talaria_workflow::engine::EngineEvent::ChildStarted { workflow_path, .. } if workflow_path == &child_path)));
    assert!(events.iter().any(|event| matches!(event, talaria_workflow::engine::EngineEvent::ChildFinished { workflow_path, ok: true } if workflow_path == &child_path)));
}

#[tokio::test(flavor = "current_thread")]
async fn blocked_implementation_never_calls_child() {
    let args =
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.args.json")).unwrap();
    let (out, _, backend) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/blocked.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["implementation"]["blocker"], "Cannot proceed");
    assert!(value["review"].is_null());
    assert!(
        backend
            .requests()
            .iter()
            .all(|r| r.workflow_path.is_empty())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn nonexistent_choice_fails() {
    let args =
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.args.json")).unwrap();
    let (out, _, backend) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/bad-choice.mock.json"),
        true,
    )
    .await;
    assert_eq!(
        message(out),
        "ultracode: the chooser picked proposal 99, which does not exist"
    );
    assert!(!backend.requests().iter().any(|r| r.label == "implement"));
}

#[tokio::test(flavor = "current_thread")]
async fn missing_child_reports_install_hint() {
    let args =
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.args.json")).unwrap();
    let (out, _, _) = run(
        "ultracode",
        Some(args),
        include_str!("data/builtins/ultracode/happy.mock.json"),
        false,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["status"], "implemented");
    assert!(value["review"].is_null());
    assert!(
        value["review_error"]
            .as_str()
            .unwrap()
            .ends_with("; run talaria workflow builtins install")
    );
}
