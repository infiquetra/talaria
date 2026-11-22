use crate::staffing_support;
use staffing_support::{budget, data, now, roster, table};
use talaria_workflow::staffing::{RosterReport, display_safe};
#[test]
fn roster_report_text_and_json() {
    assert_eq!(display_safe("a\u{1b}b\u{202e}c\u{2066}d"), "a?b?c?d");
    let mut t = table();
    t.providers.insert(
        "alpha-cloud".into(),
        talaria_workflow::staffing::ProviderLimits { max_in_flight: 4 },
    );
    let b = talaria_workflow::staffing::RunBudget {
        tokens: Some(100),
        money_usd: Some(1.0),
    };
    let report = RosterReport::build(&t, &roster(&data()), &b, now());
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["limits"]["providers"]["alpha-cloud"], 4);
    assert_eq!(value["budget"]["tokens"], 100);
    assert_eq!(value["budget"]["money_usd"], 1.0);
    assert!(!report.to_text().is_empty());
    let reference = talaria_workflow::staffing::RosterReport {
        profile: "fixture".into(), taken_at: "2026-10-22 16:00 UTC".into(),
        providers: vec![
            talaria_workflow::staffing::ProviderReport { slug: "ollama-cloud".into(), name: "ollama-cloud".into(), models: 12, model_ids: vec![], unavailable_models: vec![], availability: "ready".into(), warning: None },
            talaria_workflow::staffing::ProviderReport { slug: "example-cloud".into(), name: "example-cloud".into(), models: 8, model_ids: vec![], unavailable_models: vec![], availability: "limited until 16:30 UTC".into(), warning: None },
        ],
        tiers: vec![
            talaria_workflow::staffing::TierReport { tier: "survey".into(), route: "minimax-m3.1-flash on ollama-cloud".into(), price: "unknown".into(), status: "ok".into() },
            talaria_workflow::staffing::TierReport { tier: "judge".into(), route: "big-model on example-cloud (effort high)".into(), price: "$3.00/$15.00".into(), status: "warning: rate-limited until 16:30 UTC".into() },
        ],
        limits: talaria_workflow::staffing::LimitsReport { max_in_flight: 10, providers: std::collections::BTreeMap::from([("ollama-cloud".into(), 4)]) },
        budget: budget(), check: talaria_workflow::staffing::RosterCheck { problems: vec![], warnings: vec![] },
        current: None,
        estimate: "no estimate without a token budget; tier prices: survey unknown, judge $3.00/$15.00 per million tokens".into(),
    };
    assert_eq!(
        reference.to_text(),
        include_str!("../data/staffing/report.txt")
    );
}

fn two_providers() -> serde_json::Value {
    serde_json::json!({"model":"m-small","provider":"alpha-cloud","providers":[
        {"slug":"alpha-cloud","name":"Alpha","authenticated":true,
         "models":["m-small","m-large","m-retired"],"unavailable_models":["m-retired"]},
        {"slug":"beta-cloud","name":"Beta","authenticated":false,"models":["b-one","b-two"]}]})
}

#[test]
fn roster_envelope_lists_model_ids() {
    let report = RosterReport::build(&table(), &roster(&two_providers()), &budget(), now());
    let value = serde_json::to_value(&report).unwrap();
    let alpha = &value["providers"][0];
    assert_eq!(alpha["models"], 3);
    assert_eq!(
        alpha["model_ids"],
        serde_json::json!(["m-small", "m-large"])
    );
    assert_eq!(
        alpha["unavailable_models"],
        serde_json::json!(["m-retired"])
    );
    // A provider that is not set up keeps its count and lists no ids.
    let beta = &value["providers"][1];
    assert_eq!(beta["models"], 2);
    assert_eq!(beta["model_ids"], serde_json::json!([]));
    let text = report.to_text();
    assert!(text.contains("    models: m-small, m-large\n"), "{text}");
    assert!(text.contains("    unavailable: m-retired\n"), "{text}");
    assert!(!text.contains("b-one"), "{text}");
}

#[test]
fn roster_current_is_the_session_model_when_the_session_is_known() {
    let from_profile = RosterReport::build(&table(), &roster(&two_providers()), &budget(), now());
    let value = serde_json::to_value(&from_profile).unwrap();
    assert_eq!(
        value["current"],
        serde_json::json!({"model":"m-small","provider":"alpha-cloud","source":"profile"})
    );
    assert!(!from_profile.to_text().contains("this session"));

    let session = roster(&two_providers()).with_session_model("m-large", "alpha-cloud");
    let report = RosterReport::build(&table(), &session, &budget(), now());
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(
        value["current"],
        serde_json::json!({"model":"m-large","provider":"alpha-cloud","source":"session"})
    );
    assert!(
        report
            .to_text()
            .contains("Current model: m-large on alpha-cloud (this session)\n"),
        "{}",
        report.to_text()
    );
}
