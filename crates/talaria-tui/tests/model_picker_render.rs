//! Issue 230, scenarios for the model overlay and the confirmation card.

mod support;

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::{Map, Value, json};
use support::{ByteSink, Vt, probe};
use talaria_core::models::{PickerRow, PickerView, RowKey};
use talaria_core::mutation::Confirmation;
use talaria_tui::live::LiveSection;
use talaria_tui::model_picker::{ModelConfirmSection, draw_model_picker};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const TITLE: &str = "Switch model · this session";
const FILTER: &str = "glm";
const EFFORT: &str = "effort: high → xhigh";
const HINTS: &str = "enter switch · ctrl+g scope · ctrl+f ★ · ctrl+r refresh · esc";
const HEADER: &str = "Ollama Cloud · current · session 42% · weekly 12%";
const PRICE: &str = "$0.60 in · $2.20 out";
const ANTHROPIC: &str = "Anthropic · needs ANTHROPIC_API_KEY · set up with hermes model";
const LONG_MODEL: &str = "  glm-5-extra-long-name-that-overflows-the-narrow-overlay";
const T16: &str = "Could not load models: timed out. Press ctrl+r to try again.";
const T17: &str = "No models found. Set up a provider with hermes model, then press ctrl+r.";
const T18: &str = "No model matches \"zzz\".";
const T30: &str =
    "refreshing all providers can take a few seconds; other actions wait until it finishes";
const LOADING: &str = "Loading models…";

fn row(
    text: &str,
    key: Option<RowKey>,
    badges: &[&str],
    price: Option<&str>,
    header: bool,
) -> PickerRow {
    PickerRow {
        key,
        text: text.to_string(),
        badges: badges.iter().map(|badge| (*badge).to_string()).collect(),
        price: price.map(str::to_string),
        dim: false,
        header,
    }
}

fn picture_rows() -> Vec<PickerRow> {
    vec![
        row("Favourites", None, &[], None, true),
        row(
            "★ Ollama Cloud / minimax-m3.1-flash",
            Some(RowKey::Favourite {
                provider: "ollama-cloud".to_string(),
                model: "minimax-m3.1-flash".to_string(),
            }),
            &[],
            Some("free"),
            false,
        ),
        row(HEADER, None, &[], None, true),
        row(
            "● minimax-m3.1-flash",
            Some(RowKey::Model {
                provider: "ollama-cloud".to_string(),
                model: "minimax-m3.1-flash".to_string(),
            }),
            &[],
            Some("free"),
            false,
        ),
        row(
            LONG_MODEL,
            Some(RowKey::Model {
                provider: "ollama-cloud".to_string(),
                model: "glm-5".to_string(),
            }),
            &["fast"],
            Some(PRICE),
            false,
        ),
        row("Not set up (2)", None, &[], None, true),
        row(
            ANTHROPIC,
            Some(RowKey::NotSetUp {
                provider: "anthropic".to_string(),
            }),
            &[],
            None,
            false,
        ),
    ]
}

fn view<'a>(rows: &'a [PickerRow], banner: Option<&'a str>, refreshing: bool) -> PickerView<'a> {
    PickerView {
        title: if refreshing {
            "Switch model · this session · refreshing…"
        } else {
            TITLE
        },
        filter: FILTER,
        effort: EFFORT,
        hints: if refreshing { T30 } else { HINTS },
        refreshing,
        rows,
        selected: Some(4),
        banner,
    }
}

fn open(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let _ = sink.take();
    (term, sink, Vt::new(cols, rows))
}

fn paint(term: &mut InlineTerminal<ByteSink>, sink: &ByteSink, vt: &mut Vt, view: &PickerView) {
    term.draw_overlay(|area, buf| draw_model_picker(view, area, buf))
        .expect("draw overlay");
    vt.absorb(sink);
}

fn joined(rows: &[String]) -> String {
    rows.join("\n")
}

fn assert_in_order(rows: &[String], phrases: &[&str]) {
    let mut start = 0;
    for phrase in phrases {
        let found = rows
            .iter()
            .skip(start)
            .position(|row| row.contains(phrase))
            .unwrap_or_else(|| panic!("missing {phrase} in {rows:?}"));
        start += found + 1;
    }
}

/// Issue 230, scenario "layout at 100 columns".
#[test]
fn layout_at_100_columns() {
    let rows = picture_rows();
    let view = view(&rows, None, false);
    let (mut term, sink, mut vt) = open(100, 30);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    paint(&mut term, &sink, &mut vt, &view);
    let screen = vt.rows();
    assert_in_order(
        &screen,
        &[
            TITLE,
            "> glm",
            "★ Ollama Cloud / minimax-m3.1-flash",
            HEADER,
            "● minimax-m3.1-flash",
            "glm-5",
            "Not set up (2)",
            ANTHROPIC,
        ],
    );
    let glm = screen
        .iter()
        .find(|row| row.contains("glm-5"))
        .expect("glm row");
    assert!(glm.contains("fast"), "badges stay at 100 columns: {glm}");
    assert!(glm.contains(PRICE), "the price stays at 100 columns: {glm}");
    assert!(
        screen.iter().any(|row| row.contains("free")),
        "the favourite price is on screen: {screen:?}"
    );
    let footer = screen
        .iter()
        .find(|row| row.contains(EFFORT))
        .expect("effort footer");
    assert!(
        footer.contains("enter switch"),
        "the footer names the switch key: {footer}"
    );
}

/// Issue 230, scenario "narrow widths".
#[test]
fn narrow_widths() {
    let rows = picture_rows();
    let view = view(&rows, None, false);

    let (mut term, sink, mut vt) = open(60, 30);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    paint(&mut term, &sink, &mut vt, &view);
    let screen = joined(&vt.rows());
    assert!(!screen.contains(PRICE), "60 columns drop prices: {screen}");
    assert!(screen.contains("fast"), "60 columns keep badges: {screen}");

    let (mut term, sink, mut vt) = open(40, 30);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    paint(&mut term, &sink, &mut vt, &view);
    let screen = joined(&vt.rows());
    assert!(!screen.contains("fast"), "40 columns drop badges: {screen}");
    assert!(!screen.contains(PRICE), "40 columns drop prices: {screen}");
    assert!(
        screen.contains('…'),
        "narrow text is cut with an ellipsis: {screen}"
    );
}

/// Issue 230, scenario "states: loading, failed, empty, refreshing".
#[test]
fn states_loading_failed_empty_refreshing() {
    let loading_rows = vec![row(LOADING, None, &[], None, false)];
    let (mut term, sink, mut vt) = open(100, 30);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    paint(&mut term, &sink, &mut vt, &view(&loading_rows, None, false));
    assert!(joined(&vt.rows()).contains(LOADING));

    let failed = vec![row(T16, None, &[], None, false)];
    paint(&mut term, &sink, &mut vt, &view(&failed, Some(T16), false));
    assert!(joined(&vt.rows()).contains(T16));

    let empty = vec![row(T17, None, &[], None, false)];
    paint(&mut term, &sink, &mut vt, &view(&empty, Some(T17), false));
    assert!(joined(&vt.rows()).contains(T17));

    let missed = vec![row(T18, None, &[], None, false)];
    paint(&mut term, &sink, &mut vt, &view(&missed, Some(T18), false));
    assert!(joined(&vt.rows()).contains(T18));

    let cached = picture_rows();
    paint(&mut term, &sink, &mut vt, &view(&cached, None, true));
    let screen = joined(&vt.rows());
    assert!(
        screen.contains("refreshing…"),
        "the title names the reload: {screen}"
    );
    assert!(screen.contains(T30), "the footer names the wait: {screen}");
    assert!(
        !screen.contains("enter switch"),
        "refreshing replaces the key hints: {screen}"
    );
}

/// Issue 230, scenario "hostile strings are sanitised".
#[test]
fn hostile_strings_are_sanitised() {
    let hostile = "\u{1b}]0;x\u{7}\u{202e}glm".to_string();
    let rows = vec![row(
        &hostile,
        Some(RowKey::Model {
            provider: "ollama-cloud".to_string(),
            model: "glm".to_string(),
        }),
        &[],
        None,
        false,
    )];
    let (mut term, sink, mut vt) = open(80, 24);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    paint(&mut term, &sink, &mut vt, &view(&rows, None, false));
    let screen = joined(&vt.rows());
    assert!(
        screen.contains('\u{241b}'),
        "escape is a stand-in on screen: {screen:?}"
    );
    assert!(
        screen.contains('\u{2407}'),
        "the bell is a stand-in on screen: {screen:?}"
    );
    assert!(
        screen.contains("\u{2039}U+202E\u{203a}"),
        "the direction mark is a stand-in: {screen:?}"
    );
    assert!(
        !screen
            .chars()
            .any(|ch| ch == '\u{1b}' || ch == '\u{202e}' || ch == '\u{7}'),
        "the screen rows do not contain the raw controls: {screen:?}"
    );
}

fn confirmation(message: &str) -> Confirmation {
    let pick: Map<String, Value> = json!({"model": "glm-5", "provider": "ollama-cloud"})
        .as_object()
        .expect("object")
        .clone();
    Confirmation {
        token: "tok-1".to_string(),
        message: message.to_string(),
        target_model: "glm-5".to_string(),
        target_provider: Some("ollama-cloud".to_string()),
        pick,
    }
}

fn card_lines(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .map(|row| row.trim_end().to_string())
        .filter(|row| !row.is_empty())
        .collect()
}

/// Issue 230, scenario "confirmation card".
#[test]
fn confirmation_card() {
    let message = "word ".repeat(60);
    assert_eq!(message.chars().count(), 300);
    let card = confirmation(&message);
    let section = ModelConfirmSection::new(&card, "minimax-m3.1-flash");
    let area = Rect::new(0, 0, 60, 16);
    let mut buf = Buffer::empty(area);
    section.render(area, &mut buf);
    let lines = card_lines(&buf);
    assert!(
        lines.iter().any(|line| line.contains("Switch to glm-5?")),
        "title: {lines:?}"
    );
    let wrapped = lines.iter().filter(|line| line.contains("word")).count();
    assert!(wrapped > 1, "a 300-character message wraps: {lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.contains("enter switch anyway · esc keep minimax-m3.1-flash")),
        "keys: {lines:?}"
    );

    let waiting = ModelConfirmSection::new(&card, "minimax-m3.1-flash").waiting_on_draft();
    let mut held = Buffer::empty(area);
    waiting.render(area, &mut held);
    let lines = card_lines(&held);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("clear the draft to answer")),
        "draft keys: {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("enter switch anyway")),
        "a draft does not offer the switch keys: {lines:?}"
    );
}

/// Issue 230, scenario "no frames when idle".
#[test]
fn no_frames_when_idle() {
    let rows = picture_rows();
    let view = view(&rows, None, false);
    let (mut term, sink, _vt) = open(100, 30);
    term.open_overlay().expect("open overlay");
    let _ = sink.take();
    term.draw_overlay(|area, buf| draw_model_picker(&view, area, buf))
        .expect("first draw");
    let _ = sink.take();
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        sink.is_empty(),
        "an idle picker writes nothing while it waits"
    );
    let again = term
        .draw_overlay(|area, buf| draw_model_picker(&view, area, buf))
        .expect("second draw");
    assert!(!again.drew);
    assert_eq!(again.bytes, 0);
    assert!(
        sink.is_empty(),
        "the second draw of the same view writes nothing"
    );
}
