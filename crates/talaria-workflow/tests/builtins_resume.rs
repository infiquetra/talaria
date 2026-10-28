#![cfg(feature = "mock")]
mod common;
use common::{backend, config, execute, replay, result};
use std::sync::Arc;
use talaria_workflow::{builtins::BUILTINS, engine::MapResolver};
#[tokio::test(flavor = "current_thread")]
async fn replay_reuses_every_finished_review_agent() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let first = backend(include_str!("data/builtins/review-branch/happy.mock.json"));
    let (out, events) = execute(
        source,
        config(),
        first.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    let original = result(out);
    let saved = replay(&events, &first);
    assert_eq!(saved.entries.len(), first.requests().len());
    let second = backend(r#"{"default":{"failed":"must not be called"},"rules":[]}"#);
    let mut cfg = config();
    cfg.replay = Some(saved);
    let (out, _) = execute(source, cfg, second.clone(), Arc::new(MapResolver::new())).await;
    assert_eq!(result(out), original);
    assert!(second.requests().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn replay_reuses_every_finished_ultracode_agent() {
    let source = BUILTINS
        .iter()
        .find(|b| b.name == "ultracode")
        .unwrap()
        .source;
    let review = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let resolver = Arc::new(MapResolver::new().with_name("review-branch", review));
    let first = backend(include_str!("data/builtins/ultracode/happy.mock.json"));
    let mut cfg = config();
    cfg.args = Some(
        serde_json::from_str(include_str!("data/builtins/ultracode/happy.args.json")).unwrap(),
    );
    let (out, events) = execute(source, cfg.clone(), first.clone(), resolver.clone()).await;
    let original = result(out);
    let saved = replay(&events, &first);
    assert_eq!(saved.entries.len(), first.requests().len());
    assert!(first.requests().iter().any(|r| !r.workflow_path.is_empty()));
    let second = backend(r#"{"default":{"failed":"must not be called"},"rules":[]}"#);
    cfg.replay = Some(saved);
    let (out, _) = execute(source, cfg, second.clone(), resolver).await;
    assert_eq!(result(out), original);
    assert!(second.requests().is_empty());
}
