#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::engine::{EngineEvent, MapResolver, ReplayLog};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn replay_keeps_results_under_reversed_completion_order() {
    let body = "return await pipeline(['A','B'], async (_,item) => {await agent('prepare '+item); return item}, async (_,item) => await agent('choose'), async (choice,item) => [item,choice,await agent('synthesis '+item+' '+choice)]);";
    let rules = |a, b| {
        format!(
            r#"{{"default":{{"text":"ok"}},"rules":[{{"match":{{"prompt_equals":"prepare A"}},"outcome":{{"text":"ready"}},"delay_ms":{a}}},{{"match":{{"prompt_equals":"prepare B"}},"outcome":{{"text":"ready"}},"delay_ms":{b}}},{{"match":{{"prompt_equals":"choose","occurrence":0}},"outcome":{{"text":"X"}}}},{{"match":{{"prompt_equals":"choose","occurrence":1}},"outcome":{{"text":"Y"}}}}]}}"#
        )
    };
    let first = backend(&rules(200, 100));
    let (outcome, events) = run(body, first.clone()).await;
    let original = result(outcome);
    assert_eq!(original, json!([["A", "Y", "ok"], ["B", "X", "ok"]]));
    let log = replay(&events, &first);
    assert_eq!(log.entries.len(), 6);
    let second = backend(&rules(100, 200));
    let mut cfg = config();
    cfg.replay = Some(log);
    let (outcome, events) = execute(
        &source(body),
        cfg,
        second.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), original);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
            .count(),
        6
    );
    assert!(!events.iter().any(|e| matches!(
        e,
        EngineEvent::AgentStarted {
            replayed: false,
            ..
        }
    )));
    assert!(second.requests().is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn live_outcomes_wait_and_divergence_releases_them() {
    let body =
        "return await parallel([() => agent('one'),() => agent('two'),() => agent('three')]);";
    let rules = r#"{"rules":[{"match":{"prompt_equals":"one"},"outcome":{"text":"one"},"delay_ms":10},{"match":{"prompt_equals":"two"},"outcome":{"text":"two"},"delay_ms":20},{"match":{"prompt_equals":"three"},"outcome":{"text":"three"},"delay_ms":30},{"match":{"prompt_equals":"different"},"outcome":{"text":"changed"}}]}"#;
    let first = backend(rules);
    let (outcome, events) = run(body, first.clone()).await;
    let fresh = result(outcome);
    assert_eq!(fresh, json!(["one", "two", "three"]));
    let full = replay(&events, &first);
    let middle_id = full.entries[1].call_id.clone();
    let later_id = full.entries[2].call_id.clone();
    let partial = ReplayLog {
        entries: vec![full.entries[0].clone(), full.entries[2].clone()],
    };
    let fast_live = rules.replace("\"delay_ms\":20", "\"delay_ms\":0");
    let live = backend(&fast_live);
    let mut cfg = config();
    cfg.replay = Some(partial);
    let (outcome, events) = execute(
        &source(body),
        cfg,
        live.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), fresh);
    assert_eq!(
        live.requests()
            .iter()
            .map(|r| r.prompt.as_str())
            .collect::<Vec<_>>(),
        vec!["two"]
    );
    let earlier = events
        .iter()
        .position(|e| matches!(e, EngineEvent::AgentFinished {call_id,..} if *call_id == later_id))
        .unwrap();
    let later = events
        .iter()
        .position(|e| matches!(e, EngineEvent::AgentFinished {call_id,..} if *call_id == middle_id))
        .unwrap();
    assert!(earlier < later);

    let live = backend(rules);
    let mut cfg = config();
    cfg.replay = Some(full.clone());
    let edited = body.replace("agent('two')", "agent('different')");
    let (outcome, events) = execute(
        &source(&edited),
        cfg,
        live.clone(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), json!(["one", "changed", "three"]));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, EngineEvent::ReplayDiverged { at_seq: 1 }))
    );
    assert_eq!(
        live.requests()
            .iter()
            .map(|r| r.prompt.as_str())
            .collect::<Vec<_>>(),
        vec!["different"]
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
            .count(),
        2
    );

    let with_extra = "const values=await parallel([() => agent('one'),() => agent('two'),() => agent('three')]); try {await agent('extra')} catch(e) {values.push(e.name)} return values;";
    for (budget, max_agents, expected) in [
        (r#"{"tokens_total":0}"#, 1000, "BudgetExhaustedError"),
        (r#"{"tokens_total":10000}"#, 3, "AgentLimitError"),
    ] {
        let rules = format!(r#"{{"default":{{"text":"ok"}},"rules":[],"budget":{budget}}}"#);
        let live = backend(&rules);
        let mut cfg = config();
        cfg.replay = Some(full.clone());
        cfg.max_agents = max_agents;
        let (outcome, events) = execute(
            &source(with_extra),
            cfg,
            live.clone(),
            Arc::new(MapResolver::new()),
        )
        .await;
        assert_eq!(result(outcome), json!(["one", "two", "three", expected]));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, EngineEvent::AgentStarted { replayed: true, .. }))
                .count(),
            3
        );
        assert!(live.requests().is_empty());
    }
}
