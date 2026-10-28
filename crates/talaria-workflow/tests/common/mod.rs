#![allow(dead_code)]
use serde_json::Value;
use std::sync::Arc;
use talaria_workflow::{
    engine::{
        self, EngineConfig, EngineEvent, MapResolver, ReplayEntry, ReplayLog, RunOutcome,
        ScriptFailure, WorkflowResolver,
    },
    mock::MockBackend,
};
use tokio_util::sync::CancellationToken;

pub const HEADER: &str = "export const meta = {name: 'fixture', description: 'fixture'};\n";
pub fn source(body: &str) -> String {
    format!("{HEADER}{body}")
}
pub fn backend(rules: &str) -> Arc<MockBackend> {
    Arc::new(MockBackend::from_json(rules).unwrap())
}
pub fn default_backend() -> Arc<MockBackend> {
    backend(r#"{"default":{"text":"ok"},"rules":[]}"#)
}
pub fn config() -> EngineConfig {
    EngineConfig::new("fixture", 1234, 7)
}
pub async fn execute(
    script: &str,
    cfg: EngineConfig,
    backend: Arc<MockBackend>,
    resolver: Arc<dyn WorkflowResolver>,
) -> (RunOutcome, Vec<EngineEvent>) {
    let parsed = engine::parse_script(script).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let outcome = engine::run(parsed, cfg, backend, resolver, tx, CancellationToken::new()).await;
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    (outcome, events)
}
pub async fn run(body: &str, backend: Arc<MockBackend>) -> (RunOutcome, Vec<EngineEvent>) {
    execute(
        &source(body),
        config(),
        backend,
        Arc::new(MapResolver::new()),
    )
    .await
}
pub fn result(outcome: RunOutcome) -> Value {
    match outcome {
        RunOutcome::Completed { result } => result,
        other => panic!("expected completed: {other:?}"),
    }
}
pub fn failure(outcome: RunOutcome) -> ScriptFailure {
    match outcome {
        RunOutcome::Failed { error } => error,
        other => panic!("expected failure: {other:?}"),
    }
}
pub fn replay(events: &[EngineEvent], backend: &MockBackend) -> ReplayLog {
    let calls = backend.calls();
    let entries = events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::AgentFinished {
                call_id,
                completion_seq: Some(_),
                ..
            } => {
                let (_, outcome) = calls
                    .iter()
                    .find(|(request, _)| request.call_id == *call_id)
                    .expect("finished call recorded");
                Some(ReplayEntry {
                    call_id: call_id.clone(),
                    outcome: outcome.clone(),
                })
            }
            _ => None,
        })
        .collect();
    ReplayLog { entries }
}
