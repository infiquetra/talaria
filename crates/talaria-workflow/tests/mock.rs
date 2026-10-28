#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::mock::{MockBackend, MockBudget, MockMatch, MockOutcome, MockRule};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn malformed_rules_are_refused() {
    let cases = [
        (
            r#"{"rules":[{"match":{"unknown":"x"},"outcome":{"text":"x"}}]}"#,
            "unknown",
            true,
        ),
        (r#"{"unexpected":1,"rules":[]}"#, "unexpected", false),
        (
            r#"{"rules":[{"match":{"prompt_equals":"x"},"outcome":{"text":"x","failed":"bad"}}]}"#,
            "outcome",
            true,
        ),
        (
            r#"{"rules":[{"match":{"prompt_equals":"x"},"outcome":{"text":"x"},"delay_ms":-1}]}"#,
            "delay",
            true,
        ),
        (
            r#"{"rules":[],"budget":{"money_per_agent":-1}}"#,
            "money",
            false,
        ),
    ];
    for (rules, field, has_rule) in cases {
        let error = MockBackend::from_json(rules).unwrap_err().to_string();
        assert!(error.contains(field), "{error}");
        if has_rule {
            assert!(error.contains('0'), "{error}");
        }
    }
    let from_json = backend(
        r#"{"default":{"text":"fallback"},"rules":[{"match":{"prompt_equals":"match"},"outcome":{"object":{"n":1}},"delay_ms":25}],"budget":{"tokens_total":5000,"tokens_per_agent":100,"money_total":1.0,"money_per_agent":0.1}}"#,
    );
    let built = Arc::new(
        MockBackend::builder()
            .default_outcome(MockOutcome::Text("fallback".into()))
            .rule(
                MockRule::new(
                    MockMatch {
                        prompt_equals: Some("match".into()),
                        ..MockMatch::default()
                    },
                    MockOutcome::Object(json!({"n":1})),
                )
                .delay_ms(25),
            )
            .budget(MockBudget {
                tokens_total: Some(5000),
                tokens_per_agent: 100,
                money_total: Some(1.0),
                money_per_agent: 0.1,
            })
            .build(),
    );
    let script =
        "return [await agent('match'),await agent('other'),budget.spent(),budget.money.spent()];";
    let (left, events_left) = run(script, from_json.clone()).await;
    let (right, events_right) = run(script, built.clone()).await;
    assert_eq!(result(left), json!([{"n":1},"fallback",200,0.2]));
    assert_eq!(result(right), json!([{"n":1},"fallback",200,0.2]));
    assert_eq!(events_left, events_right);
    assert_eq!(from_json.calls(), built.calls());
}
