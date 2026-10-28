use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use hermes_client::{Recorder, RecorderOptions};
use serde_json::Value;

pub(crate) fn open(path: &Path, append: bool) -> Result<Recorder, String> {
    if !append {
        return Recorder::create(path, RecorderOptions::default()).map_err(|e| e.to_string());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut lines = bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty());
    let header: Value = serde_json::from_slice(
        lines
            .next()
            .ok_or_else(|| "recording has no header".to_owned())?,
    )
    .map_err(|e| e.to_string())?;
    if header["kind"] != "header" {
        return Err("recording has no header".into());
    }
    let (mut sequence, mut milliseconds) = (0, 0);
    for line in lines {
        let record: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        sequence = sequence.max(
            record["seq"]
                .as_u64()
                .ok_or_else(|| "recording has an invalid sequence".to_owned())?,
        );
        milliseconds = milliseconds.max(
            record["tMs"]
                .as_u64()
                .ok_or_else(|| "recording has an invalid timestamp".to_owned())?,
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|e| e.to_string())?;
    Recorder::from_writer(
        Append {
            file,
            sequence,
            milliseconds,
            header: false,
            buffer: Vec::new(),
        },
        RecorderOptions::default(),
    )
    .map_err(|e| e.to_string())
}

// The recorder still owns redaction and frame encoding. This writer joins the child's
// stream to its parent's recording while preserving one header and monotone counters.
struct Append {
    file: File,
    sequence: u64,
    milliseconds: u64,
    header: bool,
    buffer: Vec<u8>,
}
impl Write for Append {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.buffer.drain(..=end).collect();
            let mut value: Value = serde_json::from_slice(&line).map_err(io::Error::other)?;
            if !self.header {
                if value["kind"] != "header" {
                    return Err(io::Error::other("child recording has no header"));
                }
                self.header = true;
                continue;
            }
            let seq = value["seq"]
                .as_u64()
                .ok_or_else(|| io::Error::other("child recording sequence is invalid"))?;
            let t_ms = value["tMs"]
                .as_u64()
                .ok_or_else(|| io::Error::other("child recording timestamp is invalid"))?;
            value["seq"] = Value::from(self.sequence.saturating_add(seq));
            value["tMs"] = Value::from(self.milliseconds.saturating_add(t_ms));
            serde_json::to_writer(&mut self.file, &value)?;
            self.file.write_all(b"\n")?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_recording_keeps_one_header_and_increasing_sequences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frames.jsonl");
        let first = open(&path, false).unwrap();
        first.record_outbound(
            1,
            r#"{"jsonrpc":"2.0","id":1,"method":"runtime.describe","params":{}}"#,
        );
        first.finish().unwrap();
        let child = open(&path, true).unwrap();
        child.record_outbound(
            1,
            r#"{"jsonrpc":"2.0","id":2,"method":"runtime.describe","params":{}}"#,
        );
        child.finish().unwrap();
        let text = fs::read_to_string(path).unwrap();
        let rows: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["kind"], "header");
        assert_eq!(rows[1]["seq"], 1);
        assert_eq!(rows[2]["seq"], 2);
        assert!(rows[2]["tMs"].as_u64().unwrap() >= rows[1]["tMs"].as_u64().unwrap());
    }
}
