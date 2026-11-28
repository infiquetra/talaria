//! What a usage-limit card does once its model switch has been answered.

use talaria_core::models::switch_was_accepted;
use talaria_core::mutation::MutationOutcome;
use talaria_core::rewind::RewindInput;
use talaria_core::session::UsageLimitCard;

/// After an accepted switch, close the card and ask for one retry of the failed turn. Any
/// other outcome leaves the card open and retries nothing.
pub fn after_usage_limit_switch(
    card: &mut Option<UsageLimitCard>,
    outcome: &MutationOutcome,
) -> Option<RewindInput> {
    if card.is_some() && switch_was_accepted(outcome) {
        *card = None;
        // The card's turn is finished: a running turn has already closed it.
        Some(RewindInput::Retry {
            turn_running: false,
        })
    } else {
        None
    }
}

/// The session, and the refused turn within it, that a usage-limit card was offered for.
///
/// Each card that opens gets the next `offer` number, so a switch sent for one card never
/// settles a later card, even in the same session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageLimitOwner {
    pub session: talaria_core::directory::rows::RowKey,
    pub offer: u64,
}
