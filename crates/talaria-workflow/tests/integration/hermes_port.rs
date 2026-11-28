#![cfg(feature = "mock")]
use crate::hermes_common as common;

use hermes_contract::schemas::{CanonicalSnapshot, RuntimeDescribeResult};
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::hermes::{
    AdmissionState, CreateSession, GatewayPort, HermesPort, PollConfig, PortConfig, PortError,
    RpcTimeouts, SetupError, StopResult, WireEvent, accept_wire_fixture, mock_wire::ScriptedWire,
};

fn cfg() -> PortConfig {
    PortConfig {
        home: PathBuf::from("/work/proj"),
        poll: PollConfig::default(),
        timeouts: RpcTimeouts::default(),
    }
}

fn scripted(name: &str) -> Arc<ScriptedWire> {
    Arc::new(ScriptedWire::load(&common::data(&format!("scripts/{name}.json"))).unwrap())
}

fn session() -> CreateSession {
    CreateSession {
        request_id: "req-1".into(),
        launch: common::launch(),
        label: "label".into(),
    }
}

#[tokio::test]
async fn preflight_requires_durable_admission() {
    let _raw = std::fs::read_to_string(common::data("scripts/describe_missing.json")).unwrap();
    for needle in ["durable-admission-v1", "tool", "request_id"] {
        let wire = scripted("describe_missing");
        let result = HermesPort::with_wires(cfg(), wire, "fixture").await;
        assert!(result.is_err(), "missing {needle}");
        match result {
            Ok(_) => unreachable!(),
            Err(SetupError::Unsupported { missing }) => {
                assert!(
                    missing.iter().any(|item| item == needle),
                    "missing list {missing:?} did not name {needle}"
                );
            }
            Err(other) => panic!("expected unsupported, got {other}"),
        }
    }
}

#[tokio::test]
async fn runtime_describe_with_null_skin_colours_opens_profile() {
    let mut describe = common::read_json("wire/describe_ok.json");
    describe["skin"] = json!({
        "name": "nullcolour",
        "colors": {"accent": "#00ff00", "prompt": null},
        "light_colors": {"ui_ok": null},
        "dark_colors": {"ui_warn": null},
    });
    assert!(accept_wire_fixture("RuntimeDescribeResult", &describe).is_err());
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps": [
                {"expect": "runtime.describe", "reply": describe},
                {"expect": "session.create", "reply": "create_ok.json"}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .expect("workflow port accepts null skin colours");
    port.create_session(session()).await.unwrap();
    let frames = wire.outbound();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].method, "runtime.describe");
    assert_eq!(frames[1].method, "session.create");
    assert!(frames.iter().all(|frame| frame.profile == "fixture"));
}

#[tokio::test]
async fn frames_on_the_wire_have_closed_keys() {
    let wire = scripted("text_ok");
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let _bound = port.create_session(session()).await.unwrap();
    let _submitted = port
        .submit(talaria_workflow::hermes::SubmitTurn {
            profile: "fixture".into(),
            session_id: "session".into(),
            submission_id: "sub-1".into(),
            text: "hello".into(),
        })
        .await
        .unwrap();
    let frames = wire.outbound();
    assert!(frames.iter().any(|frame| frame.method == "session.create"));
    assert!(frames.iter().any(|frame| frame.method == "prompt.submit"));
    for frame in &frames {
        assert!(!frame.params.contains_key("profile"), "{}", frame.method);
        assert!(!frame.params.contains_key("unattended"), "{}", frame.method);
        assert_eq!(frame.profile, "fixture");
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn wait_settled_polls_then_reads_result() {
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"prompt.receipt","reply":"submit_ok.json"},
                {"expect":"prompt.receipt","reply":"receipt_terminal.json"},
                {"expect":"prompt.receipt","reply":"receipt_terminal.json"}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let receipt = port
        .wait_settled("fixture".into(), "session".into(), "adm-1".into())
        .await
        .unwrap();
    assert_eq!(receipt.status, AdmissionState::Terminal);
    let usage = receipt.usage.expect("usage");
    assert!(usage.get("input_tokens").is_some());
    assert!(usage.get("output_tokens").is_some());
    assert!(usage.get("total_tokens").is_some());
    let receipts: Vec<_> = wire
        .outbound()
        .into_iter()
        .filter(|frame| frame.method == "prompt.receipt")
        .collect();
    assert!(receipts.len() >= 2, "{receipts:?}");
    assert_eq!(
        receipts
            .last()
            .and_then(|frame| frame.params.get("include_result")),
        Some(&json!(true))
    );
    let _gaps = [
        Duration::from_millis(250),
        Duration::from_millis(500),
        Duration::from_millis(1000),
    ];
}

#[tokio::test]
async fn a_first_failed_receipt_without_a_result_recovers_its_exact_cache_split() {
    let mut created = common::read_json("wire/create_ok.json");
    created["info"]["source"] = json!("tool");
    let mut failed = common::read_json("wire/receipt_terminal.json");
    failed["outcome"] = json!("failed");
    failed.as_object_mut().unwrap().remove("result");
    let mut next_failed = failed.clone();
    next_failed["admission_id"] = json!("admission-2");
    let mut next_started = common::read_json("wire/submit_ok.json");
    next_started["admission_id"] = json!("admission-2");
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"session.create","reply":created},
                {"expect":"prompt.submit","reply":"submit_ok.json"},
                {"expect":"prompt.receipt","reply":failed},
                {"expect":"session.stats","reply":{"total_tokens":10,"prompt_tokens":8,
                    "output_tokens":2,"cache_read_tokens":6}},
                {"expect":"prompt.submit","reply":next_started},
                {"expect":"prompt.receipt","reply":next_failed}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    port.create_session(session()).await.unwrap();
    for (submission, expected) in [("sub-1", json!(6)), ("sub-2", json!(null))] {
        let started = port
            .submit(talaria_workflow::hermes::SubmitTurn {
                profile: "fixture".into(),
                session_id: "session".into(),
                submission_id: submission.into(),
                text: "hello".into(),
            })
            .await
            .unwrap();
        let receipt = port
            .receipt("fixture".into(), "session".into(), started.admission_id)
            .await
            .unwrap();
        let usage = receipt.usage.unwrap();
        assert_eq!(usage["session_cache_read_tokens"], expected);
        assert_eq!(usage["total_tokens"], 10);
        assert!(usage["session_cost"].is_null());
    }
    assert_eq!(
        wire.outbound()
            .iter()
            .filter(|frame| frame.method == "session.stats")
            .count(),
        1
    );
}

#[tokio::test]
async fn wait_settled_returns_unknown_untouched() {
    let wire = scripted("unknown_after_restart");
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    port.create_session(session()).await.unwrap();
    port.submit(talaria_workflow::hermes::SubmitTurn {
        profile: "fixture".into(),
        session_id: "session".into(),
        submission_id: "sub-1".into(),
        text: "hello".into(),
    })
    .await
    .unwrap();
    let receipt = port
        .wait_settled("fixture".into(), "session".into(), "adm-1".into())
        .await
        .unwrap();
    assert_eq!(receipt.status, AdmissionState::Unknown);
    assert!(
        wire.outbound()
            .iter()
            .all(|frame| frame.method != "prompt.resolve_unknown")
    );
}

#[tokio::test]
async fn refusals_map_to_port_errors() {
    let wire = scripted("refused_toolsets");
    let (port, _feed) = HermesPort::with_wires(cfg(), wire, "fixture")
        .await
        .unwrap();
    let refused = port.create_session(session()).await.unwrap_err();
    match refused {
        PortError::Refused { field, detail, .. } => {
            assert_eq!(field.as_deref(), Some("toolsets"));
            let text = detail.unwrap_or_default();
            assert!(
                text.contains("toolsets") || text.contains("fields"),
                "{text}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    let dropped = port
        .submit(talaria_workflow::hermes::SubmitTurn {
            profile: "fixture".into(),
            session_id: "session".into(),
            submission_id: "sub-1".into(),
            text: "hello".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(dropped, PortError::Transport(_)), "{dropped:?}");
    let voice_wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"prompt.submit","reply":{"voice_stopped":true}}
            ]}),
        )
        .unwrap(),
    );
    let (voice_port, _feed) = HermesPort::with_wires(cfg(), voice_wire, "fixture")
        .await
        .unwrap();
    let voice = voice_port
        .submit(talaria_workflow::hermes::SubmitTurn {
            profile: "fixture".into(),
            session_id: "session".into(),
            submission_id: "voice".into(),
            text: "hello".into(),
        })
        .await
        .unwrap_err();
    match voice {
        PortError::Refused { reason, detail, .. } => {
            assert_eq!(reason, "voice_stopped");
            assert_eq!(
                detail.as_deref(),
                Some("Hermes treated the prompt as a voice stop")
            );
        }
        other => panic!("expected voice stop, got {other:?}"),
    }
}

#[tokio::test]
async fn one_wire_per_profile() {
    let wire = scripted("two_profiles");
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "default")
        .await
        .unwrap();
    let mut fixture = session();
    fixture.launch.profile = "fixture".into();
    let mut default_session = session();
    default_session.launch.profile = "default".into();
    let _ = port.create_session(default_session).await.unwrap();
    let _ = port.create_session(fixture).await.unwrap();
    let frames = wire.outbound();
    assert!(frames.iter().any(|frame| frame.profile == "default"));
    assert!(frames.iter().any(|frame| frame.profile == "fixture"));
    assert!(
        frames
            .iter()
            .all(|frame| !frame.params.contains_key("profile"))
    );
}

#[tokio::test]
async fn events_forwarded_without_loss() {
    let wire = scripted("flood_events");
    let (port, mut feed) = HermesPort::with_wires(cfg(), wire, "fixture")
        .await
        .unwrap();
    port.create_session(session()).await.unwrap();
    port.submit(talaria_workflow::hermes::SubmitTurn {
        profile: "fixture".into(),
        session_id: "session".into(),
        submission_id: "sub-1".into(),
        text: "hello".into(),
    })
    .await
    .unwrap();
    let receipt = port
        .wait_settled("fixture".into(), "session".into(), "adm-1".into())
        .await
        .unwrap();
    assert_eq!(receipt.status, AdmissionState::Terminal);
    let mut count = 0;
    while let Some(_item) = feed.recv().await {
        count += 1;
        if count == 10_000 {
            break;
        }
    }
    assert_eq!(count, 10_000);
    drop(feed);
    let started = std::time::Instant::now();
    let again = port
        .wait_settled("fixture".into(), "session".into(), "adm-1".into())
        .await
        .unwrap();
    assert_eq!(again.status, AdmissionState::Terminal);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn stop_cases() {
    let file = common::read_json("scripts/stop.json");
    let cases = file["scenarios"].as_array().unwrap();
    assert_eq!(cases.len(), 13);
    for case in cases {
        let wire =
            Arc::new(ScriptedWire::from_value(&common::data(""), case["script"].clone()).unwrap());
        let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
            .await
            .unwrap();
        port.receipt("fixture".into(), "session".into(), "admission".into())
            .await
            .unwrap();
        let expected = match case["result"].as_str().unwrap() {
            "Sent" => StopResult::Sent,
            "NotRunning" => StopResult::NotRunning,
            "Unknown" => StopResult::Unknown,
            other => panic!("unknown stop result {other}"),
        };
        assert_eq!(
            port.stop("fixture", "session").await,
            expected,
            "{}",
            case["name"]
        );
        let frames = wire.outbound();
        assert_eq!(
            frames.len(),
            case["script"]["steps"].as_array().unwrap().len(),
            "{}",
            case["name"]
        );
        for frame in frames {
            if frame.method == "session.interrupt" {
                assert_eq!(
                    frame.params,
                    json!({"session_id":"session","execution_generation":1})
                        .as_object()
                        .unwrap()
                        .clone()
                );
            }
            if frame.method == "prompt.cancel" {
                assert_eq!(
                    frame.params,
                    json!({"session_id":"session","admission_id":"admission"})
                        .as_object()
                        .unwrap()
                        .clone()
                );
            }
        }
    }
}

#[tokio::test]
async fn attach_resumes_once_per_connection() {
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"session.resume","reply":"create_ok.json"},
                {"expect":"session.resume","reply":"create_ok.json"}
            ]}),
        )
        .unwrap(),
    );
    let (port, mut feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    port.attach("fixture".into(), "session".into())
        .await
        .unwrap();
    port.attach("fixture".into(), "session".into())
        .await
        .unwrap();
    wire.emit(
        "fixture",
        WireEvent::Reattached {
            session_id: "session".into(),
            subscription_id: "subscription-2".into(),
        },
    );
    port.attach("fixture".into(), "session".into())
        .await
        .unwrap();
    wire.emit(
        "fixture",
        WireEvent::Gap {
            session_id: "session".into(),
            reason: "snapshot_required".into(),
        },
    );
    let mut saw_gap = false;
    let mut saw_resynced = false;
    while let Some(item) = feed.recv().await {
        match item {
            talaria_workflow::hermes::AgentFeedItem::Gap { .. } => saw_gap = true,
            talaria_workflow::hermes::AgentFeedItem::Resynced { .. } => {
                saw_resynced = true;
                break;
            }
            _ => {}
        }
    }
    assert!(saw_gap && saw_resynced);
    let resumes = wire
        .outbound()
        .iter()
        .filter(|frame| frame.method == "session.resume")
        .count();
    let resyncs = wire.tracks().iter().filter(|track| track.resync).count();
    assert_eq!(resumes, 2);
    assert_eq!(resyncs, 1);
}

#[tokio::test]
async fn wire_fixtures_decode_as_contract_types() {
    let manifest = common::read_json("wire/manifest.json");
    for entry in manifest.as_array().unwrap() {
        let file = entry["file"].as_str().unwrap();
        let kind = entry["type"].as_str().unwrap();
        let value = common::read_json(&format!("wire/{file}"));
        accept_wire_fixture(kind, &value).unwrap();
        match kind {
            "CanonicalSnapshot" => {
                let decoded: CanonicalSnapshot = serde_json::from_value(value).unwrap();
                assert_eq!(decoded.session_id, "session");
                assert_eq!(decoded.info.cwd.as_deref(), Some("/work/proj"));
            }
            "RuntimeDescribeResult" => {
                let decoded: RuntimeDescribeResult = serde_json::from_value(value).unwrap();
                assert!(
                    decoded
                        .capabilities
                        .iter()
                        .any(|cap| cap == "durable-admission-v1")
                );
            }
            "EventFrame" => {
                let frame = talaria_workflow::hermes::EventFrame::from_params(&value).unwrap();
                assert_eq!(frame.type_name(), Some("message.complete"));
            }
            other => panic!("unknown fixture type {other}"),
        }
    }
}

/// The session as Hermes reports it after it reset the conversation: a fresh stored session,
/// with the notice nobody tracked either running or waiting behind nothing.
fn moved_snapshot(running: bool, queued: bool) -> serde_json::Value {
    let mut value = common::read_json("wire/create_ok.json");
    value["stored_session_id"] = json!("stored-2");
    value["running"] = json!(running);
    if queued {
        value["pending"] = json!([{"admission_id":"notice","ref":{"profile_id":"fixture","session_id":"session"},
            "sequence":2,"status":"queued","outcome":null,"authority_epoch":1,"execution_generation":null,
            "input_id":"input","text":"[a background process finished]"}]);
    }
    value
}

#[tokio::test(start_paused = true)]
async fn a_sweep_interrupts_a_running_turn_on_a_moved_route_and_reads_its_usage() {
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"session.resume","reply":moved_snapshot(true, false)},
                {"expect":"session.resume","reply":moved_snapshot(true, false)},
                {"expect":"session.resume","reply":moved_snapshot(true, false)},
                {"expect":"session.interrupt","reply":"interrupt_ok.json",
                    "match":{"session_id":"session","execution_generation":1}},
                {"expect":"session.resume","reply":moved_snapshot(false, false)},
                {"expect":"session.stats","reply":{"total_tokens":77,"cache_read_tokens":60}}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let sweep = port
        .sweep_route("fixture".into(), "session".into(), "stored".into())
        .await
        .unwrap();
    assert_eq!(sweep.moved_to.as_deref(), Some("stored-2"));
    assert_eq!(sweep.cancelled, 0);
    assert_eq!(sweep.moved_tokens, Some(77));
    assert_eq!(sweep.moved_cache_read_tokens, Some(60));
    assert!(!sweep.still_active, "{sweep:?}");
    assert!(sweep.problems.is_empty(), "{sweep:?}");
}

#[tokio::test(start_paused = true)]
async fn a_route_that_cannot_be_stopped_is_reported_still_active() {
    let refused =
        json!({"error":{"code":4001,"message":"refused","data":{"reason":"stale_generation"}}});
    let mut steps = vec![json!({"expect":"runtime.describe","reply":"describe_ok.json"})];
    // Twenty reads each find the notice waiting and fail to cancel it; the next one gives up.
    for _ in 0..20 {
        steps.push(json!({"expect":"session.resume","reply":moved_snapshot(true, true)}));
        steps.push(json!({"expect":"prompt.cancel","reply":refused}));
    }
    steps.push(json!({"expect":"session.resume","reply":moved_snapshot(true, true)}));
    steps.push(json!({"expect":"session.stats","reply":{"total_tokens":0}}));
    let wire =
        Arc::new(ScriptedWire::from_value(&common::data(""), json!({ "steps": steps })).unwrap());
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let sweep = port
        .sweep_route("fixture".into(), "session".into(), "stored".into())
        .await
        .unwrap();
    assert!(sweep.still_active, "{sweep:?}");
    assert_eq!(sweep.cancelled, 0);
    assert_eq!(sweep.problems.len(), 20, "{sweep:?}");
    assert!(sweep.problems[0].contains("notice"), "{sweep:?}");
    // A zero reading is no reading: nothing is counted.
    assert_eq!(sweep.moved_tokens, None);
}

#[tokio::test(start_paused = true)]
async fn a_turn_that_has_only_just_settled_is_not_interrupted() {
    let mut running = common::read_json("wire/create_ok.json");
    running["running"] = json!(true);
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"session.resume","reply":running},
                {"expect":"session.resume","reply":"create_ok.json"}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let sweep = port
        .sweep_route("fixture".into(), "session".into(), "stored".into())
        .await
        .unwrap();
    assert_eq!(sweep, talaria_workflow::journal::RouteSweep::default());
    assert!(
        !wire
            .outbound()
            .iter()
            .any(|f| f.method == "session.interrupt" || f.method == "prompt.cancel")
    );
}
