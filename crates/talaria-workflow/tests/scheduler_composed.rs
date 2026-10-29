#![cfg(feature = "fake-gateway")]
mod scheduler_support;
use scheduler_support::{Harness, no_budget, roster_data};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use talaria_workflow::{
    engine::{self, MapResolver, RunOutcome},
    hermes::{ExplicitLaunch, LaunchDefaults, SchemaTurnPolicy},
    journal::{
        JournaledBackend, NewManifest, OnUnknown, RetryPolicy, RunDir, RunJournal,
        fake_gateway::{FakeGateway, ScriptRule},
    },
    staffing::{ProviderLimits, StaffedLaunch, StaffingResolver},
};
async fn run_fanout(
    width: u32,
    token_budget: Option<u64>,
) -> (
    RunOutcome,
    talaria_workflow::schedule::Scheduler,
    FakeGateway,
    talaria_workflow::schedule::SchedulerEvents,
) {
    let mut h = Harness::new(width, roster_data()).await;
    h.table
        .providers
        .insert("alpha".into(), ProviderLimits { max_in_flight: 3 });
    h.table.budget.tokens = token_budget;
    for (tier, provider) in [("survey", "alpha"), ("builder", "beta")] {
        h.table.tiers.insert(
            tier.into(),
            talaria_workflow::staffing::TierSpec {
                model: "m".into(),
                provider: provider.into(),
                effort: None,
                note: None,
            },
        );
    }
    let source = "export const meta={name:'fixture',description:'test',phases:[]}; return await parallel(Array.from({length:20},(_,i)=>()=>agent('request '+i,{tier:i%2?'survey':'builder'})));";
    let dir = RunDir::create(
        h.root.path(),
        NewManifest {
            workflow_name: "fixture".into(),
            args: None,
            started_at_ms: 0,
            seed: 1,
            profile: "fixture".into(),
            on_unknown: OnUnknown::Wait,
            talaria_version: "test".into(),
            hermes: json!({}),
            roster: json!({}),
            approval_policy: json!({}),
        },
        source,
    )
    .unwrap();
    let (journal, _, _) = RunJournal::open(dir, source).await.unwrap();
    let journal = Arc::new(journal);
    let (scheduler, events) = talaria_workflow::schedule::Scheduler::start(
        talaria_workflow::schedule::SchedulerConfig::from_table(
            &h.table,
            h.table.budget.clone(),
            "fixture",
            h.prices.clone(),
        ),
        h.cache.clone(),
        journal.dir().path().join("journal.jsonl"),
        h.clock.clone(),
    )
    .await
    .unwrap();
    let launch = StaffedLaunch::new(
        StaffingResolver::new(
            Arc::new(h.table.clone()),
            h.cache.cached("fixture").unwrap(),
        ),
        ExplicitLaunch::new(LaunchDefaults {
            profile: "fixture".into(),
            cwd: h.root.path().canonicalize().unwrap(),
            toolsets: Some(vec!["file".into()]),
            worktrees: None,
        }),
    );
    let config = journal.engine_config(&launch).unwrap();
    let fake = FakeGateway::new().with_script(vec![ScriptRule {
        duration: Duration::from_millis(10),
        usage: json!({"total_tokens":1500,"input_tokens":1000,"output_tokens":500}),
        ..Default::default()
    }]);
    let backend = JournaledBackend::new(
        journal.clone(),
        fake.clone(),
        SchemaTurnPolicy,
        launch,
        scheduler.clone(),
        RetryPolicy::default(),
    );
    let handle = engine::spawn(
        engine::parse_script(source).unwrap(),
        config,
        Arc::new(backend),
        Arc::new(MapResolver::new()),
    );
    let forward = journal.forward_events(handle.events);
    let mut outcome = handle.outcome;
    let result = loop {
        tokio::select! {result=&mut outcome=>break result.unwrap(),_=tokio::time::sleep(Duration::from_millis(100))=>{h.clock.advance(chrono::Duration::milliseconds(100));}}
    };
    forward.await.unwrap().unwrap();
    (result, scheduler, fake, events)
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fan_out_respects_widths_and_budget() {
    let (result, scheduler, fake, mut events) = run_fanout(4, Some(9000)).await;
    assert!(
        matches!(result, RunOutcome::Completed { .. }),
        "unexpected outcome: {result:?}"
    );
    assert!(
        scheduler.status().spend.tokens >= 9000,
        "result={result:?}, stats={:?}, status={:?}",
        fake.stats(),
        scheduler.status()
    );
    assert!(fake.stats().admissions < 20);
    let mut admitted = 0;
    while let Ok(event) = events.0.try_recv() {
        if let talaria_workflow::schedule::SchedulerEvent::Admitted {
            in_flight,
            lane_in_flight,
            provider,
            ..
        } = event.event
        {
            admitted += 1;
            assert!(in_flight <= 4);
            if provider == "alpha" {
                assert!(lane_in_flight <= 3);
            }
        }
    }
    assert!(admitted > 0 && admitted < 20);
    assert_eq!(scheduler.status().in_flight, 0);
}

#[derive(Clone)]
struct RetryGateway {
    inner: FakeGateway,
    failed: Arc<std::sync::Mutex<std::collections::BTreeMap<String, bool>>>,
    always: bool,
}
impl RetryGateway {
    fn adjust(
        &self,
        mut receipt: talaria_workflow::journal::Receipt,
    ) -> talaria_workflow::journal::Receipt {
        if receipt.status == talaria_workflow::journal::AdmissionState::Terminal
            && self.failed.lock().unwrap().get(&receipt.session_id) == Some(&true)
        {
            receipt.outcome = Some("failed".into());
            receipt.result =
                Some(json!({"failed":true,"failure_reason":"rate_limit","final_response":""}));
        }
        receipt
    }
}
impl talaria_workflow::journal::GatewayPort for RetryGateway {
    fn create_session(
        &self,
        request: talaria_workflow::journal::CreateSession,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::SessionBound, talaria_workflow::journal::PortError>,
    > {
        let this = self.clone();
        Box::pin(async move {
            let failed = this.always || !request.request_id.ends_with(":a1");
            let session = this.inner.create_session(request).await?;
            this.failed
                .lock()
                .unwrap()
                .insert(session.session_id.clone(), failed);
            Ok(session)
        })
    }
    fn attach(
        &self,
        p: String,
        s: String,
    ) -> futures_util::future::BoxFuture<'static, Result<(), talaria_workflow::journal::PortError>>
    {
        self.inner.attach(p, s)
    }
    fn submit(
        &self,
        r: talaria_workflow::journal::SubmitTurn,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.inner.submit(r)
    }
    fn receipt(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        let this = self.clone();
        Box::pin(async move {
            let result = this.inner.receipt(p, s, a).await?;
            Ok(this.adjust(result))
        })
    }
    fn wait_settled(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        let this = self.clone();
        Box::pin(async move {
            let result = this.inner.wait_settled(p, s, a).await?;
            Ok(this.adjust(result))
        })
    }
    fn resolve_unknown(
        &self,
        p: String,
        s: String,
        a: String,
        g: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.inner.resolve_unknown(p, s, a, g)
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rate_limited_call_retries_once() {
    use talaria_workflow::journal::{JournalReader, Record};
    for always in [false, true] {
        let h = Harness::new(10, roster_data()).await;
        h.source.push("fixture", Ok(roster_data()));
        h.source.push("fixture", Ok(roster_data()));
        let source = "export const meta={name:'fixture',description:'test',phases:[]}; return await agent('request',{model:'m',provider:'alpha'});";
        let dir = RunDir::create(
            h.root.path(),
            NewManifest {
                workflow_name: "fixture".into(),
                args: None,
                started_at_ms: 0,
                seed: 1,
                profile: "fixture".into(),
                on_unknown: OnUnknown::Wait,
                talaria_version: "test".into(),
                hermes: json!({}),
                roster: json!({}),
                approval_policy: json!({}),
            },
            source,
        )
        .unwrap();
        let (journal, _, _) = RunJournal::open(dir, source).await.unwrap();
        let journal = Arc::new(journal);
        let (scheduler, _) = talaria_workflow::schedule::Scheduler::start(
            talaria_workflow::schedule::SchedulerConfig::from_table(
                &h.table,
                no_budget(),
                "fixture",
                h.prices.clone(),
            ),
            h.cache.clone(),
            journal.dir().path().join("journal.jsonl"),
            h.clock.clone(),
        )
        .await
        .unwrap();
        let launch = StaffedLaunch::new(
            StaffingResolver::new(
                Arc::new(h.table.clone()),
                h.cache.cached("fixture").unwrap(),
            ),
            ExplicitLaunch::new(LaunchDefaults {
                profile: "fixture".into(),
                cwd: h.root.path().canonicalize().unwrap(),
                toolsets: Some(vec!["file".into()]),
                worktrees: None,
            }),
        );
        let config = journal.engine_config(&launch).unwrap();
        let fake = FakeGateway::new().with_script(vec![ScriptRule {
            duration: Duration::from_millis(1),
            usage: json!({"total_tokens":1}),
            ..Default::default()
        }]);
        let port = RetryGateway {
            inner: fake.clone(),
            failed: Default::default(),
            always,
        };
        let backend = JournaledBackend::new(
            journal.clone(),
            port,
            SchemaTurnPolicy,
            launch,
            scheduler.clone(),
            RetryPolicy::default(),
        );
        let handle = engine::spawn(
            engine::parse_script(source).unwrap(),
            config,
            Arc::new(backend),
            Arc::new(MapResolver::new()),
        );
        let forward = journal.forward_events(handle.events);
        let mut pending = handle.outcome;
        let outcome = loop {
            tokio::select! {v=&mut pending=>break v.unwrap(),_=tokio::time::sleep(Duration::from_millis(100))=>h.clock.advance(chrono::Duration::milliseconds(100))}
        };
        forward.await.unwrap().unwrap();
        let result = match outcome {
            RunOutcome::Completed { result } => result,
            other => panic!("{other:?}"),
        };
        assert_eq!(result, if always { Value::Null } else { json!("ok") });
        assert_eq!(fake.stats().sessions, 2);
        let (lines, _) = JournalReader::read_from(journal.dir().path(), 0).unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|r| matches!(r.record, Record::CallRetryFinished { attempt: 0, .. }))
                .count(),
            1
        );
        assert!(lines.iter().any(|r|matches!(&r.record,Record::CallIntent{attempt:1,request_id,..} if request_id.ends_with(":a1"))));
        assert!(lines.iter().any(|r| matches!(&r.record, Record::TurnSettled { result_meta, .. } if result_meta["failure_reason"] == "rate_limit")));
        assert!(
            !lines
                .iter()
                .any(|r| matches!(&r.record, Record::CallIntent { attempt, .. } if *attempt > 1))
        );
        assert_eq!(scheduler.status().global_width, 5);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn resumed_call_intent_respects_max_in_flight() {
    use talaria_workflow::journal::{JournalReader, Record};
    let h = Harness::new(1, roster_data()).await;
    let source = "export const meta={name:'fixture',description:'test',phases:[]}; return await parallel([()=>agent('first',{model:'m',provider:'alpha',label:'a0'}),()=>agent('second',{model:'m',provider:'alpha',label:'a1'})]);";
    let dir = RunDir::create(
        h.root.path(),
        NewManifest {
            workflow_name: "fixture".into(),
            args: None,
            started_at_ms: 0,
            seed: 1,
            profile: "fixture".into(),
            on_unknown: OnUnknown::Wait,
            talaria_version: "test".into(),
            hermes: json!({}),
            roster: json!({}),
            approval_policy: json!({}),
        },
        source,
    )
    .unwrap();
    let run_path = dir.path().to_path_buf();
    let fake = FakeGateway::new().with_script(vec![ScriptRule {
        duration: Duration::from_millis(200),
        usage: json!({"total_tokens":10}),
        ..Default::default()
    }]);
    {
        let (journal, _, _) = RunJournal::open(dir, source).await.unwrap();
        let journal = Arc::new(journal);
        let (scheduler, _) = talaria_workflow::schedule::Scheduler::start(
            talaria_workflow::schedule::SchedulerConfig::from_table(
                &h.table,
                no_budget(),
                "fixture",
                h.prices.clone(),
            ),
            h.cache.clone(),
            journal.dir().path().join("journal.jsonl"),
            h.clock.clone(),
        )
        .await
        .unwrap();
        let launch = StaffedLaunch::new(
            StaffingResolver::new(
                Arc::new(h.table.clone()),
                h.cache.cached("fixture").unwrap(),
            ),
            ExplicitLaunch::new(LaunchDefaults {
                profile: "fixture".into(),
                cwd: h.root.path().canonicalize().unwrap(),
                toolsets: Some(vec!["file".into()]),
                worktrees: None,
            }),
        );
        let config = journal.engine_config(&launch).unwrap();
        let backend = JournaledBackend::new(
            journal.clone(),
            fake.clone(),
            SchemaTurnPolicy,
            launch,
            scheduler,
            RetryPolicy::default(),
        );
        let handle = engine::spawn(
            engine::parse_script(source).unwrap(),
            config,
            Arc::new(backend),
            Arc::new(MapResolver::new()),
        );
        let forward = journal.forward_events(handle.events);
        loop {
            let (lines, _) = JournalReader::read_from(&run_path.join("journal.jsonl"), 0).unwrap();
            let intents = lines
                .iter()
                .filter(|r| matches!(r.record, Record::CallIntent { .. }))
                .count();
            let admitted = lines
                .iter()
                .filter(|r| matches!(r.record, Record::TurnAdmitted { .. }))
                .count();
            if intents == 2 && admitted == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        handle.cancel.cancel();
        let _ = handle.outcome.await;
        let _ = forward.await;
    }
    let dir = RunDir::open(&run_path).unwrap();
    let (journal, _, _) = RunJournal::open(dir, source).await.unwrap();
    let journal = Arc::new(journal);
    h.source.push("fixture", Ok(roster_data()));
    let (scheduler, mut events) = talaria_workflow::schedule::Scheduler::start(
        talaria_workflow::schedule::SchedulerConfig::from_table(
            &h.table,
            no_budget(),
            "fixture",
            h.prices.clone(),
        ),
        h.cache.clone(),
        journal.dir().path().join("journal.jsonl"),
        h.clock.clone(),
    )
    .await
    .unwrap();
    let launch = StaffedLaunch::new(
        StaffingResolver::new(
            Arc::new(h.table.clone()),
            h.cache.cached("fixture").unwrap(),
        ),
        ExplicitLaunch::new(LaunchDefaults {
            profile: "fixture".into(),
            cwd: h.root.path().canonicalize().unwrap(),
            toolsets: Some(vec!["file".into()]),
            worktrees: None,
        }),
    );
    let config = journal.engine_config(&launch).unwrap();
    let backend = JournaledBackend::new(
        journal.clone(),
        fake,
        SchemaTurnPolicy,
        launch,
        scheduler,
        RetryPolicy::default(),
    );
    let handle = engine::spawn(
        engine::parse_script(source).unwrap(),
        config,
        Arc::new(backend),
        Arc::new(MapResolver::new()),
    );
    let forward = journal.forward_events(handle.events);
    let mut pending = handle.outcome;
    let outcome = loop {
        tokio::select! {
            v = &mut pending => break v.unwrap(),
            _ = tokio::time::sleep(Duration::from_millis(10)) => h.clock.advance(chrono::Duration::milliseconds(10)),
        }
    };
    forward.await.unwrap().unwrap();
    assert!(matches!(outcome, RunOutcome::Completed { .. }));
    let mut peak_in_flight = 0;
    while let Ok(ev) = events.0.try_recv() {
        if let talaria_workflow::schedule::SchedulerEvent::Admitted { in_flight, .. } = ev.event {
            peak_in_flight = peak_in_flight.max(in_flight);
        }
    }
    assert_eq!(
        peak_in_flight, 1,
        "resumed CallIntent bypassed max_in_flight = 1"
    );
}
