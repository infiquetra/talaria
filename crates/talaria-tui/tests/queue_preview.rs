//! Issue 232. Queue and steer. The queue section on a 30-row screen.
//!
//! Widths are 60, 100 and 160. Nothing draws the section until the preview
//! functions return lines.

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use talaria_core::queue::{AdmissionId, Controls, QueueView, QueuedRow, RowStatus, SubmissionId};
use talaria_tui::history::sanitize;
use talaria_tui::queue::{composer_title, render_indicator, render_preview};
use vt100::Callbacks;

struct TitleGrab(String);

impl Callbacks for TitleGrab {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.0 = String::from_utf8_lossy(title).into_owned();
    }
}

fn row(id: &str, text: &str, seq: i64) -> QueuedRow {
    QueuedRow {
        admission_id: AdmissionId(id.to_string()),
        input_id: SubmissionId(format!("in-{id}")),
        text: text.to_string(),
        status: RowStatus::Queued,
        outcome: None,
        controls: Controls {
            steer: true,
            replace: true,
        },
        steering: false,
        sequence: seq,
        execution_generation: Some(1),
    }
}

fn view(rows: Vec<QueuedRow>, selected: Option<usize>, paused: bool, running: bool) -> QueueView {
    QueueView {
        rows,
        steering: Vec::new(),
        selected,
        paused,
        running,
        editing: None,
        sending: 0,
    }
}

fn span_text(span: &Span<'_>) -> String {
    span.content.to_string()
}

fn lines_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| line.spans.iter().map(span_text).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw(lines: &[Line<'_>], cols: u16) -> vt100::Parser<TitleGrab> {
    let mut parser = vt100::Parser::new_with_callbacks(30, cols, 0, TitleGrab(String::new()));
    let mut bytes = Vec::new();
    for line in lines {
        for span in &line.spans {
            if span.style.add_modifier.contains(Modifier::DIM) {
                bytes.extend_from_slice(b"\x1b[2m");
            }
            bytes.extend_from_slice(span.content.to_string().as_bytes());
            if span.style.add_modifier.contains(Modifier::DIM) {
                bytes.extend_from_slice(b"\x1b[0m");
            }
        }
        bytes.extend_from_slice(b"\r\n");
    }
    parser.process(&bytes);
    parser
}

fn alpha_bravo() -> Vec<QueuedRow> {
    vec![
        row("adm-a", "Reply with the word alpha in capital letters.", 1),
        row("adm-b", "Reply with the word bravo in capital letters.", 2),
        row(
            "adm-c",
            "Reply with the word charlie in capital letters.\nsecond line\nthird line",
            3,
        ),
    ]
}

#[test]
fn indicator_at_60_100_160_columns() {
    for width in [60_u16, 100, 160] {
        let span = render_indicator(2, false, width);
        let text = span_text(&span);
        assert!(text.contains("queued (2)"), "width {width}");
        assert!(text.trim_end().ends_with("queued (2)"), "width {width}");
        let quiet = render_indicator(0, false, width);
        assert!(
            !span_text(&quiet).contains("queued"),
            "no label when nothing is counted"
        );
    }
}

#[test]
fn indicator_idle_on_preview_line() {
    let rows = alpha_bravo();
    for width in [60_u16, 100, 160] {
        let hinted = render_preview(&view(rows.clone(), None, false, false), width, true);
        let text = lines_text(&hinted);
        assert!(text.contains("queued (3)"));
        assert!(text.contains(" · ↑ select"), "width {width}");
        let quiet = render_preview(&view(rows.clone(), None, false, false), width, false);
        assert!(!lines_text(&quiet).contains(" · ↑ select"));
    }
}

#[test]
fn indicator_paused_and_short() {
    let wide = render_indicator(2, true, 60);
    assert!(span_text(&wide).contains("queued (2) · paused"));
    let narrow = render_indicator(2, true, 40);
    assert!(span_text(&narrow).contains("q(2)!"));
    let running = render_indicator(2, false, 40);
    assert!(span_text(&running).contains("q(2)"));
    assert!(!span_text(&running).contains('!'));
}

#[test]
fn preview_three_rows_and_more() {
    let lines = render_preview(&view(alpha_bravo(), None, false, true), 100, true);
    assert!(lines.len() <= 5);
    let text = lines_text(&lines);
    assert!(text.contains("Reply with the word alpha in capital letters."));
    assert!(text.contains("(+2 lines)"));
    assert!(text.contains("+") && text.contains("more") || text.contains("(+"));
}

#[test]
fn preview_is_dimmed() {
    let lines = render_preview(&view(alpha_bravo(), None, false, true), 100, false);
    let parser = draw(&lines, 100);
    let screen = parser.screen();
    let cell = screen.cell(0, 0).expect("cell");
    assert!(cell.dim(), "preview rows are dimmed");
}

#[test]
fn steering_line_newest_and_count() {
    let mut shown = view(alpha_bravo(), None, false, true);
    shown.steering = vec![
        row("adm-s1", "older steer", 1),
        row("adm-s2", "The code word is PELICAN.", 2),
    ];
    let text = lines_text(&render_preview(&shown, 100, true));
    assert!(text.contains("↳ steering into the running turn: The code word is PELICAN."));
    assert!(text.contains("(+1 more)"));
}

#[test]
fn selection_window_scrolls() {
    let rows: Vec<_> = (0..12)
        .map(|index| row(&format!("adm-{index}"), &format!("row {index} text"), index))
        .collect();
    let lines = render_preview(&view(rows, Some(10), false, false), 100, false);
    assert!(lines.len() <= 10);
    let text = lines_text(&lines);
    assert!(text.contains("› "));
    assert!(text.contains("above"));
    assert!(text.contains("below"));
}

#[test]
fn hint_line_shortens() {
    let rows = alpha_bravo();
    let wide = lines_text(&render_preview(
        &view(rows.clone(), Some(0), false, false),
        100,
        false,
    ));
    assert!(wide.contains(
        "enter edit · d remove · s steer · n send now (stops the turn) · K/J move · esc back"
    ));
    let mid = lines_text(&render_preview(
        &view(rows.clone(), Some(0), false, false),
        80,
        false,
    ));
    assert!(mid.contains("enter edit · d remove · s steer · n now · K/J move · esc"));
    let narrow = lines_text(&render_preview(
        &view(rows, Some(0), false, false),
        40,
        false,
    ));
    assert!(narrow.contains("⏎ e · d · s · n · K/J · esc"));
}

#[test]
fn lost_row_label() {
    let mut lost = row("adm-lost", "lost prompt", 1);
    lost.status = RowStatus::Unknown;
    let text = lines_text(&render_preview(
        &view(vec![lost], None, true, false),
        100,
        false,
    ));
    assert!(text.contains("⚠ lost in a gateway restart · d to discard"));
}

#[test]
fn edit_mode_composer_title() {
    let mut editing = view(alpha_bravo(), None, false, true);
    editing.editing = Some((1, 2));
    assert_eq!(
        composer_title(&editing).as_deref(),
        Some("editing queued prompt 1 of 2 · enter save · esc cancel")
    );
    let mut sending = view(Vec::new(), None, false, true);
    sending.sending = 1;
    assert_eq!(composer_title(&sending).as_deref(), Some("Sending…"));
}

#[test]
fn untrusted_text_is_literal() {
    let mut parser = vt100::Parser::new_with_callbacks(30, 80, 0, TitleGrab(String::new()));
    parser.process(b"\x1b]2;queued\x07");
    assert_eq!(parser.callbacks().0, "queued");
    let hostile = "hello\u{1b}[2J\u{202e}there".to_string();
    let clean = sanitize(&hostile);
    let lines = render_preview(
        &view(vec![row("adm-h", &hostile, 1)], None, false, false),
        80,
        false,
    );
    let rendered = lines_text(&lines);
    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\u{202e}'));
    assert!(rendered.contains(clean.as_ref()));
}

#[test]
fn live_region_height_bounded() {
    let rows: Vec<_> = (0..12)
        .map(|index| {
            row(
                &format!("adm-{index}"),
                &format!("row {index}\nextra"),
                index,
            )
        })
        .collect();
    for width in [60_u16, 100, 160] {
        let preview = render_preview(&view(rows.clone(), None, false, true), width, true);
        assert!(preview.len() <= 5, "preview width {width}");
        let selection = render_preview(&view(rows.clone(), Some(8), false, true), width, false);
        assert!(selection.len() <= 10, "selection width {width}");
        let parser = draw(&selection, width);
        for row_index in 0..selection.len() {
            let row = u16::try_from(row_index).unwrap_or(u16::MAX);
            assert!(!parser.screen().row_wrapped(row), "width {width} wraps");
        }
    }
}
