use crate::support;

use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Line};
use support::{Alac, ByteSink, Vt, assert_tags_once_in_order, live_view_sections, probe, tagged};
use talaria_tui::{
    history::PlainCellLines,
    live::LiveView,
    pager::{Details, TranscriptEntry},
    terminal::{InlineTerminal, RestoreLink},
    workspace::{Workspace, WorkspaceView, menu::MenuState},
};

fn terminal() -> (InlineTerminal<ByteSink>, ByteSink, Alac) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut emulator = Alac::with_shell_history(80, 24);
    emulator.absorb(&sink);
    (term, sink, emulator)
}

fn paint(area: Rect, buffer: &mut Buffer) {
    buffer.set_string(area.x, area.y, "workspace frame", Style::default());
}

#[test]
fn enter_and_leave_restore_the_inline_view() {
    let (mut term, sink, mut emulator) = terminal();
    let sections = live_view_sections(2);
    let view = LiveView {
        sections: sections
            .iter()
            .map(|s| s as &dyn talaria_tui::live::LiveSection)
            .collect(),
    };
    term.draw_live(&view).unwrap();
    emulator.absorb(&sink);
    let before = emulator.screen();
    term.enter_workspace().unwrap();
    let enter = emulator.absorb(&sink);
    assert!(
        emulator.alternate(),
        "workspace must own the alternate screen"
    );
    assert_eq!(String::from_utf8_lossy(&enter).matches("?1049h").count(), 1);
    term.enter_workspace().unwrap();
    assert!(sink.is_empty(), "entry is idempotent");
    term.draw_workspace(paint).unwrap();
    emulator.absorb(&sink);
    term.leave_workspace().unwrap();
    let leave = emulator.absorb(&sink);
    assert_eq!(String::from_utf8_lossy(&leave).matches("?1049l").count(), 1);
    term.draw_live(&view).unwrap();
    emulator.absorb(&sink);
    assert!(!emulator.alternate());
    assert_eq!(emulator.screen(), before);
    term.leave_workspace().unwrap();
    assert!(sink.is_empty());
}

#[test]
fn text_finished_in_the_workspace_reaches_scrollback_once() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    emulator.absorb(&sink);
    // Cell commits and all four non-cell application effects reach these two terminal methods.
    let first = support::cells(&[true]);
    term.commit(&first, &PlainCellLines).unwrap();
    for label in [
        "command output",
        "context block",
        "checklist block",
        "subagent batch",
    ] {
        term.commit_lines(vec![Line::from(label)]).unwrap();
    }
    let last = talaria_core::session::Cell::User(talaria_core::session::UserCell {
        id: talaria_core::session::CellId::new(2),
        text: "last cell".into(),
        expanded_paste: None,
    });
    term.commit(&[last], &PlainCellLines).unwrap();
    let during = emulator.absorb(&sink);
    assert!(emulator.alternate(), "commits must remain in the workspace");
    assert!(
        during.is_empty(),
        "commit calls queue output until workspace drawing or exit"
    );
    term.leave_workspace().unwrap();
    emulator.absorb(&sink);
    let all = emulator.history_and_screen().join("\n");
    let mut previous = None;
    for label in [
        "cell 1",
        "command output",
        "context block",
        "checklist block",
        "subagent batch",
        "last cell",
    ] {
        assert_eq!(all.matches(label).count(), 1, "{label}: {all}");
        let at = all.find(label).unwrap();
        assert!(
            previous.is_none_or(|before| before < at),
            "interleaving changed: {all}"
        );
        previous = Some(at);
    }
}

#[test]
fn overlay_inside_the_workspace_returns_to_it() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    term.draw_workspace(paint).unwrap();
    emulator.absorb(&sink);
    term.commit_lines(tagged(3)).unwrap();
    term.open_overlay().unwrap();
    term.draw_overlay(|area, buffer| {
        buffer.set_string(area.x, area.y, "picker", Style::default());
    })
    .unwrap();
    emulator.absorb(&sink);
    term.close_overlay().unwrap();
    let closing = emulator.absorb(&sink);
    assert!(
        emulator.alternate(),
        "closing a picker must return to the workspace"
    );
    assert!(!String::from_utf8_lossy(&closing).contains("?1049l"));
    assert!(!String::from_utf8_lossy(&closing).contains("line 000"));
    term.draw_workspace(paint).unwrap();
    emulator.absorb(&sink);
    assert!(emulator.screen()[0].contains("workspace frame"));
    term.leave_workspace().unwrap();
    emulator.absorb(&sink);
    assert_tags_once_in_order(&emulator.history_and_screen(), 3);
}

#[test]
fn transient_menu_is_opaque_on_first_frame_and_restores_the_transcript() {
    let (mut terminal, sink, mut emulator) = terminal();
    emulator.resize(100, 36);
    terminal.resize(100, 36).unwrap();
    emulator.absorb(&sink);
    let transcript = vec![Line::from("t".repeat(90)); 36];
    terminal.commit_lines(transcript.clone()).unwrap();
    emulator.absorb(&sink);

    let footer = LiveView { sections: vec![] };
    let mut menu = MenuState::shown();
    menu.transient = true;
    let mut view = WorkspaceView {
        entries: &[TranscriptEntry::Lines(&transcript)],
        details: Details::Expanded,
        panel: talaria_core::layout::PanelVisibility::Hidden,
        footer: &footer,
        menu,
        checklist: None,
        checklist_expanded: false,
        work: None,
    };
    let mut workspace = Workspace::default();
    terminal.enter_workspace().unwrap();
    let mut paint =
        |terminal: &mut InlineTerminal<ByteSink>, emulator: &mut Alac, view: &WorkspaceView<'_>| {
            terminal
                .draw_workspace(|area, buffer| workspace.draw(view, area, buffer, &PlainCellLines))
                .unwrap();
            emulator.absorb(&sink);
            emulator.screen()
        };
    let first = paint(&mut terminal, &mut emulator, &view);
    assert_eq!(
        first[0].trim_end(),
        "Talaria   Session   Model   View   Voice   Settings   Help"
    );
    assert!(first[1].starts_with(&"t".repeat(90)));

    view.menu.dropdown_open = true;
    let open = paint(&mut terminal, &mut emulator, &view);
    assert!(open[1].starts_with("tttttttttt    Sessions"));
    assert!(open[1][22..40].chars().all(|ch| ch == ' '), "{open:?}");
    assert!(open[1][40..].starts_with(&"t".repeat(50)));
    assert!(open[2].contains("Fleet"));

    view.menu.dropdown_open = false;
    let closed = paint(&mut terminal, &mut emulator, &view);
    assert_eq!(closed, first);
    view.menu = MenuState::hidden();
    let no_menu = paint(&mut terminal, &mut emulator, &view);
    assert!(no_menu[0].starts_with(&"t".repeat(90)));
}

#[test]
fn quit_from_the_workspace_flushes_the_queue() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    emulator.absorb(&sink);
    assert!(emulator.alternate());
    term.commit_lines(tagged(2000)).unwrap();
    emulator.absorb(&sink);
    term.restore().unwrap();
    emulator.absorb(&sink);
    assert!(!emulator.alternate());
    assert_tags_once_in_order(&emulator.history_and_screen(), 2000);
    term.restore().unwrap();
    assert!(sink.is_empty(), "restore is idempotent");
}

#[test]
fn restore_leaves_the_alternate_screen() {
    let (mut term, sink, _) = terminal();
    let mut emulator = Vt::new(80, 24);
    term.enter_workspace().unwrap();
    emulator.absorb(&sink);
    assert!(emulator.alternate());
    term.open_overlay().unwrap();
    term.restore().unwrap();
    emulator.absorb(&sink);
    assert!(!emulator.alternate());
}

#[test]
fn workspace_never_enables_mouse() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    let mut bytes = emulator.absorb(&sink);
    assert!(emulator.alternate());
    term.draw_workspace(paint).unwrap();
    bytes.extend(sink.take());
    term.open_overlay().unwrap();
    term.close_overlay().unwrap();
    term.leave_workspace().unwrap();
    bytes.extend(sink.take());
    let bytes = String::from_utf8_lossy(&bytes);
    for mode in ["?1000h", "?1002h", "?1003h", "?1006h", "?1015h"] {
        assert!(!bytes.contains(mode), "mouse mode {mode}");
    }
}

#[test]
fn idle_workspace_writes_nothing() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    assert!(term.draw_workspace(paint).unwrap().drew);
    emulator.absorb(&sink);
    assert!(emulator.alternate());
    let idle = term.draw_workspace(paint).unwrap();
    assert!(!idle.drew);
    assert_eq!(idle.bytes, 0);
    assert!(sink.is_empty());
}

#[test]
fn resize_keeps_the_workspace_and_queued_output() {
    let (mut term, sink, mut emulator) = terminal();
    term.enter_workspace().unwrap();
    term.commit_lines(tagged(3)).unwrap();
    emulator.absorb(&sink);
    for (columns, rows) in [(120, 40), (80, 24)] {
        emulator.resize(columns, rows);
        term.resize(columns, rows).unwrap();
        term.draw_workspace(|area, buffer| {
            assert_eq!(area, Rect::new(0, 0, columns, rows));
            paint(area, buffer);
        })
        .unwrap();
        emulator.absorb(&sink);
        assert!(emulator.alternate());
    }
    term.leave_workspace().unwrap();
    emulator.absorb(&sink);
    assert_tags_once_in_order(&emulator.history_and_screen(), 3);
}

#[test]
fn streaming_frame_bytes_do_not_grow_with_transcript_length() {
    fn measure(history: usize) -> usize {
        let (mut term, sink, mut emulator) = terminal();
        term.enter_workspace().unwrap();
        emulator.absorb(&sink);
        assert!(emulator.alternate());
        term.commit_lines(tagged(history)).unwrap();
        term.draw_workspace(paint).unwrap();
        sink.take();
        let stats = term
            .draw_workspace(|area, buffer| {
                paint(area, buffer);
                buffer.set_string(0, 1, "streaming delta", Style::default());
            })
            .unwrap();
        assert!(stats.drew && stats.bytes > 0);
        assert_eq!(stats.bytes, sink.len());
        stats.bytes
    }
    assert_eq!(measure(3), measure(5000));
}
