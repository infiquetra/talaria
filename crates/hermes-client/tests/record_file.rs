//! Frame log file format, queueing, and failure behaviour of the recorder (P1-04, test row 6).
//!
//! Spec sections 3 and 5. Nothing here asserts a wall-clock time. The gated writers below wait on
//! channels, so the tests decide the order of events, not a timer.

use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hermes_client::{
    GatewayIdentity, REDACTED, RecordError, RecordSummary, Recorder, RecorderOptions,
};
use serde_json::{Value, json};

/// A random value that stands in for a secret. Hex, so it never looks like a real key.
fn sentinel() -> String {
    format!("zq{:032x}", rand::random::<u128>())
}

fn options() -> RecorderOptions {
    RecorderOptions {
        endpoint: "ws://127.0.0.1:9119/api/ws".to_string(),
        subprotocols: vec!["hermes-gateway-v1".to_string()],
        ..RecorderOptions::default()
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn parse_lines(bytes: &[u8]) -> Vec<Value> {
    let text = std::str::from_utf8(bytes).unwrap_or_else(|_| panic!("the log is not UTF-8"));
    assert!(
        text.is_empty() || text.ends_with('\n'),
        "the last line has no newline"
    );
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|_| panic!("line {} is not valid JSON", index + 1))
        })
        .collect()
}

fn read_file(path: &Path) -> (Vec<u8>, Vec<Value>) {
    let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("the log file cannot be read"));
    let lines = parse_lines(&bytes);
    (bytes, lines)
}

fn event(kind: &str, n: usize) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": kind, "session_id": "s1", "seq": n, "payload": {"n": n}}
    })
    .to_string()
}

fn request(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("record has no field {key}"))
}

fn redaction_set(record: &Value) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = record
        .get("redactions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    (
                        field(item, "path").as_str().unwrap_or("").to_string(),
                        field(item, "reason").as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn assert_gapless(records: &[Value]) {
    for (index, record) in records.iter().enumerate() {
        assert_eq!(
            field(record, "seq").as_u64(),
            Some(index as u64 + 1),
            "seq is not gapless at record {}",
            index + 1
        );
    }
}

// ---- in-memory writers -------------------------------------------------------------------------

type Sink = Arc<Mutex<Vec<u8>>>;

fn sink_bytes(sink: &Sink) -> Vec<u8> {
    sink.lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

fn append(sink: &Sink, bytes: &[u8]) {
    sink.lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .extend_from_slice(bytes);
}

/// A writer that lets the header through and then blocks every write until it is released.
struct GatedWriter {
    sink: Sink,
    armed: Arc<AtomicBool>,
    release: Receiver<()>,
    blocked: Sender<()>,
}

impl Write for GatedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.armed.load(Ordering::SeqCst) {
            let _ = self.blocked.send(());
            // Returns an error as soon as the sender is dropped: that is the release.
            let _ = self.release.recv();
        }
        append(&self.sink, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Gate {
    sink: Sink,
    armed: Arc<AtomicBool>,
    release: Option<Sender<()>>,
    blocked: Receiver<()>,
}

impl Gate {
    /// From now on the writer blocks. Call it after `from_writer` returned: the header is out.
    fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Waits until the writer is stuck inside a write. A hang guard, not a timing assertion.
    fn wait_blocked(&self) {
        self.blocked
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("the writer never reached the gated write"));
    }

    fn release(&mut self) {
        self.release.take();
    }

    fn bytes(&self) -> Vec<u8> {
        sink_bytes(&self.sink)
    }
}

fn gated() -> (GatedWriter, Gate) {
    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let armed = Arc::new(AtomicBool::new(false));
    let (release_tx, release_rx) = mpsc::channel();
    let (blocked_tx, blocked_rx) = mpsc::channel();
    (
        GatedWriter {
            sink: Arc::clone(&sink),
            armed: Arc::clone(&armed),
            release: release_rx,
            blocked: blocked_tx,
        },
        Gate {
            sink,
            armed,
            release: Some(release_tx),
            blocked: blocked_rx,
        },
    )
}

/// A writer that works until armed and then fails every write and flush.
struct FailingWriter {
    sink: Sink,
    armed: Arc<AtomicBool>,
    failed: Sender<()>,
}

impl Write for FailingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.armed.load(Ordering::SeqCst) {
            let _ = self.failed.send(());
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"));
        }
        append(&self.sink, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.armed.load(Ordering::SeqCst) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"));
        }
        Ok(())
    }
}

// ---- header and records ------------------------------------------------------------------------

#[test]
fn header_then_gapless_seq_and_monotonic_tms() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(
        &path,
        RecorderOptions {
            gateway: Some(GatewayIdentity {
                code_version: "0.22.4".to_string(),
                code_sha: "0123abc".to_string(),
            }),
            scenario: Some("approval".to_string()),
            expected_statuses: vec!["complete".to_string(), "interrupted".to_string()],
            ..options()
        },
    )
    .expect("create");

    // The header is on disk before create returns (spec 5.1).
    let (_, early) = read_file(&path);
    assert_eq!(
        early.len(),
        1,
        "the header must be written before create returns"
    );
    assert_eq!(field(&early[0], "kind"), "header");

    let texts: Vec<String> = (0..10).map(|n| event("message.delta", n)).collect();
    for (n, text) in texts.iter().enumerate() {
        if n % 2 == 0 {
            recorder.record_inbound(1, text);
        } else {
            recorder.record_outbound(1, text);
        }
    }
    let summary = recorder.finish().expect("finish");
    assert_eq!(
        summary,
        RecordSummary {
            frames: 10,
            redactions: 0,
            dropped: 0
        }
    );

    let (bytes, lines) = read_file(&path);
    assert_eq!(lines.len(), 11, "one header and ten frames");
    let header = &lines[0];
    assert_eq!(field(header, "kind"), "header");
    assert_eq!(field(header, "version"), 3);
    let started = field(header, "startedAt").as_str().unwrap_or("");
    let shape = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$").expect("regex");
    assert!(
        shape.is_match(started),
        "startedAt is not ISO 8601 UTC with milliseconds"
    );
    assert_eq!(field(header, "endpoint"), "ws://127.0.0.1:9119/api/ws");
    assert_eq!(field(header, "subprotocols"), &json!(["hermes-gateway-v1"]));
    assert_eq!(
        field(header, "gateway"),
        &json!({"codeVersion": "0.22.4", "codeSha": "0123abc"})
    );
    assert_eq!(field(header, "scenario"), "approval");
    assert_eq!(
        field(header, "expectedStatuses"),
        &json!(["complete", "interrupted"])
    );
    assert!(bytes.ends_with(b"\n"), "the file must end with a newline");

    let records = &lines[1..];
    assert_gapless(records);
    let mut last_t = 0;
    for (n, record) in records.iter().enumerate() {
        assert_eq!(field(record, "kind"), "frame");
        let t = field(record, "tMs")
            .as_u64()
            .unwrap_or_else(|| panic!("tMs is not a non-negative integer"));
        assert!(t >= last_t, "tMs went backwards at record {}", n + 1);
        last_t = t;
        let expected: Value = serde_json::from_str(&texts[n]).expect("sample json");
        assert_eq!(field(record, "frame"), &expected, "frame {} changed", n + 1);
        assert!(
            record.get("redactions").is_none(),
            "a clean frame carries no redactions field"
        );
        assert!(
            record.get("parseError").is_none(),
            "a parsed frame has no parseError"
        );
    }
}

#[test]
fn header_optional_fields_absent_when_unset() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    let summary = recorder.finish().expect("finish");
    assert_eq!(
        summary,
        RecordSummary {
            frames: 0,
            redactions: 0,
            dropped: 0
        }
    );
    let (_, lines) = read_file(&path);
    assert_eq!(lines.len(), 1, "a header-only file");
    let header = &lines[0];
    assert!(
        header.get("gateway").is_none_or(Value::is_null),
        "gateway is optional"
    );
    assert!(
        header.get("scenario").is_none_or(Value::is_null),
        "scenario is optional"
    );
    let statuses = header.get("expectedStatuses");
    assert!(
        statuses.is_none_or(|value| value.is_null() || value == &json!([])),
        "expectedStatuses is optional"
    );
}

#[test]
fn generation_and_direction_recorded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    // The recorder copies the generation; it does not own the counter, so it can go backwards.
    let plan: [(u64, &str); 5] = [(1, "in"), (1, "out"), (2, "in"), (7, "out"), (1, "in")];
    for (n, (generation, dir)) in plan.iter().enumerate() {
        let text = event("message.delta", n);
        if *dir == "in" {
            recorder.record_inbound(*generation, &text);
        } else {
            recorder.record_outbound(*generation, &text);
        }
    }
    recorder.finish().expect("finish");
    let (_, lines) = read_file(&path);
    assert_eq!(lines.len(), 6);
    for (n, (generation, dir)) in plan.iter().enumerate() {
        let record = &lines[n + 1];
        assert_eq!(
            field(record, "generation").as_u64(),
            Some(*generation),
            "record {}",
            n + 1
        );
        assert_eq!(field(record, "dir"), *dir, "record {}", n + 1);
    }
}

// ---- create semantics --------------------------------------------------------------------------

#[test]
fn refuses_overwrite_and_file_is_private() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let first = Recorder::create(&path, options()).expect("create");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the recording must be private to its owner");
    }

    let before = std::fs::read(&path).expect("read");
    // A second recorder on the same path, while the first is still open.
    let again = Recorder::create(&path, options());
    assert!(
        matches!(again, Err(RecordError::AlreadyExists)),
        "a second create on a recording must be AlreadyExists"
    );
    first.finish().expect("finish");
    assert_eq!(
        std::fs::read(&path).expect("read"),
        before,
        "the refused create must not touch the file"
    );

    // A file that is not a recording is refused the same way and keeps its content.
    let other = dir.path().join("notes.txt");
    std::fs::write(&other, b"keep me").expect("write");
    let refused = Recorder::create(&other, options());
    assert!(matches!(refused, Err(RecordError::AlreadyExists)));
    assert_eq!(std::fs::read(&other).expect("read"), b"keep me");
}

#[test]
fn header_drops_ticket_subprotocols() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let secret = sentinel();
    let recorder = Recorder::create(
        &path,
        RecorderOptions {
            subprotocols: vec![
                "hermes-gateway-v1".to_string(),
                format!("hermes-gateway-ticket.{secret}"),
                "hermes-gateway-v2".to_string(),
            ],
            ..options()
        },
    )
    .expect("create");
    recorder.finish().expect("finish");
    let (bytes, lines) = read_file(&path);
    assert_eq!(
        field(&lines[0], "subprotocols"),
        &json!(["hermes-gateway-v1", "hermes-gateway-v2"])
    );
    assert!(!contains(&bytes, &secret), "the ticket reached the file");
    assert!(
        !contains(&bytes, "hermes-gateway-ticket"),
        "a ticket subprotocol reached the file"
    );
}

#[test]
fn header_endpoint_goes_through_redact_url() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let user = sentinel();
    let password = sentinel();
    let ticket = sentinel();
    let recorder = Recorder::create(
        &path,
        RecorderOptions {
            endpoint: format!(
                "wss://{user}:{password}@host:9119/api/ws?Ticket={ticket}&mode=1#frag"
            ),
            ..options()
        },
    )
    .expect("create");
    recorder.finish().expect("finish");
    let (bytes, lines) = read_file(&path);
    assert_eq!(
        field(&lines[0], "endpoint"),
        "wss://%5Bredacted%5D@host:9119/api/ws?Ticket=%5Bredacted%5D&mode=1#%5Bredacted%5D"
    );
    for value in [&user, &password, &ticket] {
        assert!(
            !contains(&bytes, value),
            "an endpoint credential reached the file"
        );
    }
}

#[test]
fn withheld_endpoint_is_marker_without_redactions_entry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(
        &path,
        RecorderOptions {
            endpoint: "not a url".to_string(),
            ..options()
        },
    )
    .expect("create");
    recorder.finish().expect("finish");
    let (_, lines) = read_file(&path);
    assert_eq!(field(&lines[0], "endpoint"), REDACTED);
    assert!(
        lines[0].get("redactions").is_none(),
        "the header has no redactions list; the marker stands alone"
    );
}

#[test]
fn header_endpoint_gets_substitutions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(
        &path,
        RecorderOptions {
            endpoint: "ws://gateway-host.test:9119/api/ws".to_string(),
            substitutions: vec![(
                "gateway-host.test:9119".to_string(),
                "<gateway>".to_string(),
            )],
            ..options()
        },
    )
    .expect("create");
    recorder.finish().expect("finish");
    let (_, lines) = read_file(&path);
    assert_eq!(field(&lines[0], "endpoint"), "ws://<gateway>/api/ws");
}

// ---- redaction paths and reasons at file level ---------------------------------------------------

#[test]
fn redaction_paths_use_dots_indices_and_escapes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    let dotted = sentinel();
    let slashed = sentinel();
    let indexed = sentinel();
    // `a.b_password` holds a dot and `c\d_password` holds a backslash: both are escaped in a path.
    let text = request(
        3,
        "session.list",
        json!({
            "a.b_password": dotted,
            "c\\d_password": slashed,
            "items": [{"keep": "ok"}, {"password": indexed}],
        }),
    );
    recorder.record_outbound(1, &text);
    let summary = recorder.finish().expect("finish");
    assert_eq!(summary.redactions, 3);
    let (bytes, lines) = read_file(&path);
    for value in [&dotted, &slashed, &indexed] {
        assert!(
            !contains(&bytes, value),
            "a withheld value reached the file"
        );
    }
    let record = &lines[1];
    assert_eq!(
        redaction_set(record),
        vec![
            (
                "params.a\\.b_password".to_string(),
                "suspicious-key".to_string()
            ),
            (
                "params.c\\\\d_password".to_string(),
                "suspicious-key".to_string()
            ),
            (
                "params.items.1.password".to_string(),
                "suspicious-key".to_string()
            ),
        ]
    );
    let params = field(field(record, "frame"), "params");
    assert_eq!(field(params, "a.b_password"), REDACTED);
    assert_eq!(field(params, "c\\d_password"), REDACTED);
    assert_eq!(field(&field(params, "items")[1], "password"), REDACTED);
    assert_eq!(field(&field(params, "items")[0], "keep"), "ok");
}

#[test]
fn ticket_string_in_a_frame_is_withheld() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    let secret = sentinel();
    let text = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "error",
            "note": format!("upgrade failed with hermes-gateway-ticket.{secret} offered"),
            "list": ["fine", format!("hermes-gateway-ticket.{secret}")],
        }
    })
    .to_string();
    recorder.record_inbound(1, &text);
    let summary = recorder.finish().expect("finish");
    assert_eq!(summary.redactions, 2);
    let (bytes, lines) = read_file(&path);
    assert!(!contains(&bytes, &secret), "the ticket reached the file");
    assert!(
        !contains(&bytes, "hermes-gateway-ticket"),
        "a ticket subprotocol reached the file"
    );
    let record = &lines[1];
    assert_eq!(
        redaction_set(record),
        vec![
            ("params.list.1".to_string(), "ticket".to_string()),
            ("params.note".to_string(), "ticket".to_string()),
        ]
    );
    let params = field(field(record, "frame"), "params");
    assert_eq!(field(params, "note"), REDACTED);
    assert_eq!(field(params, "list")[0], "fine");
    assert_eq!(field(params, "list")[1], REDACTED);
}

#[test]
fn unparseable_text_has_null_frame_and_no_payload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    let secret = sentinel();
    let bad = [
        format!("{{\"password\": {secret}"),
        format!("not json at all {secret}"),
        format!("{{\"a\": \"{secret}\", }}"),
    ];
    recorder.record_inbound(1, &event("message.delta", 0));
    for text in &bad {
        recorder.record_inbound(1, text);
    }
    recorder.record_outbound(1, &event("message.delta", 1));
    let summary = recorder.finish().expect("finish");
    assert_eq!(
        summary.frames, 5,
        "null-frame records count as frames written"
    );
    let (bytes, lines) = read_file(&path);
    assert!(
        !contains(&bytes, &secret),
        "unparseable text reached the file"
    );
    let records = &lines[1..];
    assert_gapless(records);
    for record in &records[1..4] {
        assert_eq!(field(record, "kind"), "frame");
        assert_eq!(field(record, "frame"), &Value::Null);
        let message = field(record, "parseError").as_str().unwrap_or("");
        assert!(!message.is_empty(), "parseError must say what went wrong");
    }
    assert!(records[0].get("parseError").is_none());
    assert_ne!(field(&records[4], "frame"), &Value::Null);
}

// ---- substitutions -----------------------------------------------------------------------------

#[test]
fn substitutions_longest_first() {
    let long = ("alpha-dir/project".to_string(), "<long>".to_string());
    let short = ("alpha-dir".to_string(), "<short>".to_string());
    // The result must not depend on the order the pairs were given in.
    for pairs in [
        vec![short.clone(), long.clone()],
        vec![long.clone(), short.clone()],
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("log.jsonl");
        let recorder = Recorder::create(
            &path,
            RecorderOptions {
                substitutions: pairs,
                ..options()
            },
        )
        .expect("create");
        let text = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "type": "tool.complete",
                "cwd": "alpha-dir/project/src",
                "other": "alpha-dir only",
                "list": ["x alpha-dir/project", "alpha-dir"],
                "alpha-dir/project": "keyed",
            }
        })
        .to_string();
        recorder.record_inbound(1, &text);
        recorder.finish().expect("finish");
        let (bytes, lines) = read_file(&path);
        assert!(!contains(&bytes, "alpha-dir"), "a needle survived");
        let params = field(field(&lines[1], "frame"), "params");
        assert_eq!(field(params, "cwd"), "<long>/src");
        assert_eq!(field(params, "other"), "<short> only");
        assert_eq!(field(params, "list"), &json!(["x <long>", "<short>"]));
        assert_eq!(
            field(params, "<long>"),
            "keyed",
            "object keys are substituted too"
        );
    }
}

#[test]
fn short_needle_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    // Three characters, by count: the umlauts take six bytes.
    for needle in ["abc", "\u{e4}\u{f6}\u{fc}", "x"] {
        let refused = Recorder::create(
            &path,
            RecorderOptions {
                substitutions: vec![
                    ("long-enough".to_string(), "<ok>".to_string()),
                    (needle.to_string(), "<no>".to_string()),
                ],
                ..options()
            },
        );
        assert!(
            matches!(refused, Err(RecordError::NeedleTooShort)),
            "a needle under four characters must be refused"
        );
        assert!(
            !path.exists(),
            "no file may be created for a refused needle"
        );
    }

    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let refused = Recorder::from_writer(
        SharedBuf(Arc::clone(&sink)),
        RecorderOptions {
            substitutions: vec![("abc".to_string(), "<no>".to_string())],
            ..options()
        },
    );
    assert!(matches!(refused, Err(RecordError::NeedleTooShort)));
    assert!(
        sink_bytes(&sink).is_empty(),
        "nothing may be written before a needle is refused"
    );

    // Four characters is allowed.
    let accepted = Recorder::create(
        &path,
        RecorderOptions {
            substitutions: vec![("abcd".to_string(), "<ok>".to_string())],
            ..options()
        },
    )
    .expect("a four character needle is allowed");
    accepted.finish().expect("finish");
}

struct SharedBuf(Sink);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        append(&self.0, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---- non-blocking queue ------------------------------------------------------------------------

fn dropped_total(records: &[Value]) -> usize {
    records
        .iter()
        .filter(|record| field(record, "kind") == "dropped")
        .map(|record| {
            let count = field(record, "count")
                .as_u64()
                .unwrap_or_else(|| panic!("count is not an integer"));
            assert!(count > 0, "a dropped record must count at least one frame");
            assert!(record.get("tMs").is_some(), "a dropped record carries tMs");
            count as usize
        })
        .sum()
}

#[test]
fn slow_writer_never_blocks_and_drops_are_recorded() {
    const CAPACITY: usize = 4;
    const CALLS: usize = 200;
    let (writer, mut gate) = gated();
    let recorder = Recorder::from_writer(
        writer,
        RecorderOptions {
            queue_capacity: Some(CAPACITY),
            ..options()
        },
    )
    .expect("from_writer");
    gate.arm();
    // The first frame goes to the writer, which then sits inside a blocked write.
    recorder.record_inbound(1, &event("message.delta", 0));
    gate.wait_blocked();

    // Far more calls than the queue holds. Every one must return while the writer is stuck.
    let producer = recorder.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        for n in 1..CALLS {
            producer.record_inbound(1, &event("message.delta", n));
        }
        let _ = done_tx.send(());
    });
    if done_rx.recv_timeout(Duration::from_secs(60)).is_err() {
        gate.release();
        panic!("record_inbound blocked on a stalled writer");
    }
    worker
        .join()
        .unwrap_or_else(|_| panic!("the producer panicked"));

    gate.release();
    let summary = recorder.finish().expect("finish");
    assert_eq!(
        summary.frames + summary.dropped,
        CALLS,
        "every call is either written or counted as dropped"
    );
    // One frame sat in the writer and at most `CAPACITY` waited in the queue.
    assert!(
        summary.frames >= CAPACITY && summary.frames <= CAPACITY + 2,
        "the queue must hold its capacity and no more: {} written",
        summary.frames
    );
    assert!(summary.dropped > 0);

    let lines = parse_lines(&gate.bytes());
    let records = &lines[1..];
    assert_gapless(records);
    let frames = records
        .iter()
        .filter(|record| field(record, "kind") == "frame")
        .count();
    assert_eq!(frames, summary.frames);
    assert_eq!(
        dropped_total(records),
        summary.dropped,
        "the file must state exactly how many frames were lost"
    );
}

#[test]
fn finish_with_clones_writes_pending_dropped_and_counts() {
    const CAPACITY: usize = 2;
    const CALLS: usize = 21;
    let (writer, mut gate) = gated();
    let original = Recorder::from_writer(
        writer,
        RecorderOptions {
            queue_capacity: Some(CAPACITY),
            ..options()
        },
    )
    .expect("from_writer");
    let clone = original.clone();
    let secret = sentinel();

    gate.arm();
    // The first frame is accepted for certain (the queue is empty) and carries one redaction.
    original.record_outbound(1, &request(1, "session.list", json!({"password": secret})));
    gate.wait_blocked();
    for n in 1..CALLS {
        let text = event("message.delta", n);
        if n % 2 == 0 {
            original.record_inbound(1, &text);
        } else {
            clone.record_inbound(1, &text);
        }
    }
    gate.release();

    // `finish` on one handle closes the queue for every clone.
    let summary = original.finish().expect("finish");
    assert_eq!(summary.frames + summary.dropped, CALLS);
    assert_eq!(summary.redactions, 1);
    assert!(
        summary.dropped > 0,
        "the queue was full for most of the calls"
    );

    let settled = gate.bytes();
    assert!(
        !contains(&settled, &secret),
        "the withheld value reached the file"
    );
    let lines = parse_lines(&settled);
    let records = &lines[1..];
    assert_gapless(records);
    let frames = records
        .iter()
        .filter(|record| field(record, "kind") == "frame")
        .count();
    assert_eq!(frames, summary.frames);
    assert_eq!(
        dropped_total(records),
        summary.dropped,
        "no drop may go unrecorded, including one with no later frame to precede"
    );

    // A later call on a clone is a no-op: no panic, no block, no new bytes.
    clone.record_inbound(1, &event("message.delta", 99));
    clone.record_outbound(1, &event("message.delta", 100));
    assert_eq!(
        gate.bytes(),
        settled,
        "a clone must not record after finish"
    );
}

// ---- write errors ------------------------------------------------------------------------------

#[test]
fn write_error_stops_recording() {
    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let armed = Arc::new(AtomicBool::new(false));
    let (failed_tx, failed_rx) = mpsc::channel();
    let recorder = Recorder::from_writer(
        FailingWriter {
            sink: Arc::clone(&sink),
            armed: Arc::clone(&armed),
            failed: failed_tx,
        },
        options(),
    )
    .expect("from_writer");
    let header_only = sink_bytes(&sink);
    assert_eq!(
        parse_lines(&header_only).len(),
        1,
        "the header went through"
    );

    armed.store(true, Ordering::SeqCst);
    recorder.record_inbound(1, &event("message.delta", 0));
    failed_rx
        .recv_timeout(Duration::from_secs(60))
        .unwrap_or_else(|_| panic!("the writer never tried to write the frame"));

    // Recording has stopped: more calls return at once and never panic.
    for n in 1..6 {
        recorder.record_inbound(1, &event("message.delta", n));
        recorder.record_outbound(1, &event("message.delta", n));
    }
    let outcome = recorder.finish();
    assert_eq!(
        outcome,
        Err(RecordError::Io(io::ErrorKind::BrokenPipe)),
        "finish must return the write error"
    );
    assert_eq!(
        sink_bytes(&sink),
        header_only,
        "nothing but the header reached the writer"
    );
}

#[test]
fn header_write_error_is_returned() {
    let sink: Sink = Arc::new(Mutex::new(Vec::new()));
    let (failed_tx, _failed_rx) = mpsc::channel();
    let outcome = Recorder::from_writer(
        FailingWriter {
            sink,
            armed: Arc::new(AtomicBool::new(true)),
            failed: failed_tx,
        },
        options(),
    );
    assert!(
        matches!(outcome, Err(RecordError::Io(io::ErrorKind::BrokenPipe))),
        "a header that cannot be written must be an Io error"
    );
}
