#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use std::sync::Arc;
use support::harness::*;
use talaria_workflow::{
    engine::{self, EngineEvent, MapResolver},
    journal::{
        RunJournal,
        fake_gateway::{FakeGateway, FakeOutcome},
    },
};
use tokio_util::sync::CancellationToken;

fn admitted_count(harness: &Harness) -> usize {
    harness
        .records()
        .iter()
        .filter(|line| {
            matches!(
                line.record,
                talaria_workflow::journal::Record::TurnAdmitted { .. }
            )
        })
        .count()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn finished_calls_are_not_rerun() {
    let harness = Harness::new(SCRIPT);
    let first = harness.run(SCRIPT).await;
    let first_result = result(first.outcome);
    let before = harness.gateway.stats();
    assert_eq!((before.sessions, before.admissions), (3, 3));
    let second = harness.run(SCRIPT).await;
    assert_eq!(result(second.outcome), first_result);
    let after = harness.gateway.stats();
    assert_eq!(
        (after.sessions, after.admissions, after.receipts),
        (3, 3, before.receipts)
    );
    assert_eq!(
        second
            .events
            .iter()
            .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
            .count(),
        3
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn kill_after_two_of_three() {
    let mut harness = Harness::new(SCRIPT);
    harness.gateway = FakeGateway::new().with_script(vec![
        rule("correctness", "a", 10, None),
        rule("security", "b", 20, None),
        rule("simplicity", "c", 30, Some("hold")),
    ]);
    harness
        .crash_when(SCRIPT, |events| {
            events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        EngineEvent::AgentFinished {
                            completion_seq: Some(_),
                            ..
                        }
                    )
                })
                .count()
                >= 2
                && harness.gateway.stats().admissions == 3
                && admitted_count(&harness) == 3
        })
        .await;
    let before = harness.gateway.stats();
    assert_eq!((before.sessions, before.admissions), (3, 3));
    harness.gateway.release("hold");
    let resumed = harness.run(SCRIPT).await;
    assert_eq!(result(resumed.outcome), json!(["a", "b", "c"]));
    assert_eq!(
        resumed
            .events
            .iter()
            .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
            .count(),
        2
    );
    let after = harness.gateway.stats();
    assert_eq!((after.sessions, after.admissions), (3, 3));
    assert!(after.attaches > before.attaches);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn identical_calls_keep_their_results() {
    let script = source(
        "return await pipeline(['A','B'], async (_,item)=>{await agent('prepare '+item,{label:'prepare '+item}); return item}, async (_,item)=>[item,await agent('choose',{label:'choose '+item})]);",
    );
    for crash in [false, true] {
        let mut harness = Harness::new(&script);
        let rules = |reverse: bool| {
            vec![
                rule("prepare A", "ready", if reverse { 100 } else { 200 }, None),
                rule("prepare B", "ready", if reverse { 200 } else { 100 }, None),
                rule(
                    "choose A",
                    "Y",
                    1,
                    if crash { Some("choose-hold") } else { None },
                ),
                rule("choose B", "X", 1, None),
            ]
        };
        harness.gateway = FakeGateway::new().with_script(rules(false));
        if crash {
            harness.crash_when(&script,|events|events.iter().any(|e|matches!(e,EngineEvent::AgentStarted{request,..} if request.label=="choose A")) && events.iter().filter(|e|matches!(e,EngineEvent::AgentFinished{completion_seq:Some(_),..})).count()>=3 && harness.gateway.stats().admissions==4 && admitted_count(&harness)==4).await;
            harness.gateway = harness.gateway.clone().with_script(rules(true));
            harness.gateway.release("choose-hold");
            let resumed = harness.run(&script).await;
            assert_eq!(result(resumed.outcome), json!([["A", "Y"], ["B", "X"]]));
            let stats = harness.gateway.stats();
            assert_eq!((stats.sessions, stats.admissions), (4, 4));
            assert!(stats.attaches > 0);
        } else {
            let initial = harness.run(&script).await;
            let expected = result(initial.outcome);
            assert_eq!(expected, json!([["A", "Y"], ["B", "X"]]));
            let before = harness.gateway.stats();
            harness.gateway = harness.gateway.clone().with_script(rules(true));
            let resumed = harness.run(&script).await;
            assert_eq!(result(resumed.outcome), expected);
            let after = harness.gateway.stats();
            assert_eq!(
                (after.sessions, after.admissions),
                (before.sessions, before.admissions)
            );
        }
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn failed_calls_run_again_on_resume() {
    let script = source("return await agent('retry',{label:'retry'});");
    let mut harness = Harness::new(&script);
    let mut fail = rule("retry", "bad", 1, None);
    fail.outcome = FakeOutcome::Failed;
    harness.gateway = FakeGateway::new().with_script(vec![fail]);
    let first = harness.run(&script).await;
    assert_eq!(result(first.outcome), json!(null));
    harness.gateway = harness
        .gateway
        .clone()
        .with_script(vec![rule("retry", "ok", 1, None)]);
    let second = harness.run(&script).await;
    assert_eq!(result(second.outcome), json!("ok"));
    let stats = harness.gateway.stats();
    assert_eq!((stats.sessions, stats.admissions), (2, 2));
    let records = harness.records();
    assert!(records.iter().any(|line| matches!(
        &line.record,
        talaria_workflow::journal::Record::CallFinished {
            attempt: 0,
            outcome: talaria_workflow::journal::CallOutcome::Failed { .. },
            ..
        }
    )));
    assert!(records.iter().any(|line| matches!(&line.record,talaria_workflow::journal::Record::CallIntent{attempt:1,request_id,..} if request_id.ends_with(":a1"))));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn edited_script_reuses_unchanged_calls() {
    let original =
        source("return await parallel([()=>agent('one'),()=>agent('two'),()=>agent('three')]);");
    let edited = original.replace("agent('two')", "agent('changed')");
    let harness = Harness::new(&original);
    harness.run(&original).await;
    let before = harness.gateway.stats();
    let (journal, summary, _) = RunJournal::open(harness.dir.clone(), &edited)
        .await
        .unwrap();
    assert!(summary.orphans.is_empty());
    drop(journal);
    let second = harness.run(&edited).await;
    let after = harness.gateway.stats();
    assert_eq!(after.sessions, before.sessions + 1);
    assert_eq!(
        second
            .events
            .iter()
            .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
            .count(),
        2
    );
    assert_eq!(second.summary.orphans.len(), 1);
    let files = std::fs::read_dir(harness.path().join("scripts"))
        .unwrap()
        .count();
    assert_eq!(files, 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn engine_config_replays_clock_and_seed() {
    let script = source("return [now(),random('k')];");
    let (_temp, dir) = new_dir();
    let mut values = Vec::new();
    for _ in 0..2 {
        let (journal, _, _) = RunJournal::open(dir.clone(), &script).await.unwrap();
        let cfg = journal.engine_config(&TestResolver::default()).unwrap();
        assert_eq!((cfg.started_at_ms, cfg.seed), (1234, 7));
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mock = Arc::new(
            talaria_workflow::mock::MockBackend::from_json(
                r#"{"default":{"text":"ok"},"rules":[]}"#,
            )
            .unwrap(),
        );
        let out = engine::run(
            engine::parse_script(&script).unwrap(),
            cfg,
            mock,
            Arc::new(MapResolver::new()),
            tx,
            CancellationToken::new(),
        )
        .await;
        values.push(result(out));
        journal
            .finish(talaria_workflow::engine::RunOutcomeKind::Completed)
            .await
            .unwrap();
    }
    assert_eq!(values[0], values[1]);
    assert_eq!(values[0][0], 1234);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn settled_usage_survives_crash() {
    use talaria_workflow::journal::JournalNotice;
    let script = source(
        "return await agent('schema',{label:'schema',schema:{type:'object',properties:{x:{type:'number'}}}});",
    );
    let mut harness = Harness::new(&script);
    let mut first = rule("schema", "not json", 5, None);
    first.text_contains = Some("schema".into());
    first.usage = json!({"total_tokens":5});
    let mut correction = rule("schema", r#"{"x":1}"#, 5, Some("correction"));
    correction.text_contains = Some("Return valid JSON".into());
    correction.usage = json!({"total_tokens":7});
    harness.gateway = FakeGateway::new().with_script(vec![first, correction]);
    harness
        .crash_when(&script, |_| {
            harness.gateway.stats().admissions == 2
                && harness.records().iter().any(|line| {
                    matches!(
                        line.record,
                        talaria_workflow::journal::Record::TurnAdmitted { turn: 1, .. }
                    )
                })
        })
        .await;
    let (journal, summary, _) = RunJournal::open(harness.dir.clone(), &script)
        .await
        .unwrap();
    assert_eq!(summary.settled_usage.len(), 1);
    assert_eq!(summary.settled_usage[0].turn, 0);
    assert_eq!(
        summary.settled_usage[0].usage,
        Some(json!({"total_tokens":5}))
    );
    assert_eq!(summary.unsettled.len(), 1);
    assert_eq!(summary.unsettled[0].turn, 1);
    drop(journal);
    harness.gateway.release("correction");
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome), json!({"x":1}));
    assert_eq!(run.summary.settled_usage.len(), 2);
    let notices = run
        .notices
        .iter()
        .filter(|n| matches!(n, JournalNotice::TurnSettled { .. }))
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    let mut ids = run
        .summary
        .settled_usage
        .iter()
        .map(|usage| usage.admission_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2);
}
