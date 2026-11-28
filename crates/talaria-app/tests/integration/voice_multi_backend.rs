//! Fleet transcripts wait for Enter and stay off the Hermes session wire.

use crate::acp_harness;
use crate::mock_gateway;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use talaria_app::acp::PermissionResponder;
use talaria_app::voice::fleet::{FleetVoiceCommit, FleetVoiceDelivery, commit_fleet_transcript};
use talaria_core::fleet::RowKey;
use talaria_core::fleet::RowState;

fn follow_up_script() -> Vec<Value> {
    let mut steps = acp_harness::standard_hello();
    steps.extend([
        acp_harness::expect("session/prompt"),
        acp_harness::reply(json!({"stopReason":"end_turn"})),
        acp_harness::expect("session/prompt"),
        acp_harness::reply(json!({"stopReason":"end_turn"})),
    ]);
    steps.extend(acp_harness::finish());
    steps
}

fn permission_script() -> Vec<Value> {
    let mut steps = acp_harness::standard_hello();
    steps.extend([
        acp_harness::expect("session/prompt"),
        acp_harness::permission(7),
        acp_harness::response(7),
        acp_harness::reply(json!({"stopReason":"end_turn"})),
    ]);
    steps.extend(acp_harness::finish());
    steps
}

fn prompt_texts() -> Vec<String> {
    acp_harness::messages("session/prompt")
        .into_iter()
        .filter_map(|message| {
            message
                .pointer("/params/prompt/0/text")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn fleet_row(agent_id: &str, session_id: &str) -> RowKey {
    RowKey {
        profile: format!("acp:{agent_id}"),
        session_id: session_id.to_owned(),
    }
}

fn assert_no_hermes_session_frames(gateway: &mock_gateway::StandaloneGateway, before: usize) {
    let seen = gateway.seen();
    let forbidden = seen.iter().skip(before).any(|frame| {
        frame.method.as_deref().is_some_and(|method| {
            method.starts_with("voice.")
                || method == "prompt.submit"
                || method == "approval.respond"
        })
    });
    assert!(
        !forbidden,
        "an external voice target sends no Hermes session frames"
    );
}

async fn finish_setup_turn(service: &mut talaria_app::acp::AcpService, session_id: &str) {
    service
        .prompt(
            &talaria_core::fleet::acp::AcpSessionKey {
                agent_id: "fixture".to_owned(),
                session_id: session_id.to_owned(),
            },
            "go",
        )
        .await
        .expect("setup prompt");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snap = service.fleet_snapshot();
        let idle = snap
            .rows
            .iter()
            .any(|row| row.key.session_id == session_id && row.state != RowState::Working);
        if idle && prompt_texts().iter().any(|text| text == "go") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the setup turn finishes before the transcript"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn acp_transcript_waits_for_enter() {
    let name = "voice_multi_backend::acp_transcript_waits_for_enter";
    if !acp_harness::in_child(name) {
        acp_harness::run_case(name, "", |_| follow_up_script());
        return;
    }
    let mut harness = acp_harness::Harness::new(acp_harness::agent());
    let session = harness
        .service
        .new_session("fixture", &acp_harness::root().join("work"), "")
        .await
        .expect("session");
    finish_setup_turn(&mut harness.service, &session.session_id).await;
    let row = fleet_row(&session.agent_id, &session.session_id);
    let waiting = FleetVoiceCommit {
        key: row,
        text: "ship the fix".to_owned(),
        entered: false,
        delivery: FleetVoiceDelivery::Prompt,
    };
    commit_fleet_transcript(&waiting, Some(&mut harness.service)).await;
    acp_harness::settle().await;
    assert!(
        !prompt_texts().iter().any(|text| text == "ship the fix"),
        "an ACP transcript is not sent before Enter"
    );
    let entered = FleetVoiceCommit {
        entered: true,
        ..waiting
    };
    commit_fleet_transcript(&entered, Some(&mut harness.service)).await;
    acp_harness::settle().await;
    assert!(
        prompt_texts().iter().any(|text| text == "ship the fix"),
        "Enter sends the ACP transcript"
    );
    harness.service.close().await;
}

#[tokio::test]
async fn permission_answer_waits_for_enter() {
    let name = "voice_multi_backend::permission_answer_waits_for_enter";
    if !acp_harness::in_child(name) {
        acp_harness::run_case(name, "", |_| permission_script());
        return;
    }
    let mut harness = acp_harness::Harness::new(acp_harness::agent());
    let session = harness
        .service
        .new_session("fixture", &acp_harness::root().join("work"), "")
        .await
        .expect("session");
    harness
        .service
        .prompt(&session, "go")
        .await
        .expect("prompt");
    let deadline = Instant::now() + Duration::from_secs(5);
    let options = loop {
        let snap = harness.service.fleet_snapshot();
        if let Some(item) = snap.queue.iter().find(|item| {
            item.external_options
                .iter()
                .any(|option| option.name == "Allow")
        }) {
            break item.external_options.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the permission is waiting to be answered"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let row = fleet_row(&session.agent_id, &session.session_id);
    let responder = PermissionResponder {
        generation: 1,
        request_id: "7".to_owned(),
        key: session,
        tool_call_id: "tool-1".to_owned(),
    };
    let waiting = FleetVoiceCommit {
        key: row,
        text: "  ALLOW  ".to_owned(),
        entered: false,
        delivery: FleetVoiceDelivery::Permission {
            responder: responder.clone(),
            options: options.clone(),
        },
    };
    commit_fleet_transcript(&waiting, Some(&mut harness.service)).await;
    acp_harness::settle().await;
    assert!(
        acp_harness::responses(7).is_empty(),
        "a permission answer waits for Enter"
    );
    let entered = FleetVoiceCommit {
        entered: true,
        ..waiting
    };
    commit_fleet_transcript(&entered, Some(&mut harness.service)).await;
    acp_harness::settle().await;
    let answers = acp_harness::responses(7);
    assert!(!answers.is_empty(), "Enter sends the permission answer");
    assert_eq!(
        answers[0]["result"],
        json!({"outcome":{"outcome":"selected","optionId":"allow"}})
    );
    harness.service.close().await;
}

#[tokio::test]
async fn zero_hermes_traffic_on_external_voice() {
    let name = "voice_multi_backend::zero_hermes_traffic_on_external_voice";
    if acp_harness::in_child(name) {
        external_acp_sends_no_hermes_frames().await;
        return;
    }
    acp_harness::run_case(name, "", |_| follow_up_script());
}

async fn external_acp_sends_no_hermes_frames() {
    let gateway = mock_gateway::StandaloneGateway::start(&acp_harness::root());
    let mut harness = acp_harness::Harness::new(acp_harness::agent());
    let session = harness
        .service
        .new_session("fixture", &acp_harness::root().join("work"), "")
        .await
        .expect("session");
    finish_setup_turn(&mut harness.service, &session.session_id).await;
    let before = gateway.frame_count();
    let entered = FleetVoiceCommit {
        key: fleet_row(&session.agent_id, &session.session_id),
        text: "ship the fix".to_owned(),
        entered: true,
        delivery: FleetVoiceDelivery::Prompt,
    };
    commit_fleet_transcript(&entered, Some(&mut harness.service)).await;
    acp_harness::settle().await;
    assert!(
        prompt_texts().iter().any(|text| text == "ship the fix"),
        "Enter delivers the ACP transcript before the gateway is checked"
    );
    assert_no_hermes_session_frames(&gateway, before);
    harness.service.close().await;
}
