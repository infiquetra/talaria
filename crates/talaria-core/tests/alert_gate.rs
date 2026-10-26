//! The shared alert gate: one window for foreground and background alerts.

#![allow(unreachable_code)]

use std::time::{Duration, Instant};

use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence};
use talaria_core::alert_gate::{AlertGate, urgency};
use talaria_core::attention::{Alert, AlertKind};

fn alert(kind: AlertKind) -> Alert {
    Alert {
        kind,
        bell: true,
        desktop: true,
        text: String::new(),
    }
}

fn proptest_config() -> Config {
    Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    }
}

#[test]
fn cross_source_rate_limit_and_urgency() {
    let window = Duration::from_secs(2);
    let base = Instant::now();

    let mut gate = AlertGate::new(window);
    let first = gate.admit(alert(AlertKind::NeedsYou), base);
    assert!(first.is_some(), "the first alert is admitted");
    assert!(
        gate.admit(alert(AlertKind::NeedsYou), base + Duration::from_secs(1))
            .is_none()
    );
    assert!(urgency(AlertKind::NeedsYou) > urgency(AlertKind::Failed));
    assert!(urgency(AlertKind::Failed) > urgency(AlertKind::Done));

    let mut gate = AlertGate::new(window);
    assert!(gate.admit(alert(AlertKind::Done), base).is_some());
    assert!(
        gate.admit(
            alert(AlertKind::NeedsYou),
            base + Duration::from_millis(500)
        )
        .is_some()
    );

    let mut gate = AlertGate::new(window);
    assert!(gate.admit(alert(AlertKind::NeedsYou), base).is_some());
    assert!(
        gate.admit(alert(AlertKind::Failed), base + Duration::from_secs(1))
            .is_none()
    );

    let mut gate = AlertGate::new(window);
    assert!(gate.admit(alert(AlertKind::NeedsYou), base).is_some());
    assert!(gate.admit(alert(AlertKind::Done), base + window).is_some());

    // A drop does not move the window.
    let mut gate = AlertGate::new(window);
    assert!(gate.admit(alert(AlertKind::NeedsYou), base).is_some());
    assert!(
        gate.admit(alert(AlertKind::Done), base + Duration::from_secs(1))
            .is_none()
    );
    assert!(
        gate.admit(alert(AlertKind::Failed), base + Duration::from_millis(1500))
            .is_none()
    );
    assert!(gate.admit(alert(AlertKind::Done), base + window).is_some());
}

fn kind_strategy() -> impl Strategy<Value = AlertKind> {
    prop_oneof![
        Just(AlertKind::NeedsYou),
        Just(AlertKind::Failed),
        Just(AlertKind::Done),
    ]
}

proptest! {
    #![proptest_config(proptest_config())]
    #[test]
    fn prop_gate_emissions_respect_window(
        steps in proptest::collection::vec((kind_strategy(), 0u64..10_000), 1..12)
    ) {
        let window = Duration::from_secs(2);
        let base = Instant::now();
        let mut gate = AlertGate::new(window);
        let mut clock = 0u64;
        let mut emitted: Vec<(u64, AlertKind)> = Vec::new();
        let mut first_admitted = false;
        for (index, (kind, gap)) in steps.iter().enumerate() {
            clock = clock.saturating_add(*gap);
            let admitted = gate
                .admit(alert(*kind), base + Duration::from_millis(clock))
                .is_some();
            if index == 0 {
                first_admitted = admitted;
            }
            if admitted {
                emitted.push((clock, *kind));
            }
        }
        prop_assert!(first_admitted);
        for pair in emitted.windows(2) {
            let (earlier_at, earlier) = pair[0];
            let (later_at, later) = pair[1];
            if later_at.saturating_sub(earlier_at) < 2_000 {
                let rising = urgency(later) > urgency(earlier);
                prop_assert!(rising);
            }
        }

        let mut gate = AlertGate::new(window);
        let mut at = 0u64;
        let mut last: Option<AlertKind> = None;
        for (kind, _) in &steps {
            if let Some(previous) = last {
                let rising = urgency(*kind) > urgency(previous);
                if !rising {
                    at = at.saturating_add(2_000);
                }
            }
            let admitted = gate
                .admit(alert(*kind), base + Duration::from_millis(at))
                .is_some();
            prop_assert!(admitted);
            last = Some(*kind);
        }
    }
}
