//! Roster and liveness rows, decoded from raw JSON.

use serde_json::Value;

use super::RowKey;

/// One `session.list` row. Malformed rows are `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct RosterRow {
    /// Session id.
    pub session_id: String,
    /// Title as sent. An empty string stays empty.
    pub title: String,
    /// Source, when the row carried one.
    pub source: Option<String>,
    /// Seconds since 1970.
    pub started_at: Option<f64>,
    /// Message count.
    pub message_count: u64,
    /// Whether the row said it was running.
    pub running: bool,
    /// Model. An empty string and JSON null are both absent.
    pub model: Option<String>,
}

impl RosterRow {
    /// Read one list row. `None` when a required field is missing or the wrong type.
    pub fn from_value(row: &Value) -> Option<RosterRow> {
        let _ = row;
        todo!()
    }
}

/// Liveness status. `streaming` and `resuming` are working.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveStatus {
    /// Idle.
    Idle,
    /// Starting.
    Starting,
    /// Working. `streaming` and `resuming` parse as this.
    Working,
    /// Waiting on a person.
    Waiting,
    /// A word that is not one of the four, and is not `streaming` or `resuming`.
    /// The row still shows as working; the word is kept so the caller can count it.
    Other(String),
}

impl LiveStatus {
    /// Map a status word. `streaming` and `resuming` are working. Any other unknown word is
    /// [`LiveStatus::Other`].
    pub fn parse(status: &str) -> LiveStatus {
        let _ = status;
        todo!()
    }
}

/// One `session.active_list` item. Malformed items are `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveItem {
    /// Session id.
    pub session_id: String,
    /// Status word after [`LiveStatus::parse`].
    pub status: LiveStatus,
    /// Seconds since 1970.
    pub last_active: Option<f64>,
    /// Message count.
    pub message_count: u64,
    /// Model. An empty string and JSON null are both absent.
    pub model: Option<String>,
    /// Title as sent. An empty string stays empty.
    pub title: String,
}

impl LiveItem {
    /// Read one liveness item. `None` when a required field is missing or the wrong type.
    pub fn from_value(item: &Value) -> Option<LiveItem> {
        let _ = item;
        todo!()
    }
}

/// Display group, in the order a listing prints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// A queue item is open.
    NeedsYou,
    /// A turn is running, or a waiting row has not read its first item yet.
    Working,
    /// A finished turn failed.
    Failed,
    /// A finished turn is done.
    Done,
    /// Nothing is running and nothing is held.
    Idle,
}

/// How a probed turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Status `complete`.
    Completed,
    /// Status `interrupted`.
    Interrupted,
    /// Status `error`.
    Failed,
    /// Any other status, or no completion.
    Unknown,
}

impl Outcome {
    /// Map `message.complete` `status`. `None` and any other word are unknown.
    pub fn from_turn_status(status: Option<&str>) -> Outcome {
        let _ = status;
        todo!()
    }
}

/// One fleet row.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetRow {
    /// Profile and session id.
    pub key: RowKey,
    /// Title as sent.
    pub title: String,
    /// Model, when known.
    pub model: Option<String>,
    /// Source, when the roster carried one.
    pub source: Option<String>,
    /// Seconds since 1970.
    pub started_at: Option<f64>,
    /// Message count from the latest applied liveness, else the roster.
    pub message_count: u64,
    /// Display group.
    pub state: RowState,
    /// Seconds since 1970 when `state` last changed.
    pub state_since: f64,
    /// Held outcome, when the row is done or failed.
    pub outcome: Option<Outcome>,
    /// The foreground session.
    pub foreground: bool,
    /// An owner is reporting this session.
    pub subscribed: bool,
    /// The last roster or liveness call for this profile failed, or the link dropped.
    pub stale: bool,
    /// Missing from the roster while an unacknowledged done or failed outcome is held.
    pub gone: bool,
    /// Created from a waiting liveness item the roster did not list.
    pub unlisted: bool,
    /// Activity text from an owner.
    pub activity: Option<String>,
    /// Usage cost from an owner. List calls do not carry a cost.
    pub cost_usd: Option<f64>,
}
