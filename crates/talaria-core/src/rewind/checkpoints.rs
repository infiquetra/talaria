//! Checkpoint lists and the prompt-to-checkpoint match. Issue 233.

use super::turns::RewindTurn;

/// One checkpoint row, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// 1-based index in list order. 1 is the newest.
    pub index: usize,
    /// Full hash.
    pub hash: String,
    /// First 8 characters, or the whole hash when it is shorter.
    pub short: String,
    /// Checkpoint message.
    pub message: String,
    /// Timestamp text as the gateway sent it.
    pub raw_time: String,
    /// Unix seconds, when `raw_time` parses.
    pub time: Option<i64>,
}

/// The answer of a checkpoint list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointList {
    /// Checkpoints are off.
    Off,
    /// Checkpoints are on and none exist.
    Empty,
    /// Rows, newest first.
    Rows(Vec<Checkpoint>),
    /// The list call failed.
    Unavailable(String),
}

impl CheckpointList {
    /// Build a list from the enabled flag and `(hash, timestamp, message)` rows.
    #[must_use]
    pub fn from_result(enabled: bool, rows: Vec<(String, String, String)>) -> Self {
        if !enabled {
            return Self::Off;
        }
        if rows.is_empty() {
            return Self::Empty;
        }
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(position, (hash, raw_time, message))| Checkpoint {
                index: position + 1,
                short: hash.chars().take(8).collect(),
                time: parse_checkpoint_time(&raw_time),
                hash,
                message,
                raw_time,
            })
            .collect();
        Self::Rows(rows)
    }
}

/// Unix seconds for a git ISO-8601 author time, or `None`.
#[must_use]
pub fn parse_checkpoint_time(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    let year = digits(bytes, 0, 4)?;
    let month = digits(bytes, 5, 2)?;
    let day = digits(bytes, 8, 2)?;
    let hour = digits(bytes, 11, 2)?;
    let minute = digits(bytes, 14, 2)?;
    let second = digits(bytes, 17, 2)?;
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let offset = match &bytes[19..] {
        b"Z" => 0,
        [sign @ (b'+' | b'-'), rest @ ..] if rest.len() == 5 && rest[2] == b':' => {
            let hours = digits(rest, 0, 2)?;
            let minutes = digits(rest, 3, 2)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = hours * 3600 + minutes * 60;
            if *sign == b'-' { -magnitude } else { magnitude }
        }
        _ => return None,
    };
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some(local - offset)
}

/// The value of `len` ASCII digits at `start`.
fn digits(bytes: &[u8], start: usize, len: usize) -> Option<i64> {
    let slice = bytes.get(start..start + len)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(
        slice
            .iter()
            .fold(0, |value, digit| value * 10 + i64::from(*digit - b'0')),
    )
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `HH:MM` from a time that parses, otherwise empty.
#[must_use]
pub fn clock_text(s: &str) -> String {
    if parse_checkpoint_time(s).is_none() {
        return String::new();
    }
    s.chars().skip(11).take(5).collect()
}

/// The checkpoint a confirm screen would restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointMatch {
    /// Index in the list.
    pub index: usize,
    /// No kept checkpoint is older than the prompt.
    pub oldest_kept: bool,
}

/// Why a prompt has no checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoMatch {
    /// Checkpoints are off.
    Off,
    /// The list could not be read.
    Unknown,
    /// The prompt has no time.
    NoTime,
    /// Nothing at or after the prompt.
    NoChanges,
    /// The turn edited files and no checkpoint falls inside its window.
    NoCheckpointForEdits,
}

/// The oldest checkpoint at or after the prompt's window start.
pub fn checkpoint_for_turn(
    turn: &RewindTurn,
    list: &CheckpointList,
) -> Result<CheckpointMatch, NoMatch> {
    let rows = match list {
        CheckpointList::Off => return Err(NoMatch::Off),
        CheckpointList::Unavailable(_) => return Err(NoMatch::Unknown),
        CheckpointList::Empty => return Err(NoMatch::NoChanges),
        CheckpointList::Rows(rows) => rows,
    };
    let (Some(_), Some(start)) = (turn.timestamp, turn.window_start) else {
        return Err(NoMatch::NoTime);
    };
    // Smallest time wins; a tie goes to the larger index, the older row.
    let matched = rows
        .iter()
        .filter_map(|row| {
            row.time
                .filter(|time| *time >= start)
                .map(|time| (time, row))
        })
        .min_by_key(|(time, row)| (*time, std::cmp::Reverse(row.index)))
        .ok_or(NoMatch::NoChanges)?;
    let (time, row) = matched;
    if turn.edits_files && turn.window_end.is_some_and(|end| time >= end) {
        return Err(NoMatch::NoCheckpointForEdits);
    }
    let oldest_kept = !rows
        .iter()
        .any(|other| other.time.is_some_and(|other_time| other_time < start));
    Ok(CheckpointMatch {
        index: row.index,
        oldest_kept,
    })
}
