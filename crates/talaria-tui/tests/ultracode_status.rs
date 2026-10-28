mod support;
use ratatui::{Terminal, backend::TestBackend, buffer::Cell};
use support::{ByteSink, Vt, probe};
use talaria_tui::{
    live::{LiveSection, LiveView},
    status::{Segment, StatusInputs, StatusPalette, StatusSection, Tone, plan},
    terminal::{InlineTerminal, RestoreLink},
};
use unicode_width::UnicodeWidthStr;
fn shown(inputs: &StatusInputs, columns: u16) -> Vec<String> {
    let palette = StatusPalette::default();
    let section = StatusSection::new(inputs, None, &palette);
    let sink = ByteSink::default();
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (columns, 3),
        1,
        probe(false, false, Some((0, 0))),
        RestoreLink::detached(),
    )
    .unwrap();
    sink.take();
    terminal
        .draw_live(&LiveView {
            sections: vec![&section as &dyn LiveSection],
        })
        .unwrap();
    let mut vt = Vt::new(columns, 3);
    vt.absorb(&sink);
    vt.rows()
}
#[test]
fn segment_snapshot() {
    let input = StatusInputs {
        ultracode: true,
        model: Some("fixture-model-with-extra-detail".into()),
        working_dir: Some("fixture/project/component".into()),
        branch: Some("feature".into()),
        context_percent: Some(34),
        ..StatusInputs::default()
    };
    let wide = plan(&input, None, 120);
    let model = wide
        .pieces
        .iter()
        .position(|(kind, _, _)| *kind == Segment::Model)
        .unwrap();
    let (_, text, tone) = &wide.pieces[model + 1];
    assert_eq!(text, "ultracode");
    assert_eq!(*tone, Tone::Accent);
    let wide_rows = shown(&input, 120);
    assert!(wide_rows.iter().any(|row| row.contains("ultracode")));
    let narrow = shown(&input, 40);
    assert!(narrow.iter().all(|row| !row.contains("ultracode")));
    let palette = StatusPalette::default();
    let section = StatusSection::new(&input, None, &palette);
    let mut terminal = Terminal::new(TestBackend::new(120, 1)).unwrap();
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect();
    let byte_start = row.find("ultracode").unwrap();
    let start = UnicodeWidthStr::width(&row[..byte_start]);
    let mut expected = Cell::default();
    expected.set_style(palette.style(Tone::Accent));
    for x in start..start + "ultracode".len() {
        assert_eq!(buffer[(x as u16, 0)].style(), expected.style());
    }
}
