#![allow(dead_code)]
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::{
    engine::{AgentRequest, CallId},
    journal::{
        AdmissionGate, GateDecision, GatePermit, GateReason, Record, RecordLine, ResolvedLaunch,
        journal_key,
    },
    schedule::{Scheduler, SchedulerConfig, SchedulerEvents},
    staffing::{
        FixtureRosterSource, ManualClock, PriceBook, RosterCache, RunBudget, StaffingTable,
    },
};
pub fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
        .unwrap()
        .to_utc()
}
pub fn roster_data() -> Value {
    json!({"model":"m","provider":"alpha","providers":[{"slug":"alpha","name":"Alpha","authenticated":true,"models":["m"],"pricing":{"m":{"input":"$3","output":"$15","free":false}}},{"slug":"beta","name":"Beta","authenticated":true,"models":["m"]}]})
}
pub fn request(n: u32) -> AgentRequest {
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
pub fn launch(profile: &str, provider: &str) -> ResolvedLaunch {
    ResolvedLaunch {
        profile: profile.into(),
        model: "m".into(),
        provider: Some(provider.into()),
        effort: None,
        toolsets: vec!["file".into()],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}
pub fn permit(decision: GateDecision) -> GatePermit {
    match decision {
        GateDecision::Proceed(p) => p,
        _ => panic!("expected permit"),
    }
}
pub struct Harness {
    pub root: tempfile::TempDir,
    pub path: PathBuf,
    pub clock: Arc<ManualClock>,
    pub source: Arc<FixtureRosterSource>,
    pub cache: RosterCache,
    pub table: StaffingTable,
    pub prices: PriceBook,
}
impl Harness {
    pub async fn new(width: u32, data: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("journal.jsonl");
        std::fs::write(&path, "").unwrap();
        let clock = Arc::new(ManualClock::new(now()));
        let source = Arc::new(FixtureRosterSource::with_clock(clock.clone()));
        source.push("fixture", Ok(data));
        let cache = RosterCache::new(source.clone(), clock.clone(), Duration::from_secs(30));
        let roster = cache.load("fixture").await.unwrap();
        let table = StaffingTable::parse(&format!("[limits]\nmax_in_flight={width}\n")).unwrap();
        let snapshot = roster.snapshot_with(&table, &table.budget);
        let prices = PriceBook::from_snapshot(&snapshot);
        Self {
            root,
            path,
            clock,
            source,
            cache,
            table,
            prices,
        }
    }
    pub async fn start(&self, budget: RunBudget) -> (Scheduler, SchedulerEvents) {
        Scheduler::start(
            SchedulerConfig::from_table(&self.table, budget, "fixture", self.prices.clone()),
            self.cache.clone(),
            self.path.clone(),
            self.clock.clone(),
        )
        .await
        .unwrap()
    }
    pub async fn advance(&self, seconds: i64) {
        self.clock.advance(chrono::Duration::seconds(seconds));
        tokio::time::advance(Duration::from_secs(seconds as u64)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
    }
    pub fn append(&self, record: Record) {
        use std::io::Write;
        let bytes = std::fs::read(&self.path).unwrap();
        let seq = bytes.iter().filter(|&&b| b == b'\n').count() as u64 + 1;
        let line = RecordLine {
            v: 1,
            seq,
            at_ms: now().timestamp_millis(),
            record,
        };
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .unwrap();
        writeln!(file, "{}", serde_json::to_string(&line).unwrap()).unwrap();
        file.sync_all().unwrap();
    }
    pub fn settle(
        &self,
        q: &AgentRequest,
        l: &ResolvedLaunch,
        attempt: u32,
        turn: u32,
        reason: Option<&str>,
        usage: Value,
    ) {
        let key = journal_key(&q.call_id, l);
        if turn == 0 {
            self.append(Record::CallIntent {
                key: key.clone(),
                attempt,
                call_id: q.call_id.clone(),
                label: q.label.clone(),
                phase: None,
                workflow_path: vec![],
                launch: l.clone(),
                request_id: format!("request-{attempt}"),
            });
        }
        self.append(Record::TurnSettled {
            key,
            attempt,
            turn,
            admission_id: format!("admission-{attempt}-{turn}"),
            outcome: if reason.is_some() {
                "failed"
            } else {
                "completed"
            }
            .into(),
            final_text: Some("ok".into()),
            final_text_ref: None,
            result_meta: reason.map_or(json!({}), |s| json!({"failure_reason":s})),
            usage: Some(usage),
        });
    }
}
pub fn no_budget() -> RunBudget {
    RunBudget {
        tokens: None,
        money_usd: None,
    }
}
pub async fn acquire(s: &Scheduler, n: u32, profile: &str, provider: &str) -> GatePermit {
    permit(
        s.acquire(&request(n), &launch(profile, provider), GateReason::Fresh)
            .await,
    )
}
