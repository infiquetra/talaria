//! Times picker search keystrokes and cursor moves over 2,000 rows (#231).
//!
//! A search keystroke (filter and draw) must stay within 16 ms at the 95th
//! percentile and 33 ms worst; a cursor move within 16 ms at the 95th
//! percentile. The bench exits non-zero on a breach.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};
use talaria_core::directory::state::{PickerAction, PickerInput, PickerState, update};
use talaria_tui::session_picker::{PickerTheme, SessionPickerView};

const ROWS: usize = 2_000;
const WARM_UP: usize = 100;
const TIMED: usize = 1_000;
const P95_US: u128 = 16_000;
const FILTER_MAX_US: u128 = 33_000;

fn rows() -> Vec<DirectoryRow> {
    let models = [
        "ollama-cloud/minimax-m3.1-flash",
        "openrouter/quiet-draft",
        "local/edge-case-",
    ];
    (0..ROWS)
        .map(|n| DirectoryRow {
            key: RowKey {
                profile: if n % 2 == 0 { "default" } else { "fixture" }.to_owned(),
                session_id: format!("s-{n:04}"),
            },
            title: format!("fixture row {n} · wide Ｍ text"),
            source: Some("tui".to_owned()),
            started_at: Some(1_792_700_000.0 - n as f64 * 60.0),
            message_count: n as u64,
            running: false,
            model: Some(models[n % models.len()].to_owned()),
            origin: if n % 3 == 0 {
                RowOrigin::Stored
            } else {
                RowOrigin::Live
            },
        })
        .collect()
}

fn opened() -> PickerState {
    let mut state = PickerState::new();
    let rows = rows();
    let (live, stored): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .partition(|row| row.origin == RowOrigin::Live);
    update(
        &mut state,
        PickerInput::Opened {
            profiles: vec!["default".to_owned(), "fixture".to_owned()],
            foreground: None,
            scope_note: None,
            cached: Vec::new(),
            generation: 1,
        },
        20,
    );
    for (origin, rows) in [(RowOrigin::Live, live), (RowOrigin::Stored, stored)] {
        update(
            &mut state,
            PickerInput::Listed {
                generation: 1,
                profile: "fixture".to_owned(),
                origin,
                rows,
                skipped: 0,
            },
            20,
        );
    }
    state
}

fn percentile(mut durations: Vec<u128>) -> (u128, u128) {
    durations.sort_unstable();
    let p95 = durations[949.min(durations.len() - 1)];
    let max = durations[durations.len() - 1];
    (p95, max)
}

fn main() {
    let area = Rect::new(0, 0, 160, 48);
    let theme = PickerTheme::new();

    for _ in 0..WARM_UP {
        let mut state = opened();
        update(&mut state, PickerInput::Action(PickerAction::Search), 20);
        update(&mut state, PickerInput::Text('x'), 20);
        let mut buf = Buffer::empty(area);
        SessionPickerView::draw(&state, area, &mut buf, 1_792_700_000.0, &theme);
    }
    let mut filter_us = Vec::with_capacity(TIMED);
    for n in 0..TIMED {
        let mut state = opened();
        update(&mut state, PickerInput::Action(PickerAction::Search), 20);
        for key in "row ".chars() {
            update(&mut state, PickerInput::Text(key), 20);
        }
        let last = char::from_digit((n % 10) as u32, 10).unwrap_or('0');
        let start = Instant::now();
        update(&mut state, PickerInput::Text(last), 20);
        let mut buf = Buffer::empty(area);
        SessionPickerView::draw(&state, area, &mut buf, 1_792_700_000.0, &theme);
        filter_us.push(start.elapsed().as_micros());
    }
    let (filter_p95, filter_max) = percentile(filter_us);

    let mut move_us = Vec::with_capacity(TIMED);
    for _ in 0..TIMED {
        let mut state = opened();
        let start = Instant::now();
        update(&mut state, PickerInput::Action(PickerAction::Down), 20);
        let mut buf = Buffer::empty(area);
        SessionPickerView::draw(&state, area, &mut buf, 1_792_700_000.0, &theme);
        move_us.push(start.elapsed().as_micros());
    }
    let (move_p95, _) = percentile(move_us);

    println!(
        "picker_filter_p95_us={filter_p95} picker_filter_max_us={filter_max} picker_move_p95_us={move_p95}"
    );
    if filter_p95 > P95_US || filter_max > FILTER_MAX_US || move_p95 > P95_US {
        std::process::exit(1);
    }
}
