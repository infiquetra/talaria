//! Fleet header warning count rendering.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_core::fleet_view::{FleetViewInput, FleetViewState, update};
use talaria_tui::fleet_view::{FleetTheme, FleetView};

const NOW: f64 = 1_792_700_000.0;

fn draw_first_line(state: &FleetViewState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                state,
                None,
                frame.area(),
                frame.buffer_mut(),
                &FleetTheme::default(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn header_note_shows_the_warning_count() {
    let mut state = FleetViewState::new(None, NOW);
    update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: Vec::new(),
            queue: Vec::new(),
            profiles: Vec::new(),
            scope_note: None,
            link_note: Some("connecting\u{2026}".to_owned()),
        },
        8,
    );

    state.set_warning_count(0);
    let line_zero = draw_first_line(&state, 80, 24);
    assert!(
        line_zero.contains("connecting\u{2026}"),
        "expected link note when warning count is 0, got {line_zero:?}"
    );
    assert!(
        !line_zero.contains("warning"),
        "expected no warning text when warning count is 0, got {line_zero:?}"
    );

    state.set_warning_count(2);
    let line_two = draw_first_line(&state, 80, 24);
    assert!(
        line_two.contains("2 warnings"),
        "expected '2 warnings' when warning count is 2, got {line_two:?}"
    );
    assert!(
        !line_two.contains("connecting\u{2026}"),
        "expected warning count to replace link note, got {line_two:?}"
    );
}
