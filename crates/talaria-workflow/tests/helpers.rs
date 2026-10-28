#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::engine::MapResolver;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn now_and_random_are_deterministic() {
    let body = "let error; try {random()} catch(e) {error=e.name} return [now(),random('a'),random('a'),random('b'),error];";
    let (first, _) = run(body, default_backend()).await;
    let first = result(first);
    let (second, _) = run(body, default_backend()).await;
    assert_eq!(result(second), first);
    assert_eq!(first[0], json!(1234));
    assert_eq!(first[1], first[2]);
    assert_ne!(first[1], first[3]);
    let number = first[1].as_f64().unwrap();
    assert!((0.0..1.0).contains(&number));
    assert_eq!(number, 0.7652683343944517);
    assert_eq!(first[4], "WorkflowOptionError");
    let mut cfg = config();
    cfg.seed = 8;
    let (other, _) = execute(
        &source(body),
        cfg,
        default_backend(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_ne!(result(other)[1], first[1]);
}
