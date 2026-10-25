//! Shell and side cells and the side-tasks section on screen.

#[path = "../../talaria-core/tests/support/events.rs"]
mod events;
mod markdown_support;

use events::{SESSION, ev, submit};
use markdown_support::to_ansi;
use ratatui::style::Modifier;
use serde_json::json;
use talaria_core::session::{Cell, SessionId, SessionState, update};
use talaria_core::side::{
    Origin, PendingKind, PendingTask, RequestId, ShellCell, ShellOutcome, SideCell, SideOutcome,
    SideTasks,
};
use talaria_tui::history::CommitLedger;
use talaria_tui::live::LiveSection;
use talaria_tui::side::{
    SideTasksSection, SideTheme, shell_cell_lines, shell_mode_title, side_cell_lines,
};

fn theme() -> SideTheme {
    SideTheme::default()
}

fn shell(command: &str, outcome: ShellOutcome, duration_ms: u64) -> ShellCell {
    ShellCell {
        command: command.to_string(),
        outcome,
        duration_ms,
    }
}

fn side(question: &str, outcome: SideOutcome, origin: Origin, late: bool) -> SideCell {
    SideCell {
        question: question.to_string(),
        outcome,
        origin,
        late,
    }
}

fn screen_text(screen: &vt100::Screen) -> String {
    screen.contents()
}

#[test]
fn shell_cell_layout_at_80_columns() {
    let cell = shell(
        "echo oops >&2; exit 3",
        ShellOutcome::Ok {
            stdout: String::new(),
            stderr: "oops\n".to_string(),
            code: 3,
        },
        200,
    );
    let lines = shell_cell_lines(&cell, 80, &theme());
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&to_ansi(&lines));
    let screen = parser.screen();
    let text = screen_text(screen);
    assert!(text.contains("! echo oops >&2; exit 3"), "{text}");
    let edge: String = (76..80)
        .map(|col| screen.cell(0, col).expect("row 0").contents())
        .collect();
    assert_eq!(edge, "0.2s", "the duration sits at the right edge");
    let oops = screen.cell(1, 2).expect("the stderr row");
    assert_eq!(oops.fgcolor(), vt100::Color::Idx(3));
    let exit = screen.cell(2, 2).expect("the exit row");
    assert_eq!(exit.fgcolor(), vt100::Color::Idx(1));
}

#[test]
fn shell_cell_no_output_and_trim_lines() {
    for stdout in ["", "  \n"] {
        let cell = shell(
            "true",
            ShellOutcome::Ok {
                stdout: stdout.to_string(),
                stderr: String::new(),
                code: 0,
            },
            5,
        );
        let lines = shell_cell_lines(&cell, 80, &theme());
        let text: String = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("(no output)"), "{text}");
    }
    let cell = shell(
        "killable",
        ShellOutcome::Ok {
            stdout: String::new(),
            stderr: String::new(),
            code: -9,
        },
        5,
    );
    let lines = shell_cell_lines(&cell, 80, &theme());
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("exit -9 (stopped by signal 9)"), "{text}");
    for (len, due) in [(4_000, true), (3_999, false)] {
        let cell = shell(
            "long",
            ShellOutcome::Ok {
                stdout: "x".repeat(len),
                stderr: String::new(),
                code: 0,
            },
            5,
        );
        let lines = shell_cell_lines(&cell, 80, &theme());
        let text: String = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            text.contains("Hermes keeps at most the last 4,000 characters"),
            due,
            "{len} code points"
        );
    }
}

#[test]
fn shell_header_wraps_and_counts_lines() {
    let long = "x".repeat(60);
    let cell = shell(
        &long,
        ShellOutcome::Ok {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        },
        1_200,
    );
    let lines = shell_cell_lines(&cell, 30, &theme());
    assert!(lines.len() >= 2);
    assert!(
        lines[1]
            .spans
            .iter()
            .any(|span| span.content.starts_with("  "))
    );
    for (command, marker) in [("one\ntwo", "(+1 line)"), ("one\ntwo\nthree", "(+2 lines)")] {
        let cell = shell(
            command,
            ShellOutcome::Ok {
                stdout: String::new(),
                stderr: String::new(),
                code: 0,
            },
            1,
        );
        let lines = shell_cell_lines(&cell, 80, &theme());
        let head: String = lines[0]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(head.contains(marker), "{head}");
    }
}

#[derive(Default)]
struct TitleCalls {
    calls: Vec<Vec<u8>>,
}

impl vt100::Callbacks for TitleCalls {
    fn set_window_title(&mut self, _screen: &mut vt100::Screen, title: &[u8]) {
        self.calls.push(title.to_vec());
    }
}

#[test]
fn hostile_output_never_reaches_the_terminal() {
    let cell = shell(
        "hostile",
        ShellOutcome::Ok {
            stdout: "\x1b]0;pwned\x07a\x1b[31mb\u{202E}c\u{9B}d".to_string(),
            stderr: String::new(),
            code: 0,
        },
        1,
    );
    let lines = shell_cell_lines(&cell, 80, &theme());
    let bytes = to_ansi(&lines);
    let mut parser = vt100::Parser::new_with_callbacks(24, 80, 0, TitleCalls::default());
    parser.process(&bytes);
    assert!(
        parser.callbacks().calls.is_empty(),
        "the window title must stay untouched"
    );
    let text = screen_text(parser.screen());
    for piece in ["a", "b", "c", "d"] {
        assert!(text.contains(piece), "{text}");
    }
    assert!(!text.contains("pwned"), "{text}");
}

#[test]
fn side_cell_layout_and_markers() {
    let cell = side(
        "what file?",
        SideOutcome::Answered {
            text: "**bold**".to_string(),
        },
        Origin::Here,
        false,
    );
    let lines = side_cell_lines(&cell, 80, &theme());
    assert!(
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.style.add_modifier.contains(Modifier::BOLD)),
        "the answer renders as Markdown"
    );
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("side answer · not part of the conversation"),
        "{text}"
    );
    let failed = side(
        "why?",
        SideOutcome::Failed {
            text: "boom".to_string(),
        },
        Origin::Here,
        false,
    );
    let lines = side_cell_lines(&failed, 80, &theme());
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("The side question failed: boom"), "{text}");
    assert!(!text.contains("not part of the conversation"), "{text}");
    let empty = side(
        "why?",
        SideOutcome::Answered {
            text: String::new(),
        },
        Origin::Here,
        false,
    );
    let lines = side_cell_lines(&empty, 80, &theme());
    let text: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("(no answer)"), "{text}");
    let away = side(
        "why?",
        SideOutcome::Answered {
            text: "hi".to_string(),
        },
        Origin::Elsewhere,
        false,
    );
    let lines = side_cell_lines(&away, 80, &theme());
    let head: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(head.contains("asked in another window"), "{head}");
    let late = side(
        "why?",
        SideOutcome::Answered {
            text: "hi".to_string(),
        },
        Origin::Here,
        true,
    );
    let lines = side_cell_lines(&late, 80, &theme());
    let head: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(head.contains("late answer"), "{head}");
}

#[test]
fn side_answer_is_cleaned_and_sanitised() {
    let cell = side(
        "why?",
        SideOutcome::Answered {
            text: "\x1b[2Jx\u{202E}y".to_string(),
        },
        Origin::Here,
        false,
    );
    let lines = side_cell_lines(&cell, 80, &theme());
    // `to_ansi` opens every styled run with its own SGR sequence, so the escape check reads the
    // text the cell renders rather than the encoded bytes.
    for span in lines.iter().flat_map(|line| line.spans.iter()) {
        assert!(
            !span.content.contains('\u{1b}'),
            "no escape byte may reach the screen: {:?}",
            span.content
        );
    }
    let bytes = to_ansi(&lines);
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&bytes);
    let text = screen_text(parser.screen());
    assert!(
        text.contains("btw \u{b7} why?"),
        "the answer must not clear the screen: {text}"
    );
    assert!(text.contains('x') && text.contains('y'), "{text}");
    assert!(text.contains("U+202E"), "a stand-in for U+202E: {text}");
}

fn pending_shell(command: &str, id: u64) -> PendingTask {
    PendingTask {
        request_id: RequestId(id),
        started_ms: 0,
        kind: PendingKind::Shell {
            command: command.to_string(),
        },
    }
}

fn pending_side(question: &str, id: u64) -> PendingTask {
    PendingTask {
        request_id: RequestId(id),
        started_ms: 0,
        kind: PendingKind::Side {
            question: question.to_string(),
            task_id: None,
            accepted_ms: None,
        },
    }
}

#[test]
fn side_tasks_section_rows_overflow_and_height() {
    let theme = theme();
    let empty = SideTasks::default();
    let section = SideTasksSection {
        tasks: &empty,
        now_ms: 0,
        theme: &theme,
    };
    assert_eq!(section.height(80), 0);
    let full = SideTasks {
        pending: vec![
            pending_shell("one", 1),
            pending_side("two", 2),
            pending_shell("three", 3),
            pending_side("four", 4),
            pending_shell("five", 5),
        ],
        ..SideTasks::default()
    };
    let section = SideTasksSection {
        tasks: &full,
        now_ms: 4_900,
        theme: &theme,
    };
    assert_eq!(section.height(80), 4);
    let mut area = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 80, 4));
    section.render(ratatui::layout::Rect::new(0, 0, 80, 4), &mut area);
    let text: String = (0..4u16)
        .map(|row| {
            (0..80u16)
                .map(|col| area[(col, row)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("+2 more"), "{text}");
    assert!(text.contains("4s"), "{text}");
}

#[test]
fn next_tick_only_while_pending() {
    let empty = SideTasks::default();
    assert_eq!(talaria_core::side::next_tick(&empty, 400), None);
    let theme = theme();
    let section = SideTasksSection {
        tasks: &empty,
        now_ms: 400,
        theme: &theme,
    };
    assert_eq!(section.next_tick(), None);
    let one = SideTasks {
        pending: vec![pending_shell("sleep 1", 1)],
        ..SideTasks::default()
    };
    assert_eq!(talaria_core::side::next_tick(&one, 400), Some(1_000));
}

#[test]
fn shell_mode_title_only_for_bang_drafts() {
    assert_eq!(
        shell_mode_title("!ls"),
        Some(
            "shell · runs in the session folder on the gateway host · the agent does not see the output"
        )
    );
    assert_eq!(shell_mode_title(" !ls"), None);
    assert_eq!(shell_mode_title("hi"), None);
}

#[test]
fn ledger_commits_side_cell_before_streaming_cell() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    let _ = update(&mut state, submit("one", "s1"));
    let _ = update(&mut state, ev(1, "message.start", json!({})));
    let _ = update(&mut state, ev(2, "message.delta", json!({"text": "live"})));
    let _ = update(
        &mut state,
        talaria_core::session::Event::ShellRequested {
            command: "ls".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        talaria_core::session::Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "x".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 1,
        },
    );
    let mut ledger = CommitLedger::default();
    let ready = ledger.take_ready(state.cells());
    assert!(ready.iter().any(|cell| matches!(cell, Cell::Shell(_))));
    assert_eq!(state.final_prefix_len(), 2);
}
