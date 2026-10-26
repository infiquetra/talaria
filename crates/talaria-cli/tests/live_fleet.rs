//! Live fleet list against a running Hermes gateway. Ignored unless someone asks.
//!
//! A skip is not a pass. With `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//! Each test prints exactly one `LIVE_FLEET_RESULT` line.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use hermes_client::{
    CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError,
    Purpose, Scope, connect, default_home, discover_with, mint_ticket,
};
use serde_json::{Value, json};

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn skip(reason: &str) {
    println!("LIVE_FLEET_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn finish(result: Result<(), String>) {
    match result {
        Ok(()) => println!("LIVE_FLEET_RESULT: PASSED"),
        Err(reason) => {
            println!("LIVE_FLEET_RESULT: FAILED ({reason})");
            panic!("{reason}");
        }
    }
}

async fn open_fixture() -> Result<(PathBuf, Client), String> {
    let (home, client, _inbound) = attach_fixture().await?;
    Ok((home, client))
}

async fn attach_fixture() -> Result<(PathBuf, Client, hermes_client::InboundStream), String> {
    let home = hermes_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = discover_with(&home, &options)
        .await
        .map_err(|error| match error {
            DiscoveryError::NoGateway => "no gateway".to_string(),
            other => format!("discovery failed: {other}"),
        })?;
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        return Err(reason);
    }
    let profile = endpoint
        .resolve_profile("fixture")
        .map_err(|error| match error {
            ProfileLookupError::NeverDiscovered { .. } => "fixture not served".to_string(),
            other => format!("fixture lookup failed: {other}"),
        })?;
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|error| format!("ticket failed: {error}"))?;
    let connection = connect(&endpoint, ticket)
        .await
        .map_err(|error| format!("connect failed: {error}"))?;
    let (client, inbound) = Client::start(connection.into_parts());
    Ok((home, client, inbound))
}

async fn create_session(client: &Client, folder: &Path, nonce: &str) -> Result<String, String> {
    let created = client
        .call_raw(
            "session.create",
            json!({
                // Hermes keeps a receipt per request id, so every run needs its own.
                "request_id": format!("fleet-{nonce}-{}", run_nonce()),
                "source": "tui",
                "cwd": folder.display().to_string(),
                "profile": "fixture",
            }),
            CallOptions {
                timeout: Some(Duration::from_secs(20)),
            },
        )
        .await
        .map_err(|error| format!("session.create failed: {error}"))?;
    created["session_id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "session.create returned no session id".to_string())
}

fn run_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

fn run_fleet(home: &Path, args: &[&str], limit: Duration) -> Result<std::process::Output, String> {
    let child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("HERMES_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("the talaria binary could not be started: {error}"))?;
    let pid = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(limit) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("the talaria binary could not be read: {error}")),
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            Err("the fleet command did not exit".into())
        }
    }
}

fn spawn_watch(
    home: &Path,
    seconds: &str,
) -> Result<(std::process::Child, std::sync::mpsc::Receiver<String>), String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "fleet",
            "list",
            "--watch",
            "--json",
            "--profile",
            "fixture",
            "--for",
            seconds,
        ])
        .env("HERMES_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("the talaria binary could not be started: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "the watch has no stdout".to_string())?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in std::io::BufRead::lines(reader) {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    Ok((child, receiver))
}

fn stop_watch(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn poll_json(lines: &std::sync::mpsc::Receiver<String>) -> Option<Value> {
    let line = lines.try_recv().ok()?;
    serde_json::from_str(&line).ok()
}

#[tokio::test]
#[ignore]
async fn live_fleet_lists_fixture_sessions() {
    let (home, client) = match open_fixture().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let result = async {
        let folder = tempfile::tempdir().map_err(|error| error.to_string())?;
        let first = create_session(&client, folder.path(), "a").await?;
        let second = create_session(&client, folder.path(), "b").await?;
        let started = Instant::now();
        let output = run_fleet(
            &home,
            &["fleet", "list", "--json", "--profile", "fixture"],
            Duration::from_secs(20),
        )?;
        if started.elapsed() > Duration::from_secs(5) {
            return Err("both sessions were not listed within 5 s".into());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if !text.contains(&first) || !text.contains(&second) {
            return Err("the list did not show both fixture sessions".into());
        }
        if text.contains("\"profile\":\"default\"") {
            return Err("the list showed a default row".into());
        }
        Ok(())
    }
    .await;
    finish(result);
}

#[tokio::test]
#[ignore]
async fn live_waiting_session_reaches_queue() {
    let (home, client, mut inbound) = match attach_fixture().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let result = async {
        let folder = tempfile::tempdir().map_err(|error| error.to_string())?;
        std::fs::write(folder.path().join("talaria-fleet-probe"), b"")
            .map_err(|error| error.to_string())?;
        let session_id = create_session(&client, folder.path(), "wait").await?;
        let (mut child, lines) = spawn_watch(&home, "300")?;
        let mut needs = Vec::new();
        let mut closed = Vec::new();
        for episode in 0..3 {
            client
                .call_raw(
                    "prompt.submit",
                    json!({
                        "session_id": session_id,
                        "text": "Run this exact shell command: rm -rf ./talaria-fleet-probe",
                        "submission_id": format!("fleet-wait-{episode}"),
                    }),
                    CallOptions {
                        timeout: Some(Duration::from_secs(30)),
                    },
                )
                .await
                .map_err(|error| format!("prompt.submit failed: {error}"))?;
            let started = Instant::now();
            let mut asked = None;
            let mut opened = None;
            let mut prompt_id = String::new();
            let mut generation = 1_i64;
            while started.elapsed() < Duration::from_secs(60)
                && (asked.is_none() || opened.is_none())
            {
                if child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    break;
                }
                while let Some(value) = poll_json(&lines) {
                    if value["kind"] == "need_opened"
                        && value["need"]["kind"] == "approval"
                        && value["need"]["summary"]
                            .as_str()
                            .is_some_and(|text| text.contains("talaria-fleet-probe"))
                    {
                        opened = Some(Instant::now());
                    }
                }
                let next = tokio::time::timeout(Duration::from_millis(50), inbound.next()).await;
                if let Ok(Some(hermes_client::Inbound::Event(event))) = next
                    && event.raw["type"] == "approval.request"
                    && asked.is_none()
                {
                    let payload = &event.raw["payload"];
                    prompt_id = payload["prompt_id"]
                        .as_str()
                        .or_else(|| event.raw["prompt_id"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    generation = payload["execution_generation"]
                        .as_i64()
                        .or_else(|| event.raw["execution_generation"].as_i64())
                        .unwrap_or(1);
                    asked = Some(Instant::now());
                }
            }
            let (Some(asked), Some(opened_at)) = (asked, opened) else {
                stop_watch(&mut child);
                return Err(
                    "no approval appeared; check that fixture's approval mode is manual".into(),
                );
            };
            if prompt_id.is_empty() {
                stop_watch(&mut child);
                return Err(
                    "no approval appeared; check that fixture's approval mode is manual".into(),
                );
            }
            needs.push(opened_at.saturating_duration_since(asked).as_millis());
            client
                .call_raw(
                    "approval.respond",
                    json!({
                        "session_id": session_id,
                        "prompt_id": prompt_id,
                        "choice": "deny",
                        "execution_generation": generation,
                    }),
                    CallOptions {
                        timeout: Some(Duration::from_secs(20)),
                    },
                )
                .await
                .map_err(|error| format!("approval.respond failed: {error}"))?;
            let reply_at = Instant::now();
            let mut closed_at = None;
            while reply_at.elapsed() < Duration::from_secs(60) {
                if let Some(value) = poll_json(&lines) {
                    if value["kind"] == "need_closed" {
                        closed_at = Some(Instant::now());
                        break;
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
            let Some(closed_at) = closed_at else {
                stop_watch(&mut child);
                return Err("needs-you latency above budget".into());
            };
            closed.push(closed_at.saturating_duration_since(reply_at).as_millis());
            let settle = Instant::now();
            let mut left = false;
            while settle.elapsed() < Duration::from_secs(60) {
                if let Some(value) = poll_json(&lines) {
                    if value["kind"] == "row"
                        && value["row"]["session_id"] == session_id.as_str()
                        && value["row"]["state"] != "working"
                        && value["row"]["state"] != "needs_you"
                    {
                        left = true;
                        break;
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
            if !left {
                stop_watch(&mut child);
                return Err("the turn did not leave working".into());
            }
            println!("needs_you_latency_ms={}", needs.last().unwrap_or(&0));
            println!("closed_latency_ms={}", closed.last().unwrap_or(&0));
        }
        stop_watch(&mut child);
        let p95 = needs.iter().copied().max().unwrap_or(u128::MAX);
        println!("needs_you_p95_ms={p95}");
        if needs.len() != 3 || closed.iter().any(|ms| *ms > 5000) || p95 > 5000 {
            return Err("needs-you latency above budget".into());
        }
        Ok(())
    }
    .await;
    finish(result);
}

#[tokio::test]
#[ignore]
async fn live_finished_turn_outcome() {
    let (home, client, mut inbound) = match attach_fixture().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let result = async {
        let folder = tempfile::tempdir().map_err(|error| error.to_string())?;
        let session_id = create_session(&client, folder.path(), "done").await?;
        let (mut child, lines) = spawn_watch(&home, "180")?;
        // The watch has seen the session before the turn starts: a turn shorter than the
        // watch's start-up would otherwise end before its first read.
        let opened = Instant::now();
        let mut watching = false;
        while opened.elapsed() < Duration::from_secs(20) && !watching {
            match poll_json(&lines) {
                Some(value) => watching = value["kind"] == "snapshot",
                None => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
        if !watching {
            stop_watch(&mut child);
            return Err("the watch printed no first snapshot".into());
        }
        client
            .call_raw(
                "prompt.submit",
                json!({
                    "session_id": session_id,
                    "text": "Reply with the single word: ready",
                    "submission_id": "fleet-ready",
                }),
                CallOptions {
                    timeout: Some(Duration::from_secs(20)),
                },
            )
            .await
            .map_err(|error| format!("prompt.submit failed: {error}"))?;
        let started = Instant::now();
        let mut saw_done = false;
        let mut saw_attention = false;
        while started.elapsed() < Duration::from_secs(90) && !(saw_done && saw_attention) {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                break;
            }
            if let Some(value) = poll_json(&lines) {
                if value["kind"] == "row"
                    && value["row"]["session_id"] == session_id.as_str()
                    && value["row"]["state"] == "done"
                    && value["row"]["outcome"] == "completed"
                {
                    saw_done = true;
                }
                if value["kind"] == "attention" && value["alert"] == "done" {
                    saw_attention = true;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        if !saw_done || !saw_attention {
            stop_watch(&mut child);
            return Err("the finished turn was not reported done".into());
        }
        // The first turn's events still wait on this connection; drop them so the delta read
        // below belongs to the second turn.
        let mut first_generation = 0_i64;
        while let Ok(Some(item)) =
            tokio::time::timeout(Duration::from_millis(300), inbound.next()).await
        {
            if let hermes_client::Inbound::Event(event) = item
                && let Some(value) = event.raw["execution_generation"].as_i64()
            {
                first_generation = first_generation.max(value);
            }
        }
        client
            .call_raw(
                "prompt.submit",
                json!({
                    "session_id": session_id,
                    // A fast model finishes fifty numbers before the interrupt lands; the
                    // recorder's interrupted scenario uses this longer turn.
                    "text": "Count from 1 to 500, one number per line, and do not stop early.",
                    "submission_id": "fleet-count",
                }),
                CallOptions {
                    timeout: Some(Duration::from_secs(20)),
                },
            )
            .await
            .map_err(|error| format!("prompt.submit failed: {error}"))?;
        let mut generation = 1_i64;
        let mut saw_delta = false;
        // Deltas of the first turn may still wait on this connection: only a delta after the
        // second turn's message.start belongs to the turn to interrupt.
        let mut started_second = false;
        let delta_started = Instant::now();
        while delta_started.elapsed() < Duration::from_secs(45) {
            let next = tokio::time::timeout(Duration::from_millis(200), inbound.next()).await;
            if let Ok(Some(hermes_client::Inbound::Event(event))) = next {
                if event.raw["type"] == "message.start"
                    && event.raw["execution_generation"]
                        .as_i64()
                        .is_none_or(|value| value > first_generation)
                {
                    started_second = true;
                    if let Some(value) = event.raw["execution_generation"].as_i64() {
                        generation = value;
                    }
                } else if event.raw["type"] == "message.delta" && started_second {
                    saw_delta = true;
                    if let Some(value) = event.raw["execution_generation"].as_i64() {
                        generation = value;
                    }
                    break;
                }
            }
        }
        if !saw_delta {
            stop_watch(&mut child);
            return Err("the slow turn produced no delta to interrupt".into());
        }
        client
            .call_raw(
                "session.interrupt",
                json!({
                    "session_id": session_id,
                    "execution_generation": generation,
                }),
                CallOptions {
                    timeout: Some(Duration::from_secs(20)),
                },
            )
            .await
            .map_err(|error| format!("session.interrupt failed: {error}"))?;
        let mut interrupted = false;
        let interrupt_started = Instant::now();
        while interrupt_started.elapsed() < Duration::from_secs(60) {
            if let Some(value) = poll_json(&lines) {
                if value["kind"] == "row" && value["row"]["outcome"] == "interrupted" {
                    interrupted = true;
                    break;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        stop_watch(&mut child);
        if !interrupted {
            return Err("the interrupted turn was not reported interrupted".into());
        }
        Ok(())
    }
    .await;
    finish(result);
}

#[tokio::test]
#[ignore]
async fn live_sessions_changed_sibling_report() {
    let home = hermes_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => {
            skip("no gateway");
            return;
        }
        Err(error) => {
            finish(Err(format!("discovery failed: {error}")));
            return;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip(&reason);
        return;
    }
    let result = async {
        let profile = endpoint
            .resolve_profile("default")
            .map_err(|error| format!("default lookup failed: {error}"))?;
        let host_scope = Scope::host(Purpose::Interactive)
            .map_err(|error| format!("host scope failed: {error}"))?;
        let ticket = mint_ticket(&home, &endpoint, &profile, Purpose::Interactive, host_scope)
            .await
            .map_err(|error| format!("host ticket failed: {error}"))?;
        let connection = connect(&endpoint, ticket)
            .await
            .map_err(|error| format!("host connect failed: {error}"))?;
        let (_host, mut inbound) = Client::start(connection.into_parts());
        let (_home, fixture) = open_fixture().await?;
        let folder = tempfile::tempdir().map_err(|error| error.to_string())?;
        let _session = create_session(&fixture, folder.path(), "sibling").await?;
        let started = Instant::now();
        let mut seen = false;
        while started.elapsed() < Duration::from_secs(6) {
            let next = tokio::time::timeout(Duration::from_millis(200), inbound.next()).await;
            if let Ok(Some(hermes_client::Inbound::Event(event))) = next
                && event.raw["type"] == "sessions.changed"
            {
                seen = true;
                break;
            }
        }
        if seen {
            println!("SESSIONS_CHANGED_SIBLING: seen");
        } else {
            println!("SESSIONS_CHANGED_SIBLING: not_seen");
        }
        Ok(())
    }
    .await;
    finish(result);
}
