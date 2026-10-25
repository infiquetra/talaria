//! Notice text for a refused queue call, and the short preview of a prompt.
//!
//! Issue 232. Queue and steer.

use super::QueueAction;

pub(crate) const BUSY: &str = "Still moving prompts…";
pub(crate) const UNSUPPORTED: &str = "Hermes does not offer that action for this prompt.";
pub(crate) const LOST_ROW: &str = "A lost prompt can only be discarded. Press d to discard it.";
pub(crate) const NO_GENERATION: &str =
    "Hermes gave no turn number for this lost prompt, so Talaria cannot discard it.";
pub(crate) const EMPTY_EDIT: &str =
    "An empty edit would remove the prompt. Press d on it to remove it instead.";
pub(crate) const SAVED_AT_BACK: &str =
    "Saved. Hermes moved the edited prompt to the back of the queue.";
pub(crate) const JUST_STARTED: &str = "That prompt had just started, so Hermes stopped it.";
pub(crate) const NOT_IN_QUEUE: &str = "That prompt is no longer in the queue.";
pub(crate) const QUEUE_CHANGED: &str =
    "The queue changed while moving prompts. This is the order Hermes holds.";
pub(crate) const CHANGE_UNCONFIRMED: &str = "Hermes did not confirm the change. Check the queue.";
pub(crate) const STEER_RETURNED: &str =
    "The steer did not reach the turn. The prompt is back in the queue in its place.";
pub(crate) const STEER_UNCONFIRMED: &str =
    "The running turn did not confirm the steer. Your text is back in the composer.";
pub(crate) const TURN_ENDED_FIRST: &str =
    "The turn you meant to stop already ended. Your prompt is first in the queue.";
pub(crate) const ABOUT_TO_START: &str = "Hermes is about to start it.";
pub(crate) const PAUSED: &str = "The queue is paused behind a prompt lost in a gateway restart. Press d on it to discard it first.";
pub(crate) const REMOVAL_UNCONFIRMED: &str =
    "Hermes did not confirm the removal. The prompt is still queued.";
pub(crate) const DISCARD_UNCONFIRMED: &str =
    "Hermes did not confirm the discard. Press d to try again.";
pub(crate) const REMOVAL_TOO_LATE: &str = "That prompt had already started, so it was not removed.";
pub(crate) const NOTHING_QUEUED: &str = "Nothing is queued.";
pub(crate) const VOICE_ENDED: &str = "Voice chat ended; nothing was sent.";
pub(crate) const PROMPT_UNCONFIRMED: &str =
    "Hermes did not confirm the prompt. It is back in the composer.";
pub(crate) const DRAFT_KEPT: &str = "Your draft was kept, so the returned prompt is shown here:";
pub(crate) const SESSION_IDLE: &str =
    "No turn is running to steer. The prompt stays queued and runs next.";
pub(crate) const EDIT_STARTED: &str = "That prompt started before you saved it. Your edited text is still in the composer; press Enter to send it as a new prompt.";
pub(crate) const STEER_SENT_SUFFIX: &str = " · sent; the agent reads it after its next tool call";

/// The notice for a refusal, or none when this layer does not map that pair.
///
/// Issue 232. Queue and steer. Plain submits keep the submit path's own notice, so
/// [`QueueAction::Submit`] always maps to none.
#[must_use]
pub fn refusal_notice(action: QueueAction, reason: &str) -> Option<&'static str> {
    use QueueAction::{Edit, Move, Remove, SendNow, Steer, SteerText, Submit};
    if action == Submit {
        return None;
    }
    match (reason, action) {
        ("permission_denied", _) => Some("Hermes does not let this client change that prompt."),
        ("queue_full", Edit) => Some(
            "Hermes holds at most 32 queued prompts, so the edit could not be saved. Your edited text is still in the composer.",
        ),
        ("queue_full", Move | SendNow) => Some(
            "Hermes holds at most 32 queued prompts, so prompts cannot be moved now. Remove one first.",
        ),
        ("admission_started", Edit) => Some(EDIT_STARTED),
        ("admission_started", _) => Some("That prompt has already started, so it was not changed."),
        ("session_idle", Steer) => Some(SESSION_IDLE),
        ("steer_unavailable", Steer) => Some(
            "The running turn could not take the steer, so the prompt stays queued in its place.",
        ),
        ("unsupported_control", SteerText) => Some("This session's turn does not accept steering."),
        ("unsupported_control", _) => Some(UNSUPPORTED),
        ("unknown_execution", Remove) => {
            Some("That prompt was lost in a gateway restart. Press d again to discard it.")
        }
        ("execution_not_ready", SteerText) => Some(
            "The running turn cannot take a steer yet. Your text is queued as the next prompt.",
        ),
        ("not_found", _) => Some(NOT_IN_QUEUE),
        ("admission_conflict", _) => {
            Some("Hermes matched this change to a different prompt, so nothing changed. Try again.")
        }
        ("stale_generation", Remove) => Some(REMOVAL_TOO_LATE),
        ("stale_generation", SteerText) => {
            Some("That turn has already ended. Your text is queued as the next prompt.")
        }
        ("stale_generation", _) => Some("That turn has already ended."),
        _ => None,
    }
}

/// First line of `text`, cut to 60 characters with an ellipsis.
///
/// Issue 232. Queue and steer. The cut keeps the whole result at 60 characters.
#[must_use]
pub fn preview(text: &str) -> String {
    const LIMIT: usize = 60;
    let line = text.lines().next().unwrap_or("");
    if line.chars().count() <= LIMIT {
        return line.to_string();
    }
    let mut cut: String = line.chars().take(LIMIT - 1).collect();
    cut.push('…');
    cut
}
