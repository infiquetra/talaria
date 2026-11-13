//! The agent diagnostics command.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn mock_binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Some(path) = std::env::var_os("TALARIA_MOCK_ACP_BIN") {
                return path.into();
            }
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
            let binary = target.join("debug/mock-acp-agent");
            if !binary.is_file() {
                let status = Command::new("cargo")
                    .args(["build", "-p", "talaria-app", "--bin", "mock-acp-agent"])
                    .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
                    .status()
                    .expect("build mock agent");
                assert!(status.success(), "mock agent build");
            }
            binary
        })
        .clone()
}

fn run(root: &Path, args: &[&str], mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("HOME", root)
        .env("HERMES_HOME", root)
        .env("TALARIA_MOCK_ACP_MODE", mode)
        .env("TALARIA_MOCK_ACP_LOG", root.join("log.jsonl"))
        .env("TALARIA_MOCK_ACP_PID_FILE", root.join("pid"))
        .env_remove("TALARIA_MOCK_ACP_SCRIPT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("talaria command");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().expect("process status").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("diagnostics command timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().expect("command output")
}
fn config(root: &Path, text: &str) {
    std::fs::create_dir_all(root.join(".talaria")).expect("config directory");
    std::fs::write(root.join(".talaria/config.toml"), text).expect("configuration");
}
fn agent_config(binary: &Path) -> String {
    let command = serde_json::to_string(&binary.to_string_lossy()).expect("quoted command");
    format!(
        "[[acp.agents]]\nid = \"fixture\"\ndisplay_name = \"Fixture Agent\"\ncommand = {command}\n"
    )
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn log(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(root.join("log.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn agents_lists_configured_agents() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    let first = agent_config(&mock_binary());
    let rest = "\n[[acp.agents]]\nid = \"missing\"\ndisplay_name = \"Missing Agent\"\ncommand = \"missing-agent-acp\"\n\n[[acp.agents]]\nid = \"empty\"\ncommand = \"  \"\n\n[[acp.agents]]\nid = \"fixture\"\ndisplay_name = \"Duplicate\"\ncommand = \"another-command\"\n";
    config(tmp.path(), &format!("{first}{rest}"));
    let out = run(tmp.path(), &["acp", "agents"], "");
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stdout),
        "fixture: Fixture Agent - ok\nmissing: Missing Agent - missing: command \"missing-agent-acp\" not found\n"
    );
}

#[test]
fn probe_handshakes_without_a_session() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    config(tmp.path(), &agent_config(&mock_binary()));
    let out = run(tmp.path(), &["acp", "agents", "--probe"], "");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        text(&out.stdout),
        "fixture: Fixture Agent - ok - Mock Agent, protocol 1\n"
    );
    let events = log(tmp.path());
    let init = events
        .iter()
        .find_map(|v| v.get("seen").filter(|m| m["method"] == "initialize"))
        .expect("initialization");
    assert_eq!(
        init["params"]["clientCapabilities"],
        serde_json::json!({"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false})
    );
    assert!(events.iter().all(
        |v| v.pointer("/seen/method") != Some(&Value::from("session/new"))
            && v.pointer("/seen/method") != Some(&Value::from("authenticate"))
    ));
    assert_eq!(
        events.last().expect("counts")["counts"],
        serde_json::json!({"session/new":0,"authenticate":0})
    );
}

#[test]
fn probe_wrong_version_exits_one() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    config(tmp.path(), &agent_config(&mock_binary()));
    let out = run(tmp.path(), &["acp", "agents", "--probe"], "wrong_version");
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stdout).is_empty());
    let error = text(&out.stderr);
    assert!(error.contains('1') && error.contains("99"));
    let pid = std::fs::read_to_string(tmp.path().join("pid")).expect("agent pid");
    assert!(
        !Command::new("kill")
            .args(["-0", pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("pid check")
            .success()
    );
}

#[test]
fn unreadable_config_exits_two() {
    let tmp = tempfile::tempdir().expect("temporary directory");
    config(tmp.path(), "[acp\n");
    let out = run(tmp.path(), &["acp", "agents"], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let path = tmp.path().join(".talaria/config.toml");
    std::fs::write(&path, "").expect("valid config");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))
            .expect("unreadable");
        let out = run(tmp.path(), &["acp", "agents"], "");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("restore permissions");
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    std::fs::remove_file(&path).expect("remove config");
    let missing = run(tmp.path(), &["acp", "agents"], "");
    assert_eq!(missing.status.code(), Some(0));
    assert!(missing.stdout.is_empty());
    config(tmp.path(), "[ui]\n");
    let empty = run(tmp.path(), &["acp", "agents"], "");
    assert_eq!(empty.status.code(), Some(0));
    assert!(empty.stdout.is_empty());
}
