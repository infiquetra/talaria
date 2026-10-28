mod staffing_support;
use serde_json::json;
use staffing_support::{data, roster, table};
use talaria_workflow::staffing::estimate_run_cost;
#[test]
fn range_unknown_and_free() {
    let t = table();
    let mut free = data();
    free["providers"][0]["pricing"] =
        json!({"m-small":{"input":"free","output":"free","free":true}});
    assert_eq!(
        estimate_run_cost(&t, &roster(&free), Some(2_000_000)).to_string(),
        "$0.00 to $0.00"
    );
    let mut priced = data();
    priced["providers"][0]["pricing"] =
        json!({"m-small":{"input":"$3.00","output":"$15.00","free":false}});
    assert_eq!(
        estimate_run_cost(&t, &roster(&priced), Some(2_000_000)).to_string(),
        "$6.00 to $30.00"
    );
    assert_eq!(
        estimate_run_cost(&t, &roster(&data()), Some(2_000_000)).to_string(),
        "unknown"
    );
    assert!(
        estimate_run_cost(&t, &roster(&priced), None)
            .to_string()
            .contains("no estimate without a token budget")
    );
    let mut mixed = table();
    mixed.tiers.insert(
        "judge".into(),
        talaria_workflow::staffing::TierSpec {
            model: "m-large".into(),
            provider: "alpha-cloud".into(),
            effort: None,
            note: None,
        },
    );
    assert_eq!(
        estimate_run_cost(&mixed, &roster(&priced), Some(2_000_000)).to_string(),
        "$6.00 to $30.00, plus 1 unpriced tiers (unknown)"
    );
    let text = estimate_run_cost(&t, &roster(&priced), None).to_string();
    assert!(text.contains("survey") && text.contains("$3.00/$15.00"));
}
