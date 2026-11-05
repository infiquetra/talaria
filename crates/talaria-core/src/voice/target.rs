//! The fleet row a recording is bound to when it starts.

use crate::fleet::{FleetBackend, NeedId, RowKey};
use crate::voice::SubmitMode;

/// One needs-you item that could receive the recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueCandidate {
    /// The item and the row it belongs to.
    pub id: NeedId,
    /// The backend that owns the row.
    pub backend: FleetBackend,
    /// Higher means newer. Selection wins over recency.
    pub seq: u64,
    /// The operator has this item selected.
    pub selected: bool,
}

/// One fleet row that could receive the recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowCandidate {
    /// The row.
    pub key: RowKey,
    /// The backend that owns the row.
    pub backend: FleetBackend,
    /// The operator has this row selected.
    pub selected: bool,
    /// This row is the foreground session.
    pub foreground: bool,
    /// The row is still working. That alone does not make it the target.
    pub working: bool,
}

/// The row fixed when recording starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordTarget {
    /// The row the transcript belongs to.
    pub key: RowKey,
    /// The backend that will send it, after Enter.
    pub backend: FleetBackend,
    /// The need the recording answers, when recording started from the queue.
    pub need: Option<NeedId>,
}

/// Where a transcript sits until the operator sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptPlacement {
    /// Staged in that row's reply editor.
    Draft { key: RowKey, text: String },
    /// The selection moved. The text stays on the recorded row and is not sent.
    NotSent { key: RowKey, text: String },
}

/// Bind the target when recording starts.
///
/// The selected needs-you item wins, then the selected fleet row, then the foreground
/// session. A newer queue item, a similar name, or a background working row is not
/// chosen on its own.
#[must_use]
pub fn bind_record_target(queue: &[QueueCandidate], rows: &[RowCandidate]) -> Option<RecordTarget> {
    if let Some(item) = queue.iter().find(|item| item.selected) {
        return Some(RecordTarget {
            key: item.id.key.clone(),
            backend: item.backend,
            need: Some(item.id.clone()),
        });
    }
    rows.iter()
        .find(|row| row.selected)
        .or_else(|| rows.iter().find(|row| row.foreground))
        .map(|row| RecordTarget {
            key: row.key.clone(),
            backend: row.backend,
            need: None,
        })
}

/// Keep a transcript on the row it was recorded for.
///
/// A fleet transcript waits for Enter, including when the client would otherwise submit
/// directly. A moved selection does not retarget it.
#[must_use]
pub fn place_transcript(
    recorded: &RecordTarget,
    selection_moved: bool,
    text: &str,
    mode: SubmitMode,
) -> TranscriptPlacement {
    let _ = mode;
    if selection_moved {
        TranscriptPlacement::NotSent {
            key: recorded.key.clone(),
            text: text.to_owned(),
        }
    } else {
        TranscriptPlacement::Draft {
            key: recorded.key.clone(),
            text: text.to_owned(),
        }
    }
}
