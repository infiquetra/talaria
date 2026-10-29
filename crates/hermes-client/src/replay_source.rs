//! Read a version 3 frame log and feed its inbound frames back with no socket.
//!
//! `run` sends text, not decoded values: the consumer decodes each item with
//! [`crate::decode_frame`], the same function the live socket reader uses.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::record::GatewayIdentity;

/// How fast `run` replays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Speed {
    Instant,
    Recorded,
    Factor(f64),
}

impl Speed {
    /// `instant`, `recorded`, or a positive number.
    pub fn parse(text: &str) -> Result<Speed, ReplayError> {
        match text {
            "instant" => Ok(Speed::Instant),
            "recorded" => Ok(Speed::Recorded),
            other => {
                let factor: f64 = other.parse().map_err(|_| ReplayError::BadSpeed)?;
                Speed::Factor(factor).checked()
            }
        }
    }

    fn checked(self) -> Result<Speed, ReplayError> {
        match self {
            Speed::Factor(factor) if !(factor.is_finite() && factor > 0.0) => {
                Err(ReplayError::BadSpeed)
            }
            speed => Ok(speed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    #[error("could not read the recording: {0}")]
    Io(String),
    #[error("unsupported frame log version {found}")]
    UnsupportedVersion { found: u64 },
    #[error("the first line is not a header")]
    NotAHeader,
    #[error("line {line} is not a valid record")]
    Corrupt { line: usize },
    #[error("sequence gap at {at}")]
    SeqGap { at: u64 },
    #[error("the replay speed must be above zero")]
    BadSpeed,
}

/// The first line of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameLogHeader {
    pub version: u64,
    pub started_at: String,
    pub endpoint: String,
    pub subprotocols: Vec<String>,
    pub gateway: Option<GatewayIdentity>,
    pub scenario: Option<String>,
    pub expected_statuses: Vec<String>,
}

/// One recorded frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayFrame {
    pub seq: u64,
    pub t_ms: u64,
    pub generation: u64,
    pub frame: serde_json::Value,
}

/// Frames lost by the recorder at a position in the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossRecord {
    pub seq: u64,
    pub t_ms: u64,
    pub count: u64,
}

/// One frame handed to the consumer: its generation and its JSON text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayItem {
    pub generation: u64,
    pub text: String,
}

#[derive(Debug)]
pub struct ReplaySource {
    header: FrameLogHeader,
    inbound: Vec<ReplayFrame>,
    outbound: Vec<ReplayFrame>,
    loss: Vec<LossRecord>,
    dropped_frames: u64,
    truncated: bool,
}

impl ReplaySource {
    /// Read and check the whole file. The error never names the path.
    pub fn open(path: &Path) -> Result<ReplaySource, ReplayError> {
        let bytes =
            std::fs::read(path).map_err(|error| ReplayError::Io(format!("{:?}", error.kind())))?;
        let last_newline = bytes.iter().rposition(|byte| *byte == b'\n');
        let truncated = last_newline.is_some_and(|last| last + 1 < bytes.len());
        let complete = if truncated {
            &bytes[..last_newline.map_or(0, |last| last + 1)]
        } else {
            &bytes
        };
        let text =
            std::str::from_utf8(complete).map_err(|_| ReplayError::Io("InvalidData".into()))?;
        let mut lines: Vec<&str> = text.split('\n').collect();
        if text.ends_with('\n') {
            lines.pop();
        }
        let Some((first, records)) = lines.split_first() else {
            return Err(ReplayError::NotAHeader);
        };
        let header = parse_header(first)?;
        let mut source = ReplaySource {
            header,
            inbound: Vec::new(),
            outbound: Vec::new(),
            loss: Vec::new(),
            dropped_frames: 0,
            truncated,
        };
        let mut expected_seq = 1_u64;
        for (index, line) in records.iter().enumerate() {
            let number = index + 2;
            let corrupt = ReplayError::Corrupt { line: number };
            let record: Value = serde_json::from_str(line).map_err(|_| corrupt.clone())?;
            let kind = record
                .get("kind")
                .and_then(Value::as_str)
                .ok_or(corrupt.clone())?;
            let seq = record.get("seq").and_then(Value::as_u64);
            match kind {
                "frame" | "dropped" if seq.is_none() => return Err(corrupt),
                _ => {}
            }
            if let Some(seq) = seq {
                if seq != expected_seq {
                    return Err(ReplayError::SeqGap { at: expected_seq });
                }
                expected_seq += 1;
            }
            if kind == "dropped" {
                let (Some(seq), Some(t_ms), Some(count)) = (
                    seq,
                    record.get("tMs").and_then(Value::as_u64),
                    record.get("count").and_then(Value::as_u64),
                ) else {
                    return Err(corrupt);
                };
                source.dropped_frames = source.dropped_frames.checked_add(count).ok_or(corrupt)?;
                source.loss.push(LossRecord { seq, t_ms, count });
                continue;
            }
            if kind != "frame" {
                continue;
            }
            let field = |name: &str| record.get(name).and_then(Value::as_u64);
            let (Some(seq), Some(t_ms), Some(generation)) =
                (seq, field("tMs"), field("generation"))
            else {
                return Err(corrupt);
            };
            let frame = record.get("frame").ok_or(corrupt.clone())?;
            let target = match record.get("dir").and_then(Value::as_str) {
                Some("in") => &mut source.inbound,
                Some("out") => &mut source.outbound,
                _ => return Err(corrupt),
            };
            // Text that was not JSON was recorded with a null frame; there is nothing to replay.
            if frame.is_null() {
                continue;
            }
            target.push(ReplayFrame {
                seq,
                t_ms,
                generation,
                frame: frame.clone(),
            });
        }
        Ok(source)
    }

    pub fn header(&self) -> &FrameLogHeader {
        &self.header
    }

    /// The inbound frames in `seq` order, without null frames.
    pub fn inbound(&self) -> impl Iterator<Item = &ReplayFrame> {
        self.inbound.iter()
    }

    /// The outbound frames in `seq` order, without null frames.
    pub fn outbound(&self) -> impl Iterator<Item = &ReplayFrame> {
        self.outbound.iter()
    }

    /// Loss records in log order; these are not gateway frames.
    pub fn loss_records(&self) -> &[LossRecord] {
        &self.loss
    }

    /// Total frames the recorder reported losing.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }

    /// True when the last line had no newline and was ignored.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Send each inbound frame into `sink`, paced by `speed` on `tokio::time`. Returns how many
    /// were sent; a closed sink ends the run early without an error.
    pub async fn run(
        &self,
        speed: Speed,
        sink: mpsc::Sender<ReplayItem>,
    ) -> Result<usize, ReplayError> {
        let speed = speed.checked()?;
        let started = tokio::time::Instant::now();
        let mut sent = 0;
        for frame in &self.inbound {
            let due = match speed {
                Speed::Instant => None,
                Speed::Recorded => Some(Duration::from_millis(frame.t_ms)),
                Speed::Factor(factor) => Some(Duration::from_micros(
                    (frame.t_ms as f64 * 1_000.0 / factor).round() as u64,
                )),
            };
            if let Some(due) = due {
                tokio::select! {
                    () = sink.closed() => break,
                    () = tokio::time::sleep_until(started + due) => {}
                }
            }
            let item = ReplayItem {
                generation: frame.generation,
                text: frame.frame.to_string(),
            };
            if sink.send(item).await.is_err() {
                break;
            }
            sent += 1;
        }
        Ok(sent)
    }
}

fn parse_header(line: &str) -> Result<FrameLogHeader, ReplayError> {
    let value: Value = serde_json::from_str(line).map_err(|_| ReplayError::NotAHeader)?;
    if value.get("kind").and_then(Value::as_str) != Some("header") {
        return Err(ReplayError::NotAHeader);
    }
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or(ReplayError::NotAHeader)?;
    if version != 3 {
        return Err(ReplayError::UnsupportedVersion { found: version });
    }
    let text = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_owned);
    let strings = |name: &str| -> Vec<String> {
        value
            .get(name)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let gateway = value.get("gateway").and_then(|gateway| {
        Some(GatewayIdentity {
            code_version: gateway.get("codeVersion")?.as_str()?.to_owned(),
            code_sha: gateway.get("codeSha")?.as_str()?.to_owned(),
        })
    });
    Ok(FrameLogHeader {
        version,
        started_at: text("startedAt").unwrap_or_default(),
        endpoint: text("endpoint").unwrap_or_default(),
        subprotocols: strings("subprotocols"),
        gateway,
        scenario: text("scenario"),
        expected_statuses: strings("expectedStatuses"),
    })
}
