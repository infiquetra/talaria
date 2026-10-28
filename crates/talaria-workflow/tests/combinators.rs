#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use talaria_workflow::engine::EngineEvent;
use tokio::time::{Duration, Instant};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn parallel_is_a_barrier_and_concurrent() {
    let backend = backend(
        r#"{"rules":[
      {"match":{"prompt_equals":"0"},"outcome":{"text":"0"},"delay_ms":500},
      {"match":{"prompt_equals":"1"},"outcome":{"text":"1"},"delay_ms":100},
      {"match":{"prompt_equals":"2"},"outcome":{"text":"2"},"delay_ms":300},
      {"match":{"prompt_equals":"3"},"outcome":{"text":"3"},"delay_ms":200},
      {"match":{"prompt_equals":"4"},"outcome":{"text":"4"},"delay_ms":400}]}"#,
    );
    let start = Instant::now();
    let (outcome, _) = run(
        "return await parallel([0,1,2,3,4].map(i => () => agent(String(i))));",
        backend.clone(),
    )
    .await;
    assert_eq!(result(outcome), json!(["0", "1", "2", "3", "4"]));
    assert_eq!(backend.max_in_flight(), 5);
    assert_eq!(start.elapsed(), Duration::from_millis(500));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn parallel_never_rejects() {
    let backend =
        backend(r#"{"rules":[{"match":{"prompt_equals":"bad"},"outcome":{"failed":"failed"}}]}"#);
    let body = "let n = 0; const slots = await parallel([() => {throw Error('x')}, () => Promise.reject(Error('x')), () => agent('bad')]); let errors=[]; for (const f of [() => parallel('bad'), () => parallel([() => {n++;}, 1])]) {try {await f()} catch(e) {errors.push(e.name)}} return [slots, errors, n];";
    let (outcome, _) = run(body, backend.clone()).await;
    assert_eq!(
        result(outcome),
        json!([
            [null, null, null],
            ["WorkflowOptionError", "WorkflowOptionError"],
            0
        ])
    );
    assert_eq!(backend.requests().len(), 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pipeline_has_no_barrier() {
    let backend = backend(
        r#"{"default":{"text":"ok"},"rules":[{"match":{"prompt_equals":"first 0"},"outcome":{"text":"0"},"delay_ms":100},{"match":{"prompt_equals":"first 1"},"outcome":{"text":"1"},"delay_ms":1000}]}"#,
    );
    let body = "return await pipeline([0,1], async (_, item) => await agent('first '+item), async (_, item) => await agent('second '+item));";
    let (outcome, events) = run(body, backend.clone()).await;
    assert_eq!(result(outcome), json!(["ok", "ok"]));
    let requests = backend.requests();
    let first_one = requests.iter().find(|r| r.prompt == "first 1").unwrap();
    let second_zero = events.iter().position(|e| matches!(e, EngineEvent::AgentStarted {request, ..} if request.prompt == "second 0")).unwrap();
    let first_one_finished = events.iter().position(|e| matches!(e, EngineEvent::AgentFinished {call_id, ..} if *call_id == first_one.call_id)).unwrap();
    assert!(second_zero < first_one_finished);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pipeline_stage_arguments_and_drops() {
    let body = "const seen=[]; const values=await pipeline(['a','b','c'], (prev,item,i)=>{seen.push([1,prev,item,i]); if(i===0) throw Error('drop'); return i===1 ? null : prev+'!';}, (prev,item,i)=>{seen.push([2,prev,item,i]); return [prev,item,i];}); return {values,seen};";
    let (outcome, _) = run(body, default_backend()).await;
    let value = result(outcome);
    assert_eq!(
        value["values"],
        json!([null, [null, "b", 1], ["c!", "c", 2]])
    );
    let seen = value["seen"].as_array().unwrap();
    for entry in [
        json!([1, "a", "a", 0]),
        json!([1, "b", "b", 1]),
        json!([1, "c", "c", 2]),
        json!([2, null, "b", 1]),
        json!([2, "c!", "c", 2]),
    ] {
        assert!(seen.contains(&entry), "missing {entry}");
    }
    assert_eq!(seen.len(), 5);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn item_limit() {
    let mut cfg = config();
    cfg.max_items_per_call = 4;
    let body = "const names=[]; for (const f of [() => parallel(Array(5).fill(() => 1)), () => pipeline([1,2,3,4,5], x => x)]) {try {await f()} catch(e) {names.push(e.name)}} return [names, await parallel(Array(4).fill(() => 1)), await pipeline([1,2,3,4], x => x)];";
    let (outcome, _) = execute(
        &source(body),
        cfg,
        default_backend(),
        std::sync::Arc::new(talaria_workflow::engine::MapResolver::new()),
    )
    .await;
    assert_eq!(
        result(outcome),
        json!([
            ["ItemLimitError", "ItemLimitError"],
            [1, 1, 1, 1],
            [1, 2, 3, 4]
        ])
    );
}
