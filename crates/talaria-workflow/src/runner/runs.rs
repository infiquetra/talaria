use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_json::Value;

use crate::journal::{JournalError, JournalReader, Manifest, RecordLine};

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
    Failed,
    Aborted,
    Interrupted,
    Unreadable,
}
impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Parked => "parked",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Aborted => "aborted",
            Self::Interrupted => "interrupted",
            Self::Unreadable => "unreadable",
        }
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
    pub tokens_spent: Option<u64>,
    pub money_spent_usd: Option<f64>,
    pub run_dir: String,
    pub warning: Option<String>,
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
                    let tokens = usage["total_tokens"].as_u64().or_else(|| {
                        usage["input_tokens"]
                            .as_u64()
                            .zip(usage["output_tokens"].as_u64())
                            .map(|(a, b)| a.saturating_add(b))
                    });
                    if let Some(tokens) = tokens {
                        folded.tokens =
                            Some(folded.tokens.unwrap_or_default().saturating_add(tokens));
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
        money_spent_usd: None,
        run_dir: path.to_string_lossy().into_owned(),
        warning: Some(warning),
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
        money_spent_usd: fold.money,
        run_dir: path.to_string_lossy().into_owned(),
        warning: None,
    }
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
