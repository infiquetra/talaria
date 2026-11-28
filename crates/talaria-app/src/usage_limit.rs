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
