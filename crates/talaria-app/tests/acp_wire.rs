//! Scripted agent protocol and fleet integration.

#[path = "support/acp_harness.rs"]
mod harness;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use harness::*;
use serde_json::{Value, json};
use std::time::Duration;
use talaria_app::acp::{PermissionResponder, replay_recording};
use talaria_core::fleet::acp::AcpCell;
use talaria_core::fleet::{FleetBackend, Outcome, RowKey, RowState};

fn script_with_finish(mut steps: Vec<Value>) -> Vec<Value> {
    steps.extend(finish());
    steps
}
fn responder(key: talaria_core::fleet::acp::AcpSessionKey, id: &str) -> PermissionResponder {
    PermissionResponder {
        generation: 1,
        request_id: id.into(),
        key,
        tool_call_id: "tool-1".into(),
    }
}
fn error_reply() -> Value {
    json!({"kind":"reply_error","error":{"code":-32000,"message":"Authentication required"}})
}

#[tokio::test]
async fn handshake_speaks_the_configured_protocol_version() {
    let name = "handshake_speaks_the_configured_protocol_version";
    if !in_child(name) {
        run_case(name, "", |_| script_with_finish(standard_hello()));
        return;
    }
    let mut h = Harness::new(agent());
    h.service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.close().await;
    let init = messages("initialize");
    assert_eq!(init.len(), 1);
    assert_eq!(init[0]["params"]["protocolVersion"], 1);
    assert_eq!(init[0]["params"]["clientInfo"]["name"], "talaria");
    assert_eq!(
        init[0]["params"]["clientInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        init[0]["params"]["clientCapabilities"],
        json!({"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false})
    );
    let events = seen();
    let answered = events
        .iter()
        .position(|event| event.pointer("/sent/result/protocolVersion") == Some(&json!(1)))
        .expect("matching initialize reply");
    let new = events
        .iter()
        .position(|event| event.pointer("/seen/method") == Some(&json!("session/new")))
        .expect("session request");
    assert!(answered < new);
}

#[tokio::test]
async fn handshake_accepts_missing_optional_fields() {
    let name = "handshake_accepts_missing_optional_fields";
    if !in_child(name) {
        for (variant, reply_value) in [
            ("absent", json!({"protocolVersion":1})),
            ("null", json!({"protocolVersion":1,"agentInfo":null})),
        ] {
            run_case(name, variant, |_| script_with_finish(hello(reply_value)));
        }
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    let snapshot = h.service.fleet_snapshot();
    let row = snapshot
        .rows
        .iter()
        .find(|row| row.key.session_id == key.session_id)
        .expect("row");
    assert_eq!(row.agent(), Some("Fixture Agent"));
    h.service.close().await;
}

#[tokio::test]
async fn version_mismatch_refuses_and_kills_the_process() {
    let name = "version_mismatch_refuses_and_kills_the_process";
    if !in_child(name) {
        run_case(name, "", |_| {
            vec![
                expect("initialize"),
                reply(json!({"protocolVersion":99})),
                expect("session/new"),
            ]
        });
        return;
    }
    let mut h = Harness::new(agent());
    let error = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect_err("version refused")
        .to_string();
    assert!(error.contains('1') && error.contains("99"));
    settle().await;
    assert!(!process_alive());
    assert!(h.service.fleet_snapshot().rows.is_empty());
    assert!(messages("session/new").is_empty());
}

#[tokio::test]
async fn auth_success_retries_session_new_once() {
    let name = "auth_success_retries_session_new_once";
    if !in_child(name) {
        run_case(name, "", |_| {
            script_with_finish(vec![
                expect("initialize"),
                reply(json!({"protocolVersion":1,"authMethods":[{"id":"login","name":"Login"}]})),
                expect("session/new"),
                error_reply(),
                expect("authenticate"),
                reply(json!({})),
                expect("session/new"),
                reply(json!({"sessionId":"session-1"})),
            ])
        });
        return;
    }
    let mut entry = agent();
    entry.auth_method = Some("login".into());
    let mut h = Harness::new(entry);
    h.service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("authenticated session");
    h.service.close().await;
    let sessions = messages("session/new");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["params"], sessions[1]["params"]);
    let auth = messages("authenticate");
    assert_eq!(auth.len(), 1);
    assert_eq!(auth[0]["params"], json!({"methodId":"login"}));
}

#[tokio::test]
async fn auth_missing_or_unadvertised_fails_plainly() {
    let name = "auth_missing_or_unadvertised_fails_plainly";
    if !in_child(name) {
        for variant in ["missing", "unadvertised", "terminal"] {
            run_case(name, variant, |_| {
                let advertised = if variant == "terminal" {
                    json!({"id":"login","name":"Login","_meta":{"terminal-auth":{"type":"terminal","command":"login"}}})
                } else {
                    json!({"id":"login","name":"Login"})
                };
                vec![
                    expect("initialize"),
                    reply(json!({"protocolVersion":1,"authMethods":[advertised]})),
                    expect("session/new"),
                    error_reply(),
                    expect("authenticate"),
                ]
            });
        }
        return;
    }
    let mut entry = agent();
    entry.auth_method = match variant().as_str() {
        "missing" => None,
        "unadvertised" => Some("other".into()),
        _ => Some("login".into()),
    };
    let mut h = Harness::new(entry);
    let error = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect_err("no automatic authentication")
        .to_string();
    assert!(!error.trim().is_empty());
    settle().await;
    assert!(messages("authenticate").is_empty());
    assert_eq!(messages("session/new").len(), 1);
    assert!(h.service.fleet_snapshot().rows.is_empty());
    assert!(!process_alive());
}

#[tokio::test]
async fn new_session_creates_one_namespaced_row() {
    let name = "new_session_creates_one_namespaced_row";
    if !in_child(name) {
        run_case(name, "", |_| script_with_finish(standard_hello()));
        return;
    }
    let mut h = Harness::new(agent());
    let cwd = root()
        .join("work")
        .canonicalize()
        .expect("working directory");
    let key = h
        .service
        .new_session("fixture", &cwd, "")
        .await
        .expect("new session");
    assert_eq!(key.agent_id, "fixture");
    let snapshot = h.service.fleet_snapshot();
    let rows = snapshot
        .rows
        .iter()
        .filter(|r| r.key.session_id == key.session_id)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.profile, "acp:fixture");
    assert_eq!(rows[0].backend(), FleetBackend::Acp);
    assert_eq!(rows[0].agent(), Some("Fixture Agent"));
    assert_ne!(
        rows[0].key,
        RowKey {
            profile: "fixture".into(),
            session_id: key.session_id.clone()
        }
    );
    let request = messages("session/new");
    assert_eq!(request[0]["params"], json!({"cwd":cwd,"mcpServers":[]}));
    h.service.close().await;
}

#[tokio::test]
async fn prompt_streams_updates_then_completes() {
    let name = "prompt_streams_updates_then_completes";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![expect("session/prompt"), update(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello "}})), update(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world"}})), update(json!({"sessionUpdate":"tool_call","toolCallId":"tool-1","title":"Run"})), update(json!({"sessionUpdate":"tool_call_update","toolCallId":"tool-1","content":[{"type":"content","content":{"type":"text","text":"output"}}]})), update(json!({"sessionUpdate":"usage_update","used":25,"size":100})), reply(json!({"stopReason":"end_turn"}))]);
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "hello").await.expect("prompt");
    settle().await;
    let prompt = messages("session/prompt");
    assert_eq!(
        prompt[0]["params"],
        json!({"sessionId":"session-1","prompt":[{"type":"text","text":"hello"}]})
    );
    let snapshots = h.snapshots.lock().expect("snapshots").clone();
    assert!(
        snapshots
            .iter()
            .any(|s| s.rows.iter().any(|r| r.state == RowState::Working))
    );
    let row = h.service.fleet_snapshot();
    let row = row
        .rows
        .iter()
        .find(|r| r.key.session_id == key.session_id)
        .expect("row");
    assert_eq!(row.state, RowState::Done);
    assert_eq!(row.outcome, Some(Outcome::Completed));
    assert_eq!(row.occupancy(), Some((25, 100)));
    let cells = h.service.transcript(&key);
    assert!(cells.contains(&AcpCell::Text("Hello world".into())));
    assert_eq!(
        cells
            .iter()
            .filter(|c| matches!(c, AcpCell::Tool { .. }))
            .count(),
        1
    );
    h.service.close().await;
}

#[tokio::test]
async fn failed_tool_then_recovery_stays_working_until_end() {
    let name = "failed_tool_then_recovery_stays_working_until_end";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![expect("session/prompt"), update(json!({"sessionUpdate":"tool_call","toolCallId":"tool-1","title":"Run"})), update(json!({"sessionUpdate":"tool_call_update","toolCallId":"tool-1","status":"failed"})), json!({"kind":"sleep_ms","ms":150}), update(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Recovered"}})), reply(json!({"stopReason":"end_turn"}))]);
            script_with_finish(s)
        });
        return;
    }
    let _trace_guard = talaria_app::acp::trace_fixture_guard(false);
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    talaria_app::acp::trace_fixture_event(false, "prompt_returned");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let checkpoint = h.service.fleet_snapshot();
    let state = checkpoint
        .rows
        .iter()
        .find(|row| row.key.session_id == key.session_id)
        .expect("row")
        .state;
    talaria_app::acp::trace_fixture_event(
        false,
        match state {
            RowState::Working => "checkpoint_working",
            RowState::Done => "checkpoint_done",
            _ => "checkpoint_other",
        },
    );
    let snapshots = h.snapshots.lock().expect("snapshots").clone();
    assert!(
        !snapshots
            .iter()
            .flat_map(|s| &s.rows)
            .any(|row| row.key.session_id == key.session_id && row.state == RowState::Failed)
    );
    assert!(
        snapshots
            .iter()
            .flat_map(|s| &s.rows)
            .any(|row| row.key.session_id == key.session_id && row.state == RowState::Working)
    );
    let final_snapshot = h.service.fleet_snapshot();
    assert_eq!(
        final_snapshot
            .rows
            .iter()
            .find(|r| r.key.session_id == key.session_id)
            .expect("row")
            .state,
        RowState::Done
    );
    assert!(
        h.service
            .transcript(&key)
            .contains(&AcpCell::Text("Recovered".into()))
    );
    h.service.close().await;
}

#[tokio::test]
async fn cancel_stops_the_turn() {
    let name = "cancel_stops_the_turn";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![
                expect("session/prompt"),
                expect("session/cancel"),
                reply(json!({"stopReason":"cancelled"})),
            ]);
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    h.service.cancel(&key).await.expect("cancel");
    settle().await;
    let cancel = messages("session/cancel");
    assert_eq!(cancel.len(), 1);
    assert!(cancel[0].get("id").is_none());
    assert_eq!(cancel[0]["params"], json!({"sessionId":"session-1"}));
    let snap = h.service.fleet_snapshot();
    let row = snap
        .rows
        .iter()
        .find(|r| r.key.session_id == key.session_id)
        .expect("row");
    assert_eq!(row.state, RowState::Done);
    assert_eq!(row.outcome, Some(Outcome::Interrupted));
    h.service.close().await;
}

#[tokio::test]
async fn cancel_with_open_permission_settles_it_cancelled() {
    let name = "cancel_with_open_permission_settles_it_cancelled";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![
                expect("session/prompt"),
                permission(7),
                response(7),
                expect("session/cancel"),
                reply(json!({"stopReason":"cancelled"})),
            ]);
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    assert_eq!(h.service.fleet_snapshot().queue.len(), 1);
    h.service.cancel(&key).await.expect("cancel");
    settle().await;
    assert_eq!(responses(7).len(), 1);
    assert_eq!(
        responses(7)[0]["result"],
        json!({"outcome":{"outcome":"cancelled"}})
    );
    let snap = h.service.fleet_snapshot();
    assert!(snap.queue.is_empty());
    assert_eq!(
        snap.rows
            .iter()
            .find(|r| r.key.session_id == key.session_id)
            .expect("row")
            .outcome,
        Some(Outcome::Interrupted)
    );
    h.service.close().await;
}

#[tokio::test]
async fn permission_answer_sends_the_chosen_option() {
    let name = "permission_answer_sends_the_chosen_option";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![
                expect("session/prompt"),
                permission(7),
                permission(8),
                response(7),
                response(8),
                reply(json!({"stopReason":"end_turn"})),
            ]);
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    assert_eq!(h.service.fleet_snapshot().queue.len(), 2);
    let first = responder(key.clone(), "7");
    h.service
        .answer_permission(&first, "reject")
        .await
        .expect("answer");
    h.service
        .answer_permission(&responder(key.clone(), "8"), "allow")
        .await
        .expect("second request");
    let _ = h.service.answer_permission(&first, "reject").await;
    settle().await;
    assert_eq!(responses(7).len(), 1);
    assert_eq!(
        responses(7)[0]["result"],
        json!({"outcome":{"outcome":"selected","optionId":"reject"}})
    );
    assert_eq!(responses(8).len(), 1);
    assert!(h.service.fleet_snapshot().queue.is_empty());
    assert!(
        h.service
            .fleet_snapshot()
            .diagnostics
            .iter()
            .any(|line| line.contains("already settled"))
    );
    h.service.close().await;
}

#[tokio::test]
async fn mixed_backend_actions_send_no_hermes_requests() {
    let name = "mixed_backend_actions_send_no_hermes_requests";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![
                expect("session/prompt"),
                permission(7),
                response(7),
                expect("session/cancel"),
                reply(json!({"stopReason":"cancelled"})),
            ]);
            script_with_finish(s)
        });
        return;
    }
    let gateway = mock_gateway::StandaloneGateway::start(&root());
    gateway.set_fleet_profiles(vec!["fixture".into()]);
    gateway.set_fleet_session(
        "fixture",
        json!({"id":"session-1","title":"Hermes","status":"idle","message_count":0}),
    );
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let snap = h.service.fleet_snapshot();
        if snap
            .rows
            .iter()
            .any(|r| r.key.profile == "fixture" && r.key.session_id == "session-1")
            && snap
                .rows
                .iter()
                .any(|r| r.key.profile == "acp:fixture" && r.key.session_id == "session-1")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "both backends must be visible"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    settle().await;
    let before = gateway.frame_count();
    h.service
        .answer_permission(&responder(key.clone(), "7"), "reject")
        .await
        .expect("answer");
    h.service.cancel(&key).await.expect("cancel");
    settle().await;
    assert_eq!(
        gateway.frame_count(),
        before,
        "external actions send no gateway frames"
    );
    assert!(gateway.requests("approval.respond").is_empty());
    assert!(gateway.requests("session.interrupt").is_empty());
    let snap = h.service.fleet_snapshot();
    assert!(
        snap.rows
            .iter()
            .any(|r| r.key.profile == "fixture" && r.key.session_id == "session-1")
    );
    assert!(
        snap.rows
            .iter()
            .any(|r| r.key.profile == "acp:fixture" && r.key.session_id == "session-1")
    );
    h.service.close().await;
}

#[test]
#[ignore = "gate: recorded ACP fixture (specification R5)"]
fn recorded_acp_fixture_replays() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/acp/mock-agent-session.jsonl");
    let Ok(recording) = std::fs::read_to_string(path) else {
        eprintln!(
            "RECORDED_ACP_RESULT: BLOCKED (operator-selected real-agent recording is missing)"
        );
        return;
    };
    let frames: Vec<Value> = recording
        .lines()
        .map(|line| serde_json::from_str(line).expect("recorded JSON line"))
        .collect();
    let expected = frames
        .last()
        .and_then(|value| value.get("expected"))
        .expect("recorded expected outcome");
    let mapped = replay_recording(&recording).expect("recording replay");
    assert_eq!(
        format!("{:?}", mapped.state).to_lowercase(),
        expected["row"]["state"].as_str().expect("state")
    );
    assert_eq!(mapped.title.as_deref(), expected["row"]["title"].as_str());
    let cells: Vec<Value> = mapped
        .transcript
        .iter()
        .map(|cell| match cell {
            AcpCell::Text(text) => json!({"type":"text","text":text}),
            AcpCell::Thought(text) => json!({"type":"thought","text":text}),
            AcpCell::Tool {
                id,
                title,
                body,
                status,
            } => json!({"type":"tool","id":id,"title":title,"body":body,"status":status}),
            AcpCell::Plan(text) => json!({"type":"plan","text":text}),
            AcpCell::Notice(text) => json!({"type":"notice","text":text}),
        })
        .collect();
    assert_eq!(json!(cells), expected["transcript"]);
    let usage = mapped.usage.expect("usage");
    assert_eq!(
        json!({"used":usage.used,"size":usage.size,"cost_amount":usage.cost_amount,"cost_currency":usage.cost_currency}),
        expected["usage"]
    );
}

#[tokio::test]
async fn client_powers_refused_by_default() {
    let name = "client_powers_refused_by_default";
    if !in_child(name) {
        run_case(name, "", |root| {
            let mut s = standard_hello();
            s.push(expect("session/prompt"));
            for (id, method, params) in [
                (
                    11,
                    "fs/read_text_file",
                    json!({"sessionId":"session-1","path":root.join("work/input.txt")}),
                ),
                (
                    12,
                    "fs/write_text_file",
                    json!({"sessionId":"session-1","path":root.join("work/created.txt"),"content":"changed"}),
                ),
                (
                    13,
                    "terminal/create",
                    json!({"sessionId":"session-1","command":"env","args":[]}),
                ),
            ] {
                s.push(emit(
                    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
                ));
                s.push(response(id));
            }
            s.push(reply(json!({"stopReason":"end_turn"})));
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    for id in [11, 12, 13] {
        let responses = responses(id);
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["error"]["code"], -32601);
        assert!(
            responses[0]["error"]["message"]
                .as_str()
                .expect("error")
                .contains("not enabled")
        );
    }
    assert_eq!(
        std::fs::read_to_string(root().join("work/input.txt")).expect("file"),
        "inside"
    );
    assert!(!root().join("work/created.txt").exists());
    assert_eq!(
        h.service
            .fleet_snapshot()
            .diagnostics
            .iter()
            .filter(|line| line.contains("not enabled"))
            .count(),
        3
    );
    h.service.close().await;
}

#[tokio::test]
async fn enabled_file_power_stays_inside_the_working_directory() {
    let name = "enabled_file_power_stays_inside_the_working_directory";
    if !in_child(name) {
        for variant in ["enabled", "disabled"] {
            run_case(name, variant, |root| {
                let mut s = standard_hello();
                s.push(expect("session/prompt"));
                let work = root.join("work");
                let outside = root.join("outside");
                for (id, method, path, content) in [
                    (21, "fs/read_text_file", work.join("input.txt"), None),
                    (
                        22,
                        "fs/write_text_file",
                        work.join("input.txt"),
                        Some("updated"),
                    ),
                    (
                        23,
                        "fs/write_text_file",
                        work.join("new.txt"),
                        Some("created"),
                    ),
                    (
                        24,
                        "fs/write_text_file",
                        work.join("link/escape.txt"),
                        Some("escape"),
                    ),
                    (25, "fs/read_text_file", outside.join("secret.txt"), None),
                    (
                        26,
                        "fs/write_text_file",
                        outside.join("secret.txt"),
                        Some("escape"),
                    ),
                ] {
                    let mut params = json!({"sessionId":"session-1","path":path});
                    if let Some(content) = content {
                        params["content"] = json!(content);
                    }
                    s.push(emit(
                        json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
                    ));
                    s.push(response(id));
                }
                s.push(reply(json!({"stopReason":"end_turn"})));
                script_with_finish(s)
            });
        }
        return;
    }
    let mut entry = agent();
    entry.allow_fs = variant() == "enabled";
    let mut h = Harness::new(entry);
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    if variant() == "enabled" {
        for id in [21, 22, 23] {
            assert!(
                responses(id)[0].get("result").is_some(),
                "in-tree request {id}"
            );
        }
        for id in [24, 25, 26] {
            assert_eq!(responses(id)[0]["error"]["code"], -32601);
            assert!(
                responses(id)[0]["error"]["message"]
                    .as_str()
                    .expect("error")
                    .contains("outside")
            );
        }
        assert_eq!(
            std::fs::read_to_string(root().join("work/input.txt")).expect("file"),
            "updated"
        );
        assert_eq!(
            std::fs::read_to_string(root().join("work/new.txt")).expect("new file"),
            "created"
        );
        assert!(
            h.service
                .transcript(&key)
                .iter()
                .any(|c| matches!(c, AcpCell::Tool { body, .. } if body.contains("new.txt")))
        );
    } else {
        for id in [21, 22, 23, 24, 25, 26] {
            assert_eq!(responses(id)[0]["error"]["code"], -32601);
        }
        assert_eq!(
            std::fs::read_to_string(root().join("work/input.txt")).expect("file"),
            "inside"
        );
        assert!(!root().join("work/new.txt").exists());
    }
    assert_eq!(
        std::fs::read_to_string(root().join("outside/secret.txt")).expect("outside file"),
        "outside"
    );
    assert!(!root().join("outside/escape.txt").exists());
    h.service.close().await;
}

#[tokio::test]
async fn enabled_terminal_uses_allowlisted_environment() {
    let name = "enabled_terminal_uses_allowlisted_environment";
    if !in_child(name) {
        for variant in ["enabled", "disabled"] {
            run_case(name, variant, |root| {
                let mut s = standard_hello();
                s.push(expect("session/prompt"));
                s.push(emit(json!({"jsonrpc":"2.0","id":31,"method":"terminal/create","params":{"sessionId":"session-1","command":"env","args":[],"cwd":root.join("work")}})));
                if variant == "enabled" {
                    s.push(json!({"kind":"expect","response_id":31,"save":{"terminal":"/result/terminalId"}}));
                    s.push(emit(json!({"jsonrpc":"2.0","id":32,"method":"terminal/wait_for_exit","params":{"sessionId":"session-1","terminalId":"$terminal"}})));
                    s.push(response(32));
                    s.push(emit(json!({"jsonrpc":"2.0","id":33,"method":"terminal/output","params":{"sessionId":"session-1","terminalId":"$terminal"}})));
                    s.push(response(33));
                    s.push(emit(json!({"jsonrpc":"2.0","id":34,"method":"terminal/release","params":{"sessionId":"session-1","terminalId":"$terminal"}})));
                    s.push(response(34));
                } else {
                    s.push(response(31));
                }
                s.push(reply(json!({"stopReason":"end_turn"})));
                script_with_finish(s)
            });
        }
        return;
    }
    let mut entry = agent();
    entry.allow_terminal = variant() == "enabled";
    let mut h = Harness::new(entry);
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    if variant() == "enabled" {
        let result = responses(33);
        let output = result[0]["result"]["output"]
            .as_str()
            .expect("terminal output");
        let mut names = output
            .lines()
            .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_owned()))
            .collect::<Vec<_>>();
        names.sort();
        let mut allowed = ["PATH", "HOME", "TERM", "LANG", "LC_ALL", "TMPDIR"]
            .into_iter()
            .filter(|name| std::env::var_os(name).is_some())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        allowed.sort();
        assert_eq!(names, allowed);
        assert!(!output.contains("profile-leak-sentinel"));
    } else {
        assert_eq!(responses(31)[0]["error"]["code"], -32601);
    }
    h.service.close().await;
}

#[tokio::test]
async fn process_exit_fails_working_rows() {
    let name = "process_exit_fails_working_rows";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![
                expect("session/prompt"),
                permission(7),
                json!({"kind":"sleep_ms","ms":100}),
                json!({"kind":"exit","code":23}),
            ]);
            s
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    let _ = h.service.prompt(&key, "go").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let snap = h.service.fleet_snapshot();
    let row = snap
        .rows
        .iter()
        .find(|r| r.key.session_id == key.session_id)
        .expect("row");
    assert_eq!(row.state, RowState::Failed);
    assert!(snap.queue.is_empty());
    assert!(
        snap.diagnostics
            .iter()
            .any(|line| line.contains("agent process exited (code 23)"))
    );
    assert!(responses(7).is_empty());
    assert!(!process_alive());
    h.service.close().await;
}

#[tokio::test]
async fn stderr_is_never_protocol() {
    let name = "stderr_is_never_protocol";
    if !in_child(name) {
        run_case(name, "", |_| {
            let mut s = standard_hello();
            s.extend(vec![expect("session/prompt"), json!({"kind":"stderr","protocol":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"fake"}}}}}), update(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"real"}})), reply(json!({"stopReason":"end_turn"}))]);
            script_with_finish(s)
        });
        return;
    }
    let mut h = Harness::new(agent());
    let key = h
        .service
        .new_session("fixture", &root().join("work"), "")
        .await
        .expect("session");
    h.service.prompt(&key, "go").await.expect("prompt");
    settle().await;
    let cells = h.service.transcript(&key);
    assert!(cells.contains(&AcpCell::Text("real".into())));
    assert!(!cells.contains(&AcpCell::Text("fake".into())));
    assert!(
        h.service
            .fleet_snapshot()
            .diagnostics
            .iter()
            .any(|line| line.contains("diagnostic line"))
    );
    h.service.close().await;
}
