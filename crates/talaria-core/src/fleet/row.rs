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
        let session_id = row.get("session_id")?.as_str()?.to_string();
        Some(RosterRow {
            session_id,
            title: text_field(row, "title")?,
            source: optional_text(row, "source")?,
            started_at: optional_number(row, "started_at")?,
            message_count: count_field(row, "message_count")?,
            running: match row.get("running") {
                None | Some(Value::Null) => false,
                Some(value) => value.as_bool()?,
            },
            model: optional_text(row, "model")?,
        })
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
        match status {
            "idle" => LiveStatus::Idle,
            "starting" => LiveStatus::Starting,
            "working" | "streaming" | "resuming" => LiveStatus::Working,
            "waiting" => LiveStatus::Waiting,
            other => LiveStatus::Other(other.to_string()),
        }
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
        let session_id = match item.get("id") {
            Some(id) => id.as_str()?,
            None => item.get("session_key")?.as_str()?,
        };
        Some(LiveItem {
            session_id: session_id.to_string(),
            status: LiveStatus::parse(item.get("status")?.as_str()?),
            last_active: optional_number(item, "last_active")?,
            message_count: count_field(item, "message_count")?,
            model: optional_text(item, "model")?,
            title: text_field(item, "title")?,
        })
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
        match status {
            Some("complete") => Outcome::Completed,
            Some("interrupted") => Outcome::Interrupted,
            Some("error") => Outcome::Failed,
            _ => Outcome::Unknown,
        }
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
    /// Message count from the latest roster, or from the liveness item of an unlisted row.
    /// A liveness reply records its count for the missed-turn rule without showing it here,
    /// so a count rise that turns out to be no finished turn changes nothing visible.
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
    /// The backend that owns this row.
    pub backend: super::FleetBackend,
    /// The display name of an external agent.
    pub agent: Option<String>,
    /// Context occupancy reported by an external agent.
    pub occupancy: Option<(u64, u64)>,
    /// External-agent cost with its original currency.
    pub cost: Option<(f64, String)>,
    /// Display location of a terminal agent.
    pub terminal_location: Option<String>,
}

/// A string field. Missing or null is empty; another type is malformed.
fn text_field(value: &Value, name: &str) -> Option<String> {
    match value.get(name) {
        None | Some(Value::Null) => Some(String::new()),
        Some(field) => field.as_str().map(str::to_string),
    }
}

/// An optional string field. Missing, null and empty are all absent.
fn optional_text(value: &Value, name: &str) -> Option<Option<String>> {
    match value.get(name) {
        None | Some(Value::Null) => Some(None),
        Some(field) => {
            let text = field.as_str()?;
            Some((!text.is_empty()).then(|| text.to_string()))
        }
    }
}

/// An optional number field.
fn optional_number(value: &Value, name: &str) -> Option<Option<f64>> {
    match value.get(name) {
        None | Some(Value::Null) => Some(None),
        Some(field) => field.as_f64().map(Some),
    }
}

/// A count. Missing or null is zero; a negative or fractional number is malformed.
fn count_field(value: &Value, name: &str) -> Option<u64> {
    match value.get(name) {
        None | Some(Value::Null) => Some(0),
        Some(field) => field.as_u64(),
    }
}

impl FleetRow {
    pub fn backend(&self) -> super::FleetBackend {
        self.backend
    }
    pub fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }
    pub fn occupancy(&self) -> Option<(u64, u64)> {
        self.occupancy
    }
    pub fn cost(&self) -> Option<(f64, &str)> {
        self.cost
            .as_ref()
            .map(|(amount, currency)| (*amount, currency.as_str()))
    }
}

impl FleetRow {
    pub fn terminal_location(&self) -> Option<&str> {
        self.terminal_location.as_deref()
    }
}
