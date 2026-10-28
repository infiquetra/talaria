//! Private, write-ahead workflow journals. Production code depends only on ports.
mod blobs;
mod durable;
#[cfg(feature = "fake-gateway")]
pub mod fake_gateway;
mod keys;
mod port;
mod reader;
mod record;
mod replay;
mod run_dir;
mod store;
pub use durable::*;
pub use keys::*;
pub use port::*;
pub use reader::*;
pub use record::*;
pub use replay::*;
pub use run_dir::*;
use serde::{Deserialize, Serialize};
pub use store::{RunJournal, testing};
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("run {run_id} is already open in another runner")]
    Locked { run_id: String },
    #[error("run {run_id} already exists")]
    Exists { run_id: String },
    #[error("journal is corrupt at line {line}: {reason}")]
    Corrupt { line: u64, reason: String },
    #[error("journal is closed")]
    Closed,
    #[error("journal record too large: {t} ({bytes} bytes)")]
    RecordTooLarge { t: String, bytes: u64 },
    #[error("resolution pending: {file}")]
    ResolutionPending { file: String },
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Manifest(String),
    #[error("bad run id: {0}")]
    BadRunId(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JournalWarning {
    TornTail { line: u64, bytes: u64 },
    StaleResolution { key: String },
    BadResolution { file: String, reason: String },
}
pub(crate) fn corrupt(line: u64, reason: impl ToString) -> JournalError {
    JournalError::Corrupt {
        line,
        reason: reason.to_string(),
    }
}
pub(crate) fn io_error(error: impl ToString) -> JournalError {
    JournalError::Io(std::io::Error::other(error.to_string()))
}
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
pub(crate) fn cut(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}
