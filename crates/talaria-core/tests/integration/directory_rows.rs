//! Red-run tests 1 to 5 (#231): directory rows.

use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::directory::rows::{
    DirectoryRow, RowKey, RowOrigin, format_age, matches, merge, order, short_model,
};

fn live_row() -> Value {
    json!({
        "session_id": "s-1",
        "id": "s-1",
        "title": "Fix the average",
        "source": "tui",
        "started_at": 1_792_700_000.5,
        "message_count": 4,
        "model": "minimax-m3.1-flash",
        "running": false,
    })
}

fn stored_row() -> Value {
    json!({
        "session_id": "s-9",
        "id": "root-9",
        "title": "Older work",
        "source": "cli",
        "started_at": 1_792_000_000.0,
        "message_count": 30,
        "running": false,
        "resolved_id": "s-9",
    })
}

fn row(profile: &str, id: &str, origin: RowOrigin, started_at: Option<f64>) -> DirectoryRow {
    DirectoryRow {
        key: RowKey {
            profile: profile.to_owned(),
            session_id: id.to_owned(),
        },
        title: format!("title {id}"),
        source: Some("tui".to_owned()),
        started_at,
        message_count: 1,
        running: false,
        model: None,
        origin,
    }
}

#[test]
fn row_from_list_row_reads_contract_fields() {
    let live = DirectoryRow::from_list_row("fixture", RowOrigin::Live, &live_row())
        .expect("the live row reads");
    assert_eq!(live.key.session_id, "s-1");
    assert_eq!(live.title, "Fix the average");
    assert_eq!(live.source.as_deref(), Some("tui"));
    assert_eq!(live.started_at, Some(1_792_700_000.5));
    assert_eq!(live.message_count, 4);
    assert_eq!(live.model.as_deref(), Some("minimax-m3.1-flash"));
    assert!(!live.running);

    let stored = DirectoryRow::from_list_row("fixture", RowOrigin::Stored, &stored_row())
        .expect("the stored row reads");
    assert_eq!(stored.key.session_id, "s-9");

    let mut untitled = live_row();
    untitled["title"] = Value::from("");
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &untitled)
            .expect("untitled reads")
            .title,
        ""
    );

    let mut null_model = live_row();
    null_model["model"] = Value::Null;
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &null_model)
            .expect("null model reads")
            .model,
        None
    );
    let mut missing_model = live_row();
    missing_model
        .as_object_mut()
        .expect("object")
        .remove("model");
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &missing_model)
            .expect("missing model reads")
            .model,
        None
    );

    let mut null_started = live_row();
    null_started["started_at"] = Value::Null;
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &null_started)
            .expect("null start reads")
            .started_at,
        None
    );
    let mut null_source = live_row();
    null_source["source"] = Value::Null;
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &null_source)
            .expect("null source reads")
            .source,
        None
    );

    let mut extra = live_row();
    extra["future_field"] = Value::from(1);
    assert!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &extra).is_some(),
        "an extra key is accepted"
    );

    let mut wrong_type = live_row();
    wrong_type["message_count"] = Value::from("four");
    assert_eq!(
        DirectoryRow::from_list_row("fixture", RowOrigin::Live, &wrong_type),
        None
    );
}

#[test]
fn merge_prefers_live_and_keys_by_profile_and_id() {
    let live = row("fixture", "s-1", RowOrigin::Live, Some(100.0));
    let stored_same = row("fixture", "s-1", RowOrigin::Stored, Some(50.0));
    let stored_other = row("default", "s-1", RowOrigin::Stored, Some(50.0));
    let merged = merge(vec![live], vec![stored_same, stored_other]);
    assert_eq!(merged.len(), 2);
    let kept = merged
        .iter()
        .find(|row| row.key.profile == "fixture")
        .expect("the fixture key is kept");
    assert_eq!(kept.origin, RowOrigin::Live);
    assert!(merged.iter().any(|row| row.key.profile == "default"));
}

#[test]
fn order_groups_then_newest_then_key() {
    let mut rows = vec![
        row("fixture", "s-old", RowOrigin::Stored, Some(10.0)),
        row("fixture", "s-live-old", RowOrigin::Live, Some(20.0)),
        row("fixture", "s-live-new", RowOrigin::Live, Some(30.0)),
        row("default", "s-nostart", RowOrigin::Live, None),
        row("fixture", "s-tie-b", RowOrigin::Live, Some(30.0)),
        row("fixture", "s-tie-a", RowOrigin::Live, Some(30.0)),
    ];
    order(&mut rows);
    let ids: Vec<&str> = rows.iter().map(|row| row.key.session_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "s-live-new",
            "s-tie-a",
            "s-tie-b",
            "s-live-old",
            "s-nostart",
            "s-old"
        ]
    );
}

#[test]
fn age_text_boundaries() {
    assert_eq!(format_age(None, 1_792_700_000.0), "—");
    let now = 1_792_700_000.0;
    let cases: [(f64, &str); 11] = [
        (59.9, "now"),
        (60.0, "1m"),
        (3_599.0, "59m"),
        (3_600.0, "1h"),
        (86_399.0, "23h"),
        (86_400.0, "1d"),
        (604_799.0, "6d"),
        (604_800.0, "1w"),
        (4_838_399.0, "7w"),
        (4_838_400.0, "2026-08-27"),
        (-5.0, "now"),
    ];
    for (ago, expected) in cases {
        assert_eq!(format_age(Some(now - ago), now), expected, "ago {ago}");
    }
    assert_eq!(short_model(None), "—");
    assert_eq!(short_model(Some("")), "—");
    assert_eq!(
        short_model(Some("ollama-cloud/minimax-m3.1-flash")),
        "minimax-m3.1-flash"
    );
}

#[test]
fn search_matches_fields_case_insensitively() {
    let mut found = row("fixture", "s-1", RowOrigin::Live, Some(10.0));
    found.model = Some("MiniMax-M3.1".to_owned());
    found.title = "unrelated".to_owned();
    found.source = Some("tui".to_owned());
    assert!(matches(&found, "minimax"));

    let mut hidden = row("other", "s-zzz", RowOrigin::Stored, None);
    hidden.title = "nothing here".to_owned();
    hidden.model = None;
    hidden.source = Some("tui".to_owned());
    assert!(!matches(&hidden, "root-9"));
}

fn row_strategy() -> impl Strategy<Value = DirectoryRow> {
    (
        prop_oneof![Just("default"), Just("fixture")],
        "[a-z0-9-]{1,12}",
        "[ -~]{0,24}",
        prop::option::of(1_700_000_000.0..1_800_000_000.0),
        prop::option::of("[a-z/.-]{0,24}"),
        prop::option::of("[a-z]{0,8}"),
        any::<bool>(),
        prop_oneof![Just(RowOrigin::Live), Just(RowOrigin::Stored)],
    )
        .prop_map(
            |(profile, id, title, started_at, model, source, running, origin)| DirectoryRow {
                key: RowKey {
                    profile: profile.to_owned(),
                    session_id: id,
                },
                title,
                source,
                started_at,
                message_count: 1,
                running,
                model,
                origin,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn filter_result_is_ordered_subset(
        mut rows in prop::collection::vec(row_strategy(), 0..32),
        query in prop::collection::vec(any::<char>(), 0..20).prop_map(|chars| chars.into_iter().collect::<String>()),
    ) {
        order(&mut rows);
        let before: Vec<String> = rows.iter().map(|row| format!("{:?}", row.key)).collect();
        let kept: Vec<String> = rows
            .iter()
            .filter(|row| matches(row, &query))
            .map(|row| format!("{:?}", row.key))
            .collect();
        let mut at = 0;
        for key in &kept {
            while at < before.len() && &before[at] != key {
                at += 1;
            }
            prop_assert!(at < before.len(), "filtered rows keep their order");
            at += 1;
        }
    }
}
