#[allow(dead_code)]
#[path = "../../../talaria-workflow/tests/runner_support/mod.rs"]
mod folders;
use crate::mock_gateway;
use crate::support;

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
/// The limit is generous on purpose: these tests start real processes on what may be a loaded
/// machine.
fn wait_for(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(60);
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
        include_str!("../data/runner/golden/finished.jsonl")
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
    let expected = include_str!("../data/runner/golden/events.jsonl")
        .replace("\"<RUN_DIR>\"", &serde_json::to_string(&run_dir).unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn a_draft_with_the_workflow_extension_runs() {
    let temp = tempfile::tempdir().unwrap();
    let draft = temp.path().join("drafts/fixture-draft.workflow");
    fs::create_dir_all(draft.parent().unwrap()).unwrap();
    fs::copy(fixture("scripts/slow.js"), &draft).unwrap();
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
            .arg(temp.path().join("runs"))
            .arg(&draft),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).lines().any(|line| {
            serde_json::from_str::<Value>(line)
                .is_ok_and(|v| v["event"] == "run_finished" && v["outcome"] == "completed")
        }),
        "{}",
        text(&out.stdout)
    );
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
    let expected = include_str!("../data/runner/golden/list.txt")
        .replace("{runs_dir}", &runs.to_string_lossy());
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
            "cache_read_tokens",
            "cache_unknown_turns",
            "money_spent_usd",
            "run_dir",
            "warning",
            "failed_agents",
            "cut_short_agents",
            "uncommitted_edits",
            "saved_branches",
            "phases_not_started",
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
        assert_eq!(resumed.status.code(), Some(if graceful { 5 } else { 0 }), "{}", text(&resumed.stderr));
        let after = records(&folder);
        assert_eq!(
            after
                .iter()
                .filter(|r| r["t"] == "call_intent" && r["key"] == first)
                .count(),
            1
        );
        // The interrupt settled the second agent as cancelled, so a resume finishes it as failed
        // and the run says it completed with a failure. After a hard kill nothing settled, and
        // the agent runs again to a plain completion.
        let expected = if graceful { "completed_with_failures" } else { "completed" };
        assert!(
            after
                .iter()
                .any(|r| r["t"] == "run_finished" && r["outcome"] == expected),
            "graceful={graceful}"
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
    // The view learns that its pause was written from a message that arrives after the file
    // exists. A second key before that message asks to pause again, not to resume, so wait for
    // the view to say so.
    wait_for(|| {
        let screen = terminal.screen_text();
        screen.contains("pausing") || screen.contains("paused")
    });
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
fn budget_exhausted_run_reports_its_reason_and_exit_code() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let run = |json: bool| {
        let mut cmd = command(temp.path(), "foreground-wire.json");
        cmd.args(["workflow", "run", "--profile", "fixture", "--budget-tokens", "10"]);
        if json {
            cmd.arg("--json");
        }
        output(cmd.arg("--runs-dir").arg(&runs).arg(fixture("scripts/budget.js")))
    };
    let out = run(true);
    assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
    let finished: Value = text(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "budget_exhausted");
    assert_eq!(finished["result"], json!(["READY", "BudgetExhaustedError"]));
    assert_eq!(finished["reason"], "token budget exhausted (spent 10 of 10)");
    assert_eq!(finished["summary"]["refused"], 1);
    assert!(
        text(&out.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .any(|v| v["event"] == "agent_finished" && v["label"] == "again" && v["state"] == "refused")
    );

    let out = run(false);
    assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains(
            "Budget exhausted: token budget exhausted (spent 10 of 10). Agents refused: 1, stopped: 0.\nResult:\n"
        ),
        "{}",
        text(&out.stdout)
    );
}

fn run_finished(stdout: &[u8]) -> Value {
    text(stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["event"] == "run_finished")
        .expect("no run_finished line")
}

#[test]
fn a_run_reports_cache_reads_as_their_own_number_live_listed_and_replayed() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "cache-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/one.js")),
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let finished = run_finished(&out.stdout);
    // The recorded turn used 10 tokens, 6 of them cache reads: the budget sees the other 4.
    let spend = finished["spend"].clone();
    assert_eq!(spend["tokens"], 4, "{spend}");
    assert_eq!(spend["cache_read_tokens"], 6, "{spend}");
    assert_eq!(spend["cache_unknown_turns"], 0, "{spend}");
    let shown = text(&out.stdout);
    let last = shown.lines().last().unwrap();
    assert!(last.contains("4 tokens + 6 cache reads,"), "{last}");
    let id = finished["run_id"].as_str().unwrap().to_owned();
    let listed = output(
        command(temp.path(), "cache-wire.json")
            .args(["workflow", "list", "--json", "--runs-dir"])
            .arg(&runs),
    );
    let rows: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(rows[0]["tokens_spent"], 4);
    assert_eq!(rows[0]["cache_read_tokens"], 6);
    assert_eq!(rows[0]["cache_unknown_turns"], 0);
    let plain = text(
        &output(
            command(temp.path(), "cache-wire.json")
                .args(["workflow", "list", "--runs-dir"])
                .arg(&runs),
        )
        .stdout,
    );
    assert!(plain.contains("4 tokens + 6 cache reads"), "{plain}");
    // Resuming a finished run rebuilds its spend from the journal and gets the same split.
    let replay = output(
        command(temp.path(), "cache-wire.json")
            .env_remove("TALARIA_WORKFLOW_MOCK")
            .args(["workflow", "resume", "--json", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    assert_eq!(replay.status.code(), Some(0), "{}", text(&replay.stderr));
    assert_eq!(run_finished(&replay.stdout)["spend"]["tokens"], 4);
    assert_eq!(run_finished(&replay.stdout)["spend"]["cache_read_tokens"], 6);
}

#[test]
fn json_mode_ends_with_a_short_line_naming_the_run_and_how_to_read_it_again() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let run = |args: &[&str], target: &Path| {
        output(
            command(temp.path(), "failed-wire.json")
                .args(args)
                .arg("--runs-dir")
                .arg(&runs)
                .arg(target),
        )
    };
    let out = run(&["workflow", "run", "--json", "--profile", "fixture"], &fixture("scripts/one.js"));
    // The run used a custom runs folder, so the printed command names it. The same folder can be
    // spelled two ways (/var and /private/var on macOS), so compare the resolved folders.
    let named = |line: &str| -> PathBuf {
        let rest = line.split("--runs-dir ").nth(1).expect("--runs-dir in the hint");
        fs::canonicalize(rest.split(' ').next().unwrap()).unwrap()
    };
    let resolved_runs = fs::canonicalize(&runs).unwrap();
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    let lines: Vec<&str> = shown.lines().collect();
    let last = *lines.last().unwrap();
    // The run_finished object is unchanged and is the last JSON object; the last line is plain text.
    let finished = run_finished(&out.stdout);
    let id = finished["run_id"].as_str().unwrap().to_owned();
    assert!(serde_json::from_str::<Value>(last).is_err(), "{last}");
    let before = lines[lines.len() - 2];
    assert_eq!(serde_json::from_str::<Value>(before).unwrap(), finished);
    // Short, even with a long --runs-dir path and the problems the run left.
    assert!(last.len() < 340, "{last}");
    // The recording gives no cache read total, so the line says they are unknown for that turn.
    assert!(
        last.contains("10 tokens (cache reads unknown for 1 turn, counted in full)"),
        "{last}"
    );
    for part in [
        id.as_str(),
        "completed_with_failures",
        &format!("{} tokens", finished["spend"]["tokens"]),
        &format!("{} turn", finished["spend"]["cache_unknown_turns"]),
        "talaria workflow result --json --runs-dir ",
    ] {
        assert!(last.contains(part), "{part} missing from {last}");
    }
    assert_eq!(named(last), resolved_runs, "{last}");
    // With the default runs folder the command stays as short as it was.
    let default_run = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture"])
            .arg(fixture("scripts/one.js")),
    );
    let default_text = text(&default_run.stdout);
    let default_last = default_text.lines().last().unwrap();
    let default_id = run_finished(&default_run.stdout)["run_id"].as_str().unwrap().to_owned();
    assert!(
        default_last.ends_with(&format!("Full result: talaria workflow result --json {default_id}")),
        "{default_last}"
    );
    // A replay prints the same shape, and the plain-text mode adds nothing.
    let replay = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "resume", "--json", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    let replay_last = text(&replay.stdout).lines().last().unwrap().to_owned();
    assert!(replay_last.ends_with(&format!(" {id}")), "{replay_last}");
    assert_eq!(named(&replay_last), resolved_runs, "{replay_last}");
    let plain = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "resume", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    assert!(!text(&plain.stdout).contains("Full result:"), "{}", text(&plain.stdout));
}

#[test]
fn a_failed_agent_ends_the_run_completed_with_failures_and_is_listed_first() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let run = |json: bool| {
        let mut cmd = command(temp.path(), "failed-wire.json");
        cmd.args(["workflow", "run", "--profile", "fixture"]);
        if json {
            cmd.arg("--json");
        }
        output(cmd.arg("--runs-dir").arg(&runs).arg(fixture("scripts/one.js")))
    };
    let out = run(true);
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let finished = run_finished(&out.stdout);
    assert_eq!(finished["outcome"], "completed_with_failures");
    assert_eq!(finished["reason"], "1 agent failed");
    let reason = "agent failed: The provider refused this request and nothing more can be done. (provider_rejected)";
    assert_eq!(finished["failed_agents"], json!([{"label": "review", "reason": reason}]));
    // The script finished and kept what it made of the failure.
    assert_eq!(finished["result"]["first"], Value::Null);
    // The failed agents come before the result in the printed line.
    let line = text(&out.stdout)
        .lines()
        .find(|l| l.contains("\"run_finished\""))
        .unwrap()
        .to_owned();
    assert!(line.find("failed_agents").unwrap() < line.find("\"result\"").unwrap(), "{line}");
    let id = finished["run_id"].as_str().unwrap().to_owned();
    // The closing line names the failure before the hint, where a short tail still shows it.
    let closing = text(&out.stdout).lines().last().unwrap().to_owned();
    assert!(
        closing.contains(" Attention: 1 agent failed. Full result: "),
        "{closing}"
    );

    // Listed with its own status, and readable again without paid work.
    let listed = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "list", "--json", "--runs-dir"])
            .arg(&runs),
    );
    let rows: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(rows[0]["status"], "completed_with_failures");
    assert_eq!(rows[0]["failed_agents"], 1, "{rows}");
    assert_eq!(rows[0]["cut_short_agents"], 0, "{rows}");
    // The text listing says it in words; only JSON keeps the underscores.
    let plain_list = text(
        &output(
            command(temp.path(), "failed-wire.json")
                .args(["workflow", "list", "--runs-dir"])
                .arg(&runs),
        )
        .stdout,
    );
    assert!(plain_list.contains("  completed with failures  agents "), "{plain_list}");
    assert!(plain_list.contains("  failed 1  "), "{plain_list}");
    assert!(!plain_list.contains("completed_with_failures"), "{plain_list}");
    assert!(
        plain_list.contains("10 tokens (cache reads unknown for 1 turn, counted in full)"),
        "{plain_list}"
    );
    let replay = output(
        command(temp.path(), "failed-wire.json")
            .env_remove("TALARIA_WORKFLOW_MOCK")
            .args(["workflow", "resume", "--json", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    assert_eq!(replay.status.code(), Some(5), "{}", text(&replay.stderr));
    assert!(!text(&replay.stderr).contains("budget ran out"), "{}", text(&replay.stderr));
    let again = run_finished(&replay.stdout);
    assert_eq!(again["outcome"], "completed_with_failures");
    assert_eq!(again["failed_agents"], finished["failed_agents"]);
    assert_eq!(again["result"], finished["result"]);

    let plain = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "resume", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    let shown = text(&plain.stdout);
    assert!(
        shown.contains(&format!(
            "Completed with failures: 1 agent failed.\nFailed agents:\n  review: {reason}\n"
        )),
        "{shown}"
    );
    // The agent ran without isolation, so the report says nothing was saved and claims no edit.
    assert!(
        shown.contains("Folders agents ran in without worktree isolation (anything they changed is already there; nothing was saved):\n"),
        "{shown}"
    );
    assert!(shown.contains(": review\nResult:\n"), "{shown}");
}

#[test]
fn work_found_on_a_moved_route_is_printed_live_and_listed_before_the_result() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "moved-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/one.js")),
    );
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    let note = "runner: untracked work: ";
    // Printed live, as a log event, when the sweep wrote it.
    let live = shown
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["event"] == "log" && v["message"].as_str().is_some_and(|m| m.starts_with(note)))
        .expect("no live route note");
    let finished = run_finished(&out.stdout);
    let listed = finished["untracked_work"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{finished}");
    let entry = listed[0].as_str().unwrap();
    assert_eq!(format!("{note}{entry}"), live["message"].as_str().unwrap());
    for part in ["agent review", "session session", "new stored session session-2", "cancelled 1 turn(s)"] {
        assert!(entry.contains(part), "{part} missing from {entry}");
    }
    // The failed agent ran without isolation: its folder is named as one it ran in.
    let leftovers = finished["leftover_edits"].as_array().unwrap();
    assert!(leftovers.iter().all(|f| f["in_place"] == true), "{finished}");
    // After the failed agents, before the result.
    let line = shown.lines().find(|l| l.contains("\"run_finished\"")).unwrap().to_owned();
    assert!(line.find("failed_agents").unwrap() < line.find("untracked_work").unwrap(), "{line}");
    assert!(line.find("untracked_work").unwrap() < line.find("\"result\"").unwrap(), "{line}");
    // The plain text names it too, and so does a replay.
    let id = finished["run_id"].as_str().unwrap().to_owned();
    let plain = output(
        command(temp.path(), "moved-wire.json")
            .args(["workflow", "resume", "--runs-dir"])
            .arg(&runs)
            .arg(&id),
    );
    let shown = text(&plain.stdout);
    assert!(shown.contains("Untracked work found on agent routes:\n  agent review"), "{shown}");
    assert!(shown.find("Untracked work").unwrap() < shown.find("Result:").unwrap(), "{shown}");
}

#[test]
fn a_route_that_could_not_be_checked_is_not_listed_as_untracked_work() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    // This wire has no answer for the sweep's session.resume.
    let out = output(
        command(temp.path(), "failed-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/one.js")),
    );
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let finished = run_finished(&out.stdout);
    assert!(finished.get("untracked_work").is_none(), "{finished}");
    assert!(
        text(&out.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .any(|v| v["event"] == "log"
                && v["message"].as_str().is_some_and(|m| m.starts_with("runner: route not checked: ")))
    );
}

#[test]
fn an_agent_stopped_at_its_step_cap_keeps_its_reply_and_is_marked_everywhere() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "capped-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/one.js")),
    );
    // The run is a plain success: the reply was kept.
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let finished = run_finished(&out.stdout);
    assert_eq!(finished["outcome"], "completed");
    // The script received the reply, and could see the mark through cutShort().
    assert_eq!(finished["result"]["first"], "PARTIAL ANSWER");
    assert_eq!(
        finished["result"]["capped"],
        json!([{"label": "review", "steps": 60, "limit": 60}])
    );
    assert_eq!(
        finished["cut_short_agents"],
        json!([{"label": "review", "reason": "cut short (60 of 60 steps)"}])
    );
    assert!(finished.get("failed_agents").is_none());
    // The event that ends the agent says so, and so does the journal record.
    let agent_state = text(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["event"] == "agent_finished")
        .map(|v| v["state"].clone());
    assert_eq!(agent_state, Some(json!("cut short")));
    let folder = fs::read_dir(&runs).unwrap().next().unwrap().unwrap().path();
    let finished_call = records(&folder)
        .into_iter()
        .find(|r| r["t"] == "call_finished")
        .unwrap();
    assert_eq!(finished_call["outcome"]["kind"], "completed");
    assert_eq!(finished_call["outcome"]["cut_short"], json!({"steps": 60, "limit": 60}));
    let engine_finished = records(&folder)
        .into_iter()
        .find(|r| r["t"] == "engine" && r["event"]["event"] == "agent_finished")
        .unwrap();
    assert_eq!(engine_finished["event"]["outcome"]["kind"], "cut_short");

    let plain = output(
        command(temp.path(), "capped-wire.json")
            .args(["workflow", "resume", "--runs-dir"])
            .arg(&runs)
            .arg(folder.file_name().unwrap()),
    );
    assert_eq!(plain.status.code(), Some(0));
    assert!(
        text(&plain.stdout).contains("Cut short agents:\n  review: cut short (60 of 60 steps)\nFolders agents ran in without worktree isolation"),
        "{}",
        text(&plain.stdout)
    );
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

#[path = "../workflow_builtins_cli.rs"]
mod builtins_contract;

fn run_in(temp: &Path, cwd: &Path, runs: &str) -> Output {
    output(
        command(temp, "events-wire.json")
            .args(["workflow", "run", "--json", "--profile", "fixture", "--cwd"])
            .arg(cwd)
            .arg("--runs-dir")
            .arg(temp.join(runs))
            .arg(fixture("scripts/events.js")),
    )
}

#[test]
fn a_run_outside_a_git_repository_warns_but_still_runs() {
    let temp = tempfile::tempdir().unwrap();
    let plain = temp.path().join("new-project");
    fs::create_dir_all(&plain).unwrap();
    let out = run_in(temp.path(), &plain, "plain-runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stderr = text(&out.stderr);
    assert!(stderr.contains("is not inside a git repository"), "{stderr}");
    assert!(stderr.contains("isolation: 'worktree' will fail"), "{stderr}");
    assert!(stderr.contains("git init"), "{stderr}");
    assert!(
        text(&out.stdout).contains("\"event\":\"run_finished\""),
        "the warning must not stop the run"
    );

    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("sub")).unwrap();
    let out = run_in(temp.path(), &repo.join("sub"), "repo-runs");
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!text(&out.stderr).contains("not inside a git repository"));
}

fn json_line(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).unwrap_or_else(|_| panic!("not one JSON line: {}", text(stdout)))
}

#[test]
fn restart_runs_only_the_failed_agent_again_and_kept_values_are_readable_by_label() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let out = output(
        command(temp.path(), "restart-run-wire.json")
            .args(["workflow", "run", "--profile", "fixture", "--json", "--runs-dir"])
            .arg(&runs)
            .arg(fixture("scripts/restart.js")),
    );
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let finished = run_finished(&out.stdout);
    assert_eq!(finished["failed_agents"][0]["label"], "review");
    let id = finished["run_id"].as_str().unwrap().to_owned();
    let folder = runs.join(&id);
    let workflow = |args: &[&str]| {
        output(
            command(temp.path(), "restart-resume-wire.json")
                .arg("workflow")
                .args(args)
                .arg("--runs-dir")
                .arg(&runs),
        )
    };

    // A finished agent's kept value is readable by its label, without the journal by hand.
    let plan = workflow(&["result", &id, "--agent", "plan", "--json"]);
    assert_eq!(plan.status.code(), Some(0), "{}", text(&plan.stderr));
    let plan = json_line(&plan.stdout);
    assert_eq!(
        (plan["state"].clone(), plan["value"].clone()),
        (json!("completed"), json!("PLAN READY"))
    );
    let review = workflow(&["result", &id, "--agent", "review", "--json"]);
    assert_eq!(review.status.code(), Some(1));
    let review = json_line(&review.stdout);
    assert_eq!(review["state"], "failed");
    assert!(review["reason"].as_str().unwrap().starts_with("agent failed:"));
    assert!(review.get("value").is_none());
    let unknown = workflow(&["result", &id, "--agent", "nobody"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(text(&unknown.stderr).contains("no agent labelled or keyed 'nobody'"));

    // A finished agent is refused: running it again would pay for its work twice.
    let kept = workflow(&["restart", &id, "plan"]);
    assert_eq!(kept.status.code(), Some(2));
    assert!(
        text(&kept.stderr).contains("finished agents are kept — restart would pay for the work twice"),
        "{}",
        text(&kept.stderr)
    );
    // The failed one is asked for, and the caller is told to resume next.
    let asked = workflow(&["restart", &id, "review", "--json"]);
    assert_eq!(asked.status.code(), Some(0), "{}", text(&asked.stderr));
    let asked = json_line(&asked.stdout);
    let key = asked["requested"][0]["key"].as_str().unwrap().to_owned();
    assert_eq!(asked["requested"][0]["label"], "review");
    assert_eq!(asked["requested"][0]["written"], true);
    assert_eq!(asked["also_run_again"], json!([]));
    assert!(
        asked["next"]
            .as_str()
            .unwrap()
            .starts_with("talaria workflow resume --runs-dir "),
        "{asked}"
    );
    assert!(asked["next"].as_str().unwrap().ends_with(&id));
    let restart_file = folder.join("control").join(format!("restart-{key}.json"));
    assert!(restart_file.exists());
    // Asking again is not an error; it says the request was already there.
    let again = workflow(&["restart", &id, "--all-failed"]);
    assert_eq!(again.status.code(), Some(0), "{}", text(&again.stderr));
    assert!(
        text(&again.stdout).contains("restart already requested for agent review"),
        "{}",
        text(&again.stdout)
    );

    // The resume runs the failed agent again. The finished one is replayed from the journal:
    // the resume's scripted gateway serves exactly one new session, so a second call for it
    // would fail the run.
    let resumed = workflow(&["resume", "--json", &id]);
    assert_eq!(resumed.status.code(), Some(0), "{}", text(&resumed.stderr));
    let finished = run_finished(&resumed.stdout);
    assert_eq!(finished["outcome"], "completed");
    assert_eq!(
        finished["result"],
        json!({"plan": "PLAN READY", "review": "REVIEW FIXED"})
    );
    assert!(!restart_file.exists());
    let journal = records(&folder);
    let intents = |label: &str| {
        journal
            .iter()
            .filter(|r| r["t"] == "call_intent" && r["label"] == label)
            .map(|r| r["attempt"].as_u64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(intents("plan"), [0]);
    assert_eq!(intents("review"), [0, 1]);
    let fixed = workflow(&["result", &id, "--agent", "review"]);
    assert_eq!(fixed.status.code(), Some(0));
    assert_eq!(text(&fixed.stdout), "\"REVIEW FIXED\"\n");
    // Nothing failed any more.
    let none = workflow(&["restart", &id, "--all-failed"]);
    assert_eq!(none.status.code(), Some(2));
    assert!(text(&none.stderr).contains("has no failed agents"));
}

#[test]
fn a_stopped_agent_parks_and_tells_the_caller_it_waits_for_an_answer() {
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let child = command(temp.path(), "stopped-wire.json")
        .args(["workflow", "run", "--profile", "fixture", "--json", "--runs-dir"])
        .arg(&runs)
        .arg(fixture("scripts/one.js"))
        .spawn()
        .unwrap();
    let folder = || {
        fs::read_dir(&runs)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.is_dir())
    };
    wait_for(|| folder().is_some_and(|f| records(&f).iter().any(|r| r["t"] == "call_attention")));
    let folder = folder().unwrap();
    let parked = records(&folder)
        .into_iter()
        .find(|r| r["t"] == "call_attention")
        .unwrap();
    assert_eq!(parked["cause"], "stopped");
    let id = folder.file_name().unwrap().to_string_lossy().into_owned();
    let key = parked["key"].as_str().unwrap().to_owned();
    let resolve = |answer: &str| {
        output(
            command(temp.path(), "stopped-wire.json")
                .args(["workflow", "resolve", &id, "--key", &key])
                .args(["--attempt", "0", "--episode", "0", "--resolution", answer])
                .arg("--runs-dir")
                .arg(&runs),
        )
    };
    let answered = resolve("fail");
    assert_eq!(answered.status.code(), Some(0), "{}", text(&answered.stderr));
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(5), "{}", text(&out.stderr));
    let lines: Vec<Value> = text(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let event = lines.iter().find(|l| l["event"] == "parked").expect("a parked line");
    assert_eq!(event["cause"], "stopped");
    assert_eq!(event["waiting_for"], "operator");
    assert_eq!(event["choices"], json!(["fail", "rerun", "abort", "use-reply"]));
    assert!(
        event["answer"].as_str().unwrap().starts_with(&format!(
            "talaria workflow resolve {id} --runs-dir "
        )),
        "{event}"
    );
    assert!(event["answer"].as_str().unwrap().ends_with(&format!(
        "--key {key} --attempt 0 --episode 0 --resolution <choice>"
    )));
    assert!(event["use_reply"].as_str().unwrap().contains("continue"));
    let reason = event["reason"].as_str().unwrap();
    assert!(reason.contains("a stop was requested"), "{reason}");
    assert!(reason.contains("not a timeout"), "{reason}");
    assert!(reason.contains("80.0s elapsed"), "{reason}");
    let finished = run_finished(&out.stdout);
    assert!(
        finished["failed_agents"][0]["reason"]
            .as_str()
            .unwrap()
            .starts_with("operator marked a stopped agent as failed: agent stopped:"),
        "{finished}"
    );
}
