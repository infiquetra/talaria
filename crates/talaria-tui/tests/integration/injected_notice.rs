//! A notice that carries what Hermes handed the agent: one line and a line count on the main
//! screen, the whole text in the transcript view, where search finds it too.

use crate::pager_support as support;

use ratatui::text::Line;
use talaria_core::session::{Cell, CellId, NoticeCell, NoticeKind};
use talaria_tui::history::{CellLines, PlainCellLines};
use talaria_tui::pager::{Details, Query, SearchIndex, cell_search_text, find_all};
use talaria_tui::theme::cells::ThemedCells;
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
    ThemeInputs, resolve,
};

const LABEL: &str = "Subagent Tasks Completed: write the parser (1 task)";
const BODY: &str = "[ASYNC DELEGATION BATCH COMPLETE — deleg_0001]\nTask 1: write the parser\nResult: {\"status\": \"ok\", \"marker\": \"zebra\"}";

fn injected(id: u64) -> Cell {
    Cell::Notice(NoticeCell {
        id: CellId::new(id),
        kind: NoticeKind::Automation,
        text: LABEL.to_string(),
        detail: None,
        body: Some(BODY.to_string()),
    })
}

fn rows(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn notice_with_body_shows_one_line_and_line_count() {
    let cell = injected(1);
    let want = vec![format!("· {LABEL} (+3 lines in transcript view)")];
    assert_eq!(rows(&PlainCellLines.lines(&cell, 200)), want);
    let theme = resolve(&ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: false,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    });
    let unbound = ThemedCells {
        theme: &theme,
        transcript_key: None,
    };
    assert_eq!(rows(&unbound.lines(&cell, 200)), want);
    // With the transcript key bound, the hint names it, as a cut tool preview does.
    let bound = ThemedCells {
        theme: &theme,
        transcript_key: Some("Ctrl+O"),
    };
    assert_eq!(
        rows(&bound.lines(&cell, 200)),
        [format!("· {LABEL} (+3 lines, Ctrl+O to view)")]
    );
    for row in rows(&PlainCellLines.lines(&cell, 40)) {
        assert!(!row.starts_with("> "), "{row}");
        assert!(!row.contains("deleg_0001"), "{row}");
        assert!(!row.contains("zebra"), "{row}");
    }
}

#[test]
fn notice_with_one_body_line_says_line() {
    let cell = Cell::Notice(NoticeCell {
        id: CellId::new(1),
        kind: NoticeKind::Automation,
        text: "Background process finished".to_string(),
        detail: None,
        body: Some("exit 0".to_string()),
    });
    assert_eq!(
        rows(&PlainCellLines.lines(&cell, 200)),
        ["· Background process finished (+1 line in transcript view)"]
    );
}

#[test]
fn a_body_that_repeats_the_notice_shows_nowhere() {
    let cell = Cell::Notice(NoticeCell {
        id: CellId::new(1),
        kind: NoticeKind::Automation,
        text: "Ran the check".to_string(),
        detail: None,
        body: Some("Ran the check\n".to_string()),
    });
    assert_eq!(rows(&PlainCellLines.lines(&cell, 200)), ["· Ran the check"]);
    let plain = PlainCellLines;
    let expanded = rows(&support::lines_for(Details::Expanded, 120, &plain).lines(&cell, 120));
    assert_eq!(expanded, ["! Ran the check"]);
}

#[test]
fn notice_body_shows_when_expanded_and_counts_when_compact() {
    let plain = PlainCellLines;
    let cell = injected(1);
    let expanded = rows(&support::lines_for(Details::Expanded, 120, &plain).lines(&cell, 120));
    assert_eq!(
        expanded,
        [
            format!("! {LABEL}"),
            "  [ASYNC DELEGATION BATCH COMPLETE — deleg_0001]".to_string(),
            "  Task 1: write the parser".to_string(),
            "  Result: {\"status\": \"ok\", \"marker\": \"zebra\"}".to_string(),
        ]
    );
    let compact = rows(&support::lines_for(Details::Compact, 120, &plain).lines(&cell, 120));
    assert_eq!(compact, [format!("! {LABEL}"), " (+3 lines)".to_string()]);
}

#[test]
fn search_finds_text_inside_a_notice_body() {
    let cells = vec![support::user(1, "start"), injected(2)];
    assert_eq!(
        cell_search_text(&cells[1], &PlainCellLines),
        format!("{LABEL}\n{BODY}")
    );
    let mut index = SearchIndex::new();
    let found = find_all(&cells, 1, &mut index, &Query::new("zebra"), &PlainCellLines);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].cell_index, 1);
}
