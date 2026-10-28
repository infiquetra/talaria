#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn budget_reads_backend_snapshot() {
    let backend = backend(
        r#"{"default":{"text":"ok"},"rules":[],"budget":{"tokens_total":3000,"tokens_per_agent":1000,"money_total":2.5,"money_per_agent":0.5}}"#,
    );
    let body = "const before=[budget.total,budget.spent(),budget.remaining(),budget.money.total,budget.money.spent(),budget.money.remaining()]; await agent('x'); return [before,[budget.total,budget.spent(),budget.remaining(),budget.money.total,budget.money.spent(),budget.money.remaining()]];";
    let (outcome, _) = run(body, backend).await;
    assert_eq!(
        result(outcome),
        json!([
            [3000, 0, 3000, 2.5, 0, 2.5],
            [3000, 1000, 2000, 2.5, 0.5, 2]
        ])
    );
    let (outcome, _) = run("return [budget.total, budget.remaining() === Infinity, budget.money.total, budget.money.remaining() === Infinity];", default_backend()).await;
    assert_eq!(result(outcome), json!([null, true, null, true]));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn exhausted_budget_rejects_agent() {
    for rules in [
        r#"{"default":{"text":"ok"},"rules":[],"budget":{"tokens_total":2000,"tokens_per_agent":1000}}"#,
        r#"{"default":{"text":"ok"},"rules":[],"budget":{"money_total":2.0,"money_per_agent":1.0}}"#,
    ] {
        let backend = backend(rules);
        let body = "await agent('one'); await agent('two'); let name; try {await agent('three')} catch(e) {name=e.name} return [name, await parallel([() => agent('four')])];";
        let (outcome, _) = run(body, backend.clone()).await;
        assert_eq!(result(outcome), json!(["BudgetExhaustedError", [null]]));
        assert_eq!(backend.requests().len(), 2);
    }
}
