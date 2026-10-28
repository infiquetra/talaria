#![cfg(feature = "mock")]
mod common;
use common::{backend, config, execute, failure, result};
use std::sync::Arc;
use talaria_workflow::{builtins::BUILTINS, engine::MapResolver};

#[tokio::test(flavor = "current_thread")]
async fn nothing_to_review_starts_one_agent() {
    let script = BUILTINS.iter().find(|b| b.name == "review-branch").unwrap();
    let mock = backend(include_str!("data/builtins/review-branch/empty.mock.json"));
    let expected = serde_json::from_str::<serde_json::Value>(include_str!(
        "data/builtins/review-branch/empty.expected.json"
    ))
    .unwrap();
    let (outcome, _) = execute(
        script.source,
        config(),
        mock.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), expected);
    assert_eq!(mock.requests().len(), 1);
    assert_eq!(mock.requests()[0].label, "collect");
}
#[tokio::test(flavor = "current_thread")]
async fn collector_failure_fails_the_run() {
    let script = BUILTINS.iter().find(|b| b.name == "review-branch").unwrap();
    let mock = backend(include_str!(
        "data/builtins/review-branch/collector-fails.mock.json"
    ));
    let (outcome, _) = execute(
        script.source,
        config(),
        mock.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(
        failure(outcome).message,
        "review-branch: could not collect the diff (the collector agent failed); see its session in the progress view"
    );
    assert_eq!(mock.requests().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn every_reviewer_failed_fails_the_run() {
    let script = BUILTINS.iter().find(|b| b.name == "review-branch").unwrap();
    let mock = backend(include_str!(
        "data/builtins/review-branch/all-reviewers-fail.mock.json"
    ));
    let (outcome, _) = execute(
        script.source,
        config(),
        mock.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(
        failure(outcome).message,
        "review-branch: every reviewer failed; nothing was reviewed"
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 4);
    assert!(
        !requests
            .iter()
            .any(|r| r.label.starts_with("verify:") || r.label == "synthesize")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn happy_path_three_lenses() {
    use talaria_workflow::engine::Isolation;
    let script = BUILTINS.iter().find(|b| b.name == "review-branch").unwrap();
    let mock = backend(include_str!("data/builtins/review-branch/happy.mock.json"));
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/builtins/review-branch/happy.expected.json"
    ))
    .unwrap();
    let (outcome, _) = execute(
        script.source,
        config(),
        mock.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), expected);
    let requests = mock.requests();
    assert_eq!(requests.len(), 6);
    for (label, tier, toolsets, isolation) in [
        (
            "collect",
            "survey",
            vec!["file", "terminal"],
            Some(Isolation::Worktree),
        ),
        (
            "review: correctness",
            "reviewer-1",
            vec!["file"],
            Some(Isolation::Worktree),
        ),
        (
            "review: security",
            "reviewer-2",
            vec!["file"],
            Some(Isolation::Worktree),
        ),
        (
            "review: simplicity",
            "reviewer-3",
            vec!["file"],
            Some(Isolation::Worktree),
        ),
        (
            "verify: correctness-1",
            "judge",
            vec!["file"],
            Some(Isolation::Worktree),
        ),
        ("synthesize", "judge", vec!["todo"], None),
    ] {
        let req = requests.iter().find(|r| r.label == label).unwrap();
        assert_eq!(req.tier.as_deref(), Some(tier));
        assert_eq!(
            req.toolsets
                .as_ref()
                .unwrap()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            toolsets
        );
        assert_eq!(req.isolation, isolation);
        assert!(!req.prompt.contains("not the reviewed revision"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn failed_reviewer_is_reported() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let mock = backend(include_str!(
        "data/builtins/review-branch/reviewer-fails.mock.json"
    ));
    let (outcome, _) = execute(source, config(), mock.clone(), Arc::new(MapResolver::new())).await;
    let value = result(outcome);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/builtins/review-branch/reviewer-fails.expected.json"
    ))
    .unwrap();
    assert_eq!(value, expected);
    assert_eq!(value["reviewers_failed"], serde_json::json!(["security"]));
    assert!(
        mock.requests()
            .iter()
            .any(|r| r.label == "verify: correctness-1")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn synthesizer_ids_are_checked() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    for (raw, is_null) in [
        (
            include_str!("data/builtins/review-branch/synth-ids.mock.json"),
            false,
        ),
        (
            include_str!("data/builtins/review-branch/synth-null.mock.json"),
            true,
        ),
    ] {
        let mock = backend(raw);
        let (outcome, _) = execute(source, config(), mock, Arc::new(MapResolver::new())).await;
        let value = result(outcome);
        if is_null {
            assert!(value["summary"].is_null());
            assert_eq!(
                value["ranked"],
                serde_json::json!(["correctness-1", "security-1"])
            );
            assert_eq!(value["themes"], serde_json::json!([]));
        } else {
            let expected: serde_json::Value = serde_json::from_str(include_str!(
                "data/builtins/review-branch/synth-ids.expected.json"
            ))
            .unwrap();
            assert_eq!(value, expected);
            assert_eq!(
                value["ranked"],
                serde_json::json!(["security-1", "correctness-1"])
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn diff_is_cut_by_the_script() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let mock = backend(include_str!(
        "data/builtins/review-branch/truncate.mock.json"
    ));
    let (outcome, _) = execute(source, config(), mock.clone(), Arc::new(MapResolver::new())).await;
    let value = result(outcome);
    assert_eq!(value["truncated"], true);
    for request in mock
        .requests()
        .iter()
        .filter(|r| r.label.starts_with("review:"))
    {
        assert!(!request.prompt.contains(&"x".repeat(60_001)));
        assert!(
            request
                .prompt
                .contains("The diff was cut to 60000 characters")
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn review_of_another_revision_uses_collector_context() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let mock = backend(include_str!(
        "data/builtins/review-branch/other-revision.mock.json"
    ));
    let mut cfg = config();
    cfg.args = Some(
        serde_json::from_str(include_str!(
            "data/builtins/review-branch/other-revision.args.json"
        ))
        .unwrap(),
    );
    let (outcome, _) = execute(source, cfg, mock.clone(), Arc::new(MapResolver::new())).await;
    let _ = result(outcome);
    for request in mock
        .requests()
        .iter()
        .filter(|r| r.label.starts_with("review:") || r.label == "verify: correctness-1")
    {
        assert!(request.prompt.contains("ADDED_AT_HEAD"));
        assert!(request.prompt.contains("not the reviewed revision"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn findings_are_capped_and_ordered() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let mock = backend(include_str!("data/builtins/review-branch/many.mock.json"));
    let (outcome, _) = execute(source, config(), mock.clone(), Arc::new(MapResolver::new())).await;
    let value = result(outcome);
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "data/builtins/review-branch/many.expected.json"
    ))
    .unwrap();
    assert_eq!(value, expected);
    let confirmed = value["confirmed"].as_array().unwrap();
    let deferred = value["not_verified"].as_array().unwrap();
    assert_eq!(confirmed.len(), 12);
    assert_eq!(deferred.len(), 18);
    assert_eq!(confirmed[0]["id"], "security-1");
    assert!(confirmed.iter().any(|v| v["id"] == "correctness-1"));
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.label.starts_with("verify:"))
            .count(),
        12
    );
}
