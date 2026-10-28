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
    let mut v = data();
    v["providers"][0]["usage"] =
        json!({"windows":[{"label":"week","scope":"account","used_percent":85,"resets_at":null}]});
    let check = check_tiers(&t, &roster(&v), &budget(), now());
    assert!(check.is_ok());
    assert_eq!(
        check.warnings,
        ["tier 'survey': 'alpha-cloud' has 15% of 'week' left"]
    );
    let b = talaria_workflow::staffing::RunBudget {
        tokens: None,
        money_usd: Some(1.0),
    };
    let check = check_tiers(&t, &roster(&data()), &b, now());
    assert!(check.is_ok());
    assert_eq!(
        check.warnings,
        ["tier 'survey': 'm-small' has no price; the money budget cannot see its cost"]
    );
}
