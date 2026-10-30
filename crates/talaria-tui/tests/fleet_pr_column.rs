//! Fleet pull-request column red test. Issue #245.
//!
//! Written against the fleet view's temporary stubs; after that unit merges,
//! this test is re-pointed at its merged rows and columns, with two labels on
//! three rows. Until then it pins the column's contract against the stub draw.

use std::collections::BTreeMap;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, RowState};
use talaria_core::fleet_view::{FleetViewInput, FleetViewState, update};
use talaria_core::review::{LabelTone, PrLabel};
use talaria_tui::fleet_view::{FleetTheme, FleetView};

fn row(session_id: &str, state: RowState) -> FleetRow {
    FleetRow {
        terminal_location: None,
        key: RowKey {
            profile: "fixture".to_owned(),
            session_id: session_id.to_owned(),
        },
        title: session_id.to_owned(),
        model: Some("fixture-model".to_owned()),
        source: None,
        started_at: None,
        message_count: 0,
        state,
        state_since: 1.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn key(session_id: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn screen_text(backend: &TestBackend) -> String {
    let buffer = backend.buffer().clone();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

/// The `PR` column shows at 120 columns with label tones, and drops at 100.
#[test]
fn pr_column_shows_and_drops_by_width() {
    let mut state = FleetViewState::new(None, 1.0);
    let _ = update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: vec![
                row("local-one", RowState::Working),
                row("local-two", RowState::NeedsYou),
                row("local-three", RowState::Idle),
            ],
            queue: vec![],
            profiles: vec![],
            scope_note: None,
            link_note: None,
        },
        10,
    );
    let labels = BTreeMap::from([
        (
            key("local-one"),
            PrLabel {
                text: "#12 draft ✓".to_owned(),
                tone: LabelTone::Success,
            },
        ),
        (
            key("local-two"),
            PrLabel {
                text: "#13 open ◐".to_owned(),
                tone: LabelTone::Warning,
            },
        ),
    ]);

    let theme = FleetTheme::default();
    let mut wide = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    wide.draw(|frame| {
        FleetView::draw_with_labels(
            &state,
            None,
            frame.area(),
            frame.buffer_mut(),
            &theme,
            &labels,
        );
    })
    .expect("draw");
    let text = screen_text(wide.backend());
    assert!(
        text.contains("#12 draft ✓"),
        "the success label shows: {text}"
    );

    let mut narrow = Terminal::new(TestBackend::new(100, 40)).expect("terminal");
    narrow
        .draw(|frame| {
            FleetView::draw_with_labels(
                &state,
                None,
                frame.area(),
                frame.buffer_mut(),
                &theme,
                &labels,
            );
        })
        .expect("draw");
    let text = screen_text(narrow.backend());
    assert!(
        !text.contains("#12 draft"),
        "the PR column drops at 100 columns: {text}"
    );
}
