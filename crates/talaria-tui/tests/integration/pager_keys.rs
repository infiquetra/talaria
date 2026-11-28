//! The pager copy key asks for the reply at the top and leaves the pager open.

use crate::pager_support as support;

use support::{context, replay_fixture};
use talaria_tui::pager::{Pager, PagerAction, PagerOutcome, SearchIndex};

#[test]
fn copy_key_requests_the_top_assistant_reply() {
    let state = replay_fixture("plain-answer.jsonl");
    let cells = state.cells();
    let ctx = context(cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 24, &mut index);

    let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
    let outcome = pager.handle_action(PagerAction::Copy, &ctx, &mut index);
    assert!(
        matches!(outcome, PagerOutcome::Copy { index: 0 }),
        "copy asks for the top cell and does not close the pager"
    );
}
