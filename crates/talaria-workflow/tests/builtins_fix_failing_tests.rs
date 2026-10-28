#![cfg(feature = "mock")]
mod builtins_common;
use builtins_common::{completed, message, run};
#[tokio::test(flavor = "current_thread")]
async fn outcomes_without_fixing() {
    for (mock, status, excerpt) in [
        (
            include_str!("data/builtins/fix-failing-tests/no-failures.mock.json"),
            "no-failures",
            None,
        ),
        (
            include_str!("data/builtins/fix-failing-tests/unparsed.mock.json"),
            "unparsed-failure",
            Some("collection error"),
        ),
    ] {
        let (out, _, backend) = run("fix-failing-tests", None, mock, true).await;
        let result = completed(out);
        assert_eq!(result["status"], status);
        assert_eq!(backend.requests().len(), 1);
        assert_eq!(result["baseline"], serde_json::json!([]));
        assert_eq!(result["attempted"], serde_json::json!([]));
        assert_eq!(result["not_attempted"], serde_json::json!([]));
        if let Some(s) = excerpt {
            assert_eq!(result["excerpt"], s)
        } else {
            assert!(result["excerpt"].is_null())
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn finder_failure_fails_the_run() {
    let (out, _, backend) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/finder-fails.mock.json"),
        true,
    )
    .await;
    assert_eq!(
        message(out),
        "fix-failing-tests: could not run the tests (the finder agent failed)"
    );
    assert_eq!(backend.requests().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn cap_and_bad_commit() {
    let (out, _, backend) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/cap.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["attempted"].as_array().unwrap().len(), 5);
    assert_eq!(value["not_attempted"].as_array().unwrap().len(), 3);
    assert_eq!(
        backend
            .requests()
            .iter()
            .filter(|r| r.label.starts_with("fix "))
            .count(),
        5
    );
    assert!(
        !backend
            .requests()
            .iter()
            .any(|r| r.label.starts_with("verify "))
    );
    let (out, _, backend) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/bad-commit.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["attempted"][0]["status"], "not-fixed");
    assert_eq!(
        value["attempted"][0]["reason"],
        "fixer reported fixed without a commit"
    );
    assert!(
        !backend
            .requests()
            .iter()
            .any(|r| r.label.starts_with("verify "))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn happy_path_one_verified_one_not_fixed() {
    let (out, _, backend) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/happy.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/builtins/fix-failing-tests/happy.expected.json"
    ))
    .unwrap();
    assert_eq!(value, expected);
    assert_eq!(value["status"], "partial");
    assert_eq!(value["baseline"], serde_json::json!(["test_00", "test_01"]));
    assert_eq!(value["attempted"][0]["status"], "verified");
    assert_eq!(value["attempted"][1]["status"], "not-fixed");
    let requests = backend.requests();
    let verifier = requests
        .iter()
        .find(|r| r.label == "verify 1: test_00")
        .unwrap();
    assert!(
        verifier
            .prompt
            .contains(&format!("git merge --ff-only {}", "a".repeat(40)))
    );
    assert!(!requests.iter().any(|r| r.label.starts_with("verify 2:")));
}

#[tokio::test(flavor = "current_thread")]
async fn failed_fast_forward_is_unverified() {
    let (out, _, _) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/ff-fails.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(value["attempted"][0]["status"], "unverified");
    assert_eq!(value["attempted"][0]["reason"], "fast-forward failed");
}

#[tokio::test(flavor = "current_thread")]
async fn suite_error_with_no_parsed_failures_is_unverified() {
    let (out, _, _) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/suite-error.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    assert_eq!(
        value["attempted"][0]["reason"],
        "suite failed without naming failing tests"
    );
    assert_eq!(value["attempted"][1]["reason"], "suite did not complete");
    assert_eq!(value["attempted"][0]["status"], "unverified");
    assert_eq!(value["attempted"][1]["status"], "unverified");
}

#[tokio::test(flavor = "current_thread")]
async fn long_ids_get_distinct_labels() {
    let (out, _, backend) = run(
        "fix-failing-tests",
        None,
        include_str!("data/builtins/fix-failing-tests/long-ids.mock.json"),
        true,
    )
    .await;
    let value = completed(out);
    let labels: Vec<_> = backend
        .requests()
        .iter()
        .filter(|r| r.label.starts_with("fix "))
        .map(|r| r.label.clone())
        .collect();
    assert_eq!(labels.len(), 2);
    assert_ne!(labels[0], labels[1]);
    assert!(labels.iter().all(|s| s.chars().count() <= 100));
    assert_eq!(value["attempted"][0]["commit"], "a".repeat(40));
    assert_eq!(value["attempted"][1]["commit"], "b".repeat(40));
}
