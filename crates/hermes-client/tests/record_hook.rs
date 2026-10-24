//! Where the recorder plugs into a connection (test row 7).
//!
//! The hook is `Connection::set_recorder(recorder, generation)`, called per
//! connection before the handshake. These tests run the handshake against the in-process mock
//! gateway of `tests/support/mod.rs`.
//!
//! `handshake_frames_recorded_once_with_generation_1` hands the connection over to the channel
//! `Client`, because the property it proves (a frame is not recorded twice across the hand-over)
//! cannot be seen without it.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hermes_client::{
    Client, Connection, DiscoverOptions, EnsurePolicy, Handshake, Inbound, Purpose, Recorder,
    RecorderOptions, Scope, connect, discover_with, handshake, mint_ticket,
};
use serde_json::{Value, json};
use support::{
    AttachMock, ControlReply, Echo, Home, INSTANCE_ID, WsPlan, WsStep, ensure_fixture,
    frame_capabilities, frame_describe, frame_ready, ready_identify, serve, success_steps,
};

fn text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| panic!("json"))
}

fn never() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

/// A mock gateway for `home`, plus a connection to it that has not done the handshake yet.
async fn opened(home: &Home, plan: WsPlan) -> (AttachMock, Connection) {
    ensure_fixture(home.path());
    let canonical = home.canonical();
    let fixture = format!("{canonical}/profiles/fixture");
    let socket = home.path().join("gateway.sock");
    let mock = serve(
        &socket,
        {
            let canonical = canonical.clone();
            let fixture = fixture.clone();
            move |origin| {
                vec![
                    ControlReply::Line(text(&json!({
                        "ok": true,
                        "protocol": 1,
                        "id": 1,
                        "result": ready_identify(&canonical, origin),
                    }))),
                    ControlReply::Issue {
                        profile_id: fixture,
                        instance_id: INSTANCE_ID.into(),
                        expired: false,
                    },
                ]
            }
        },
        plan,
    )
    .await;
    let endpoint = discover_with(home.path(), &never())
        .await
        .expect("discover");
    let profile = endpoint.resolve_profile("fixture").expect("fixture");
    let ticket = mint_ticket(
        home.path(),
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("ticket");
    let connection = connect(&endpoint, ticket).await.expect("connect");
    (mock, connection)
}

fn talk(steps: Vec<WsStep>) -> WsPlan {
    WsPlan::Talk {
        echo: Echo::Version1,
        steps,
        guard: false,
    }
}

type Sink = Arc<Mutex<Vec<u8>>>;

struct SharedBuf(Sink);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn recorder() -> (Recorder, Sink) {
    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::from_writer(
        SharedBuf(Arc::clone(&sink)),
        RecorderOptions {
            endpoint: "ws://127.0.0.1:9119/api/ws".to_string(),
            subprotocols: vec!["hermes-gateway-v1".to_string()],
            ..RecorderOptions::default()
        },
    )
    .expect("from_writer");
    (recorder, sink)
}

/// The records after the header.
fn records(sink: &Sink) -> Vec<Value> {
    let bytes = sink
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("the log is not UTF-8"));
    let mut lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("bad log line")))
        .collect();
    assert_eq!(lines[0]["kind"], "header", "the first line is the header");
    lines.remove(0);
    lines
}

fn event_type(record: &Value) -> Option<&str> {
    record["frame"]["params"]["type"].as_str()
}

fn method(record: &Value) -> Option<&str> {
    record["frame"]["method"].as_str()
}

#[tokio::test]
async fn connection_records_both_directions_with_generation() {
    let home = Home::new();
    let fixture = format!("{}/profiles/fixture", home.canonical());
    let (_mock, mut connection) = opened(&home, talk(success_steps(&fixture))).await;
    let (recorder, sink) = recorder();
    connection.set_recorder(recorder.clone(), 4);
    let _ = handshake(&mut connection).await.expect("handshake");
    recorder.finish().expect("finish");

    let log = records(&sink);
    // gateway.ready in, capabilities out, its reply in, describe out, its reply in.
    let directions: Vec<&str> = log
        .iter()
        .map(|record| record["dir"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(directions, ["in", "out", "in", "out", "in"]);
    for (index, record) in log.iter().enumerate() {
        assert_eq!(record["kind"], "frame");
        assert_eq!(
            record["seq"].as_u64(),
            Some(index as u64 + 1),
            "gapless seq"
        );
        assert_eq!(
            record["generation"].as_u64(),
            Some(4),
            "every handshake frame carries the generation given to set_recorder"
        );
    }
    assert_eq!(event_type(&log[0]), Some("gateway.ready"));
    assert_eq!(
        method(&log[1]),
        Some(hermes_contract::methods::legacy::client_capabilities::NAME)
    );
    assert_eq!(log[2]["frame"]["id"], 1);
    assert!(log[2]["frame"].get("result").is_some());
    assert_eq!(
        method(&log[3]),
        Some(hermes_contract::methods::canonical::runtime_describe::NAME)
    );
    assert_eq!(log[4]["frame"]["id"], 2);
    assert!(log[4]["frame"].get("result").is_some());
}

#[tokio::test]
async fn handshake_frames_recorded_once_with_generation_1() {
    let early =
        r#"{"jsonrpc":"2.0","method":"event","params":{"type":"sessions.changed","payload":{}}}"#;
    let late =
        r#"{"jsonrpc":"2.0","method":"event","params":{"type":"platforms.changed","payload":{}}}"#;
    let home = Home::new();
    let fixture = format!("{}/profiles/fixture", home.canonical());
    // `early` arrives before `gateway.ready`: the handshake reads it, does not use it, and keeps it
    // in the hand-over buffer. `late` arrives after the handshake and is read by the client.
    let steps = vec![
        WsStep::Send(early.to_string()),
        WsStep::Send(frame_ready()),
        WsStep::Take,
        WsStep::Send(frame_capabilities()),
        WsStep::Take,
        WsStep::Send(frame_describe(&fixture)),
        WsStep::Send(late.to_string()),
    ];
    let (_mock, mut connection) = opened(&home, talk(steps)).await;
    let (recorder, sink) = recorder();
    connection.set_recorder(recorder.clone(), 1);
    let _ = handshake(&mut connection).await.expect("handshake");
    let parts = connection.into_parts();
    assert_eq!(
        parts.buffered.len(),
        1,
        "the early event waits in the buffer"
    );

    // The channel client delivers the buffered frame first, then reads from the socket.
    let (client, mut inbound) = Client::start(parts);
    let mut seen_early = false;
    let mut seen_late = false;
    let read = tokio::time::timeout(Duration::from_secs(10), async {
        while !(seen_early && seen_late) {
            match inbound.next().await {
                Some(Inbound::Event(item)) => match item.raw["type"].as_str() {
                    Some("sessions.changed") => seen_early = true,
                    Some("platforms.changed") => seen_late = true,
                    _ => {}
                },
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    assert!(read.is_ok(), "the client never delivered both events");
    assert!(
        seen_early && seen_late,
        "both events must reach the consumer"
    );
    drop(client);
    recorder.finish().expect("finish");

    let log = records(&sink);
    let count = |wanted: &str| {
        log.iter()
            .filter(|record| event_type(record) == Some(wanted))
            .count()
    };
    assert_eq!(
        count("gateway.ready"),
        1,
        "gateway.ready is recorded exactly once"
    );
    assert_eq!(
        count("sessions.changed"),
        1,
        "a frame replayed from the hand-over buffer is not recorded a second time"
    );
    assert_eq!(
        count("platforms.changed"),
        1,
        "a frame read after hand-over is recorded once"
    );
    // early, ready, capabilities out, its reply, describe out, its reply, late.
    assert_eq!(
        log.len(),
        7,
        "every text frame crossed the socket once and was recorded once"
    );
    for record in &log {
        assert_eq!(
            record["generation"].as_u64(),
            Some(1),
            "the first connection is generation 1"
        );
    }
    let ready = log
        .iter()
        .find(|record| event_type(record) == Some("gateway.ready"))
        .expect("ready record");
    assert_eq!(ready["dir"], "in");
}

/// What a handshake leaves behind that a recorder must not change.
struct Outcome {
    done: Handshake,
    seen_by_gateway: Vec<String>,
    next_request_id: u64,
    buffered: Vec<String>,
    instance_id: String,
}

async fn run(recorder: Option<Recorder>) -> Outcome {
    let early =
        r#"{"jsonrpc":"2.0","method":"event","params":{"type":"sessions.changed","payload":{}}}"#;
    let home = Home::new();
    let fixture = format!("{}/profiles/fixture", home.canonical());
    let mut steps = vec![WsStep::Send(early.to_string())];
    steps.extend(success_steps(&fixture));
    let (mock, mut connection) = opened(&home, talk(steps)).await;
    if let Some(recorder) = recorder {
        connection.set_recorder(recorder, 1);
    }
    let done = handshake(&mut connection).await.expect("handshake");
    let parts = connection.into_parts();
    Outcome {
        done,
        seen_by_gateway: mock.frames(),
        next_request_id: parts.next_request_id,
        buffered: parts.buffered.clone(),
        instance_id: parts.instance_id.clone(),
    }
}

#[tokio::test]
async fn none_recorder_changes_nothing() {
    let without = run(None).await;
    let (recorder, sink) = recorder();
    let with = run(Some(recorder.clone())).await;
    recorder.finish().expect("finish");

    assert!(
        without.done.ready == with.done.ready,
        "the ready payload differs"
    );
    assert!(
        without.done.server_requests == with.done.server_requests,
        "the capabilities result differs"
    );
    assert_eq!(
        without.done.describe.instance_id,
        with.done.describe.instance_id
    );
    assert_eq!(without.instance_id, with.instance_id);
    assert_eq!(
        without.seen_by_gateway, with.seen_by_gateway,
        "the frames on the wire must be identical with and without a recorder"
    );
    assert_eq!(without.next_request_id, with.next_request_id);
    assert_eq!(
        without.buffered, with.buffered,
        "the hand-over buffer differs"
    );
    assert!(
        !records(&sink).is_empty(),
        "the recorder in the second run saw the handshake"
    );
}
