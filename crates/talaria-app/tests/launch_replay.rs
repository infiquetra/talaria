//! Recorded frames for a background launch. The fixture is produced by the recorder, not by hand.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use mock_gateway::StandaloneGateway;
use serde_json::Value;
use talaria_app::bg::machine::NewSessionSpec;
use talaria_app::bg::{LaunchConn, LaunchLink, wire};

/// The recorded call and reply of one method: the call's param keys and the reply's keys.
#[derive(Default)]
struct Exchange {
    params: BTreeSet<String>,
    reply: BTreeSet<String>,
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

/// Each recorded call by method, matched with its reply by id.
fn recorded() -> BTreeMap<String, Exchange> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/bg-launch.jsonl");
    let recorded = std::fs::read(&path).is_ok_and(|bytes| !bytes.is_empty());
    if !recorded {
        panic!("bg-launch fixture not recorded (BLOCKED)");
    }
    let bytes = std::fs::read(&path).unwrap_or_default();
    let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("recorded frame is not text"));
    let mut methods: BTreeMap<String, String> = BTreeMap::new();
    let mut exchanges: BTreeMap<String, Exchange> = BTreeMap::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let record: Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("recorded frame is not json"));
        // Each line wraps one frame: `{"kind":"frame","dir":"out"|"in","frame":{...}}`.
        let frame = &record["frame"];
        let id = frame["id"].to_string();
        match (record["dir"].as_str(), frame["method"].as_str()) {
            (Some("out"), Some(method)) => {
                methods.insert(id, method.to_owned());
                exchanges.entry(method.to_owned()).or_default().params = keys(&frame["params"]);
            }
            (Some("in"), None) => {
                if let Some(method) = methods.get(&id) {
                    exchanges.entry(method.clone()).or_default().reply = keys(&frame["result"]);
                }
            }
            _ => {}
        }
    }
    exchanges
}

#[tokio::test]
async fn mock_replies_match_recorded_frames() {
    let recorded = recorded();
    let create = recorded.get("session.create").expect("a recorded create");
    let submit = recorded.get("prompt.submit").expect("a recorded submit");
    let detach = recorded.get("session.detach").expect("a recorded detach");
    assert!(
        create.params.contains("request_id"),
        "recorded create has no request id"
    );
    assert!(create.params.contains("source"));
    assert!(submit.params.contains("text"));
    assert!(detach.params.contains("session_id"));
    let forbidden = ["title", "hidden", "cwd_explicit", "finite", "unattended"];
    for key in forbidden {
        assert!(!create.params.contains(key));
        assert!(!submit.params.contains(key));
        assert!(!detach.params.contains(key));
    }

    // The mock's replies to the same three calls have the recorded key sets.
    let home = tempfile::tempdir().expect("home");
    let _gateway = StandaloneGateway::start(home.path());
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    let conn = LaunchConn::Link(link);
    let spec = NewSessionSpec {
        request_id: "0123456789abcdef0123456789abcdef".to_owned(),
        cwd: std::env::temp_dir().canonicalize().expect("temp"),
        model: None,
        provider: None,
        reasoning: None,
    };
    let limit = Duration::from_secs(5);
    let created = conn
        .call("session.create", wire::create_params(&spec, None), limit)
        .await
        .expect("create");
    assert_eq!(keys(&created), create.reply, "session.create reply keys");
    let session = wire::read_created(&created).expect("a created session");
    let submitted = conn
        .call(
            "prompt.submit",
            wire::submit_params(&session.session_id, "ready", "sub-1", None),
            limit,
        )
        .await
        .expect("submit");
    assert_eq!(keys(&submitted), submit.reply, "prompt.submit reply keys");
    let detached = conn
        .call(
            "session.detach",
            wire::detach_params(&session.session_id, &session.subscription_id, None, None),
            limit,
        )
        .await
        .expect("detach");
    assert_eq!(keys(&detached), detach.reply, "session.detach reply keys");
    conn.close().await;
}
