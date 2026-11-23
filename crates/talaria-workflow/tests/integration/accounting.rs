use talaria_workflow::journal::AdmissionGate;
use talaria_workflow::schedule::{MoneySource, Spend};
#[test]
fn money_text_reports_unknown_turns() {
    let mut value = Spend {
        tokens: 10,
        cache_read_tokens: 0,
        cache_unknown_turns: 0,
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

use crate::scheduler_support;
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

fn cache_turn(total: u64, running_cache: u64) -> serde_json::Value {
    json!({"total_tokens": total, "session_cache_read_tokens": running_cache})
}
fn budget_of(tokens: u64) -> talaria_workflow::staffing::RunBudget {
    talaria_workflow::staffing::RunBudget {
        tokens: Some(tokens),
        money_usd: None,
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn cache_reads_are_kept_apart_from_the_tokens_the_budget_counts() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    // Two turns of one agent: Hermes gives each turn's tokens, and the session's running total
    // of cache reads, so the second turn's cache reads are the difference.
    h.settle(&q, &l, 0, 0, None, cache_turn(1000, 800));
    h.settle(&q, &l, 0, 1, None, cache_turn(1500, 1800));
    let (s, _) = h.start(no_budget()).await;
    let spend = s.status().spend;
    assert_eq!(spend.tokens, 200 + 500);
    assert_eq!(spend.cache_read_tokens, 1800);
    assert_eq!(spend.cache_unknown_turns, 0);
    assert_eq!(s.budget().tokens_spent, 700);
    // A resumed run folds the same journal to the same split.
    drop(s);
    let (resumed, _) = h.start(no_budget()).await;
    assert_eq!(resumed.status().spend, spend);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_budget_the_old_counting_exhausted_survives_when_most_tokens_are_cache_reads() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    // 10,000 tokens in all, 9,500 of them cache reads: counting everything would pass 1,000.
    h.settle(&q, &l, 0, 0, None, cache_turn(10_000, 9_500));
    let (s, _) = h.start(budget_of(1000)).await;
    assert_eq!(s.budget().tokens_spent, 500);
    assert_eq!(s.budget_stop(), None);
    drop(s);
    // The same turn with no split is counted whole, so that budget is spent.
    let whole = Harness::new(10, roster_data()).await;
    whole.settle(&q, &l, 0, 0, None, json!({"total_tokens": 10_000}));
    let (s, _) = whole.start(budget_of(1000)).await;
    assert_eq!(s.budget().tokens_spent, 10_000);
    assert!(s.budget_stop().is_some());
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_journal_without_cache_reads_loads_and_says_they_are_unknown() {
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    // How every turn was journaled before cache reads were kept: tokens only.
    h.settle(
        &q,
        &l,
        0,
        0,
        None,
        json!({"input_tokens": 900, "output_tokens": 100, "total_tokens": 1000}),
    );
    h.settle(
        &q,
        &l,
        0,
        1,
        None,
        json!({"input_tokens": 400, "output_tokens": 100}),
    );
    h.settle(&request(1), &l, 0, 0, None, json!({}));
    let (s, _) = h.start(no_budget()).await;
    let spend = s.status().spend;
    assert_eq!(spend.tokens, 1500);
    assert_eq!(spend.cache_read_tokens, 0);
    assert_eq!(spend.cache_unknown_turns, 2);
    assert_eq!(spend.tokens_unknown_turns, 1);
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_live_reading_before_the_turn_settles_counts_without_its_cache_reads() {
    use talaria_workflow::{journal::Record, tokens::Reading};
    let h = Harness::new(10, roster_data()).await;
    let q = request(0);
    let l = launch("fixture", "alpha");
    let key = talaria_workflow::journal::journal_key(&q.call_id, &l);
    h.append(Record::CallIntent {
        key: key.clone(),
        attempt: 0,
        call_id: q.call_id.clone(),
        label: q.label.clone(),
        phase: None,
        workflow_path: vec![],
        launch: l.clone(),
        request_id: "request-0".into(),
    });
    h.append(Record::SessionBound {
        key,
        attempt: 0,
        session_id: "s-live".into(),
        stored_session_id: "s-live".into(),
    });
    let (s, _) = h.start(budget_of(1000)).await;
    // Hermes folds cache reads into the total and also reports them on their own.
    s.observe_usage(
        "s-live",
        Reading {
            total: 9000,
            cache_read: Some(8500),
        },
    );
    assert_eq!(s.budget().tokens_spent, 500);
    assert_eq!(s.status().spend.cache_read_tokens, 8500);
    assert_eq!(s.budget_stop(), None);
    // A reading that does not say is counted whole.
    s.observe_usage(
        "s-bare",
        Reading {
            total: 700,
            cache_read: None,
        },
    );
    assert_eq!(s.budget().tokens_spent, 1200);
    assert!(s.budget_stop().is_some());
}
