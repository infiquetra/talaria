//! Classification, joining and explanations. Issue #253.

use std::collections::BTreeSet;

use proptest::prelude::*;
use talaria_core::profiles::ServeState;
use talaria_core::profiles::roster::{
    GatewayView, HostTopology, RosterEntry, classify, join_roster, normalize_profile_name,
};
use talaria_core::profiles::text::explain;

fn gateway(served: &[&str], parked: &[(&str, &str)]) -> GatewayView {
    GatewayView {
        served: served.iter().map(ToString::to_string).collect(),
        parked: parked
            .iter()
            .map(|(name, reason)| (name.to_string(), reason.to_string()))
            .collect(),
    }
}

fn topology(
    stopped: &[&str],
    own: Option<&[&str]>,
    mode: Option<&str>,
    reason: Option<&str>,
) -> HostTopology {
    HostTopology {
        stopped: stopped.iter().map(ToString::to_string).collect(),
        own_gateway: own.map(|names| names.iter().map(ToString::to_string).collect()),
        gateway_mode: mode.map(ToString::to_string),
        single_reason: reason.map(ToString::to_string),
    }
}

fn entry(name: &str, model: &str, provider: &str) -> RosterEntry {
    RosterEntry {
        name: name.to_owned(),
        display_name: name.to_owned(),
        is_default: name == "default",
        model: model.to_owned(),
        provider: provider.to_owned(),
        description: String::new(),
        skill_count: 0,
    }
}

fn state_of(
    name: &str,
    gateway: &GatewayView,
    topo: Option<&HostTopology>,
    created_at_ms: Option<u64>,
    now_ms: u64,
) -> ServeState {
    classify(name, gateway, topo, created_at_ms, now_ms)
}

#[test]
fn classify_every_state_and_overlap() {
    let full_gateway = gateway(
        &["default", "fixture", "fixture-a"],
        &[("fixture-b", "session store unusable: <home>/x/state.db")],
    );
    let full_topo = topology(
        &["fixture-b", "fixture-c"],
        Some(&["fixture-c", "fixture-d", "default"]),
        Some("multiplex"),
        None,
    );

    // Each row on its own.
    assert_eq!(
        state_of("fixture", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Served
    );
    assert!(matches!(
        state_of("fixture-b", &full_gateway, None, None, 0),
        ServeState::Parked { .. }
    ));
    assert_eq!(
        state_of(
            "fixture-c",
            &gateway(&[], &[]),
            Some(&topology(
                &["fixture-c"],
                Some(&[]),
                Some("multiplex"),
                None
            )),
            None,
            0
        ),
        ServeState::Stopped
    );
    assert_eq!(
        state_of(
            "fixture-d",
            &gateway(&[], &[]),
            Some(&topology(
                &[],
                Some(&["fixture-d"]),
                Some("multiplex"),
                None
            )),
            None,
            0
        ),
        ServeState::OwnGateway
    );
    assert_eq!(
        state_of(
            "fixture-d",
            &gateway(&[], &[]),
            Some(&topology(&[], Some(&[]), Some("single"), Some("opted out"))),
            None,
            0
        ),
        ServeState::HostSingle {
            reason: "opted out".to_owned()
        }
    );
    assert_eq!(
        state_of("fixture-new", &gateway(&[], &[]), None, Some(1_000), 2_000),
        ServeState::Starting
    );
    assert_eq!(
        state_of("fixture-d", &gateway(&[], &[]), None, None, 0),
        ServeState::NeverDiscovered
    );

    // Overlaps: the first matching row wins.
    assert_eq!(
        state_of("fixture", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Served,
        "served beats parked"
    );
    assert!(
        matches!(
            state_of("fixture-b", &full_gateway, Some(&full_topo), None, 0),
            ServeState::Parked { .. }
        ),
        "parked beats stopped"
    );
    assert_eq!(
        state_of("fixture-c", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Stopped,
        "stopped beats own gateway"
    );
    assert!(
        matches!(
            state_of(
                "fixture-b",
                &full_gateway,
                Some(&full_topo),
                Some(1_000),
                2_000
            ),
            ServeState::Parked { .. }
        ),
        "parked beats starting"
    );
    assert_eq!(
        state_of("default", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Served,
        "the host's own gateway entry still reads served"
    );

    // The starting window ends at exactly 40 000 ms.
    assert_eq!(
        state_of("fixture-new", &gateway(&[], &[]), None, Some(1_000), 40_999),
        ServeState::Starting,
        "39 999 ms after creation"
    );
    assert_eq!(
        state_of("fixture-new", &gateway(&[], &[]), None, Some(1_000), 41_000),
        ServeState::NeverDiscovered,
        "40 000 ms after creation falls through"
    );

    // Without a topology, rows 3 to 5 never match.
    for name in ["fixture-c", "fixture-d"] {
        let state = state_of(name, &gateway(&[], &[]), None, None, 0);
        assert!(
            !matches!(
                state,
                ServeState::Stopped | ServeState::OwnGateway | ServeState::HostSingle { .. }
            ),
            "{name} without a topology: {state:?}"
        );
    }

    // Without the gateway list, row 4 never matches but rows 3 and 5 still do.
    let gated = topology(&["fixture-c"], None, Some("single"), Some("why"));
    assert_eq!(
        state_of("fixture-c", &gateway(&[], &[]), Some(&gated), None, 0),
        ServeState::Stopped
    );
    assert!(matches!(
        state_of("fixture-d", &gateway(&[], &[]), Some(&gated), None, 0),
        ServeState::HostSingle { .. }
    ));
    let ungated = topology(&[], None, Some("multiplex"), None);
    assert_eq!(
        state_of("fixture-d", &gateway(&[], &[]), Some(&ungated), None, 0),
        ServeState::NeverDiscovered,
        "row 4 needs the gateway list"
    );

    // Row 5 needs a present mode and a non-empty reason.
    let no_mode = topology(&[], Some(&[]), None, Some("why"));
    assert_eq!(
        state_of("fixture-d", &gateway(&[], &[]), Some(&no_mode), None, 0),
        ServeState::NeverDiscovered,
        "an absent mode never matches row 5"
    );
    let unknown_mode = topology(&[], Some(&[]), Some("unknown"), Some("why"));
    assert!(
        matches!(
            state_of(
                "fixture-d",
                &gateway(&[], &[]),
                Some(&unknown_mode),
                None,
                0
            ),
            ServeState::HostSingle { .. }
        ),
        "an unknown mode matches row 5 when a reason is present"
    );
    let empty_reason = topology(&[], Some(&[]), Some("single"), Some(""));
    assert_eq!(
        state_of(
            "fixture-d",
            &gateway(&[], &[]),
            Some(&empty_reason),
            None,
            0
        ),
        ServeState::NeverDiscovered,
        "an empty reason never matches row 5"
    );
    let multiplex = topology(&[], Some(&[]), Some("multiplex"), Some("why"));
    assert_eq!(
        state_of("fixture-d", &gateway(&[], &[]), Some(&multiplex), None, 0),
        ServeState::NeverDiscovered,
        "multiplex never matches row 5"
    );

    // Names compare after trimming and lower-casing.
    assert_eq!(
        state_of(" Fixture ", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Served
    );
    assert_eq!(
        state_of("DEFAULT", &full_gateway, Some(&full_topo), None, 0),
        ServeState::Served
    );
    assert_eq!(normalize_profile_name(" Default "), "default");

    // Joining: the union, each name once, default first, then served, then the rest by name.
    let roster = [
        entry("default", "minimax-m3.1-flash", "ollama-cloud"),
        entry("fixture", "", ""),
        entry("fixture-a", "", ""),
        entry("fixture-b", "", ""),
        entry("fixture-c", "", ""),
        entry("fixture-d", "", ""),
    ];
    let rows = join_roster(Some(&roster), &full_gateway, Some(&full_topo), &[], 0);
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "default",
            "fixture",
            "fixture-a",
            "fixture-b",
            "fixture-c",
            "fixture-d"
        ]
    );
    assert_eq!(rows[0].state, ServeState::Served);
    assert_eq!(rows[1].state, ServeState::Served);
    assert_eq!(rows[2].state, ServeState::Served);
    assert!(matches!(rows[3].state, ServeState::Parked { .. }));
    assert_eq!(rows[4].state, ServeState::Stopped);
    assert_eq!(rows[5].state, ServeState::OwnGateway);
    assert_eq!(rows[0].model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(rows[0].provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(rows[1].model, None, "siblings carry no model");
    assert!(rows.iter().all(|row| row.in_roster));

    // A created name joins the union while its window runs, then leaves it.
    let created = [("fixture-new".to_owned(), 1_000)];
    let rows = join_roster(
        Some(&roster),
        &full_gateway,
        Some(&full_topo),
        &created,
        2_000,
    );
    let fresh = rows
        .iter()
        .find(|row| row.name == "fixture-new")
        .expect("the created row");
    assert_eq!(fresh.state, ServeState::Starting);
    assert!(!fresh.in_roster);
    assert_eq!(fresh.model, None);
    let rows = join_roster(
        Some(&roster),
        &full_gateway,
        Some(&full_topo),
        &created,
        41_000,
    );
    assert!(!rows.iter().any(|row| row.name == "fixture-new"));

    // A stopped name absent from a failed roster still gets a row.
    let rows = join_roster(None, &gateway(&[], &[]), Some(&full_topo), &[], 0);
    let stopped: Vec<&str> = rows
        .iter()
        .filter(|row| row.state == ServeState::Stopped)
        .map(|row| row.name.as_str())
        .collect();
    assert!(stopped.contains(&"fixture-c"), "{stopped:?}");
    assert!(stopped.contains(&"fixture-b"), "{stopped:?}");
    assert!(
        rows.iter().all(|row| !row.in_roster),
        "no row came from a roster"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn prop_join_lists_each_name_once(
        roster in prop::collection::vec(
            (small_name(), small_name(), prop::bool::ANY, small_word(), small_word(), 0..8u32),
            0..8,
        ),
        served in prop::collection::vec(small_name(), 0..8),
        parked in prop::collection::vec((small_name(), small_word()), 0..6),
        stopped in prop::collection::vec(small_name(), 0..6),
        own in prop::option::of(prop::collection::vec(small_name(), 0..6)),
        mode in prop::option::of(small_word()),
        reason in prop::option::of(small_word()),
        created in prop::collection::vec((small_name(), 0..100_000u64), 0..4),
        now_ms in 0..200_000u64,
    ) {
        let roster: Vec<RosterEntry> = roster
            .into_iter()
            .map(|(name, display, is_default, model, provider, skills)| RosterEntry {
                name,
                display_name: display,
                is_default,
                model,
                provider,
                description: String::new(),
                skill_count: skills,
            })
            .collect();
        let gateway = GatewayView {
            served,
            parked: parked.into_iter().collect(),
        };
        let topo = HostTopology {
            stopped,
            own_gateway: own,
            gateway_mode: mode,
            single_reason: reason,
        };
        let rows = join_roster(Some(&roster), &gateway, Some(&topo), &created, now_ms);

        // The union, each name once.
        let mut union = BTreeSet::new();
        for entry in &roster {
            union.insert(normalize_profile_name(&entry.name));
        }
        for name in &gateway.served {
            union.insert(normalize_profile_name(name));
        }
        for (name, _) in &gateway.parked {
            union.insert(normalize_profile_name(name));
        }
        for name in &topo.stopped {
            union.insert(normalize_profile_name(name));
        }
        if let Some(names) = &topo.own_gateway {
            for name in names {
                union.insert(normalize_profile_name(name));
            }
        }
        for (name, at) in &created {
            if *at <= now_ms && now_ms - *at < 40_000 {
                union.insert(normalize_profile_name(name));
            }
        }
        let joined: BTreeSet<String> =
            rows.iter().map(|row| normalize_profile_name(&row.name)).collect();
        prop_assert!(joined.len() == rows.len(), "each name exactly once");
        prop_assert_eq!(&joined, &union);

        // `default` first when present.
        if union.contains("default") {
            prop_assert!(normalize_profile_name(&rows[0].name) == "default");
        }

        // Every row has exactly one state, and a served name is always served.
        for row in &rows {
            let single = matches!(
                row.state,
                ServeState::Served
                    | ServeState::Parked { .. }
                    | ServeState::Stopped
                    | ServeState::OwnGateway
                    | ServeState::HostSingle { .. }
                    | ServeState::Starting
                    | ServeState::NeverDiscovered
                    | ServeState::Unknown
            );
            prop_assert!(single);
        }
        for name in &gateway.served {
            let wanted = normalize_profile_name(name);
            let row = rows
                .iter()
                .find(|row| normalize_profile_name(&row.name) == wanted)
                .expect("a served name is joined");
            prop_assert!(row.state == ServeState::Served);
        }

        // No row holds a path: the generator draws no `/`, and none appears.
        let debug = format!("{rows:?}");
        prop_assert!(!debug.contains('/'), "{debug}");
    }
}

fn small_name() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "a".to_owned(),
        "b".to_owned(),
        "c".to_owned(),
        "default".to_owned(),
        "Default".to_owned(),
        " fixture ".to_owned(),
        "a-b_c9".to_owned(),
    ])
}

fn small_word() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "".to_owned(),
        "why".to_owned(),
        "single".to_owned(),
        "unknown".to_owned(),
        "multiplex".to_owned(),
        "a reason".to_owned(),
    ])
}

#[test]
fn explanations_are_exact() {
    let cases: &[(ServeState, &str, &str, &str)] = &[
        (
            ServeState::Served,
            "served",
            "Served by this gateway. Start a session on it with: talaria --profile fixture",
            "talaria --profile fixture",
        ),
        (
            ServeState::Parked {
                reason: "session store unusable: <home>/x".to_owned(),
            },
            "parked",
            "The gateway could not serve this profile: session store unusable: <home>/x. Fix that, then press R to ask the gateway to try again.",
            "fix the reason, then rescan with R in /profiles",
        ),
        (
            ServeState::Stopped,
            "stopped",
            "This profile's service was stopped (hermes -p fixture gateway stop). Start it again with: hermes -p fixture gateway start",
            "hermes -p fixture gateway start",
        ),
        (
            ServeState::OwnGateway,
            "own gateway",
            "This profile runs its own gateway, so this gateway does not serve it. To serve it here, stop that gateway (hermes -p fixture gateway stop) and remove gateway.standalone: true from its config if it is set.",
            "hermes -p fixture gateway stop, then rescan with R in /profiles",
        ),
        (
            ServeState::HostSingle {
                reason: "this profile is standalone".to_owned(),
            },
            "not served",
            "This gateway serves only its own profile: this profile is standalone",
            "hermes gateway migrate --multiplex",
        ),
        (
            ServeState::Starting,
            "starting",
            "Created moments ago. The gateway usually serves a new profile within 30 seconds.",
            "wait up to 40 s, then refresh",
        ),
        (
            ServeState::NeverDiscovered,
            "not discovered",
            "The gateway never discovered this profile. It may opt out with gateway.standalone: true in its config, or it was created after the gateway's last scan. Press R to ask the gateway to rescan now.",
            "rescan with R in /profiles",
        ),
    ];
    for (state, label, text, next_step) in cases {
        let explanation = explain(state, "fixture");
        assert_eq!(explanation.label, *label, "{state:?}");
        assert_eq!(explanation.text, (*text).to_owned(), "{state:?}");
        assert_eq!(explanation.next_step, (*next_step).to_owned(), "{state:?}");
    }
    // Unknown rows keep the label; the state fills the words from the failure.
    assert_eq!(explain(&ServeState::Unknown, "fixture").label, "unknown");

    // A long reason is cut to 300 characters where the screen shows it.
    let explanation = explain(
        &ServeState::Parked {
            reason: "r".repeat(400),
        },
        "fixture",
    );
    assert_eq!(
        explanation.text,
        format!(
            "The gateway could not serve this profile: {}. Fix that, then press R to ask the gateway to try again.",
            "r".repeat(300)
        )
    );
    let explanation = explain(
        &ServeState::HostSingle {
            reason: "r".repeat(400),
        },
        "fixture",
    );
    assert_eq!(
        explanation.text,
        format!(
            "This gateway serves only its own profile: {}",
            "r".repeat(300)
        )
    );
}
