//! Turns Hermes writes into the conversation for the agent.
//!
//! Hermes starts some turns itself: subagent results coming back, a background process that
//! finished, a goal continuation, a loop wakeup, the tool-call limit nudge. Their text is written
//! for the model, so the transcript shows one notice with a short label and keeps the text as the
//! notice's body for the transcript view. A live start and a resumed row go through this one rule,
//! so both look the same.

use super::{NoticeKind, SessionState};
use crate::automation::{FoldKind, fold_user_text};

/// What one user row draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UserRow {
    /// A prompt the person typed, as the transcript shows it.
    Typed(String),
    /// A turn Hermes wrote: the notice label and, when the row has any, the full text.
    Injected {
        /// The one line the main screen shows.
        label: String,
        /// What Hermes handed the agent. It is kept even when it repeats the label, because it
        /// marks the notice as one a resumed snapshot draws again.
        body: Option<String>,
    },
    /// A row no surface draws.
    Skip,
}

/// Hermes appends this user row, with no display kind, when the agent runs out of tool-call
/// iterations. Its exact text is the only marker; Hermes recognises it the same way. The text is
/// `MAX_ITERATIONS_SUMMARY_REQUEST` in Hermes's `agent/context_compressor.py` at the pinned
/// revision.
const TOOL_LIMIT_NUDGE: &str = "You've reached the maximum number of tool-calling iterations allowed. Please provide a final response summarizing what you've found and accomplished so far, without calling any more tools.";

const TOOL_LIMIT_LABEL: &str =
    "Tool-call limit reached: Hermes asked the agent for a final summary";

/// Display kinds of rows the person wrote. A steer is drawn by its own rule before this one.
const TYPED_KINDS: [&str; 2] = ["skill_invocation", "content_refused"];

/// The longest first line an unknown kind's label quotes, in characters.
const QUOTE_LIMIT: usize = 80;

/// Decide how one user row draws, from Hermes's display kind and display text and the row text.
///
/// A row with no kind is typed, unless its text is a scaffold only Hermes writes: a goal
/// continuation, a loop wakeup or the tool-call limit nudge. A `/plan` scaffold stays typed under
/// any kind, because the person typed `/plan`. Every other kind, including one this version does
/// not know, is a notice.
pub(crate) fn classify_user_row(
    kind: Option<&str>,
    display_text: Option<&str>,
    text: &str,
) -> UserRow {
    let kind = kind.map(str::trim).filter(|kind| !kind.is_empty());
    if kind == Some("hidden") {
        return UserRow::Skip;
    }
    // A prompt Hivecode routed shows what the person typed, never the skill or its command.
    if let Some(typed) = crate::hivecode::typed_text(text) {
        return UserRow::Typed(typed.to_owned());
    }
    if kind == Some("skill_invocation")
        && let Some(request) = crate::hivecode::replayed_request(text)
    {
        return UserRow::Typed(request.to_owned());
    }
    let folded = fold_user_text(text);
    if let Some(plan) = folded
        .as_ref()
        .filter(|folded| folded.kind == FoldKind::Plan)
    {
        return UserRow::Typed(plan.display.clone());
    }
    let display_text = display_text
        .map(str::trim)
        .filter(|display| !display.is_empty());
    let typed = kind.is_none_or(|kind| TYPED_KINDS.contains(&kind));
    let tool_limit = text.trim() == TOOL_LIMIT_NUDGE;
    if typed && display_text.is_none() && folded.is_none() && !tool_limit {
        return UserRow::Typed(text.to_owned());
    }
    let label = if let Some(display) = display_text {
        display.to_owned()
    } else if let Some(folded) = folded {
        folded.display
    } else if tool_limit {
        TOOL_LIMIT_LABEL.to_owned()
    } else {
        kind_label(kind.unwrap_or_default(), text)
    };
    let body = (!text.trim().is_empty()).then(|| text.to_owned());
    UserRow::Injected { label, body }
}

/// Draw a user row Hermes did not mark as this client's own: a prompt cell, one notice, or
/// nothing. `category` is Hermes's `notification_category`, which picks the notice kind.
pub(crate) fn draw_user_row(
    state: &mut SessionState,
    kind: Option<&str>,
    display_text: Option<&str>,
    category: Option<&str>,
    text: &str,
) {
    match classify_user_row(kind, display_text, text) {
        UserRow::Typed(text) => state.user(text),
        UserRow::Injected { label, body } => {
            state.notice_with_body(notice_kind(category), label, body);
        }
        UserRow::Skip => {}
    }
}

/// The notice kind of a turn Hermes wrote: a diagnostic when Hermes says so, otherwise an
/// automation notice. `message.start` carries the category itself; a stored row carries it in its
/// display metadata.
pub(crate) fn notice_kind(category: Option<&str>) -> NoticeKind {
    if category == Some("diagnostic") {
        NoticeKind::Diagnostic
    } else {
        NoticeKind::Automation
    }
}

/// The label of a kind whose row has no display text. Hermes's own terminal uses the same words.
fn kind_label(kind: &str, text: &str) -> String {
    match kind {
        "async_delegation_complete" => "Subagent results returned to the agent".to_owned(),
        "process_complete" => "Background process finished".to_owned(),
        "internal_notification" => "Hermes note to the agent".to_owned(),
        "model_switch" => "Model changed".to_owned(),
        "auto_continue" => "Resumed interrupted turn".to_owned(),
        "personality_switch" => "Personality changed".to_owned(),
        _ => format!("Hermes message to the agent: {}", first_line(text)),
    }
}

/// The first non-blank line of `text`, cut to [`QUOTE_LIMIT`] characters.
fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if line.chars().count() > QUOTE_LIMIT {
        let cut: String = line.chars().take(QUOTE_LIMIT).collect();
        format!("{cut}\u{2026}")
    } else {
        line.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{UserRow, classify_user_row};

    #[test]
    fn display_text_alone_labels_a_start_without_a_body() {
        assert_eq!(
            classify_user_row(None, Some(" Ran the check "), ""),
            UserRow::Injected {
                label: "Ran the check".to_owned(),
                body: None,
            }
        );
    }

    #[test]
    fn blank_kind_and_text_is_typed() {
        assert_eq!(
            classify_user_row(Some(" "), Some(" "), "hi"),
            UserRow::Typed("hi".to_owned())
        );
    }
}
