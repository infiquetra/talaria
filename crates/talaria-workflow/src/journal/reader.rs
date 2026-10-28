use super::{JournalError, JournalWarning, RecordLine, corrupt};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub struct JournalReader;
impl JournalReader {
    pub fn read_from(path: &Path, offset: u64) -> Result<(Vec<RecordLine>, u64), JournalError> {
        let path = if path.is_dir() {
            path.join("journal.jsonl")
        } else {
            path.into()
        };
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let (lines, used, _) = parse(&bytes, offset == 0)?;
        Ok((lines, offset + used))
    }
}
pub(crate) fn parse(
    bytes: &[u8],
    from_start: bool,
) -> Result<(Vec<RecordLine>, u64, Vec<JournalWarning>), JournalError> {
    let used = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut out = Vec::new();
    for chunk in bytes[..used].split_inclusive(|b| *b == b'\n') {
        let raw = &chunk[..chunk.len() - 1];
        let line = out.len() as u64 + 1;
        let value: serde_json::Value = serde_json::from_slice(raw).map_err(|e| corrupt(line, e))?;
        let record: RecordLine =
            serde_json::from_value(value.clone()).map_err(|e| corrupt(line, e))?;
        if record.v != 1 {
            return Err(corrupt(line, "unsupported record version"));
        }
        if from_start && record.seq != line {
            return Err(corrupt(line, "sequence gap"));
        }
        record.record.validate(line)?;
        let required: &[&str] = match record.record {
            super::Record::CallIntent { .. } => &["phase"],
            super::Record::TurnAdmitted { .. } => &["outcome", "execution_generation"],
            super::Record::CallAttention { .. } => {
                &["turn", "admission_id", "execution_generation"]
            }
            super::Record::TurnSettled { .. } => &["usage"],
            _ => &[],
        };
        for field in required {
            if value.get(field).is_none() {
                return Err(corrupt(line, format!("missing field {field}")));
            }
        }
        out.push(record);
    }
    let warnings = if used < bytes.len() {
        vec![JournalWarning::TornTail {
            line: out.len() as u64 + 1,
            bytes: (bytes.len() - used) as u64,
        }]
    } else {
        vec![]
    };
    Ok((out, used as u64, warnings))
}
