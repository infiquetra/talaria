mod staffing_support;
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
    let _ = budget();
}
