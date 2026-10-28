#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use std::time::Duration;
use support::harness::*;
use talaria_workflow::{
    engine::{EngineEvent, RunOutcome},
    journal::{
        AttentionCause, JournalNotice, NoGate, OnUnknown, PortError, Record, Resolution,
        fake_gateway::FakeGateway,
    },
};

async fn interrupted(script: &str, policy: OnUnknown) -> Harness {
    let mut data = manifest();
    data.on_unknown = policy;
    let mut harness = Harness::new_with(script, data);
    harness.gateway = FakeGateway::new().with_script(vec![
        rule("slow", "ok", 100, Some("hold")),
        rule("fast", "fast", 10, None),
    ]);
    harness
        .crash_when(script, |events| {
            events
                .iter()
                .any(|e| matches!(e,EngineEvent::AgentStarted{request,..} if request.label=="slow"))
                && harness.gateway.stats().admissions >= if script.contains("fast") { 2 } else { 1 }
                && (!script.contains("fast")
                    || events.iter().any(|event| {
                        matches!(
                            event,
                            EngineEvent::AgentFinished {
                                completion_seq: Some(_),
                                ..
                            }
                        )
                    }))
        })
        .await;
    tokio::time::advance(Duration::from_millis(5)).await;
    tokio::task::yield_now().await;
    harness.gateway.restart();
    harness.gateway.release("hold");
    harness
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn gateway_restart_parks_never_reruns() {
    let script = source(
        "return await parallel([()=>agent('slow',{label:'slow'}),()=>agent('fast',{label:'fast'})]);",
    );
    let harness = interrupted(&script, OnUnknown::Wait).await;
    let before = harness.gateway.stats().admissions;
    let (run, notice) = harness
        .run_parked(&script, Some(Resolution::Fail), Duration::from_secs(60))
        .await;
    assert!(matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::Unknown,
            episode: 0,
            ..
        }
    ));
    assert_eq!(result(run.outcome), json!([null, "fast"]));
    assert_eq!(harness.gateway.stats().admissions, before);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn operator_answers() {
    let script = source("return await agent('slow',{label:'slow'});");
    for answer in [Resolution::Fail, Resolution::Rerun, Resolution::Abort] {
        let harness = interrupted(&script, OnUnknown::Wait).await;
        let (run, notice) = harness
            .run_parked(&script, Some(answer), Duration::ZERO)
            .await;
        assert!(matches!(
            notice,
            JournalNotice::NeedsOperator {
                cause: AttentionCause::Unknown,
                episode: 0,
                ..
            }
        ));
        match answer {
            Resolution::Fail => {
                assert_eq!(result(run.outcome), json!(null));
                assert_eq!(harness.gateway.stats().resolves, 1);
            }
            Resolution::Rerun => {
                assert_eq!(result(run.outcome), json!("ok"));
                assert_eq!(
                    (
                        harness.gateway.stats().sessions,
                        harness.gateway.stats().admissions
                    ),
                    (2, 2)
                );
                assert!(harness.records().iter().any(|r|matches!(&r.record,Record::CallIntent{attempt:1,request_id,..} if request_id.ends_with(":a1"))));
            }
            Resolution::Abort => {
                assert!(matches!(run.outcome, RunOutcome::Aborted { .. }));
                let resumed = harness.run(&script).await;
                assert_eq!(result(resumed.outcome), json!("ok"));
                assert_eq!(harness.gateway.stats().sessions, 2);
            }
        }
    }
    let harness = interrupted(&script, OnUnknown::Fail).await;
    let run = harness.run(&script).await;
    assert!(run.notices.iter().any(|notice| matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::Unknown,
            ..
        }
    )));
    assert_eq!(result(run.outcome), json!(null));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn refusals_map_to_causes() {
    let script = source("return await agent('slow',{label:'slow'});");
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![rule("slow", "ok", 100, Some("hold"))]);
    harness.crash_after_submit(&script).await;
    let path = harness.path().join("journal.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut records = text
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let turn = records
        .iter_mut()
        .find(|record| record["t"] == "turn_intent")
        .unwrap();
    turn["text"] = json!("different");
    std::fs::write(
        &path,
        format!(
            "{}\n",
            records
                .into_iter()
                .map(|r| serde_json::to_string(&r).unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
    .unwrap();
    let (_, notice) = harness
        .run_parked(&script, Some(Resolution::Fail), Duration::ZERO)
        .await;
    assert!(matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::IdentityConflict,
            ..
        }
    ));

    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![rule("slow", "ok", 100, Some("hold"))]);
    let paused = harness.crash_after_submit(&script).await;
    let session = paused.sent.lock().unwrap()[0].session_id.clone();
    harness.gateway.delete_session(&session);
    let (_, notice) = harness
        .run_parked(&script, Some(Resolution::Fail), Duration::ZERO)
        .await;
    assert!(matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::Lost,
            ..
        }
    ));

    let harness = Harness::new(&script);
    let port = RejectSubmit {
        inner: harness.gateway.clone(),
        reason: "queue_full",
    };
    let run = harness.run_port(&script, port, NoGate).await;
    assert_eq!(result(run.outcome), json!(null));
    let harness = Harness::new(&script);
    harness
        .gateway
        .fail_next(1000, PortError::Transport("offline".into()));
    let run = harness.run(&script).await;
    match run.outcome {
        RunOutcome::Aborted { reason } => assert!(reason.contains("gateway unavailable")),
        other => panic!("expected abort: {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn saved_results_are_kept() {
    use futures_util::future::BoxFuture;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use talaria_workflow::engine::{self, MapResolver};
    use talaria_workflow::journal::{
        CreateSession, GatewayPort, JournaledBackend, Receipt, RetryPolicy, RunJournal,
        SessionBound, SubmitTurn,
    };
    use tokio_util::sync::CancellationToken;
    let script = source("return await agent('slow',{label:'slow'});");
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![rule("slow", "kept", 10, None)]);
    harness.crash_after_submit(&script).await;
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(20)).await;
    tokio::task::yield_now().await;
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome), json!("kept"));
    let stats = harness.gateway.stats();
    assert_eq!((stats.sessions, stats.admissions), (1, 1));
    assert!(stats.receipts >= 1);
    for answer in [Resolution::Fail, Resolution::Rerun, Resolution::Abort] {
        let mut harness = Harness::new(&script);
        let mut lost = rule("slow", "kept", 10, None);
        lost.settlement_lost = true;
        harness.gateway = FakeGateway::new().with_script(vec![lost]);
        let (run, _) = harness
            .run_parked(&script, Some(answer), Duration::ZERO)
            .await;
        if answer == Resolution::Abort {
            assert!(matches!(run.outcome, RunOutcome::Aborted { .. }));
            assert_eq!(result(harness.run(&script).await.outcome), json!("kept"));
        } else {
            assert_eq!(result(run.outcome), json!("kept"));
        }
        let stats = harness.gateway.stats();
        assert_eq!((stats.sessions, stats.admissions), (1, 1));
    }
    #[derive(Clone)]
    struct StaleOnce {
        inner: FakeGateway,
        first: Arc<AtomicBool>,
    }
    impl GatewayPort for StaleOnce {
        fn create_session(
            &self,
            r: CreateSession,
        ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
            self.inner.create_session(r)
        }
        fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
            self.inner.attach(p, s)
        }
        fn submit(&self, r: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
            self.inner.submit(r)
        }
        fn receipt(
            &self,
            p: String,
            s: String,
            a: String,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            self.inner.receipt(p, s, a)
        }
        fn wait_settled(
            &self,
            p: String,
            s: String,
            a: String,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            self.inner.wait_settled(p, s, a)
        }
        fn resolve_unknown(
            &self,
            p: String,
            s: String,
            a: String,
            g: i64,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            let generation = if self.first.swap(false, Ordering::SeqCst) {
                g + 1
            } else {
                g
            };
            self.inner.resolve_unknown(p, s, a, generation)
        }
    }
    let mut harness = Harness::new(&script);
    let mut lost = rule("slow", "kept", 10, None);
    lost.settlement_lost = true;
    harness.gateway = FakeGateway::new().with_script(vec![lost]);
    let (journal, _, _) = RunJournal::open(harness.dir.clone(), &script)
        .await
        .unwrap();
    let journal = Arc::new(journal);
    let mut notices = journal.notices();
    let config = journal.engine_config(&harness.resolver).unwrap();
    let port = StaleOnce {
        inner: harness.gateway.clone(),
        first: Arc::new(AtomicBool::new(true)),
    };
    let backend = Arc::new(JournaledBackend::new(
        journal.clone(),
        port,
        TestPolicy,
        harness.resolver.clone(),
        NoGate,
        RetryPolicy::default(),
    ));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let forward = journal.forward_events(rx);
    let mut execution = Box::pin(engine::run(
        engine::parse_script(&script).unwrap(),
        config,
        backend,
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    ));
    let mut first_key = String::new();
    for expected in 0..2 {
        let notice = loop {
            tokio::select! {out=&mut execution=>panic!("ended before re-park: {out:?}"), notice=notices.recv()=>{let notice=notice.unwrap();if matches!(notice,JournalNotice::NeedsOperator{..}) {break notice}}}
        };
        let JournalNotice::NeedsOperator {
            key,
            attempt,
            episode,
            ..
        } = notice
        else {
            unreachable!()
        };
        assert_eq!(episode, expected);
        if episode == 0 {
            first_key = key.clone();
        } else {
            assert_eq!(key, first_key);
        }
        journal
            .dir()
            .request_resolution(&key, attempt, episode, Resolution::Fail)
            .unwrap();
    }
    assert_eq!(result(execution.await), json!("kept"));
    forward.await.unwrap().unwrap();
    drop(journal);
    assert_eq!(harness.gateway.stats().sessions, 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn resolution_files_are_validated() {
    use talaria_workflow::journal::{JournalError, JournalWarning, RunJournal};
    let (_temp, dir) = new_dir();
    write_records(&dir, &parked_records(&dir));
    let folder = dir.path().join("resolutions");
    let entries=[("b".repeat(32),"{invalid".to_owned()),("c".repeat(32),json!({"v":1,"key":"c".repeat(32),"attempt":0,"episode":0,"resolution":"fail","extra":1}).to_string()),("d".repeat(32),json!({"v":1,"key":"d".repeat(32),"attempt":0,"episode":0,"resolution":"unknown"}).to_string()),("e".repeat(32),json!({"v":1,"key":"f".repeat(32),"attempt":0,"episode":0,"resolution":"fail"}).to_string())];
    for (name, content) in &entries {
        std::fs::write(folder.join(format!("{name}.0.0.json")), content).unwrap();
    }
    let stale = "f".repeat(32);
    std::fs::write(
        folder.join(format!("{stale}.0.4.json")),
        json!({"v":1,"key":stale,"attempt":0,"episode":4,"resolution":"fail"}).to_string(),
    )
    .unwrap();
    let (journal, _, warnings) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|w| matches!(w, JournalWarning::BadResolution { .. }))
            .count(),
        4
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, JournalWarning::StaleResolution { .. }))
    );
    for (name, _) in &entries {
        assert!(folder.join(format!("{name}.0.0.json.rejected")).exists());
    }
    assert!(!folder.join(format!("{stale}.0.4.json")).exists());
    let journal = std::sync::Arc::new(journal);
    let reached = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = reached.clone();
    journal.observe_appends(move |record| {
        let pause = matches!(record, Record::CallResolved { .. });
        let signal = signal.clone();
        Box::pin(async move {
            if pause {
                signal.notify_one();
                std::future::pending::<()>().await;
            }
        })
    });
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let forward = journal.forward_events(rx);
    dir.request_resolution(&key(), 0, 0, Resolution::Fail)
        .unwrap();
    assert!(matches!(
        dir.request_resolution(&key(), 0, 0, Resolution::Rerun),
        Err(JournalError::ResolutionPending { .. })
    ));
    tokio::time::timeout(Duration::from_secs(5), reached.notified())
        .await
        .unwrap();
    let answered = folder.join(format!("{}.0.0.json", key()));
    assert!(answered.exists());
    journal.stop_resolution_poller();
    drop(tx);
    forward.abort();
    let _ = forward.await;
    drop(journal);
    let (journal, _, warnings) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    assert!(!answered.exists());
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w, JournalWarning::StaleResolution { .. }))
    );
    drop(journal);
    let (records, _) =
        talaria_workflow::journal::JournalReader::read_from(&records_path(&dir), 0).unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r.record, Record::CallResolved { .. }))
            .count(),
        1
    );
}
