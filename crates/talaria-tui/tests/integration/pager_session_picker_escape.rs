//! Line and paragraph separators must not reach a pager or session-picker cell.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};
use talaria_core::directory::state::{PickerAction, PickerInput, PickerState, update};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{ConnectionBanner, Pager, PagerContext, SearchIndex, SearchInput};
use talaria_tui::session_picker::{PickerTheme, SessionPickerView};

const NOW: f64 = 1_792_700_000.0;
/// Gateway text: line separator, paragraph separator, ESC, then two C1 controls.
const HOSTILE: &str = "ok\u{2028}\u{2029}\u{1b}[2J\u{0085}\u{009b}";
const STAND_IN: &str = "ok‹U+2028›‹U+2029›\u{241b}[2J‹U+0085›‹U+009B›";

fn forbidden(ch: char) -> bool {
    ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}'
}

fn assert_plain(buf: &Buffer) {
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let symbol = buf[(x, y)].symbol();
            for ch in symbol.chars() {
                assert!(
                    !forbidden(ch),
                    "cell {x},{y} symbol {symbol:?} holds {ch:?}"
                );
            }
        }
    }
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn pager_buffer(title: &str, cols: u16, rows: u16) -> Buffer {
    let cells = vec![crate::pager_support::user(1, "hello")];
    let ctx = PagerContext {
        cells: &cells,
        source_epoch: 1,
        title,
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    };
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, cols, rows, &mut index);
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows));
    pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    buf
}

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn directory_row(profile: &str, id: &str, title: &str, model: &str, source: &str) -> DirectoryRow {
    DirectoryRow {
        key: key(profile, id),
        title: title.to_owned(),
        source: Some(source.to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 1,
        running: false,
        model: Some(model.to_owned()),
        origin: RowOrigin::Live,
    }
}

fn opened(rows: Vec<DirectoryRow>) -> PickerState {
    let mut state = PickerState::new();
    update(
        &mut state,
        PickerInput::Opened {
            profiles: vec!["fixture".to_owned()],
            foreground: None,
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    update(
        &mut state,
        PickerInput::Listed {
            generation: 1,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Live,
            rows,
            skipped: 0,
        },
        20,
    );
    state
}

fn picker_buffer(state: &PickerState, cols: u16, rows: u16) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows));
    SessionPickerView::draw(state, buf.area, &mut buf, NOW, &PickerTheme::new());
    buf
}

#[test]
fn pager_header_draws_separators_as_stand_ins() {
    let buf = pager_buffer(HOSTILE, 80, 12);
    let header = row_text(&buf, 0);
    assert!(
        header.contains(STAND_IN),
        "header should show stand-ins, got {header:?}"
    );
    assert_plain(&buf);
}

#[test]
fn pager_search_footer_draws_separators_as_stand_ins() {
    let cells = vec![crate::pager_support::user(1, "hello")];
    let ctx = PagerContext {
        cells: &cells,
        source_epoch: 1,
        title: "notes",
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    };
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 12, &mut index);
    pager.handle_search_input(SearchInput::Paste(HOSTILE.into()), &ctx, &mut index);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 12));
    pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    let footer = row_text(&buf, 11);
    let caret_after = format!("{STAND_IN}\u{258f}");
    assert!(
        footer.contains(&caret_after),
        "search caret should follow the stand-in, got {footer:?}"
    );
    assert_plain(&buf);
}

#[test]
fn session_picker_title_draws_separators_as_stand_ins() {
    let state = opened(vec![directory_row(
        "fixture", "s-1", HOSTILE, HOSTILE, HOSTILE,
    )]);
    let buf = picker_buffer(&state, 160, 24);
    let screen = (0..buf.area.height)
        .map(|y| row_text(&buf, y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        screen.contains(STAND_IN),
        "row fields should show stand-ins, got {screen:?}"
    );
    assert_plain(&buf);
}

#[test]
fn session_picker_query_draws_separators_as_stand_ins() {
    let mut state = opened(vec![directory_row(
        "fixture", "s-1", "hello", "model", "tui",
    )]);
    update(&mut state, PickerInput::Action(PickerAction::Search), 20);
    update(&mut state, PickerInput::Paste(HOSTILE.to_owned()), 20);
    let typing = picker_buffer(&state, 100, 24);
    let notice = row_text(&typing, 22);
    let caret_after = format!("Search: {STAND_IN}\u{258f}");
    assert!(
        notice.contains(&caret_after),
        "search caret should follow the stand-in, got {notice:?}"
    );
    assert_plain(&typing);

    update(&mut state, PickerInput::Enter, 20);
    let confirmed = picker_buffer(&state, 120, 24);
    let header = row_text(&confirmed, 0);
    assert!(
        header.contains(&format!("Search: \"{STAND_IN}\"")),
        "confirmed query should show stand-ins, got {header:?}"
    );
    assert_plain(&confirmed);

    let narrow = picker_buffer(&state, 24, 8);
    assert_plain(&narrow);
}

#[test]
fn session_picker_export_dialog_draws_separators_as_stand_ins() {
    let mut state = opened(vec![directory_row(
        "fixture", HOSTILE, HOSTILE, "model", "tui",
    )]);
    state.set_save_folder(format!("/tmp/{HOSTILE}"));
    update(&mut state, PickerInput::Action(PickerAction::Export), 20);
    let buf = picker_buffer(&state, 160, 24);
    let heading = row_text(&buf, 21);
    let saves = row_text(&buf, 23);
    assert!(
        heading.contains(&format!("Export \"{STAND_IN}\"")),
        "dialog title should show stand-ins, got {heading:?}"
    );
    assert!(
        saves.contains("hermes_session_ok2J.md"),
        "fallback name keeps only safe id characters, got {saves:?}"
    );
    assert!(
        saves.contains("‹U+2028›"),
        "save folder separators should show as stand-ins, got {saves:?}"
    );
    assert_plain(&buf);
}

#[test]
fn session_picker_rename_caret_stays_after_expanded_stand_in() {
    let mut state = opened(vec![directory_row(
        "fixture",
        "s-1",
        "ok\u{2028}X",
        "model",
        "tui",
    )]);
    update(&mut state, PickerInput::Action(PickerAction::Rename), 20);
    update(&mut state, PickerInput::Left, 20);
    let buf = picker_buffer(&state, 120, 24);
    let notice = row_text(&buf, 22);
    assert!(
        notice.contains(": ok‹U+2028›\u{258f}X"),
        "caret should sit after the stand-in, got {notice:?}"
    );
    assert_plain(&buf);
}
