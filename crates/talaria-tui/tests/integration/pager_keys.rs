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

#[test]
fn copy_notice_wraps_without_truncating_at_eighty_columns() {
    let state = replay_fixture("plain-answer.jsonl");
    let cells = state.cells();
    let ctx = context(cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 24, &mut index);
    pager.show_notice(
        "Copy requested: reply 1, 4 characters. Your terminal must allow OSC 52 clipboard writes.",
    );

    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 24));
    pager.draw(
        buf.area,
        &mut buf,
        &ctx,
        &talaria_tui::history::PlainCellLines,
    );
    let footer_rows = format!(
        "{} {}",
        support::row_text(&buf, 22).trim(),
        support::row_text(&buf, 23).trim()
    );
    assert!(
        footer_rows.contains("Copy requested: reply 1, 4 characters."),
        "copy prefix missing: {footer_rows}"
    );
    assert!(
        footer_rows.contains("Your terminal must allow OSC 52 clipboard writes."),
        "OSC 52 instruction clipped at 80 columns: {footer_rows}"
    );
    assert!(
        !footer_rows.contains('\u{2026}'),
        "copy notice should wrap instead of ellipsizing: {footer_rows}"
    );
    assert!(
        support::row_text(&buf, 23).contains("cell "),
        "bottom footer row should still show the position counter"
    );
}
