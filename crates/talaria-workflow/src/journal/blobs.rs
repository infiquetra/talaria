use super::{BlobRef, CallOutcome, JournalError, Record, corrupt, keys::sha, run_dir::immutable};
use serde_json::Value;
use std::{fs, path::Path};
pub(crate) const INLINE_LIMIT: usize = 256 * 1024;
pub(crate) const RECORD_LIMIT: usize = 1024 * 1024;
pub(crate) fn put(dir: &Path, bytes: &[u8]) -> Result<BlobRef, JournalError> {
    let sha256 = sha(bytes);
    immutable(&dir.join("blobs"), &sha256, bytes)?;
    Ok(BlobRef {
        sha256,
        bytes: bytes.len() as u64,
    })
}
pub(crate) fn externalize(dir: &Path, record: &mut Record) -> Result<(), JournalError> {
    match record {
        Record::Engine { event, event_ref } => {
            if let Some(value) = event.as_ref() {
                let bytes = serde_json::to_vec(value).map_err(super::io_error)?;
                if bytes.len() > INLINE_LIMIT {
                    *event_ref = Some(put(dir, &bytes)?);
                    *event = None;
                }
            }
        }
        Record::TurnIntent { text, text_ref, .. } => external_text(dir, text, text_ref)?,
        Record::TurnSettled {
            final_text,
            final_text_ref,
            ..
        } => external_text(dir, final_text, final_text_ref)?,
        Record::CallFinished {
            outcome: CallOutcome::Completed {
                value, value_ref, ..
            },
            ..
        } => {
            if let Some(v) = value.as_ref() {
                let bytes = serde_json::to_vec(v).map_err(super::io_error)?;
                if bytes.len() > INLINE_LIMIT {
                    *value_ref = Some(put(dir, &bytes)?);
                    *value = None;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn external_text(
    dir: &Path,
    text: &mut Option<String>,
    reference: &mut Option<BlobRef>,
) -> Result<(), JournalError> {
    if let Some(s) = text.as_ref()
        && serde_json::to_vec(s).map_err(super::io_error)?.len() > INLINE_LIMIT
    {
        *reference = Some(put(dir, s.as_bytes())?);
        *text = None;
    }
    Ok(())
}
pub(crate) fn check_size(dir: &Path, r: &BlobRef, line: u64) -> Result<(), JournalError> {
    if r.sha256.len() != 64
        || !r
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(corrupt(line, "invalid blob hash"));
    }
    let m = fs::metadata(dir.join("blobs").join(&r.sha256))
        .map_err(|e| corrupt(line, format!("blob {}: {e}", r.sha256)))?;
    if !m.is_file() || m.len() != r.bytes {
        return Err(corrupt(line, format!("blob {} has wrong size", r.sha256)));
    }
    Ok(())
}
pub(crate) fn load(dir: &Path, r: &BlobRef, line: u64) -> Result<Vec<u8>, JournalError> {
    check_size(dir, r, line)?;
    let bytes = fs::read(dir.join("blobs").join(&r.sha256)).map_err(|e| corrupt(line, e))?;
    if sha(&bytes) != r.sha256 {
        return Err(corrupt(
            line,
            format!("journal blob {} failed its hash check", r.sha256),
        ));
    }
    Ok(bytes)
}
pub(crate) fn text(
    dir: &Path,
    inline: &Option<String>,
    r: &Option<BlobRef>,
    line: u64,
) -> Result<Option<String>, JournalError> {
    match r {
        Some(r) => String::from_utf8(load(dir, r, line)?)
            .map(Some)
            .map_err(|e| corrupt(line, e)),
        None => Ok(inline.clone()),
    }
}
pub(crate) fn outcome(
    dir: &Path,
    outcome: &CallOutcome,
    line: u64,
) -> Result<crate::engine::AgentOutcome, JournalError> {
    use crate::engine::AgentOutcome;
    match outcome {
        CallOutcome::Completed {
            value,
            value_ref,
            cut_short,
        } => {
            let value = match value_ref {
                Some(r) => serde_json::from_slice::<Value>(&load(dir, r, line)?)
                    .map_err(|e| corrupt(line, e))?,
                None => value
                    .clone()
                    .ok_or_else(|| corrupt(line, "missing completed value"))?,
            };
            Ok(match cut_short {
                Some(cut_short) => AgentOutcome::CutShort {
                    value,
                    cut_short: cut_short.clone(),
                },
                None => AgentOutcome::Completed(value),
            })
        }
        CallOutcome::Failed { reason } => Ok(AgentOutcome::Failed {
            reason: reason.clone(),
        }),
        CallOutcome::FailedWithReport {
            reason,
            path,
            report_ref,
        } => {
            let value = serde_json::from_slice(&super::load_report_blob(dir, report_ref, line)?)
                .map_err(|e| corrupt(line, e))?;
            Ok(AgentOutcome::FailedWithReport {
                reason: reason.clone(),
                value,
                report: crate::engine::RecoveredReport {
                    path: path.clone(),
                    sha256: report_ref.sha256.clone(),
                    bytes: report_ref.bytes,
                },
            })
        }
        CallOutcome::Skipped => Ok(AgentOutcome::Skipped),
        CallOutcome::Stopped { reason } => Ok(AgentOutcome::Stopped {
            reason: reason.clone(),
        }),
    }
}
pub(crate) fn event(
    dir: &Path,
    record: &Record,
    line: u64,
) -> Result<Option<crate::engine::EngineEvent>, JournalError> {
    match record {
        Record::Engine { event, event_ref } => match event_ref {
            Some(r) => serde_json::from_slice(&load(dir, r, line)?)
                .map(Some)
                .map_err(|e| corrupt(line, e)),
            None => Ok(event.clone()),
        },
        _ => Ok(None),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{AgentOutcome, CutShort};
    use serde_json::json;

    #[test]
    fn a_cut_short_call_is_stored_as_completed_with_the_mark_and_replays_marked() {
        let cut_short = CutShort {
            steps: Some(60),
            limit: Some(60),
            saved_branch: Some("talaria/wf/run/call-0-saved".into()),
        };
        let stored = CallOutcome::from_agent(AgentOutcome::CutShort {
            value: json!("partial"),
            cut_short: cut_short.clone(),
        });
        // The call_finished record names the branch that holds the agent's saved work.
        assert_eq!(
            serde_json::to_value(&stored).unwrap(),
            json!({"kind": "completed", "value": "partial", "cut_short": {"steps": 60, "limit": 60,
                "saved_branch": "talaria/wf/run/call-0-saved"}})
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            outcome(dir.path(), &stored, 1).unwrap(),
            AgentOutcome::CutShort {
                value: json!("partial"),
                cut_short
            }
        );
    }

    #[test]
    fn a_completed_call_has_no_mark_and_an_older_record_reads_as_completed() {
        let plain = CallOutcome::from_agent(AgentOutcome::Completed(json!("whole")));
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            json!({"kind": "completed", "value": "whole"})
        );
        let older: CallOutcome =
            serde_json::from_value(json!({"kind": "completed", "value": "whole"})).unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            outcome(dir.path(), &older, 1).unwrap(),
            AgentOutcome::Completed(json!("whole"))
        );
    }
}
