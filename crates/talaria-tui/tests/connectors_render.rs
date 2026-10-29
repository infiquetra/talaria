//! Connectors overlay drawing at two sizes.

#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::connectors::{
    ConnectorsState, ConnectorsTab, ConnectorsView, FieldRow, FieldValue, Line, Row, Screen,
    ToolRow, ToolState, next_deadline,
};
use talaria_core::settings::ReviewRow;
use talaria_tui::connectors::draw_connectors;

fn view(screen: Screen) -> ConnectorsView {
    ConnectorsView {
        title: "Connectors and messaging · profile fixture".into(),
        tab: ConnectorsTab::Connectors,
        screen,
        notice: None,
        footer: vec![("esc".into(), "close".into())],
        review_scroll: 0,
    }
}

fn line(text: &str) -> Line {
    Line {
        text: text.into(),
        tone: talaria_core::connectors::Tone::Normal,
    }
}

fn row(label: &str, state: &str) -> Row {
    Row {
        label: label.into(),
        state: state.into(),
        marker: None,
    }
}

fn paint(screen: Screen, cols: u16, rows: u16) -> vt100::Parser {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    draw_connectors(&view(screen), &BTreeMap::new(), area, &mut buf);
    let mut parser = vt100::Parser::new(rows, cols, 0);
    let mut bytes = Vec::new();
    for y in 0..rows {
        for x in 0..cols {
            bytes.extend(buf[(x, y)].symbol().as_bytes());
        }
        if y + 1 < rows {
            bytes.extend(b"\r\n");
        }
    }
    parser.process(&bytes);
    parser
}

fn screen_text(parser: &vt100::Parser, cols: u16) -> String {
    parser
        .screen()
        .contents()
        .lines()
        .take(80)
        .collect::<Vec<_>>()
        .join("\n")
        + &format!(" cols {cols}")
}

#[test]
fn review_scrolls_to_the_last_change_and_warning() {
    let rows = (0..12)
        .map(|n| ReviewRow {
            key: format!("tool-{n:02}"),
            old: "off".into(),
            new: "on".into(),
            effect_label: String::new(),
            warnings: vec![format!("warning-{n:02}")],
        })
        .collect();
    let mut view = view(Screen::Review {
        title: "Review tools".into(),
        rows,
    });
    view.review_scroll = 100;
    view.footer = vec![("y".into(), "confirm".into()), ("n".into(), "back".into())];
    let area = Rect::new(0, 0, 60, 12);
    let mut buf = Buffer::empty(area);
    draw_connectors(&view, &BTreeMap::new(), area, &mut buf);
    let screen = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("tool-11"), "{screen}");
    assert!(screen.contains("warning-11"), "{screen}");
    assert!(screen.contains("y confirm"), "{screen}");
    let up = talaria_tui::connectors::review_scroll_target(
        &view,
        talaria_core::connectors::ConnectorsKey::Up,
        area,
    )
    .unwrap();
    let down = talaria_tui::connectors::review_scroll_target(
        &view,
        talaria_core::connectors::ConnectorsKey::Down,
        area,
    )
    .unwrap();
    assert_eq!(up + 1, down, "up moves immediately from the visible bottom");
}

#[test]
fn tabs_snapshots() {
    for (cols, rows) in [(160, 48), (80, 24)] {
        let consent = paint(
            Screen::Consent {
                text: "Hermes creates a free guest identity for it when you continue.".into(),
            },
            cols,
            rows,
        );
        assert!(screen_text(&consent, cols).contains("guest identity"));
        let loading = paint(Screen::Loading, cols, rows);
        assert!(screen_text(&loading, cols).contains("Loading"));
        let unavailable = paint(
            Screen::Unavailable {
                text: "Connectors are not available for profile fixture.".into(),
                command: Some("hermes -p fixture auth upgrade".into()),
            },
            cols,
            rows,
        );
        assert!(screen_text(&unavailable, cols).contains("auth upgrade"));
        let list = paint(
            Screen::ConnectorList {
                rows: vec![row("GitHub", "connected")],
                selected: 0,
                filter: None,
            },
            cols,
            rows,
        );
        assert!(screen_text(&list, cols).contains("GitHub"));
        let empty = paint(
            Screen::ConnectorList {
                rows: Vec::new(),
                selected: 0,
                filter: None,
            },
            cols,
            rows,
        );
        assert!(screen_text(&empty, cols).contains("fixture"));
        let messaging = paint(
            Screen::PlatformList {
                rows: vec![row("Signal", "needs setup")],
                selected: 0,
                restart_line: true,
                other_pairing: Vec::new(),
            },
            cols,
            rows,
        );
        assert!(
            screen_text(&messaging, cols).contains("R opens the doctor")
                || messaging.screen().contents().contains("restart")
        );
    }
}

#[test]
fn cards_snapshots() {
    for (cols, rows) in [(160, 48), (80, 24)] {
        let card = paint(
            Screen::ConnectorCard {
                title: "GitHub".into(),
                lines: vec![line("connected")],
                accounts: vec![row("work", "active")],
                retired: 0,
                selected: 0,
            },
            cols,
            rows,
        );
        assert!(screen_text(&card, cols).contains("work"));
        let link = paint(
            Screen::LinkCard {
                app: "GitHub".into(),
                link: Some("https://example.invalid/connect/test".into()),
                left: Some("02:00 left".into()),
                ssh: true,
                starting: false,
            },
            cols,
            rows,
        );
        assert!(
            screen_text(&link, cols).contains("remote shell")
                || link.screen().contents().contains("https://example.invalid")
        );
        let tools = paint(
            Screen::Tools {
                header: "GitHub tools".into(),
                cached: true,
                rows: vec![ToolRow {
                    name: "Issues".into(),
                    facet: "read".into(),
                    state: ToolState::Off,
                    draft: false,
                    deprecated: false,
                }],
                selected: 0,
            },
            cols,
            rows,
        );
        assert!(screen_text(&tools, cols).contains("Issues"));
        let review = paint(
            Screen::Review {
                title: "Review".into(),
                rows: vec![ReviewRow {
                    key: "SIGNAL_HTTP_URL".into(),
                    old: "not set".into(),
                    new: "new value (hidden)".into(),
                    effect_label: String::new(),
                    warnings: Vec::new(),
                }],
            },
            cols,
            rows,
        );
        assert!(screen_text(&review, cols).contains("hidden"));
        let confirm = paint(
            Screen::Confirm {
                text: "Approve pairing?".into(),
            },
            cols,
            rows,
        );
        assert!(screen_text(&confirm, cols).contains("Approve"));
        let editor = paint(
            Screen::SecretEditor {
                key: "SIGNAL_HTTP_URL".into(),
                title: "Bridge address".into(),
                help: None,
            },
            cols,
            rows,
        );
        assert!(screen_text(&editor, cols).contains("SIGNAL_HTTP_URL"));
        let list = paint(
            Screen::Prompt {
                title: "Allowed users".into(),
                text: "alpha, beta".into(),
                help: Some("Comma or newline separates entries.".into()),
                problem: None,
            },
            cols,
            rows,
        );
        assert!(screen_text(&list, cols).contains("alpha"));
        let _fields = FieldRow {
            key: "SIGNAL_HTTP_URL".into(),
            prompt: "Bridge address".into(),
            required: true,
            value: FieldValue::NotSet,
        };
    }
}

#[test]
fn opens_without_waiting_and_idles() {
    let started = Instant::now();
    let painted = paint(Screen::Loading, 80, 24);
    assert!(started.elapsed() < Duration::from_millis(50));
    let _ = painted;
    let deadline = next_deadline(&ConnectorsState::new());
    assert!(deadline.is_none());
    std::thread::sleep(Duration::from_secs(2));
    let again = paint(Screen::Loading, 80, 24);
    assert_eq!(again.screen().contents(), painted.screen().contents());
}
