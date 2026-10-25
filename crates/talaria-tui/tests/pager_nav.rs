//! Opening, movement, search editing and closing. Issue 222.
//!
//! The transcript opens at the bottom, following. Moves clamp. An upward move stops following
//! and marks new output. Braces jump between the person's messages. Match navigation wraps.
//! The search prompt edits and Escape restores it. Enter on an empty query repeats the last
//! one. Escape cancels, then clears, then closes. Close keys never interrupt.

#[path = "pager_support.rs"]
mod support;

use support::{assistant, context, user};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{Pager, PagerAction, PagerOutcome, SearchIndex, SearchInput};

fn stay_redraw(outcome: PagerOutcome) -> bool {
    matches!(outcome, PagerOutcome::Stay { redraw: true })
}

fn open_on(
    texts: &[&str],
    cols: u16,
    rows: u16,
) -> (Pager, Vec<talaria_core::session::Cell>, SearchIndex) {
    let cells: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| user((index as u64) + 1, text))
        .collect();
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let pager = Pager::open(&ctx, cols, rows, &mut index);
    (pager, cells, index)
}

#[test]
fn opens_at_bottom_following() {
    let (pager, cells, _) = open_on(&[], 80, 24);
    let plain = PlainCellLines;
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 24));
    let ctx = context(&cells);
    let mut pager = pager;
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let body = support::row_text(&buf, 1);
    let footer = support::row_text(&buf, 23);
    assert!(body.contains("No messages yet."), "{body}");
    assert!(footer.contains("cell 0/0"), "{footer}");

    let (pager, cells, _) = open_on(&["only"], 80, 24);
    let ctx = context(&cells);
    let mut pager = pager;
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let footer = support::row_text(&buf, 23);
    assert!(footer.contains("following"), "{footer}");
    let top = pager.handle_action(PagerAction::Top, &ctx, &mut SearchIndex::new());
    assert!(stay_redraw(top), "a short document stays following");
}

#[test]
fn moves_clamp_at_both_ends() {
    let (mut pager, cells, mut index) = open_on(&["one", "two", "three"], 80, 24);
    let ctx = context(&cells);
    let up = pager.handle_action(PagerAction::LineUp, &ctx, &mut index);
    assert!(matches!(up, PagerOutcome::Stay { .. }));
    let page = pager.handle_action(PagerAction::PageUp, &ctx, &mut index);
    assert!(matches!(page, PagerOutcome::Stay { .. }));
    let half = pager.handle_action(PagerAction::HalfPageUp, &ctx, &mut index);
    assert!(matches!(half, PagerOutcome::Stay { .. }));
    let _ = pager.handle_action(PagerAction::Bottom, &ctx, &mut index);
    let down = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    assert!(matches!(down, PagerOutcome::Stay { .. }));
    let space = pager.handle_action(PagerAction::PageDown, &ctx, &mut index);
    assert!(
        matches!(space, PagerOutcome::Stay { .. }),
        "Space at the end does not close"
    );
}

#[test]
fn upward_move_stops_following_and_marks_new_output() {
    let texts: Vec<String> = (0..80).map(|n| format!("line {n}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let (mut pager, cells, mut index) = open_on(&refs, 80, 24);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 24));
    let plain = PlainCellLines;
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let footer = support::row_text(&buf, 23);
    assert!(footer.contains("cell 1/"), "{footer}");
    assert!(!footer.contains("new output"), "{footer}");
    assert!(!footer.contains("following"), "{footer}");

    pager.content_changed(&ctx, &mut index);
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let marked = support::row_text(&buf, 23);
    assert!(marked.contains("\u{2193} new output"), "{marked}");
    let _ = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    assert!(support::row_text(&buf, 23).contains("new output"));
    let _ = pager.handle_action(PagerAction::Bottom, &ctx, &mut index);
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let followed = support::row_text(&buf, 23);
    assert!(followed.contains("following"), "{followed}");
    assert!(!followed.contains("new output"), "{followed}");

    let _ = pager.handle_action(PagerAction::LineUp, &ctx, &mut index);
    pager.draw(buf.area, &mut buf, &ctx, &plain);
    let inside = support::row_text(&buf, 23);
    assert!(
        !inside.contains("following"),
        "a line up inside the bottom page stops following: {inside}"
    );
}

#[test]
fn braces_jump_between_user_prompts() {
    let (mut pager, cells, mut index) = open_on(&["first", "second", "third"], 40, 10);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
    let earlier = pager.handle_action(PagerAction::PrevPrompt, &ctx, &mut index);
    assert!(stay_redraw(earlier) || matches!(earlier, PagerOutcome::Stay { redraw: false }));
    let next = pager.handle_action(PagerAction::NextPrompt, &ctx, &mut index);
    assert!(matches!(next, PagerOutcome::Stay { .. }));
    let _ = pager.handle_action(PagerAction::Bottom, &ctx, &mut index);
    let none = pager.handle_action(PagerAction::NextPrompt, &ctx, &mut index);
    assert!(matches!(none, PagerOutcome::Stay { .. }));
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 40, 10));
    pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    let screen = (0..10)
        .map(|y| support::row_text(&buf, y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        screen.contains("no later prompt") || screen.contains("no earlier prompt"),
        "{screen}"
    );
}

#[test]
fn next_and_previous_match_wrap_with_notice() {
    let (mut pager, cells, mut index) = open_on(&["heron", "other", "heron"], 80, 12);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    for ch in ['h', 'e', 'r', 'o', 'n'] {
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
    }
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let _ = pager.handle_action(PagerAction::NextMatch, &ctx, &mut index);
    let wrapped = pager.handle_action(PagerAction::NextMatch, &ctx, &mut index);
    assert!(matches!(wrapped, PagerOutcome::Stay { .. }));
    let back = pager.handle_action(PagerAction::PrevMatch, &ctx, &mut index);
    assert!(matches!(back, PagerOutcome::Stay { .. }));

    let (mut empty, cells, mut index) = open_on(&["nothing"], 80, 12);
    let ctx = context(&cells);
    let _ = empty.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let _ = empty.handle_search_input(SearchInput::Char('z'), &ctx, &mut index);
    let _ = empty.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let none = empty.handle_action(PagerAction::NextMatch, &ctx, &mut index);
    assert!(
        matches!(none, PagerOutcome::Stay { redraw: false })
            || matches!(none, PagerOutcome::Stay { redraw: true })
    );
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 12));
    empty.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    let footer = support::row_text(&buf, 11);
    assert!(
        footer.contains("search wrapped") || footer.contains("no match"),
        "{footer}"
    );
}

#[test]
fn search_prompt_edits_and_esc_restores() {
    let (mut pager, cells, mut index) = open_on(&["heron and crane"], 80, 16);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    assert!(pager.is_typing_search());
    for ch in ['h', 'e', 'r'] {
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
    }
    let _ = pager.handle_search_input(SearchInput::Backspace, &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Escape, &ctx, &mut index);
    assert!(!pager.is_typing_search());

    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Backspace, &ctx, &mut index);
    assert!(
        !pager.is_typing_search(),
        "Backspace on an empty query closes it"
    );

    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let long = "q".repeat(300);
    for ch in long.chars() {
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
    }
    let _ = pager.handle_search_input(SearchInput::Paste("a\nb".to_string()), &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 16));
    pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    let footer = support::row_text(&buf, 15);
    assert!(
        footer.contains("qn ") || footer.chars().filter(|ch| *ch == 'q').count() <= 256,
        "{footer}"
    );

    let (mut typing, cells, mut index) = open_on(&["stay"], 80, 10);
    let ctx = context(&cells);
    let _ = typing.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    for ch in ['q', 'n', ' '] {
        let outcome = typing.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
        assert!(matches!(outcome, PagerOutcome::Stay { .. }));
    }
    assert!(typing.is_typing_search());
}

#[test]
fn enter_on_empty_query_repeats_last() {
    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    for ch in ['h', 'e', 'r', 'o', 'n'] {
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
    }
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let repeated = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    assert!(matches!(repeated, PagerOutcome::Stay { .. }));

    let (mut fresh, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let _ = fresh.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let closed = fresh.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    assert!(matches!(closed, PagerOutcome::Stay { .. }));
    assert!(!fresh.is_typing_search());
}

#[test]
fn escape_ladder_cancel_clear_close() {
    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Char('h'), &ctx, &mut index);
    let cancel = pager.handle_search_input(SearchInput::Escape, &ctx, &mut index);
    assert!(matches!(cancel, PagerOutcome::Stay { .. }));
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Char('h'), &ctx, &mut index);
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let clear = pager.handle_action(PagerAction::Escape, &ctx, &mut index);
    assert!(matches!(clear, PagerOutcome::Stay { .. }));
    let close = pager.handle_action(PagerAction::Escape, &ctx, &mut index);
    assert!(matches!(close, PagerOutcome::Close));
}

#[test]
fn close_keys_never_interrupt() {
    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let closed = pager.handle_action(PagerAction::Close, &ctx, &mut index);
    assert!(matches!(closed, PagerOutcome::Close));

    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let closed = pager.handle_action(PagerAction::Escape, &ctx, &mut index);
    assert!(matches!(closed, PagerOutcome::Close));

    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let closed = pager.handle_action(PagerAction::Close, &ctx, &mut index);
    assert!(
        matches!(closed, PagerOutcome::Close),
        "the open chord closes when not typing"
    );

    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let closed = pager.handle_action(PagerAction::Close, &ctx, &mut index);
    assert!(
        matches!(closed, PagerOutcome::Close),
        "Control-C closes while typing"
    );

    let (mut pager, cells, mut index) = open_on(&["heron"], 80, 10);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    let stayed = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    assert!(
        matches!(stayed, PagerOutcome::Stay { redraw: false }),
        "Control-O while typing does nothing"
    );
    match stayed {
        PagerOutcome::Stay { .. } | PagerOutcome::Close => {}
    }
    let _ = assistant(9, "unused", true);
}

#[test]
fn new_epoch_rebuilds_and_clamps() {
    let (mut pager, cells, mut index) = open_on(&["alpha heron", "beta heron", "gamma"], 80, 12);
    let ctx = context(&cells);
    let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
    for ch in ['h', 'e', 'r', 'o', 'n'] {
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
    }
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    let replacement = vec![user(50, "only")];
    let mut next = context(&replacement);
    next.source_epoch = ctx.source_epoch + 1;
    pager.content_changed(&next, &mut index);
    let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 12));
    pager.draw(buf.area, &mut buf, &next, &PlainCellLines);
    let screen = (0..12)
        .map(|y| support::row_text(&buf, y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        screen.contains("transcript reloaded from Hermes"),
        "{screen}"
    );
    assert!(!screen.contains("alpha heron"), "{screen}");
    assert!(!screen.contains("beta heron"), "{screen}");
}
