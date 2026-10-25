//! Directory rows: reading, merging, ordering and matching (#231).

/// A row's key: its profile and session id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RowKey {
    pub profile: String,
    pub session_id: String,
}

/// Which scope a row came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
        profile: &str,
        origin: RowOrigin,
        row: &serde_json::Value,
    ) -> Option<DirectoryRow> {
        let session_id = row.get("session_id")?.as_str()?.to_owned();
        let title = row.get("title")?.as_str()?.to_owned();
        let source = match row.get("source") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str()?.to_owned()),
        };
        let started_at = match row.get("started_at") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_f64()?),
        };
        let message_count = match row.get("message_count") {
            Some(value) => value.as_u64()?,
            None => 0,
        };
        let running = match row.get("running") {
            None => false,
            Some(value) => value.as_bool()?,
        };
        let model = match row.get("model") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(value.as_str()?.to_owned()),
        };
        Some(DirectoryRow {
            key: RowKey {
                profile: profile.to_owned(),
                session_id,
            },
            title,
            source,
            started_at,
            message_count,
            running,
            model,
            origin,
        })
    }
}

/// Merge live over stored by key; live wins.
pub fn merge(live: Vec<DirectoryRow>, stored: Vec<DirectoryRow>) -> Vec<DirectoryRow> {
    let mut merged = live;
    for row in stored {
        if !merged.iter().any(|kept| kept.key == row.key) {
            merged.push(row);
        }
    }
    merged
}

/// Order in place: live group then stored; newest first; missing last; then key.
// The unit document pins the parameter as a `Vec`; a slice would read the
// same, so the lint is silenced rather than the shape changed.
#[allow(clippy::ptr_arg)]
pub fn order(rows: &mut Vec<DirectoryRow>) {
    rows.sort_by(|left, right| {
        let group = |row: &DirectoryRow| match row.origin {
            RowOrigin::Live => 0,
            RowOrigin::Stored => 1,
        };
        group(left)
            .cmp(&group(right))
            .then_with(|| {
                // A row without a start time sorts last inside its group.
                match (left.started_at, right.started_at) {
                    (Some(one), Some(other)) => {
                        other.partial_cmp(&one).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            })
            .then_with(|| left.key.cmp(&right.key))
    });
}

/// The age text for a start time: now, minutes, hours, days, weeks, else a date.
pub fn format_age(started_at: Option<f64>, now: f64) -> String {
    let Some(started_at) = started_at else {
        return "—".to_owned();
    };
    let ago = now - started_at;
    if ago < 60.0 {
        return "now".to_owned();
    }
    if ago < 3_600.0 {
        return format!("{}m", (ago / 60.0).floor() as i64);
    }
    if ago < 86_400.0 {
        return format!("{}h", (ago / 3_600.0).floor() as i64);
    }
    if ago < 604_800.0 {
        return format!("{}d", (ago / 86_400.0).floor() as i64);
    }
    if ago < 4_838_400.0 {
        return format!("{}w", (ago / 604_800.0).floor() as i64);
    }
    utc_date(started_at)
}

/// The UTC date of a timestamp, as `YYYY-MM-DD`.
fn utc_date(seconds: f64) -> String {
    let days = (seconds / 86_400.0).floor() as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The civil date of a day count since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The text after the last `/`; `—` when missing or empty.
pub fn short_model(model: Option<&str>) -> String {
    short_model_text(model).to_owned()
}

/// The text after the last `/`; `—` when missing or empty. The text is
/// borrowed from the model, so no copy is made.
pub fn short_model_text(model: Option<&str>) -> &str {
    let Some(model) = model else {
        return "—";
    };
    let short = model.rsplit('/').next().unwrap_or(model);
    if short.is_empty() { "—" } else { short }
}

/// Whether the row matches a search query, case-insensitively.
pub fn matches(row: &DirectoryRow, query: &str) -> bool {
    let query = query.to_lowercase();
    if query.is_empty() {
        return true;
    }
    let mut buffer = String::new();
    matches_folded(row, &query, &mut buffer)
}

/// Whether the row matches a query that is already lower-cased. The buffer
/// holds the folded field and is written over for each field.
pub(crate) fn matches_folded(row: &DirectoryRow, query: &str, buffer: &mut String) -> bool {
    for field in row_fields(row) {
        buffer.clear();
        push_folded(buffer, field);
        if buffer.contains(query) {
            return true;
        }
    }
    false
}

/// The fields a query is matched against.
fn row_fields(row: &DirectoryRow) -> [&str; 5] {
    [
        row.title.as_str(),
        short_model_text(row.model.as_deref()),
        row.key.profile.as_str(),
        row.key.session_id.as_str(),
        row.source.as_deref().unwrap_or_default(),
    ]
}

/// Appends the lower-cased form of `text` to `out`, the way
/// `str::to_lowercase` writes it.
fn push_folded(out: &mut String, text: &str) {
    out.reserve(text.len());
    for character in text.chars() {
        if character.is_ascii() {
            out.push(char::from((character as u8).to_ascii_lowercase()));
            continue;
        }
        if character == 'Σ' {
            // `str::to_lowercase` writes ς where a sigma ends a word, and
            // that rule is not worth reproducing here. The field is rare
            // enough to hand over to it whole.
            out.clear();
            out.push_str(&text.to_lowercase());
            return;
        }
        for lower in character.to_lowercase() {
            out.push(lower);
        }
    }
}
