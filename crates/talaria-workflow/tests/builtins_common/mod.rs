#![allow(dead_code)]
use serde_json::Value;
use std::sync::Arc;
use talaria_workflow::{
    builtins::BUILTINS,
    engine::{self, EngineConfig, EngineEvent, MapResolver, RunOutcome},
    mock::MockBackend,
};
use tokio_util::sync::CancellationToken;
pub async fn run(
    name: &str,
    args: Option<Value>,
    mock: &str,
    child: bool,
) -> (RunOutcome, Vec<EngineEvent>, Arc<MockBackend>) {
    let source = BUILTINS.iter().find(|b| b.name == name).unwrap().source;
    let review = BUILTINS
        .iter()
        .find(|b| b.name == "review-branch")
        .unwrap()
        .source;
    let resolver = if child {
        MapResolver::new().with_name("review-branch", review)
    } else {
        MapResolver::new()
    };
    let backend = Arc::new(MockBackend::from_json(mock).unwrap());
    let mut cfg = EngineConfig::new("fixture", 0, 0);
    cfg.args = args;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let outcome = engine::run(
        engine::parse_script(source).unwrap(),
        cfg,
        backend.clone(),
        Arc::new(resolver),
        tx,
        CancellationToken::new(),
    )
    .await;
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e)
    }
    (outcome, events, backend)
}
pub fn completed(value: RunOutcome) -> Value {
    match value {
        RunOutcome::Completed { result } => result,
        other => panic!("expected completed: {other:?}"),
    }
}
pub fn message(value: RunOutcome) -> String {
    match value {
        RunOutcome::Failed { error } => error.message,
        other => panic!("expected failed: {other:?}"),
    }
}
