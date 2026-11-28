//! Read the workflow's verdict and associate a named report with an older failed call.
use super::{agents, control, files, runs};
use crate::{
    engine::{EngineEvent, Isolation, RecoveredReport},
    journal::{self, CallOutcome, Record, RunDir},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedReport {
    pub key: String,
    pub attempt: u32,
    pub report: RecoveredReport,
}

/// Sidecars are named for the original call_finished sequence and never replace history.
pub(super) fn associated(dir: &Path, seq: u64) -> io::Result<Option<SavedReport>> {
    let path = dir.join("reports").join(format!("{seq}.json"));
    match files::read_json(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        other => other.map(Some),
    }
}

pub(super) fn read_value(
    dir: &Path,
    report: &RecoveredReport,
    seq: u64,
) -> Result<Value, agents::AgentError> {
    let reference = journal::BlobRef {
        sha256: report.sha256.clone(),
        bytes: report.bytes,
    };
    let bytes = journal::load_report_blob(dir, &reference, seq)?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::other(e).into())
}

/// Only an explicitly named file in the failed worker's own isolated folder is eligible.
pub fn associate(
    runs_dir: &Path,
    target: &str,
    name: &str,
    file: &str,
) -> Result<SavedReport, agents::AgentError> {
    use agents::AgentError::Refused;
    let path = runs::find_run(runs_dir, target)?;
    let dir = RunDir::open(&path)?;
    let lock = fs::File::options()
        .read(true)
        .write(true)
        .open(path.join("run.lock"))?;
    lock.try_lock().map_err(|e| {
        Refused(format!(
            "cannot associate a report while the run is in use: {e}"
        ))
    })?;
    let records = control::read_records(&path)?;
    if !records
        .last()
        .is_some_and(|line| matches!(line.record, Record::RunFinished { .. }))
    {
        return Err(Refused(
            "only a finished run can accept a historical report".into(),
        ));
    }
    let entries = agents::agents(&records);
    let entry = agents::pick(dir.run_id().as_str(), &entries, name)?;
    if !matches!(entry.outcome, Some(CallOutcome::Failed { .. })) {
        return Err(Refused(
            "select a failed call without a declared report".into(),
        ));
    }
    let intent = records
        .iter()
        .rev()
        .find_map(|line| match &line.record {
            Record::CallIntent {
                key,
                attempt,
                call_id,
                launch,
                ..
            } if key == &entry.key
                && *attempt == entry.attempt
                && launch.isolation == Some(Isolation::Worktree) =>
            {
                launch.cwd.as_deref().map(|cwd| (call_id, cwd))
            }
            _ => None,
        })
        .ok_or_else(|| Refused("the worker has no recorded isolated worktree".into()))?;
    let mut schema = None;
    for line in records.iter().rev() {
        if let Some(EngineEvent::AgentStarted { request, .. }) =
            journal::stored_engine_event(&path, &line.record, line.seq)?
            && &request.call_id == intent.0
        {
            schema = request.schema;
            break;
        }
    }
    let schema =
        schema.ok_or_else(|| Refused("the worker has no recorded output schema".into()))?;
    let (bytes, _) =
        crate::worktree::read_report_file(&path, Path::new(intent.1), intent.0, file, &schema)
            .map_err(Refused)?;
    let folder = path.join("reports");
    let saved = SavedReport {
        key: entry.key.clone(),
        attempt: entry.attempt,
        report: RecoveredReport {
            path: file.into(),
            sha256: journal::sha(&bytes),
            bytes: bytes.len() as u64,
        },
    };
    if let Some(existing) = associated(&path, entry.seq)?
        && existing != saved
    {
        return Err(Refused(
            "a different report is already associated with this attempt".into(),
        ));
    }
    journal::put_report_blob(&path, &bytes)?;
    files::private_dir(&folder)?;
    journal::immutable(
        &folder,
        &format!("{}.json", entry.seq),
        &serde_json::to_vec(&saved).map_err(io::Error::other)?,
    )?;
    Ok(saved)
}

/// Read small references on opening or finishing a run. Report bodies are loaded on request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Verdict {
    pub status: Option<String>,
    pub reports: Vec<SavedReport>,
}

impl Verdict {
    pub fn apply(self, progress: &mut crate::progress::RunProgress) {
        progress.run.script_status = self.status;
        for saved in self.reports {
            if let Some(agent) = progress.agent_mut(&saved.key)
                && agent.attempt == saved.attempt
                && agent.state == crate::progress::AgentState::Failed
            {
                agent.recovered_report = Some(saved.report.path);
            }
        }
    }
}

pub fn verdict(dir: &Path) -> Verdict {
    let status = files::read_json::<files::OutcomeFile>(&dir.join("outcome.json"))
        .ok()
        .and_then(|outcome| outcome.result)
        .and_then(|value| value.get("status")?.as_str().map(str::to_owned));
    let reports = fs::read_dir(dir.join("reports"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| files::read_json(&entry.path()).ok())
        .collect();
    Verdict { status, reports }
}

pub fn run_document(dir: &Path) -> io::Result<Value> {
    let outcome: Value = files::read_json(&dir.join("outcome.json"))?;
    Ok(outcome.get("result").cloned().unwrap_or(outcome))
}
