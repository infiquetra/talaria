use crate::staffing_support;
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
        "$0.00 to $0.00, not counting cache reads"
    );
    let mut priced = data();
    priced["providers"][0]["pricing"] =
        json!({"m-small":{"input":"$3.00","output":"$15.00","free":false}});
    assert_eq!(
        estimate_run_cost(&t, &roster(&priced), Some(2_000_000)).to_string(),
        "$6.00 to $30.00, not counting cache reads"
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
        "$6.00 to $30.00, not counting cache reads, plus 1 unpriced tiers (unknown)"
    );
    let text = estimate_run_cost(&t, &roster(&priced), None).to_string();
    assert!(text.contains("survey") && text.contains("$3.00/$15.00"));
}

#[test]
fn no_tiers_says_why_there_is_no_price_and_does_not_promise_tiers_would_help() {
    // A script that names models directly leaves the table with no tiers.
    let mut t = table();
    t.tiers.clear();
    // Hermes lists no price for any model: the cost of directly named models will be unknown, and
    // the text must not suggest that setting tiers would change that.
    let text = estimate_run_cost(&t, &roster(&data()), None).to_string();
    assert_eq!(
        text,
        "no estimate without a token budget; no tiers are set, so there are no tier prices. Hermes lists no price for any model on the configured providers, so the cost of agents that name a model directly will be unknown"
    );
    assert!(!text.contains("per million"), "{text}");
    assert!(!text.contains("no price list"), "{text}");
    // Hermes does list a price: a directly named model is priced when it is on that list.
    let mut priced = data();
    priced["providers"][0]["pricing"] =
        json!({"m-small":{"input":"$3.00","output":"$15.00","free":false}});
    let text = estimate_run_cost(&t, &roster(&priced), None).to_string();
    assert_eq!(
        text,
        "no estimate without a token budget; no tiers are set, so there are no tier prices. An agent that names a model directly is priced only if Hermes lists a price for that model on its provider"
    );
}
