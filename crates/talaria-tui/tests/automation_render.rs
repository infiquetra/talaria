//! Drawing the automation section and command cells. Behavior contract: issue 236.

#[path = "support/color.rs"]
mod color;
mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use serde_json::{Value, json};
use support::{ByteSink, probe};
use talaria_core::automation::{
    AutomationEvent, AutomationState, HermesCommandCell, ModeKey, update,
};
use talaria_tui::automation::{AutomationSection, AutomationTheme, hermes_command_lines};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const NOW: u64 = 1790000000000;
const SESSION: &str = "s1";

fn goal(status: &str) -> Value {
    json!({
        "title": "Write the release notes",
        "status": status,
        "turns_used": 3,
        "max_turns": 20,
        "contract": {"outcome": "", "verification": "", "constraints": "", "boundaries": "", "stop_when": ""},
        "subgoals": ["draft", "review"],
        "gates": [{"command": "make check", "timeout_seconds": 300, "max_retries": 2, "attempts": 1}],
    })
}

fn loop_() -> Value {
    json!({
        "prompt": "check the build",
        "status": "active",
        "mode": "interval",
        "interval_seconds": 2700,
        "current_delay": 0,
        "times": 0,
        "until": "",
        "max_ticks": 0,
        "ticks_fired": 0,
        "created_at": 1790000000,
        "last_fired_at": 0,
        "next_due_at": 1790002700,
        "awaiting_response": false,
        "deferred_by_goal": false
    })
}

fn heartbeat() -> Value {
    json!({
        "prompt": "ping the team",
        "status": "active",
        "interval_seconds": 3600,
        "created_at": 1789997120,
        "last_fired_at": 0,
        "fire_count": 4
    })
}

fn full_snapshot() -> Value {
    json!({
        "goal": goal("active"), "loop": loop_(), "heartbeat": heartbeat(),
        "revision": "r1", "updated_at": 1790000000
    })
}

/// A state holding the full view with YOLO on and fast mode set.
fn full_state() -> AutomationState {
    let mut state = AutomationState::default();
    let effects = update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    let mut tokens = [0u64; 2];
    for effect in &effects {
        if let talaria_core::automation::AutomationEffect::ReadMode { key, token, .. } = effect {
            match key {
                ModeKey::Yolo => tokens[0] = *token,
                ModeKey::Fast => tokens[1] = *token,
            }
        }
    }
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw: full_snapshot(),
        },
    );
    for (key, token, value) in [
        (ModeKey::Yolo, tokens[0], "1"),
        (ModeKey::Fast, tokens[1], "fast"),
    ] {
        update(
            &mut state,
            AutomationEvent::ModeRead {
                session_id: SESSION.to_owned(),
                key,
                token,
                result: Ok(Some(value.to_owned())),
            },
        );
    }
    state
}

fn explicit_theme() -> AutomationTheme {
    AutomationTheme {
        accent: Style::default().fg(Color::Cyan),
        warning: Style::default().fg(Color::Yellow),
        dim: Style::default().add_modifier(Modifier::DIM),
    }
}

/// Draw one section through the real terminal into a vt100 screen.
fn draw_screen(section: &AutomationSection, cols: u16, rows: u16) -> vt100::Parser {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        rows,
        probe(true, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let view = LiveView {
        sections: vec![section as &dyn LiveSection],
    };
    term.draw_live(&view).unwrap();
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&sink.take());
    parser
}

/// The foreground colour and bold flag of the first cell of `needle`.
fn first_cell(parser: &vt100::Parser, needle: &str, rows: u16, cols: u16) -> (vt100::Color, bool) {
    use unicode_width::UnicodeWidthStr;
    for row in 0..rows {
        let text: String = parser
            .screen()
            .rows(0, cols)
            .nth(row as usize)
            .unwrap_or_default()
            .trim_end()
            .to_owned();
        if let Some(index) = text.find(needle) {
            let col = UnicodeWidthStr::width(&text[..index]) as u16;
            let cell = parser
                .screen()
                .cell(row, col)
                .expect("a cell for the match");
            return (cell.fgcolor(), cell.bold());
        }
    }
    panic!("{needle:?} is not on the screen")
}

#[test]
fn section_rows_at_80_columns() {
    if color::run_with_terminal_colors("section_rows_at_80_columns") {
        return;
    }
    let state = full_state();
    let theme = explicit_theme();
    let section = AutomationSection {
        state: &state,
        now_ms: NOW,
        theme: &theme,
    };
    assert_eq!(section.height(80), 4);
    let parser = draw_screen(&section, 80, 24);

    let (yolo_fg, yolo_bold) = first_cell(&parser, "⚠ YOLO", 24, 80);
    assert!(yolo_bold, "the YOLO label is bold");
    assert_ne!(yolo_fg, vt100::Color::Default, "the YOLO label is coloured");
    let (goal_fg, goal_bold) = first_cell(&parser, "⊙ goal", 24, 80);
    assert!(goal_bold, "the goal label is bold");
    assert_ne!(goal_fg, yolo_fg, "warning and accent labels differ");
    let (loop_fg, loop_bold) = first_cell(&parser, "↻ loop", 24, 80);
    assert!(loop_bold);
    assert_eq!(loop_fg, goal_fg, "one accent colour");
    let (beat_fg, beat_bold) = first_cell(&parser, "♥ heartbeat", 24, 80);
    assert!(beat_bold);
    assert_eq!(beat_fg, goal_fg, "one accent colour");

    let (fast_fg, _) = first_cell(&parser, "⚡ fast mode", 24, 80);
    assert_eq!(
        fast_fg,
        vt100::Color::Default,
        "the tail keeps the default colour"
    );
    for tail in ["Write the release", "check the build", "ping the team"] {
        let (fg, _) = first_cell(&parser, tail, 24, 80);
        assert_eq!(
            fg,
            vt100::Color::Default,
            "{tail:?} keeps the default colour"
        );
    }
    let screen: String = (0..24)
        .map(|row| {
            parser
                .screen()
                .rows(0, 80)
                .nth(row as usize)
                .unwrap_or_default()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    for detail in [
        "commands run without approval",
        "3/20 turns",
        "every 45m",
        "every 1h",
    ] {
        assert!(screen.contains(detail), "the screen holds {detail:?}");
    }

    let default = AutomationTheme::default();
    assert_eq!(default.accent.fg, Some(Color::Cyan));
    assert_eq!(default.warning.fg, Some(Color::Yellow));
    assert!(default.dim.add_modifier.contains(Modifier::DIM));
}

#[test]
fn section_is_empty_when_nothing_stands() {
    let theme = explicit_theme();
    let state = AutomationState::default();
    let section = AutomationSection {
        state: &state,
        now_ms: NOW,
        theme: &theme,
    };
    assert_eq!(section.height(80), 0);

    let mut state = AutomationState::default();
    update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw: json!({"goal": null, "loop": null, "heartbeat": null, "revision": "", "updated_at": 0}),
        },
    );
    let section = AutomationSection {
        state: &state,
        now_ms: NOW,
        theme: &theme,
    };
    assert_eq!(section.height(80), 0);
}

#[test]
fn narrow_and_hostile_titles() {
    let mut hostile = goal("paused");
    hostile["title"] = json!("Bad\ntitle\x1b]0;x\x07 here\u{202e}!");
    hostile["paused_reason"] = json!("stopped\nnow\x1b[2J \u{202e}done");
    let mut raw = full_snapshot();
    raw["goal"] = hostile;
    let mut state = AutomationState::default();
    update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw,
        },
    );
    let theme = explicit_theme();

    for width in [30u16, 10u16] {
        let section = AutomationSection {
            state: &state,
            now_ms: NOW,
            theme: &theme,
        };
        let parser = draw_screen(&section, width, 24);
        use unicode_width::UnicodeWidthStr;
        let mut saw_stand_in = false;
        for row in 0..24 {
            let text: String = parser
                .screen()
                .rows(0, width)
                .nth(row as usize)
                .unwrap_or_default()
                .trim_end()
                .to_owned();
            assert!(
                !text.contains('\x1b'),
                "no escape byte reaches the screen at width {width}"
            );
            assert!(
                UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                "row {row} fits width {width}: {text:?}"
            );
            saw_stand_in |= text.contains('␛') || text.contains("‹U+202E›");
        }
        // At width 10 every row is cut inside its label, so no Hermes text is on screen.
        assert!(
            width != 30 || saw_stand_in,
            "stand-ins are visible at width {width}"
        );
    }

    let section = AutomationSection {
        state: &state,
        now_ms: NOW,
        theme: &theme,
    };
    let area = Rect::new(0, 0, 1, 4);
    let mut buf = Buffer::empty(area);
    section.render(area, &mut buf);
    assert_eq!(buf[(0, 0)].symbol(), "…");

    let area = Rect::new(0, 0, 0, 4);
    let mut buf = Buffer::empty(area);
    section.render(area, &mut buf);
}

#[test]
fn hermes_command_cell_markdown() {
    let theme = explicit_theme();
    let on = HermesCommandCell {
        name: "yolo".to_owned(),
        typed: "/yolo".to_owned(),
        body:
            "⚡ YOLO mode **ON** for this session — all commands auto-approved. Use with caution."
                .to_owned(),
        warning: false,
    };
    let lines = hermes_command_lines(&on, 80, &theme);
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!text.contains("**"), "no raw asterisks: {text:?}");
    assert_eq!(
        text.matches("/yolo").count(),
        1,
        "the typed line appears once"
    );
    assert!(
        lines.iter().flat_map(|line| line.spans.iter()).any(|span| {
            span.content.contains("ON") && span.style.add_modifier.contains(Modifier::BOLD)
        }),
        "ON is bold"
    );
    assert!(
        lines[0].spans.iter().any(|span| {
            span.style.add_modifier.contains(Modifier::BOLD)
                && span.style.add_modifier.contains(Modifier::DIM)
        }),
        "the typed line is dim bold"
    );

    let off = HermesCommandCell {
        name: "yolo".to_owned(),
        typed: "/yolo".to_owned(),
        body: "⚠ YOLO mode **OFF** for this session — dangerous commands will require approval."
            .to_owned(),
        warning: true,
    };
    let lines = hermes_command_lines(&off, 80, &theme);
    let first_body = &lines[1];
    let first_char = first_body
        .spans
        .iter()
        .find(|span| !span.content.trim().is_empty())
        .expect("a first body span");
    assert_eq!(first_char.style.fg, Some(Color::Yellow));

    let missing = HermesCommandCell {
        name: "loop".to_owned(),
        typed: "/loop 45m".to_owned(),
        body: "/loop: missing prompt (usage: /loop [interval] \\<prompt\\>)".to_owned(),
        warning: false,
    };
    let lines = hermes_command_lines(&missing, 80, &theme);
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("<prompt>"), "placeholders stay visible");
    assert!(!text.contains("\\<"), "no raw escape: {text:?}");

    let hostile = HermesCommandCell {
        name: "yolo".to_owned(),
        typed: "/yolo".to_owned(),
        body: "before \x1b[2J after".to_owned(),
        warning: false,
    };
    let lines = hermes_command_lines(&hostile, 80, &theme);
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!text.contains('\x1b'), "no escape byte in the output");
    assert!(text.contains('␛'), "the stand-in shows");

    let _ = hermes_command_lines(&on, 2, &theme);
}
