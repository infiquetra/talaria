//! Offset tailing delegates torn-line and corruption policy to the journal reader.
use super::RunProgress;
use crate::journal::{JournalError, JournalReader, RunDir};
pub const POLL_MS: u64 = 500;
pub struct Tail {
    dir: RunDir,
    pub offset: u64,
}
impl Tail {
    pub fn open(dir: &RunDir) -> Result<(RunProgress, Self), JournalError> {
        let (lines, offset) = JournalReader::read_from(&dir.path().join("journal.jsonl"), 0)?;
        Ok((
            RunProgress::from_records(dir.manifest(), &lines),
            Self {
                dir: dir.clone(),
                offset,
            },
        ))
    }
    pub fn poll(&mut self, progress: &mut RunProgress) -> Result<usize, JournalError> {
        let (lines, offset) =
            JournalReader::read_from(&self.dir.path().join("journal.jsonl"), self.offset)
                .map_err(|error| absolute_error(error, progress.last_seq))?;
        check_sequence(&lines, progress.last_seq)?;
        let count = lines.len();
        progress.apply_all(&lines);
        self.offset = offset;
        Ok(count)
    }
}
pub fn absolute_error(error: JournalError, last_seq: u64) -> JournalError {
    match error {
        JournalError::Corrupt { line, reason } => JournalError::Corrupt {
            line: last_seq.saturating_add(line),
            reason,
        },
        error => error,
    }
}
pub fn check_sequence(
    lines: &[crate::journal::RecordLine],
    last_seq: u64,
) -> Result<(), JournalError> {
    for (i, line) in lines.iter().enumerate() {
        let expected = last_seq.saturating_add(i as u64 + 1);
        if line.seq != expected {
            return Err(JournalError::Corrupt {
                line: expected,
                reason: "sequence gap".into(),
            });
        }
    }
    Ok(())
}
