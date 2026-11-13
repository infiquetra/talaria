//! Live rows, the process overlay, and a finished background-task cell.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::background::{
    BackgroundCell, BackgroundEffect, BackgroundEvent, BackgroundWork, BgOrigin, BgResult, BgState,
    CallFailure, ListedProcess, ProcessId, ProcessesIntent, update,
};
use talaria_tui::background::{
    BackgroundSection, BackgroundTheme, ProcessesView, background_cell_lines,
};
use talaria_tui::keys::{Key, KeyCode, KeyKind, Mods};
use talaria_tui::live::LiveSection;

fn pid(id: &str) -> ProcessId {
    ProcessId(id.to_string())
}

fn listed(id: &str, command: &str, status: &str, started: Option<f64>) -> ListedProcess {
    ListedProcess {
        id: pid(id),
        command: command.to_string(),
        status_raw: status.to_string(),
        exit_code: if status == "running" { None } else { Some(0) },
        started_epoch_s: started,
        exited_at_s: None,
        completion_reason: None,
        output_tail: None,
    }
}

fn apply(work: &mut BackgroundWork, event: BackgroundEvent) -> Vec<BackgroundEffect> {
    update(work, event)
}

fn foreground(work: &mut BackgroundWork) -> talaria_core::side::RequestId {
    let _ = apply(work, BackgroundEvent::Attached { now_ms: 0 });
    let effects = apply(
        work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("the foreground lists")
}

fn list(
    work: &mut BackgroundWork,
    request: talaria_core::side::RequestId,
    rows: Vec<ListedProcess>,
) {
    let _ = apply(
        work,
        BackgroundEvent::Listed {
            request,
            processes: rows,
            now_ms: 1,
        },
    );
}

fn press(code: KeyCode) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

fn shift(ch: char) -> Key {
    Key {
        code: KeyCode::Char(ch),
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

fn ctrl(ch: char) -> Key {
    Key {
        code: KeyCode::Char(ch),
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

fn row_text(buf: &Buffer, y: u16) -> String {
    let mut text = String::new();
    for x in 0..buf.area.width {
        text.push_str(buf[(x, y)].symbol());
    }
    text.trim_end().to_string()
}

fn rows_of(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height).map(|y| row_text(buf, y)).collect()
}

fn draw_section(work: &BackgroundWork, width: u16, height: u16) -> Buffer {
    let theme = BackgroundTheme::default();
    let section = BackgroundSection {
        work,
        width_hint: width,
        theme: &theme,
    };
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    section.render(area, &mut buf);
    buf
}

fn section_height(work: &BackgroundWork, width: u16) -> u16 {
    let theme = BackgroundTheme::default();
    BackgroundSection {
        work,
        width_hint: width,
        theme: &theme,
    }
    .height(width)
}

fn draw_overlay(view: &ProcessesView, work: &BackgroundWork, width: u16, height: u16) -> Buffer {
    draw_overlay_at(view, work, width, height, 42_000)
}

fn draw_overlay_at(
    view: &ProcessesView,
    work: &BackgroundWork,
    width: u16,
    height: u16,
    now_ms: u64,
) -> Buffer {
    let theme = BackgroundTheme::default();
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    view.render(work, now_ms, 185.0, area, &mut buf, &theme);
    buf
}

#[test]
fn reopening_keeps_the_displayed_and_protected_process_together() {
    use talaria_core::background::MAX_TRACKED_OUTPUTS;

    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    list(
        &mut work,
        request,
        (0..=MAX_TRACKED_OUTPUTS)
            .map(|index| {
                listed(
                    &format!("p{index}"),
                    &format!("command{index}"),
                    "running",
                    Some(index as f64),
                )
            })
            .collect(),
    );
    let _ = apply(&mut work, BackgroundEvent::OverlayOpened { now_ms: 2 });
    let mut view = ProcessesView::new();
    view.reopen(&work);
    let ProcessesIntent::Select(selected) = view.handle_key(press(KeyCode::Down), &work, 3) else {
        panic!("Down selects the second process");
    };
    assert_eq!(selected, pid("p1"));
    let _ = apply(&mut work, BackgroundEvent::Selected { id: selected });
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "p1".to_owned(),
            chunk: "kept output\n".to_owned(),
            now_ms: 4,
        },
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('x')), &work, 5),
        ProcessesIntent::None
    );
    let _ = apply(&mut work, BackgroundEvent::OverlayClosed);
    let _ = apply(&mut work, BackgroundEvent::OverlayOpened { now_ms: 6 });

    // The native screen calls the same reopen method when taking the overlay again.
    view.reopen(&work);
    let drawn = draw_overlay_at(&view, &work, 120, 30, 6);
    assert!(
        rows_of(&drawn)
            .iter()
            .any(|row| row.starts_with('\u{25b8}') && row.ends_with("command1"))
    );
    assert_eq!(work.selected_process(), Some(&pid("p1")));
    assert_eq!(
        view.handle_key(press(KeyCode::Char('x')), &work, 7),
        ProcessesIntent::None,
        "reopening clears the previous kill confirmation"
    );
    let version = work.version();
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "p1".to_owned(),
            chunk: "new output\n".to_owned(),
            now_ms: 8,
        },
    );
    assert!(
        work.version() > version,
        "the visible output requests a redraw"
    );

    for index in (0..=MAX_TRACKED_OUTPUTS).filter(|index| *index != 1) {
        let _ = apply(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: format!("p{index}"),
                chunk: "other output\n".to_owned(),
                now_ms: 100 + index as u64,
            },
        );
    }
    assert_eq!(
        work.rows()
            .filter(|row| work.output(&row.id).is_some())
            .count(),
        MAX_TRACKED_OUTPUTS
    );
    assert_eq!(
        work.output(&pid("p1")).map(|output| output.raw()),
        Some("kept output\nnew output\n"),
        "the displayed process survives when older output must be evicted"
    );
}

fn lines_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn live_example() -> BackgroundWork {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let start = apply(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "summarise the open issues".to_string(),
            now_ms: 0,
        },
    );
    let request_bg = start.iter().find_map(|effect| match effect {
        BackgroundEffect::StartBackground { request, .. } => Some(*request),
        _ => None,
    });
    if let Some(request_bg) = request_bg {
        let _ = apply(
            &mut work,
            BackgroundEvent::BgAccepted {
                request: request_bg,
                task_id: "bg_000001".to_string(),
                now_ms: 0,
            },
        );
    }
    list(
        &mut work,
        request,
        vec![listed("tick", "sh tick.sh", "running", Some(0.0))],
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".to_string(),
            chunk: "tick 185\n".to_string(),
            now_ms: 1,
        },
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 42_000,
            wall_s: 185.0,
        },
    );
    work
}

fn overlay_example() -> BackgroundWork {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    list(
        &mut work,
        request,
        vec![
            listed("tick", "sh tick.sh", "running", Some(0.0)),
            listed("dev", "npm run dev", "running", Some(143.0)),
            ListedProcess {
                id: pid("pytest"),
                command: "pytest -q".to_string(),
                status_raw: "exited".to_string(),
                exit_code: Some(0),
                started_epoch_s: Some(100.0),
                exited_at_s: Some(145.0),
                completion_reason: Some("exited".to_string()),
                output_tail: Some("tick 183\ntick 184\ntick 185\n".to_string()),
            },
        ],
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".to_string(),
            chunk: "tick 183\ntick 184\ntick 185\n".to_string(),
            now_ms: 1,
        },
    );
    let _ = apply(&mut work, BackgroundEvent::OverlayOpened { now_ms: 42_000 });
    let _ = apply(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 42_000,
            wall_s: 185.0,
        },
    );
    work
}

#[test]
fn an_empty_section_has_no_height() {
    let empty = BackgroundWork::default();
    assert_eq!(section_height(&empty, 80), 0);
    let mut stopped = BackgroundWork::default();
    let request = foreground(&mut stopped);
    let _ = apply(
        &mut stopped,
        BackgroundEvent::ListFailed {
            request,
            failure: CallFailure::Refused {
                reason: "not_found".into(),
                detail: None,
            },
            now_ms: 1,
        },
    );
    for event in [
        BackgroundEvent::OverlayOpened { now_ms: 2 },
        BackgroundEvent::RefreshRequested { now_ms: 3 },
        BackgroundEvent::Clock {
            now_ms: 3 + 60_000,
            wall_s: 0.0,
        },
        BackgroundEvent::ToolCompleted {
            name: "terminal".to_string(),
            now_ms: 4,
        },
    ] {
        let effects = apply(&mut stopped, event);
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, BackgroundEffect::ListProcesses { .. })),
            "a stopped session is not listed again"
        );
    }
    let view = ProcessesView::new();
    let drawn = rows_of(&draw_overlay(&view, &stopped, 80, 12))
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(drawn.contains(
        "This session is no longer live in Hermes, so its processes cannot be listed. Resume it to see them."
    ));
    let intent = view
        .clone()
        .handle_key(press(KeyCode::Char('r')), &stopped, 5);
    assert_eq!(intent, ProcessesIntent::Refresh);
}

#[test]
fn live_rows_use_the_settled_text() {
    let work = live_example();
    assert_eq!(section_height(&work, 80), 2);
    let drawn = draw_section(&work, 80, 4);
    assert_eq!(
        row_text(&drawn, 0),
        "◌ bg · summarise the open issues · running · 42s"
    );
    assert_eq!(
        row_text(&drawn, 1),
        "⚙ sh tick.sh · 3m05s · last: tick 185 · /processes"
    );

    let mut several = BackgroundWork::default();
    let request = foreground(&mut several);
    list(
        &mut several,
        request,
        vec![
            listed("old", "sleep 1", "running", Some(1.0)),
            listed("mid", "sleep 2", "running", Some(2.0)),
            listed("new", "npm run dev", "running", Some(9.0)),
        ],
    );
    let _ = apply(
        &mut several,
        BackgroundEvent::Clock {
            now_ms: 10_000,
            wall_s: 20.0,
        },
    );
    let text = row_text(&draw_section(&several, 80, 2), 0);
    assert_eq!(
        text,
        "⚙ 3 background processes running · newest: npm run dev · /processes"
    );

    let mut quiet = BackgroundWork::default();
    let request = foreground(&mut quiet);
    list(
        &mut quiet,
        request,
        vec![ListedProcess {
            exited_at_s: Some(10.0),
            ..listed("done", "pytest -q", "exited", Some(1.0))
        }],
    );
    let _ = apply(
        &mut quiet,
        BackgroundEvent::Clock {
            now_ms: 50_000,
            wall_s: 50.0,
        },
    );
    let finished = row_text(&draw_section(&quiet, 100, 2), 0);
    assert!(finished.starts_with('✓'), "{finished}");
    assert!(finished.contains("pytest -q"), "{finished}");
    assert!(finished.contains("ended "), "{finished}");
    assert!(finished.contains("/processes"), "{finished}");

    let mut bare = BackgroundWork::default();
    let request = foreground(&mut bare);
    list(
        &mut bare,
        request,
        vec![listed("bare", "", "running", Some(0.0))],
    );
    let _ = apply(
        &mut bare,
        BackgroundEvent::Clock {
            now_ms: 1_000,
            wall_s: 1.0,
        },
    );
    let bare_row = row_text(&draw_section(&bare, 80, 2), 0);
    assert!(bare_row.contains("background process"), "{bare_row}");
    assert!(bare_row.contains("no output yet"), "{bare_row}");
    assert!(!bare_row.contains("last:"), "{bare_row}");

    let mut tasks = BackgroundWork::default();
    let _ = foreground(&mut tasks);
    for (index, text) in ["one", "two", "three", "four"].iter().enumerate() {
        let started = apply(
            &mut tasks,
            BackgroundEvent::BgCommand {
                text: (*text).to_string(),
                now_ms: index as u64,
            },
        );
        let request = started.iter().find_map(|effect| match effect {
            BackgroundEffect::StartBackground { request, .. } => Some(*request),
            _ => None,
        });
        if let Some(request) = request {
            let _ = apply(
                &mut tasks,
                BackgroundEvent::BgAccepted {
                    request,
                    task_id: format!("bg_{index}"),
                    now_ms: index as u64,
                },
            );
        }
    }
    let _ = apply(
        &mut tasks,
        BackgroundEvent::Clock {
            now_ms: 2_000,
            wall_s: 2.0,
        },
    );
    let task_rows = rows_of(&draw_section(&tasks, 80, 6));
    assert!(
        task_rows
            .iter()
            .any(|row| row.contains("+2 more background tasks")),
        "{task_rows:?}"
    );

    let mut starting = BackgroundWork::default();
    let _ = foreground(&mut starting);
    let _ = apply(
        &mut starting,
        BackgroundEvent::BgCommand {
            text: "wait please".to_string(),
            now_ms: 1,
        },
    );
    let starting_row = row_text(&draw_section(&starting, 80, 2), 0);
    assert_eq!(starting_row, "◌ bg · wait please · starting");
    let _ = apply(&mut starting, BackgroundEvent::Attached { now_ms: 2 });
    assert!(
        starting
            .bg_tasks()
            .iter()
            .any(|task| matches!(task.state, BgState::ReplyLost))
    );
    let lost = row_text(&draw_section(&starting, 80, 2), 0);
    assert_eq!(
        lost,
        "◌ bg · wait please · reply lost · the answer will still appear"
    );
}

#[test]
fn a_wide_row_keeps_the_suffix_until_it_cannot() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    list(
        &mut work,
        request,
        vec![listed(
            "wide",
            "python train.py --epochs 40 --data warehouse",
            "running",
            Some(0.0),
        )],
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "wide".to_string(),
            chunk: "epoch 1 of 40 is still training the warehouse model\n".to_string(),
            now_ms: 1,
        },
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 5_000,
            wall_s: 5.0,
        },
    );
    let wide = row_text(&draw_section(&work, 48, 2), 0);
    assert!(wide.contains('…'), "{wide}");
    assert!(wide.ends_with("/processes") || wide.contains("…"), "{wide}");
    let narrow = row_text(&draw_section(&work, 16, 2), 0);
    assert!(!narrow.contains("/processes"), "{narrow}");
    assert!(narrow.contains('…'), "{narrow}");
}

#[test]
fn a_closed_overlay_does_not_repaint_on_a_chunk() {
    let mut work = live_example();
    assert!(!work.overlay_open());
    let before = row_text(&draw_section(&work, 80, 3), 1);
    assert!(before.contains("last: tick 185"), "{before}");
    let version = work.version();
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".to_string(),
            chunk: "tick 186\n".to_string(),
            now_ms: 42_100,
        },
    );
    assert_eq!(
        work.version(),
        version,
        "a chunk with the overlay closed does not redraw"
    );
    let after = row_text(&draw_section(&work, 80, 3), 1);
    assert_eq!(after, before);
    let _ = apply(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 43_000,
            wall_s: 186.0,
        },
    );
    let ticked = row_text(&draw_section(&work, 80, 3), 1);
    assert!(ticked.contains("last: tick 186"), "{ticked}");
}

#[test]
fn the_overlay_matches_the_worked_layout() {
    let work = overlay_example();
    let view = ProcessesView::new();
    let rows = rows_of(&draw_overlay(&view, &work, 80, 20));
    let screen = rows.join("\n");
    assert!(
        screen.contains("Background processes · 2 running · 1 finished"),
        "{screen}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains('▸') && row.contains("sh tick.sh")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains('●') && row.contains("npm run dev")),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains('✓') && row.contains("pytest -q")),
        "{rows:?}"
    );
    assert!(screen.contains("$ sh tick.sh"), "{screen}");
    assert!(screen.contains("tick 185"), "{screen}");
    let footer = "↑↓ select · PgUp PgDn scroll · End follow · x kill · S stop all · d dismiss · r refresh · esc close";
    assert_eq!(
        rows.last().map(String::as_str),
        Some("↑↓ select · PgUp PgDn scroll · End follow · x kill · S stop all · d… · esc close")
    );
    let wide = rows_of(&draw_overlay(&view, &work, 160, 48));
    assert!(wide.iter().any(|row| row.contains(footer)), "{wide:?}");
    assert!(screen.contains("esc close"), "{screen}");
}

#[test]
fn overlay_keys_select_refresh_dismiss_and_close() {
    let work = overlay_example();
    let mut view = ProcessesView::new();
    let down = view.handle_key(press(KeyCode::Down), &work, 1);
    assert!(matches!(down, ProcessesIntent::Select(_)), "{down:?}");
    let up = view.handle_key(press(KeyCode::Up), &work, 1);
    assert!(matches!(up, ProcessesIntent::Select(_)), "{up:?}");
    let stayed = view.handle_key(press(KeyCode::Char('k')), &work, 1);
    assert_eq!(stayed, ProcessesIntent::None, "selection does not wrap");
    assert_eq!(
        view.handle_key(press(KeyCode::PageUp), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::PageDown), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Home), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('g')), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::End), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('G')), &work, 1),
        ProcessesIntent::None
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('d')), &work, 1),
        ProcessesIntent::None
    );
    let _ = view.handle_key(press(KeyCode::Down), &work, 1);
    let _ = view.handle_key(press(KeyCode::Down), &work, 1);
    assert!(matches!(
        view.handle_key(press(KeyCode::Char('d')), &work, 1),
        ProcessesIntent::Dismiss(_)
    ));
    assert_eq!(
        view.handle_key(press(KeyCode::Char('r')), &work, 1),
        ProcessesIntent::Refresh
    );
    for key in [press(KeyCode::Esc), press(KeyCode::Char('q')), ctrl('c')] {
        assert_eq!(view.handle_key(key, &work, 1), ProcessesIntent::Close);
    }
    let _ = view.handle_key(press(KeyCode::Char('x')), &work, 1_000);
    assert_eq!(
        view.handle_key(press(KeyCode::Char('z')), &work, 1_100),
        ProcessesIntent::None
    );
    let screen = rows_of(&draw_overlay(&view, &work, 80, 20)).join("\n");
    assert!(!screen.contains("Press x again"), "{screen}");
}

#[test]
fn a_small_terminal_shows_the_widen_note() {
    let work = overlay_example();
    let view = ProcessesView::new();
    for (width, height) in [(39, 20), (80, 9)] {
        let screen = rows_of(&draw_overlay(&view, &work, width, height))
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            screen.contains(
                "Widen the terminal to at least 40 columns and 10 rows to see background processes. Esc closes."
            ),
            "{width}x{height}: {screen}"
        );
    }
}

struct HostileHooks {
    titles: Vec<Vec<u8>>,
    clipboard: Vec<Vec<u8>>,
}

impl vt100::Callbacks for HostileHooks {
    fn set_window_title(&mut self, _screen: &mut vt100::Screen, title: &[u8]) {
        self.titles.push(title.to_vec());
    }

    fn copy_to_clipboard(&mut self, _screen: &mut vt100::Screen, _ty: &[u8], data: &[u8]) {
        self.clipboard.push(data.to_vec());
    }
}

fn buffer_bytes(buf: &Buffer) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.extend(buf[(x, y)].symbol().as_bytes());
        }
        out.extend(b"\r\n");
    }
    out
}

#[test]
fn hostile_output_cannot_change_the_terminal() {
    let hostile =
        "KEEPME\n\u{1b}]0;pwned\u{7}\u{1b}]52;c;aGk=\u{7}\u{1b}[2Jprogress\r\u{202e}line\n";
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let _ = apply(
        &mut work,
        BackgroundEvent::BgCommand {
            text: format!("prompt {hostile}"),
            now_ms: 0,
        },
    );
    list(
        &mut work,
        request,
        vec![listed(
            "bad",
            &format!("cmd {hostile}"),
            "running",
            Some(0.0),
        )],
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "bad".to_string(),
            chunk: hostile.to_string(),
            now_ms: 1,
        },
    );
    let _ = apply(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 1_000,
            wall_s: 1.0,
        },
    );
    let _ = apply(&mut work, BackgroundEvent::OverlayOpened { now_ms: 1_000 });
    let cell = BackgroundCell {
        task_id: "bg_hostile".to_string(),
        prompt: Some(format!("answer {hostile}")),
        origin: BgOrigin::Here,
        result: BgResult::Answer(hostile.to_string()),
    };
    let theme = BackgroundTheme::default();
    let mut bytes = buffer_bytes(&draw_section(&work, 80, 6));
    bytes.extend(buffer_bytes(&draw_overlay(
        &ProcessesView::new(),
        &work,
        80,
        20,
    )));
    for line in background_cell_lines(&cell, 80, &theme) {
        for span in &line.spans {
            bytes.extend(span.content.as_bytes());
        }
        bytes.extend(b"\r\n");
    }
    assert!(
        !bytes.contains(&0x1b),
        "an escape byte reached the terminal"
    );
    assert!(!bytes.contains(&0x07), "a bell byte reached the terminal");
    let mut parser = vt100::Parser::new_with_callbacks(
        40,
        80,
        0,
        HostileHooks {
            titles: Vec::new(),
            clipboard: Vec::new(),
        },
    );
    parser.process(b"KEEP\r\n");
    parser.process(&bytes);
    assert!(parser.callbacks().titles.is_empty());
    assert!(parser.callbacks().clipboard.is_empty());
    let screen = parser.screen().contents();
    assert!(screen.contains("KEEP"), "{screen}");
    assert!(screen.contains("KEEPME"), "{screen}");
    assert!(screen.contains("\u{2039}U+202E\u{203a}"), "{screen}");
}

#[test]
fn kill_and_stop_need_a_second_press() {
    let work = overlay_example();
    let mut expired = ProcessesView::new();
    assert_eq!(
        expired.handle_key(press(KeyCode::Char('x')), &work, 1_000),
        ProcessesIntent::None
    );
    let after_expiry = rows_of(&draw_overlay_at(&expired, &work, 80, 20, 4_001)).join("\n");
    assert!(!after_expiry.contains("Press x again"));
    assert_eq!(
        expired.handle_key(press(KeyCode::Char('x')), &work, 4_001),
        ProcessesIntent::None
    );
    assert_eq!(
        expired.handle_key(press(KeyCode::Char('x')), &work, 7_001),
        ProcessesIntent::Kill(pid("tick"))
    );
    let mut expired_stop = ProcessesView::new();
    assert_eq!(
        expired_stop.handle_key(shift('S'), &work, 1_000),
        ProcessesIntent::None
    );
    assert_eq!(
        expired_stop.handle_key(shift('S'), &work, 4_001),
        ProcessesIntent::None
    );
    assert_eq!(
        expired_stop.handle_key(shift('S'), &work, 7_001),
        ProcessesIntent::StopAll
    );
    let mut view = ProcessesView::new();
    assert_eq!(
        view.handle_key(press(KeyCode::Char('x')), &work, 1_000),
        ProcessesIntent::None
    );
    let asked = rows_of(&draw_overlay_at(&view, &work, 80, 20, 1_000)).join("\n");
    assert!(
        asked.contains("Press x again within 3 s to kill: sh tick.sh"),
        "{asked}"
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('x')), &work, 4_000),
        ProcessesIntent::Kill(pid("tick"))
    );
    assert_eq!(
        view.handle_key(press(KeyCode::Char('x')), &work, 4_001),
        ProcessesIntent::None
    );
    let again = rows_of(&draw_overlay_at(&view, &work, 80, 20, 4_001)).join("\n");
    assert!(
        again.contains("Press x again within 3 s to kill:"),
        "{again}"
    );

    let mut stop = ProcessesView::new();
    assert_eq!(
        stop.handle_key(shift('S'), &work, 1_000),
        ProcessesIntent::None
    );
    let narrow = rows_of(&draw_overlay_at(&stop, &work, 100, 20, 1_000));
    assert!(
        narrow
            .iter()
            .all(|row| Line::from(row.as_str()).width() <= 100)
    );
    assert!(
        narrow
            .iter()
            .any(|row| row.contains("Press S again within 3 s"))
    );
    let note = rows_of(&draw_overlay_at(&stop, &work, 160, 20, 1_000)).join("\n");
    assert!(
        note.contains(
            "Press S again within 3 s to stop all 2 running processes of this session (also interrupts its background delegations)."
        ),
        "{note}"
    );
    assert_eq!(
        stop.handle_key(shift('S'), &work, 4_000),
        ProcessesIntent::StopAll
    );

    let mut one = BackgroundWork::default();
    let request = foreground(&mut one);
    list(
        &mut one,
        request,
        vec![listed("only", "sleep 9", "running", Some(0.0))],
    );
    let mut view = ProcessesView::new();
    let _ = view.handle_key(shift('S'), &one, 1);
    let singular = rows_of(&draw_overlay_at(&view, &one, 120, 16, 1)).join("\n");
    assert!(
        singular.contains(
            "Press S again within 3 s to stop the 1 running process of this session (also interrupts its background delegations)."
        ),
        "{singular}"
    );

    let mut finished = BackgroundWork::default();
    let request = foreground(&mut finished);
    list(
        &mut finished,
        request,
        vec![ListedProcess {
            status_raw: "exited".to_string(),
            exit_code: Some(0),
            exited_at_s: Some(1.0),
            ..listed("gone", "pytest -q", "exited", Some(0.0))
        }],
    );
    let mut view = ProcessesView::new();
    assert_eq!(
        view.handle_key(shift('S'), &finished, 1),
        ProcessesIntent::None
    );
    let none = rows_of(&draw_overlay(&view, &finished, 80, 12)).join("\n");
    assert!(
        none.contains("No background processes are running."),
        "{none}"
    );
    assert_ne!(
        view.handle_key(press(KeyCode::Char('x')), &finished, 2),
        ProcessesIntent::Kill(pid("gone"))
    );
    let ended = rows_of(&draw_overlay(&view, &finished, 80, 12)).join("\n");
    assert!(ended.contains("This process has already ended."), "{ended}");
}

#[test]
fn a_result_cell_shows_the_answer_and_the_footer() {
    let theme = BackgroundTheme::default();
    let here = BackgroundCell {
        task_id: "bg_0000aa".to_string(),
        prompt: Some("summarise the open issues".to_string()),
        origin: BgOrigin::Here,
        result: BgResult::Answer("A short answer.".to_string()),
    };
    let here_text = lines_text(&background_cell_lines(&here, 80, &theme));
    assert!(
        here_text.contains("bg · summarise the open issues"),
        "{here_text}"
    );
    assert!(here_text.contains("  A short answer."), "{here_text}");
    assert!(
        here_text.contains("background task bg_0000aa · not part of the conversation"),
        "{here_text}"
    );

    let elsewhere = BackgroundCell {
        task_id: "bg_other".to_string(),
        prompt: None,
        origin: BgOrigin::Elsewhere,
        result: BgResult::Answer(String::new()),
    };
    let elsewhere_text = lines_text(&background_cell_lines(&elsewhere, 80, &theme));
    assert!(
        elsewhere_text.contains("bg · bg_other · started in another window"),
        "{elsewhere_text}"
    );
    assert!(elsewhere_text.contains("(no answer)"), "{elsewhere_text}");

    let failed = BackgroundCell {
        task_id: "bg_bad".to_string(),
        prompt: Some("do the thing".to_string()),
        origin: BgOrigin::Here,
        result: BgResult::Failed("disk full".to_string()),
    };
    let failed_text = lines_text(&background_cell_lines(&failed, 80, &theme));
    assert!(
        failed_text.contains("The background task failed: disk full"),
        "{failed_text}"
    );
    assert!(
        failed_text.contains("background task bg_bad · not part of the conversation"),
        "{failed_text}"
    );
}
