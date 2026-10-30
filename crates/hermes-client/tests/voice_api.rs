//! Voice wrappers against a loopback peer. Nothing is sent until the wrappers exist.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use hermes_client::voice_api::{
    set_voice_mode, voice_mode, voice_record, voice_requirements, voice_speak, voice_submit_mode,
};
use hermes_client::{CallError, Client, InboundStream};
use hermes_contract::schemas::VoiceRecordStatus;
use serde_json::{Value, json};
use support::peer::Peer;
use support::voice_peer::next_request;

async fn pair() -> (Client, InboundStream, Peer) {
    let (parts, peer) = Peer::pair().await;
    let (client, inbound) = Client::start(parts);
    (client, inbound, peer)
}

async fn sent<T>(
    call: impl std::future::Future<Output = Result<T, CallError>>,
    peer: &mut Peer,
) -> Option<Value> {
    tokio::pin!(call);
    tokio::select! {
        _ = &mut call => None,
        frame = next_request(peer) => frame,
    }
}

fn properties(schema: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hermes-contract/vendor/gateway-contract.openrpc.json");
    let text = fs::read_to_string(&path).expect("vendored contract");
    let value: Value = serde_json::from_str(&text).expect("vendored contract");
    value["components"]["schemas"][schema]["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("missing schema {schema}"))
        .keys()
        .cloned()
        .collect()
}

fn keys_of(frame: &Value) -> BTreeSet<String> {
    frame["params"]
        .as_object()
        .unwrap_or_else(|| panic!("params object: {frame}"))
        .keys()
        .cloned()
        .collect()
}

fn forbid(frame: &Value) {
    let text = frame.to_string();
    assert!(!text.contains("voice_turn"), "voice_turn in {text}");
    assert!(
        !text.contains("voice_chat_mode"),
        "voice_chat_mode in {text}"
    );
    assert!(!text.contains("/api/audio"), "audio route in {text}");
    assert!(!text.contains("wake."), "wake method in {text}");
    assert!(
        frame["params"].get("profile").is_none(),
        "profile in {frame}"
    );
    let params = frame["params"].as_object().expect("params");
    for (key, value) in params {
        assert!(!value.is_null(), "null {key} in {frame}");
    }
    if frame["method"] == "voice.toggle" {
        assert_eq!(frame["params"]["action"], "status");
    }
}

fn expect_frame(frame: Option<Value>, what: &str) -> Value {
    frame.unwrap_or_else(|| panic!("{what} was not sent"))
}

#[tokio::test]
async fn request_shapes_match_contract() {
    let (client, _inbound, mut peer) = pair().await;
    let frame = expect_frame(
        sent(voice_record(&client, "fixture-session", true), &mut peer).await,
        "voice.record start",
    );
    assert_eq!(frame["method"], "voice.record");
    assert_eq!(frame["params"]["action"], "start");
    assert_eq!(frame["params"]["session_id"], "fixture-session");
    assert_eq!(
        keys_of(&frame),
        BTreeSet::from(["action".to_owned(), "session_id".to_owned()])
    );
    assert!(keys_of(&frame).is_subset(&properties("VoiceRecordParams")));
    forbid(&frame);

    let frame = expect_frame(
        sent(voice_record(&client, "fixture-session", false), &mut peer).await,
        "voice.record stop",
    );
    assert_eq!(frame["method"], "voice.record");
    assert_eq!(frame["params"]["action"], "stop");
    forbid(&frame);

    let frame = expect_frame(
        sent(voice_requirements(&client), &mut peer).await,
        "voice.toggle status",
    );
    assert_eq!(frame["method"], "voice.toggle");
    assert_eq!(frame["params"]["action"], "status");
    assert!(frame["params"].get("session_id").is_none());
    assert_eq!(keys_of(&frame), BTreeSet::from(["action".to_owned()]));
    assert!(keys_of(&frame).is_subset(&properties("VoiceToggleParams")));
    forbid(&frame);

    let frame = expect_frame(
        sent(voice_speak(&client, "Talaria voice check."), &mut peer).await,
        "voice.tts",
    );
    assert_eq!(frame["method"], "voice.tts");
    assert_eq!(frame["params"]["text"], "Talaria voice check.");
    assert_eq!(keys_of(&frame), BTreeSet::from(["text".to_owned()]));
    assert!(keys_of(&frame).is_subset(&properties("VoiceTtsParams")));
    forbid(&frame);

    let frame = expect_frame(
        sent(set_voice_mode(&client, "fixture-session", true), &mut peer).await,
        "slash.exec voice on",
    );
    assert_eq!(frame["method"], "slash.exec");
    assert_eq!(frame["params"]["command"], "voice on");
    assert_eq!(frame["params"]["session_id"], "fixture-session");
    assert!(keys_of(&frame).is_subset(&properties("SlashExecParams")));
    forbid(&frame);

    let frame = expect_frame(
        sent(set_voice_mode(&client, "fixture-session", false), &mut peer).await,
        "slash.exec voice off",
    );
    assert_eq!(frame["params"]["command"], "voice off");
    forbid(&frame);

    let frame = expect_frame(
        sent(voice_mode(&client, "fixture-session"), &mut peer).await,
        "config.get voice",
    );
    assert_eq!(frame["method"], "config.get");
    assert_eq!(frame["params"]["key"], "voice");
    assert_eq!(frame["params"]["session_id"], "fixture-session");
    assert!(keys_of(&frame).is_subset(&properties("ConfigGetParams")));
    forbid(&frame);

    let frame = expect_frame(
        sent(voice_submit_mode(&client), &mut peer).await,
        "config.get full",
    );
    assert_eq!(frame["method"], "config.get");
    assert_eq!(frame["params"]["key"], "full");
    assert!(frame["params"].get("session_id").is_none());
    forbid(&frame);
}

async fn record_error(code: i64, message: &str) -> CallError {
    let (client, _inbound, mut peer) = pair().await;
    let call = voice_record(&client, "fixture-session", true);
    tokio::pin!(call);
    let frame = tokio::select! {
        _ = &mut call => None,
        frame = next_request(&mut peer) => frame,
    };
    let frame = expect_frame(frame, "voice.record");
    peer.reply_err(frame["id"].clone(), code, message, Some("fixture"))
        .await;
    match call.await {
        Ok(_) => panic!("{code} was decoded as success"),
        Err(err) => err,
    }
}

#[tokio::test]
async fn errors_surface_codes_and_messages() {
    for (code, message) in [
        (4015, "voice off"),
        (4016, "another session"),
        (4019, "bad request"),
        (5025, "gateway failed"),
        (5026, "speech failed"),
        (4020, "empty text"),
        (-32601, "method not found"),
    ] {
        let err = record_error(code, message).await;
        let CallError::Rpc(rpc) = err else {
            panic!("{code} did not reach CallError: {err:?}");
        };
        assert_eq!(rpc.code, code);
        assert_eq!(rpc.message, message);
    }

    let (client, _inbound, mut peer) = pair().await;
    let call = voice_record(&client, "fixture-session", true);
    tokio::pin!(call);
    let frame = tokio::select! {
        _ = &mut call => None,
        frame = next_request(&mut peer) => frame,
    };
    let frame = expect_frame(frame, "voice.record busy");
    peer.reply_ok(
        frame["id"].clone(),
        json!({"status": "busy", "reason": "wake_owned"}),
    )
    .await;
    let ok = call.await.expect("busy is a successful record reply");
    assert_eq!(ok.status, VoiceRecordStatus::Busy);
    assert_eq!(ok.reason.as_deref(), Some("wake_owned"));
}

#[tokio::test]
async fn requirements_absent_fields_are_unknown() {
    let (client, _inbound, mut peer) = pair().await;
    let call = voice_requirements(&client);
    tokio::pin!(call);
    let frame = tokio::select! {
        _ = &mut call => None,
        frame = next_request(&mut peer) => frame,
    };
    let frame = expect_frame(frame, "voice.toggle status");
    peer.reply_ok(
        frame["id"].clone(),
        json!({"enabled": false, "record_key": "alt+r", "tts": false}),
    )
    .await;
    let result = call
        .await
        .expect("a reply with only enabled, record_key and tts decodes");
    assert!(result.audio_available.is_none(), "audio");
    assert!(result.stt_available.is_none(), "speech-to-text");
    assert!(result.available.is_none(), "ready");
    assert!(!result.enabled);
    assert_eq!(result.record_key, "alt+r");
    assert!(!result.tts);
}

#[tokio::test]
async fn forbidden_requests_never_sent() {
    let (client, _inbound, mut peer) = pair().await;
    let frame = expect_frame(
        sent(voice_requirements(&client), &mut peer).await,
        "voice.toggle status",
    );
    assert_eq!(frame["method"], "voice.toggle");
    assert_eq!(frame["params"]["action"], "status");
    forbid(&frame);

    let mut frames = vec![frame];
    frames.push(expect_frame(
        sent(voice_record(&client, "fixture-session", true), &mut peer).await,
        "voice.record",
    ));
    frames.push(expect_frame(
        sent(voice_speak(&client, "hello"), &mut peer).await,
        "voice.tts",
    ));
    frames.push(expect_frame(
        sent(set_voice_mode(&client, "fixture-session", true), &mut peer).await,
        "slash.exec",
    ));
    frames.push(expect_frame(
        sent(voice_mode(&client, "fixture-session"), &mut peer).await,
        "config.get",
    ));
    frames.push(expect_frame(
        sent(voice_submit_mode(&client), &mut peer).await,
        "config.get full",
    ));
    for frame in &frames {
        forbid(frame);
        assert_ne!(frame["method"], "wake.start");
        if frame["method"] == "voice.toggle" {
            assert_eq!(frame["params"]["action"], "status");
        }
    }
}
