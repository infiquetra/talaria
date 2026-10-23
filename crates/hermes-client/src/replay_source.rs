//! Read a version 3 frame log and feed its inbound frames back with no socket.
//!
//! Red-run stubs for P1-04: every body is `todo!("P1-04")`.

use std::path::Path;

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
        let _ = text;
        todo!("P1-04")
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

/// One frame handed to the consumer: its generation and its JSON text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayItem {
    pub generation: u64,
    pub text: String,
}

#[derive(Debug)]
pub struct ReplaySource {
    _private: (),
}

impl ReplaySource {
    pub fn open(path: &Path) -> Result<ReplaySource, ReplayError> {
        let _ = path;
        todo!("P1-04")
    }

    pub fn header(&self) -> &FrameLogHeader {
        todo!("P1-04")
    }

    pub fn inbound(&self) -> impl Iterator<Item = &ReplayFrame> {
        let none: Option<&ReplayFrame> = None;
        if true {
            todo!("P1-04")
        }
        none.into_iter()
    }

    pub fn outbound(&self) -> impl Iterator<Item = &ReplayFrame> {
        let none: Option<&ReplayFrame> = None;
        if true {
            todo!("P1-04")
        }
        none.into_iter()
    }

    /// True when the last line had no newline and was ignored.
    pub fn truncated(&self) -> bool {
        todo!("P1-04")
    }

    pub async fn run(
        &self,
        speed: Speed,
        sink: mpsc::Sender<ReplayItem>,
    ) -> Result<usize, ReplayError> {
        let _ = (speed, sink);
        todo!("P1-04")
    }
}
