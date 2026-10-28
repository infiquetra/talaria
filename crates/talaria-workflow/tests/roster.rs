mod staffing_support;
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use staffing_support::{budget, now, roster, table};
use talaria_workflow::staffing::{Availability, LimitScope, PriceValue};
#[test]
fn reads_the_recorded_model_options() {
    let text = std::fs::read_to_string("../../fixtures/frames/p5-04-model-options.jsonl").unwrap();
    let frames: Vec<Value> = text
        .lines()
        .map(|x| serde_json::from_str(x).unwrap())
        .collect();
    let request = frames
        .iter()
        .find(|x| x["frame"]["method"] == "model.options")
        .unwrap();
    let reply = frames
        .iter()
        .find(|x| x["dir"] == "in" && x["frame"]["id"] == request["frame"]["id"])
        .unwrap();
    let r = roster(&reply["frame"]["result"]);
    assert!(!r.providers().is_empty());
    let p = r
        .providers()
        .iter()
        .find(|p| p.slug == "ollama-cloud")
        .unwrap();
    assert!(p.authenticated);
    assert!(p.models.iter().any(|m| m == "minimax-m3.1-flash"));
    assert!(!r.current().0.is_empty());
    if let Some(price) = r.price("ollama-cloud", "minimax-m3.1-flash") {
        assert_eq!(price.input, PriceValue::Unknown);
        assert_eq!(price.output, PriceValue::Unknown);
    }
}
#[test]
fn availability_table() {
    let time = now();
    let future = time + Duration::minutes(5);
    let cases = [
        ("a1", Availability::Missing),
        ("a2", Availability::Unauthenticated),
        ("a3", Availability::NotOffered),
        ("a4", Availability::Unavailable),
        (
            "a5",
            Availability::Limited {
                until: None,
                scope: LimitScope::Account,
            },
        ),
        ("a5-past", Availability::Ready),
        (
            "a6",
            Availability::Limited {
                until: Some(future),
                scope: LimitScope::Model,
            },
        ),
        (
            "a7",
            Availability::Limited {
                until: Some(future),
                scope: LimitScope::Account,
            },
        ),
        (
            "a7-unknown",
            Availability::Limited {
                until: None,
                scope: LimitScope::Account,
            },
        ),
        ("a7-ready", Availability::Ready),
        (
            "a8",
            Availability::Limited {
                until: None,
                scope: LimitScope::Account,
            },
        ),
        (
            "a9",
            Availability::NearLimit {
                remaining_percent: 15,
                window: "week".into(),
            },
        ),
        ("a10", Availability::Ready),
    ];
    for (name, expected) in cases {
        let p = format!("tests/data/staffing/{name}.json");
        let value: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
        assert_eq!(
            roster(&value).availability("alpha-cloud", "m-small", time),
            expected,
            "{name}"
        );
    }
    let _ = Utc;
}
#[test]
fn snapshot_has_no_private_fields() {
    let mut v = staffing_support::data();
    v["providers"][0]["api_url"] = json!("private-endpoint-marker");
    v["providers"][0]["key_env"] = json!("private-key-marker");
    v["providers"][0]["warning"] = json!("private-warning-marker");
    v["providers"][0]["usage"] = json!({"accounts":[{"id":"private-id-marker","label":"private-label-marker","state":"ready"}]});
    let snap = roster(&v).snapshot_with(&table(), &budget()).to_string();
    for marker in [
        "private-endpoint-marker",
        "private-key-marker",
        "private-warning-marker",
        "private-id-marker",
        "private-label-marker",
    ] {
        assert!(!snap.contains(marker));
    }
}
