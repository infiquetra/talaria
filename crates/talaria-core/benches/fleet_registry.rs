//! `update` of a 200-item liveness reply over 26 profiles.

use std::hint::black_box;

use criterion::{BatchSize, Criterion};
use talaria_core::fleet::{FleetInput, FleetRegistry, LiveItem, LiveStatus, RosterRow, update};

fn names() -> Vec<String> {
    std::iter::once("fixture".to_string())
        .chain((1..26).map(|index| format!("fixture-{index}")))
        .collect()
}

fn roster(session_id: &str) -> RosterRow {
    RosterRow {
        session_id: session_id.to_string(),
        title: String::new(),
        source: Some("tui".to_string()),
        started_at: Some(1_792_690_000.0),
        message_count: 1,
        running: false,
        model: None,
    }
}

fn setup() -> FleetRegistry {
    let mut registry = FleetRegistry::new();
    let served = names();
    let _ = update(
        &mut registry,
        FleetInput::Profiles {
            served: served.clone(),
            parked: Vec::new(),
            in_scope: served.clone(),
            at: 1_792_690_000.0,
        },
    );
    for (index, profile) in served.iter().enumerate() {
        let _ = update(
            &mut registry,
            FleetInput::Roster {
                profile: profile.clone(),
                request: 1,
                rows: vec![roster(&format!("s{index}"))],
                malformed: 0,
                at: 1_792_690_001.0,
            },
        );
    }
    registry
}

fn liveness() -> FleetInput {
    let items = (0..200)
        .map(|index| LiveItem {
            session_id: format!("s{index}"),
            status: LiveStatus::Idle,
            last_active: Some(1_792_690_002.0),
            message_count: 4,
            model: None,
            title: String::new(),
        })
        .collect();
    FleetInput::Liveness {
        profile: "fixture".to_string(),
        request: 2,
        items,
        malformed: 0,
        at: 1_792_690_002.0,
    }
}

fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    criterion.bench_function("fleet_update_200", |bencher| {
        bencher.iter_batched(
            setup,
            |mut registry| {
                black_box(update(&mut registry, black_box(liveness())));
            },
            BatchSize::SmallInput,
        );
    });
    criterion.final_summary();
}
