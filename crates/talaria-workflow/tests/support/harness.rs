use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use talaria_workflow::{
    engine::{
        self, AgentOutcome, AgentRequest, EngineEvent, MapResolver, RunOutcome, RunOutcomeKind,
    },
    journal::{
        AdmissionGate, JournalReader, JournaledBackend, LaunchResolver, NewManifest, NextStep,
        NoGate, OnUnknown, RecordLine, ResolvedLaunch, RetryPolicy, RunDir, RunJournal,
        SettledTurn, TurnPolicy, fake_gateway::FakeGateway,
    },
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

pub const SCRIPT: &str = include_str!("../scripts/three_reviewers.js");
pub const HEADER: &str = "export const meta = {name:'fixture',description:'fixture'};\n";
pub fn source(body: &str) -> String {
    format!("{HEADER}{body}")
}
pub fn manifest() -> NewManifest {
    NewManifest {
        workflow_name: "fixture".into(),
        args: None,
        started_at_ms: 1234,
        seed: 7,
        profile: "fixture".into(),
        on_unknown: OnUnknown::Wait,
        talaria_version: "0.7.0".into(),
        hermes: json!({}),
        roster: json!({}),
        approval_policy: json!({}),
    }
}
pub fn new_dir() -> (TempDir, RunDir) {
    let temp = tempfile::tempdir().unwrap();
    let dir = RunDir::create(temp.path(), manifest(), SCRIPT).unwrap();
    (temp, dir)
}
pub fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: "fixture".into(),
        model: "fixture-model".into(),
        provider: None,
        effort: None,
        toolsets: vec!["web".into()],
        skills: vec!["skill".into()],
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}
#[derive(Clone, Default)]
pub struct TestResolver {
    pub tiers: BTreeMap<String, String>,
    pub cwd: Option<String>,
}
impl LaunchResolver for TestResolver {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String> {
        let mut result = launch();
        if let Some(model) = &request.model {
            result.model = model.clone();
        } else if let Some(tier) = &request.tier
            && let Some(model) = self.tiers.get(tier)
        {
            result.model = model.clone();
        }
        result.profile = request.profile.clone().unwrap_or_else(|| "fixture".into());
        result.provider = request.provider.clone();
        result.effort = request.effort;
        result.toolsets = request.toolsets.clone().unwrap_or_default();
        result.skills = request.skills.clone().unwrap_or_default();
        result.max_turns = request.max_turns;
        result.isolation = request.isolation;
        result.cwd = self.cwd.clone();
        Ok(result)
    }
}
#[derive(Clone)]
pub struct TestPolicy;
impl TurnPolicy for TestPolicy {
    fn first_turn(&self, request: &AgentRequest, _launch: &ResolvedLaunch) -> String {
        request.prompt.clone()
    }
    fn after_turn(&self, request: &AgentRequest, turns: &[SettledTurn]) -> NextStep {
        let last = turns.last().unwrap();
        if last.outcome != "completed" {
            return NextStep::Finish(AgentOutcome::Failed {
                reason: last.outcome.clone(),
            });
        }
        let text = last.final_text.clone().unwrap_or_default();
        if request.schema.is_some() {
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => NextStep::Finish(AgentOutcome::Completed(value)),
                Err(_) if turns.len() == 1 => NextStep::Turn {
                    text: "Return valid JSON".into(),
                },
                Err(_) => NextStep::Finish(AgentOutcome::Failed {
                    reason: "invalid JSON".into(),
                }),
            }
        } else {
            NextStep::Finish(AgentOutcome::Completed(json!(text)))
        }
    }
}
pub struct Harness {
    pub temp: TempDir,
    pub dir: RunDir,
    pub gateway: FakeGateway,
    pub resolver: TestResolver,
}
pub struct RunData {
    pub outcome: RunOutcome,
    pub events: Vec<EngineEvent>,
    pub summary: talaria_workflow::journal::ReplaySummary,
    pub notices: Vec<talaria_workflow::journal::JournalNotice>,
}
impl Harness {
    pub fn new(script: &str) -> Self {
        Self::new_with(script, manifest())
    }
    pub fn new_with(script: &str, data: NewManifest) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let dir = RunDir::create(temp.path(), data, script).unwrap();
        Self {
            temp,
            dir,
            gateway: FakeGateway::new(),
            resolver: TestResolver::default(),
        }
    }
    pub async fn run(&self, script: &str) -> RunData {
        self.run_with(script, NoGate).await
    }
    pub async fn run_with<G: AdmissionGate>(&self, script: &str, gate: G) -> RunData {
        self.run_port(script, self.gateway.clone(), gate).await
    }
    pub async fn run_port<P: talaria_workflow::journal::GatewayPort, G: AdmissionGate>(
        &self,
        script: &str,
        port: P,
        gate: G,
    ) -> RunData {
        let (journal, _, _) = RunJournal::open(self.dir.clone(), script).await.unwrap();
        let journal = Arc::new(journal);
        let mut notices = journal.notices();
        let config = journal.engine_config(&self.resolver).unwrap();
        let backend = Arc::new(JournaledBackend::new(
            journal.clone(),
            port,
            TestPolicy,
            self.resolver.clone(),
            gate,
            RetryPolicy::default(),
        ));
        let (engine_tx, mut engine_rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
        let (journal_tx, journal_rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
        let forward = journal.forward_events(journal_rx);
        let collect = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(event) = engine_rx.recv().await {
                journal_tx.send(event.clone()).unwrap();
                events.push(event);
            }
            events
        });
        let parsed = engine::parse_script(script).unwrap();
        let outcome = engine::run(
            parsed,
            config,
            backend,
            Arc::new(MapResolver::new()),
            engine_tx,
            CancellationToken::new(),
        )
        .await;
        let events = collect.await.unwrap();
        forward.await.unwrap().unwrap();
        let kind = match outcome {
            RunOutcome::Completed { .. } => RunOutcomeKind::Completed,
            RunOutcome::Failed { .. } => RunOutcomeKind::Failed,
            RunOutcome::Aborted { .. } => RunOutcomeKind::Aborted,
        };
        journal.finish(kind).await.unwrap();
        let summary = journal.summary();
        let mut observed = Vec::new();
        while let Ok(notice) = notices.try_recv() {
            observed.push(notice);
        }
        RunData {
            outcome,
            events,
            summary,
            notices: observed,
        }
    }
    pub fn records(&self) -> Vec<RecordLine> {
        JournalReader::read_from(&self.dir.path().join("journal.jsonl"), 0)
            .unwrap()
            .0
    }
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}
pub fn records_path(dir: &RunDir) -> PathBuf {
    dir.path().join("journal.jsonl")
}
pub fn elapsed(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
pub fn result(outcome: RunOutcome) -> Value {
    match outcome {
        RunOutcome::Completed { result } => result,
        other => panic!("expected completed: {other:?}"),
    }
}

pub fn call_id() -> talaria_workflow::engine::CallId {
    talaria_workflow::engine::CallId {
        digest: "0123456789abcdef".repeat(4),
        occurrence: 2,
    }
}
pub fn key() -> String {
    "a".repeat(32)
}
pub fn line(seq: u64, record: talaria_workflow::journal::Record) -> RecordLine {
    RecordLine {
        v: 1,
        seq,
        at_ms: 0,
        record,
    }
}
pub fn write_records(dir: &RunDir, records: &[talaria_workflow::journal::Record]) {
    use std::io::Write;
    let mut f = std::fs::File::create(records_path(dir)).unwrap();
    for (i, record) in records.iter().enumerate() {
        let mut record = record.clone();
        match &mut record {
            talaria_workflow::journal::Record::CallIntent {
                key,
                attempt,
                request_id,
                ..
            } => *request_id = talaria_workflow::journal::request_id(dir.run_id(), key, *attempt),
            talaria_workflow::journal::Record::TurnIntent {
                key,
                attempt,
                turn,
                submission_id,
                ..
            } => {
                *submission_id =
                    talaria_workflow::journal::submission_id(dir.run_id(), key, *attempt, *turn)
            }
            _ => {}
        }
        writeln!(
            f,
            "{}",
            serde_json::to_string(&line(i as u64 + 1, record)).unwrap()
        )
        .unwrap();
    }
}
pub fn start_record(dir: &RunDir) -> talaria_workflow::journal::Record {
    talaria_workflow::journal::Record::RunStarted {
        run_id: dir.run_id().as_str().into(),
        script_sha256: dir.manifest().workflow.script_sha256.clone(),
    }
}
pub fn request() -> AgentRequest {
    AgentRequest {
        call_id: call_id(),
        label: "label".into(),
        phase: Some("phase".into()),
        prompt: "review".into(),
        schema: None,
        tier: None,
        model: Some("fixture-model".into()),
        provider: None,
        effort: None,
        profile: Some("fixture".into()),
        toolsets: Some(vec!["web".into()]),
        skills: Some(vec!["skill".into()]),
        isolation: None,
        max_turns: None,
        workflow_path: vec![],
    }
}
pub fn rule(
    label: &str,
    text: &str,
    delay_ms: u64,
    hold: Option<&str>,
) -> talaria_workflow::journal::fake_gateway::ScriptRule {
    talaria_workflow::journal::fake_gateway::ScriptRule {
        label: Some(label.into()),
        model: None,
        text_contains: None,
        outcome: talaria_workflow::journal::fake_gateway::FakeOutcome::Completed,
        final_text: text.into(),
        usage: json!({"total_tokens":1}),
        duration: elapsed(delay_ms),
        hold: hold.map(str::to_owned),
        settlement_lost: false,
    }
}
impl Harness {
    pub async fn crash_after_finished(&self, script: &str, count: usize) -> Vec<EngineEvent> {
        self.crash_when(script, |events| {
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
                >= count
        })
        .await
    }
    pub async fn crash_when<F: Fn(&[EngineEvent]) -> bool>(
        &self,
        script: &str,
        predicate: F,
    ) -> Vec<EngineEvent> {
        let (journal, _, _) = RunJournal::open(self.dir.clone(), script).await.unwrap();
        let journal = Arc::new(journal);
        let config = journal.engine_config(&self.resolver).unwrap();
        let backend = Arc::new(JournaledBackend::new(
            journal.clone(),
            self.gateway.clone(),
            TestPolicy,
            self.resolver.clone(),
            NoGate,
            RetryPolicy::default(),
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
        let parsed = engine::parse_script(script).unwrap();
        let mut execution = Box::pin(engine::run(
            parsed,
            config,
            backend.clone(),
            Arc::new(MapResolver::new()),
            tx,
            CancellationToken::new(),
        ));
        let mut events = Vec::new();
        loop {
            tokio::select! {
                outcome=&mut execution => panic!("run ended before crash: {outcome:?}"),
                event=rx.recv() => {let event=event.expect("event stream closed");journal.record_engine_event(&event).await.unwrap();events.push(event);if predicate(&events) {break;}}
                _=tokio::time::sleep(Duration::from_millis(1)) => {if predicate(&events) {break;}}
            }
        }
        drop(execution);
        drop(backend);
        drop(journal);
        events
    }
}
#[derive(Clone)]
pub struct TracePort {
    pub inner: FakeGateway,
    pub requests: Arc<std::sync::Mutex<Vec<Value>>>,
}
impl TracePort {
    pub fn new(inner: FakeGateway) -> Self {
        Self {
            inner,
            requests: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
    fn record<T: serde::Serialize>(&self, method: &str, params: &T) {
        self.requests
            .lock()
            .unwrap()
            .push(json!({"method":method,"params":params}));
    }
}
impl talaria_workflow::journal::GatewayPort for TracePort {
    fn create_session(
        &self,
        req: talaria_workflow::journal::CreateSession,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::SessionBound, talaria_workflow::journal::PortError>,
    > {
        self.record("create_session", &req);
        self.inner.create_session(req)
    }
    fn attach(
        &self,
        profile: String,
        session_id: String,
    ) -> futures_util::future::BoxFuture<'static, Result<(), talaria_workflow::journal::PortError>>
    {
        self.record(
            "attach",
            &json!({"profile":profile,"session_id":session_id}),
        );
        self.inner.attach(profile, session_id)
    }
    fn submit(
        &self,
        req: talaria_workflow::journal::SubmitTurn,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.record("submit", &req);
        self.inner.submit(req)
    }
    fn receipt(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.record(
            "receipt",
            &json!({"profile":profile,"session_id":session_id,"admission_id":admission_id}),
        );
        self.inner.receipt(profile, session_id, admission_id)
    }
    fn wait_settled(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.record(
            "wait_settled",
            &json!({"profile":profile,"session_id":session_id,"admission_id":admission_id}),
        );
        self.inner.wait_settled(profile, session_id, admission_id)
    }
    fn resolve_unknown(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
        generation: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        self.record("resolve_unknown",&json!({"profile":profile,"session_id":session_id,"admission_id":admission_id,"execution_generation":generation}));
        self.inner
            .resolve_unknown(profile, session_id, admission_id, generation)
    }
}
#[derive(Clone)]
pub struct PauseSubmit {
    pub inner: FakeGateway,
    pub reached: Arc<tokio::sync::Notify>,
    pub sent: Arc<std::sync::Mutex<Vec<talaria_workflow::journal::SubmitTurn>>>,
}
impl PauseSubmit {
    pub fn new(inner: FakeGateway) -> Self {
        Self {
            inner,
            reached: Arc::new(tokio::sync::Notify::new()),
            sent: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}
impl talaria_workflow::journal::GatewayPort for PauseSubmit {
    fn create_session(
        &self,
        req: talaria_workflow::journal::CreateSession,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::SessionBound, talaria_workflow::journal::PortError>,
    > {
        self.inner.create_session(req)
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
        req: talaria_workflow::journal::SubmitTurn,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        let inner = self.inner.clone();
        let reached = self.reached.clone();
        let sent = self.sent.clone();
        Box::pin(async move {
            sent.lock().unwrap().push(req.clone());
            let _result = inner.submit(req).await;
            reached.notify_one();
            std::future::pending().await
        })
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
        self.inner.receipt(p, s, a)
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
        self.inner.wait_settled(p, s, a)
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
impl Harness {
    pub async fn crash_after_submit(&self, script: &str) -> PauseSubmit {
        let (journal, _, _) = RunJournal::open(self.dir.clone(), script).await.unwrap();
        let journal = Arc::new(journal);
        let config = journal.engine_config(&self.resolver).unwrap();
        let port = PauseSubmit::new(self.gateway.clone());
        let wait = port.reached.clone();
        let backend = Arc::new(JournaledBackend::new(
            journal.clone(),
            port.clone(),
            TestPolicy,
            self.resolver.clone(),
            NoGate,
            RetryPolicy::default(),
        ));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let forward = journal.forward_events(rx);
        let parsed = engine::parse_script(script).unwrap();
        let mut execution = Box::pin(engine::run(
            parsed,
            config,
            backend.clone(),
            Arc::new(MapResolver::new()),
            tx,
            CancellationToken::new(),
        ));
        tokio::select! {outcome=&mut execution => panic!("run ended before crash: {outcome:?}"),_=wait.notified()=>{}}
        drop(execution);
        drop(backend);
        forward.abort();
        let _ = forward.await;
        drop(journal);
        port
    }
}
impl Harness {
    pub async fn run_parked(
        &self,
        script: &str,
        answer: Option<talaria_workflow::journal::Resolution>,
        wait: Duration,
    ) -> (RunData, talaria_workflow::journal::JournalNotice) {
        use talaria_workflow::journal::JournalNotice;
        let (journal, _, _) = RunJournal::open(self.dir.clone(), script).await.unwrap();
        let journal = Arc::new(journal);
        let mut notices = journal.notices();
        let config = journal.engine_config(&self.resolver).unwrap();
        let backend = Arc::new(JournaledBackend::new(
            journal.clone(),
            self.gateway.clone(),
            TestPolicy,
            self.resolver.clone(),
            NoGate,
            RetryPolicy::default(),
        ));
        let (engine_tx, mut engine_rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
        let (journal_tx, journal_rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
        let forward = journal.forward_events(journal_rx);
        let collect = tokio::spawn(async move {
            let mut events = Vec::new();
            while let Some(e) = engine_rx.recv().await {
                journal_tx.send(e.clone()).unwrap();
                events.push(e)
            }
            events
        });
        let mut execution = Box::pin(engine::run(
            engine::parse_script(script).unwrap(),
            config,
            backend,
            Arc::new(MapResolver::new()),
            engine_tx,
            CancellationToken::new(),
        ));
        let notice = loop {
            tokio::select! {out=&mut execution=>panic!("ended before parking: {out:?}"),notice=notices.recv()=>{let notice=notice.unwrap();if matches!(notice,JournalNotice::NeedsOperator{..}) {break notice}}}
        };
        if wait > Duration::ZERO {
            let before = self.gateway.stats().admissions;
            let timer = tokio::time::sleep(wait);
            tokio::pin!(timer);
            tokio::select! {out=&mut execution=>panic!("parked call ended: {out:?}"),_= &mut timer=>{}}
            assert_eq!(self.gateway.stats().admissions, before);
        }
        if let Some(resolution) = answer
            && let JournalNotice::NeedsOperator {
                key,
                attempt,
                episode,
                ..
            } = &notice
        {
            self.dir
                .request_resolution(key, *attempt, *episode, resolution)
                .unwrap();
        }
        let outcome = execution.await;
        let events = collect.await.unwrap();
        forward.await.unwrap().unwrap();
        let kind = match outcome {
            RunOutcome::Completed { .. } => RunOutcomeKind::Completed,
            RunOutcome::Failed { .. } => RunOutcomeKind::Failed,
            RunOutcome::Aborted { .. } => RunOutcomeKind::Aborted,
        };
        journal.finish(kind).await.unwrap();
        let summary = journal.summary();
        let mut observed = vec![notice.clone()];
        while let Ok(item) = notices.try_recv() {
            observed.push(item)
        }
        (
            RunData {
                outcome,
                events,
                summary,
                notices: observed,
            },
            notice,
        )
    }
}
#[derive(Clone)]
pub struct RejectSubmit {
    pub inner: FakeGateway,
    pub reason: &'static str,
}
impl talaria_workflow::journal::GatewayPort for RejectSubmit {
    fn create_session(
        &self,
        r: talaria_workflow::journal::CreateSession,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::SessionBound, talaria_workflow::journal::PortError>,
    > {
        self.inner.create_session(r)
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
        _r: talaria_workflow::journal::SubmitTurn,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<talaria_workflow::journal::Receipt, talaria_workflow::journal::PortError>,
    > {
        let reason = self.reason.to_owned();
        Box::pin(async move {
            Err(talaria_workflow::journal::PortError::Refused {
                reason,
                field: None,
                detail: None,
            })
        })
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
        self.inner.receipt(p, s, a)
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
        self.inner.wait_settled(p, s, a)
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
pub fn parked_records(dir: &RunDir) -> Vec<talaria_workflow::journal::Record> {
    use talaria_workflow::journal::{AdmissionState, AttentionCause, Record};
    let k = key();
    vec![
        start_record(dir),
        Record::CallIntent {
            key: k.clone(),
            attempt: 0,
            call_id: call_id(),
            label: "slow".into(),
            phase: None,
            workflow_path: vec![],
            launch: launch(),
            request_id: "fixture-request".into(),
        },
        Record::SessionBound {
            key: k.clone(),
            attempt: 0,
            session_id: "session".into(),
            stored_session_id: "session".into(),
        },
        Record::TurnIntent {
            key: k.clone(),
            attempt: 0,
            turn: 0,
            submission_id: "fixture-submission".into(),
            text: Some("prompt".into()),
            text_ref: None,
        },
        Record::TurnAdmitted {
            key: k.clone(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(1),
            authority_epoch: 0,
        },
        Record::CallAttention {
            key: k,
            attempt: 0,
            episode: 0,
            cause: AttentionCause::Unknown,
            turn: Some(0),
            admission_id: Some("admission".into()),
            execution_generation: Some(1),
            detail: "unknown".into(),
        },
    ]
}
