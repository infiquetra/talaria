//! Live terminal-manager discovery, actions and fleet routing.

#[path = "support/herdr_harness.rs"]
mod harness;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;
use harness::*;
use serde_json::json;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use talaria_core::fleet::{FleetBackend, RowState};
use talaria_core::fleet_view::{FleetViewEffect, FleetViewInput, FleetViewState, update};

#[test]
fn mock_rejects_prompt_to_blocked_pane() {
    let name = "mock_rejects_prompt_to_blocked_pane";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let out = Command::new(binary())
        .args(["agent", "prompt", "wfixture:p2", "answer"])
        .stdin(Stdio::null())
        .output()
        .expect("mock command");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("agent_blocked"));
    assert_eq!(
        calls(),
        vec![vec!["agent", "prompt", "wfixture:p2", "answer"]]
    );
}

#[tokio::test]
async fn discovery_and_lifecycle_share_one_fleet() {
    let name = "discovery_and_lifecycle_share_one_fleet";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    let snap = service.snapshot();
    for (pane, state) in [
        ("wfixture:p1", RowState::Working),
        ("wfixture:p2", RowState::NeedsYou),
        ("wfixture:p3", RowState::Idle),
        ("wfixture:p4", RowState::Done),
        ("wfixture:p5", RowState::Idle),
    ] {
        let row = snap
            .rows
            .iter()
            .find(|row| row.key == key(pane))
            .expect("row");
        assert_eq!(row.state, state);
        assert_eq!(row.backend(), FleetBackend::Herdr);
    }
    assert_eq!(
        snap.queue
            .iter()
            .filter(|item| item.id.key == key("wfixture:p2"))
            .count(),
        1
    );
    let calls = calls();
    assert!(calls.iter().any(|a| a == &["agent", "list"]));
    for pane in [
        "wfixture:p1",
        "wfixture:p2",
        "wfixture:p3",
        "wfixture:p4",
        "wfixture:p5",
    ] {
        assert!(calls.iter().any(|a| a == &["agent", "get", pane]));
    }
    service.close().await;
}

#[tokio::test]
async fn peek_reads_visible_output_and_sanitizes() {
    let name = "peek_reads_visible_output_and_sanitizes";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    let text = service.peek(&key("wfixture:p1")).await.expect("peek");
    assert_eq!(text, "hello␛[31mworld‹U+202E›done");
    assert!(
        calls()
            .iter()
            .any(|a| a == &["pane", "read", "wfixture:p1", "--source", "visible"])
    );
    service.close().await;
}

#[tokio::test]
async fn follow_up_uses_atomic_prompt() {
    let name = "follow_up_uses_atomic_prompt";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    service
        .prompt(&key("wfixture:p3"), "two words\nand a line")
        .await
        .expect("prompt");
    let calls = calls();
    assert_eq!(
        calls
            .iter()
            .filter(|a| a.first().is_some_and(|x| x == "agent")
                && a.get(1).is_some_and(|x| x == "prompt"))
            .count(),
        1
    );
    assert!(
        calls
            .iter()
            .any(|a| a == &["agent", "prompt", "wfixture:p3", "two words\nand a line"])
    );
    assert!(
        !calls
            .iter()
            .any(|a| a.get(1).is_some_and(|x| x == "send-keys"))
    );
    service.close().await;
}

#[tokio::test]
async fn answering_blocked_pane_uses_explicit_keys() {
    let name = "answering_blocked_pane_uses_explicit_keys";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    service
        .answer(&key("wfixture:p2"), "yes")
        .await
        .expect("answer sent as explicit keys");
    let recorded = calls();
    assert!(
        recorded
            .iter()
            .any(|a| a == &["agent", "send-keys", "wfixture:p2", "y", "e", "s", "enter"]),
        "recorded calls: {recorded:?}"
    );
    assert!(
        !calls()
            .iter()
            .any(|a| a.get(1).is_some_and(|x| x == "prompt"))
    );
    service.poll_once().await.expect("updated state");
    assert!(
        !service
            .snapshot()
            .queue
            .iter()
            .any(|item| item.id.key == key("wfixture:p2"))
    );
    service.close().await;
}

#[tokio::test]
async fn interrupt_sends_key_and_observes_state_change() {
    let name = "interrupt_sends_key_and_observes_state_change";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    service
        .interrupt(&key("wfixture:p1"))
        .await
        .expect("interrupt");
    let calls = calls();
    assert!(
        calls
            .iter()
            .any(|a| a == &["agent", "send-keys", "wfixture:p1", "C-c"]
                || a == &["agent", "send-keys", "wfixture:p1", "esc"])
    );
    service.poll_once().await.expect("updated state");
    assert_eq!(
        service
            .snapshot()
            .rows
            .iter()
            .find(|r| r.key == key("wfixture:p1"))
            .expect("row")
            .state,
        RowState::Idle
    );
    service.close().await;
}

#[tokio::test]
async fn enter_routes_fleet_open_to_agent_focus() {
    let name = "enter_routes_fleet_open_to_agent_focus";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    assert!(
        !calls()
            .iter()
            .any(|a| a.get(1).is_some_and(|x| x == "focus"))
    );
    let snap = service.snapshot();
    let row = snap
        .rows
        .iter()
        .find(|row| row.key == key("wfixture:p3"))
        .expect("row")
        .clone();
    let mut view = FleetViewState::new(None, 1.0);
    update(
        &mut view,
        FleetViewInput::Snapshot {
            version: 1,
            rows: vec![row],
            queue: vec![],
            profiles: vec![],
            scope_note: None,
            link_note: None,
        },
        4,
    );
    let effects = update(&mut view, FleetViewInput::Enter, 4);
    let effect = effects
        .iter()
        .find(|effect| matches!(effect,FleetViewEffect::Open(key) if key.session_id=="wfixture:p3"))
        .expect("open effect");
    service.route_effect(effect).await.expect("focus live pane");
    assert!(
        calls()
            .iter()
            .any(|a| a == &["agent", "focus", "wfixture:p3"])
    );
    service.close().await;
}

#[tokio::test]
async fn background_discovery_filters_workspace_without_focus() {
    let name = "background_discovery_filters_workspace_without_focus";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(Some("wfixture".into()));
    service.poll_once().await.expect("discovery");
    let snap = service.snapshot();
    assert!(snap.rows.iter().any(|r| r.key == key("wfixture:p1")));
    assert!(!snap.rows.iter().any(|r| r.key == key("wother:p6")));
    assert!(!calls().iter().any(|a| {
        a.get(1)
            .is_some_and(|x| matches!(x.as_str(), "focus" | "prompt" | "send-keys"))
    }));
    service.close().await;
}

#[tokio::test]
async fn mixed_backend_actions_send_no_hermes_requests() {
    let name = "mixed_backend_actions_send_no_hermes_requests";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let gateway = mock_gateway::StandaloneGateway::start(&root());
    gateway.set_fleet_profiles(vec!["fixture".into()]);
    gateway.set_fleet_session(
        "fixture",
        json!({"id":"session-1","title":"Hermes","status":"idle","message_count":0}),
    );
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snap = service.snapshot();
        if snap
            .rows
            .iter()
            .any(|r| r.key.profile == "fixture" && r.key.session_id == "session-1")
            && snap.rows.iter().any(|r| r.key == key("wfixture:p3"))
        {
            break;
        }
        assert!(Instant::now() < deadline, "both backends visible");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    settle().await;
    let before = gateway.frame_count();
    service
        .prompt(&key("wfixture:p3"), "follow up")
        .await
        .expect("prompt");
    service
        .interrupt(&key("wfixture:p1"))
        .await
        .expect("interrupt");
    service.focus(&key("wfixture:p3")).await.expect("focus");
    settle().await;
    assert_eq!(
        gateway.frame_count(),
        before,
        "terminal actions send no gateway frames"
    );
    assert!(gateway.requests("approval.respond").is_empty());
    assert!(gateway.requests("session.interrupt").is_empty());
    service.close().await;
}

#[tokio::test]
async fn disappearing_pane_removes_its_row_and_queue_item() {
    let name = "disappearing_pane_removes_its_row_and_queue_item";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    assert!(
        service
            .snapshot()
            .queue
            .iter()
            .any(|need| need.id.key == key("wfixture:p2"))
    );
    let path = root().join("state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("state")).expect("json");
    state["agents"]
        .as_array_mut()
        .expect("agents")
        .retain(|agent| agent["pane_id"] != "wfixture:p2");
    std::fs::write(path, state.to_string()).expect("updated state");
    service.poll_once().await.expect("rediscovery");
    let snapshot = service.snapshot();
    assert!(
        !snapshot
            .rows
            .iter()
            .any(|row| row.key == key("wfixture:p2"))
    );
    assert!(
        !snapshot
            .queue
            .iter()
            .any(|need| need.id.key == key("wfixture:p2"))
    );
    service.close().await;
}

#[tokio::test]
async fn unavailable_binary_fails_without_blocking_the_ui() {
    let name = "unavailable_binary_fails_without_blocking_the_ui";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let config = talaria_app::herdr::HerdrConfig {
        enabled: true,
        binary: root().join("missing"),
        poll_interval_ms: 50,
        workspace_filter: None,
    };
    let fleet = talaria_app::fleet::FleetService::start(
        root(),
        "fixture".into(),
        talaria_app::fleet::FleetConfig::default(),
        |_| {},
    );
    let mut service = talaria_app::herdr::HerdrService::start(config, fleet);
    let result = tokio::time::timeout(Duration::from_secs(5), service.poll_once())
        .await
        .expect("bounded discovery");
    assert!(result.is_err());
    assert!(service.snapshot().rows.is_empty());
    assert!(calls().is_empty());
    service.close().await;
}

#[tokio::test]
async fn blocked_answer_rejects_embedded_control_keys() {
    let name = "blocked_answer_rejects_embedded_control_keys";
    if !in_child(name) {
        run_case(name, "");
        return;
    }
    let mut service = service(None);
    service.poll_once().await.expect("discovery");
    assert!(
        service
            .answer(&key("wfixture:p2"), "yes\nno")
            .await
            .is_err()
    );
    assert!(
        !calls()
            .iter()
            .any(|args| args.get(1).is_some_and(|arg| arg == "send-keys"))
    );
    service.close().await;
}
