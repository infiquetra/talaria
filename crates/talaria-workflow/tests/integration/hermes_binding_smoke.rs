//! Additional offline checks for receipt and replay edge cases.
#![cfg(feature = "mock")]
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};
use talaria_workflow::{
    engine::*,
    hermes::{frames::CREATE_KEYS, mock_wire::ScriptedWire, validate::*, *},
};

fn describe() -> Value {
    json!({"instance_id":"instance","profile_id":"fixture","authority_epoch":1,"capabilities":["durable-admission-v1"],"session_create":{"sources":["tool"],"parameters":CREATE_KEYS}})
}
fn snapshot(id: &str) -> Value {
    json!({"session_id":id,"stored_session_id":id,"messages":[],"message_count":0,"running":false,"authority_epoch":1,"replay_epoch":"epoch","last_sequence":0,"subscription_id":"subscription","revision":0,"execution_generation":1,"pending":[],"prompts":[],"info":{"desktop_protocol":"hermes-gateway-v1","source":"tool","model":"probe","provider":"custom"}})
}
fn receipt(id: &str, status: &str, result: Option<Value>) -> Value {
    let mut r = json!({"admission_id":"admission","ref":{"profile_id":"fixture","session_id":id},"sequence":1,"status":status,"outcome":if status=="terminal"{Some("completed")}else{None},"authority_epoch":1,"execution_generation":1});
    if let Some(result) = result {
        r["result"] = result;
        r["usage"] = json!({"input_tokens":5,"output_tokens":2,"total_tokens":7});
    }
    r
}
fn step(method: &str, reply: Value) -> Value {
    json!({"expect":method,"reply":reply})
}
fn request() -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: "call".into(),
            occurrence: 0,
        },
        label: "agent".into(),
        phase: None,
        prompt: "Reply".into(),
        schema: None,
        tier: None,
        model: Some("probe".into()),
        provider: Some("custom".into()),
        effort: None,
        profile: None,
        toolsets: Some(vec![]),
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path: vec![],
    }
}
fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: "fixture".into(),
        model: "probe".into(),
        provider: Some("custom".into()),
        effort: None,
        toolsets: vec![],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: Some(
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
    }
}
async fn port(steps: Vec<Value>) -> (HermesPort, AgentFeedReceiver, Arc<ScriptedWire>) {
    let wire = Arc::new(ScriptedWire::from_value(Path::new("."), json!(steps)).unwrap());
    let (port, feed) = HermesPort::with_wires(PortConfig::default(), wire.clone(), "fixture")
        .await
        .unwrap();
    (port, feed, wire)
}
async fn create(port: &HermesPort) {
    port.create_session(CreateSession {
        request_id: "request".into(),
        launch: launch(),
        label: "agent".into(),
    })
    .await
    .unwrap();
}
async fn submit(port: &HermesPort) {
    port.submit(SubmitTurn {
        profile: "fixture".into(),
        session_id: "session".into(),
        submission_id: "submission".into(),
        text: "Reply".into(),
    })
    .await
    .unwrap();
}

#[test]
fn nested_schema_paths_sort_by_numeric_index() {
    let schema = json!({"type":"array","items":{"type":"string"}});
    let values: Vec<_> = (0..12).map(|_| json!(4)).collect();
    let errors = validate_output(&json!(values).to_string(), &schema).unwrap_err();
    assert_eq!(errors.len(), 10);
    for (i, error) in errors.iter().enumerate() {
        assert!(error.starts_with(&format!("$[{i}]:")), "{error}");
    }
    assert!(check_schema(&json!({"type":"no-such-type"})).is_err());
}
#[test]
fn extraction_uses_earliest_valid_span() {
    assert_eq!(
        extract_json_candidate("```json\n[{\"a\":1}]\n```"),
        "[{\"a\":1}]"
    );
    assert_eq!(extract_json_candidate("[1, {\"a\": oops}"), "{\"a\": oops}");
}
#[test]
fn schema_policy_has_one_correction() {
    let mut r = request();
    r.schema = Some(json!({"type":"object","required":["ok"]}));
    let bad = SettledTurn {
        turn: 0,
        outcome: "completed".into(),
        final_text: Some("{}".into()),
        result_meta: json!({"completed":true}),
        usage: None,
    };
    assert!(matches!(
        SchemaTurnPolicy.after_turn(&r, std::slice::from_ref(&bad)),
        NextStep::Turn { .. }
    ));
    let mut second = bad.clone();
    second.turn = 1;
    assert!(
        matches!(SchemaTurnPolicy.after_turn(&r,&[bad,second]),NextStep::Finish(AgentOutcome::Failed{reason})if reason.starts_with("schema validation failed after one correction:"))
    );
}
#[tokio::test]
async fn create_and_submit_keep_allowlists() {
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        step("prompt.submit", receipt("session", "started", None)),
    ])
    .await;
    create(&p).await;
    submit(&p).await;
    for frame in wire.outbound() {
        assert!(!frame.params.contains_key("profile"));
        assert!(!frame.params.contains_key("unattended"));
        assert!(!frame.params.contains_key("title"));
        assert!(!frame.params.contains_key("hidden"));
    }
    assert_eq!(p.stored_session_id("session").as_deref(), Some("session"));
}
#[tokio::test(start_paused = true)]
async fn poll_terminal_has_one_final_read() {
    let final_result =
        json!({"final_response":"READY","estimated_cost_usd":0.2,"cost_status":"estimated"});
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        step("prompt.submit", receipt("session", "started", None)),
        step("prompt.receipt", receipt("session", "started", None)),
        step("prompt.receipt", receipt("session", "terminal", None)),
        step(
            "prompt.receipt",
            receipt("session", "terminal", Some(final_result.clone())),
        ),
    ])
    .await;
    create(&p).await;
    submit(&p).await;
    let start = tokio::time::Instant::now();
    let result = p
        .wait_settled("fixture".into(), "session".into(), "admission".into())
        .await
        .unwrap();
    assert_eq!(result.result, Some(final_result));
    assert_eq!(start.elapsed(), Duration::from_millis(750));
    let calls = wire
        .outbound()
        .into_iter()
        .filter(|f| f.method == "prompt.receipt")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 3);
    assert!(calls[0].params.get("include_result").is_none());
    assert_eq!(calls[2].params["include_result"], true);
    assert_eq!(
        result.usage.unwrap()["session_cost"]["estimated_cost_usd"],
        0.2
    );
}
#[tokio::test(start_paused = true)]
async fn unknown_is_returned_without_resolution() {
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        step("prompt.receipt", receipt("session", "unknown", None)),
    ])
    .await;
    create(&p).await;
    let result = p
        .wait_settled("fixture".into(), "session".into(), "admission".into())
        .await
        .unwrap();
    assert_eq!(result.status, AdmissionState::Unknown);
    assert!(
        wire.outbound()
            .iter()
            .all(|f| f.method != "prompt.resolve_unknown")
    );
}
#[tokio::test]
async fn repeated_attach_does_not_resume_twice() {
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.resume", snapshot("session")),
    ])
    .await;
    p.attach("fixture".into(), "session".into()).await.unwrap();
    p.attach("fixture".into(), "session".into()).await.unwrap();
    assert_eq!(
        wire.outbound()
            .iter()
            .filter(|f| f.method == "session.resume")
            .count(),
        1
    );
}
#[tokio::test]
async fn stop_never_interrupts_a_newer_generation() {
    let mut next = receipt("session", "started", None);
    next["execution_generation"] = json!(2);
    let(p,_feed,wire)=port(vec![step("runtime.describe",describe()),step("session.create",snapshot("session")),step("prompt.submit",receipt("session","started",None)),step("session.interrupt",json!({"error":{"code":4001,"message":"stale_generation","data":{"reason":"stale_generation"}}})),step("prompt.receipt",next)]).await;
    create(&p).await;
    submit(&p).await;
    assert_eq!(p.stop("fixture", "session").await, StopResult::NotRunning);
    assert_eq!(
        wire.outbound()
            .iter()
            .filter(|f| f.method == "session.interrupt")
            .count(),
        1
    );
}
#[tokio::test]
async fn early_events_are_held_until_session_metadata_exists() {
    let mut created = step("session.create", snapshot("session"));
    created["events"] = json!([{ "session_id":"session","type":"message.delta","seq":1,"replay_epoch":"epoch","payload":{"text":"delta"}}]);
    let (p, mut feed, _wire) = port(vec![step("runtime.describe", describe()), created]).await;
    create(&p).await;
    let event = tokio::time::timeout(Duration::from_secs(1), feed.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(event,AgentFeedItem::Event{label,request_id,..}if label=="agent"&&request_id=="request")
    );
}
#[tokio::test]
async fn dropped_feed_does_not_block_receipt() {
    let events:Vec<_>=(1..=10_000).map(|seq|json!({"session_id":"session","type":"message.delta","seq":seq,"replay_epoch":"epoch","payload":{"text":"x"}})).collect();
    let mut submitted = step("prompt.submit", receipt("session", "started", None));
    submitted["events"] = json!(events);
    let (p, feed, _wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        submitted,
        step(
            "prompt.receipt",
            receipt(
                "session",
                "terminal",
                Some(json!({"final_response":"READY"})),
            ),
        ),
    ])
    .await;
    drop(feed);
    create(&p).await;
    submit(&p).await;
    let r = tokio::time::timeout(
        Duration::from_secs(5),
        p.receipt("fixture".into(), "session".into(), "admission".into()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(r.status, AdmissionState::Terminal);
}

#[tokio::test]
async fn replay_gap_resumes_and_publishes_snapshot() {
    let (p, mut feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        step("session.resume", snapshot("session")),
    ])
    .await;
    create(&p).await;
    wire.emit(
        "fixture",
        talaria_workflow::hermes::WireEvent::Gap {
            session_id: "session".into(),
            reason: "epoch_changed".into(),
        },
    );
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), feed.recv())
            .await
            .unwrap()
            .unwrap(),
        AgentFeedItem::Gap { .. }
    ));
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), feed.recv())
            .await
            .unwrap()
            .unwrap(),
        AgentFeedItem::Resynced { .. }
    ));
    assert_eq!(wire.tracks().iter().filter(|t| t.resync).count(), 1);
    p.attach("fixture".into(), "session".into()).await.unwrap();
    assert_eq!(
        wire.outbound()
            .iter()
            .filter(|f| f.method == "session.resume")
            .count(),
        1
    );
}
#[tokio::test]
async fn stop_table_obeys_admission_fences() {
    let handle = json!({"ref":{"profile_id":"fixture","session_id":"session"},"instance_id":"instance","authority_epoch":1,"revision":1,"execution_generation":1,"execution_state":"running"});
    let stale = json!({"error":{"code":4001,"message":"stale_generation","data":{"reason":"stale_generation"}}});
    let mut cancelled = receipt("session", "terminal", None);
    cancelled["outcome"] = json!("cancelled");
    let cases = vec![
        ("terminal", vec![], StopResult::NotRunning),
        ("unknown", vec![], StopResult::Unknown),
        (
            "started",
            vec![step("session.interrupt", handle.clone())],
            StopResult::Sent,
        ),
        (
            "started",
            vec![
                step("session.interrupt", stale.clone()),
                step("prompt.receipt", receipt("session", "started", None)),
            ],
            StopResult::Unknown,
        ),
        (
            "started",
            vec![
                step("session.interrupt", stale.clone()),
                step("prompt.receipt", receipt("session", "terminal", None)),
            ],
            StopResult::NotRunning,
        ),
        (
            "queued",
            vec![step("prompt.cancel", cancelled)],
            StopResult::Sent,
        ),
        (
            "queued",
            vec![step("prompt.cancel", receipt("session", "terminal", None))],
            StopResult::NotRunning,
        ),
        (
            "queued",
            vec![
                step("prompt.cancel", receipt("session", "started", None)),
                step("session.interrupt", handle.clone()),
            ],
            StopResult::Sent,
        ),
        (
            "queued",
            vec![
                step("prompt.cancel", stale.clone()),
                step("prompt.receipt", receipt("session", "started", None)),
                step("session.interrupt", handle),
            ],
            StopResult::Sent,
        ),
        (
            "queued",
            vec![
                step("prompt.cancel", stale.clone()),
                step("prompt.receipt", receipt("session", "started", None)),
                step("session.interrupt", stale),
            ],
            StopResult::Unknown,
        ),
        (
            "queued",
            vec![step("prompt.cancel", receipt("session", "unknown", None))],
            StopResult::Unknown,
        ),
    ];
    for (status, tail, expected) in cases {
        let mut steps = vec![
            step("runtime.describe", describe()),
            step("session.create", snapshot("session")),
            step("prompt.submit", receipt("session", status, None)),
        ];
        steps.extend(tail);
        let (p, _feed, wire) = port(steps).await;
        create(&p).await;
        submit(&p).await;
        assert_eq!(p.stop("fixture", "session").await, expected, "{status}");
        assert!(
            wire.outbound()
                .iter()
                .filter(|f| f.method == "session.interrupt")
                .count()
                <= 1
        );
    }
}
#[test]
fn committed_wire_fixtures_decode() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/hermes/wire");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    for row in manifest.as_array().unwrap() {
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(row["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        match row["type"].as_str().unwrap() {
            "CanonicalSnapshot" => {
                let _: hermes_contract::schemas::CanonicalSnapshot =
                    serde_json::from_value(value).unwrap();
            }
            "RuntimeDescribeResult" => {
                let _: hermes_contract::schemas::RuntimeDescribeResult =
                    serde_json::from_value(value).unwrap();
            }
            "AdmissionStatus" => {
                let _: hermes_contract::schemas::AdmissionStatus =
                    serde_json::from_value(value).unwrap();
            }
            "PromptReceiptResult" => {
                let _: hermes_contract::schemas::PromptReceiptResult =
                    serde_json::from_value(value).unwrap();
            }
            "CanonicalSessionHandle" => {
                let _: hermes_contract::schemas::CanonicalSessionHandle =
                    serde_json::from_value(value).unwrap();
            }
            "EventFrame" => {
                let _ = hermes_contract::events::EventFrame::from_params(&value).unwrap();
            }
            _ => panic!("unrecognized fixture type"),
        }
    }
}

#[tokio::test]
async fn receipt_without_attach_keeps_stop_metadata() {
    let handle = json!({"ref":{"profile_id":"fixture","session_id":"session"},"instance_id":"instance","authority_epoch":1,"revision":1,"execution_generation":1,"execution_state":"running"});
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("prompt.receipt", receipt("session", "started", None)),
        step("session.interrupt", handle),
    ])
    .await;
    p.receipt("fixture".into(), "session".into(), "admission".into())
        .await
        .unwrap();
    assert_eq!(p.stop("fixture", "session").await, StopResult::Sent);
    assert!(
        wire.outbound()
            .iter()
            .all(|frame| frame.method != "session.resume")
    );
}
#[tokio::test]
async fn old_receipt_keeps_latest_admission_for_stop() {
    let mut newer = receipt("session", "started", None);
    newer["admission_id"] = json!("newer");
    newer["execution_generation"] = json!(2);
    let handle = json!({"ref":{"profile_id":"fixture","session_id":"session"},"instance_id":"instance","authority_epoch":1,"revision":2,"execution_generation":2,"execution_state":"running"});
    let (p, _feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("session.create", snapshot("session")),
        step("prompt.submit", receipt("session", "started", None)),
        step("prompt.submit", newer),
        step("prompt.receipt", receipt("session", "terminal", None)),
        step("session.interrupt", handle),
    ])
    .await;
    create(&p).await;
    submit(&p).await;
    p.submit(SubmitTurn {
        profile: "fixture".into(),
        session_id: "session".into(),
        submission_id: "second".into(),
        text: "next".into(),
    })
    .await
    .unwrap();
    p.receipt("fixture".into(), "session".into(), "admission".into())
        .await
        .unwrap();
    assert_eq!(p.stop("fixture", "session").await, StopResult::Sent);
    assert_eq!(
        wire.outbound().last().unwrap().params["execution_generation"],
        2
    );
}

#[tokio::test]
async fn reading_a_receipt_does_not_claim_event_ownership() {
    let (p, mut feed, wire) = port(vec![
        step("runtime.describe", describe()),
        step("prompt.receipt", receipt("unattached", "started", None)),
        step("session.create", snapshot("session")),
    ])
    .await;
    p.receipt("fixture".into(), "unattached".into(), "admission".into())
        .await
        .unwrap();
    create(&p).await;
    for sid in ["unattached", "session"] {
        let frame=hermes_contract::events::EventFrame::from_params(&json!({"session_id":sid,"type":"message.delta","seq":1,"replay_epoch":"epoch","payload":{"text":"delta"}})).unwrap();
        wire.emit(
            "fixture",
            WireEvent::Event {
                session_id: sid.into(),
                frame,
            },
        );
    }
    let event = tokio::time::timeout(Duration::from_secs(1), feed.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event,AgentFeedItem::Event{session_id,..}if session_id=="session"));
}

#[tokio::test]
async fn receipt_usage_does_not_require_a_stored_result() {
    let mut partial = receipt("session", "started", None);
    partial["usage"] = json!({"input_tokens":5});
    let (p, _feed, _wire) = port(vec![
        step("runtime.describe", describe()),
        step("prompt.receipt", partial),
        step(
            "prompt.receipt",
            receipt(
                "session",
                "terminal",
                Some(json!({"final_response":"ready"})),
            ),
        ),
        step("prompt.receipt", receipt("session", "terminal", None)),
    ])
    .await;
    let read = || p.receipt("fixture".into(), "session".into(), "admission".into());
    let first = read().await.unwrap();
    assert_eq!(first.usage, Some(json!({"input_tokens":5})));
    assert!(first.result.is_none());
    assert_eq!(
        read().await.unwrap().usage,
        Some(json!({"input_tokens":5,"output_tokens":2,"total_tokens":7}))
    );
    assert!(read().await.unwrap().usage.is_none());
}

#[test]
fn stopped_outcomes_take_precedence_over_the_failed_flag() {
    for outcome in ["interrupted", "cancelled"] {
        let turn = SettledTurn {
            turn: 0,
            outcome: outcome.into(),
            final_text: None,
            result_meta: json!({"failed":true,"turn_exit_reason":"interrupted"}),
            usage: None,
        };
        assert_eq!(
            SchemaTurnPolicy.after_turn(&request(), &[turn]),
            NextStep::Finish(AgentOutcome::Failed {
                reason: format!("stopped in Hermes (outcome '{outcome}')"),
            })
        );
    }
}
