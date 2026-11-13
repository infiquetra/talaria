//! The hand-written fixture reduces to the expected rows. Issue #253.

use std::path::PathBuf;

use hermes_client::ReplaySource;
use talaria_app::profiles::{parse_description, parse_roster};
use talaria_core::profiles::{GatewayView, HostTopology, ProfilesInput, ProfilesState};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/profiles.jsonl")
}

#[test]
fn fixture_reduces_to_expected_rows() {
    let source = ReplaySource::open(&fixture()).expect("the fixture opens");
    let inbound: Vec<&hermes_client::ReplayFrame> = source.inbound().collect();
    assert_eq!(inbound.len(), 2);
    let list = inbound
        .iter()
        .find(|frame| frame.frame["id"] == 41)
        .expect("the list answer");
    let describe = inbound
        .iter()
        .find(|frame| frame.frame["id"] == 42)
        .expect("the describe answer");

    let roster = parse_roster(&list.frame["result"]).expect("the roster parses");
    assert_eq!(roster.len(), 6, "the own row plus five siblings");
    assert!(
        roster
            .iter()
            .all(|entry| entry.name != "default" || entry.is_default)
    );
    let described = parse_description(&describe.frame["result"]).expect("the describe parses");
    assert_eq!(described.name, "fixture");

    let mut machine = ProfilesState::new("default".to_owned());
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(
        ProfilesInput::Discovered(
            0,
            Ok(GatewayView {
                served: ["default", "fixture", "fixture-a"]
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                parked: [(
                    "fixture-b".to_owned(),
                    "session store unusable: <home>/profiles/fixture-b/state.db".to_owned(),
                )]
                .into_iter()
                .collect(),
            }),
        ),
        0,
    );
    let _ = machine.update(ProfilesInput::Listed(0, Ok(roster)), 0);
    let _ = machine.update(
        ProfilesInput::Topology(
            0,
            Ok(HostTopology {
                stopped: ["fixture-c".to_owned()].into_iter().collect(),
                own_gateway: Some(vec![]),
                gateway_mode: Some("multiplex".to_owned()),
                single_reason: None,
            }),
        ),
        0,
    );
    let _ = machine.update(
        ProfilesInput::Described(0, "fixture".to_owned(), Ok(described)),
        10,
    );

    let view = machine.view();
    let names: Vec<&str> = view.rows.iter().map(|row| row.name_cell.as_str()).collect();
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
    let labels: Vec<&str> = view.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "served",
            "served",
            "served",
            "parked",
            "stopped",
            "not discovered"
        ]
    );
    assert_eq!(view.rows[0].model_cell, "minimax-m3.1-flash · ollama-cloud");
    assert_eq!(
        view.rows[1].model_cell, "minimax-m3.1-flash · ollama-cloud",
        "the describe fills the selected row"
    );
    assert_eq!(view.header, "Profiles (6 on disk · 3 served)");
}
