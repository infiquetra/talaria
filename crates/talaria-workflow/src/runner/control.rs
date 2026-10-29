use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::Notify;

use crate::{
    engine::{AgentRequest, BudgetSnapshot},
    journal::{
        AdmissionGate, GateDecision, GateReason, JournalError, JournalReader, RecordLine,
        Resolution, ResolvedLaunch, RunDir, SettledTurn,
    },
    progress::{ControlOutcome, RunControl},
};

use super::{
    files::{self, OutcomeFile},
    runs::{self, Fold},
};

#[derive(Clone)]
pub struct PausingGate {
    inner: Arc<dyn AdmissionGate>,
    state: Arc<PauseState>,
}
struct PauseState {
    paused: AtomicBool,
    changed: Notify,
}
impl PausingGate {
    pub fn new(inner: impl AdmissionGate) -> Self {
        Self {
            inner: Arc::new(inner),
            state: Arc::new(PauseState {
                paused: AtomicBool::new(false),
                changed: Notify::new(),
            }),
        }
    }
    pub fn set_paused(&self, paused: bool) {
        if self.state.paused.swap(paused, Ordering::SeqCst) != paused {
            self.state.changed.notify_waiters();
        }
    }
    pub fn is_paused(&self) -> bool {
        self.state.paused.load(Ordering::SeqCst)
    }
}
impl AdmissionGate for PausingGate {
    fn acquire(
        &self,
        request: &AgentRequest,
        launch: &ResolvedLaunch,
        reason: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        if reason == GateReason::Reattach {
            return self.inner.acquire(request, launch, reason);
        }
        let gate = self.clone();
        let request = request.clone();
        let launch = launch.clone();
        Box::pin(async move {
            loop {
                let changed = gate.state.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if gate.is_paused() {
                    changed.await;
                    continue;
                }
                let decision = tokio::select! {biased;_=changed=>continue,decision=gate.inner.acquire(&request,&launch,reason)=>decision};
                if gate.is_paused() {
                    drop(decision);
                    continue;
                }
                return decision;
            }
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        self.inner.budget()
    }
    fn retry_after_failure(
        &self,
        request: &AgentRequest,
        launch: &ResolvedLaunch,
        turn: &SettledTurn,
        attempt: u32,
    ) -> bool {
        self.inner
            .retry_after_failure(request, launch, turn, attempt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlWrite {
    Written,
    AlreadyRequested,
}
pub fn write_control(
    dir: &Path,
    kind: &str,
    key: Option<&str>,
    at_ms: i64,
) -> io::Result<ControlWrite> {
    let dir = RunDir::open(dir).map_err(io::Error::other)?;
    let outcome = match (kind, key) {
        ("pause", None) => RunControl::request_pause(&dir, at_ms),
        ("stop", None) => RunControl::request_stop(&dir, at_ms),
        ("restart", Some(key)) => RunControl::request_restart(&dir, key, at_ms),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid workflow control request",
            ));
        }
    }
    .map_err(io::Error::other)?;
    match outcome {
        ControlOutcome::Published => Ok(ControlWrite::Written),
        ControlOutcome::AlreadyRequested => Ok(ControlWrite::AlreadyRequested),
        ControlOutcome::Refused { reason } => Err(io::Error::other(reason)),
        ControlOutcome::Removed => {
            Err(io::Error::other("control request was unexpectedly removed"))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopExit {
    Stopped,
    AlreadyFinished { outcome: String },
    Unconfirmed,
    UnknownId,
}
pub fn request_stop(runs_dir: &Path, target: &str, timeout: Duration) -> StopExit {
    let dir = match runs::find_run(runs_dir, target) {
        Ok(dir) => dir,
        Err(_) => return StopExit::UnknownId,
    };
    let journal_path = dir.join("journal.jsonl");
    let (mut records, mut offset) = match JournalReader::read_from(&journal_path, 0) {
        Ok(read) => read,
        Err(_) => return StopExit::Unconfirmed,
    };
    if let Ok(fold) = Fold::records(&records)
        && let Some(outcome) = fold.terminal
    {
        return StopExit::AlreadyFinished { outcome };
    }
    let requested_at = files::now_ms();
    if write_control(&dir, "stop", None, requested_at).is_err() {
        return StopExit::Unconfirmed;
    }
    let requested_at = RunDir::open(&dir)
        .ok()
        .and_then(|d| RunControl::pending(&d).stop)
        .map_or(requested_at, |r| r.at_ms);
    let started = Instant::now();
    loop {
        match JournalReader::read_from(&journal_path, offset) {
            Ok((new, next)) => {
                records.extend(new);
                offset = next;
            }
            Err(_) => return StopExit::Unconfirmed,
        }
        let fold = match Fold::records(&records) {
            Ok(fold) => fold,
            Err(_) => return StopExit::Unconfirmed,
        };
        match fold.terminal.as_deref() {
            Some("completed" | "failed") => {
                return StopExit::AlreadyFinished {
                    outcome: fold.terminal.unwrap_or_default(),
                };
            }
            Some("aborted") => {
                if let Ok(outcome) = files::read_json::<OutcomeFile>(&dir.join("outcome.json"))
                    && outcome.v == 1
                    && outcome.outcome == "aborted"
                    && outcome.finished_at_ms >= requested_at
                {
                    match outcome.reason.as_deref() {
                        Some("stopped") if !dir.join("control/stop.json").exists() => {
                            return StopExit::Stopped;
                        }
                        Some("stopped-unconfirmed") => return StopExit::Unconfirmed,
                        Some("operator-abort") => {
                            return StopExit::AlreadyFinished {
                                outcome: "aborted".into(),
                            };
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return StopExit::Unconfirmed;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(250)));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnswerError {
    #[error("{0}")]
    Target(#[from] runs::FindError),
    #[error("already answered for {key} attempt {attempt} episode {episode}")]
    AlreadyAnswered {
        key: String,
        attempt: u32,
        episode: u32,
    },
    #[error("{0}")]
    Journal(#[from] JournalError),
}
pub fn answer_parked(
    runs_dir: &Path,
    target: &str,
    key: &str,
    attempt: u32,
    episode: u32,
    resolution: Resolution,
) -> Result<(), AnswerError> {
    let dir = RunDir::open(&runs::find_run(runs_dir, target)?)?;
    match dir.request_resolution(key, attempt, episode, resolution) {
        Ok(()) => Ok(()),
        Err(JournalError::ResolutionPending { .. }) => Err(AnswerError::AlreadyAnswered {
            key: key.into(),
            attempt,
            episode,
        }),
        Err(e) => Err(e.into()),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StopTarget {
    pub key: String,
    pub attempt: u32,
    pub turn: u32,
    pub profile: String,
    pub session_id: String,
    pub admission_id: String,
    pub status: String,
}
pub(crate) fn stop_targets(records: &[RecordLine]) -> Result<Vec<StopTarget>, serde_json::Error> {
    let mut profiles = BTreeMap::new();
    let mut sessions = BTreeMap::new();
    let mut targets = Vec::<StopTarget>::new();
    for line in records {
        let r = serde_json::to_value(&line.record)?;
        let key = r["key"].as_str().unwrap_or_default().to_owned();
        let attempt = r["attempt"].as_u64().unwrap_or_default();
        match r["t"].as_str() {
            Some("call_intent") => {
                profiles.insert(
                    (key, attempt),
                    r["launch"]["profile"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
            Some("session_bound") => {
                sessions.insert(
                    (key, attempt),
                    r["session_id"].as_str().unwrap_or_default().to_owned(),
                );
            }
            Some("turn_admitted") => {
                let identity = (key, attempt);
                let turn = r["turn"].as_u64().unwrap_or_default() as u32;
                if let (Some(profile), Some(session_id)) =
                    (profiles.get(&identity), sessions.get(&identity))
                {
                    let target = StopTarget {
                        key: identity.0.clone(),
                        attempt: identity.1 as u32,
                        turn,
                        profile: profile.clone(),
                        session_id: session_id.clone(),
                        admission_id: r["admission_id"].as_str().unwrap_or_default().to_owned(),
                        status: r["status"].as_str().unwrap_or_default().to_owned(),
                    };
                    if let Some(slot) = targets
                        .iter_mut()
                        .find(|t| t.session_id == *session_id && t.profile == *profile)
                    {
                        *slot = target;
                    } else {
                        targets.push(target);
                    }
                }
            }
            Some("turn_settled") => {
                let id = r["admission_id"].as_str().unwrap_or_default();
                for target in &mut targets {
                    if target.admission_id == id {
                        target.status = "terminal".into();
                    }
                }
            }
            _ => {}
        }
    }
    targets.retain(|t| matches!(t.status.as_str(), "started" | "queued"));
    Ok(targets)
}

pub(crate) fn restart_keys(dir: &RunDir) -> BTreeSet<String> {
    RunControl::pending(dir)
        .restarts
        .into_iter()
        .filter_map(|request| request.key)
        .collect()
}
pub(crate) fn read_records(dir: &Path) -> Result<Vec<RecordLine>, JournalError> {
    JournalReader::read_from(&dir.join("journal.jsonl"), 0).map(|(r, _)| r)
}
pub(crate) fn restart_needs_execution(
    records: &[RecordLine],
    keys: &BTreeSet<String>,
) -> Result<bool, String> {
    let mut latest = BTreeMap::new();
    for line in records {
        if let Some((key, _)) = line.record.identity() {
            let value = serde_json::to_value(&line.record).map_err(|e| e.to_string())?;
            latest.insert(key.to_owned(), value);
        }
    }
    let mut required = false;
    for key in keys {
        let Some(record) = latest.get(key) else {
            continue;
        };
        required |= record["t"] != "call_finished" || record["outcome"]["kind"] == "failed";
    }
    Ok(required)
}
pub(crate) fn restart_targets(
    records: &[RecordLine],
    keys: &[String],
) -> Result<Vec<StopTarget>, String> {
    let mut latest = BTreeMap::new();
    for line in records {
        if let Some((key, _)) = line.record.identity() {
            latest.insert(key.to_owned(), line.record.kind());
        }
    }
    for key in keys {
        if latest.get(key) == Some(&"turn_intent") {
            return Err(format!(
                "restart for {key} needs admission recovery before it can be granted"
            ));
        }
    }
    stop_targets(records)
        .map(|targets| {
            targets
                .into_iter()
                .filter(|target| {
                    keys.contains(&target.key) && latest.get(&target.key) == Some(&"turn_admitted")
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}
pub(crate) fn uncertain_calls(records: &[RecordLine]) -> Result<Vec<String>, serde_json::Error> {
    let mut latest = BTreeMap::new();
    let mut sessions = BTreeMap::new();
    for line in records {
        let Some((key, attempt)) = line.record.identity() else {
            continue;
        };
        let value = serde_json::to_value(&line.record)?;
        let kind = value["t"].as_str().unwrap_or_default().to_owned();
        if kind == "session_bound" {
            sessions.insert(
                (key.to_owned(), attempt),
                value["session_id"].as_str().unwrap_or_default().to_owned(),
            );
        }
        latest.insert(key.to_owned(), (attempt, kind));
    }
    Ok(latest
        .into_iter()
        .filter_map(|(key, (attempt, kind))| {
            matches!(
                kind.as_str(),
                "call_intent" | "session_bound" | "turn_intent"
            )
            .then(|| {
                sessions
                    .get(&(key.clone(), attempt))
                    .cloned()
                    .unwrap_or_else(|| format!("unbound call {key}"))
            })
        })
        .collect())
}

pub(crate) fn latest_summary(records: &[RecordLine]) -> Value {
    Fold::records(records)
        .ok()
        .and_then(|f| f.summary)
        .unwrap_or_else(
            || serde_json::json!({"completed":0,"failed":0,"skipped":0,"parked":0,"unfinished":0}),
        )
}

pub(crate) fn remove_stale_runner(dir: &Path) -> io::Result<()> {
    files::remove(&dir.join("runner.json"))
}
