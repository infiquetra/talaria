//! Live headless runs against the operator's gateway and fixture profile.
//!
//! Each test skips naming the missing precondition; under
//! `TALARIA_LIVE_REQUIRE=1` a skip panics.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const PROFILE: &[&str] = &[
    "--profile",
    "fixture",
    "--provider",
    "ollama-cloud",
    "--model",
    "minimax-m3.1-flash",
];

fn live_require() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1")
}

fn skip(reason: &str) {
    println!("LIVE_PRINT_RESULT: SKIPPED ({reason})");
    if live_require() {
        panic!("a live run was required but skipped: {reason}");
    }
}

fn gateway_present() -> bool {
    let home = std::env::var_os("HERMES_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_client::default_home);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("discovery runtime");
    let options = hermes_client::DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: hermes_client::EnsurePolicy::Never,
    };
    runtime
        .block_on(hermes_client::discover_with(&home, &options))
        .is_ok()
}

fn capture_finite_turn() -> String {
    use hermes_client::Inbound;
    use hermes_client::liveness::LinkEvent;
    use hermes_client::record::{Recorder, RecorderOptions};
    use hermes_client::rpc::CallOptions;
    use serde_json::{Value, json};
    use talaria_app::connection::ConnectionService;
    use talaria_app::launch::{Launch, StartMode, new_launch_id, new_request_id};

    let home = std::env::var_os("HERMES_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_client::default_home);
    let work = workdir();
    let recording = work.path().join("finite-turn.jsonl");
    let mut substitutions = vec![
        (
            home.to_string_lossy().into_owned(),
            "/work/proj".to_string(),
        ),
        (
            work.path().to_string_lossy().into_owned(),
            "/work/proj".to_string(),
        ),
    ];
    substitutions.extend(
        hermes_client::redact::local_forbidden_literals()
            .into_iter()
            .map(|literal| (literal, "[redacted]".to_string())),
    );
    let recorder = Recorder::create(
        &recording,
        RecorderOptions {
            substitutions,
            ..RecorderOptions::default()
        },
    )
    .expect("finite-turn recorder");
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_string(),
        folder: work.path().to_path_buf(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("probe runtime");
    let captured_sequences = runtime.block_on(async {
        let mut connection = ConnectionService::open(&launch, &home, Some(recorder.clone()))
            .await.expect("open the fixture gateway without starting it");
        let client = connection.link.client();
        let call_options = CallOptions { timeout: Some(Duration::from_secs(10)) };
        let snapshot = client.call_raw("session.create", json!({
            "request_id": new_request_id(), "source": "oneshot", "cwd": launch.folder,
        }), call_options).await.expect("create the finite fixture session");
        let session_id = snapshot["session_id"].as_str().expect("a session id").to_string();
        let subscription_id = snapshot["subscription_id"].as_str().expect("a subscription id").to_string();
        let turn = async {
            let submit = client.call_raw("prompt.submit", json!({
                "session_id": session_id,
                "text": "What is 1234 plus 4321? Reply with only the number.",
                "submission_id": format!("talaria-{}-1", new_launch_id()),
                "finite": true,
            }), call_options);
            tokio::pin!(submit);
            let mut buffered = std::collections::VecDeque::new();
            let reply = loop {
                tokio::select! {
                    reply = &mut submit => break reply.expect("admit the finite prompt"),
                    event = connection.inbound.recv() => {
                        if let Some(LinkEvent::Inbound { item: Inbound::Event(event), .. }) = event {
                            buffered.push_back(event.raw);
                        } else if event.is_none() {
                            panic!("the probe link closed before admission");
                        }
                    }
                }
            };
            let admission = reply["admission_id"].as_str().expect("an admission id");
            let mut started = false;
            let mut sequences = Vec::new();
            loop {
                let params = if let Some(params) = buffered.pop_front() {
                    params
                } else {
                    match connection.inbound.recv().await {
                        Some(LinkEvent::Inbound { item: Inbound::Event(event), .. }) => event.raw,
                        Some(_) => continue,
                        None => panic!("the probe link closed before completion"),
                    }
                };
                if params["session_id"] != session_id { continue; }
                let owner = params.get("admission_id").and_then(Value::as_str)
                    .or_else(|| params.pointer("/payload/admission_id").and_then(Value::as_str));
                let ours = owner == Some(admission);
                if ours && params["type"] == "message.start" { started = true; }
                if started && (ours || owner.is_none()) {
                    sequences.push(params["seq"].as_u64().expect("an event sequence"));
                }
                if ours && params["type"] == "message.complete" {
                    if !started { panic!("the finite turn completed without its start"); }
                    break;
                }
            }
            sequences
        };
        let captured = tokio::time::timeout(Duration::from_secs(120), turn).await;
        let detached = client.call_raw("session.detach", json!({
            "session_id": session_id, "subscription_id": subscription_id,
        }), call_options).await;
        connection.link.close().await;
        detached.expect("detach the probe session");
        captured.expect("the finite fixture turn completed within two minutes")
    });
    let summary = recorder.finish().expect("finish the finite-turn recording");
    if summary.dropped != 0 {
        panic!("the fixture recording dropped frames");
    }
    let recording = std::fs::read_to_string(recording).expect("read the redacted recording");
    let mut text = String::new();
    for line in recording.lines() {
        let entry: Value = serde_json::from_str(line).expect("recording entry");
        let frame = &entry["frame"];
        if entry["kind"] == "frame"
            && entry["dir"] == "in"
            && frame["method"] == "event"
            && frame["params"]["seq"]
                .as_u64()
                .is_some_and(|seq| captured_sequences.contains(&seq))
        {
            text.push_str(&frame.to_string());
            text.push('\n');
        }
    }
    if !hermes_client::scan_for_secrets(&text).is_empty()
        || !hermes_client::scan_literals(&text, &hermes_client::redact::local_forbidden_literals())
            .is_empty()
    {
        panic!("captured fixture failed the public-data scan");
    }
    for path in ["/Users/", "/home/", "/private/", "/tmp/", "/var/"] {
        if text.contains(path) {
            panic!("captured fixture contains a local absolute path");
        }
    }
    text
}

fn workdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("work dir")
}

fn run(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("run talaria")
}

fn with_profile(extra: &[&str]) -> Vec<String> {
    let mut args = vec!["-p".to_string()];
    args.extend(PROFILE.iter().map(|entry| (*entry).to_string()));
    args.extend(extra.iter().map(|entry| (*entry).to_string()));
    args
}

#[test]
#[ignore]
fn text_answer_exit_0_and_json_usage() {
    if !gateway_present() {
        skip("no Hermes gateway answered");
        return;
    }
    let work = workdir();
    let args = with_profile(&["What is 1234 plus 4321? Reply with only the number."]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(&refs, work.path());
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "5555\n");
    let args = with_profile(&[
        "--output-format",
        "json",
        "What is 1234 plus 4321? Reply with only the number.",
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(&refs, work.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON line");
    assert_eq!(value["status"], "completed");
    assert_eq!(value["exit_code"], 0);
    assert_eq!(value["text"], "5555");
    assert!(
        !value["session_id"].as_str().unwrap_or_default().is_empty(),
        "a session id"
    );
    assert_eq!(value["usage"].as_object().map(|usage| usage.len()), Some(9));
    println!("LIVE_PRINT_RESULT: PASSED");
}

#[test]
#[ignore]
fn resume_headless_session_by_id() {
    if !gateway_present() {
        skip("no Hermes gateway answered");
        return;
    }
    let work = workdir();
    let args = with_profile(&[
        "--output-format",
        "json",
        "What is 1234 plus 4321? Reply with only the number.",
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(&refs, work.path());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON line");
    let id = value["session_id"]
        .as_str()
        .expect("a session id")
        .to_string();
    let output = run(
        &[
            "-p",
            "--profile",
            "fixture",
            "--resume",
            &id,
            "What number did you give me before? Reply with only the number.",
        ],
        work.path(),
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "5555\n");
    let output = run(&["-p", "-c", "--profile", "fixture", "hello"], work.path());
    assert_eq!(output.status.code(), Some(4));
    println!("LIVE_PRINT_RESULT: PASSED");
}

#[test]
#[ignore]
fn dangerous_command_denied_without_prompt() {
    if !gateway_present() {
        skip("no Hermes gateway answered");
        return;
    }
    let work = workdir();
    std::fs::create_dir_all(work.path().join(".pytest_cache")).expect("cache dir");
    std::fs::write(work.path().join(".pytest_cache/x"), "x").expect("marker");
    let record = work.path().join("rec.jsonl");
    let args = with_profile(&[
        "--record",
        record.to_str().expect("a path"),
        "Run this exact shell command once (do not run any fallback or alternative command if it is blocked) and tell me the result: rm -rf .pytest_cache",
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let started = Instant::now();
    let _ = run(&refs, work.path());
    assert!(
        started.elapsed() < Duration::from_secs(120),
        "no waiting on a person"
    );
    assert!(
        work.path().join(".pytest_cache/x").exists(),
        "the folder must survive"
    );
    let rec = std::fs::read_to_string(&record).expect("a recording");
    assert!(
        !rec.contains("\"approval.request\""),
        "no approval prompt may appear"
    );
    assert!(rec.contains("BLOCKED"), "the denial must be recorded");
    println!("LIVE_PRINT_RESULT: PASSED");
}

#[test]
#[ignore]
fn timeout_stops_a_long_turn() {
    if !gateway_present() {
        skip("no Hermes gateway answered");
        return;
    }
    let work = workdir();
    let args = with_profile(&[
        "--timeout",
        "5",
        "--output-format",
        "json",
        "Count from 1 to 3000, one number per line.",
    ]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let started = Instant::now();
    let output = run(&refs, work.path());
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the stop budget bounds the run"
    );
    assert_eq!(output.status.code(), Some(124));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("talaria: the time limit passed; the turn was stopped."),
        "{stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON line");
    let id = value["session_id"]
        .as_str()
        .expect("a session id")
        .to_string();
    let output = run(
        &[
            "-p",
            "--profile",
            "fixture",
            "--resume",
            &id,
            "Reply with only the word ok.",
        ],
        work.path(),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "any answer passes"
    );
    println!("LIVE_PRINT_RESULT: PASSED");
}

#[test]
#[ignore]
fn records_finite_turn_frames() {
    if !gateway_present() {
        skip("no Hermes gateway answered");
        return;
    }
    // The probe records this turn directly, independently of headless implementation.
    let text = capture_finite_turn();
    let mut seen = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let frame: serde_json::Value = serde_json::from_str(line).expect("one frame per line");
        if frame["params"]["type"] != "message.complete" {
            continue;
        }
        seen = true;
        let payload = frame["params"]["payload"].as_object().expect("a payload");
        for key in ["admission_id", "outcome", "status", "text"] {
            assert!(payload.contains_key(key), "the completion lacks {key}");
        }
    }
    assert!(seen, "a completion frame");
    if let Ok(out) = std::env::var("TALARIA_HEADLESS_RECORD_OUT") {
        std::fs::write(out, &text).expect("record out");
    }
    println!("LIVE_PRINT_RESULT: PASSED");
}
