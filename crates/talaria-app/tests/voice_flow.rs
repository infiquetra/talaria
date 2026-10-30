//! Voice through the application service and a loopback gateway.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "support/voice_gateway.rs"]
mod voice_gateway;

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::replay_source::ReplaySource;
use talaria_app::voice::VoiceService;

fn script() -> ReplaySource {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/voice/press_submit.jsonl");
    ReplaySource::open(&path).expect("contract-built script opens")
}

#[test]
fn press_speak_submit_round_trip() {
    let _ = voice_gateway::voice_handlers_registered();
    let source = script();
    assert_eq!(source.header().version, 3);
    assert_eq!(source.header().scenario.as_deref(), Some("contract-built"));
    assert!(
        source.outbound().next().is_none(),
        "the script records gateway replies only"
    );
    assert!(
        source.inbound().next().is_some(),
        "the script has gateway replies"
    );
    let frames = VoiceService::new().submit_from_script(&source);
    let submit = frames
        .iter()
        .find(|frame| frame["method"] == "prompt.submit");
    let Some(submit) = submit else {
        panic!("prompt.submit was not sent");
    };
    let params = submit["params"].as_object().expect("prompt.submit params");
    let mut keys: Vec<_> = params.keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "session_id".to_owned(),
            "submission_id".to_owned(),
            "text".to_owned()
        ]
    );
    assert_eq!(params["session_id"], "fixture-session");
    assert_eq!(params["text"], "what is two plus two");
    assert!(submit.get("voice_turn").is_none(), "no voice_turn flag");
    assert!(params.get("voice_turn").is_none(), "no voice_turn field");
}

#[tokio::test]
async fn read_aloud_and_voice_stopped() {
    let cells = vec![
        "The answer is four.".to_owned(),
        "```\n1 + 1\n```".to_owned(),
    ];
    let frames = VoiceService::new().read_aloud_frames(&cells);
    assert_eq!(frames.len(), 1, "one voice.tts");
    assert_eq!(frames[0]["method"], "voice.tts");
    assert_eq!(
        frames[0]["params"]["text"],
        "The answer is four. (code omitted)"
    );
    assert!(frames[0].get("voice_turn").is_none());

    assert!(
        !VoiceService::new().read_aloud_after_error(5026),
        "5026 turns read-aloud off"
    );

    let stopped = VoiceService::new().after_voice_stopped("fixture-session");
    assert!(
        stopped.iter().any(|frame| {
            frame["method"] == "config.get"
                && frame["params"]["key"] == "voice"
                && frame["params"]["session_id"] == "fixture-session"
        }),
        "VoiceStopped re-reads that session: {stopped:?}"
    );

    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let sessions = talaria_app::sessions::SessionService::with_client("voice-test", client.clone());
    let opened = sessions
        .open(talaria_app::launch::SessionRequest::ById {
            session_id: mock_gateway::SESSION_ID.into(),
        })
        .await
        .expect("resume a recorded session");
    let mut voice = VoiceService::new();
    voice.open(&opened.session_id);
    voice.update(
        &opened.session_id,
        talaria_core::voice::VoiceInput::ModeRead(Some("voice_only".into())),
        true,
    );
    voice.update(
        &opened.session_id,
        talaria_core::voice::VoiceInput::Press,
        true,
    );
    gateway.push_method_result("voice.record", serde_json::json!({"status":"recording"}));
    let started = hermes_client::voice_api::voice_record(&client, &opened.session_id, true)
        .await
        .expect("start recording");
    assert_eq!(
        started.status,
        hermes_contract::schemas::VoiceRecordStatus::Recording
    );
    voice.update(
        &opened.session_id,
        talaria_core::voice::VoiceInput::RecordReply {
            start: true,
            status: talaria_core::voice::RecordStatus::Recording,
            reason: None,
        },
        true,
    );
    gateway.set_silent("voice.record", true);
    let mut err = Vec::new();
    let stop_started = std::time::Instant::now();
    voice
        .stop_before_detach(&client, &opened.session_id, &mut err)
        .await;
    assert!(
        stop_started.elapsed() >= Duration::from_millis(500),
        "the stop budget elapsed"
    );
    sessions.detach(&opened).await;
    let stops = gateway.requests("voice.record");
    let detach = gateway.requests("session.detach");
    assert_eq!(stops.len(), 2, "start and stop reached the gateway");
    assert_eq!(stops[1].params["action"], "stop");
    assert_eq!(stops[1].params["session_id"], opened.session_id);
    assert_eq!(detach.len(), 1);
    assert!(stops[1].index < detach[0].index, "stop precedes detach");
    let frames: Vec<_> = gateway.seen().into_iter().map(|seen| seen.raw).collect();
    let line = String::from_utf8(err).unwrap_or_default();
    assert!(
        line.contains(
            "The gateway microphone may still be recording; it stops on silence or at the recording limit."
        ),
        "missing the open-microphone line: {line:?}"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        frames
            .iter()
            .any(|frame| frame["method"] == "session.detach"),
        "session.detach was not sent after the budget"
    );
}
