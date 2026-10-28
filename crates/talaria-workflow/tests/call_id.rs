#![cfg(feature = "mock")]
mod common;
use common::*;
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use talaria_workflow::engine::EngineEvent;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn digest_is_stable_and_ignores_label_and_phase() {
    let backend = default_backend();
    let body = "await agent('review',{effort:'high',label:'a',phase:'A'}); await agent('review',{effort:'high',label:'b',phase:'B',stallMs:1}); await agent('review',{effort:'low'}); await agent('review',{effort:undefined}); await agent('review',{});";
    let (outcome, _) = run(body, backend.clone()).await;
    result(outcome);
    let requests = backend.requests();
    let canonical = r#"{"effort":"high","prompt":"review","v":1,"workflow_path":[]}"#;
    assert_eq!(
        canonical,
        "{\"effort\":\"high\",\"prompt\":\"review\",\"v\":1,\"workflow_path\":[]}"
    );
    assert_eq!(
        requests[0].call_id.digest,
        "1d565d2e097a8f463f386770b57033419262c972f66c01796f15c8a4c4f2f9aa"
    );
    assert_eq!(requests[0].call_id.digest, requests[1].call_id.digest);
    assert_ne!(requests[0].call_id.digest, requests[2].call_id.digest);
    assert_eq!(requests[3].call_id.digest, requests[4].call_id.digest);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn identical_calls_get_occurrence_numbers() {
    let backend = default_backend();
    let (outcome, _) = run(
        "await agent('same'); await agent('other'); await agent('same'); await agent('same');",
        backend.clone(),
    )
    .await;
    result(outcome);
    let requests = backend.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.prompt == "same")
            .map(|r| r.call_id.occurrence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(requests[1].call_id.occurrence, 0);
}

#[test]
fn delivery_order_is_deterministic() {
    let mut runner = TestRunner::new(Config {
        cases: 64,
        max_shrink_iters: 0,
        failure_persistence: None,
        ..Config::default()
    });
    runner.run(&prop::collection::vec(0u64..4, 5), |delays| {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().start_paused(true).build().unwrap();
        runtime.block_on(async {
            let rules = serde_json::json!({"rules": delays.iter().enumerate().map(|(i, d)| serde_json::json!({"match":{"prompt_equals":i.to_string()},"outcome":{"text":i.to_string()},"delay_ms":d * 10})).collect::<Vec<_>>()});
            let script = "return await parallel([0,1,2,3,4].map(i => () => agent(String(i))));";
            let first = backend(&rules.to_string());
            let second = backend(&rules.to_string());
            let (_, events1) = run(script, first.clone()).await;
            let (_, events2) = run(script, second).await;
            prop_assert_eq!(&events1, &events2);
            let finished = events1.iter().filter_map(|event| match event { EngineEvent::AgentFinished {call_id, completion_seq: Some(seq), ..} => Some((call_id, *seq)), _ => None }).collect::<Vec<_>>();
            let requests = first.requests();
            let mut order = (0..delays.len()).collect::<Vec<_>>();
            order.sort_by_key(|i| (delays[*i], *i));
            for (seq, i) in order.into_iter().enumerate() { prop_assert_eq!(finished[seq], (&requests[i].call_id, seq as u64)); }
            Ok(())
        })
    }).unwrap();
}
