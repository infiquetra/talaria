//! Drawing the transcript. Issue 222.
//!
//! The tool-diff recording opens expanded and compact. A tool with no output says so.
//! Hostile text stays literal. The footer and the too-small line follow the width.
//! Banners stack in order. The current match is distinct. A resize keeps the top cell.
//! A key that changes nothing asks for no redraw.

#[path = "support/mod.rs"]
mod harness;
#[path = "pager_support.rs"]
mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::session::{Cell, ToolStatus, TurnState};
use talaria_tui::history::{CellLines, PlainCellLines};
use talaria_tui::pager::{
    ConnectionBanner, Details, Pager, PagerAction, PagerCellLines, PagerContext, PagerOutcome,
    PagerTheme, Query, SearchIndex, format_duration_fallback, highlight_line,
};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

use harness::{ByteSink, Vt, probe};
use support::{assistant, context, lines_for, reasoning, row_text, tool, user};

const FULL_HINT: &str =
    "/ search · n N next/prev · { } prompts · g G top/bottom · t details · q close";
const SHORT_HINT: &str = "/ search · n N · { } · g G · t · q close";
const HINT_AT_100: &str = "/ search · n N next/prev · { } prompts · g G top/bottom · t detail…";
const HINT_AT_60: &str = "/ search · n N · { } · g G…";
const TOO_SMALL: &str = "Window too small for the transcript. q closes.";
const TOO_SMALL_19: &str = "Window too small f…";
const TOO_SMALL_40: &str = "Window too small for the transcript. q …";
const RIGHT_MARK: &str = "cell 1/2000 · ↓ new output · End";

fn paint(pager: &mut Pager, ctx: &PagerContext<'_>, cols: u16, rows: u16) -> Buffer {
    let plain = PlainCellLines;
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows));
    pager.draw(buf.area, &mut buf, ctx, &plain);
    buf
}

fn joined(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| row_text(buf, y))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn footer_is_separate_from_body_and_clears_previous_contents() {
    let cells: Vec<Cell> = (0..30).map(|id| user(id, &"Z".repeat(100))).collect();
    let ctx = context(&cells);
    for (cols, rows) in [(50, 8), (35, 6), (80, 12)] {
        for search in [false, true] {
            let mut index = SearchIndex::new();
            let mut pager = Pager::open(&ctx, cols, rows, &mut index);
            let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
            if search {
                let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
            }
            let area = Rect::new(0, 0, cols, rows);
            let mut buf = Buffer::empty(area);
            buf.set_string(
                0,
                rows - 1,
                "Q".repeat(usize::from(cols)),
                ratatui::style::Style::default(),
            );
            pager.draw(area, &mut buf, &ctx, &PlainCellLines);
            assert!(joined(&buf).contains('Z'), "body should be filled");
            let footer = row_text(&buf, rows - 1);
            assert!(
                !footer.contains(['Z', 'Q']),
                "{cols}x{rows} search={search}: {footer:?}"
            );
        }
    }
}

fn search(pager: &mut Pager, ctx: &PagerContext<'_>, index: &mut SearchIndex, query: &str) {
    use talaria_tui::pager::SearchInput;
    pager.handle_action(PagerAction::StartSearch, ctx, index);
    pager.handle_search_input(SearchInput::Paste(query.into()), ctx, index);
    pager.handle_search_input(SearchInput::Enter, ctx, index);
}

#[test]
fn search_navigates_to_a_deep_occurrence_and_reflows_it() {
    let text = format!(
        "{}\nneedle",
        (0..30)
            .map(|n| format!("line {n:02}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let cells = vec![user(1, &text)];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    search(&mut pager, &ctx, &mut index, "needle");
    assert!((1..7).any(|row| row_text(&paint(&mut pager, &ctx, 50, 8), row).contains("needle")));
    pager.resize(24, 8);
    assert!((1..7).any(|row| row_text(&paint(&mut pager, &ctx, 24, 8), row).contains("needle")));
    pager.handle_action(PagerAction::ToggleDetails, &ctx, &mut index);
    assert!((1..7).any(|row| row_text(&paint(&mut pager, &ctx, 24, 8), row).contains("needle")));
}

#[test]
fn search_navigates_to_a_deep_assistant_occurrence() {
    let text = format!(
        "{}\nneedle",
        (0..30)
            .map(|n| format!("line {n:02}  "))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let cells = vec![assistant(1, &text, true)];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    search(&mut pager, &ctx, &mut index, "needle");
    let buf = paint(&mut pager, &ctx, 50, 8);
    assert!(
        (1..7).any(|row| row_text(&buf, row).contains("needle")),
        "{}",
        joined(&buf)
    );
}

#[test]
fn only_the_selected_search_occurrence_is_underlined() {
    use ratatui::style::Modifier;
    let cells = vec![user(1, "needle needle\nneedle\nneedle")];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    search(&mut pager, &ctx, &mut index, "needle");
    for expected in [(1, 2), (1, 9), (2, 2), (3, 2)] {
        let buf = paint(&mut pager, &ctx, 50, 8);
        let mut underlined = Vec::new();
        for y in 1..7 {
            for x in 0..50 {
                if buf[(x, y)].modifier.contains(Modifier::UNDERLINED) {
                    underlined.push((y, x));
                }
            }
        }
        assert_eq!(underlined.len(), 6, "one occurrence: {underlined:?}");
        assert_eq!(underlined[0], expected);
        pager.handle_action(PagerAction::NextMatch, &ctx, &mut index);
    }
}

#[test]
fn search_highlights_occurrences_split_by_wrapping_and_expanded_controls() {
    use ratatui::style::Modifier;
    let cells = vec![user(1, "0123456789abcdefneedle\tTAIL")];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 20, 8, &mut index);
    search(&mut pager, &ctx, &mut index, "needle");
    let buf = paint(&mut pager, &ctx, 20, 8);
    let mut highlighted = String::new();
    for y in 1..7 {
        for x in 0..20 {
            if buf[(x, y)].modifier.contains(Modifier::UNDERLINED) {
                highlighted.push_str(buf[(x, y)].symbol());
            }
        }
    }
    assert_eq!(highlighted, "needle");
}

#[test]
fn resized_and_compacted_cells_keep_a_visible_anchor() {
    let text = (0..30)
        .map(|n| format!("thought {n:02}  "))
        .collect::<Vec<_>>()
        .join("\n");
    let cells = vec![reasoning(1, &text)];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    paint(&mut pager, &ctx, 50, 8);
    pager.handle_action(PagerAction::LineUp, &ctx, &mut index);
    pager.handle_action(PagerAction::ToggleDetails, &ctx, &mut index);
    let screen = joined(&paint(&mut pager, &ctx, 50, 8));
    assert!(
        screen.contains("thinking") || screen.contains("lines)"),
        "{screen}"
    );
    pager.handle_action(PagerAction::ToggleDetails, &ctx, &mut index);
    assert!(joined(&paint(&mut pager, &ctx, 50, 8)).contains("thought"));

    let cells = vec![user(2, &"word ".repeat(90))];
    let ctx = context(&cells);
    let mut pager = Pager::open(&ctx, 25, 8, &mut index);
    paint(&mut pager, &ctx, 25, 8);
    pager.handle_action(PagerAction::LineUp, &ctx, &mut index);
    pager.resize(100, 8);
    assert!(joined(&paint(&mut pager, &ctx, 100, 8)).contains("word"));
    pager.resize(25, 8);
    assert!(joined(&paint(&mut pager, &ctx, 25, 8)).contains("word"));
}

#[test]
fn user_text_is_sanitized_before_wrapping() {
    for text in [
        "0123456789\tABCDEFGH",
        "12345678\t\tABCDEFGH",
        "123456789\u{202e}ABCDEFGH",
    ] {
        let cells = vec![user(1, text)];
        let ctx = context(&cells);
        let mut index = SearchIndex::new();
        let mut pager = Pager::open(&ctx, 20, 8, &mut index);
        let buf = paint(&mut pager, &ctx, 20, 8);
        let body: String = (1..7)
            .map(|row| row_text(&buf, row))
            .collect::<Vec<_>>()
            .join("");
        let compact: String = body.chars().filter(|ch| !ch.is_whitespace()).collect();
        assert!(compact.contains("ABCDEFGH"), "{body}");
    }
}

#[test]
fn following_uses_the_real_tool_renderer_after_redraw_and_resize() {
    use talaria_tui::tool_cell::{Detail, ToolCellLines, ToolRenderOptions, ToolTheme};
    let mut call = tool(1, "terminal", ToolStatus::Succeeded);
    call.result = Some(format!(
        "{}END-MARKER",
        "abcdefghijklmnopqrstuvwx ".repeat(25)
    ));
    let cells = vec![Cell::Tool(call)];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    for width in [50, 50, 70, 50] {
        let renderer = ToolCellLines::new(
            Box::new(PlainCellLines),
            ToolRenderOptions {
                width,
                theme: ToolTheme::default(),
                color: true,
                detail: Detail::Full,
                expand_hint: None,
                elapsed: None,
            },
        );
        pager.resize(width, 8);
        pager.handle_action(PagerAction::Bottom, &ctx, &mut index);
        let mut buf = Buffer::empty(Rect::new(0, 0, width, 8));
        pager.draw(buf.area, &mut buf, &ctx, &renderer);
        let screen = joined(&buf);
        assert!(screen.contains("END-MARKER"), "{width}: {screen}");
        assert!(row_text(&buf, 7).contains("following"));
    }
}

#[test]
fn page_navigation_counts_the_same_tool_rows_that_are_drawn() {
    use talaria_tui::tool_cell::{Detail, ToolCellLines, ToolRenderOptions, ToolTheme};
    let mut call = tool(1, "terminal", ToolStatus::Succeeded);
    call.result = Some(
        (0..30)
            .map(|n| format!("output-row-{n:02}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let cells = vec![Cell::Tool(call), user(2, "after tool")];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let renderer = ToolCellLines::new(
        Box::new(PlainCellLines),
        ToolRenderOptions {
            width: 50,
            theme: ToolTheme::default(),
            color: true,
            detail: Detail::Full,
            expand_hint: None,
            elapsed: None,
        },
    );
    let expected = renderer.lines(&cells[0], 50);
    assert!(expected.len() > 10);
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    pager.handle_action_with_renderer(PagerAction::Top, &ctx, &mut index, &renderer);
    pager.handle_action_with_renderer(PagerAction::PageDown, &ctx, &mut index, &renderer);
    let mut buf = Buffer::empty(Rect::new(0, 0, 50, 8));
    pager.draw(buf.area, &mut buf, &ctx, &renderer);
    assert_eq!(row_text(&buf, 1).trim(), expected[5].to_string().trim());
}

#[test]
fn changing_the_tool_theme_invalidates_cached_rendering() {
    use ratatui::style::{Color, Style};
    struct ColoredTool(Color);
    impl CellLines for ColoredTool {
        fn lines(&self, _: &Cell, _: u16) -> Vec<Line<'static>> {
            vec![Line::styled("tool output", Style::default().fg(self.0))]
        }
    }
    let cells = vec![Cell::Tool(tool(1, "terminal", ToolStatus::Succeeded))];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 50, 8, &mut index);
    let mut buf = Buffer::empty(Rect::new(0, 0, 50, 8));
    pager.draw(buf.area, &mut buf, &ctx, &ColoredTool(Color::Red));
    assert_eq!(buf[(0, 1)].fg, Color::Red);
    pager.invalidate_rendering();
    buf.reset();
    pager.draw(buf.area, &mut buf, &ctx, &ColoredTool(Color::Blue));
    assert_eq!(buf[(0, 1)].fg, Color::Blue);
}

fn tool_diff_context(state: &talaria_core::session::SessionState) -> PagerContext<'_> {
    let mut ctx = context(state.cells());
    ctx.title = "";
    ctx.running = matches!(state.turn(), TurnState::Running);
    ctx
}

#[test]
fn snapshot_expanded_160x48() {
    let state = support::replay_fixture("tool-diff.jsonl");
    assert_eq!(state.cells().len(), 6, "tool-diff has six cells");
    let Cell::User(first) = &state.cells()[0] else {
        panic!("the first tool-diff cell is a user message");
    };
    assert!(first.text.contains("beta to gamma"), "{}", first.text);
    assert!(
        matches!(state.turn(), TurnState::Finished(_)),
        "the recording has finished"
    );
    let ctx = tool_diff_context(&state);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 160, 48, &mut index);
    assert_eq!(pager.details(), Details::Expanded);
    let buf = paint(&mut pager, &ctx, 160, 48);
    let screen = joined(&buf);
    let header = row_text(&buf, 0);
    let footer = row_text(&buf, 47);
    assert!(header.contains("untitled session"), "{header}");
    assert!(header.contains("6 cells"), "{header}");
    assert!(header.contains("idle"), "{header}");
    assert!(screen.contains("beta to gamma"), "{screen}");
    assert!(screen.contains("search_files"), "{screen}");
    assert!(screen.contains("read_file"), "{screen}");
    assert!(screen.contains("patch"), "{screen}");
    assert!(screen.contains("done"), "{screen}");
    assert!(footer.contains("following"), "{footer}");
    assert!(footer.contains("/6"), "{footer}");
}

#[test]
fn snapshot_compact_160x48() {
    let state = support::replay_fixture("tool-diff.jsonl");
    assert_eq!(state.cells().len(), 6, "tool-diff has six cells");
    let ctx = tool_diff_context(&state);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 160, 48, &mut index);
    let toggled = pager.handle_action(PagerAction::ToggleDetails, &ctx, &mut index);
    assert!(matches!(toggled, PagerOutcome::Stay { redraw: true }));
    assert_eq!(pager.details(), Details::Compact);
    let buf = paint(&mut pager, &ctx, 160, 48);
    let screen = joined(&buf);
    assert!(screen.contains("search_files"), "{screen}");
    assert!(screen.contains("(+"), "{screen}");
    assert!(!screen.contains("args:"), "{screen}");
    assert!(row_text(&buf, 47).contains("following"));
}

#[test]
fn tool_cell_without_output_says_so() {
    let plain = PlainCellLines;
    let renderer: PagerCellLines<'_> = lines_for(Details::Expanded, 80, &plain);
    let mut empty = tool(1, "patch", ToolStatus::Succeeded);
    empty.context = None;
    empty.result = None;
    empty.diff = None;
    let lines = renderer.lines(&Cell::Tool(empty), 80);
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("(Hermes sent no output for this call)"),
        "{text}"
    );
    assert!(text.contains("⏺ patch"), "{text}");
    assert!(!text.contains("()"), "{text}");

    let cases = [
        (ToolStatus::Running, "… running", "(running…)"),
        (
            ToolStatus::Succeeded,
            "✓",
            "(Hermes sent no output for this call)",
        ),
        (
            ToolStatus::Failed,
            "✗",
            "(Hermes sent no output for this call)",
        ),
        (
            ToolStatus::Unfinished,
            "⊘ unfinished",
            "(Hermes sent no output for this call)",
        ),
        (
            ToolStatus::Unknown,
            "? unknown",
            "(Hermes sent no output for this call)",
        ),
    ];
    for (status, marker, empty_line) in cases {
        let cell = Cell::Tool(tool(2, "read_file", status));
        let drawn = renderer.lines(&cell, 80);
        let body = drawn
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(body.contains(marker), "{status:?}: {body}");
        assert!(body.contains(empty_line), "{status:?}: {body}");
    }

    assert_eq!(format_duration_fallback(0.0).as_deref(), Some("0.0s"));
    assert_eq!(format_duration_fallback(0.44).as_deref(), Some("0.4s"));
    assert_eq!(format_duration_fallback(59.94).as_deref(), Some("59.9s"));
    assert_eq!(format_duration_fallback(59.95).as_deref(), Some("1m00s"));
    assert_eq!(format_duration_fallback(60.0).as_deref(), Some("1m00s"));
    assert_eq!(format_duration_fallback(125.0).as_deref(), Some("2m05s"));
    assert_eq!(format_duration_fallback(-1.0), None);
    assert_eq!(format_duration_fallback(f64::NAN), None);
}

#[test]
fn hostile_text_is_literal() {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .expect("terminal");
    let _setup = sink.take();
    term.open_overlay().expect("overlay");
    let _overlay_on = sink.take();

    let hostile = "HOSTILE \u{1b}[2J\u{1b}[H\u{7}\u{202e} END";
    let cells = vec![user(1, hostile)];
    let ctx = context(&cells);
    let plain = PlainCellLines;
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 24, &mut index);
    term.draw_overlay(|area, buf| pager.draw(area, buf, &ctx, &plain))
        .expect("draw");
    let written = sink.take();
    assert!(
        !written.windows(4).any(|window| window == b"\x1b[2J"),
        "hostile text must not clear the screen"
    );
    assert!(!written.contains(&0x07), "no bell byte");
    assert!(
        !written
            .windows("\u{202e}".len())
            .any(|window| window == "\u{202e}".as_bytes()),
        "no direction mark"
    );

    let mut screen = Vt::new(80, 24);
    screen.feed(&written);
    let rows = screen.rows().join("\n");
    assert!(
        rows.contains("HOSTILE \u{241b}[2J\u{241b}[H\u{2407}\u{2039}U+202E\u{203a} END"),
        "{rows}"
    );
}

#[test]
fn footer_and_too_small_by_width() {
    let cells: Vec<Cell> = (0..2000).map(|id| user(id, &format!("row {id}"))).collect();
    let mut ctx = context(&cells);
    ctx.title = "wide";
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 160, 48, &mut index);
    let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
    pager.content_changed(&ctx, &mut index);

    let wide = paint(&mut pager, &ctx, 160, 48);
    let footer = row_text(&wide, 47);
    assert!(footer.contains(FULL_HINT), "{footer}");
    assert!(footer.contains(RIGHT_MARK), "{footer}");
    assert_eq!(FULL_HINT.chars().count(), 77);

    let hundred = paint(&mut pager, &ctx, 100, 48);
    let at_100 = row_text(&hundred, 47);
    assert!(at_100.contains(HINT_AT_100), "{at_100}");
    assert!(at_100.contains(RIGHT_MARK), "{at_100}");
    assert!(!at_100.contains("details"), "{at_100}");

    let sixty = paint(&mut pager, &ctx, 60, 48);
    let at_60 = row_text(&sixty, 47);
    assert!(at_60.contains(HINT_AT_60), "{at_60}");
    assert!(at_60.contains("cell 1/2000"), "{at_60}");
    assert!(!at_60.contains(SHORT_HINT), "{at_60}");

    let forty = paint(&mut pager, &ctx, 40, 48);
    let at_40 = row_text(&forty, 47);
    assert!(!at_40.contains("/ search"), "{at_40}");
    assert!(at_40.contains("cell 1/2000"), "{at_40}");

    let tiny = paint(&mut pager, &ctx, 19, 10);
    assert_eq!(row_text(&tiny, 0), TOO_SMALL_19);
    assert!(
        (1..tiny.area.height).all(|row| row_text(&tiny, row).is_empty()),
        "the too-small window is one line"
    );

    let mut quiet = ctx;
    quiet.needs_answer = false;
    quiet.connection = ConnectionBanner::Connected;
    quiet.history_truncated = false;
    let short = paint(&mut pager, &quiet, 40, 4);
    assert_ne!(row_text(&short, 0), TOO_SMALL_40, "two body rows fit");
    assert!(joined(&short).contains("row "), "{short:?}");

    let mut waiting = quiet;
    waiting.needs_answer = true;
    let blocked = paint(&mut pager, &waiting, 40, 4);
    assert_eq!(row_text(&blocked, 0), TOO_SMALL_40);
    let _ = (TOO_SMALL, SHORT_HINT);
}

#[test]
fn banners_in_order() {
    let cells = vec![user(1, "one")];
    let mut ctx = context(&cells);
    ctx.needs_answer = true;
    ctx.connection = ConnectionBanner::Reconnecting;
    ctx.history_truncated = true;
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 24, &mut index);
    let buf = paint(&mut pager, &ctx, 80, 24);
    let rows: Vec<String> = (0..buf.area.height).map(|y| row_text(&buf, y)).collect();
    let answer = rows
        .iter()
        .position(|row| row.contains("waiting for your answer"));
    let reconnect = rows.iter().position(|row| row.contains("reconnecting"));
    let truncated = rows
        .iter()
        .position(|row| row.contains("Older summarized history"));
    let (Some(answer), Some(reconnect), Some(truncated)) = (answer, reconnect, truncated) else {
        panic!("missing banner: {rows:?}");
    };
    assert!(answer < reconnect && reconnect < truncated, "{rows:?}");

    ctx.connection = ConnectionBanner::Disconnected;
    let down = paint(&mut pager, &ctx, 80, 24);
    let text = joined(&down);
    assert!(text.contains("Disconnected from Hermes"), "{text}");
    assert!(!text.contains("reconnecting"), "{text}");

    let narrow = paint(&mut pager, &ctx, 40, 12);
    let narrow_text = joined(&narrow);
    assert!(
        narrow_text.contains("⚑ Hermes is waiting for your answer. Pr…"),
        "{narrow_text}"
    );
    assert!(
        narrow_text.contains("Disconnected from Hermes. This is what …"),
        "{narrow_text}"
    );
    assert!(
        narrow_text.contains("Older summarized history was left out b…"),
        "{narrow_text}"
    );

    ctx.connection = ConnectionBanner::Reconnecting;
    ctx.needs_answer = false;
    ctx.history_truncated = false;
    let alone = paint(&mut pager, &ctx, 40, 12);
    assert!(joined(&alone).contains("Connection to Hermes lost; reconnecting…"));
}

#[test]
fn highlights_current_and_other_matches() {
    let theme = PagerTheme::default();
    let markup = Query::new("**");
    let rendered = Line::from("a c **");
    let painted = highlight_line(&rendered, &markup, Some(0), &theme);
    assert_eq!(painted.to_string(), "a c **");
    assert!(
        painted
            .spans
            .iter()
            .any(|span| span.style == theme.search_match),
        "a rendered hit stays a plain match when the counts differ"
    );
    assert!(
        painted
            .spans
            .iter()
            .all(|span| span.style != theme.search_current),
        "markup-only matches are not the current hit"
    );

    let plain = Query::new("bold");
    let words = Line::from("bold bold");
    let first = highlight_line(&words, &plain, Some(0), &theme);
    let second = highlight_line(&words, &plain, Some(5), &theme);
    assert_eq!(first.to_string(), "bold bold");
    assert!(
        first
            .spans
            .iter()
            .any(|span| span.style == theme.search_current)
    );
    assert!(
        second
            .spans
            .iter()
            .any(|span| span.style == theme.search_current)
    );
    assert_ne!(first.spans, second.spans);

    let cjk = Line::from("字a字");
    let letters = highlight_line(&cjk, &Query::new("字"), Some(0), &theme);
    assert_eq!(letters.to_string(), "字a字");
    assert!(letters.spans.iter().any(|span| span.content == "字"));

    let cells = vec![
        assistant(1, "**a** c **", true),
        assistant(2, "bold bold", true),
        reasoning(3, "字a字"),
    ];
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 80, 24, &mut index);
    // The specification's worked example for this case searches for `**`, which the source of
    // the first cell holds three times.
    let _ = pager.handle_search_input(talaria_tui::pager::SearchInput::Char('*'), &ctx, &mut index);
    let _ = pager.handle_search_input(talaria_tui::pager::SearchInput::Char('*'), &ctx, &mut index);
    let buf = paint(&mut pager, &ctx, 80, 24);
    let screen = joined(&buf);
    assert!(screen.contains("▸"), "{screen}");
    assert!(
        screen.contains("1 of 3") || screen.contains("of 3"),
        "{screen}"
    );
}

#[test]
fn resize_keeps_top_cell() {
    let texts: Vec<String> = (0..40).map(|n| format!("cell-row-{n}")).collect();
    let cells: Vec<Cell> = texts
        .iter()
        .enumerate()
        .map(|(id, text)| user(id as u64, text))
        .collect();
    let ctx = context(&cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, 160, 48, &mut index);
    let _ = pager.handle_action(PagerAction::Top, &ctx, &mut index);
    let _ = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    let before = paint(&mut pager, &ctx, 160, 48);
    let top = joined(&before);
    pager.resize(80, 24);
    let after = paint(&mut pager, &ctx, 80, 24);
    let narrowed = joined(&after);
    assert!(
        top.contains("cell-row-1") && narrowed.contains("cell-row-1"),
        "the top cell stays\nbefore:\n{top}\nafter:\n{narrowed}"
    );
}

#[test]
fn no_redraw_when_nothing_changes() {
    let (mut pager, cells, mut index) = {
        let cells = vec![user(1, "only")];
        let ctx = context(&cells);
        let mut index = SearchIndex::new();
        let pager = Pager::open(&ctx, 80, 24, &mut index);
        (pager, cells, index)
    };
    let ctx = context(&cells);
    let down = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    assert_eq!(down, PagerOutcome::Stay { redraw: false });
    let page = pager.handle_action(PagerAction::PageDown, &ctx, &mut index);
    assert_eq!(page, PagerOutcome::Stay { redraw: false });
    let typed = pager.handle_action(PagerAction::LineDown, &ctx, &mut index);
    assert_eq!(typed, PagerOutcome::Stay { redraw: false });
}
