//! One run's agents as the command line names them: by label or key, read from the journal.
//! Asking for a failed agent to run again and reading a finished agent's kept value use it.
use std::{collections::BTreeMap, path::Path};

use serde::Serialize;
use serde_json::Value;

use crate::{
    engine::AgentOutcome,
    journal::{CallOutcome, JournalError, Record, RecordLine},
};

use super::{
    control::{self, ControlWrite},
    files,
    runs::{self, FindError},
};

/// What the journal says about one agent call: its latest attempt only.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEntry {
    pub key: String,
    pub label: String,
    /// The latest record's kind, for example `call_finished` or `call_attention`.
    pub last: &'static str,
    /// The kept outcome when the latest record is a finish.
    pub outcome: Option<CallOutcome>,
    /// The journal line of that finish, which a value kept in a blob needs.
    seq: u64,
}
impl AgentEntry {
    /// The agent's state in one word: `completed`, `failed`, `skipped`, `stopped`, `parked`
    /// or `unfinished`.
    pub fn state(&self) -> &'static str {
        match (&self.outcome, self.last) {
            (Some(outcome), _) => outcome.kind(),
            (None, "call_attention") => "parked",
            _ => "unfinished",
        }
    }
    fn kept(&self) -> bool {
        matches!(
            self.outcome,
            Some(CallOutcome::Completed { .. } | CallOutcome::Skipped)
        )
    }
    fn restartable(&self) -> bool {
        matches!(
            self.outcome,
            Some(CallOutcome::Failed { .. } | CallOutcome::Stopped { .. })
        )
    }
}

/// Every agent call the journal names, in the order each first appeared.
pub fn agents(records: &[RecordLine]) -> Vec<AgentEntry> {
    let mut order: Vec<String> = Vec::new();
    let mut entries: BTreeMap<String, AgentEntry> = BTreeMap::new();
    for line in records {
        let Some((key, _)) = line.record.identity() else {
            continue;
        };
        if !entries.contains_key(key) {
            order.push(key.to_owned());
        }
        let entry = entries.entry(key.to_owned()).or_insert_with(|| AgentEntry {
            key: key.to_owned(),
            label: String::new(),
            last: "",
            outcome: None,
            seq: 0,
        });
        if let Record::CallIntent { label, .. } = &line.record {
            entry.label = label.clone();
        }
        entry.last = line.record.kind();
        entry.seq = line.seq;
        entry.outcome = match &line.record {
            Record::CallFinished { outcome, .. } => Some(outcome.clone()),
            _ => None,
        };
    }
    order
        .into_iter()
        .filter_map(|key| entries.remove(&key))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("{0}")]
    Target(#[from] FindError),
    #[error("{0}")]
    Journal(#[from] JournalError),
    #[error("run {run} has no agent labelled or keyed '{name}'")]
    Unknown { run: String, name: String },
    #[error(
        "run {run} has {} agents labelled '{name}'; name one by its key: {}",
        keys.len(),
        keys.join(", ")
    )]
    Ambiguous {
        run: String,
        name: String,
        keys: Vec<String>,
    },
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// The agent a name picks: its key, or a label only one agent has.
fn pick<'a>(run: &str, all: &'a [AgentEntry], name: &str) -> Result<&'a AgentEntry, AgentError> {
    if let Some(entry) = all.iter().find(|e| e.key == name) {
        return Ok(entry);
    }
    let labelled: Vec<_> = all.iter().filter(|e| e.label == name).collect();
    match labelled.as_slice() {
        [one] => Ok(one),
        [] => Err(AgentError::Unknown {
            run: run.into(),
            name: name.into(),
        }),
        many => Err(AgentError::Ambiguous {
            run: run.into(),
            name: name.into(),
            keys: many.iter().map(|e| e.key.clone()).collect(),
        }),
    }
}

fn open(
    runs_dir: &Path,
    target: &str,
) -> Result<(std::path::PathBuf, String, Vec<AgentEntry>), AgentError> {
    let dir = runs::find_run(runs_dir, target)?;
    let run = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let records = control::read_records(&dir)?;
    Ok((dir, run, agents(&records)))
}

/// Which agents a restart asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestartTarget {
    /// One agent, by label or key.
    Agent(String),
    /// Every agent whose latest attempt failed or was stopped by the budget.
    AllFailed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RestartRequested {
    pub label: String,
    pub key: String,
    /// False when a restart for this agent was already pending.
    pub written: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RestartReport {
    pub run_id: String,
    pub requested: Vec<RestartRequested>,
    /// Other failed agents. A resume opens a new execution, and a new execution runs every
    /// failed agent the script reaches again, not only the ones asked for.
    pub also_run_again: Vec<String>,
}

/// Write the same restart request the workflow view's restart key writes, for agents that
/// failed. A finished agent is refused: it is kept, and running it again would pay for its
/// work twice. The request waits for the next `talaria workflow resume`.
pub fn request_restart(
    runs_dir: &Path,
    target: &str,
    which: &RestartTarget,
) -> Result<RestartReport, AgentError> {
    let (dir, run, all) = open(runs_dir, target)?;
    let chosen: Vec<&AgentEntry> = match which {
        RestartTarget::Agent(name) => {
            let entry = pick(&run, &all, name)?;
            if entry.kept() {
                return Err(AgentError::Refused(format!(
                    "agent {} {}: finished agents are kept — restart would pay for the work twice",
                    entry.label,
                    entry.state()
                )));
            }
            if !entry.restartable() {
                return Err(AgentError::Refused(format!(
                    "agent {} is {}: a restart is for an agent that failed; use the restart key in the workflow view for a running agent, or talaria workflow resolve for a parked one",
                    entry.label,
                    entry.state()
                )));
            }
            vec![entry]
        }
        RestartTarget::AllFailed => {
            let failed: Vec<_> = all.iter().filter(|e| e.restartable()).collect();
            if failed.is_empty() {
                return Err(AgentError::Refused(format!(
                    "run {run} has no failed agents to restart"
                )));
            }
            failed
        }
    };
    let at_ms = files::now_ms();
    let mut requested = Vec::new();
    for entry in &chosen {
        let written = control::write_control(&dir, "restart", Some(&entry.key), at_ms)?;
        requested.push(RestartRequested {
            label: entry.label.clone(),
            key: entry.key.clone(),
            written: written == ControlWrite::Written,
        });
    }
    let also_run_again = all
        .iter()
        .filter(|e| e.restartable() && !chosen.iter().any(|c| c.key == e.key))
        .map(|e| e.label.clone())
        .collect();
    Ok(RestartReport {
        run_id: run,
        requested,
        also_run_again,
    })
}

/// One agent's kept result, read from the journal without running anything.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AgentResult {
    pub run_id: String,
    pub label: String,
    pub key: String,
    pub state: &'static str,
    /// The value the agent's call returned to the script, when it completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// Why it did not complete, when the journal says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The mark of an agent Hermes stopped at its step cap whose reply was kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cut_short: Option<String>,
}

/// The kept value of one agent of a run, by label or key.
pub fn agent_result(runs_dir: &Path, target: &str, name: &str) -> Result<AgentResult, AgentError> {
    let (dir, run, all) = open(runs_dir, target)?;
    let entry = pick(&run, &all, name)?;
    let mut result = AgentResult {
        run_id: run,
        label: entry.label.clone(),
        key: entry.key.clone(),
        state: entry.state(),
        value: None,
        reason: None,
        cut_short: None,
    };
    if let Some(outcome) = &entry.outcome {
        match crate::journal::stored_outcome(&dir, outcome, entry.seq)? {
            AgentOutcome::Completed(value) => result.value = Some(value),
            AgentOutcome::CutShort { value, cut_short } => {
                result.value = Some(value);
                result.cut_short = Some(cut_short.describe());
            }
            AgentOutcome::Failed { reason }
            | AgentOutcome::Stopped { reason }
            | AgentOutcome::Abort { reason } => result.reason = Some(reason),
            AgentOutcome::Skipped => {}
        }
    }
    Ok(result)
}
