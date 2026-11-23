//! A script sees which agents Hermes stopped at their step cap, and a script that ignores the
//! mark sees the reply exactly as it would have.
use crate::common::*;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::sync::Arc;
use talaria_workflow::engine::{
    self, AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, CutShort, EngineEvent,
    MapResolver, OutcomeKind,
};
use tokio_util::sync::CancellationToken;

/// Answers every prompt in full except those named "text" and "object", which Hermes cut short.
struct Capped;
impl AgentBackend for Capped {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        let cut_short = CutShort {
            steps: Some(60),
            limit: Some(60),
        };
        Box::pin(async move {
            match request.prompt.as_str() {
                "text" => AgentOutcome::CutShort {
                    value: json!("partial"),
                    cut_short,
                },
                "object" => AgentOutcome::CutShort {
                    value: json!({"verdict": "maybe"}),
                    cut_short,
                },
                _ => AgentOutcome::Completed(json!("whole")),
            }
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
}

async fn run_script(body: &str) -> (Value, Vec<EngineEvent>) {
    let parsed = engine::parse_script(&source(body)).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let outcome = engine::run(
        parsed,
        config(),
        Arc::new(Capped),
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    )
    .await;
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    (result(outcome), events)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_script_sees_the_mark_and_ignoring_it_changes_nothing() {
    let (value, events) = run_script(
        "const t = await agent('text', {label: 'writer'});
         const o = await agent('object', {label: 'judge'});
         const w = await agent('whole', {label: 'plain'});
         return {t, w, o, json: JSON.stringify(o), keys: Object.keys(o), spread: {...o},
                 mark: o.cutShort, plainMark: typeof w, listed: cutShort()};",
    )
    .await;
    // The replies are exactly what the script would have got before the mark existed.
    assert_eq!(value["t"], "partial");
    assert_eq!(value["w"], "whole");
    assert_eq!(value["o"], json!({"verdict": "maybe"}));
    assert_eq!(value["json"], "{\"verdict\":\"maybe\"}");
    assert_eq!(value["keys"], json!(["verdict"]));
    assert_eq!(value["spread"], json!({"verdict": "maybe"}));
    // The mark is there for a script that looks.
    assert_eq!(value["mark"], json!({"steps": 60, "limit": 60}));
    assert_eq!(value["plainMark"], "string");
    assert_eq!(
        value["listed"],
        json!([
            {"label": "writer", "steps": 60, "limit": 60},
            {"label": "judge", "steps": 60, "limit": 60},
        ])
    );
    // The engine says so in the event that ends each agent.
    let marked = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                EngineEvent::AgentFinished {
                    outcome: OutcomeKind::CutShort {
                        steps: Some(60),
                        limit: Some(60)
                    },
                    ..
                }
            )
        })
        .count();
    assert_eq!(marked, 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_script_with_no_cut_short_agent_sees_an_empty_list() {
    let (value, _) = run_script("await agent('whole'); return cutShort();").await;
    assert_eq!(value, json!([]));
}
