//! Streaming runs against a scripted gateway.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use mock_gateway::{HeadlessScenario, MockGateway, StandaloneGateway};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use talaria_app::headless::run::StopCause;
use talaria_app::headless::service::HeadlessTimings;
use talaria_app::headless::stream_input::{INPUT_QUEUE, spawn_input_reader};
use talaria_app::headless::stream_service::{
    ObservedHook, StreamSource, run_stream, run_stream_with_observer,
};
use talaria_app::headless::{OutputFormat, PrintOptions};
use talaria_app::launch::{Launch, StartMode};

#[derive(Clone, Default)]
struct Capture {
    inner: Arc<Mutex<Captured>>,
    max_lines: Option<usize>,
}
#[derive(Default)]
struct Captured {
    bytes: Vec<u8>,
    flushed: Vec<(usize, Instant)>,
}
impl Capture {
    fn records(&self) -> Vec<Value> {
        let data = self.inner.lock().expect("capture lock");
        data.bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("one JSON object per line"))
            .collect()
    }
}
impl Write for Capture {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut capture = self.inner.lock().expect("capture lock");
        if let Some(limit) = self.max_lines
            && capture.bytes.iter().filter(|b| **b == b'\n').count() >= limit
        {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "reader closed"));
        }
        capture.bytes.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut capture = self.inner.lock().expect("capture lock");
        let lines = capture.bytes.iter().filter(|byte| **byte == b'\n').count();
        capture.flushed.push((lines, Instant::now()));
        Ok(())
    }
}

enum Input {
    Prompt,
    Lines(&'static str),
    Pipe,
}
struct Report {
    code: u8,
    records: Vec<Value>,
    submissions: Vec<mock_gateway::Submission>,
    interrupts: Vec<mock_gateway::Seen>,
    creates: usize,
    detaches: Vec<mock_gateway::DetachSeen>,
    violations: Vec<mock_gateway::Violation>,
    flushes: Vec<(usize, Instant)>,
}
fn launch() -> Launch {
    Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".into(),
        folder: "/work/proj".into(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    }
}
fn options() -> PrintOptions {
    PrintOptions {
        prompt_arg: None,
        format: OutputFormat::StreamJson,
        timeout: None,
    }
}

fn run_case<F, Fut>(
    scenario: HeadlessScenario,
    input: Input,
    max_lines: Option<usize>,
    driver: F,
) -> Report
where
    F: FnOnce(
        MockGateway,
        Capture,
        Option<UnixStream>,
        tokio::sync::mpsc::Sender<StopCause>,
    ) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    run_observed_case(scenario, input, max_lines, None, driver)
}

fn run_observed_case<F, Fut>(
    scenario: HeadlessScenario,
    input: Input,
    max_lines: Option<usize>,
    observer: Option<ObservedHook>,
    driver: F,
) -> Report
where
    F: FnOnce(
        MockGateway,
        Capture,
        Option<UnixStream>,
        tokio::sync::mpsc::Sender<StopCause>,
    ) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.configure_headless(scenario);
    let capture = Capture {
        max_lines,
        ..Capture::default()
    };
    let (source, writer) = match input {
        Input::Prompt => (StreamSource::Prompt("hello".into()), None),
        Input::Lines(lines) => (
            StreamSource::Lines(spawn_input_reader(
                std::io::Cursor::new(lines.as_bytes().to_vec()),
                INPUT_QUEUE,
            )),
            None,
        ),
        Input::Pipe => {
            let (read, write) = UnixStream::pair().expect("input pair");
            (
                StreamSource::Lines(spawn_input_reader(read, INPUT_QUEUE)),
                Some(write),
            )
        }
    };
    let mock = (*gateway).clone();
    let capture_driver = capture.clone();
    let (code, flushes) = gateway.block_on(async {
        let (stop_tx, stops) = tokio::sync::mpsc::channel(8);
        let task = tokio::spawn(driver(mock, capture_driver, writer, stop_tx));
        let run = async {
            match observer {
                Some(observer) => {
                    run_stream_with_observer(
                        &launch(),
                        &options(),
                        source,
                        gateway.home(),
                        Instant::now(),
                        stops,
                        HeadlessTimings::scaled_for_tests(),
                        capture.clone(),
                        observer,
                    )
                    .await
                }
                None => {
                    run_stream(
                        &launch(),
                        &options(),
                        source,
                        gateway.home(),
                        Instant::now(),
                        stops,
                        HeadlessTimings::scaled_for_tests(),
                        capture.clone(),
                    )
                    .await
                }
            }
        };
        let code = tokio::time::timeout(Duration::from_secs(8), run)
            .await
            .expect("stream finished");
        task.await.expect("scenario driver");
        let flushes = capture.inner.lock().expect("capture lock").flushed.clone();
        (code, flushes)
    });
    Report {
        code,
        records: capture.records(),
        submissions: gateway.submissions(),
        interrupts: gateway.requests("session.interrupt"),
        creates: gateway.created_sessions().len(),
        detaches: gateway.detaches(),
        violations: gateway.violations(),
        flushes,
    }
}

async fn wait_for<F: Fn() -> bool>(f: F) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("scenario condition");
}
async fn start(mock: &MockGateway, turn: u32) {
    wait_for(|| mock.submissions().len() >= turn as usize).await;
    mock.send_stream_event(
        "message.start",
        &format!("adm-{turn}"),
        i64::from(turn),
        json!({}),
    )
    .await;
}
async fn complete(mock: &MockGateway, turn: u32, outcome: &str) {
    mock.send_stream_event("message.complete", &format!("adm-{turn}"), i64::from(turn), json!({"admission_id":format!("adm-{turn}"), "text":"5555", "content":"5555", "outcome":outcome, "status":"complete"})).await;
}
async fn ordinary_turn(mock: &MockGateway, turn: u32) {
    start(mock, turn).await;
    mock.send_stream_event(
        "message.delta",
        &format!("adm-{turn}"),
        i64::from(turn),
        json!({"text":"5555"}),
    )
    .await;
    complete(mock, turn, "completed").await;
}
fn kinds(report: &Report) -> Vec<&str> {
    report
        .records
        .iter()
        .map(|r| r["type"].as_str().expect("type"))
        .collect()
}
fn lines(report: &Report, kind: &str) -> Vec<Value> {
    report
        .records
        .iter()
        .filter(|r| r["type"] == kind)
        .cloned()
        .collect()
}
fn write_input(pipe: &mut UnixStream, text: &str) {
    pipe.write_all(text.as_bytes()).expect("input line");
}

#[test]
fn one_turn_record_sequence() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            let (start_payload, result_payload) = mock_gateway::stream_tool_payloads();
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"Listing"}))
                .await;
            mock.send_stream_event("tool.start", "adm-1", 1, start_payload)
                .await;
            mock.send_stream_event("tool.complete", "adm-1", 1, result_payload)
                .await;
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"5555"}))
                .await;
            complete(&mock, 1, "completed").await;
        },
    );
    assert_eq!(report.code, 0);
    assert_eq!(
        kinds(&report),
        [
            "system",
            "submitted",
            "text",
            "tool_use",
            "tool_result",
            "text",
            "result"
        ]
    );
    assert!(report.violations.is_empty());
}

#[test]
fn result_record_equals_json_mode() {
    let stream = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, _| async move {
            ordinary_turn(&mock, 1).await;
        },
    );
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.configure_headless(HeadlessScenario::Complete);
    let mut json_result = gateway.block_on(async {
        let (_tx, rx) = tokio::sync::mpsc::channel(8);
        let result = talaria_app::headless::service::run_headless(
            &launch(),
            &options(),
            "hello".into(),
            gateway.home(),
            Instant::now(),
            rx,
            HeadlessTimings::scaled_for_tests(),
        )
        .await;
        serde_json::from_str::<Value>(&talaria_app::headless::output::render_json(&result))
            .expect("json result")
    });
    let mut streamed = lines(&stream, "result").pop().expect("result");
    streamed["duration_ms"] = json!(0);
    json_result["duration_ms"] = json!(0);
    assert_eq!(streamed, json_result);
}

#[test]
fn tool_payload_keys_survive_inbound_path() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            let (mut first, mut result) = mock_gateway::stream_tool_payloads();
            first.as_object_mut().unwrap().remove("tool_id");
            mock.send_stream_event("tool.start", "adm-1", 1, first)
                .await;
            result["is_error"] = json!(true);
            mock.send_stream_event("tool.complete", "adm-1", 1, result.clone())
                .await;
            result["is_error"] = json!(false);
            mock.send_stream_event("tool.complete", "adm-1", 1, result)
                .await;
            complete(&mock, 1, "completed").await;
        },
    );
    assert_eq!(lines(&report, "tool_use")[0]["tool_id"], "call_1");
    let results = lines(&report, "tool_result");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["is_error"], true);
    assert_eq!(results[1]["is_error"], false);
}

#[test]
fn neighbour_events_never_become_records() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, capture, _, _| async move {
            mock.set_behavior(
                "prompt.submit",
                mock_gateway::Behavior::Delay(Duration::from_millis(200)),
            );
            start(&mock, 1).await;
            for kind in ["message.delta", "tool.start", "tool.complete"] {
                mock.send_stream_event(
                    kind,
                    "adm-foreign",
                    9,
                    json!({"text":"foreign-before","tool_id":"foreign"}),
                )
                .await;
            }
            wait_for(|| capture.records().iter().any(|r| r["type"] == "submitted")).await;
            for kind in ["message.delta", "tool.start", "tool.complete"] {
                mock.send_stream_event(
                    kind,
                    "adm-foreign",
                    9,
                    json!({"text":"foreign-after","tool_id":"foreign"}),
                )
                .await;
            }
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"own"}))
                .await;
            complete(&mock, 1, "completed").await;
        },
    );
    assert_eq!(lines(&report, "text").len(), 1);
    assert_eq!(lines(&report, "text")[0]["text"], "own");
    assert!(lines(&report, "tool_use").is_empty());
    assert!(lines(&report, "tool_result").is_empty());
}

#[test]
fn two_turns_one_session_in_order() {
    let input = "{\"type\":\"user\",\"text\":\"one\"}\n{\"type\":\"user\",\"text\":\"two\"}\n";
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Lines(input),
        None,
        |mock, _, _, _| async move {
            ordinary_turn(&mock, 1).await;
            ordinary_turn(&mock, 2).await;
        },
    );
    assert_eq!(report.code, 0);
    assert_eq!(report.creates, 1);
    assert_eq!(report.detaches.len(), 1);
    assert_eq!(report.submissions.len(), 2);
    assert_eq!(
        report.submissions[0].session_id,
        report.submissions[1].session_id
    );
    let a = &report.submissions[0].submission_id;
    let b = &report.submissions[1].submission_id;
    assert!(a.ends_with("-1") && b.ends_with("-2"));
    assert_eq!(a.trim_end_matches("-1"), b.trim_end_matches("-2"));
    assert_eq!(lines(&report, "result").len(), 2);
    assert!(report.violations.is_empty());
}

#[test]
fn second_turn_blocked_by_live_state() {
    for kind in ["approval.request", "session.info"] {
        let report = run_case(
            HeadlessScenario::StreamManual,
            Input::Pipe,
            None,
            move |mock, capture, pipe, _| async move {
                let mut pipe = pipe.expect("input pipe");
                write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"one\"}\n");
                ordinary_turn(&mock, 1).await;
                wait_for(|| capture.records().iter().any(|r| r["type"] == "result")).await;
                let payload = if kind == "approval.request" {
                    json!({"prompt_id":"foreign-prompt", "execution_generation":9})
                } else {
                    json!({"pending":[{"status":"unknown"}]})
                };
                mock.send_stream_event(kind, "adm-foreign", 9, payload)
                    .await;
                tokio::time::sleep(Duration::from_millis(30)).await;
                write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"two\"}\n");
                drop(pipe);
            },
        );
        assert_eq!(report.code, 3, "{kind}");
        assert_eq!(report.submissions.len(), 1, "{kind}");
        assert_eq!(
            lines(&report, "result").last().unwrap()["status"],
            "input_required",
            "{kind}"
        );
    }
}

#[test]
fn failed_turn_ends_run() {
    let input = "{\"type\":\"user\",\"text\":\"one\"}\n{\"type\":\"user\",\"text\":\"two\"}\n";
    let report = run_case(
        HeadlessScenario::StreamFailed,
        Input::Lines(input),
        None,
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            complete(&mock, 1, "failed").await;
        },
    );
    assert_eq!(report.code, 1);
    assert_eq!(report.submissions.len(), 1);
    assert_eq!(lines(&report, "result")[0]["status"], "failed");
}

#[test]
fn interrupt_record_mid_turn_then_next_turn() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Pipe,
        None,
        |mock, capture, pipe, _| async move {
            let mut pipe = pipe.expect("input pipe");
            write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"one\"}\n");
            start(&mock, 1).await;
            write_input(
                &mut pipe,
                "{\"type\":\"interrupt\"}\n{\"type\":\"user\",\"text\":\"two\"}\n",
            );
            wait_for(|| capture.records().iter().any(|r| r["type"] == "result")).await;
            ordinary_turn(&mock, 2).await;
            drop(pipe);
        },
    );
    assert_eq!(report.code, 0);
    let results = lines(&report, "result");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["status"], "interrupted");
    assert_eq!(results[0]["exit_code"], 130);
    assert_eq!(results[1]["status"], "completed");
    assert_eq!(report.interrupts.len(), 1);
    assert_eq!(report.interrupts[0].params["execution_generation"], 1);
}

#[test]
fn closed_stdout_stops_turn_exit_141() {
    let input = "{\"type\":\"user\",\"text\":\"one\"}\n{\"type\":\"user\",\"text\":\"two\"}\n";
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Lines(input),
        Some(2),
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"one"}))
                .await;
            wait_for(|| !mock.requests("session.interrupt").is_empty()).await;
        },
    );
    assert_eq!(report.code, 141);
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.submissions.len(), 1);
    assert_eq!(report.interrupts.len(), 1);
    assert_eq!(report.interrupts[0].params["execution_generation"], 1);
}

#[test]
fn interrupts_when_idle_warn() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Pipe,
        None,
        |mock, capture, pipe, _| async move {
            let mut pipe = pipe.expect("input pipe");
            write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"one\"}\n");
            ordinary_turn(&mock, 1).await;
            wait_for(|| {
                capture
                    .records()
                    .iter()
                    .filter(|r| r["type"] == "result")
                    .count()
                    == 1
            })
            .await;
            write_input(
                &mut pipe,
                "{\"type\":\"interrupt\"}\n{\"type\":\"user\",\"text\":\"two\"}\n",
            );
            ordinary_turn(&mock, 2).await;
            wait_for(|| {
                capture
                    .records()
                    .iter()
                    .filter(|r| r["type"] == "result")
                    .count()
                    == 2
            })
            .await;
            write_input(&mut pipe, "{\"type\":\"interrupt\"}\n");
            drop(pipe);
        },
    );
    assert_eq!(report.code, 0);
    let warnings = lines(&report, "warning");
    assert_eq!(warnings.len(), 2);
    assert!(
        warnings
            .iter()
            .all(|w| w["text"] == "nothing to interrupt.")
    );
    assert_eq!(report.records.last().unwrap()["type"], "warning");
}

#[test]
fn input_errors_end_run_with_exit_2() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Pipe,
        None,
        |mock, capture, pipe, _| async move {
            let mut pipe = pipe.expect("input pipe");
            write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"one\"}\n");
            ordinary_turn(&mock, 1).await;
            wait_for(|| capture.records().iter().any(|r| r["type"] == "result")).await;
            write_input(&mut pipe, "not json\n");
            drop(pipe);
        },
    );
    assert_eq!(report.code, 2);
    assert_eq!(report.records.last().unwrap()["type"], "input_error");
    assert_eq!(report.records.last().unwrap()["line"], 2);
    assert_eq!(
        report.records.last().unwrap()["what"],
        "line 2 is not valid JSON."
    );
    let empty = run_case(
        HeadlessScenario::StreamManual,
        Input::Lines(""),
        None,
        |_, _, _, _| async {},
    );
    assert_eq!(empty.code, 2);
    assert_eq!(
        empty.records.last().unwrap()["what"],
        "no user record arrived before the end of standard input."
    );
    assert_eq!(empty.records.last().unwrap()["line"], Value::Null);
}

#[test]
fn input_error_mid_turn_stops_then_reports_error() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Pipe,
        None,
        |mock, _, pipe, _| async move {
            let mut pipe = pipe.expect("input pipe");
            write_input(&mut pipe, "{\"type\":\"user\",\"text\":\"one\"}\n");
            start(&mock, 1).await;
            write_input(
                &mut pipe,
                "not json\n{\"type\":\"user\",\"text\":\"never\"}\n",
            );
            drop(pipe);
            wait_for(|| !mock.requests("session.interrupt").is_empty()).await;
        },
    );
    assert_eq!(report.code, 2);
    assert_eq!(report.interrupts.len(), 1);
    assert_eq!(report.submissions.len(), 1);
    assert_eq!(report.detaches.len(), 1);
    let result = lines(&report, "result").pop().expect("stopped result");
    assert_eq!(result["status"], "interrupted");
    assert_eq!(result["exit_code"], 130);
    assert_eq!(report.records.last().unwrap()["type"], "input_error");
    assert_eq!(report.records.last().unwrap()["line"], 2);
}

#[test]
fn replay_gap_writes_gap_and_full_result() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"before"}))
                .await;
            mock.skip_stream_sequence();
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"after"}))
                .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            complete(&mock, 1, "completed").await;
        },
    );
    assert_eq!(lines(&report, "gap").len(), 1);
    let gap = kinds(&report).iter().position(|k| *k == "gap").unwrap();
    assert!(!kinds(&report)[gap + 1..].contains(&"text"));
    assert_eq!(lines(&report, "result").last().unwrap()["text"], "5555");
    assert!(
        lines(&report, "warning")
            .iter()
            .any(|w| w["text"].as_str().unwrap().contains("stored copy"))
    );
}

#[test]
fn mock_tool_events_match_recorded_shape() {
    use std::collections::BTreeSet;
    let frames: Vec<Value> = include_str!("data/headless/stream_tool_turn.frames.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame"))
        .collect();
    let (start, complete) = mock_gateway::stream_tool_payloads();
    for (kind, mock) in [("tool.start", start), ("tool.complete", complete)] {
        let recorded = frames
            .iter()
            .find(|v| v["params"]["type"] == kind)
            .expect("recorded tool frame");
        let keys = |v: &Value| {
            v.as_object()
                .expect("payload object")
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(keys(&recorded["params"]["payload"]), keys(&mock), "{kind}");
    }
    let text = include_str!("data/headless/stream_tool_turn.frames.jsonl");
    assert!(hermes_client::scan_for_secrets(text).is_empty());
    assert!(
        hermes_client::scan_literals(text, &hermes_client::redact::local_forbidden_literals())
            .is_empty()
    );
}

#[test]
fn sigterm_writes_result_then_143() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, stops| async move {
            start(&mock, 1).await;
            stops.send(StopCause::Signal(15)).await.expect("signal");
            wait_for(|| !mock.requests("session.interrupt").is_empty()).await;
        },
    );
    assert_eq!(report.code, 143);
    let result = report.records.last().expect("last result");
    assert_eq!(result["type"], "result");
    assert_eq!(result["status"], "interrupted");
    assert_eq!(result["exit_code"], 143);
}

#[test]
fn records_flushed_within_50ms() {
    fn timed_observer(times: Arc<Mutex<Vec<(String, Instant)>>>) -> ObservedHook {
        Box::new(move |kind, payload| {
            if kind == "message.delta" {
                times
                    .lock()
                    .unwrap()
                    .push((payload["text"].as_str().unwrap().to_owned(), Instant::now()));
            }
        })
    }
    fn check_flushes(report: &Report, observed: &[(String, Instant)]) {
        let text_records: Vec<_> = report
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r["type"] == "text")
            .collect();
        assert_eq!(text_records.len(), observed.len());
        for ((index, record), (text, eligible)) in text_records.into_iter().zip(observed) {
            assert_eq!(record["text"], *text);
            let flushed = report
                .flushes
                .iter()
                .find(|(lines, _)| *lines > index)
                .expect("record flushed")
                .1;
            assert!(
                flushed.duration_since(*eligible) <= Duration::from_millis(50),
                "record {text} was not flushed within 50 ms of observation"
            );
        }
    }
    let observed = Arc::new(Mutex::new(Vec::new()));
    let report = run_observed_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        Some(timed_observer(observed.clone())),
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            for n in 0..200 {
                mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":format!("{n}")}))
                    .await;
            }
            complete(&mock, 1, "completed").await;
        },
    );
    let times = observed.lock().unwrap().clone();
    assert_eq!(times.len(), 200);
    check_flushes(&report, &times);

    let held = Arc::new(Mutex::new(Vec::new()));
    let before_reply = held.clone();
    let delayed = run_observed_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        Some(timed_observer(held.clone())),
        move |mock, capture, _, _| async move {
            mock.set_behavior(
                "prompt.submit",
                mock_gateway::Behavior::Delay(Duration::from_millis(200)),
            );
            start(&mock, 1).await;
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"early"}))
                .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(capture.records().iter().all(|r| r["type"] == "system"));
            assert!(before_reply.lock().unwrap().is_empty());
            wait_for(|| capture.records().iter().any(|r| r["type"] == "text")).await;
            complete(&mock, 1, "completed").await;
        },
    );
    let kinds = kinds(&delayed);
    assert!(kinds.iter().position(|k| *k == "submitted") < kinds.iter().position(|k| *k == "text"));
    let times = held.lock().unwrap().clone();
    assert_eq!(times.len(), 1);
    check_flushes(&delayed, &times);
}

#[test]
fn queued_interrupt_before_submission_stops_without_sending() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Lines("{\"type\":\"user\",\"text\":\"one\"}\n{\"type\":\"interrupt\"}\n"),
        None,
        |_, _, _, _| async {},
    );
    assert_eq!(report.code, 130);
    assert!(report.submissions.is_empty());
    assert_eq!(lines(&report, "result")[0]["exit_code"], 130);
    assert_eq!(report.detaches.len(), 1);
}

#[test]
fn gap_with_a_running_recovery_uses_the_stored_answer() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        None,
        |mock, _, _, _| async move {
            start(&mock, 1).await;
            let mut snapshot = mock_gateway::recorded_create_snapshot();
            snapshot["pending"] = json!([{
                "admission_id":"adm-1", "authority_epoch":1, "execution_generation":1,
                "input_id":"input-1", "outcome":null,
                "ref":{"profile_id":"fixture", "session_id":"local-session-1"},
                "sequence":1, "status":"started", "text":"hello"
            }]);
            mock.add_session_snapshot(snapshot);
            mock.skip_stream_sequence();
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"lost"}))
                .await;
            wait_for(|| mock.requests("session.resume").len() >= 3).await;
            mock.send_stream_event("message.complete", "adm-1", 1, json!({
                "admission_id":"adm-1", "text":"live-only", "outcome":"completed", "status":"complete"
            })).await;
        },
    );
    assert_eq!(report.code, 0);
    let result = lines(&report, "result").pop().expect("result");
    assert_eq!(result["text"], "5555");
    assert!(
        result["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("stored copy"))
    );
    assert!(
        lines(&report, "warning")
            .iter()
            .any(|warning| warning["text"].as_str().unwrap().contains("stored copy"))
    );
}

#[test]
fn signal_after_output_closes_takes_precedence() {
    let report = run_case(
        HeadlessScenario::StreamManual,
        Input::Prompt,
        Some(2),
        |mock, _, _, stops| async move {
            mock.hold("session.interrupt");
            start(&mock, 1).await;
            mock.send_stream_event("message.delta", "adm-1", 1, json!({"text":"one"}))
                .await;
            wait_for(|| !mock.requests("session.interrupt").is_empty()).await;
            stops.send(StopCause::Signal(15)).await.expect("signal");
        },
    );
    assert_eq!(report.code, 143);
    assert_eq!(report.records.len(), 2);
    assert!(report.detaches.is_empty());
}
