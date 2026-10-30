//! Terminal-agent diagnostics and fleet output.

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use talaria_app::fleet::{FleetLinkState, FleetSnapshot};
use talaria_app::side_connection::SideScope;
use talaria_core::fleet::{
    FleetRegistry,
    herdr::{HerdrPane, project},
};

fn mock_binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("TALARIA_MOCK_HERDR_BIN") {
                return path.into();
            }
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
            let binary = target.join("debug/mock-herdr");
            if !binary.is_file() {
                let status = Command::new("cargo")
                    .args(["build", "-p", "talaria-app", "--bin", "mock-herdr"])
                    .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
                    .status()
                    .expect("build mock");
                assert!(status.success(), "mock build");
            }
            binary
        })
        .clone()
}
fn run(root: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("HOME", root)
        .env("HERMES_HOME", root)
        .env("TALARIA_MOCK_HERDR_STATE", root.join("state.json"))
        .env("TALARIA_MOCK_HERDR_LOG", root.join("log.jsonl"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("talaria command");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().expect("status").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("command timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().expect("output")
}
fn prepare(root: &Path) {
    std::fs::create_dir(root.join(".talaria")).expect("configuration directory");
    let binary = serde_json::to_string(&mock_binary().to_string_lossy()).expect("binary path");
    std::fs::write(
        root.join(".talaria/config.toml"),
        format!("[herdr]\nenabled = true\nbinary = {binary}\npoll_interval_ms = 100\n"),
    )
    .expect("config");
    std::fs::write(root.join("state.json"),json!({"agents":[{"pane_id":"wfixture:p1","workspace_id":"wfixture","tab_id":"wfixture:t1","agent":"codex","name":"fixture","agent_status":"working","terminal_title_stripped":"Task"}],"visible":{"wfixture:p1":"hello"}}).to_string()).expect("mock state");
}
fn calls(root: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(root.join("log.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let v: Value = serde_json::from_str(line).ok()?;
            Some(
                v["argv"]
                    .as_array()?
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn fleet_list_json_includes_herdr_backend() {
    let pane = HerdrPane {
        pane_id: "wfixture:p1".into(),
        workspace_id: "wfixture".into(),
        tab_id: "wfixture:t1".into(),
        agent: "codex".into(),
        name: "fixture".into(),
        status: "working".into(),
        title: "Task".into(),
    };
    let mut registry = FleetRegistry::new();
    registry.observe_herdr(project(&pane, "hello", 1.0));
    let snapshot = FleetSnapshot {
        version: 1,
        scope: SideScope::Host { profiles: vec![] },
        link: FleetLinkState::Connected,
        profiles: vec![],
        rows: registry.rows().into_iter().cloned().collect(),
        queue: vec![],
        counts: registry.counts(),
        diagnostics: vec![],
        as_of: 1.0,
    };
    let json = talaria_cli::fleet_cmd::snapshot_json(&snapshot);
    assert_eq!(json["rows"][0]["backend"], "herdr");
    assert_eq!(json["rows"][0]["agent"], "codex");
    assert!(talaria_cli::fleet_cmd::render_text(&snapshot, None, 1.0).contains("[herdr codex]"));
}

#[test]
fn panes_diagnostics_and_probe_are_read_only() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    prepare(tmp.path());
    for args in [&["herdr", "panes"][..], &["herdr", "panes", "--probe"][..]] {
        let output = run(tmp.path(), args);
        assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        for part in ["wfixture:p1", "codex", "working"] {
            assert!(stdout.contains(part));
        }
    }
    let calls = calls(tmp.path());
    assert!(calls.iter().any(|a| a == &["agent", "list"]));
    assert!(!calls.iter().any(|a| {
        a.get(1)
            .is_some_and(|part| matches!(part.as_str(), "focus" | "prompt" | "send-keys"))
    }));
}

#[test]
fn fleet_list_command_includes_live_terminal_agents() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    prepare(tmp.path());
    let gateway = mock_gateway::StandaloneGateway::start(tmp.path());
    gateway.set_fleet_profiles(vec!["fixture".into()]);
    gateway.set_fleet_session(
        "fixture",
        json!({"id":"session-1","title":"Hermes","status":"idle","message_count":0}),
    );
    let output = run(tmp.path(), &["fleet", "list", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("fleet snapshot");
    let rows = value["rows"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|row| row["backend"] == "herdr" && row["session_id"] == "wfixture:p1")
    );
    assert!(
        rows.iter()
            .any(|row| row["backend"] == "hermes" && row["session_id"] == "session-1")
    );
}
