use talaria_workflow::schedule::{MoneySource, Spend};
#[test]
fn money_text_reports_unknown_turns() {
    let mut value = Spend {
        tokens: 10,
        money_usd: 0.0,
        money_source: MoneySource::None,
        money_unknown_turns: 1,
        tokens_unknown_turns: 0,
    };
    assert_eq!(value.money_text(), "unknown (1 unpriced turns)");
    value.money_usd = 0.01;
    value.money_source = MoneySource::Hermes;
    assert_eq!(value.money_text(), "$0.01 plus 1 unpriced turns");
    value.money_unknown_turns = 0;
    assert_eq!(value.money_text(), "$0.01");
    value.money_usd = 0.0;
    assert_eq!(value.money_text(), "$0.00");
}

mod scheduler_support;
use scheduler_support::{Harness, launch, no_budget, request, roster_data};
use serde_json::json;
use talaria_workflow::schedule::MoneySource as Source;
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn session_cost_is_cumulative() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let usage = |n: f64| json!({"input_tokens":1,"output_tokens":1,"session_cost":{"estimated_cost_usd":n,"cost_status":"actual"}});
    h.settle(&q, &l, 0, 0, None, usage(0.01));
    h.settle(&q, &l, 0, 1, None, usage(0.02));
    let (s, _) = h.start(no_budget()).await;
    assert!((s.status().spend.money_usd - 0.02).abs() < 1e-9);
    drop(s);
    let (resumed, _) = h.start(no_budget()).await;
    assert!((resumed.status().spend.money_usd - 0.02).abs() < 1e-9);
    drop(resumed);
    let reset = Harness::new(10, roster_data()).await;
    reset.settle(&q, &l, 0, 0, None, usage(0.03));
    reset.settle(&q, &l, 0, 1, None, usage(0.01));
    let (after_reset, _) = reset.start(no_budget()).await;
    assert!(
        (after_reset.status().spend.money_usd - 0.04).abs() < 1e-9,
        "{:?}",
        after_reset.status().spend
    );
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_cost_is_never_a_known_zero() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    h.settle(&q,&l,0,0,None,json!({"input_tokens":1000,"output_tokens":1000,"session_cost":{"estimated_cost_usd":0.0,"cost_status":"unknown"}}));
    let (s, _) = h.start(no_budget()).await;
    let spend = s.status().spend;
    assert!(spend.money_usd > 0.0);
    assert_eq!(spend.money_source, Source::Pricing);
    assert_eq!(spend.money_unknown_turns, 0);
    let q2 = request(1);
    let unpriced = launch("fixture", "beta");
    h.settle(&q2,&unpriced,0,0,None,json!({"input_tokens":1,"output_tokens":1,"session_cost":{"estimated_cost_usd":0.0,"cost_status":"unknown"}}));
    h.advance(1).await;
    let _ = s.budget();
    let spend = s.status().spend;
    assert_eq!(spend.money_unknown_turns, 1);
    assert!(spend.money_text().contains("unpriced turns"));
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn resume_restores_spend_halving_and_prices() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    h.settle(
        &q,
        &l,
        0,
        0,
        Some("rate_limit"),
        json!({"input_tokens":1000,"output_tokens":1000}),
    );
    let (first, _) = h.start(no_budget()).await;
    let saved = first.status();
    assert_eq!(saved.global_width, 5);
    assert!(saved.spend.money_usd > 0.0);
    drop(first);
    let mut new_roster = roster_data();
    new_roster["providers"][0]["pricing"]["m"]["input"] = "$99".into();
    h.source.push("fixture", Ok(new_roster));
    h.cache.refresh("fixture", true).await.unwrap();
    let (changed, mut events) = h.start(no_budget()).await;
    let same = changed.status();
    assert_eq!(same.global_width, saved.global_width);
    assert_eq!(same.spend, saved.spend);
    assert!(events.0.try_recv().is_err());
}
