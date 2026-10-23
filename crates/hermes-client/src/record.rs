//! Redacting frame recorder, frame log version 3.
//!
//! Red-run stubs for P1-04: every body is `todo!("P1-04")`.

use std::io::Write;
use std::path::Path;

/// The frame log layout this recorder writes.
pub const FRAME_LOG_VERSION: u32 = 3;

/// The gateway build identifiers that `identify` reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayIdentity {
    pub code_version: String,
    pub code_sha: String,
}

/// What the header records and how a recording is processed.
#[derive(Debug, Clone, Default)]
pub struct RecorderOptions {
    pub endpoint: String,
    pub subprotocols: Vec<String>,
    pub gateway: Option<GatewayIdentity>,
    pub scenario: Option<String>,
    pub expected_statuses: Vec<String>,
    pub substitutions: Vec<(String, String)>,
    /// Frames the queue holds before it drops; `None` means 4,096.
    pub queue_capacity: Option<usize>,
}

/// What `finish` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordSummary {
    pub frames: usize,
    pub redactions: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    #[error("the recording file already exists")]
    AlreadyExists,
    #[error("a substitution needle is shorter than four characters")]
    NeedleTooShort,
    #[error("recording failed: {0:?}")]
    Io(std::io::ErrorKind),
}

/// A cheap, cloneable handle on one recording.
#[derive(Debug, Clone)]
pub struct Recorder {
    _private: (),
}

impl Recorder {
    pub fn create(path: &Path, options: RecorderOptions) -> Result<Recorder, RecordError> {
        let _ = (path, options);
        todo!("P1-04")
    }

    pub fn from_writer<W: Write + Send + 'static>(
        writer: W,
        options: RecorderOptions,
    ) -> Result<Recorder, RecordError> {
        let _ = (writer, options);
        todo!("P1-04")
    }

    pub fn record_inbound(&self, generation: u64, text: &str) {
        let _ = (generation, text);
        todo!("P1-04")
    }

    pub fn record_outbound(&self, generation: u64, text: &str) {
        let _ = (generation, text);
        todo!("P1-04")
    }

    pub fn finish(self) -> Result<RecordSummary, RecordError> {
        todo!("P1-04")
    }
}
