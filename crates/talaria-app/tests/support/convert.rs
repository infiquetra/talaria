//! Turns what a card hands back into the reducer's events.
//!
//! This is test support. It stands in for the conversion the finished application performs when
//! it wires cards to the session reducer, and the demo includes the same file by path.

#![allow(dead_code)]

use talaria_app::prompts::PromptService;
use talaria_core::session::{Effect, Event, SessionState, update};
use talaria_tui::prompts::PromptIntent;

/// Apply one card intent.
///
/// An approval with a note stores the note on the service first, so that the later `resolved`
/// reply can turn it into an ordinary prompt. Then the matching answer event goes through the
/// reducer, and the effects it returns are handed back for the caller to execute.
///
/// A masked intent produces no reducer event: masked values never touch session state. The
/// caller answers those with `PromptService::answer_masked`, so this returns no effects.
pub fn apply_intent(
    service: &mut PromptService,
    state: &mut SessionState,
    intent: PromptIntent,
) -> Vec<Effect> {
    match intent {
        PromptIntent::Approve { key, choice, note } => {
            if let Some(note) = note {
                service.set_note(&key, note);
            }
            update(state, Event::AnswerApproval { key, choice })
        }
        PromptIntent::Clarify { key, answer } => {
            update(state, Event::AnswerClarify { key, answer })
        }
        PromptIntent::Masked { .. } => Vec::new(),
    }
}
