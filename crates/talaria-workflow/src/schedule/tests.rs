use super::*;
use crate::hermes::wire::Cursor;
use crate::{
    engine::{AgentRequest, CallId, Effort},
    hermes::{ExplicitLaunch, LaunchDefaults, Wire, WireError, WireEvent, WireFactory},
    journal::*,
    staffing::*,
};
use chrono::{DateTime, Utc};
use futures_util::{future::BoxFuture, poll};
use serde_json::{Map, Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn data() -> Value {
    json!({"model":"m","provider":"alpha","providers":[{"slug":"alpha","name":"Alpha","authenticated":true,"models":["m"],"pricing":{"m":{"input":"$3.00","output":"$15.00","free":false}}},{"slug":"beta","name":"Beta","authenticated":true,"models":["m"]}]})
}
fn request(n: u32) -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: format!("{n:064x}"),
            occurrence: 0,
        },
        label: format!("call-{n}"),
        phase: None,
        prompt: "test".into(),
        schema: None,
        tier: None,
        model: Some("m".into()),
        provider: Some("alpha".into()),
        effort: None,
        profile: None,
        toolsets: Some(vec!["file".into()]),
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path: vec![],
    }
}
fn launch(profile: &str, provider: &str) -> ResolvedLaunch {
    ResolvedLaunch {
        profile: profile.into(),
        provider: Some(provider.into()),
        model: "m".into(),
        effort: None,
        toolsets: vec!["file".into()],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}
struct Context {
    _temp: tempfile::TempDir,
    journal: Arc<RunJournal>,
    source: Arc<FixtureRosterSource>,
    clock: Arc<ManualClock>,
    cache: RosterCache,
    table: StaffingTable,
    snapshot: Value,
}
impl Context {
    async fn new(value: Value, width: u32) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source_text =
            "export const meta = {name:'schedule', description:'test', phases:[]}; return null;";
        let dir = RunDir::create(
            temp.path(),
            NewManifest {
                workflow_name: "schedule".into(),
                args: None,
                started_at_ms: now().timestamp_millis(),
                seed: 1,
                profile: "fixture".into(),
                on_unknown: OnUnknown::Wait,
                talaria_version: "test".into(),
                hermes: json!({}),
                roster: json!({}),
                approval_policy: json!({}),
            },
            source_text,
        )
        .unwrap();
        let (journal, _, _) = RunJournal::open(dir, source_text).await.unwrap();
        let clock = Arc::new(ManualClock::new(now()));
        let source = Arc::new(FixtureRosterSource::with_clock(clock.clone()));
        source.push("fixture", Ok(value));
        let cache = RosterCache::new(source.clone(), clock.clone(), Duration::from_secs(30));
        let r = cache.load("fixture").await.unwrap();
        let table = StaffingTable::parse(&format!("[limits]\nmax_in_flight={width}")).unwrap();
        let snapshot = r.snapshot_with(&table, &table.budget);
        Self {
            _temp: temp,
            journal: Arc::new(journal),
            source,
            clock,
            cache,
            table,
            snapshot,
        }
    }
    async fn start(&self) -> (Scheduler, SchedulerEvents) {
        Scheduler::start(
            SchedulerConfig::from_table(
                &self.table,
                self.table.budget.clone(),
                "fixture",
                PriceBook::from_snapshot(&self.snapshot),
            ),
            self.cache.clone(),
            self.journal.dir().path().join("journal.jsonl"),
            self.clock.clone(),
        )
        .await
        .unwrap()
    }
    async fn step(&self, secs: u64) {
        self.clock.advance(chrono::Duration::seconds(secs as i64));
        tokio::time::advance(Duration::from_secs(secs)).await;
        settle_tasks().await;
    }
    async fn record(
        &self,
        q: &AgentRequest,
        l: &ResolvedLaunch,
        attempt: u32,
        turns: Vec<(Value, Value)>,
        retry: bool,
    ) {
        let key = journal_key(&q.call_id, l);
        let dir = self.journal.dir();
        self.journal
            .append(Record::CallIntent {
                key: key.clone(),
                attempt,
                call_id: q.call_id.clone(),
                label: q.label.clone(),
                phase: None,
                workflow_path: vec![],
                launch: l.clone(),
                request_id: request_id(dir.run_id(), &key, attempt),
            })
            .await
            .unwrap();
        self.journal
            .append(Record::SessionBound {
                key: key.clone(),
                attempt,
                session_id: format!("s-{attempt}"),
                stored_session_id: format!("s-{attempt}"),
            })
            .await
            .unwrap();
        let count = turns.len() as u32;
        for (turn, (usage, meta)) in turns.into_iter().enumerate() {
            let turn = turn as u32;
            let admission = format!("admission-{attempt}-{turn}");
            self.journal
                .append(Record::TurnIntent {
                    key: key.clone(),
                    attempt,
                    turn,
                    submission_id: submission_id(dir.run_id(), &key, attempt, turn),
                    text: Some("test".into()),
                    text_ref: None,
                })
                .await
                .unwrap();
            self.journal
                .append(Record::TurnAdmitted {
                    key: key.clone(),
                    attempt,
                    turn,
                    admission_id: admission.clone(),
                    status: AdmissionState::Started,
                    outcome: None,
                    execution_generation: Some(1),
                    authority_epoch: 1,
                })
                .await
                .unwrap();
            self.journal
                .append(Record::TurnSettled {
                    key: key.clone(),
                    attempt,
                    turn,
                    admission_id: admission,
                    outcome: if meta["failure_reason"].is_string() {
                        "failed"
                    } else {
                        "completed"
                    }
                    .into(),
                    final_text: Some("ok".into()),
                    final_text_ref: None,
                    result_meta: meta,
                    usage: Some(usage),
                })
                .await
                .unwrap();
        }
        let outcome = CallOutcome::Failed {
            reason: "test".into(),
        };
        let finish = if retry {
            Record::CallRetryFinished {
                key,
                attempt,
                outcome,
                turns: count,
            }
        } else {
            Record::CallFinished {
                key,
                attempt,
                outcome,
                turns: count,
            }
        };
        self.journal.append(finish).await.unwrap();
    }
}
async fn settle_tasks() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}
fn permit(d: GateDecision) -> GatePermit {
    match d {
        GateDecision::Proceed(p) => p,
        _ => panic!("expected admission"),
    }
}
fn cost(n: f64, status: &str) -> Value {
    json!({"input_tokens":1000,"output_tokens":500,"total_tokens":1500,"session_cost":{"estimated_cost_usd":n,"cost_status":status}})
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn global_lane_widths_fifo_and_reattach() {
    let mut c = Context::new(data(), 4).await;
    c.table
        .providers
        .insert("alpha".into(), ProviderLimits { max_in_flight: 2 });
    let (s, _) = c.start().await;
    let l = launch("fixture", "alpha");
    let a = permit(s.acquire(&request(0), &l, GateReason::Fresh).await);
    let b = permit(s.acquire(&request(1), &l, GateReason::Fresh).await);
    let mut q = s.acquire(&request(2), &l, GateReason::Fresh);
    assert!(poll!(&mut q).is_pending());
    let mut later = s.acquire(&request(3), &l, GateReason::Fresh);
    assert!(poll!(&mut later).is_pending());
    let beta = permit(
        s.acquire(&request(4), &launch("fixture", "beta"), GateReason::Fresh)
            .await,
    );
    let extra = permit(s.acquire(&request(5), &l, GateReason::Reattach).await);
    assert_eq!(s.status().in_flight, 4);
    drop(extra);
    drop(a);
    let first = permit(q.await);
    assert!(poll!(&mut later).is_pending());
    drop(b);
    let second = permit(later.await);
    drop((first, second, beta));
    assert_eq!(s.status().in_flight, 0);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn twenty_five_calls_respect_global_and_lane_order() {
    let c = Context::new(data(), 10).await;
    let (s, _) = c.start().await;
    let mut held = vec![];
    let mut waiting = vec![];
    for i in 0..25 {
        let l = launch("fixture", if i % 3 == 0 { "beta" } else { "alpha" });
        let mut f = s.acquire(&request(i), &l, GateReason::Fresh);
        match poll!(&mut f) {
            std::task::Poll::Ready(d) => held.push(permit(d)),
            std::task::Poll::Pending => waiting.push(f),
        }
    }
    assert_eq!(held.len(), 10);
    let mut next = waiting.into_iter();
    for _ in 0..15 {
        held.pop();
        let p = permit(next.next().unwrap().await);
        held.insert(0, p);
        assert!(s.status().in_flight <= 10);
    }
    drop(held);
    assert_eq!(s.status().in_flight, 0);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn limit_reset_requires_refresh_and_does_not_block_other_lanes() {
    let mut value = data();
    value["providers"][0]["limit"] = json!({"scope":"account","resets_at":"2026-10-22T16:05:00Z"});
    let c = Context::new(value, 10).await;
    c.source.push("fixture", Ok(data()));
    let (s, mut events) = c.start().await;
    let l = launch("fixture", "alpha");
    let mut f = s.acquire(&request(0), &l, GateReason::Fresh);
    assert!(poll!(&mut f).is_pending());
    let b = permit(
        s.acquire(&request(1), &launch("fixture", "beta"), GateReason::Fresh)
            .await,
    );
    c.step(299).await;
    assert!(poll!(&mut f).is_pending());
    assert_eq!(c.source.fetches().len(), 1);
    c.step(1).await;
    drop(permit(f.await));
    drop(b);
    assert_eq!(c.source.fetches().len(), 2);
    assert!(
        std::iter::from_fn(|| events.0.try_recv().ok()).any(|e| matches!(
            e.event,
            SchedulerEvent::Queued {
                reason: WaitReason::ProviderLimited { .. },
                ..
            }
        ))
    );
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_reset_refreshes_once_per_cooldown_and_retains_failed_roster() {
    let mut v = data();
    v["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let c = Context::new(v.clone(), 10).await;
    for _ in 0..3 {
        c.source
            .push("fixture", Err(RosterError::Transport("offline".into())));
    }
    c.source.push("fixture", Ok(data()));
    let (s, mut events) = c.start().await;
    let mut f = s.acquire(&request(0), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut f).is_pending());
    for n in 1..=3 {
        c.step(59).await;
        assert_eq!(c.source.fetches().len(), n);
        c.step(1).await;
        assert_eq!(c.source.fetches().len(), n + 1);
        assert!(poll!(&mut f).is_pending());
    }
    assert!(
        std::iter::from_fn(|| events.0.try_recv().ok()).any(|e| matches!(
            e.event,
            SchedulerEvent::RosterReadFailed {
                persistent: true,
                ..
            }
        ))
    );
    c.step(60).await;
    drop(permit(f.await));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn per_profile_limits_and_billing_are_isolated() {
    let mut v = data();
    v["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let c = Context::new(v, 10).await;
    c.source.push("default", Ok(data()));
    let (s, _) = c.start().await;
    let mut blocked = s.acquire(&request(0), &launch("fixture", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut blocked).is_pending());
    let mut other = s.acquire(&request(1), &launch("default", "alpha"), GateReason::Fresh);
    assert!(poll!(&mut other).is_pending());
    settle_tasks().await;
    let p = permit(other.await);
    c.record(
        &request(2),
        &launch("fixture", "alpha"),
        0,
        vec![(cost(0.0, "included"), json!({"failure_reason":"billing"}))],
        false,
    )
    .await;
    drop(p);
    assert!(matches!(blocked.await, GateDecision::Skip));
    drop(permit(
        s.acquire(&request(3), &launch("default", "alpha"), GateReason::Fresh)
            .await,
    ));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rate_limits_halve_once_globally_every_time_per_lane_and_deduplicate() {
    let c = Context::new(data(), 10).await;
    c.source.push("fixture", Ok(data()));
    c.source.push("fixture", Ok(data()));
    let (s, mut ev) = c.start().await;
    let l = launch("fixture", "alpha");
    let q = request(0);
    let last = SettledTurn {
        turn: 0,
        outcome: "failed".into(),
        final_text: None,
        result_meta: json!({"failure_reason":"rate_limit"}),
        usage: Some(cost(0.0, "included")),
    };
    assert!(s.retry_after_failure(&q, &l, &last, 0));
    assert_eq!(s.status().global_width, 5);
    assert_eq!(s.status().lanes[0].width, 5);
    c.record(
        &q,
        &l,
        0,
        vec![(cost(0.0, "included"), last.result_meta.clone())],
        false,
    )
    .await;
    let p = permit(
        s.acquire(&request(1), &launch("fixture", "beta"), GateReason::Fresh)
            .await,
    );
    drop(p);
    assert_eq!(s.status().lanes[0].width, 5);
    settle_tasks().await;
    let mut retry = s.acquire(&request(2), &l, GateReason::Fresh);
    assert!(poll!(&mut retry).is_pending());
    c.step(59).await;
    assert!(poll!(&mut retry).is_pending());
    c.step(1).await;
    let p = permit(retry.await);
    c.record(
        &request(2),
        &l,
        0,
        vec![(
            cost(0.0, "included"),
            json!({"failure_reason":"overloaded"}),
        )],
        false,
    )
    .await;
    drop(p);
    assert_eq!(s.status().global_width, 5);
    assert_eq!(s.status().lanes[0].width, 2);
    let halves: Vec<_> = std::iter::from_fn(|| ev.0.try_recv().ok())
        .filter(|e| matches!(e.event, SchedulerEvent::CapHalved { .. }))
        .collect();
    assert_eq!(halves.len(), 3);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rate_limit_refresh_uses_reported_reset_instead_of_fixed_cooldown() {
    let c = Context::new(data(), 10).await;
    let mut v = data();
    v["providers"][0]["limit"] = json!({"scope":"models","models":{"m":"2026-10-22T16:05:00Z"}});
    c.source.push("fixture", Ok(v));
    c.source.push("fixture", Ok(data()));
    let (s, _) = c.start().await;
    let last = SettledTurn {
        turn: 0,
        outcome: "failed".into(),
        final_text: None,
        result_meta: json!({"failure_reason":"upstream_rate_limit"}),
        usage: None,
    };
    let l = launch("fixture", "alpha");
    assert!(s.retry_after_failure(&request(0), &l, &last, 0));
    settle_tasks().await;
    assert_eq!(s.status().lanes[0].pause_cause, None);
    let mut f = s.acquire(&request(1), &l, GateReason::Fresh);
    assert!(poll!(&mut f).is_pending());
    c.step(60).await;
    assert!(poll!(&mut f).is_pending());
    c.step(240).await;
    drop(permit(f.await));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn cumulative_money_resets_unknown_turns_and_frozen_resume_prices() {
    let c = Context::new(data(), 10).await;
    let l = launch("fixture", "alpha");
    c.record(
        &request(0),
        &l,
        0,
        vec![
            (cost(0.01, "actual"), json!({})),
            (cost(0.02, "estimated"), json!({})),
        ],
        false,
    )
    .await;
    c.record(
        &request(0),
        &l,
        1,
        vec![
            (cost(0.03, "actual"), json!({})),
            (cost(0.01, "actual"), json!({})),
        ],
        false,
    )
    .await;
    let (s, _) = c.start().await;
    assert!((s.status().spend.money_usd - 0.06).abs() < 1e-12);
    assert_eq!(s.status().spend.tokens, 6000);
    c.record(
        &request(1),
        &l,
        0,
        vec![(cost(0.0, "unknown"), json!({"failure_reason":"rate_limit"}))],
        false,
    )
    .await;
    let p = permit(
        s.acquire(
            &request(2),
            &launch("fixture", "beta"),
            GateReason::Reattach,
        )
        .await,
    );
    drop(p);
    let spent = s.status().spend;
    assert!((spent.money_usd - 0.0705).abs() < 1e-12);
    assert_eq!(spent.money_source, MoneySource::Mixed);
    assert_eq!(s.status().global_width, 5);
    let mut changed = data();
    changed["providers"][0]["pricing"]["m"]["input"] = json!("$99.00");
    c.source.push("fixture", Ok(changed));
    c.cache.load("fixture").await.unwrap();
    let (resumed, mut events) = c.start().await;
    assert_eq!(resumed.status().spend, spent);
    assert_eq!(resumed.status().global_width, 5);
    assert!(events.0.try_recv().is_err());
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_zero_included_zero_and_missing_tokens_are_accounted_separately() {
    let c = Context::new(data(), 10).await;
    let l = launch("fixture", "beta");
    c.record(
        &request(0),
        &l,
        0,
        vec![
            (cost(0.0, "unknown"), json!({})),
            (cost(0.0, "included"), json!({})),
        ],
        false,
    )
    .await;
    c.record(&request(1), &l, 0, vec![(json!({}), json!({}))], false)
        .await;
    let (s, _) = c.start().await;
    let spend = s.status().spend;
    assert_eq!(spend.money_usd, 0.0);
    assert_eq!(spend.money_unknown_turns, 2);
    assert_eq!(spend.tokens_unknown_turns, 1);
    assert_eq!(spend.money_source, MoneySource::Hermes);
    assert_eq!(spend.money_text(), "unknown (2 unpriced turns)");
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn budget_skips_queued_and_resumed_fresh_calls_but_reattach_proceeds() {
    let mut c = Context::new(data(), 1).await;
    c.table.budget.money_usd = Some(0.02);
    let (s, mut events) = c.start().await;
    let l = launch("fixture", "alpha");
    let p = permit(s.acquire(&request(0), &l, GateReason::Fresh).await);
    let mut q = s.acquire(&request(1), &l, GateReason::Fresh);
    assert!(poll!(&mut q).is_pending());
    c.record(
        &request(0),
        &l,
        0,
        vec![(cost(0.025, "actual"), json!({}))],
        false,
    )
    .await;
    drop(p);
    assert!(matches!(q.await, GateDecision::Skip));
    assert!(matches!(
        s.acquire(&request(2), &l, GateReason::Fresh).await,
        GateDecision::Skip
    ));
    drop(permit(
        s.acquire(&request(3), &l, GateReason::Reattach).await,
    ));
    assert_eq!(
        std::iter::from_fn(|| events.0.try_recv().ok())
            .filter(|e| matches!(
                e.event,
                SchedulerEvent::BudgetReached {
                    kind: BudgetKind::Money,
                    ..
                }
            ))
            .count(),
        1
    );
    let (s, _) = c.start().await;
    assert!(matches!(
        s.acquire(&request(4), &l, GateReason::Fresh).await,
        GateDecision::Skip
    ));
    let last = SettledTurn {
        turn: 0,
        outcome: "failed".into(),
        final_text: None,
        result_meta: json!({"failure_reason":"rate_limit"}),
        usage: None,
    };
    assert!(!s.retry_after_failure(&request(4), &l, &last, 0));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn events_are_flat_timed_and_lossless_for_slow_subscribers() {
    let c = Context::new(data(), 10).await;
    let (s, mut first) = c.start().await;
    let mut second = s.subscribe();
    for i in 0..100 {
        let p = permit(
            s.acquire(&request(i), &launch("fixture", "alpha"), GateReason::Fresh)
                .await,
        );
        drop(p);
    }
    let mut a = vec![];
    while let Ok(e) = first.0.try_recv() {
        a.push(e);
    }
    let mut b = vec![];
    while let Ok(e) = second.0.try_recv() {
        b.push(e);
    }
    assert_eq!(a.len(), 200);
    assert_eq!(a, b);
    for e in a {
        assert_eq!(e.at, "2026-10-22T16:00:00Z");
        let v = serde_json::to_value(e).unwrap();
        assert!(v["event"].is_string());
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn roster_cache_throttles_per_profile_and_keeps_previous_on_error() {
    let source = Arc::new(FixtureRosterSource::new());
    let clock = Arc::new(ManualClock::new(now()));
    source.push("fixture", Ok(data()));
    source.push("default", Ok(data()));
    source.push("fixture", Err(RosterError::Timeout));
    let cache = RosterCache::new(source.clone(), clock.clone(), Duration::from_secs(30));
    let a = cache.load("fixture").await.unwrap();
    cache.get_or_load("fixture").await.unwrap();
    cache.refresh("fixture", false).await.unwrap();
    assert_eq!(source.fetches().len(), 1);
    cache.load("default").await.unwrap();
    clock.advance(chrono::Duration::seconds(30));
    assert!(cache.refresh("fixture", false).await.is_err());
    assert_eq!(cache.cached("fixture").as_deref(), Some(a.as_ref()));
    assert_eq!(source.fetches().len(), 3);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn staffed_launch_is_pure_and_retains_explicit_defaults() {
    let c = Context::new(data(), 10).await;
    let table = Arc::new(
        StaffingTable::parse("[tiers.judge]\nmodel='m'\nprovider='alpha'\neffort='high'").unwrap(),
    );
    let r = c.cache.cached("fixture").unwrap();
    let explicit = ExplicitLaunch::new(LaunchDefaults {
        profile: "fixture".into(),
        cwd: c._temp.path().canonicalize().unwrap(),
        toolsets: Some(vec!["file".into()]),
        worktrees: None,
    });
    let staffed = StaffedLaunch::new(StaffingResolver::new(table.clone(), r), explicit);
    let mut q = request(0);
    q.model = None;
    q.provider = None;
    q.tier = Some("judge".into());
    q.toolsets = None;
    let a = staffed.resolve(&q).unwrap();
    assert_eq!(a, staffed.resolve(&q).unwrap());
    assert_eq!(a.effort, Some(Effort::High));
    assert_eq!(a.profile, "fixture");
    assert_eq!(a.toolsets, ["file"]);
    let snapshot = c
        .cache
        .cached("fixture")
        .unwrap()
        .snapshot_with(&table, &table.budget);
    assert!(staffing_drift(&snapshot, &table).is_empty());
    let mut changed = (*table).clone();
    changed.tiers.get_mut("judge").unwrap().effort = Some(Effort::Low);
    assert!(
        staffing_drift(&snapshot, &changed)[0]
            .to_string()
            .contains("finished agents on these tiers will run again")
    );
}
type WireCalls = Arc<Mutex<Vec<(String, Value, Duration)>>>;
#[derive(Default)]
struct Wires {
    opened: Mutex<Vec<String>>,
    calls: WireCalls,
}
struct TestWire {
    calls: WireCalls,
}
impl WireFactory for Wires {
    fn open(&self, p: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        self.opened.lock().unwrap().push(p.into());
        let w = Arc::new(TestWire {
            calls: self.calls.clone(),
        });
        Box::pin(async move { Ok(w as Arc<dyn Wire>) })
    }
}
impl Wire for TestWire {
    fn call(
        &self,
        m: &'static str,
        p: Map<String, Value>,
        t: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        let mut calls = self.calls.lock().unwrap();
        calls.push((m.into(), Value::Object(p), t));
        let n = calls.len();
        Box::pin(async move {
            if n == 4 {
                Err(WireError::Refused {
                    code: 4001,
                    message: "profile_mismatch".into(),
                    data: json!({}),
                })
            } else if n == 5 {
                Err(WireError::Refused {
                    code: 5098,
                    message: "old".into(),
                    data: json!({}),
                })
            } else if n == 6 {
                Err(WireError::NotServed(
                    "no profile identifier in transport reason".into(),
                ))
            } else {
                Ok(data())
            }
        })
    }
    fn generation(&self) -> u64 {
        1
    }
    fn track(&self, _: &str, _: Cursor) {}
    fn resync(&self, _: &str, _: Cursor) {}
    fn take_events(&self) -> Option<tokio::sync::mpsc::Receiver<WireEvent>> {
        None
    }
}
#[tokio::test]
async fn roster_source_reuses_profile_bound_wires_and_preserves_rpc_errors() {
    let wires = Arc::new(Wires::default());
    let source = LinkRosterSource::new(wires.clone());
    source.fetch("default").await.unwrap();
    source.fetch("fixture").await.unwrap();
    source.fetch("default").await.unwrap();
    assert_eq!(*wires.opened.lock().unwrap(), ["default", "fixture"]);
    for (m, p, t) in wires.calls.lock().unwrap().iter() {
        assert_eq!(m, "model.options");
        assert_eq!(p, &json!({}));
        assert_eq!(*t, Duration::from_secs(15));
    }
    assert!(matches!(
        source.fetch("default").await,
        Err(RosterError::Rpc { code: 4001, .. })
    ));
    assert!(
        matches!(source.fetch("default").await,Err(RosterError::Rpc{code:5098,message}) if message.contains("operator restarts"))
    );
    assert!(
        matches!(source.fetch("default").await, Err(RosterError::NotServed(profile)) if profile == "default")
    );
}
#[tokio::test]
async fn retry_finish_round_trips_and_keeps_version_one_grammar() {
    let c = Context::new(data(), 10).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    c.record(
        &q,
        &l,
        0,
        vec![(cost(0.0, "unknown"), json!({"failure_reason":"rate_limit"}))],
        true,
    )
    .await;
    c.record(&q, &l, 1, vec![(cost(0.0, "included"), json!({}))], false)
        .await;
    let (records, _) = JournalReader::read_from(c.journal.dir().path(), 0).unwrap();
    let r = records
        .iter()
        .find(|r| matches!(r.record, Record::CallRetryFinished { .. }))
        .unwrap();
    let v = serde_json::to_value(r).unwrap();
    assert_eq!(v["t"], "call_finished");
    assert_eq!(v["retry"], true);
    assert_eq!(serde_json::from_value::<RecordLine>(v).unwrap(), *r);
    assert!(!NoGate.retry_after_failure(
        &q,
        &l,
        &SettledTurn {
            turn: 0,
            outcome: "failed".into(),
            final_text: None,
            result_meta: json!({}),
            usage: None
        },
        0
    ));
}
#[cfg(feature = "fake-gateway")]
mod composed {
    use super::*;
    use crate::{
        engine::{self, MapResolver, RunOutcome},
        hermes::SchemaTurnPolicy,
        journal::fake_gateway::{FakeGateway, ScriptRule},
    };
    use std::collections::BTreeMap;
    #[derive(Clone)]
    struct FailFirst {
        fake: FakeGateway,
        sessions: Arc<Mutex<BTreeMap<String, bool>>>,
        always: bool,
    }
    impl FailFirst {
        fn decorate(&self, mut r: Receipt) -> Receipt {
            if r.status == AdmissionState::Terminal
                && self.sessions.lock().unwrap().get(&r.session_id) == Some(&true)
            {
                r.outcome = Some("failed".into());
                r.result =
                    Some(json!({"failed":true,"failure_reason":"rate_limit","final_response":""}));
            }
            r
        }
    }
    impl GatewayPort for FailFirst {
        fn create_session(
            &self,
            r: CreateSession,
        ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
            let this = self.clone();
            Box::pin(async move {
                let fail = this.always || !r.request_id.ends_with(":a1");
                let bound = this.fake.create_session(r).await?;
                this.sessions
                    .lock()
                    .unwrap()
                    .insert(bound.session_id.clone(), fail);
                Ok(bound)
            })
        }
        fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
            self.fake.attach(p, s)
        }
        fn submit(&self, r: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
            self.fake.submit(r)
        }
        fn receipt(
            &self,
            p: String,
            s: String,
            a: String,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            let this = self.clone();
            Box::pin(async move {
                let r = this.fake.receipt(p, s, a).await?;
                Ok(this.decorate(r))
            })
        }
        fn wait_settled(
            &self,
            p: String,
            s: String,
            a: String,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            let this = self.clone();
            Box::pin(async move {
                let r = this.fake.wait_settled(p, s, a).await?;
                Ok(this.decorate(r))
            })
        }
        fn resolve_unknown(
            &self,
            p: String,
            s: String,
            a: String,
            g: i64,
        ) -> BoxFuture<'static, Result<Receipt, PortError>> {
            self.fake.resolve_unknown(p, s, a, g)
        }
    }
    async fn run(
        c: &Context,
        scheduler: Scheduler,
        port: impl GatewayPort,
        script: &str,
    ) -> RunOutcome {
        let resolver = StaffedLaunch::new(
            StaffingResolver::new(
                Arc::new(c.table.clone()),
                c.cache.cached("fixture").unwrap(),
            ),
            ExplicitLaunch::new(LaunchDefaults {
                profile: "fixture".into(),
                cwd: c._temp.path().canonicalize().unwrap(),
                toolsets: Some(vec!["file".into()]),
                worktrees: None,
            }),
        );
        let cfg = c.journal.engine_config(&resolver).unwrap();
        let backend = JournaledBackend::new(
            c.journal.clone(),
            port,
            SchemaTurnPolicy,
            resolver,
            scheduler,
            RetryPolicy::default(),
        );
        let parsed = engine::parse_script(script).unwrap();
        let handle = engine::spawn(parsed, cfg, Arc::new(backend), Arc::new(MapResolver::new()));
        let forward = c.journal.forward_events(handle.events);
        let mut result = handle.outcome;
        let outcome = loop {
            tokio::select! {r=&mut result=>break r.unwrap(),_=tokio::time::sleep(Duration::from_millis(10))=>{c.clock.advance(chrono::Duration::milliseconds(10));}}
        };
        forward.await.unwrap().unwrap();
        outcome
    }
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn rate_limited_backend_retries_once_and_never_delivers_the_retry_finish() {
        for always in [false, true] {
            let c = Context::new(data(), 10).await;
            c.source.push("fixture", Ok(data()));
            c.source.push("fixture", Ok(data()));
            let (s, mut events) = c.start().await;
            let fake = FakeGateway::new().with_script(vec![ScriptRule {
                duration: Duration::from_millis(1),
                usage: cost(0.0, "included"),
                ..Default::default()
            }]);
            let port = FailFirst {
                fake: fake.clone(),
                sessions: Default::default(),
                always,
            };
            let result=run(&c,s.clone(),port,"export const meta={name:'retry',description:'test',phases:[]}; return await agent('one',{model:'m',provider:'alpha'});").await;
            let value = match result {
                RunOutcome::Completed { result } => result,
                v => panic!("{v:?}"),
            };
            assert_eq!(value, if always { Value::Null } else { json!("ok") });
            assert_eq!(fake.stats().sessions, 2);
            let (lines, _) = JournalReader::read_from(c.journal.dir().path(), 0).unwrap();
            let retries: Vec<_> = lines
                .iter()
                .filter(|l| matches!(l.record, Record::CallRetryFinished { attempt: 0, .. }))
                .collect();
            assert_eq!(retries.len(), 1);
            assert!(lines.iter().any(|l|matches!(&l.record,Record::TurnSettled{result_meta,..} if result_meta["failure_reason"]=="rate_limit")));
            assert!(
                !lines
                    .iter()
                    .any(|l| l.record.identity().is_some_and(|(_, a)| a > 1))
            );
            assert_eq!(s.status().global_width, 5);
            assert!(
                std::iter::from_fn(|| events.0.try_recv().ok())
                    .any(|e| matches!(e.event, SchedulerEvent::RetryAuthorised { .. }))
            );
        }
    }
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn engine_fanout_respects_widths_and_token_budget() {
        let mut c = Context::new(data(), 4).await;
        c.table
            .providers
            .insert("alpha".into(), ProviderLimits { max_in_flight: 3 });
        c.table.budget.tokens = Some(9000);
        let (s, mut events) = c.start().await;
        let fake = FakeGateway::new().with_script(vec![ScriptRule {
            duration: Duration::from_secs(1),
            usage: cost(0.0, "included"),
            ..Default::default()
        }]);
        let result=run(&c,s.clone(),fake.clone(),"export const meta={name:'fanout',description:'test',phases:[]}; return await parallel(Array.from({length:20},(_,i)=>()=>agent('one '+i,{model:'m',provider:i%2?'alpha':'beta'})));").await;
        assert!(matches!(
            result,
            RunOutcome::Completed { .. } | RunOutcome::Failed { .. }
        ));
        assert!(s.status().spend.tokens >= 9000);
        let mut admitted = 0;
        while let Ok(e) = events.0.try_recv() {
            if let SchedulerEvent::Admitted {
                in_flight,
                lane_in_flight,
                provider,
                ..
            } = e.event
            {
                admitted += 1;
                assert!(in_flight <= 4);
                if provider == "alpha" {
                    assert!(lane_in_flight <= 3);
                }
            }
        }
        assert!(admitted < 20);
        assert_eq!(s.status().in_flight, 0);
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn billing_pause_does_not_survive_a_new_scheduler() {
    let c = Context::new(data(), 10).await;
    let l = launch("fixture", "alpha");
    c.record(
        &request(0),
        &l,
        0,
        vec![(cost(0.0, "included"), json!({"failure_reason":"billing"}))],
        false,
    )
    .await;
    let (s, mut events) = c.start().await;
    drop(permit(s.acquire(&request(1), &l, GateReason::Fresh).await));
    assert!(
        !std::iter::from_fn(|| events.0.try_recv().ok())
            .any(|e| matches!(e.event, SchedulerEvent::LanePaused { .. }))
    );
}
#[tokio::test(flavor = "current_thread")]
async fn explicit_model_warns_once_per_ignored_tier() {
    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let c = Context::new(data(), 10).await;
    let output = Arc::new(Mutex::new(Vec::new()));
    let buffer = output.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(move || Capture(buffer.clone()))
        .finish();
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let staffed = StaffedLaunch::new(
        StaffingResolver::new(
            Arc::new(c.table.clone()),
            c.cache.cached("fixture").unwrap(),
        ),
        ExplicitLaunch::new(LaunchDefaults {
            profile: "fixture".into(),
            cwd: c._temp.path().canonicalize().unwrap(),
            toolsets: Some(vec!["file".into()]),
            worktrees: None,
        }),
    );
    let mut q = request(0);
    q.tier = Some("judge".into());
    assert_eq!(staffed.resolve(&q).unwrap(), staffed.resolve(&q).unwrap());
    let text = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert_eq!(
        text.matches("tier 'judge' ignored: the call names a model")
            .count(),
        1
    );
}
#[test]
fn retry_wire_uses_only_the_existing_finished_record_tag() {
    let record = json!({"t":"call_retry_finished", "key":"a".repeat(32), "attempt":0, "outcome":{"kind":"failed","reason":"rate_limit"},"turns":1});
    assert!(serde_json::from_value::<Record>(record).is_err());
}
