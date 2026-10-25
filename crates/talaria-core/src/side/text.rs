//! Cleaning untrusted text and wording shell and side outcomes.

use crate::side::{ShellOutcome, SideOutcome};

/// The hint for a `!` with nothing after it.
pub const SHELL_EMPTY_HINT: &str = "Type a command after !, for example !git status.";
/// Shown instead of sending a `!` or side question while no session is open. The draft stays.
pub const NO_SESSION_NOTICE: &str =
    "No session is open yet, so nothing was sent. Try again in a moment.";
/// The usage line for a side question with no question.
pub const SIDE_USAGE: &str = "Usage: /btw <question>. Asks about this conversation without adding to it; the agent does not see the question or the answer.";
/// Shown when the reply to a side question was lost; the question is kept for a later answer.
pub const SIDE_UNKNOWN_NOTICE: &str = "The connection dropped before Hermes confirmed the side question. If it was accepted, the answer will still appear here.";
/// The question shown when an answer arrives without one.
pub const QUESTION_NOT_GIVEN: &str = "(question not given)";

/// Refusal reasons are cut to this many code points before they are shown.
const REASON_CHARS: usize = 64;

/// The warning for a side question still unanswered when its wait runs out.
pub fn side_timeout_notice(question: &str) -> String {
    format!(
        "No answer yet to the side question \"{question}\" after 5 minutes. Hermes may have stopped it (for example during a restart). A late answer will still appear."
    )
}

/// Strip escape sequences, resolve carriage returns, drop trailing blanks.
///
/// CSI runs to its final byte and OSC to BEL or `ESC \`; either one left open is removed to the
/// end of its line. Any other `ESC` goes with the character after it, unless that is a line
/// break, which stays so lines never merge. Then `\r\n` is a break, each line keeps the text after
/// its last lone `\r`, and trailing lines holding only whitespace are dropped. Single-byte C1
/// controls stay for the terminal crate's sanitiser.
pub fn clean_output(text: &str) -> String {
    let stripped = strip_escapes(text);
    let unified = stripped.replace("\r\n", "\n");
    let mut lines: Vec<&str> = unified
        .split('\n')
        .map(|line| line.rsplit('\r').next().unwrap_or(line))
        .collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if next == '\n' {
                        break;
                    }
                    chars.next();
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(&next) = chars.peek() {
                    if next == '\n' {
                        break;
                    }
                    chars.next();
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some('\n') | None => {}
            Some(_) => {
                chars.next();
            }
        }
    }
    out
}

fn cut_reason(reason: &str) -> String {
    reason.chars().take(REASON_CHARS).collect()
}

/// The refusal line for a shell reason, cut to 64 code points.
pub fn shell_refusal_text(reason: &str, detail: Option<&str>) -> String {
    match reason {
        "dangerous_command" => "Hermes refused to run this: it looks dangerous, and ! runs only safe commands. Ask the agent instead, so the command goes through approval.".to_string(),
        "unsupported_terminal_backend" => format!(
            "! commands run on the gateway host, but this session's terminal backend is {}, so the command was not run. Ask the agent to run it instead.",
            detail.map_or_else(|| "not local".to_string(), cut_reason)
        ),
        "command_timeout" => "The command ran for more than 30 seconds, so Hermes stopped waiting for it. Run long commands in your own terminal.".to_string(),
        "cwd_unavailable" => "This session's working folder no longer exists, so the command was not run.".to_string(),
        "command_failed" => "Hermes could not start a shell for this command.".to_string(),
        "permission_denied" => "Hermes runs ! commands only for local terminal sessions it lets this window control. This session is not one, so the command was not run.".to_string(),
        "not_found" => "The gateway no longer knows this session, so the command was not run.".to_string(),
        "runtime_draining" => "The gateway is restarting, so the command was not run. Try again in a moment.".to_string(),
        "invalid_params" => "Hermes rejected the command as malformed.".to_string(),
        "profile_mismatch" => "This session belongs to a different profile than this window, so the command was not run.".to_string(),
        "unknown_method" => "This Hermes gateway does not offer ! commands (shell.exec). Update Hermes to the release Talaria is pinned to.".to_string(),
        other => format!("Hermes refused the command ({}).", cut_reason(other)),
    }
}

/// The refusal line for a side reason, cut to 64 code points.
pub fn side_refusal_text(reason: &str, detail: Option<&str>) -> String {
    // No side reason names a backend, so the detail is not part of any line.
    let _ = detail;
    match reason {
        "permission_denied" => "Hermes did not accept the side question: this window is not subscribed to the session.".to_string(),
        "not_found" => "The gateway no longer knows this session, so the side question was not asked.".to_string(),
        "runtime_draining" => "The gateway is restarting, so the side question was not asked. Try again in a moment.".to_string(),
        "profile_mismatch" => "This session belongs to a different profile than this window, so the side question was not asked.".to_string(),
        "invalid_params" => "Hermes rejected the side question as malformed.".to_string(),
        "unknown_method" => "This Hermes gateway does not offer /btw (prompt.btw). Update Hermes to the release Talaria is pinned to.".to_string(),
        other => format!("Hermes refused the side question ({}).", cut_reason(other)),
    }
}

/// The cell text for a shell outcome. `None` means the streams render.
pub fn shell_outcome_text(outcome: &ShellOutcome) -> Option<String> {
    Some(match outcome {
        ShellOutcome::Ok { .. } => return None,
        ShellOutcome::Refused { reason, detail, .. } => {
            shell_refusal_text(reason, detail.as_deref())
        }
        ShellOutcome::Unknown => "The connection dropped before Hermes answered, so the command may or may not have run. Talaria does not retry it.".to_string(),
        ShellOutcome::Unreadable => {
            "Hermes ran the command but Talaria could not read its reply.".to_string()
        }
        ShellOutcome::NotSent {
            not_connected: true,
        } => "Not connected to Hermes, so the command was not sent.".to_string(),
        ShellOutcome::NotSent {
            not_connected: false,
        } => "Talaria could not send the command (an internal error), so it was not run."
            .to_string(),
    })
}

/// The cell text for a side outcome. `None` means the answer renders.
pub fn side_outcome_text(outcome: &SideOutcome) -> Option<String> {
    Some(match outcome {
        SideOutcome::Answered { .. } => return None,
        SideOutcome::Failed { text } => {
            format!("The side question failed: {}", clean_output(text))
        }
        SideOutcome::Refused { reason, detail } => side_refusal_text(reason, detail.as_deref()),
        SideOutcome::NotSent {
            not_connected: true,
        } => "Not connected to Hermes, so the side question was not asked.".to_string(),
        SideOutcome::NotSent {
            not_connected: false,
        } => "Talaria could not send the side question (an internal error), so it was not asked."
            .to_string(),
    })
}

/// A duration for a cell header: one decimal below ten seconds, whole above.
///
/// Both round half up; 9,950 ms and later are whole seconds so `9.95` never shows as `10.0s`.
pub fn format_duration(ms: u64) -> String {
    if ms < 9_950 {
        let tenths = (ms + 50) / 100;
        format!("{}.{}s", tenths / 10, tenths % 10)
    } else {
        format!("{}s", ms.saturating_add(500) / 1_000)
    }
}
