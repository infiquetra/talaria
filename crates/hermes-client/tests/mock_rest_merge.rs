#[path = "support/mock_rest.rs"]
mod mock_rest;

use serde_json::json;

#[test]
fn mock_deep_merge_matches_hermes_rule() {
    let cases = [
        (
            json!({"a":{"b":1,"c":2}}),
            json!({"a":{"b":3}}),
            json!({"a":{"b":3,"c":2}}),
        ),
        (
            json!({"a":{"b":1}}),
            json!({"a":null}),
            json!({"a":{"b":1}}),
        ),
        (json!({"a":{"b":1}}), json!({"a":[]}), json!({"a":[]})),
        (json!({"a":[1,2]}), json!({"a":[3]}), json!({"a":[3]})),
        (json!({"a":2}), json!({"a":null}), json!({"a":null})),
        (
            json!({"a":2}),
            json!({"a":{"b":true}}),
            json!({"a":{"b":true}}),
        ),
        (json!({}), json!({"a":null}), json!({"a":null})),
    ];
    for (mut base, patch, expected) in cases {
        mock_rest::deep_merge(&mut base, &patch);
        assert_eq!(base, expected);
    }
}
