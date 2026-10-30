//! Test 23 (#237): the approvals segment sits after the model and drops third.

mod support;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use support::{ByteSink, Vt, probe};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::status::{
    ApprovalsSegment, Connection, Segment, StatusInputs, StatusPalette, StatusSection, Tone, plan,
};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn full_inputs() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        model: Some("minimax-m3.1-flash".to_owned()),
        home_dir: Some("/home-fixture".to_owned()),
        working_dir: Some("/home-fixture/work/api".to_owned()),
        branch: Some("main".to_owned()),
        context_percent: Some(34),
        context_estimated: false,
        approvals: Some(ApprovalsSegment {
            text: "approvals: off (this session)".to_owned(),
            tone: Tone::Warning,
        }),
        ..StatusInputs::default()
    }
}

/// The rows the terminal shows for one status section, through real bytes.
fn shown(inputs: &StatusInputs, cols: u16) -> (Vec<String>, Vec<u8>) {
    let palette = StatusPalette::default();
    let section = StatusSection::new(inputs, None, &palette);
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (cols, 3),
        1,
        probe(false, false, Some((0, 0))),
        RestoreLink::detached(),
    )
    .expect("a terminal");
    sink.take();
    let view = LiveView {
        sections: vec![&section as &dyn LiveSection],
    };
    term.draw_live(&view).expect("a frame");
    let mut vt = Vt::new(cols, 3);
    let bytes = vt.absorb(&sink);
    (vt.rows(), bytes)
}

fn row_with(rows: &[String], needle: &str) -> String {
    rows.iter()
        .find(|row| row.contains(needle))
        .cloned()
        .unwrap_or_else(|| panic!("no row shows {needle:?}: {rows:?}"))
}

#[test]
fn segment_position_priority_and_snapshot() {
    // Priority and position: after the model, dropped after voice.
    assert_eq!(Segment::Approvals.priority(), 4);
    assert_eq!(Segment::Approvals.position(), 3);
    assert!(Segment::Model.position() < Segment::Approvals.position());
    assert!(Segment::Approvals.position() < Segment::Folder.position());

    // The wide line shows every segment in order.
    let inputs = full_inputs();
    let (rows, _) = shown(&inputs, 160);
    assert_eq!(
        row_with(&rows, "approvals:"),
        " minimax-m3.1-flash │ approvals: off (this session) │ ~/work/api (main) │ ctx 34%"
    );

    // At 80 columns the folder goes.
    let (rows, _) = shown(&inputs, 80);
    assert_eq!(
        row_with(&rows, "approvals:"),
        " minimax-m3.1-flash │ approvals: off (this session) │ ctx 34%"
    );

    // At 40 columns the context and the model go.
    let (rows, _) = shown(&inputs, 40);
    assert_eq!(
        row_with(&rows, "approvals:"),
        " approvals: off (this session)"
    );

    // The approvals cell carries the warning tone's style.
    let palette = StatusPalette::default();
    let planned = plan(&inputs, None, 400);
    let (_, _, tone) = planned
        .pieces
        .iter()
        .find(|(segment, _, _)| *segment == Segment::Approvals)
        .expect("the approvals piece");
    assert_eq!(*tone, Tone::Warning);
    let section = StatusSection::new(&inputs, None, &palette);
    let mut terminal = Terminal::new(TestBackend::new(160, 1)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect();
    let start = row.find("approvals:").expect("the approvals text");
    // A drawn cell reports every colour, so compare with a fresh cell given the palette style.
    let mut wanted = ratatui::buffer::Cell::default();
    wanted.set_style(palette.style(Tone::Warning));
    for x in start..start + "approvals:".len() {
        assert_eq!(buffer[(x as u16, 0)].style(), wanted.style(), "column {x}");
    }

    // Without approvals the line is the plain status line, unchanged.
    let mut plain = full_inputs();
    plain.approvals = None;
    let (rows, _) = shown(&plain, 160);
    assert_eq!(
        row_with(&rows, "minimax"),
        " minimax-m3.1-flash │ ~/work/api (main) │ ctx 34%"
    );
    assert!(rows.iter().all(|row| !row.contains("approvals:")));

    // A hostile folder name reaches the terminal with no escape byte.
    let mut hostile = full_inputs();
    hostile.working_dir = Some("/w/\u{1b}]0;pwned\u{7}q\u{202e}r".to_owned());
    hostile.branch = None;
    let planned = plan(&hostile, None, 400);
    let folder = planned
        .pieces
        .iter()
        .find(|(segment, _, _)| *segment == Segment::Folder)
        .map(|(_, text, _)| text.clone())
        .expect("the folder piece");
    assert!(
        !folder.contains('\u{1b}'),
        "an escape byte survived: {folder:?}"
    );
    assert!(
        !folder.contains('\u{202e}'),
        "an override survived: {folder:?}"
    );
    let (rows, bytes) = shown(&hostile, 160);
    assert!(row_with(&rows, "pwned").contains("pwned"));
    assert!(
        !bytes.windows(2).any(|pair| pair == b"\x1b]"),
        "an operating-system command reached the terminal"
    );
    assert!(!bytes.contains(&0x07), "a bell reached the terminal");
}
