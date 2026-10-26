//! Replay of the recorded peek fixture.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fake_fleet::FakeFleetSink;
use hermes_client::liveness::LinkEvent;
use hermes_client::{Decoded, Generation, Inbound, ReplaySource, decode_frame};
use serde_json::Value;
use talaria_app::launch::{Launch, StartMode};
use talaria_app::peek::{PeekService, PeekTimeouts};
use talaria_core::session::{Cell, PromptKind, SessionState};

#[tokio::test]
async fn recorded_peek_fixture_replays() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/fleet-peek.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        panic!("fleet-peek fixture not recorded (BLOCKED)");
    }
    let source = ReplaySource::open(&path)
        .unwrap_or_else(|_| panic!("the fleet-peek recording could not be read"));
    assert!(
        !text.contains("unattended"),
        "the recording contains an unattended flag"
    );
    for frame in source.inbound().chain(source.outbound()) {
        assert!(
            profile_fields_allowed(&frame.frame),
            "a profile field is not fixture or <profile>"
        );
    }
    let mut seen = BTreeSet::new();
    for frame in source.outbound() {
        let Some(method) = frame.frame["method"].as_str() else {
            continue;
        };
        if matches!(
            method,
            "session.resume" | "prompt.submit" | "approval.respond" | "session.detach"
        ) {
            assert!(
                exact_params(&frame.frame, method),
                "outbound {method} does not have the expected parameter keys"
            );
            seen.insert(method);
        }
        let encoded = frame.frame.to_string();
        assert!(
            !encoded.contains("end_reason"),
            "a detach carried an end reason"
        );
        assert!(
            !encoded.contains("\"queued\":true") && !encoded.contains("\"queued\": true"),
            "a call forced the queue flag"
        );
    }
    for method in [
        "session.resume",
        "prompt.submit",
        "approval.respond",
        "session.detach",
    ] {
        assert!(seen.contains(method), "the recording has no {method}");
    }

    let home = tempfile::tempdir().expect("home");
    let mut peek = service(home.path());
    let mut phase = 0u8;
    for frame in source.inbound() {
        if frame.frame["method"] != "event" {
            continue;
        }
        let bytes = serde_json::to_vec(&frame.frame).unwrap_or_default();
        let decoded = decode_frame(Generation(frame.generation), &bytes, &mut |_| {
            unreachable!("an event frame is not a server request")
        });
        let Decoded::Inbound(item) = decoded else {
            continue;
        };
        if !matches!(item, Inbound::Event(_)) {
            continue;
        }
        let _ = peek.on_link_event(LinkEvent::Inbound {
            item,
            replayed: true,
        });
        if let Some((_, session)) = peek.state() {
            phase = advance(phase, session);
        }
    }
    assert_eq!(
        phase, 3,
        "the panel did not show peek-ok, then the approval card, then the card gone"
    );
}

fn advance(phase: u8, session: &SessionState) -> u8 {
    let output = saw_peek_ok(session);
    let card = approval_open(session);
    match phase {
        0 if output => 1,
        1 if card => 2,
        2 if !card => 3,
        other => other,
    }
}

fn saw_peek_ok(session: &SessionState) -> bool {
    session.cells().iter().any(|cell| match cell {
        Cell::Assistant(cell) => cell.text.contains("peek-ok"),
        Cell::Tool(cell) => {
            cell.context
                .as_deref()
                .is_some_and(|text| text.contains("peek-ok"))
                || cell
                    .result
                    .as_deref()
                    .is_some_and(|text| text.contains("peek-ok"))
                || cell
                    .summary
                    .as_deref()
                    .is_some_and(|text| text.contains("peek-ok"))
        }
        _ => false,
    })
}

fn approval_open(session: &SessionState) -> bool {
    session
        .open_prompts()
        .any(|prompt| prompt.kind == PromptKind::Approval)
}

fn profile_fields_allowed(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().all(|(key, inner)| {
            if matches!(key.as_str(), "profile" | "profile_id" | "profile_name")
                && let Some(name) = inner.as_str()
                && name != "fixture"
                && name != "<profile>"
            {
                return false;
            }
            profile_fields_allowed(inner)
        }),
        Value::Array(items) => items.iter().all(profile_fields_allowed),
        _ => true,
    }
}

fn exact_params(frame: &Value, method: &str) -> bool {
    let Some(params) = frame.get("params").and_then(Value::as_object) else {
        return false;
    };
    let keys: BTreeSet<&str> = params.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = match method {
        "session.resume" => ["session_id", "display_projection"].into(),
        "prompt.submit" => ["session_id", "text", "submission_id"].into(),
        "approval.respond" => ["session_id", "execution_generation", "prompt_id", "choice"].into(),
        "session.detach" => ["session_id", "subscription_id"].into(),
        _ => return true,
    };
    keys == expected
}

fn service(home: &Path) -> PeekService {
    let sink = Arc::new(FakeFleetSink::new());
    PeekService::new(
        Launch {
            mode: StartMode::New,
            model: Some("minimax-m3.1-flash".to_owned()),
            provider: Some("ollama-cloud".to_owned()),
            effort: None,
            profile: "fixture".to_owned(),
            folder: PathBuf::from("."),
            record: None,
            bench_replay: None,
            no_ensure: true,
        },
        home.to_path_buf(),
        sink,
        Box::new(|| "talaria-0123456789abcdef-7".to_owned()),
        PeekTimeouts::scaled_for_tests(),
    )
}
