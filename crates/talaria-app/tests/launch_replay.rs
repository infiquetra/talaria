//! Recorded frames for a background launch. The fixture is produced by the recorder, not by hand.

use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn mock_replies_match_recorded_frames() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/bg-launch.jsonl");
    let recorded = std::fs::read(&path).is_ok_and(|bytes| !bytes.is_empty());
    if !recorded {
        panic!("bg-launch fixture not recorded (BLOCKED)");
    }
    let bytes = std::fs::read(&path).unwrap_or_default();
    let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("recorded frame is not text"));
    let mut create_keys = BTreeSet::new();
    let mut submit_keys = BTreeSet::new();
    let mut detach_keys = BTreeSet::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("recorded frame is not json"));
        let method = value
            .get("method")
            .and_then(|item| item.as_str())
            .unwrap_or("");
        let keys = value
            .get("params")
            .or_else(|| value.get("result"))
            .and_then(|item| item.as_object())
            .map(|object| object.keys().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        match method {
            "session.create" => create_keys = keys,
            "prompt.submit" => submit_keys = keys,
            "session.detach" => detach_keys = keys,
            _ => {}
        }
    }
    assert!(
        create_keys.contains("request_id"),
        "recorded create has no request id"
    );
    assert!(create_keys.contains("source"));
    assert!(submit_keys.contains("text"));
    assert!(detach_keys.contains("session_id"));
    let forbidden = ["title", "hidden", "cwd_explicit", "finite", "unattended"];
    for key in forbidden {
        assert!(!create_keys.contains(key));
        assert!(!submit_keys.contains(key));
        assert!(!detach_keys.contains(key));
    }
}
