mod staffing_support;
use serde_json::json;
use staffing_support::{budget, data, now, roster, table};
use talaria_workflow::staffing::check_tiers;
#[test]
fn tier_problems_and_warnings() {
    let t = table();
    let mut v = data();
    v["providers"] = json!([]);
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(!check.is_ok());
    assert_eq!(
        check.problems,
        ["tier 'survey': provider 'alpha-cloud' is not configured in profile 'fixture'"]
    );
    let mut v = data();
    v["providers"][0]["authenticated"] = json!(false);
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(!check.is_ok());
    assert!(check.problems[0].contains("not signed in"));
    let mut v = data();
    v["providers"][0]["models"] = json!([]);
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(!check.is_ok());
    assert!(check.problems[0].contains("does not offer"));
    let mut v = data();
    v["providers"][0]["unavailable_models"] = json!(["m-small"]);
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(!check.is_ok());
    assert!(check.problems[0].contains("not available"));
    let mut v = data();
    v["providers"][0]["limit"] = json!({"scope":"account","resets_at":null});
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(check.is_ok());
    assert!(check.warnings[0].contains("rate-limited"));
}
