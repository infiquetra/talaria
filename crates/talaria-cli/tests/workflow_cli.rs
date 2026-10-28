#![cfg(feature = "workflow-mock")]
#[allow(dead_code)]
#[path = "../../talaria-workflow/tests/runner_support/mod.rs"]
mod folders;
#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;
mod support;

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/runner")
        .join(name)
}
fn command(home: &Path, wire: &str) -> Command {
    fs::create_dir_all(home.join("empty-hermes")).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_talaria"));
    cmd.env_clear()
        .env("HOME", home)
        .env("HERMES_HOME", home.join("empty-hermes"))
        .env("TALARIA_CONFIG_DIR", home.join("config"))
        .env("TALARIA_WORKFLOW_MOCK", "1")
        .env("TALARIA_WORKFLOW_MOCK_WIRE", fixture(wire))
        .env("TALARIA_WORKFLOW_MOCK_ROSTER", fixture("roster.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}
fn output(cmd: &mut Command) -> Output {
    cmd.output().unwrap()
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < until, "condition timed out");
        thread::sleep(Duration::from_millis(20));
    }
}
fn records(dir: &Path) -> Vec<Value> {
    fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn detach_outlives_parent() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let source = fixture("scripts/slow.js");
    let started = Instant::now();
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args([
                "workflow",
                "run",
                "--detach",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&runs)
            .arg(&source),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(started.elapsed() < Duration::from_secs(3));
    let row: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(row["detached"], true);
    let id = row["run_id"].as_str().unwrap();
    assert_eq!(id.len(), 26);
    let folder = PathBuf::from(row["run_dir"].as_str().unwrap());
    assert!(folder.starts_with(&runs));
    wait_for(|| {
        records(&folder)
            .iter()
            .any(|r| r["t"] == "run_finished" && r["outcome"] == "completed")
    });
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--json", "--runs-dir"])
            .arg(&runs),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows[0]["run_id"], id);
    assert_eq!(rows[0]["status"], "completed");
    assert_eq!(
        fs::read_dir(temp.path().join("empty-hermes"))
            .unwrap()
            .count(),
        0
    );
    let foreground_runs = temp.path().join("foreground-runs");
    let foreground = output(
        command(temp.path(), "foreground-wire.json")
            .args([
                "workflow",
                "run",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&foreground_runs)
            .arg(&source),
    );
    assert!(foreground.status.success(), "{}", text(&foreground.stderr));
    let stdout = text(&foreground.stdout);
    let line = stdout
        .lines()
        .find(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .is_some_and(|v| v["event"] == "run_finished")
        })
        .unwrap();
    let event: Value = serde_json::from_str(line).unwrap();
    let id = event["run_id"].as_str().unwrap();
    let normalized = format!("{}\n", line.replace(id, "<RUN_ID>"));
    assert_eq!(
        normalized,
        include_str!("data/runner/golden/finished.jsonl")
    );
    let event_runs = temp.path().join("event-runs");
    let event_output = output(
        command(temp.path(), "events-wire.json")
            .args([
                "workflow",
                "run",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&event_runs)
            .arg(fixture("scripts/events.js")),
    );
    assert!(
        event_output.status.success(),
        "{}",
        text(&event_output.stderr)
    );
    let stream = text(&event_output.stdout);
    let event_lines: Vec<_> = stream
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            if value.get("event").is_some() {
                Some((line, value))
            } else {
                None
            }
        })
        .collect();
    let started = event_lines
        .iter()
        .find(|(_, value)| value["event"] == "run_started")
        .unwrap();
    let event_id = started.1["run_id"].as_str().unwrap();
    let run_dir = started.1["run_dir"]
        .as_str()
        .unwrap()
        .replace(event_id, "<RUN_ID>");
    let actual = event_lines
        .iter()
        .map(|(line, _)| line.replace(event_id, "<RUN_ID>") + "\n")
        .collect::<String>();
    let expected = include_str!("data/runner/golden/events.jsonl")
        .replace("\"<RUN_DIR>\"", &serde_json::to_string(&run_dir).unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn list_output_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    fs::create_dir(&runs).unwrap();
    let older = folders::id(1);
    let newer = folders::id(2);
    folders::folder(
        &runs,
        &older,
        0,
        &[folders::started(&older), folders::finished("failed")],
    );
    folders::folder(
        &runs,
        &newer,
        1000,
        &[folders::started(&newer), folders::finished("completed")],
    );
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--runs-dir"])
            .arg(&runs),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let actual = text(&out.stdout)
        .replace(&older, "<RUN_ID>")
        .replace(&newer, "<RUN_ID>")
        .replace("1970-01-01T00:00:00Z", "<TIME>")
        .replace("1970-01-01T00:00:01Z", "<TIME>");
    let expected =
        include_str!("data/runner/golden/list.txt").replace("{runs_dir}", &runs.to_string_lossy());
    assert_eq!(actual, expected);
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--json", "--runs-dir"])
            .arg(&runs),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["run_id"], newer);
    assert_eq!(rows[1]["run_id"], older);
    for row in rows.as_array().unwrap() {
        let object = row.as_object().unwrap();
        let fields = [
            "run_id",
            "name",
            "profile",
            "started_at_ms",
            "status",
            "agents_finished",
            "agents_total_known",
            "parked",
            "tokens_spent",
            "money_spent_usd",
            "run_dir",
            "warning",
        ];
        assert_eq!(object.len(), fields.len());
        assert!(fields.iter().all(|field| object.contains_key(*field)));
        assert_eq!(row["warning"], Value::Null);
    }
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--json", "--limit", "1", "--runs-dir"])
            .arg(&runs),
    );
    assert!(out.status.success());
    let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["run_id"], newer);
    let missing = temp.path().join("missing");
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--runs-dir"])
            .arg(&missing),
    );
    assert!(out.status.success());
    assert_eq!(text(&out.stdout), "No workflow runs found.\n");
    assert!(!missing.exists());
    let rejected = temp.path().join("rejected");
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args([
                "workflow",
                "run",
                "--profile",
                "fixture",
                "--args",
                "{invalid",
                "--runs-dir",
            ])
            .arg(&rejected)
            .arg(fixture("scripts/slow.js")),
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(!rejected.exists());
    assert!(text(&out.stderr).to_lowercase().contains("json"));
    let out = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--json", "--limit", "0", "--runs-dir"])
            .arg(&runs),
    );
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!([])
    );
    assert_eq!(
        fs::read_dir(temp.path().join("empty-hermes"))
            .unwrap()
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn ctrl_c_graceful_then_resume_and_hard_kill() {
    use rustix::process::{Pid, Signal, kill_process};
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for graceful in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let runs = temp.path().join("runs");
        let source = fixture("scripts/two.js");
        let child = command(temp.path(), "signal-wire.json")
            .args([
                "workflow",
                "run",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&runs)
            .arg(&source)
            .spawn()
            .unwrap();
        let mut child = ChildGuard(child);
        let mut folder = PathBuf::new();
        wait_for(|| {
            if let Some(status) = child.0.try_wait().unwrap() {
                panic!("runner exited before journal checkpoint: {status}");
            }
            let Ok(mut entries) = fs::read_dir(&runs) else {
                return false;
            };
            let Some(Ok(entry)) = entries.next() else {
                return false;
            };
            folder = entry.path();
            let lines = records(&folder);
            let first = lines
                .iter()
                .find(|r| r["t"] == "call_intent" && r["label"] == "first");
            let second = lines
                .iter()
                .find(|r| r["t"] == "call_intent" && r["label"] == "second");
            first.zip(second).is_some_and(|(a, b)| {
                lines
                    .iter()
                    .any(|r| r["t"] == "call_finished" && r["key"] == a["key"])
                    && lines
                        .iter()
                        .any(|r| r["t"] == "turn_admitted" && r["key"] == b["key"])
            })
        });
        let before = records(&folder);
        let first = before
            .iter()
            .find(|r| r["t"] == "call_intent" && r["label"] == "first")
            .unwrap()["key"]
            .as_str()
            .unwrap()
            .to_owned();
        let pid = Pid::from_raw(i32::try_from(child.0.id()).unwrap()).unwrap();
        kill_process(pid, if graceful { Signal::INT } else { Signal::KILL }).unwrap();
        let status = child.0.wait().unwrap();
        if graceful {
            assert_eq!(status.code(), Some(3));
            assert!(
                records(&folder)
                    .iter()
                    .any(|r| r["t"] == "run_finished" && r["outcome"] == "aborted")
            );
            let outcome: Value =
                serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
            assert_eq!(outcome["reason"], "stopped");
        } else {
            assert!(!status.success());
            assert!(!records(&folder).iter().any(|r| r["t"] == "run_finished"));
            let listed = output(
                command(temp.path(), "resume-wire.json")
                    .args(["workflow", "list", "--json", "--runs-dir"])
                    .arg(&runs),
            );
            assert!(listed.status.success());
            let rows: Value = serde_json::from_slice(&listed.stdout).unwrap();
            assert_eq!(rows[0]["status"], "interrupted");
        }
        let target = folder.file_name().unwrap();
        let resumed = output(
            command(temp.path(), "resume-wire.json")
                .args(["workflow", "resume", "--json", "--runs-dir"])
                .arg(&runs)
                .arg(target),
        );
        assert!(resumed.status.success(), "{}", text(&resumed.stderr));
        let after = records(&folder);
        assert_eq!(
            after
                .iter()
                .filter(|r| r["t"] == "call_intent" && r["key"] == first)
                .count(),
            1
        );
        assert!(
            after
                .iter()
                .any(|r| r["t"] == "run_finished" && r["outcome"] == "completed")
        );
        assert_eq!(
            fs::read_dir(temp.path().join("empty-hermes"))
                .unwrap()
                .count(),
            0
        );
    }
}

struct DetachedGuard(PathBuf);
impl Drop for DetachedGuard {
    fn drop(&mut self) {
        let Ok(bytes) = fs::read(self.0.join("runner.json")) else {
            return;
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return;
        };
        let Some(pid) = value["pid"].as_u64() else {
            return;
        };
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
    }
}

fn terminal_submit(terminal: &mut support::Terminal, text: &str) {
    terminal.type_keys(text);
    thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\r");
}

#[test]
fn progress_detach_keeps_runner_alive() {
    use mock_gateway::{HeadlessScenario, StandaloneGateway};
    use talaria_workflow::{
        journal::JournalReader,
        progress::{RunProgress, RunState},
        runner::{RunStatus, discover_runs},
    };

    let temp = tempfile::tempdir().unwrap();
    let gateway_home = temp.path().join("gateway");
    let runs = temp.path().join("runs");
    let config = temp.path().join("config");
    fs::create_dir(&gateway_home).unwrap();
    let gateway = StandaloneGateway::start(&gateway_home);
    gateway.configure_headless(HeadlessScenario::StartedStop);
    gateway.set_session_create_descriptor(
        &["tool", "tui", "cli"],
        &[
            "request_id",
            "source",
            "model",
            "provider",
            "reasoning",
            "cwd",
            "cwd_explicit",
            "toolsets",
            "skills",
            "max_turns",
        ],
    );
    let roster: Value = serde_json::from_slice(&fs::read(fixture("roster.json")).unwrap()).unwrap();
    gateway.set_model_options(roster.clone(), roster);
    gateway.set_silent("prompt.receipt", true);

    let mut start = Command::new(env!("CARGO_BIN_EXE_talaria"));
    start
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", temp.path())
        .env("HERMES_HOME", &gateway_home)
        .env("TALARIA_CONFIG_DIR", &config)
        .env("TALARIA_RUNS_DIR", &runs)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let result = output(
        start
            .args([
                "workflow",
                "run",
                "--detach",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&runs)
            .arg(fixture("scripts/slow.js")),
    );
    assert!(result.status.success(), "{}", text(&result.stderr));
    let started: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(started["detached"], true);
    let id = started["run_id"].as_str().unwrap().to_owned();
    let folder = PathBuf::from(started["run_dir"].as_str().unwrap());
    let _guard = DetachedGuard(folder.clone());
    assert!(folder.starts_with(&runs));
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    assert!(folder.join("runner.json").is_file());

    let mut terminal = support::Launch::new(temp.path())
        .hermes_home(&gateway_home)
        .answer_queries()
        .size(32, 120)
        .env("TALARIA_CONFIG_DIR", &config.to_string_lossy())
        .env("TALARIA_RUNS_DIR", &runs.to_string_lossy())
        .args(&["--no-ensure", "--profile", "fixture"])
        .spawn();
    assert!(
        terminal.wait_for_text("Type a message", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    wait_for(|| {
        gateway
            .requests("session.create")
            .iter()
            .any(|frame| frame.params["source"] != "tool")
    });
    let creates = gateway.requests("session.create").len();
    let submits = gateway.requests("prompt.submit").len();
    terminal_submit(&mut terminal, "/workflows");
    assert!(
        terminal.wait_for_text(&id, Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("\r");
    assert!(
        terminal.wait_for_text("review", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("\x1b");
    assert!(
        terminal.wait_for_text("Type a message", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    assert!(!records(&folder).iter().any(|r| r["t"] == "run_finished"));
    assert!(folder.join("runner.json").exists());
    let discovered = discover_runs(&runs);
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].run_id, id);
    assert_eq!(discovered[0].status, RunStatus::Running);
    let listed = output(
        command(temp.path(), "slow-wire.json")
            .args(["workflow", "list", "--json", "--runs-dir"])
            .arg(&runs),
    );
    assert!(listed.status.success(), "{}", text(&listed.stderr));
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed[0]["run_id"], id);
    assert_eq!(listed[0]["status"], "running");
    let (journal, _) = JournalReader::read_from(&folder, 0).unwrap();
    let dir = talaria_workflow::journal::RunDir::open(&folder).unwrap();
    assert_eq!(
        RunProgress::from_records(dir.manifest(), &journal)
            .run()
            .state,
        RunState::Running
    );

    terminal_submit(&mut terminal, "/workflows");
    assert!(
        terminal.wait_for_text(&id, Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("\r");
    assert!(
        terminal.wait_for_text("review", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    assert_eq!(gateway.requests("session.create").len(), creates);
    assert_eq!(gateway.requests("prompt.submit").len(), submits);
    terminal.type_keys("p");
    wait_for(|| folder.join("control/pause.json").exists());
    assert!(!records(&folder).iter().any(|r| r["t"] == "run_finished"));
    terminal.type_keys("p");
    wait_for(|| !folder.join("control/pause.json").exists());
    terminal.type_keys("X");
    assert!(
        terminal.wait_for_text("confirm", Duration::from_secs(5)),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("y");
    gateway
        .wait_request("session.interrupt", 0, Duration::from_secs(10))
        .expect("the reopened viewer did not stop the active session");
    gateway.set_silent("prompt.receipt", false);
    wait_for(|| folder.join("outcome.json").exists());
    let outcome: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(outcome["outcome"], "aborted");
    assert_eq!(outcome["reason"], "stopped");
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["t"] == "run_finished" && r["outcome"] == "aborted")
    );
    let discovered = discover_runs(&runs);
    assert_eq!(discovered[0].run_id, id);
    assert_eq!(discovered[0].status, RunStatus::Aborted);
    assert!(
        terminal.wait_for_text("aborted", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("\x1b");
    assert!(
        terminal.wait_for_text("Type a message", Duration::from_secs(10)),
        "{}",
        terminal.screen_text()
    );
    terminal_submit(&mut terminal, "/quit");
    let ended = terminal.wait_exit(Duration::from_secs(5));
    assert!(!ended.timed_out);
}

#[test]
fn completed_replay_records_only_a_header_with_no_gateway() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "foreground-wire.json")
            .args([
                "workflow",
                "run",
                "--json",
                "--profile",
                "fixture",
                "--runs-dir",
            ])
            .arg(&runs)
            .arg(fixture("scripts/slow.js")),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let finished: Value = text(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["event"] == "run_finished")
        .unwrap();
    let record = temp.path().join("replay.jsonl");
    let replay = output(
        command(temp.path(), "foreground-wire.json")
            .env_remove("TALARIA_WORKFLOW_MOCK")
            .args(["workflow", "resume", "--json", "--record"])
            .arg(&record)
            .arg("--runs-dir")
            .arg(&runs)
            .arg(finished["run_id"].as_str().unwrap()),
    );
    assert!(replay.status.success(), "{}", text(&replay.stderr));
    let rows: Vec<Value> = fs::read_to_string(&record)
        .expect("replay recording missing")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "header");
    assert_eq!(
        fs::read_dir(temp.path().join("empty-hermes"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn empty_gateway_home_reports_one_plain_gateway_not_found_line() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "foreground-wire.json")
            .env_remove("TALARIA_WORKFLOW_MOCK")
            .args(["workflow", "run", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/slow.js")),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    let error = text(&out.stderr);
    assert_eq!(error.lines().count(), 1, "{error}");
    assert!(error.contains("gateway not found"), "{error}");
    assert!(!error.contains('\u{1b}'));
    assert!(!runs.exists());
}

#[path = "workflow_builtins_cli.rs"]
mod builtins_contract;
