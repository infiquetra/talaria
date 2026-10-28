mod staffing_support;
use serde_json::json;
use staffing_support::{data, roster, table};
use talaria_workflow::staffing::estimate_run_cost;
#[test]
fn range_unknown_and_free() {
    let t = table();
    let mut free = data();
    free["providers"][0]["pricing"] = json!({"m-small":{"free":true}});
    assert_eq!(
        estimate_run_cost(&t, &roster(&free), Some(2_000_000)).to_string(),
        "$0.00 to $0.00"
    );
    let mut priced = data();
    priced["providers"][0]["pricing"] = json!({"m-small":{"input":"$3.00","output":"$15.00"}});
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
}
