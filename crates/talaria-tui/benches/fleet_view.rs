//! Keystroke-to-frame time for a 500-row fleet overlay.
//!
//! The frame must contain a working group before anything is timed. One thousand
//! cursor moves and filter keystrokes are then timed. The bench exits non-zero
//! unless the 95th percentile is at most 16 ms and the worst is at most 33 ms.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, ProfileStatus, RowState};
use talaria_core::fleet_view::{FleetAction, FleetViewInput, FleetViewState, update};
use talaria_tui::fleet_view::{FleetTheme, FleetView};

const NOW: f64 = 1_792_700_000.0;
const ROWS: usize = 500;
const TIMED: usize = 1_000;
const P95_US: u128 = 16_000;
const WORST_US: u128 = 33_000;

fn row(index: usize) -> FleetRow {
    FleetRow {
        key: RowKey {
            profile: if index.is_multiple_of(2) {
                "default"
            } else {
                "fixture"
            }
            .to_owned(),
            session_id: format!("s-{index:04}"),
        },
        title: format!("fixture row {index}"),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - index as f64),
        message_count: 2,
        state: RowState::Working,
        state_since: NOW - 10.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: Some("compiling".to_owned()),
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn opened() -> FleetViewState {
    let mut state = FleetViewState::new(None, NOW);
    update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: (0..ROWS).map(row).collect(),
            queue: Vec::new(),
            profiles: vec![
                (
                    "default".to_owned(),
                    ProfileStatus::Listed { truncated: false },
                ),
                (
                    "fixture".to_owned(),
                    ProfileStatus::Listed { truncated: false },
                ),
            ],
            scope_note: None,
            link_note: None,
        },
        20,
    );
    state
}

fn text_of(buf: &Buffer) -> String {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn percentile(mut samples: Vec<u128>) -> (u128, u128, u128) {
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p95 = samples[949.min(samples.len() - 1)];
    let worst = samples[samples.len() - 1];
    (p50, p95, worst)
}

fn main() {
    let area = Rect::new(0, 0, 160, 48);
    let theme = FleetTheme::default();
    let mut state = opened();
    let mut buf = Buffer::empty(area);
    FleetView::draw(&state, None, area, &mut buf, &theme);
    assert!(
        text_of(&buf).contains("WORKING ("),
        "the fleet frame has no working group"
    );

    for _ in 0..20 {
        update(&mut state, FleetViewInput::Action(FleetAction::Down), 20);
        let mut warm = Buffer::empty(area);
        FleetView::draw(&state, None, area, &mut warm, &theme);
    }

    let mut samples = Vec::with_capacity(TIMED);
    for n in 0..TIMED {
        let start = Instant::now();
        if n % 2 == 0 {
            update(&mut state, FleetViewInput::Action(FleetAction::Down), 20);
        } else {
            update(&mut state, FleetViewInput::Action(FleetAction::Filter), 20);
            let key = char::from_digit((n % 10) as u32, 10).unwrap_or('0');
            update(&mut state, FleetViewInput::Text(key), 20);
        }
        let mut frame = Buffer::empty(area);
        FleetView::draw(&state, None, area, &mut frame, &theme);
        samples.push(start.elapsed().as_micros());
    }
    let (p50, p95, worst) = percentile(samples);
    println!("p50={p50}us p95={p95}us worst={worst}us");
    if p95 > P95_US || worst > WORST_US {
        std::process::exit(1);
    }
}
