use futures_util::future::BoxFuture;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::pending,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use super::harness::*;
use talaria_workflow::{
    engine::{self, AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, MapResolver},
    journal::{
        AdmissionGate, CreateSession, GateDecision, GatePermit, GateReason, GatewayPort,
        JournalNotice, JournaledBackend, PortError, Receipt, Resolution, ResolvedLaunch,
        RetryPolicy, RunJournal, SessionBound, SubmitTurn,
        fake_gateway::{FakeGateway, ScriptRule},
    },
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct FaultPort {
    inner: FakeGateway,
    point: u8,
    target: String,
    reached: Arc<tokio::sync::Notify>,
    labels: Arc<Mutex<BTreeMap<String, String>>>,
    resolved: Arc<AtomicBool>,
}
impl FaultPort {
    fn new(inner: FakeGateway, point: u8, target: &str, reached: Arc<tokio::sync::Notify>) -> Self {
        Self {
            inner,
            point,
            target: target.into(),
            reached,
            labels: Arc::new(Mutex::new(BTreeMap::new())),
            resolved: Arc::new(AtomicBool::new(false)),
        }
    }
    fn set_labels(&self, harness: &Harness) {
        let records = harness.records();
        let mut by_key = BTreeMap::new();
        for line in &records {
            if let talaria_workflow::journal::Record::CallIntent { key, label, .. } = &line.record {
                by_key.insert(key.clone(), label.clone());
            }
        }
        for line in &records {
            if let talaria_workflow::journal::Record::SessionBound {
                key, session_id, ..
            } = &line.record
                && let Some(label) = by_key.get(key)
            {
                self.labels
                    .lock()
                    .unwrap()
                    .insert(session_id.clone(), label.clone());
            }
        }
    }
    fn matches(&self, session: &str) -> bool {
        self.labels.lock().unwrap().get(session) == Some(&self.target)
    }
    async fn pause(reached: Arc<tokio::sync::Notify>) {
        reached.notify_one();
        pending::<()>().await;
    }
}
impl GatewayPort for FaultPort {
    fn create_session(
        &self,
        request: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let target = request.label == this.target;
            if target && this.point == 2 {
                Self::pause(this.reached.clone()).await;
            }
            let answer = this.inner.create_session(request.clone()).await;
            if let Ok(bound) = &answer {
                this.labels
                    .lock()
                    .unwrap()
                    .insert(bound.session_id.clone(), request.label);
            }
            if target && this.point == 3 && answer.is_ok() {
                Self::pause(this.reached.clone()).await;
            }
            answer
        })
    }
    fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
        self.inner.attach(p, s)
    }
    fn submit(&self, request: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let target = this.matches(&request.session_id);
            let correction = request.submission_id.ends_with(":1");
            if target && ((this.point == 5 && !correction) || (this.point == 10 && correction)) {
                Self::pause(this.reached.clone()).await;
            }
            let answer = this.inner.submit(request).await;
            if target && this.point == 6 && !correction && answer.is_ok() {
                Self::pause(this.reached.clone()).await;
            }
            answer
        })
    }
    fn receipt(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let target = this.matches(&s);
            let answer = this.inner.receipt(p, s, a).await;
            if target && this.point == 17 && this.resolved.load(Ordering::SeqCst) && answer.is_ok()
            {
                Self::pause(this.reached.clone()).await;
            }
            answer
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
            let target = this.matches(&s);
            if target && (this.point == 7 || this.point == 13) {
                Self::pause(this.reached.clone()).await;
            }
            let answer = this.inner.wait_settled(p, s, a).await;
            if target && this.point == 8 && answer.is_ok() {
                Self::pause(this.reached.clone()).await;
            }
            answer
        })
    }
    fn resolve_unknown(
        &self,
        p: String,
        s: String,
        a: String,
        g: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let target = this.matches(&s);
            let answer = this.inner.resolve_unknown(p, s, a, g).await;
            if target && answer.is_ok() {
                this.resolved.store(true, Ordering::SeqCst);
                if this.point == 16 {
                    Self::pause(this.reached.clone()).await;
                }
            }
            answer
        })
    }
}
#[derive(Clone)]
struct FaultGate {
    point: u8,
    target: String,
    reached: Arc<tokio::sync::Notify>,
    count: Arc<AtomicUsize>,
}
impl AdmissionGate for FaultGate {
    fn acquire(
        &self,
        request: &AgentRequest,
        _: &ResolvedLaunch,
        reason: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        let this = self.clone();
        let target = request.label == this.target;
        this.count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if target && this.point == 19 {
                return GateDecision::Skip;
            }
            if target && reason == GateReason::Fresh && (this.point == 1 || this.point == 18) {
                let guard = GatePermit::noop();
                this.reached.notify_one();
                pending::<()>().await;
                drop(guard);
            }
            GateDecision::Proceed(GatePermit::noop())
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
}
struct OutcomePause {
    inner: Arc<dyn AgentBackend>,
    point: u8,
    target: String,
    reached: Arc<tokio::sync::Notify>,
}
impl AgentBackend for OutcomePause {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        let future = self.inner.run_agent(request.clone());
        let reached = self.reached.clone();
        let should_pause = request.label == self.target && (self.point == 12 || self.point == 19);
        Box::pin(async move {
            let outcome = future.await;
            if should_pause {
                reached.notify_one();
                pending::<()>().await;
            }
            outcome
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        self.inner.budget()
    }
}

fn plan(items: usize, pipeline: bool, schema: bool, target: usize) -> (String, Vec<ScriptRule>) {
    let mut rules = Vec::new();
    for i in 0..items {
        let label = format!("item-{i}");
        if schema && i == target {
            let mut correction = rule(&label, r#"{"ok":true}"#, 1, None);
            correction.text_contains = Some("Return valid JSON".into());
            rules.push(correction);
            let mut first = rule(&label, "invalid", 10, None);
            first.text_contains = Some(format!("task {i}"));
            rules.push(first);
        } else {
            rules.push(rule(&label, &format!("ok-{i}"), 10, None));
        }
    }
    let call = format!(
        "agent('task '+i,{{label:'item-'+i, ...(i==={target} && {schema} ? {{schema:{{type:'object',properties:{{ok:{{type:'boolean'}}}}}}}} : {{}})}})"
    );
    let body = if pipeline {
        format!(
            "return await pipeline(Array.from({{length:{items}}},(_,i)=>i),async (_,i)=>await {call});"
        )
    } else {
        format!("return await parallel(Array.from({{length:{items}}},(_,i)=>()=>{call}));")
    };
    (source(&body), rules)
}

async fn start_and_drop(
    harness: &Harness,
    script: &str,
    point: u8,
    target: &str,
    resume_resolution: bool,
) -> Option<Resolution> {
    let (journal, _, _) = RunJournal::open(harness.dir.clone(), script).await.unwrap();
    let journal = Arc::new(journal);
    let mut notices = journal.notices();
    let config = journal.engine_config(&harness.resolver).unwrap();
    let reached = Arc::new(tokio::sync::Notify::new());
    let port = FaultPort::new(harness.gateway.clone(), point, target, reached.clone());
    if resume_resolution {
        port.set_labels(harness);
    }
    let gate = FaultGate {
        point,
        target: target.into(),
        reached: reached.clone(),
        count: Arc::new(AtomicUsize::new(0)),
    };
    let target_key = Arc::new(Mutex::new(None::<String>));
    if resume_resolution {
        for line in harness.records() {
            if let talaria_workflow::journal::Record::CallIntent { key, label, .. } = line.record
                && label == target
            {
                *target_key.lock().unwrap() = Some(key);
            }
        }
    }
    let key_slot = target_key.clone();
    let signal = reached.clone();
    let target_label = target.to_owned();
    journal.observe_appends(move |record| {
        use talaria_workflow::journal::{CallOutcome, Record};
        if let Record::CallIntent { key, label, .. } = record
            && *label == target_label
        {
            *key_slot.lock().unwrap() = Some(key.clone());
        }
        let is_target = match record {
            Record::CallIntent { key, .. }
            | Record::SessionBound { key, .. }
            | Record::TurnIntent { key, .. }
            | Record::TurnAdmitted { key, .. }
            | Record::TurnSettled { key, .. }
            | Record::CallResolved { key, .. }
            | Record::CallFinished { key, .. } => key_slot.lock().unwrap().as_ref() == Some(key),
            _ => false,
        };
        let hit = is_target
            && match record {
                Record::CallIntent { .. } => point == 2,
                Record::SessionBound { .. } => point == 4,
                Record::TurnIntent { turn: 0, .. } => point == 5,
                Record::TurnIntent { turn: 1, .. } => point == 10,
                Record::TurnAdmitted { turn: 0, .. } => point == 7 || point == 13,
                Record::TurnSettled { turn: 0, .. } => point == 9,
                Record::TurnSettled { turn: 1, .. } => point == 11,
                Record::CallFinished {
                    outcome: CallOutcome::Skipped,
                    ..
                } => point == 12 || point == 19,
                Record::CallFinished { .. } => point == 12,
                Record::CallResolved { .. } => point == 15,
                _ => false,
            };
        let signal = signal.clone();
        Box::pin(async move {
            if hit {
                signal.notify_one();
                pending::<()>().await;
            }
        })
    });
    let core = Arc::new(JournaledBackend::new(
        journal.clone(),
        port,
        TestPolicy,
        harness.resolver.clone(),
        gate,
        RetryPolicy::default(),
    ));
    let backend = Arc::new(OutcomePause {
        inner: core,
        point,
        target: target.into(),
        reached: reached.clone(),
    });
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let forward = journal.forward_events(rx);
    let future = engine::run(
        engine::parse_script(script).unwrap(),
        config,
        backend.clone(),
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    );
    let mut execution = Box::pin(future);
    let mut resolution = None;
    let deadline = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            outcome=&mut execution => panic!("execution ended before checkpoint {point}: {outcome:?}"),
            _=reached.notified()=>{break;}
            _=&mut deadline=>panic!("checkpoint {point} was not reached"),
            notice=notices.recv(), if resume_resolution => {if let Ok(JournalNotice::NeedsOperator{key,attempt,episode,..})=notice {
                journal.dir().request_resolution(&key,attempt,episode,Resolution::Rerun).unwrap();resolution=Some(Resolution::Rerun);
                if point==14 {break;}
            }}
        }
    }
    if point == 7 || point == 13 {
        use talaria_workflow::journal::{AdmissionState, Record};
        let key = target_key.lock().unwrap().clone().unwrap();
        let records = harness.records();
        let session = records
            .iter()
            .find_map(|line| match &line.record {
                Record::SessionBound {
                    key: k, session_id, ..
                } if *k == key => Some(session_id.clone()),
                _ => None,
            })
            .unwrap();
        let admission = records
            .iter()
            .find_map(|line| match &line.record {
                Record::TurnAdmitted {
                    key: k,
                    admission_id,
                    ..
                } if *k == key => Some(admission_id.clone()),
                _ => None,
            })
            .unwrap();
        let mut started = false;
        for _ in 0..100 {
            let receipt = harness
                .gateway
                .receipt("fixture".into(), session.clone(), admission.clone())
                .await
                .unwrap();
            if receipt.status == AdmissionState::Started {
                started = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert!(started, "the held turn never started");
    }
    if point == 15 {
        assert!(harness.records().iter().any(|r| matches!(
            r.record,
            talaria_workflow::journal::Record::CallResolved { .. }
        )));
        assert!(
            std::fs::read_dir(harness.path().join("resolutions"))
                .unwrap()
                .any(|e| e.unwrap().path().extension().is_some_and(|s| s == "json"))
        );
    }
    if point == 14 || point == 15 {
        journal.stop_resolution_poller();
    }
    drop(execution);
    drop(backend);
    forward.abort();
    let _ = forward.await;
    drop(journal);
    resolution
}

pub async fn run_case(
    point: u8,
    items: usize,
    pipeline: bool,
    schema: bool,
    target: usize,
) -> (usize, usize, usize) {
    let (items, target) = if (13..=18).contains(&point) {
        (1, 0)
    } else {
        (items, target)
    };
    let schema = schema || point == 9 || point == 10 || point == 11;
    let (script, rules) = plan(items, pipeline, schema, target);
    let label = format!("item-{target}");
    let mut reference = Harness::new(&script);
    reference.gateway = FakeGateway::new().with_script(rules.clone());
    let expected = if point == 19 {
        let gate = FaultGate {
            point,
            target: label.clone(),
            reached: Arc::new(tokio::sync::Notify::new()),
            count: Arc::new(AtomicUsize::new(0)),
        };
        result(reference.run_with(&script, gate).await.outcome)
    } else {
        result(reference.run(&script).await.outcome)
    };
    let mut candidate_rules = rules;
    if point == 7 || (13..=18).contains(&point) {
        for rule in &mut candidate_rules {
            if rule.label.as_deref() == Some(label.as_str()) {
                rule.hold = Some("hold".into());
            }
        }
    }
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(candidate_rules);
    if point <= 12 || point == 19 {
        start_and_drop(&harness, &script, point, &label, false).await;
        if point == 7 {
            harness.gateway.release("hold");
        }
    } else {
        start_and_drop(&harness, &script, 13, &label, false).await;
        tokio::time::advance(Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        harness.gateway.restart();
        harness.gateway.release("hold");
        if point == 13 {
            let (run, notice) = harness
                .run_parked(&script, Some(Resolution::Rerun), Duration::ZERO)
                .await;
            assert!(matches!(notice, JournalNotice::NeedsOperator { .. }));
            assert_eq!(result(run.outcome), expected);
            return assert_invariants(&harness, true);
        }
        start_and_drop(&harness, &script, point, &label, true).await;
    }
    let resumed = harness.run(&script).await;
    assert_eq!(result(resumed.outcome), expected);
    assert_invariants(&harness, point >= 13)
}
fn assert_invariants(harness: &Harness, allow_rerun: bool) -> (usize, usize, usize) {
    use talaria_workflow::journal::Record;
    let mut sessions: BTreeMap<(String, u32), BTreeSet<String>> = BTreeMap::new();
    let mut admissions: BTreeMap<(String, u32, u32), BTreeSet<String>> = BTreeMap::new();
    let mut attempts = BTreeSet::new();
    let mut rerun_keys = BTreeSet::new();
    for line in harness.records() {
        match line.record {
            Record::SessionBound {
                key,
                attempt,
                session_id,
                ..
            } => {
                sessions
                    .entry((key.clone(), attempt))
                    .or_default()
                    .insert(session_id);
                attempts.insert((key, attempt));
            }
            Record::TurnAdmitted {
                key,
                attempt,
                turn,
                admission_id,
                ..
            } => {
                admissions
                    .entry((key, attempt, turn))
                    .or_default()
                    .insert(admission_id);
            }
            Record::CallResolved {
                key,
                resolution: Resolution::Rerun,
                ..
            } => {
                rerun_keys.insert(key);
            }
            _ => {}
        }
    }
    assert!(sessions.values().all(|ids| ids.len() == 1));
    assert!(admissions.values().all(|ids| ids.len() == 1));
    assert!(attempts.iter().all(|(key, attempt)| *attempt == 0
        || (allow_rerun && *attempt == 1 && rerun_keys.contains(key))));
    let stats = harness.gateway.stats();
    assert_eq!(stats.sessions as usize, sessions.len());
    assert_eq!(stats.admissions as usize, admissions.len());
    assert!(stats.admissions_by_session.values().all(|ids| {
        let unique = ids.iter().collect::<BTreeSet<_>>();
        unique.len() == ids.len() && ids.len() <= 2
    }));
    let session_max = sessions.values().map(BTreeSet::len).max().unwrap_or(0);
    let admission_max = admissions.values().map(BTreeSet::len).max().unwrap_or(0);
    let retry_max = rerun_keys
        .iter()
        .map(|key| attempts.iter().filter(|(k, a)| k == key && *a > 0).count())
        .max()
        .unwrap_or(0);
    assert!(retry_max <= 1);
    (session_max, admission_max, retry_max)
}
