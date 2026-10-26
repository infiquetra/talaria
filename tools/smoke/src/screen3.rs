//! Pure readers over phase 3 screen text and command output.
//!
//! Each reader takes normalised lines (box characters removed and trimmed). The bodies
//! panic until the readers are written. `lines_after_last` stays the one in `screen2b`.

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
    let _ = text;
    panic!("not implemented");
}

/// Whether a row line contains the foreground suffix ` (here)`.
pub fn has_here_row(visible: &str) -> bool {
    let _ = visible;
    panic!("not implemented");
}

/// The count `n` in `⚑ n needs you` on the status line.
pub fn needs_you_segment(visible: &str) -> Option<u32> {
    let _ = visible;
    panic!("not implemented");
}

/// The launch object when `out` is exactly one non-blank JSON line with both fields.
pub fn launch_result(out: &str) -> Option<LaunchResult> {
    let _ = out;
    panic!("not implemented");
}

/// The `state` of the row whose `session_id` is `session_id`.
pub fn fleet_row_state(json: &str, session_id: &str) -> Option<String> {
    let _ = (json, session_id);
    panic!("not implemented");
}

/// The `session_id` of each item of the `needs_you` array.
pub fn needs_you_ids(json: &str) -> Vec<String> {
    let _ = json;
    panic!("not implemented");
}

/// The digit `n` of a line `<n>. deny`.
pub fn deny_digit(visible: &str) -> Option<char> {
    let _ = visible;
    panic!("not implemented");
}

/// The title in `Opened "<title>" in a new Herdr pane.` The tmux sentence is not a match.
pub fn opened_notice(text: &str) -> Option<String> {
    let _ = text;
    panic!("not implemented");
}

/// The commit in `Worktree: branch talaria/<name> from origin/main at <commit>`.
pub fn worktree_line(out: &str, name: &str) -> Option<String> {
    let _ = (out, name);
    panic!("not implemented");
}

/// The id in `Launched session <id> in profile <p>.`
pub fn launched_id(out: &str) -> Option<String> {
    let _ = out;
    panic!("not implemented");
}
