use super::{JournalError, JournalWarning, RecordLine, corrupt};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(crate) const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;

fn invalid_file(reason: &str) -> JournalError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason).into()
}

pub struct JournalReader;
pub(crate) fn read_bytes(path: &Path, offset: u64) -> Result<Vec<u8>, JournalError> {
    let path = if path.is_dir() {
        path.join("journal.jsonl")
    } else {
        path.to_path_buf()
    };
    let before = fs::symlink_metadata(&path)?;
    if !before.is_file() || before.len() > MAX_JOURNAL_BYTES || offset > MAX_JOURNAL_BYTES {
        return Err(invalid_file(
            "journal must be a regular file no larger than 64 MiB",
        ));
    }
    let mut file = crate::untrusted_file::open_read_only(&path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() > MAX_JOURNAL_BYTES {
        return Err(invalid_file(
            "journal must be a regular file no larger than 64 MiB",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != before.dev() || opened.ino() != before.ino() {
            return Err(invalid_file("journal changed while it was opened"));
        }
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES - offset + 1)
        .read_to_end(&mut bytes)?;
    if offset + bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(invalid_file("journal exceeds 64 MiB"));
    }
    Ok(bytes)
}

impl JournalReader {
    pub fn read_from(path: &Path, offset: u64) -> Result<(Vec<RecordLine>, u64), JournalError> {
        let bytes = read_bytes(path, offset)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_script_hashes_and_oversized_journals() {
        let temp = tempfile::tempdir().unwrap();
        let journal = temp.path().join("journal.jsonl");
        fs::write(
            &journal,
            br#"{"v":1,"seq":1,"at_ms":0,"t":"run_started","run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C","script_sha256":"../../outside"}
"#,
        )
        .unwrap();
        assert!(matches!(
            JournalReader::read_from(&journal, 0),
            Err(JournalError::Corrupt { .. })
        ));

        fs::File::create(&journal)
            .unwrap()
            .set_len(MAX_JOURNAL_BYTES + 1)
            .unwrap();
        assert!(matches!(
            JournalReader::read_from(&journal, 0),
            Err(JournalError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_instead_of_following_it() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::write(&target, b"").unwrap();
        let journal = temp.path().join("journal.jsonl");
        std::os::unix::fs::symlink(target, &journal).unwrap();
        assert!(matches!(
            JournalReader::read_from(&journal, 0),
            Err(JournalError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData
        ));
    }
}
