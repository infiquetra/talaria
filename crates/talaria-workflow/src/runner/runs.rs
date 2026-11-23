use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;

use crate::{
    journal::{JournalError, JournalReader, Manifest, RecordLine},
    tokens::CacheTracker,
};

pub fn configuration_dir() -> PathBuf {
    env::var_os("TALARIA_CONFIG_DIR")
        .filter(|p| !p.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("HOME")
                .or_else(|| env::var_os("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".talaria")
        })
}
pub fn runs_dir(explicit: Option<&Path>) -> PathBuf {
    explicit
        .filter(|p| !p.as_os_str().to_string_lossy().trim().is_empty())
        .map(Path::to_owned)
        .or_else(|| {
            env::var_os("TALARIA_RUNS_DIR")
                .filter(|p| !p.to_string_lossy().trim().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| configuration_dir().join("runs"))
}
pub fn workflows_dir(explicit: Option<&Path>) -> PathBuf {
    explicit
        .filter(|p| !p.as_os_str().to_string_lossy().trim().is_empty())
        .map(Path::to_owned)
        .unwrap_or_else(|| configuration_dir().join("workflows"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Parked,
    Completed,
    /// The script returned, but at least one agent failed.
    CompletedWithFailures,
    /// The script returned, but the budget refused or stopped at least one agent.
    BudgetExhausted,
    Failed,
    Aborted,
    Interrupted,
    Unreadable,
}
impl RunStatus {
    /// Whether the run reached a recorded end. Running, parked, interrupted and unreadable runs
    /// have not: their agents may still be working or may be resumed.
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::CompletedWithFailures
                | Self::BudgetExhausted
                | Self::Failed
                | Self::Aborted
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Parked => "parked",
            Self::Completed => "completed",
            Self::CompletedWithFailures => "completed_with_failures",
            Self::BudgetExhausted => "budget_exhausted",
            Self::Failed => "failed",
            Self::Aborted => "aborted",
            Self::Interrupted => "interrupted",
            Self::Unreadable => "unreadable",
        }
    }
    /// The status as a person reads it: the raw word with its underscores as spaces. JSON keeps
    /// [`Self::as_str`].
    pub fn label(self) -> String {
        self.as_str().replace('_', " ")
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub name: String,
    pub profile: String,
    pub started_at_ms: i64,
    pub status: RunStatus,
    pub agents_finished: u32,
    pub agents_total_known: u32,
    pub parked: u32,
    /// Tokens the budget counts: everything except cache reads. A turn whose cache reads Hermes
    /// did not report is counted whole.
    pub tokens_spent: Option<u64>,
    /// Cache reads Hermes reported; not in `tokens_spent`. `None` when no turn reported tokens.
    pub cache_read_tokens: Option<u64>,
    /// Settled turns whose cache reads were not reported (every turn of a run journaled before
    /// cache reads were kept apart); their tokens are all in `tokens_spent`.
    pub cache_unknown_turns: u32,
    pub money_spent_usd: Option<f64>,
    pub run_dir: String,
    pub warning: Option<String>,
    /// What a finished run left for its reader to look at.
    #[serde(flatten)]
    pub attention: RunAttention,
}

/// The problems a finished run's reader should not miss, as counts. All zero (and no status)
/// for a run that has not finished.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RunAttention {
    pub failed_agents: u32,
    pub cut_short_agents: u32,
    /// Agent worktrees that still held uncommitted changes when the run ended.
    pub uncommitted_edits: u32,
    /// Distinct branches Talaria saved uncommitted agent work to.
    pub saved_branches: u32,
    /// Phases the script's `meta.phases` declares that never started.
    pub phases_not_started: u32,
    /// The `status` string at the top of the script's own result object, when it has one. It
    /// is the script's word, not Talaria's outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_status: Option<String>,
}
impl RunAttention {
    /// Count what the run folder's `outcome.json` and journal say. A missing or unreadable
    /// outcome file gives no counts.
    pub(crate) fn read(path: &Path, records: &[RecordLine], latest_script: Option<&str>) -> Self {
        let Ok(outcome) = fs::read(path.join("outcome.json"))
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()))
        else {
            return Self::default();
        };
        let list = |name: &str| outcome[name].as_array().cloned().unwrap_or_default();
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let leftovers = list("leftover_edits");
        let mut saved = BTreeSet::new();
        for entry in list("failed_agents")
            .iter()
            .chain(&list("cut_short_agents"))
            .chain(&leftovers)
        {
            if let Some(branch) = entry["saved_branch"].as_str() {
                saved.insert(branch.to_owned());
            }
        }
        let declared = latest_script
            .and_then(|sha| fs::read_to_string(path.join("scripts").join(format!("{sha}.js"))).ok())
            .and_then(|source| crate::engine::parse_script(&source).ok())
            .map(|parsed| parsed.meta.phases)
            .unwrap_or_default();
        let started: BTreeSet<&str> = records
            .iter()
            .filter_map(|line| match &line.record {
                crate::journal::Record::Engine {
                    event: Some(crate::engine::EngineEvent::Phase { title, .. }),
                    ..
                } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        Self {
            failed_agents: count(list("failed_agents").len()),
            cut_short_agents: count(list("cut_short_agents").len()),
            uncommitted_edits: count(
                leftovers
                    .iter()
                    .filter(|entry| entry["in_place"] != true)
                    .count(),
            ),
            saved_branches: count(saved.len()),
            phases_not_started: count(
                declared
                    .iter()
                    .filter(|phase| !started.contains(phase.title.as_str()))
                    .count(),
            ),
            script_status: outcome["result"]["status"].as_str().map(str::to_owned),
        }
    }
}

/// The attention counts of one finished run folder, read from its outcome file and journal.
pub fn run_attention(path: &Path) -> RunAttention {
    let Ok((records, _)) = JournalReader::read_from(&path.join("journal.jsonl"), 0) else {
        return RunAttention::default();
    };
    let latest = Fold::records(&records)
        .ok()
        .and_then(|fold| fold.latest_script);
    RunAttention::read(path, &records, latest.as_deref())
}

#[derive(Debug, thiserror::Error)]
pub enum FindError {
    #[error("unknown run '{id}' (searched {searched})")]
    UnknownId { id: String, searched: PathBuf },
    #[error("'{path}' is not a run folder")]
    NotAFolder { path: PathBuf },
    #[error("{0}")]
    Io(#[from] io::Error),
}

pub(crate) fn is_run_folder(path: &Path) -> bool {
    path.is_dir()
        && ["manifest.json", "journal.jsonl", "run.lock"]
            .iter()
            .any(|name| path.join(name).exists())
}
pub fn find_run(runs_dir: &Path, id_or_path: &str) -> Result<PathBuf, FindError> {
    let target = Path::new(id_or_path);
    let is_path = target.is_absolute() || target.components().count() != 1 || target.exists();
    let candidate = if is_path {
        target.to_owned()
    } else {
        runs_dir.join(target)
    };
    if is_run_folder(&candidate) {
        return candidate.canonicalize().map_err(FindError::Io);
    }
    if is_path || candidate.exists() {
        Err(FindError::NotAFolder { path: candidate })
    } else {
        Err(FindError::UnknownId {
            id: id_or_path.into(),
            searched: runs_dir.into(),
        })
    }
}

/// Whether a runner holds the run lock now. A free or missing lock means no runner.
/// Read access is enough to test the lock, so a viewer that cannot write the run folder
/// still gets an answer.
pub fn runner_alive(dir: &Path) -> io::Result<bool> {
    let file = match File::open(dir.join("run.lock")) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(false)
        }
        Err(fs::TryLockError::WouldBlock) => Ok(true),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}
pub(crate) enum LockProbe {
    Free(Option<File>),
    Held,
}
pub(crate) fn probe_lock(dir: &Path) -> io::Result<LockProbe> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("run.lock"))
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(LockProbe::Free(None)),
        Err(e) => return Err(e),
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            Ok(LockProbe::Free(Some(file)))
        }
        Err(fs::TryLockError::WouldBlock) => Ok(LockProbe::Held),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

#[derive(Default)]
pub(crate) struct Fold {
    pub terminal: Option<String>,
    pub latest_script: Option<String>,
    pub summary: Option<Value>,
    pub finished: u32,
    pub total: u32,
    pub parked: u32,
    pub tokens: Option<u64>,
    pub cache_read_tokens: u64,
    pub cache_unknown_turns: u32,
    pub money: Option<f64>,
}
impl Fold {
    pub fn records(records: &[RecordLine]) -> Result<Self, serde_json::Error> {
        let mut folded = Self::default();
        let mut keys = BTreeSet::new();
        let mut finished = BTreeMap::new();
        let mut parked = BTreeSet::new();
        let mut money = BTreeMap::new();
        let mut unknown_money = false;
        let mut settled = false;
        let mut cache = CacheTracker::new();
        for line in records {
            let record = serde_json::to_value(&line.record)?;
            let kind = record["t"].as_str().unwrap_or_default();
            let key = record["key"].as_str().unwrap_or_default();
            if [
                "call_intent",
                "session_bound",
                "turn_intent",
                "turn_admitted",
                "turn_settled",
                "call_attention",
                "call_resolved",
                "call_finished",
            ]
            .contains(&kind)
            {
                keys.insert(key.to_owned());
            }
            match kind {
                "run_started" | "run_resumed" => {
                    folded.terminal = None;
                    folded.summary = None;
                    folded.latest_script = record["script_sha256"].as_str().map(str::to_owned);
                }
                "run_finished" => {
                    folded.terminal = record["outcome"].as_str().map(str::to_owned);
                    folded.summary = Some(record["summary"].clone());
                }
                "call_attention" => {
                    parked.insert(key.to_owned());
                }
                "call_resolved" => {
                    parked.remove(key);
                }
                "call_finished" => {
                    parked.remove(key);
                    finished.insert(
                        key.to_owned(),
                        matches!(
                            record["outcome"]["kind"].as_str(),
                            Some("completed" | "skipped")
                        ),
                    );
                }
                "turn_settled" => {
                    settled = true;
                    let usage = &record["usage"];
                    let attempt = record["attempt"].as_u64().unwrap_or_default();
                    if let Some(turn) = cache.settle((key.to_owned(), attempt), usage) {
                        folded.tokens = Some(
                            folded
                                .tokens
                                .unwrap_or_default()
                                .saturating_add(turn.tokens.counted),
                        );
                        folded.cache_read_tokens = folded
                            .cache_read_tokens
                            .saturating_add(turn.tokens.cache_read);
                        if !turn.cache_known {
                            folded.cache_unknown_turns =
                                folded.cache_unknown_turns.saturating_add(1);
                        }
                    }
                    let cost = &usage["session_cost"];
                    let amount = cost["estimated_cost_usd"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0);
                    if !matches!(
                        cost["cost_status"].as_str(),
                        Some("actual" | "estimated" | "included")
                    ) || amount.is_none()
                    {
                        unknown_money = true;
                    }
                    if let Some(amount) = amount {
                        money.insert(
                            (
                                key.to_owned(),
                                record["attempt"].as_u64().unwrap_or_default(),
                            ),
                            amount,
                        );
                    }
                }
                _ => {}
            }
        }
        folded.finished = finished
            .values()
            .filter(|v| **v)
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        folded.total = keys.len().try_into().unwrap_or(u32::MAX);
        folded.parked = parked.len().try_into().unwrap_or(u32::MAX);
        if settled && !unknown_money {
            let total: f64 = money.values().sum();
            if total.is_finite() {
                folded.money = Some(total);
            }
        }
        Ok(folded)
    }
}

fn unreadable(path: &Path, id: &str, warning: String) -> RunSummary {
    RunSummary {
        run_id: id.into(),
        name: "(unreadable)".into(),
        profile: String::new(),
        started_at_ms: 0,
        status: RunStatus::Unreadable,
        agents_finished: 0,
        agents_total_known: 0,
        parked: 0,
        tokens_spent: None,
        cache_read_tokens: None,
        cache_unknown_turns: 0,
        money_spent_usd: None,
        run_dir: path.to_string_lossy().into_owned(),
        warning: Some(warning),
        attention: RunAttention::default(),
    }
}
fn summarize(path: &Path, id: &str) -> RunSummary {
    let bytes = match fs::read(path.join("manifest.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return unreadable(path, id, "missing manifest.json".into());
        }
        Err(e) => return unreadable(path, id, format!("corrupt manifest.json: {e}")),
    };
    let manifest: Manifest = match serde_json::from_slice(&bytes) {
        Ok(manifest) => manifest,
        Err(e) => return unreadable(path, id, format!("corrupt manifest.json: {e}")),
    };
    if manifest.v != 1 {
        return unreadable(
            path,
            id,
            "corrupt manifest.json: unsupported manifest version".into(),
        );
    }
    let records = match JournalReader::read_from(&path.join("journal.jsonl"), 0) {
        Ok((records, _)) => records,
        Err(JournalError::Corrupt { line, reason }) => {
            return unreadable(
                path,
                id,
                format!("corrupt journal.jsonl at line {line}: {reason}"),
            );
        }
        Err(JournalError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
            return unreadable(path, id, "missing journal.jsonl".into());
        }
        Err(e) => return unreadable(path, id, format!("corrupt journal.jsonl at line 1: {e}")),
    };
    let fold = match Fold::records(&records) {
        Ok(fold) => fold,
        Err(e) => return unreadable(path, id, format!("corrupt journal.jsonl at line 1: {e}")),
    };
    let running = match probe_lock(path) {
        Ok(LockProbe::Held) => true,
        Ok(LockProbe::Free(file)) => {
            drop(file);
            false
        }
        Err(e) => return unreadable(path, id, format!("lock probe failed: {e}")),
    };
    let status = match fold.terminal.as_deref() {
        Some("completed") => RunStatus::Completed,
        Some("completed_with_failures") => RunStatus::CompletedWithFailures,
        Some("budget_exhausted") => RunStatus::BudgetExhausted,
        Some("failed") => RunStatus::Failed,
        Some("aborted") => RunStatus::Aborted,
        _ if fold.parked > 0 => RunStatus::Parked,
        _ if running => RunStatus::Running,
        _ => RunStatus::Interrupted,
    };
    RunSummary {
        run_id: manifest.run_id.as_str().into(),
        name: manifest.workflow.name,
        profile: manifest.profile,
        started_at_ms: manifest.started_at_ms,
        status,
        agents_finished: fold.finished,
        agents_total_known: fold.total,
        parked: fold.parked,
        tokens_spent: fold.tokens,
        cache_read_tokens: fold.tokens.map(|_| fold.cache_read_tokens),
        cache_unknown_turns: fold.cache_unknown_turns,
        money_spent_usd: fold.money,
        run_dir: path.to_string_lossy().into_owned(),
        warning: None,
        attention: if status.is_finished() {
            RunAttention::read(path, &records, fold.latest_script.as_deref())
        } else {
            RunAttention::default()
        },
    }
}
/// One run folder's summary, derived exactly as `workflow list` derives it.
pub fn summarize_run(path: &Path) -> RunSummary {
    let id = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    summarize(path, &id)
}
pub fn discover_runs(runs_dir: &Path) -> Vec<RunSummary> {
    let entries = match fs::read_dir(runs_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut runs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str() else { continue };
        let path = entry.path();
        if is_run_folder(&path) {
            runs.push(summarize(&path, id));
        }
    }
    runs.sort_by(|a, b| {
        b.started_at_ms
            .cmp(&a.started_at_ms)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    runs
}
/// The runs that started at or after `since_ms`, newest first. A folder last changed well
/// before the cutoff is skipped without reading it, so a frequent check stays cheap.
pub fn discover_runs_since(runs_dir: &Path, since_ms: i64) -> Vec<RunSummary> {
    let entries = match fs::read_dir(runs_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    // A coarse file system clock may stamp a new folder slightly before the cutoff.
    let cutoff = since_ms.saturating_sub(5_000);
    let mut runs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str() else { continue };
        let path = entry.path();
        let changed_ms = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|at| i64::try_from(at.as_millis()).unwrap_or(i64::MAX));
        if changed_ms.is_some_and(|ms| ms < cutoff) || !is_run_folder(&path) {
            continue;
        }
        let summary = summarize(&path, id);
        if summary.started_at_ms >= since_ms {
            runs.push(summary);
        }
    }
    runs.sort_by(|a, b| {
        b.started_at_ms
            .cmp(&a.started_at_ms)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    runs
}

#[cfg(test)]
mod attention_tests {
    use super::*;
    use crate::engine::EngineEvent;
    use crate::journal::Record;

    fn phase(title: &str) -> RecordLine {
        RecordLine {
            v: 1,
            seq: 1,
            at_ms: 0,
            record: Record::Engine {
                event: Some(EngineEvent::Phase {
                    title: title.into(),
                    workflow_path: vec![],
                }),
                event_ref: None,
            },
        }
    }

    #[test]
    fn counts_come_from_the_outcome_file_and_the_declared_phases() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        fs::create_dir(dir.join("scripts")).unwrap();
        let source = "export const meta = {name: 'x', description: 'd', phases: [{title: 'Plan'}, {title: 'Build'}, {title: 'Check'}]};\nreturn 1;";
        fs::write(dir.join("scripts").join("abc.js"), source).unwrap();
        fs::write(
            dir.join("outcome.json"),
            serde_json::json!({
                "outcome": "completed",
                "failed_agents": [{"label": "a", "reason": "r", "saved_branch": "b1"}],
                "cut_short_agents": [{"label": "c", "reason": "r", "saved_branch": "b2"}],
                "leftover_edits": [
                    {"path": "/w/a", "changed_files": 1, "saved_branch": "b1"},
                    {"path": "/w/c", "changed_files": 2, "saved_branch": "b2"},
                    {"path": "/w/d", "changed_files": 1},
                    {"path": "/project", "changed_files": 0, "in_place": true, "agents": ["e"]}
                ],
                "result": {"status": "partial", "items": []}
            })
            .to_string(),
        )
        .unwrap();
        let counts = RunAttention::read(dir, &[phase("Plan")], Some("abc"));
        assert_eq!(
            counts,
            RunAttention {
                failed_agents: 1,
                cut_short_agents: 1,
                uncommitted_edits: 3,
                saved_branches: 2,
                phases_not_started: 2,
                script_status: Some("partial".into()),
            }
        );
        // A result without a top-level status string, and no outcome file at all.
        fs::write(
            dir.join("outcome.json"),
            r#"{"outcome":"completed","result":"done"}"#,
        )
        .unwrap();
        assert_eq!(RunAttention::read(dir, &[], None).script_status, None);
        fs::remove_file(dir.join("outcome.json")).unwrap();
        assert_eq!(
            RunAttention::read(dir, &[], Some("abc")),
            RunAttention::default()
        );
    }
}
