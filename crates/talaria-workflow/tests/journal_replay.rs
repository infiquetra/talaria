#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use std::sync::Arc;
use support::harness::*;
use talaria_workflow::{
    engine::{self, EngineEvent, MapResolver},
    journal::{JournaledBackend, NoGate, RetryPolicy, RunJournal},
};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn lost_delivery_event_is_harmless() {
    let script = source(
        "const a=await agent('one',{label:'one'}); const b=await agent('two',{label:'two'}); return [a,b];",
    );
    let harness = Harness::new(&script);
    let (journal, _, _) = RunJournal::open(harness.dir.clone(), &script)
        .await
        .unwrap();
    let journal = Arc::new(journal);
    let config = journal.engine_config(&harness.resolver).unwrap();
    let backend = Arc::new(JournaledBackend::new(
        journal.clone(),
        harness.gateway.clone(),
        TestPolicy,
        harness.resolver.clone(),
        NoGate,
        RetryPolicy::default(),
    ));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut run = Box::pin(engine::run(
        engine::parse_script(&script).unwrap(),
        config,
        backend.clone(),
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    ));
    let mut delivery_held = false;
    loop {
        tokio::select! {
            outcome=&mut run=>panic!("run ended before the barrier: {outcome:?}"),
            event=rx.recv()=> {let event=event.unwrap();
                if matches!(event,EngineEvent::AgentFinished{completion_seq:Some(_),..}) {delivery_held=true;continue;}
                if delivery_held && matches!(&event,EngineEvent::AgentStarted{request,..} if request.label=="two") {break;}
                if !delivery_held {journal.record_engine_event(&event).await.unwrap();}
            }
        }
    }
    assert_eq!(harness.gateway.stats().sessions, 1);
    let records = std::fs::read_to_string(harness.path().join("journal.jsonl")).unwrap();
    assert!(records.contains("call_finished"));
    assert!(!records.contains("\"label\":\"two\""));
    drop(run);
    drop(backend);
    drop(journal);
    let outcome = harness.run(&script).await;
    assert_eq!(result(outcome.outcome), json!(["ok", "ok"]));
    let stats = harness.gateway.stats();
    assert_eq!((stats.sessions, stats.admissions), (2, 2));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn changed_launch_runs_again() {
    let script =
        source("return await parallel([()=>agent('one',{tier:'selected'}),()=>agent('two')]);");
    let mut harness = Harness::new(&script);
    harness
        .resolver
        .tiers
        .insert("selected".into(), "model-a".into());
    harness.run(&script).await;
    let before = harness.gateway.stats();
    harness
        .resolver
        .tiers
        .insert("selected".into(), "model-b".into());
    let (journal, summary, _) = RunJournal::open(harness.dir.clone(), &script)
        .await
        .unwrap();
    assert!(summary.orphans.is_empty());
    let replay = journal
        .engine_config(&harness.resolver)
        .unwrap()
        .replay
        .unwrap();
    assert_eq!(replay.entries.len(), 1);
    drop(journal);
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome), json!(["ok", "ok"]));
    assert_eq!(run.summary.orphans.len(), 1);
    assert_eq!(harness.gateway.stats().sessions, before.sessions + 1);
}
