#![cfg(feature = "fake-gateway")]
mod support;
use hermes_client::CallError;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};
use support::live_port::{self, Capture, LivePort};

fn unique(label: &str) -> String {
    format!(
        "fixture-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
fn create(id: &str, model: &str) -> Value {
    json!({"profile":"fixture","request_id":id,"source":"tool","hidden":true,"model":model,"provider":"ollama-cloud","toolsets":["search"]})
}
fn submit(session: &str, id: &str, text: &str) -> Value {
    json!({"profile":"fixture","session_id":session,"submission_id":id,"text":text,"finite":true})
}
async fn make_session(port: &LivePort, id: &str) -> String {
    port.call("session.create", create(id, "minimax-m3.1-flash"))
        .await
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
async fn admit(port: &LivePort, session: &str, id: &str, text: &str) -> Value {
    let params = submit(session, id, text);
    assert!(params.get("unattended").is_none());
    port.call("prompt.submit", params).await.unwrap()
}
fn finish(capture: Option<Capture>) {
    if let Some(capture) = capture {
        capture.finish()
    }
    println!("JOURNAL_LIVE_RESULT: PASSED");
}
fn refusal(result: Result<Value, CallError>, reason: &str) {
    match result {
        Err(CallError::Rpc(error)) => assert_eq!(error.reason.as_deref(), Some(reason)),
        _ => panic!("expected refusal {reason}"),
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn live_create_is_idempotent_across_connections() {
    let Some(env) = live_port::preconditions().await else {
        return;
    };
    let capture = Capture::new(1);
    let recorder = capture.as_ref().map(|c| &c.recorder);
    let id = unique("create");
    let first = live_port::attach(&env, recorder).await;
    let session = make_session(&first, &id).await;
    first.client.close().await;
    let second = live_port::attach(&env, recorder).await;
    let again = make_session(&second, &id).await;
    assert_eq!(session, again);
    match second
        .call("session.create", create(&id, "different-model"))
        .await
    {
        Err(CallError::Rpc(error)) => {
            assert_eq!(error.reason.as_deref(), Some("invalid_params"));
            assert!(
                error
                    .data
                    .as_ref()
                    .and_then(|v| v.get("field"))
                    .and_then(Value::as_str)
                    == Some("request_id")
                    || error.message.contains("request_id")
            );
        }
        _ => panic!("changed launch was accepted"),
    }
    println!("session_id={session}");
    second.client.close().await;
    finish(capture);
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn live_submit_is_idempotent() {
    let Some(env) = live_port::preconditions().await else {
        return;
    };
    let capture = Capture::new(2);
    let port = live_port::attach(&env, capture.as_ref().map(|c| &c.recorder)).await;
    let session = make_session(&port, &unique("submit")).await;
    let id = unique("turn");
    let first = admit(&port, &session, &id, "Reply with ok.").await;
    let second = admit(&port, &session, &id, "Reply with ok.").await;
    assert_eq!(first["admission_id"], second["admission_id"]);
    assert!(first.get("result").is_none() && second.get("result").is_none());
    refusal(
        port.call(
            "prompt.submit",
            submit(&session, &id, "Reply with different text."),
        )
        .await,
        "admission_conflict",
    );
    port.client.close().await;
    finish(capture);
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn live_receipt_includes_result() {
    let Some(env) = live_port::preconditions().await else {
        return;
    };
    let capture = Capture::new(3);
    let port = live_port::attach(&env, capture.as_ref().map(|c| &c.recorder)).await;
    let session = make_session(&port, &unique("receipt")).await;
    let admitted = admit(&port, &session, &unique("turn"), "Reply with ok.").await;
    let admission = admitted["admission_id"].as_str().unwrap();
    let receipt = live_port::wait_terminal(&port, &session, admission).await;
    assert!(receipt["result"]["final_response"].as_str().is_some());
    let usage = receipt["usage"].as_object().unwrap();
    assert!(
        usage
            .keys()
            .all(|key| ["input_tokens", "output_tokens", "total_tokens"].contains(&key.as_str()))
    );
    port.client.close().await;
    finish(capture);
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn live_turn_survives_runner_disconnect() {
    let Some(env) = live_port::preconditions().await else {
        return;
    };
    let first = live_port::attach(&env, None).await;
    let session = make_session(&first, &unique("disconnect")).await;
    let admitted = admit(
        &first,
        &session,
        &unique("turn"),
        "Count from 1 to 200, one number per line.",
    )
    .await;
    let admission = admitted["admission_id"].as_str().unwrap().to_owned();
    first.client.close().await;
    let second = live_port::attach(&env, None).await;
    second
        .call(
            "session.resume",
            json!({"profile":"fixture","session_id":session}),
        )
        .await
        .unwrap();
    let receipt = live_port::wait_terminal(&second, &session, &admission).await;
    assert_eq!(receipt["outcome"], "completed");
    assert!(
        !receipt["result"]["final_response"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    second.client.close().await;
    finish(None);
}

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn live_unknown_after_restart() {
    if std::env::var("TALARIA_OPERATOR_RESTART").ok().as_deref() != Some("1") {
        let _: Option<()> = live_port::skip("set TALARIA_OPERATOR_RESTART=1 to run");
        return;
    }
    let Some(env) = live_port::preconditions().await else {
        return;
    };
    let first = live_port::attach(&env, None).await;
    let id = unique("restart");
    let session = make_session(&first, &id).await;
    let admitted = admit(
        &first,
        &session,
        &unique("turn"),
        "Count from 1 to 200, one number per line.",
    )
    .await;
    let admission = admitted["admission_id"].as_str().unwrap().to_owned();
    println!("RESTART THE GATEWAY NOW");
    let changed = live_port::changed_gateway(&env)
        .await
        .expect("gateway restart did not arrive");
    first.client.close().await;
    let second = live_port::attach(&changed, None).await;
    assert_eq!(make_session(&second, &id).await, session);
    let snapshot = second
        .call(
            "session.resume",
            json!({"profile":"fixture","session_id":session}),
        )
        .await
        .unwrap();
    assert!(
        snapshot["pending"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["admission_id"] == admission && p["status"] == "unknown")
    );
    let receipt=second.call("prompt.receipt",json!({"profile":"fixture","session_id":session,"admission_id":admission,"include_result":true})).await.unwrap();
    assert_eq!(receipt["status"], "unknown");
    let params = json!({"session_id":session,"admission_id":admission,"execution_generation":receipt["execution_generation"]});
    let first = second
        .call("prompt.resolve_unknown", params.clone())
        .await
        .unwrap();
    assert_eq!(first["status"], "terminal");
    assert_eq!(first["outcome"], "interrupted");
    assert_eq!(
        second.call("prompt.resolve_unknown", params).await.unwrap(),
        first
    );
    second.client.close().await;
    finish(None);
}
