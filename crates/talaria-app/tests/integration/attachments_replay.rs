//! The recorded mention and image exchange. Issue 224.

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::{REDACTED, ReplaySource};
use talaria_app::attachments::AttachmentsState;
use talaria_core::attachments::CollapseRule;

/// The fixture names the file stem, and the first typed edit is `@calc`.
#[test]
fn replays_recorded_mention_and_image() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/attachments-mention-image.jsonl");
    let source = ReplaySource::open(&path).expect("fixture");
    assert!(!source.truncated());
    assert_eq!(
        source.header().scenario.as_deref(),
        Some("attachments-mention-image")
    );
    let outbound: Vec<_> = source.outbound().collect();
    // The recording is a full session, so its outbound frames carry the session
    // handshake around the three the scenario sends. The frames are found by method
    // and the scenario's own three by their order, the way
    // `crates/hermes-client/tests/fixtures_replay.rs` finds them.
    let call = |method: &str| {
        outbound
            .iter()
            .find(|frame| frame.frame["method"] == method)
            .unwrap_or_else(|| panic!("no outbound {method}"))
    };
    let at = |method: &str| {
        outbound
            .iter()
            .position(|frame| frame.frame["method"] == method)
            .unwrap_or_else(|| panic!("no outbound {method}"))
    };
    let complete_path = call("complete.path");
    assert_eq!(complete_path.frame["params"]["word"], "@calc");
    assert_eq!(
        keys(&complete_path.frame["params"]),
        ["session_id".to_string(), "word".to_string()]
    );
    let attach = call("image.attach_bytes");
    // The recorder redacts the image bytes on disk, so the recording carries the
    // redaction marker where the payload was, not the PNG itself.
    assert_eq!(
        attach.frame["params"]["content_base64"].as_str(),
        Some(REDACTED)
    );
    assert_eq!(attach.frame["params"]["filename"].as_str(), Some(REDACTED));
    let submit = call("prompt.submit");
    assert_eq!(
        submit.frame["params"]["text"],
        "[Image #1] What colour is this image? One word."
    );
    assert!(submit.frame["params"]["attachments"].is_array());
    assert!(at("complete.path") < at("image.attach_bytes"));
    assert!(at("image.attach_bytes") < at("prompt.submit"));
    let completed = source.inbound().find(|frame| {
        frame.frame["method"] == "event" && frame.frame["params"]["type"] == "message.complete"
    });
    let completed = completed.expect("message.complete");
    assert_eq!(completed.frame["params"]["payload"]["status"], "complete");

    let mut state = AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024);
    let now = std::time::Instant::now();
    state.on_edit("@calc", 5, true, now);
    let request = state
        .mention
        .tick(now + Duration::from_millis(60))
        .expect("the word is sent after the debounce");
    assert_eq!(request.word, "@calc");
    assert_eq!(request.generation, 1);
}

fn keys(value: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
    names.sort();
    names
}
