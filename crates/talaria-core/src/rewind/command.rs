//! Typed `/rollback`, `/undo`, and `/diff`. Issue 233. Checkpoint rewind and diff.

/// One parsed `/rollback` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackCommand {
    /// List checkpoints.
    List,
    /// Diff one target.
    Diff(String),
    /// Restore a target, optionally one file.
    Restore {
        /// Number or hash, as typed.
        target: String,
        /// Relative path, inner spaces kept.
        file_path: Option<String>,
        /// True only when a flag is present and there is no file.
        all: bool,
    },
    /// The target is not a checkpoint number or hash.
    BadTarget(String),
    /// The argument does not match the grammar.
    Usage,
}

/// Parse the argument of `/rollback`.
#[must_use]
pub fn parse_rollback(arg: &str) -> RollbackCommand {
    let arg = arg.trim();
    let (first, rest) = split_word(arg);
    if first.is_empty() {
        return RollbackCommand::List;
    }
    if first.eq_ignore_ascii_case("list") || first.eq_ignore_ascii_case("ls") {
        return if rest.is_empty() {
            RollbackCommand::List
        } else {
            RollbackCommand::Usage
        };
    }
    if first == "diff" {
        let (word, extra) = split_word(rest);
        if word.is_empty() || !extra.is_empty() {
            return RollbackCommand::Usage;
        }
        return if is_checkpoint_target(word) {
            RollbackCommand::Diff(word.to_string())
        } else {
            RollbackCommand::BadTarget(word.to_string())
        };
    }
    if !is_checkpoint_target(first) {
        return RollbackCommand::BadTarget(first.to_string());
    }
    let mut flagged = false;
    let mut rest = rest;
    while let Some(after) = strip_leading_flag(rest) {
        flagged = true;
        rest = after;
    }
    while let Some(before) = strip_trailing_flag(rest) {
        flagged = true;
        rest = before;
    }
    let file_path = rest.trim();
    let file_path = (!file_path.is_empty()).then(|| file_path.to_string());
    RollbackCommand::Restore {
        target: first.to_string(),
        all: flagged && file_path.is_none(),
        file_path,
    }
}

/// The first whitespace-delimited word and the text after it, trimmed at the start.
fn split_word(text: &str) -> (&str, &str) {
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], text[end..].trim_start())
}

const FLAGS: [&str; 2] = ["--all", "--force"];

/// `text` without a flag word at its start, or `None` when it does not start with one.
fn strip_leading_flag(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let (word, rest) = split_word(text);
    FLAGS.contains(&word).then_some(rest)
}

/// `text` without a flag word at its end, or `None` when it does not end with one.
fn strip_trailing_flag(text: &str) -> Option<&str> {
    let text = text.trim_end();
    let start = text.rfind(char::is_whitespace).map_or(0, |at| {
        at + text[at..].chars().next().map_or(1, char::len_utf8)
    });
    FLAGS.contains(&&text[start..]).then(|| &text[..start])
}

/// A row number of 1 to 6 digits, or 4 to 64 hex characters that do not start with `-`.
#[must_use]
pub fn is_checkpoint_target(s: &str) -> bool {
    let all_digits = !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit());
    if all_digits {
        // Hermes reads an all-digit target as a row number, never as a hash prefix.
        return s.len() <= 6;
    }
    (4..=64).contains(&s.len()) && s.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Parse `/undo`'s count. `Err` is the usage text.
#[allow(clippy::result_unit_err)]
pub fn parse_undo(arg: &str) -> Result<usize, ()> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Ok(1);
    }
    if !arg.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(());
    }
    match arg.parse::<usize>() {
        Ok(count) if (1..=999).contains(&count) => Ok(count),
        _ => Err(()),
    }
}

/// One parsed `/diff` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffCommand {
    /// The session diff. `stat_only` omits the patch body.
    Session { stat_only: bool },
    /// Git's view, which this command does not run.
    GitView,
}

/// Parse the argument of `/diff`.
#[must_use]
pub fn parse_diff(arg: &str) -> DiffCommand {
    let words: Vec<&str> = arg.split_whitespace().collect();
    match words.as_slice() {
        [] | ["session"] => DiffCommand::Session { stat_only: false },
        ["--stat"] | ["stat"] | ["session", "--stat"] | ["--stat", "session"] => {
            DiffCommand::Session { stat_only: true }
        }
        _ => DiffCommand::GitView,
    }
}
