//! Live stream checks against the isolated fixture gateway.

use std::time::Duration;

fn live_require() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1")
}

fn skip(reason: &str) {
    println!("LIVE_STREAM_RESULT: SKIPPED ({reason})");
    if live_require() {
        panic!("a live run was required but skipped: {reason}");
    }
}

fn live_skip_reason() -> Option<String> {
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
    match runtime.block_on(hermes_client::discover_with(&home, &options)) {
        Ok(endpoint) => {
            if endpoint.resolve_profile("fixture").is_err() {
                return Some("fixture profile is not served".into());
            }
            hermes_contract::pinned::live_skip_reason(endpoint.code_sha())
        }
        Err(_) => Some("no Hermes gateway answered".into()),
    }
}

fn capture_tool_turn() -> String {
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
    std::fs::write(work.path().join("a.txt"), b"").expect("create fixture file");
    let capture = tempfile::tempdir().expect("recording dir");
    let recording = capture.path().join("tool-turn.jsonl");
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
    let (captured_sequences, interim_count, duration_seen) = runtime.block_on(async {
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
                "text": "Use the terminal tool to run ls in the current folder, then reply with only the file names.",
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
            let mut interim_count = 0;
            let mut duration_seen = false;
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
                if ours && params["type"] == "message.interim" { interim_count += 1; }
                if ours && params["type"] == "tool.complete" && params["payload"].get("duration_s").is_some() { duration_seen = true; }
                let wanted = matches!(params["type"].as_str(), Some("message.delta" | "tool.start" | "tool.complete" | "session.info" | "message.complete"));
                if wanted && (ours || (owner.is_none() && params["type"] == "session.info")) {
                    sequences.push(params["seq"].as_u64().expect("an event sequence"));
                }
                if ours && params["type"] == "message.complete" {
                    if !started { panic!("the finite turn completed without its start"); }
                    break;
                }
            }
            (sequences, interim_count, duration_seen)
        };
        let captured = tokio::time::timeout(Duration::from_secs(120), turn).await;
        let detached = client.call_raw("session.detach", json!({
            "session_id": session_id, "subscription_id": subscription_id,
        }), call_options).await;
        connection.link.close().await;
        detached.expect("detach the probe session");
        captured.expect("the finite fixture turn completed within two minutes")
    });
    println!("probe: message.interim={interim_count}, tool.complete duration_s={duration_seen}");
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

#[test]
#[ignore]
fn records_tool_turn_frames() {
    if let Some(reason) = live_skip_reason() {
        skip(&reason);
        return;
    }
    let recording = capture_tool_turn();
    let frames: Vec<serde_json::Value> = recording
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame JSON"))
        .collect();
    for kind in ["message.delta", "tool.start", "tool.complete"] {
        let matching: Vec<_> = frames
            .iter()
            .filter(|frame| frame["params"]["type"] == kind)
            .collect();
        assert!(!matching.is_empty(), "no own {kind} event arrived");
        for frame in matching {
            let params = &frame["params"];
            assert!(params["admission_id"].is_string());
            assert!(params["authority_epoch"].is_number());
            assert!(params["execution_generation"].is_number());
            let payload = params["payload"].as_object().expect("event payload object");
            let keys: &[&str] = match kind {
                "message.delta" => &["text"],
                "tool.start" => &[
                    "tool_id",
                    "name",
                    "context",
                    "tool_call_id",
                    "tool_name",
                    "args",
                ],
                _ => &[
                    "tool_id",
                    "name",
                    "context",
                    "tool_call_id",
                    "tool_name",
                    "args",
                    "is_error",
                    "result",
                ],
            };
            for key in keys {
                assert!(payload.contains_key(*key), "{kind} missing {key}");
            }
        }
    }
    if let Some(destination) = std::env::var_os("TALARIA_CAPTURE_STREAM_FIXTURE") {
        std::fs::write(destination, &recording).expect("write the redacted fixture");
    }
    println!("LIVE_STREAM_RESULT: PASSED");
}

fn command(work: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_talaria"));
    cmd.current_dir(work)
        .args([
            "-p",
            "--no-ensure",
            "--profile",
            "fixture",
            "--provider",
            "ollama-cloud",
            "--model",
            "minimax-m3.1-flash",
            "--output-format",
            "stream-json",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd
}

fn run_live(mut cmd: std::process::Command, input: &str) -> std::process::Output {
    use std::io::{Read, Write};
    let mut child = cmd.spawn().expect("start live stream");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(input.as_bytes()).expect("input");
    drop(stdin);
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).expect("stdout read");
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).expect("stderr read");
        bytes
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("status") {
            break status;
        }
        if started.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            panic!("live stream exceeded two minutes");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    std::process::Output {
        status,
        stdout: out.join().expect("stdout thread"),
        stderr: err.join().expect("stderr thread"),
    }
}

fn records(bytes: &[u8]) -> Vec<serde_json::Value> {
    std::str::from_utf8(bytes)
        .expect("UTF-8 stream")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON record per line"))
        .collect()
}

#[test]
#[ignore]
fn streams_text_and_result_live() {
    if let Some(reason) = live_skip_reason() {
        skip(&reason);
        return;
    }
    let work = workdir();
    let mut cmd = command(work.path());
    cmd.arg("Reply with only the word ok.");
    let out = run_live(cmd, "");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = records(&out.stdout);
    assert_eq!(rows.first().expect("init")["type"], "system");
    assert!(rows.iter().any(|r| r["type"] == "text"));
    assert_eq!(rows.last().expect("result")["status"], "completed");
    println!("LIVE_STREAM_RESULT: PASSED");
}

fn verify_two_finite_turns() {
    use hermes_client::{Inbound, liveness::LinkEvent, rpc::CallOptions};
    use serde_json::json;
    use talaria_app::connection::ConnectionService;
    use talaria_app::launch::{Launch, StartMode, new_launch_id, new_request_id};
    let home = std::env::var_os("HERMES_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(hermes_client::default_home);
    let work = workdir();
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".into(),
        folder: work.path().to_path_buf(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("probe runtime");
    runtime.block_on(async {
        let mut connection = ConnectionService::open(&launch, &home, None).await.expect("open fixture gateway");
        let client = connection.link.client();
        let opts = CallOptions { timeout:Some(Duration::from_secs(10)) };
        let opened = client.call_raw("session.create", json!({"request_id":new_request_id(), "source":"oneshot", "cwd":launch.folder}), opts).await.expect("create oneshot session");
        let session = opened["session_id"].as_str().expect("session").to_owned();
        let subscription = opened["subscription_id"].as_str().expect("subscription").to_owned();
        let launch_id = new_launch_id();
        let turn = async {
            for (number, prompt) in [(1,"What is 1234 plus 4321? Reply with only the number."), (2,"What number did you just give me? Reply with only the number.")] {
                let submit = client.call_raw("prompt.submit", json!({"session_id":session,"text":prompt,"submission_id":format!("talaria-{launch_id}-{number}"),"finite":true}), opts);
                tokio::pin!(submit);
                let mut held = std::collections::VecDeque::new();
                let reply = loop {
                    tokio::select! {
                        result = &mut submit => break result.expect("finite prompt accepted"),
                        event = connection.inbound.recv() => match event {
                            Some(LinkEvent::Inbound { item:Inbound::Event(event), .. }) => held.push_back(event.raw),
                            Some(_) => {},
                            None => panic!("link closed before admission"),
                        }
                    }
                };
                let admission = reply["admission_id"].as_str().expect("admission");
                loop {
                    let params = if let Some(event) = held.pop_front() { event } else { match connection.inbound.recv().await {
                        Some(LinkEvent::Inbound { item:Inbound::Event(event), .. }) => event.raw,
                        Some(_) => continue,
                        None => panic!("link closed before completion"),
                    }};
                    if params["session_id"] == session && params["admission_id"] == admission && params["type"] == "message.complete" {
                        assert_eq!(params["payload"]["outcome"], "completed");
                        assert_eq!(params["payload"]["text"].as_str().expect("answer").trim(), "5555");
                        break;
                    }
                }
            }
        };
        let outcome = tokio::time::timeout(Duration::from_secs(120), turn).await;
        let detached = client.call_raw("session.detach", json!({"session_id":session,"subscription_id":subscription}), opts).await;
        connection.link.close().await;
        detached.expect("detach probe session");
        outcome.expect("both finite turns finished");
    });
    println!("ONESHOT_PROBE: two finite turns completed in one session");
}

#[test]
#[ignore]
fn two_turns_recall_live() {
    if let Some(reason) = live_skip_reason() {
        skip(&reason);
        return;
    }
    verify_two_finite_turns();
    let work = workdir();
    let mut cmd = command(work.path());
    cmd.args(["--input-format", "stream-json"]);
    let input = "{\"type\":\"user\",\"text\":\"What is 1234 plus 4321? Reply with only the number.\"}\n{\"type\":\"user\",\"text\":\"What number did you just give me? Reply with only the number.\"}\n";
    let out = run_live(cmd, input);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows = records(&out.stdout);
    let results: Vec<_> = rows.iter().filter(|r| r["type"] == "result").collect();
    assert_eq!(results.len(), 2);
    for result in &results {
        assert_eq!(result["status"], "completed");
        assert_eq!(result["text"].as_str().unwrap().trim(), "5555");
    }
    assert_eq!(results[0]["session_id"], results[1]["session_id"]);
    assert_eq!(rows.iter().filter(|r| r["type"] == "system").count(), 1);
    println!("LIVE_STREAM_RESULT: PASSED");
}

#[test]
#[ignore]
fn interrupt_record_live() {
    use std::io::{BufRead, BufReader, Write};
    if let Some(reason) = live_skip_reason() {
        skip(&reason);
        return;
    }
    let work = workdir();
    let mut cmd = command(work.path());
    cmd.args(["--input-format", "stream-json"]);
    let mut child = cmd.spawn().expect("start live stream");
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.expect("output line")).is_err() {
                break;
            }
        }
    });
    writeln!(
        stdin,
        "{{\"type\":\"user\",\"text\":\"Count from 1 to 3000, one number per line.\"}}"
    )
    .expect("first prompt");
    loop {
        let line = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("submitted record");
        let row: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        if row["type"] == "submitted" {
            break;
        }
    }
    std::thread::sleep(Duration::from_secs(8));
    let interrupted_at = std::time::Instant::now();
    writeln!(stdin, "{{\"type\":\"interrupt\"}}\n{{\"type\":\"user\",\"text\":\"Reply with only the word ok.\"}}").expect("interrupt and next prompt");
    drop(stdin);
    let mut results = Vec::new();
    while results.len() < 2 {
        let line = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("result record");
        let row: serde_json::Value = serde_json::from_str(&line).expect("JSON");
        if row["type"] == "result" {
            if results.is_empty() {
                assert!(interrupted_at.elapsed() < Duration::from_secs(30));
            }
            results.push(row);
        }
    }
    assert_eq!(results[0]["status"], "interrupted");
    assert_eq!(results[0]["exit_code"], 130);
    assert_eq!(results[1]["status"], "completed");
    assert_eq!(child.wait().expect("exit").code(), Some(0));
    println!("LIVE_STREAM_RESULT: PASSED");
}
