//! Pure readers over phase 3 screen text and command output.
//!
//! Each reader takes normalised lines (box characters removed and trimmed). `lines_after_last`
//! stays the one in `screen2b`.

use regex::Regex;
use serde_json::Value;

use crate::screen2a::normalised;
use crate::wording3::{
    DENY_SUFFIX, HERE_SUFFIX, LAUNCHED_IN_PROFILE, LAUNCHED_PREFIX, NEEDS_YOU_MARK,
    NEEDS_YOU_SEGMENT_SUFFIX, OPENED_HERDR_SUFFIX, OPENED_PREFIX, WORKTREE_FROM, WORKTREE_PREFIX,
};

/// One JSON launch line: the status word and the session id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchResult {
    /// `launched` or `not_started`.
    pub status: String,
    /// The session id on that line.
    pub session_id: String,
}

/// Group headers such as `NEEDS YOU (1)`, in order, as `(name, count)`.
pub fn fleet_headers(text: &str) -> Vec<(String, u32)> {
    let Ok(header) = Regex::new(r"^([A-Z][A-Z ]*[A-Z]) \((\d+)\)$") else {
        return Vec::new();
    };
    normalised(text)
        .iter()
        .filter_map(|line| {
            let found = header.captures(line)?;
            let count = found[2].parse().ok()?;
            Some((found[1].to_owned(), count))
        })
        .collect()
}

/// Whether a row line contains the foreground suffix ` (here)`.
pub fn has_here_row(visible: &str) -> bool {
    visible.lines().any(|line| line.contains(HERE_SUFFIX))
}

/// The count `n` in `⚑ n needs you` on the status line.
pub fn needs_you_segment(visible: &str) -> Option<u32> {
    visible.lines().rev().find_map(|line| {
        line.match_indices(NEEDS_YOU_MARK)
            .find_map(|(start, mark)| {
                let rest = &line[start + mark.len()..];
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                rest[digits.len()..]
                    .starts_with(NEEDS_YOU_SEGMENT_SUFFIX)
                    .then(|| digits.parse().ok())
                    .flatten()
            })
    })
}

/// The launch object when `out` is exactly one non-blank JSON line with both fields.
pub fn launch_result(out: &str) -> Option<LaunchResult> {
    let mut lines = out.lines().filter(|line| !line.trim().is_empty());
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    Some(LaunchResult {
        status: value.get("status")?.as_str()?.to_owned(),
        session_id: value.get("session_id")?.as_str()?.to_owned(),
    })
}

/// The `state` of the row whose `session_id` is `session_id`.
pub fn fleet_row_state(json: &str, session_id: &str) -> Option<String> {
    let document = fleet_document(json)?;
    document
        .get("rows")?
        .as_array()?
        .iter()
        .find(|row| row.get("session_id").and_then(Value::as_str) == Some(session_id))?
        .get("state")?
        .as_str()
        .map(str::to_owned)
}

/// The `session_id` of each item of the `needs_you` array.
pub fn needs_you_ids(json: &str) -> Vec<String> {
    let Some(document) = fleet_document(json) else {
        return Vec::new();
    };
    document
        .get("needs_you")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("session_id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The digit `n` of a line `<n>. deny`.
pub fn deny_digit(visible: &str) -> Option<char> {
    normalised(visible).iter().find_map(|line| {
        line.match_indices(DENY_SUFFIX).find_map(|(start, suffix)| {
            let after = &line[start + suffix.len()..];
            if after.chars().next().is_some_and(char::is_alphanumeric) {
                return None;
            }
            let mut before = line[..start].chars().rev();
            let digit = before.next().filter(char::is_ascii_digit)?;
            before
                .next()
                .is_none_or(|previous| !previous.is_alphanumeric())
                .then_some(digit)
        })
    })
}

/// The title in `Opened "<title>" in a new Herdr pane.` The tmux sentence is not a match.
pub fn opened_notice(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let start = line.find(OPENED_PREFIX)?;
        let rest = &line[start + OPENED_PREFIX.len()..];
        let end = rest.find(OPENED_HERDR_SUFFIX)?;
        Some(rest[..end].to_owned())
    })
}

/// The commit in `Worktree: branch talaria/<name> from origin/main at <commit>`.
pub fn worktree_line(out: &str, name: &str) -> Option<String> {
    let prefix = format!("{WORKTREE_PREFIX}{name}{WORKTREE_FROM}");
    out.lines().find_map(|line| {
        let commit = line
            .trim()
            .strip_prefix(&prefix)?
            .split_whitespace()
            .next()?;
        Some(commit.to_owned())
    })
}

/// The id in `Launched session <id> in profile <p>.`
pub fn launched_id(out: &str) -> Option<String> {
    out.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(LAUNCHED_PREFIX)?;
        let id = &rest[..rest.find(LAUNCHED_IN_PROFILE)?];
        (!id.is_empty() && !id.contains(char::is_whitespace)).then(|| id.to_owned())
    })
}

/// The fleet document: the whole text, else its last non-blank line.
fn fleet_document(json: &str) -> Option<Value> {
    serde_json::from_str(json.trim()).ok().or_else(|| {
        let last = json.lines().rev().find(|line| !line.trim().is_empty())?;
        serde_json::from_str(last.trim()).ok()
    })
}
