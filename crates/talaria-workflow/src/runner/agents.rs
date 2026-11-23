//! One run's agents as the command line names them: by label or key, read from the journal.
//! Asking for a failed agent to run again and reading a finished agent's kept value use it.
use std::{collections::BTreeMap, path::Path};

use serde::Serialize;
use serde_json::Value;

use crate::{
    engine::AgentOutcome,
    journal::{CallOutcome, JournalError, Record, RecordLine, stopped_outcome},
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
    /// Whether a restart request can be granted for it, read from its latest record.
    restartable: bool,
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
    /// Whether its latest attempt failed or the budget stopped it: a new execution runs such
    /// an agent again when the script reaches it, asked for or not.
    fn failed(&self) -> bool {
        matches!(
            self.outcome,
            Some(CallOutcome::Failed { .. } | CallOutcome::Stopped { .. })
        )
    }
    /// Why a restart request for it is refused, or `None` when one can be granted.
    fn refusal(&self) -> Option<String> {
        if self.restartable {
            return None;
        }
        Some(if self.kept() {
            format!(
                "agent {} {}: finished agents are kept — restart would pay for the work twice",
                self.label,
                self.state()
            )
        } else if self.last == "call_attention" {
            format!(
                "agent {} is parked: answer it with talaria workflow resolve; the rerun answer runs it again",
                self.label
            )
        } else {
            format!(
                "agent {} is unfinished: talaria workflow resume continues it; if it then fails, ask again",
                self.label
            )
        })
    }
}

/// Whether an agent whose latest record is `last` can be restarted: its attempt failed or was
/// stopped, a stop that was journaled ended its turn, or it waits after an operator's answer.
/// Anything else is either kept, parked for the operator, or still unfinished: a resume
/// continues an unfinished agent, and restarting one whose turn completed would throw its
/// paid reply away.
fn restartable(last: &Record) -> bool {
    match last {
        Record::CallFinished { outcome, .. } => matches!(
            outcome,
            CallOutcome::Failed { .. } | CallOutcome::Stopped { .. }
        ),
        Record::TurnSettled { outcome, .. } => stopped_outcome(outcome),
        Record::CallResolved { .. } => true,
        _ => false,
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
            restartable: false,
        });
        if let Record::CallIntent { label, .. } = &line.record {
            entry.label = label.clone();
        }
        entry.last = line.record.kind();
        entry.seq = line.seq;
        entry.restartable = restartable(&line.record);
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
    /// Every agent a restart by name accepts: its latest attempt failed or was stopped by the
    /// budget, a journaled stop ended its turn, or it waits after a use-reply answer.
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
/// failed, were stopped, or wait after a use-reply answer. A finished agent is refused: it is
/// kept, and running it again would pay for its work twice. A parked agent is refused: its
/// answer belongs to `talaria workflow resolve`. Any other unfinished agent is refused: a resume
/// continues it. The request waits for the next `talaria workflow resume`.
pub fn request_restart(
    runs_dir: &Path,
    target: &str,
    which: &RestartTarget,
) -> Result<RestartReport, AgentError> {
    let (dir, run, all) = open(runs_dir, target)?;
    let chosen: Vec<&AgentEntry> = match which {
        RestartTarget::Agent(name) => {
            let entry = pick(&run, &all, name)?;
            if let Some(why) = entry.refusal() {
                return Err(AgentError::Refused(why));
            }
            vec![entry]
        }
        RestartTarget::AllFailed => {
            let failed: Vec<_> = all.iter().filter(|e| e.restartable).collect();
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
        .filter(|e| e.failed() && !chosen.iter().any(|c| c.key == e.key))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{AdmissionState, AttentionCause, Resolution, ResolutionSource};
    use serde_json::json;

    fn line(seq: u64, record: Record) -> RecordLine {
        RecordLine {
            v: 1,
            seq,
            at_ms: 0,
            record,
        }
    }
    fn key(n: u8) -> String {
        char::from(b'a' + n).to_string().repeat(32)
    }
    fn settled(key: String, outcome: &str, meta: Value) -> Record {
        Record::TurnSettled {
            key,
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            outcome: outcome.into(),
            final_text: None,
            final_text_ref: None,
            result_meta: meta,
            usage: None,
        }
    }
    fn finished(key: String, outcome: CallOutcome) -> Record {
        Record::CallFinished {
            key,
            attempt: 0,
            turns: 1,
            outcome,
        }
    }

    #[test]
    fn a_restart_accepts_only_a_failed_stopped_or_answered_agent() {
        let failed = || CallOutcome::Failed {
            reason: "agent failed".into(),
        };
        let records = vec![
            line(1, finished(key(0), failed())),
            line(
                2,
                finished(
                    key(1),
                    CallOutcome::Stopped {
                        reason: "token budget exhausted".into(),
                    },
                ),
            ),
            // The run's own stop settled this turn; the call never finished.
            line(
                3,
                settled(key(2), "interrupted", json!({"stopped_by": "run_stop"})),
            ),
            line(4, settled(key(3), "cancelled", json!({}))),
            // Answered with use-reply, this one waits for a reply.
            line(
                5,
                Record::CallResolved {
                    key: key(4),
                    attempt: 0,
                    episode: 0,
                    resolution: Resolution::UseReply,
                    source: ResolutionSource::Operator,
                },
            ),
            // Its turn completed: a resume keeps the paid reply.
            line(6, settled(key(5), "completed", json!({}))),
            // A retry is already due: a plain resume runs it again.
            line(
                7,
                Record::CallRetryFinished {
                    key: key(6),
                    attempt: 0,
                    outcome: failed(),
                    turns: 1,
                },
            ),
            line(
                8,
                Record::CallAttention {
                    key: key(7),
                    attempt: 0,
                    episode: 0,
                    cause: AttentionCause::Stopped,
                    turn: Some(0),
                    admission_id: None,
                    execution_generation: None,
                    detail: "stopped".into(),
                    seen_admissions: Vec::new(),
                },
            ),
            line(
                9,
                Record::TurnAdmitted {
                    key: key(8),
                    attempt: 0,
                    turn: 0,
                    admission_id: "admission".into(),
                    status: AdmissionState::Started,
                    outcome: None,
                    execution_generation: None,
                    authority_epoch: 0,
                },
            ),
            line(
                10,
                Record::TurnIntent {
                    key: key(9),
                    attempt: 0,
                    turn: 0,
                    submission_id: "submission".into(),
                    text: Some("prompt".into()),
                    text_ref: None,
                },
            ),
            line(
                11,
                finished(
                    key(10),
                    CallOutcome::Completed {
                        value: Some(json!("kept")),
                        value_ref: None,
                        cut_short: None,
                    },
                ),
            ),
        ];
        let all = agents(&records);
        let restartable: Vec<bool> = all.iter().map(|e| e.restartable).collect();
        assert_eq!(
            restartable,
            [
                true, true, true, true, true, false, false, false, false, false, false
            ]
        );
        let refusal = |n: usize| all[n].refusal().unwrap_or_default();
        assert!(
            refusal(5)
                .contains("talaria workflow resume continues it; if it then fails, ask again")
        );
        assert!(refusal(6).contains("talaria workflow resume continues it"));
        assert!(
            refusal(7).contains(
                "answer it with talaria workflow resolve; the rerun answer runs it again"
            )
        );
        assert!(refusal(8).contains("talaria workflow resume continues it"));
        assert!(refusal(9).contains("talaria workflow resume continues it"));
        assert!(refusal(10).contains("finished agents are kept"));
    }

    #[test]
    fn a_completed_turn_or_a_due_retry_is_refused_by_name_and_left_out_of_every_failed() {
        let temp = tempfile::tempdir().unwrap();
        let run = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let dir = temp.path().join(run);
        std::fs::create_dir_all(&dir).unwrap();
        let intent = |n: u8, label: &str| Record::CallIntent {
            key: key(n),
            attempt: 0,
            call_id: crate::engine::CallId {
                digest: "0".repeat(64),
                occurrence: u32::from(n),
            },
            label: label.into(),
            phase: None,
            workflow_path: Vec::new(),
            launch: crate::journal::ResolvedLaunch {
                profile: "fixture".into(),
                model: "fixture-model".into(),
                provider: None,
                effort: None,
                toolsets: Vec::new(),
                skills: Vec::new(),
                max_turns: None,
                isolation: None,
                cwd: None,
            },
            request_id: format!("request-{n}"),
        };
        let records = [
            Record::RunStarted {
                run_id: run.into(),
                script_sha256: "0".repeat(64),
            },
            intent(0, "answered"),
            settled(key(0), "completed", json!({})),
            intent(1, "retrying"),
            Record::CallRetryFinished {
                key: key(1),
                attempt: 0,
                outcome: CallOutcome::Failed {
                    reason: "rate limited".into(),
                },
                turns: 1,
            },
        ];
        let text: String = records
            .into_iter()
            .enumerate()
            .map(|(n, record)| serde_json::to_string(&line(n as u64 + 1, record)).unwrap() + "\n")
            .collect();
        std::fs::write(dir.join("journal.jsonl"), text).unwrap();
        for name in ["answered", "retrying"] {
            let refused = request_restart(temp.path(), run, &RestartTarget::Agent(name.into()));
            assert!(
                matches!(&refused, Err(AgentError::Refused(why)) if why.contains("talaria workflow resume continues it")),
                "{name}: {refused:?}"
            );
        }
        let none = request_restart(temp.path(), run, &RestartTarget::AllFailed);
        assert!(
            matches!(&none, Err(AgentError::Refused(why)) if why.contains("has no failed agents")),
            "{none:?}"
        );
        assert!(!dir.join("control").exists());
    }
}
