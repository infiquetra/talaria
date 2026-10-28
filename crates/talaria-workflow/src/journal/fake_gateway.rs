//! Offline session authority modeled exclusively from the pinned gateway rules.
use super::*;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tokio::time::Instant;
#[derive(Clone)]
pub struct FakeGateway {
    state: Arc<Mutex<State>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptRule {
    pub label: Option<String>,
    pub model: Option<String>,
    pub text_contains: Option<String>,
    pub outcome: FakeOutcome,
    pub final_text: String,
    pub usage: Value,
    pub duration: Duration,
    pub hold: Option<String>,
    pub settlement_lost: bool,
}
impl Default for ScriptRule {
    fn default() -> Self {
        Self {
            label: None,
            model: None,
            text_contains: None,
            outcome: FakeOutcome::Completed,
            final_text: "ok".into(),
            usage: json!({}),
            duration: Duration::from_secs(1),
            hold: None,
            settlement_lost: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FakeOutcome {
    Completed,
    Failed,
    Interrupted,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FakeStats {
    pub sessions: u32,
    pub admissions: u32,
    pub receipts: u32,
    pub resolves: u32,
    pub attaches: u32,
    pub calls: Vec<String>,
    pub admissions_by_session: BTreeMap<String, Vec<String>>,
}
#[derive(Serialize, Deserialize)]
struct State {
    v: u32,
    epoch: i64,
    principal: String,
    sessions: BTreeMap<String, Session>,
    rules: Vec<ScriptRule>,
    stats: FakeStats,
    #[serde(skip)]
    subscriptions: BTreeSet<String>,
    #[serde(skip)]
    released: BTreeSet<String>,
    #[serde(skip)]
    fail_count: u32,
    #[serde(skip)]
    fail_error: Option<PortError>,
    #[serde(skip)]
    autosave: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Session {
    request_id: String,
    launch: ResolvedLaunch,
    label: String,
    generation: i64,
    deleted: bool,
    admissions: Vec<Admission>,
}
#[derive(Serialize, Deserialize)]
struct Admission {
    id: String,
    submission_id: String,
    text: String,
    status: AdmissionState,
    generation: Option<i64>,
    epoch: i64,
    outcome: Option<String>,
    result: Option<Value>,
    usage: Option<Value>,
    rule: ScriptRule,
    remaining_ms: u64,
    captured: bool,
    resolved: bool,
    #[serde(skip)]
    deadline: Option<Instant>,
}
impl Default for FakeGateway {
    fn default() -> Self {
        Self::new()
    }
}
impl FakeGateway {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                v: 1,
                epoch: 0,
                principal: "uid:1000".into(),
                sessions: BTreeMap::new(),
                rules: Vec::new(),
                stats: FakeStats::default(),
                subscriptions: BTreeSet::new(),
                released: BTreeSet::new(),
                fail_count: 0,
                fail_error: None,
                autosave: None,
            })),
        }
    }
    pub fn with_script(self, rules: Vec<ScriptRule>) -> Self {
        if let Ok(mut s) = self.state.lock() {
            s.rules = rules;
        }
        self
    }
    pub fn autosave(self, path: PathBuf) -> Self {
        if let Ok(mut s) = self.state.lock() {
            s.autosave = Some(path);
            let _ = save(&mut s);
        }
        self
    }
    pub fn save_state(&self, path: &Path) -> std::io::Result<()> {
        let mut s = self
            .state
            .lock()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        refresh(&mut s);
        write_state(&s, path)
    }
    pub fn load_state(path: &Path) -> std::io::Result<Self> {
        let mut s: State =
            serde_json::from_slice(&fs::read(path)?).map_err(std::io::Error::other)?;
        if s.v != 1 {
            return Err(std::io::Error::other("unsupported fake state version"));
        }
        for session in s.sessions.values_mut() {
            for a in &mut session.admissions {
                a.rule.hold = None;
                if a.status == AdmissionState::Started {
                    a.deadline = Some(Instant::now() + Duration::from_millis(a.remaining_ms));
                }
            }
        }
        for rule in &mut s.rules {
            rule.hold = None;
        }
        Ok(Self {
            state: Arc::new(Mutex::new(s)),
        })
    }
    pub fn release(&self, hold: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.released.insert(hold.into());
            refresh(&mut s);
            let _ = save(&mut s);
        }
    }
    pub fn disconnect(&self) {
        if let Ok(mut s) = self.state.lock() {
            refresh(&mut s);
            s.subscriptions.clear();
            let _ = save(&mut s);
        }
    }
    pub fn restart(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.epoch += 1;
            s.subscriptions.clear();
            for session in s.sessions.values_mut() {
                for a in &mut session.admissions {
                    if a.status == AdmissionState::Started {
                        a.status = AdmissionState::Unknown;
                        a.deadline = None;
                    }
                    a.captured = false;
                    if a.status != AdmissionState::Terminal {
                        a.result = None;
                        a.usage = None;
                    }
                }
            }
            let _ = save(&mut s);
        }
    }
    pub fn delete_session(&self, id: &str) {
        if let Ok(mut s) = self.state.lock() {
            if let Some(session) = s.sessions.get_mut(id) {
                session.deleted = true;
            }
            s.subscriptions.remove(id);
            let _ = save(&mut s);
        }
    }
    pub fn fail_next(&self, n: u32, error: PortError) {
        if let Ok(mut s) = self.state.lock()
            && matches!(error, PortError::Transport(_) | PortError::Timeout)
        {
            s.fail_count = n;
            s.fail_error = Some(error);
        }
    }
    pub fn stats(&self) -> FakeStats {
        if let Ok(mut s) = self.state.lock() {
            refresh(&mut s);
            let _ = save(&mut s);
            s.stats.clone()
        } else {
            FakeStats::default()
        }
    }
    fn access(&self, method: &str) -> Result<MutexGuard<'_, State>, PortError> {
        let mut s = self
            .state
            .lock()
            .map_err(|e| PortError::Transport(e.to_string()))?;
        refresh(&mut s);
        s.stats.calls.push(method.into());
        if s.fail_count > 0 {
            s.fail_count -= 1;
            return Err(s.fail_error.clone().unwrap_or(PortError::Timeout));
        }
        Ok(s)
    }
    fn get_receipt(
        &self,
        session: String,
        admission: String,
        method: &str,
    ) -> Result<Receipt, PortError> {
        let mut s = self.access(method)?;
        s.stats.receipts += 1;
        let a = find(&s, &session, &admission)?;
        let receipt = receipt(&session, a, true);
        save(&mut s)?;
        Ok(receipt)
    }
}
fn refresh(s: &mut State) {
    let now = Instant::now();
    for session in s.sessions.values_mut() {
        if session.deleted {
            continue;
        }
        for a in &mut session.admissions {
            if a.status == AdmissionState::Unknown {
                break;
            }
            if a.status == AdmissionState::Terminal {
                continue;
            }
            if a.status == AdmissionState::Queued {
                session.generation += 1;
                a.status = AdmissionState::Started;
                a.generation = Some(session.generation);
                a.epoch = s.epoch;
                a.deadline = Some(now + Duration::from_millis(a.remaining_ms));
            }
            let held = a
                .rule
                .hold
                .as_ref()
                .is_some_and(|h| !s.released.contains(h));
            if let Some(deadline) = a.deadline {
                a.remaining_ms = deadline.saturating_duration_since(now).as_millis() as u64;
            }
            if held || a.deadline.is_some_and(|d| d > now) {
                break;
            }
            if a.rule.settlement_lost {
                a.status = AdmissionState::Unknown;
                a.captured = true;
                a.deadline = None;
                break;
            }
            a.status = AdmissionState::Terminal;
            a.outcome = Some(outcome(&a.rule).into());
            a.result = Some(result(&a.rule));
            a.usage = Some(a.rule.usage.clone());
            a.deadline = None;
        }
    }
}
fn outcome(rule: &ScriptRule) -> &'static str {
    match rule.outcome {
        FakeOutcome::Completed => "completed",
        FakeOutcome::Failed => "failed",
        FakeOutcome::Interrupted => "interrupted",
    }
}
fn result(rule: &ScriptRule) -> Value {
    json!({"final_response":rule.final_text,"completed":rule.outcome==FakeOutcome::Completed,"failed":rule.outcome==FakeOutcome::Failed,"interrupted":rule.outcome==FakeOutcome::Interrupted,"turn_exit_reason":outcome(rule)})
}
fn receipt(session: &str, a: &Admission, include: bool) -> Receipt {
    Receipt {
        admission_id: a.id.clone(),
        session_id: session.into(),
        status: a.status,
        outcome: a.outcome.clone(),
        authority_epoch: a.epoch,
        execution_generation: a.generation,
        result: if include { a.result.clone() } else { None },
        usage: if include { a.usage.clone() } else { None },
    }
}
fn find<'a>(s: &'a State, session: &str, admission: &str) -> Result<&'a Admission, PortError> {
    let session = s
        .sessions
        .get(session)
        .filter(|s| !s.deleted)
        .ok_or_else(|| PortError::refused("not_found"))?;
    session
        .admissions
        .iter()
        .find(|a| a.id == admission)
        .ok_or_else(|| PortError::refused("not_found"))
}
fn write_state(s: &State, path: &Path) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(s).map_err(std::io::Error::other)?;
    run_dir::atomic_replace(path, &bytes)
}
fn save(s: &mut State) -> Result<(), PortError> {
    if let Some(path) = &s.autosave {
        write_state(s, path).map_err(|e| PortError::Transport(e.to_string()))?;
    }
    Ok(())
}
impl GatewayPort for FakeGateway {
    fn create_session(
        &self,
        req: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let mut s = this.access("create_session")?;
            if req.request_id.is_empty() || req.request_id.chars().count() > 256 {
                return Err(PortError::Refused {
                    reason: "invalid_params".into(),
                    field: Some("request_id".into()),
                    detail: None,
                });
            }
            let id = format!(
                "local-{}",
                keys::sha(
                    json!([req.launch.profile, s.principal, req.request_id])
                        .to_string()
                        .as_bytes()
                )
            );
            if let Some(old) = s.sessions.get(&id) {
                if old.launch != req.launch {
                    return Err(PortError::Refused {
                        reason: "invalid_params".into(),
                        field: Some("request_id".into()),
                        detail: Some(
                            "a resumed request_id must carry the same frozen request".into(),
                        ),
                    });
                }
            } else {
                s.sessions.insert(
                    id.clone(),
                    Session {
                        request_id: req.request_id,
                        launch: req.launch,
                        label: req.label,
                        generation: 0,
                        deleted: false,
                        admissions: Vec::new(),
                    },
                );
                s.stats.sessions += 1;
            }
            s.subscriptions.insert(id.clone());
            save(&mut s)?;
            Ok(SessionBound {
                session_id: id.clone(),
                stored_session_id: id,
            })
        })
    }
    fn attach(
        &self,
        _profile: String,
        session: String,
    ) -> BoxFuture<'static, Result<(), PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let mut s = this.access("attach")?;
            s.stats.attaches += 1;
            if !s.sessions.get(&session).is_some_and(|s| !s.deleted) {
                return Err(PortError::refused("not_found"));
            }
            s.subscriptions.insert(session);
            save(&mut s)?;
            Ok(())
        })
    }
    fn submit(&self, req: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let mut s = this.access("submit")?;
            if !s.subscriptions.contains(&req.session_id) {
                return Err(PortError::refused("permission_denied"));
            }
            if req.submission_id.is_empty() || req.submission_id.chars().count() > 1024 {
                return Err(PortError::refused("invalid_params"));
            }
            let session = s
                .sessions
                .get(&req.session_id)
                .ok_or_else(|| PortError::refused("not_found"))?;
            if let Some(a) = session
                .admissions
                .iter()
                .find(|a| a.submission_id == req.submission_id)
            {
                if a.text != req.text {
                    return Err(PortError::refused("admission_conflict"));
                }
                return Ok(receipt(&req.session_id, a, false));
            }
            if session
                .admissions
                .iter()
                .filter(|a| a.status == AdmissionState::Queued)
                .count()
                >= 32
            {
                return Err(PortError::refused("queue_full"));
            }
            let rule = s
                .rules
                .iter()
                .find(|rule| {
                    rule.label.as_ref().is_none_or(|l| l == &session.label)
                        && rule
                            .model
                            .as_ref()
                            .is_none_or(|m| m == &session.launch.model)
                        && rule
                            .text_contains
                            .as_ref()
                            .is_none_or(|t| req.text.contains(t))
                })
                .cloned()
                .unwrap_or_default();
            let id = format!("admission-{}", s.stats.admissions + 1);
            let epoch = s.epoch;
            let session = s
                .sessions
                .get_mut(&req.session_id)
                .ok_or_else(|| PortError::refused("not_found"))?;
            session.admissions.push(Admission {
                id: id.clone(),
                submission_id: req.submission_id,
                text: req.text,
                status: AdmissionState::Queued,
                generation: None,
                epoch,
                outcome: None,
                result: None,
                usage: None,
                remaining_ms: rule.duration.as_millis() as u64,
                rule,
                captured: false,
                resolved: false,
                deadline: None,
            });
            s.stats.admissions += 1;
            s.stats
                .admissions_by_session
                .entry(req.session_id.clone())
                .or_default()
                .push(id.clone());
            refresh(&mut s);
            let a = find(&s, &req.session_id, &id)?;
            let r = receipt(&req.session_id, a, false);
            save(&mut s)?;
            Ok(r)
        })
    }
    fn receipt(
        &self,
        _profile: String,
        session: String,
        admission: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move { this.get_receipt(session, admission, "receipt") })
    }
    fn wait_settled(
        &self,
        _profile: String,
        session: String,
        admission: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            loop {
                let r = this.get_receipt(session.clone(), admission.clone(), "wait_settled")?;
                if matches!(r.status, AdmissionState::Terminal | AdmissionState::Unknown) {
                    return Ok(r);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    }
    fn resolve_unknown(
        &self,
        _profile: String,
        session: String,
        admission: String,
        generation: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let mut s = this.access("resolve_unknown")?;
            s.stats.resolves += 1;
            let stored = s
                .sessions
                .get_mut(&session)
                .filter(|s| !s.deleted)
                .ok_or_else(|| PortError::refused("not_found"))?;
            let a = stored
                .admissions
                .iter_mut()
                .find(|a| a.id == admission)
                .ok_or_else(|| PortError::refused("not_found"))?;
            if a.generation != Some(generation)
                || (a.status != AdmissionState::Unknown && !a.resolved)
            {
                return Err(PortError::refused("stale_generation"));
            }
            if !a.resolved {
                if a.captured {
                    a.outcome = Some("completed".into());
                    a.result = Some(result(&a.rule));
                    a.usage = Some(a.rule.usage.clone());
                } else {
                    a.outcome = Some("interrupted".into());
                }
                a.status = AdmissionState::Terminal;
                a.resolved = true;
            }
            let r = receipt(&session, a, false);
            refresh(&mut s);
            save(&mut s)?;
            Ok(r)
        })
    }
}
