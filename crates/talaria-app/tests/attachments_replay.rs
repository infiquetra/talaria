//! The recorded mention and image exchange. Issue 224.

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::ReplaySource;
use talaria_app::attachments::AttachmentsState;
use talaria_core::attachments::CollapseRule;

fn decode_base64(text: &str) -> Vec<u8> {
    fn value(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("content_base64 is not standard base64"),
        }
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index + 1 < bytes.len() {
        let b0 = value(bytes[index]);
        let b1 = value(bytes[index + 1]);
        out.push((b0 << 2) | (b1 >> 4));
        if bytes.get(index + 2).is_none_or(|byte| *byte == b'=') {
            break;
        }
        let b2 = value(bytes[index + 2]);
        out.push((b1 << 4) | (b2 >> 2));
        if bytes.get(index + 3).is_none_or(|byte| *byte == b'=') {
            break;
        }
        let b3 = value(bytes[index + 3]);
        out.push((b2 << 6) | b3);
        index += 4;
    }
    out
}

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
    let methods: Vec<_> = outbound
        .iter()
        .map(|frame| frame.frame["method"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        methods,
        [
            "complete.path",
            "image.attach_bytes",
            "prompt.submit",
            "session.resume"
        ]
    );
    assert_eq!(outbound[0].frame["params"]["word"], "@calc");
    assert_eq!(
        keys(&outbound[0].frame["params"]),
        ["session_id".to_string(), "word".to_string()]
    );
    let encoded = outbound[1].frame["params"]["content_base64"]
        .as_str()
        .expect("base64");
    let png =
        std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/red-1x1.png"))
            .expect("red image");
    assert_eq!(decode_base64(encoded), png);
    assert_eq!(
        outbound[2].frame["params"]["text"],
        "[Image #1] What colour is this image? One word."
    );
    assert!(outbound[2].frame["params"]["attachments"].is_array());
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
