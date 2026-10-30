//! Isolated local terminal-manager scenarios.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use talaria_app::fleet::{FleetConfig, FleetService};
use talaria_app::herdr::{HerdrConfig, HerdrService};
use talaria_core::fleet::RowKey;

pub fn in_child(name: &str) -> bool {
    std::env::var("TALARIA_HERDR_TEST_CASE").ok().as_deref() == Some(name)
}
pub fn root() -> PathBuf {
    std::env::var_os("TALARIA_HERDR_TEST_ROOT")
        .expect("test root")
        .into()
}
pub fn variant() -> String {
    std::env::var("TALARIA_HERDR_TEST_VARIANT").unwrap_or_default()
}
pub fn key(pane: &str) -> RowKey {
    RowKey {
        profile: format!("herdr:{pane}"),
        session_id: pane.into(),
    }
}

pub fn run_case(name: &str, variant: &str) {
    let tmp = tempfile::tempdir().expect("temporary directory");
    let root = tmp.path();
    let mut agents = Vec::new();
    let mut visible = serde_json::Map::new();
    for (id, workspace, status, agent) in [
        (1, "wfixture", "working", "codex"),
        (2, "wfixture", "blocked", "claude"),
        (3, "wfixture", "idle", "codex"),
        (4, "wfixture", "done", "codex"),
        (5, "wfixture", "unknown", "codex"),
        (6, "wother", "idle", "codex"),
    ] {
        let pane = format!("{workspace}:p{id}");
        agents.push(json!({"pane_id":pane,"workspace_id":workspace,"tab_id":format!("{workspace}:t1"),"agent":agent,"name":"fixture","agent_status":status,"terminal_title_stripped":"Task"}));
        visible.insert(pane, json!("hello\u{1b}[31mworld\u{202e}done"));
    }
    std::fs::write(
        root.join("state.json"),
        json!({"agents":agents,"visible":visible}).to_string(),
    )
    .expect("mock state");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", name, "--nocapture"])
        .env("TALARIA_HERDR_TEST_CASE", name)
        .env("TALARIA_HERDR_TEST_VARIANT", variant)
        .env("TALARIA_HERDR_TEST_ROOT", root)
        .env("TALARIA_MOCK_HERDR_STATE", root.join("state.json"))
        .env("TALARIA_MOCK_HERDR_LOG", root.join("log.jsonl"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated test");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().expect("child status").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("isolated test timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("child output");
    if !output.status.success() {
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        let root_text = root.to_string_lossy();
        panic!(
            "isolated test failed:\n{}",
            text.replace(root_text.as_ref(), "<tmp>")
        );
    }
}

pub fn binary() -> PathBuf {
    std::env::var_os("TALARIA_MOCK_HERDR_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mock-herdr")))
}
pub fn service(workspace_filter: Option<String>) -> HerdrService {
    let config = HerdrConfig {
        enabled: true,
        binary: binary(),
        poll_interval_ms: 50,
        workspace_filter,
    };
    let fleet_config = FleetConfig {
        hot_poll: Duration::from_secs(60),
        cold_poll: Duration::from_secs(60),
        backstop: Duration::from_secs(60),
        ..FleetConfig::default()
    };
    let fleet = FleetService::start(root(), "fixture".into(), fleet_config, |_| {});
    HerdrService::start(config, fleet)
}
pub fn calls() -> Vec<Vec<String>> {
    std::fs::read_to_string(root().join("log.jsonl"))
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
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(100)).await;
}
