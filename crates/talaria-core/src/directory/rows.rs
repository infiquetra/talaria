//! Directory rows: reading, merging, ordering and matching (#231).

/// A row's key: its profile and session id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowKey {
    pub profile: String,
    pub session_id: String,
}

/// Which scope a row came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowOrigin {
    Live,
    Stored,
}

/// One session row of the directory.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectoryRow {
    pub key: RowKey,
    /// As Hermes sent it; `""` means untitled.
    pub title: String,
    pub source: Option<String>,
    /// Seconds since 1970.
    pub started_at: Option<f64>,
    pub message_count: u64,
    pub running: bool,
    pub model: Option<String>,
    pub origin: RowOrigin,
}

impl DirectoryRow {
    /// Read one list row. None when a required field is missing or wrong-typed.
    pub fn from_list_row(
        _profile: &str,
        _origin: RowOrigin,
        _row: &serde_json::Value,
    ) -> Option<DirectoryRow> {
        unimplemented!("#231")
    }
}

/// Merge live over stored by key; live wins.
pub fn merge(_live: Vec<DirectoryRow>, _stored: Vec<DirectoryRow>) -> Vec<DirectoryRow> {
    unimplemented!("#231")
}

/// Order in place: live group then stored; newest first; missing last; then key.
pub fn order(_rows: &mut Vec<DirectoryRow>) {
    unimplemented!("#231")
}

/// The age text for a start time: now, minutes, hours, days, weeks, else a date.
pub fn format_age(_started_at: Option<f64>, _now: f64) -> String {
    unimplemented!("#231")
}

/// The text after the last `/`; `—` when missing or empty.
pub fn short_model(_model: Option<&str>) -> String {
    unimplemented!("#231")
}

/// Whether the row matches a search query, case-insensitively.
pub fn matches(_row: &DirectoryRow, _query: &str) -> bool {
    unimplemented!("#231")
}
