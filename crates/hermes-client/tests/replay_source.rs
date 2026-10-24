//! Test row 9: `ReplaySource` reads a version 3 frame log and feeds the inbound frames
//! back through the real decode function with no socket.
//!
//! Every file here is written by hand from the format in `docs/formats/frame-log.md`, so these
//! tests do not depend on the `Recorder`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::{
    Decoded, Generation, Inbound, MalformedKind, ReplayError, ReplaySource, Responder, Speed,
    decode_frame,
};
use hermes_contract::events::{Notification, ServerRequest};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn header_with(version: u64) -> String {
    json!({
        "kind": "header",
        "version": version,
        "startedAt": "2026-10-23T09:14:02.331Z",
        "endpoint": "ws://<gateway>/api/ws",
        "subprotocols": ["hermes-gateway-v1"],
    })
    .to_string()
}

fn header() -> String {
    header_with(3)
}

fn record(seq: u64, t_ms: u64, generation: u64, dir: &str, frame: Value) -> String {
    json!({
        "kind": "frame",
        "seq": seq,
        "tMs": t_ms,
        "generation": generation,
        "dir": dir,
        "frame": frame,
    })
    .to_string()
}

fn inbound(seq: u64, t_ms: u64, generation: u64, frame: Value) -> String {
    record(seq, t_ms, generation, "in", frame)
}

fn outbound(seq: u64, t_ms: u64, generation: u64, frame: Value) -> String {
    record(seq, t_ms, generation, "out", frame)
}

fn event(kind: &str, seq: u64, payload: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": kind,
            "session_id": "s1",
            "seq": seq,
            "payload": payload,
        },
    })
}

fn server_request(id: &str, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// Writes the lines (each ending in a newline, except possibly the last) and keeps the directory
/// alive for as long as the returned guard lives.
fn write_log(lines: &[String], final_newline: bool) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("log.jsonl");
    let mut text = lines.join("\n");
    if final_newline {
        text.push('\n');
    }
    std::fs::write(&path, text).expect("write the synthetic log");
    (dir, path)
}

/// One word for what the shared decode function made of a frame. It never prints frame text.
fn summarize(decoded: &Decoded) -> String {
    match decoded {
        Decoded::Reply(reply) => format!("reply:{}", reply.id),
        Decoded::Inbound(Inbound::Event(event)) => {
            format!("event:{}", event.raw["type"].as_str().unwrap_or("?"))
        }
        Decoded::Inbound(Inbound::ServerRequest(request)) => format!("request:{}", request.id),
        Decoded::Inbound(Inbound::Malformed(_)) => "malformed".to_owned(),
        Decoded::Inbound(Inbound::Closed(_)) => "closed".to_owned(),
    }
}

/// Runs the source to the end at instant speed and returns the items it sent.
async fn collect(source: &ReplaySource) -> (usize, Vec<hermes_client::ReplayItem>) {
    let (sink, mut receiver) = mpsc::channel(256);
    let sent = source
        .run(Speed::Instant, sink)
        .await
        .expect("an instant replay of a good log");
    let mut items = Vec::new();
    while let Ok(item) = receiver.try_recv() {
        items.push(item);
    }
    (sent, items)
}

// ---------------------------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn round_trip_yields_the_same_inbound_sequence() {
    let srq = "srq-0123456789ab";
    let written_inbound: Vec<(u64, Value)> = vec![
        (1, event("message.start", 1, json!({}))),
        (
            3,
            json!({"jsonrpc": "2.0", "id": 3, "result": {"ok": true}}),
        ),
        (6, event("message.delta", 2, json!({"text": "pong"}))),
        (8, server_request(srq, "sudo", json!({"session_id": "s1"}))),
        (
            10,
            event("message.complete", 3, json!({"status": "complete"})),
        ),
    ];
    let lines = vec![
        header(),
        inbound(1, 0, 1, written_inbound[0].1.clone()),
        outbound(
            2,
            5,
            1,
            json!({"jsonrpc": "2.0", "id": 3, "method": "prompt.submit",
                   "params": {"session_id": "s1", "text": "hi"}}),
        ),
        inbound(3, 10, 1, written_inbound[1].1.clone()),
        // Text that was not JSON: recorded with a null frame, nothing to replay.
        json!({"kind": "frame", "seq": 4, "tMs": 12, "generation": 1, "dir": "in",
               "frame": null, "parseError": "expected value at line 1 column 1"})
        .to_string(),
        json!({"kind": "dropped", "seq": 5, "tMs": 13, "count": 2}).to_string(),
        inbound(6, 20, 1, written_inbound[2].1.clone()),
        // A record kind this reader does not know is skipped.
        json!({"kind": "mystery", "seq": 7, "tMs": 21, "note": "ignored"}).to_string(),
        inbound(8, 30, 2, written_inbound[3].1.clone()),
        outbound(
            9,
            31,
            2,
            json!({"jsonrpc": "2.0", "id": srq, "result": "[redacted]"}),
        ),
        inbound(10, 40, 2, written_inbound[4].1.clone()),
    ];
    let (_dir, path) = write_log(&lines, true);

    let source = ReplaySource::open(&path).expect("open a good version 3 log");
    assert_eq!(source.header().version, 3);
    assert!(!source.truncated());

    let seqs: Vec<u64> = source.inbound().map(|frame| frame.seq).collect();
    assert_eq!(
        seqs,
        [1, 3, 6, 8, 10],
        "inbound() keeps seq order and skips null frames"
    );
    let out_seqs: Vec<u64> = source.outbound().map(|frame| frame.seq).collect();
    assert_eq!(out_seqs, [2, 9]);
    let first = source.inbound().next().expect("a first inbound frame");
    assert_eq!((first.t_ms, first.generation), (0, 1));
    let fourth = source.inbound().nth(3).expect("the server request frame");
    assert_eq!((fourth.t_ms, fourth.generation), (30, 2));

    let (sent, items) = collect(&source).await;
    assert_eq!(
        sent, 5,
        "run reports the frames it sent; null frames are not counted"
    );
    assert_eq!(items.len(), 5);

    // Same frames, same order, same generation.
    let generations: Vec<u64> = items.iter().map(|item| item.generation).collect();
    assert_eq!(generations, [1, 1, 1, 2, 2]);
    for (item, (_, expected)) in items.iter().zip(&written_inbound) {
        let parsed: Value = serde_json::from_str(&item.text).expect("item text is JSON");
        assert_eq!(&parsed, expected);
    }

    // Decode through the shared function, as the live pipeline does.
    let swallowed = Arc::new(AtomicU64::new(0));
    let mut decoded_words = Vec::new();
    for item in &items {
        let counter = Arc::clone(&swallowed);
        let decoded = decode_frame(
            Generation(item.generation),
            item.text.as_bytes(),
            &mut |_id| Responder::detached(Arc::clone(&counter)),
        );
        decoded_words.push(summarize(&decoded));
    }
    assert_eq!(
        decoded_words,
        [
            "event:message.start".to_owned(),
            "reply:3".to_owned(),
            "event:message.delta".to_owned(),
            format!("request:{srq}"),
            "event:message.complete".to_owned(),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[test]
fn other_versions_rejected_with_found_version() {
    for version in [1_u64, 2, 4, 99] {
        let lines = vec![
            header_with(version),
            inbound(1, 0, 1, event("message.start", 1, json!({}))),
        ];
        let (_dir, path) = write_log(&lines, true);
        let error = ReplaySource::open(&path).expect_err("only version 3 is readable");
        assert_eq!(
            error,
            ReplayError::UnsupportedVersion { found: version },
            "version {version}"
        );
    }
}

#[test]
fn not_a_header_and_corrupt_line_errors() {
    // The first line is a record, not a header.
    let (_dir, path) = write_log(
        &[inbound(1, 0, 1, event("message.start", 1, json!({})))],
        true,
    );
    assert_eq!(
        ReplaySource::open(&path).expect_err("a record first"),
        ReplayError::NotAHeader
    );

    // The first line is JSON of another shape.
    let (_dir, path) = write_log(&["[1,2,3]".to_owned()], true);
    assert_eq!(
        ReplaySource::open(&path).expect_err("an array first"),
        ReplayError::NotAHeader
    );

    // A complete line (it ends in a newline) that is not JSON, on line 3 of the file.
    let lines = vec![
        header(),
        inbound(1, 0, 1, event("message.start", 1, json!({}))),
        "{this is not json".to_owned(),
        inbound(
            3,
            9,
            1,
            event("message.complete", 2, json!({"status": "complete"})),
        ),
    ];
    let (_dir, path) = write_log(&lines, true);
    assert_eq!(
        ReplaySource::open(&path).expect_err("a corrupt complete line"),
        ReplayError::Corrupt { line: 3 }
    );
}

#[test]
fn cut_short_last_line_flagged() {
    let complete = vec![
        header(),
        inbound(1, 0, 1, event("message.start", 1, json!({}))),
        inbound(2, 5, 1, event("message.delta", 2, json!({"text": "a"}))),
    ];
    let (_dir, path) = write_log(&complete, true);
    let whole = ReplaySource::open(&path).expect("a complete log");
    assert!(!whole.truncated());
    assert_eq!(whole.inbound().count(), 2);

    // Same log, then a third record cut off mid-line with no newline: the recording stopped there.
    let mut cut = complete.clone();
    cut.push(r#"{"kind":"frame","seq":3,"tMs":9,"generat"#.to_owned());
    let (_dir, path) = write_log(&cut, false);
    let source = ReplaySource::open(&path).expect("a cut-short last line is not an error");
    assert!(source.truncated());
    assert_eq!(
        source.inbound().count(),
        2,
        "the cut-short line is ignored, the rest is kept"
    );
}

#[test]
fn seq_gap_is_an_error() {
    // A hole in the middle.
    let lines = vec![
        header(),
        inbound(1, 0, 1, event("message.start", 1, json!({}))),
        inbound(2, 5, 1, event("message.delta", 2, json!({"text": "a"}))),
        inbound(
            4,
            9,
            1,
            event("message.complete", 3, json!({"status": "complete"})),
        ),
    ];
    let (_dir, path) = write_log(&lines, true);
    assert!(
        matches!(ReplaySource::open(&path), Err(ReplayError::SeqGap { .. })),
        "a missing seq 3 is a gap"
    );

    // The numbering starts at 1.
    let lines = vec![
        header(),
        inbound(2, 0, 1, event("message.start", 1, json!({}))),
    ];
    let (_dir, path) = write_log(&lines, true);
    assert!(
        matches!(ReplaySource::open(&path), Err(ReplayError::SeqGap { .. })),
        "a log that starts at seq 2 has a gap before it"
    );
}

// ---------------------------------------------------------------------------------------------
// Header-only and the header's optional fields
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn header_only_yields_nothing() {
    let (_dir, path) = write_log(&[header()], true);
    let source = ReplaySource::open(&path).expect("a header-only log is valid");
    let head = source.header();
    assert_eq!(head.version, 3);
    assert_eq!(head.started_at, "2026-10-23T09:14:02.331Z");
    assert_eq!(head.endpoint, "ws://<gateway>/api/ws");
    assert_eq!(head.subprotocols, ["hermes-gateway-v1"]);
    assert_eq!(head.gateway, None);
    assert_eq!(head.scenario, None);
    assert!(head.expected_statuses.is_empty());
    assert_eq!(source.inbound().count(), 0);
    assert_eq!(source.outbound().count(), 0);
    assert!(!source.truncated());

    let (sent, items) = collect(&source).await;
    assert_eq!(sent, 0);
    assert!(items.is_empty());

    // The optional fields, when the header carries them.
    let full = json!({
        "kind": "header",
        "version": 3,
        "startedAt": "2026-10-23T09:14:02.331Z",
        "endpoint": "ws://<gateway>/api/ws",
        "subprotocols": ["hermes-gateway-v1"],
        "gateway": {"codeVersion": "0.22.4", "codeSha": "4d04303"},
        "scenario": "approval",
        "expectedStatuses": ["complete"],
        "someFutureField": {"ignored": true},
    })
    .to_string();
    let (_dir, path) = write_log(&[full], true);
    let source = ReplaySource::open(&path).expect("a header with every optional field");
    let head = source.header();
    let gateway = head.gateway.as_ref().expect("the gateway identity");
    assert_eq!(gateway.code_version, "0.22.4");
    assert_eq!(gateway.code_sha, "4d04303");
    assert_eq!(head.scenario.as_deref(), Some("approval"));
    assert_eq!(head.expected_statuses, ["complete"]);
}

// ---------------------------------------------------------------------------------------------
// Speeds
// ---------------------------------------------------------------------------------------------

fn paced_log() -> (TempDir, PathBuf) {
    let lines = vec![
        header(),
        inbound(1, 0, 1, event("message.start", 1, json!({}))),
        outbound(
            2,
            200,
            1,
            json!({"jsonrpc": "2.0", "id": 1, "method": "prompt.submit", "params": {}}),
        ),
        inbound(3, 400, 1, event("message.delta", 2, json!({"text": "a"}))),
        inbound(4, 1000, 1, event("message.delta", 3, json!({"text": "b"}))),
        inbound(
            5,
            4000,
            1,
            event("message.complete", 4, json!({"status": "complete"})),
        ),
    ];
    write_log(&lines, true)
}

/// Replays at `speed` and returns each item's arrival time on the virtual clock.
async fn arrival_times(source: &ReplaySource, speed: Speed) -> Vec<Duration> {
    let start = tokio::time::Instant::now();
    let (sink, mut receiver) = mpsc::channel(4);
    let consumer = async {
        let mut arrivals = Vec::new();
        while let Some(_item) = receiver.recv().await {
            arrivals.push(start.elapsed());
        }
        arrivals
    };
    let (sent, arrivals) = tokio::join!(source.run(speed, sink), consumer);
    assert_eq!(sent.expect("a replay at a good speed"), 4);
    arrivals
}

fn assert_close(actual: Duration, expected_ms: u64, what: &str) {
    let expected = Duration::from_millis(expected_ms);
    let difference = actual.abs_diff(expected);
    assert!(
        difference <= Duration::from_millis(1),
        "{what}: expected about {expected_ms} ms on the virtual clock, got {} ms",
        actual.as_millis()
    );
}

#[tokio::test]
async fn speeds_on_paused_time() {
    tokio::time::pause();
    let (_dir, path) = paced_log();
    let source = ReplaySource::open(&path).expect("a paced log");

    // Instant never sleeps: no virtual time passes.
    let started = tokio::time::Instant::now();
    let arrivals = arrival_times(&source, Speed::Instant).await;
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "instant replay advanced the virtual clock"
    );
    assert!(arrivals.iter().all(|at| *at == Duration::ZERO));

    // Recorded waits until each frame's tMs, so the clock ends at the last inbound frame's tMs.
    let started = tokio::time::Instant::now();
    let arrivals = arrival_times(&source, Speed::Recorded).await;
    for (at, expected) in arrivals.iter().zip([0, 400, 1000, 4000]) {
        assert_close(*at, expected, "recorded arrival");
    }
    assert_close(started.elapsed(), 4000, "recorded total");

    // Ten times faster takes a tenth of the time.
    let started = tokio::time::Instant::now();
    let arrivals = arrival_times(&source, Speed::Factor(10.0)).await;
    for (at, expected) in arrivals.iter().zip([0, 40, 100, 400]) {
        assert_close(*at, expected, "factor 10 arrival");
    }
    assert_close(started.elapsed(), 400, "factor 10 total");
}

#[tokio::test]
async fn bad_speed_is_an_error() {
    assert_eq!(Speed::parse("instant"), Ok(Speed::Instant));
    assert_eq!(Speed::parse("recorded"), Ok(Speed::Recorded));
    assert_eq!(Speed::parse("10"), Ok(Speed::Factor(10.0)));
    assert_eq!(Speed::parse("0.5"), Ok(Speed::Factor(0.5)));
    assert_eq!(Speed::parse("2.5"), Ok(Speed::Factor(2.5)));
    for text in ["0", "0.0", "-1", "-0.5", "fast", "", "nan", "1x"] {
        assert_eq!(
            Speed::parse(text),
            Err(ReplayError::BadSpeed),
            "speed {text:?} must be refused"
        );
    }

    // run checks the speed before it sends anything.
    let (_dir, path) = paced_log();
    let source = ReplaySource::open(&path).expect("a paced log");
    for speed in [
        Speed::Factor(0.0),
        Speed::Factor(-1.0),
        Speed::Factor(f64::NAN),
    ] {
        let (sink, mut receiver) = mpsc::channel(8);
        let result = source.run(speed, sink).await;
        assert_eq!(result, Err(ReplayError::BadSpeed));
        assert!(
            receiver.try_recv().is_err(),
            "a refused speed must not send a frame"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Pipeline behaviours that need the shared decode function
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn replayed_responder_swallows_and_counts() {
    let first = "srq-aaaaaaaaaaaa";
    let second = "srq-bbbbbbbbbbbb";
    let lines = vec![
        header(),
        inbound(
            1,
            0,
            1,
            server_request(first, "sudo", json!({"session_id": "s1"})),
        ),
        outbound(
            2,
            3,
            1,
            json!({"jsonrpc": "2.0", "id": first, "result": "[redacted]"}),
        ),
        inbound(
            3,
            6,
            1,
            server_request(second, "vault.code", json!({"session_id": "s1"})),
        ),
        inbound(4, 9, 1, event("message.start", 1, json!({}))),
    ];
    let (_dir, path) = write_log(&lines, true);
    let source = ReplaySource::open(&path).expect("a log with two server requests");
    let (sent, items) = collect(&source).await;
    assert_eq!(sent, 3, "the outbound answer in the file is not replayed");

    let swallowed = Arc::new(AtomicU64::new(0));
    let mut asked_for = Vec::new();
    let mut answered = 0_u64;
    for item in &items {
        let counter = Arc::clone(&swallowed);
        let decoded = decode_frame(
            Generation(item.generation),
            item.text.as_bytes(),
            &mut |id| {
                asked_for.push(id.to_owned());
                Responder::detached(Arc::clone(&counter))
            },
        );
        if let Decoded::Inbound(Inbound::ServerRequest(request)) = decoded {
            // The consumer answers; nothing reaches a socket and the answer is counted.
            if request.id == first {
                request
                    .responder
                    .respond(&json!({"value": "answer"}))
                    .await
                    .expect("a detached responder accepts an answer");
            } else {
                request
                    .responder
                    .fail(-32603, "skipped in replay")
                    .await
                    .expect("a detached responder accepts an error");
            }
            answered += 1;
        }
    }
    assert_eq!(asked_for, [first, second]);
    assert_eq!(answered, 2);
    assert_eq!(
        swallowed.load(Ordering::SeqCst),
        2,
        "each swallowed answer is counted once"
    );
}

#[tokio::test]
async fn redacted_marker_in_typed_field_degrades() {
    // Redaction leaves the marker "[redacted]" where a typed field expects a number, an object
    // or an array. The decoder must degrade, never panic.
    let marker = "[redacted]";
    let frames = [
        // `count` is an integer in the contract.
        event(
            "pet.generate.progress",
            1,
            json!({"count": marker, "token": marker}),
        ),
        // `billing` is an object.
        event(
            "message.complete",
            2,
            json!({"status": "complete", "billing": marker}),
        ),
        // `duration_s` is a number.
        event(
            "tool.complete",
            3,
            json!({"name": "shell", "duration_s": marker, "inline_diff": marker}),
        ),
        // A server request whose `questions` should be an array.
        server_request(
            "srq-cccccccccccc",
            "clarify",
            json!({"session_id": "s1", "questions": marker}),
        ),
    ];
    let mut lines = vec![header()];
    for (index, frame) in frames.iter().enumerate() {
        let seq = (index + 1) as u64;
        lines.push(inbound(seq, seq * 10, 1, frame.clone()));
    }
    let (_dir, path) = write_log(&lines, true);
    let source = ReplaySource::open(&path).expect("a log with markers in typed fields");
    let (sent, items) = collect(&source).await;
    assert_eq!(sent, frames.len());

    let swallowed = Arc::new(AtomicU64::new(0));
    for (index, item) in items.iter().enumerate() {
        let counter = Arc::clone(&swallowed);
        let decoded = decode_frame(
            Generation(item.generation),
            item.text.as_bytes(),
            &mut |_id| Responder::detached(Arc::clone(&counter)),
        );
        match decoded {
            Decoded::Inbound(Inbound::Event(event)) => assert!(
                matches!(event.frame.notification, Notification::Unknown(_)),
                "frame {index}: a marker in a typed field must give the Unknown variant"
            ),
            Decoded::Inbound(Inbound::ServerRequest(request)) => assert!(
                matches!(request.request, ServerRequest::Unknown(_)),
                "frame {index}: a marker in a typed field must give the Unknown variant"
            ),
            Decoded::Inbound(Inbound::Malformed(frame)) => assert_eq!(
                frame.kind,
                MalformedKind::BadPayload,
                "frame {index}: a counted decode error must say BadPayload"
            ),
            other => panic!("frame {index}: unexpected result {}", summarize(&other)),
        }
    }
}
