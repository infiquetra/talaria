//! Isolated scripted agent process for transport tests.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use talaria_app::acp::{AcpAgentEntry, AcpConfig, AcpService};
use talaria_app::fleet::{FleetConfig, FleetNotice, FleetService, FleetSnapshot};

pub fn in_child(name: &str) -> bool {
    std::env::var("TALARIA_ACP_TEST_CASE").ok().as_deref() == Some(name)
}

pub fn run_case(name: &str, variant: &str, make_script: impl FnOnce(&Path) -> Vec<Value>) {
    let tmp = tempfile::tempdir().expect("temporary test directory");
    let root = tmp.path();
    std::fs::create_dir(root.join("work")).expect("work directory");
    std::fs::create_dir(root.join("outside")).expect("outside directory");
    std::fs::write(root.join("work/input.txt"), "inside").expect("inside file");
    std::fs::write(root.join("outside/secret.txt"), "outside").expect("outside file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("outside"), root.join("work/link")).expect("test symlink");
    let script = make_script(root)
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(root.join("script.jsonl"), script).expect("mock script");
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", name, "--nocapture"])
        .env("TALARIA_ACP_TEST_CASE", name)
        .env("TALARIA_ACP_TEST_VARIANT", variant)
        .env("TALARIA_ACP_TEST_ROOT", root)
        .env("TALARIA_MOCK_ACP_SCRIPT", root.join("script.jsonl"))
        .env("TALARIA_MOCK_ACP_LOG", root.join("step-log.jsonl"))
        .env("TALARIA_MOCK_ACP_PID_FILE", root.join("pid"))
        .env("TALARIA_SHELL_PROFILE_SENTINEL", "profile-leak-sentinel")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("isolated test");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().expect("child status").is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            if name == "failed_tool_then_recovery_stays_working_until_end" {
                eprintln!("ACP_TRACE_INCOMPLETE timeout");
                let _trace_complete = retain_fixture_trace(root);
            }
            panic!("isolated test timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("child output");
    let mut trace_complete = true;
    if name == "failed_tool_then_recovery_stays_working_until_end" {
        eprintln!(
            "ACP_TRACE_CHILD {}",
            if output.status.success() {
                "passed"
            } else {
                "failed"
            }
        );
        trace_complete = retain_fixture_trace(root);
    }
    if !output.status.success() {
        let cap = if name == "failed_tool_then_recovery_stays_working_until_end" {
            65536
        } else {
            usize::MAX
        };
        if output.stdout.len() > cap || output.stderr.len() > cap {
            eprintln!("ACP_TRACE_INCOMPLETE child-output");
        }
        let mut text =
            String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(cap)]).to_string();
        text.push_str(&String::from_utf8_lossy(
            &output.stderr[..output.stderr.len().min(cap)],
        ));
        let root_text = root.to_string_lossy();
        let cleaned = text.replace(root_text.as_ref(), "<tmp>");
        panic!("isolated test failed:\n{cleaned}");
    }
    assert!(
        trace_complete,
        "ACP trace capture incomplete (child passed)"
    );
}

pub fn root() -> PathBuf {
    PathBuf::from(std::env::var_os("TALARIA_ACP_TEST_ROOT").expect("test root"))
}
pub fn variant() -> String {
    std::env::var("TALARIA_ACP_TEST_VARIANT").unwrap_or_default()
}

pub fn agent() -> AcpAgentEntry {
    AcpAgentEntry {
        id: "fixture".into(),
        display_name: "Fixture Agent".into(),
        command: std::env::var("TALARIA_MOCK_ACP_BIN")
            .unwrap_or_else(|_| env!("CARGO_BIN_EXE_mock-acp-agent").into()),
        args: vec![],
        auth_method: None,
        allow_fs: false,
        allow_terminal: false,
    }
}

pub struct Harness {
    pub service: AcpService,
    pub snapshots: Arc<Mutex<Vec<FleetSnapshot>>>,
}

impl Harness {
    pub fn new(agent: AcpAgentEntry) -> Self {
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&snapshots);
        let fleet = FleetService::start(
            root(),
            "fixture".into(),
            FleetConfig::default(),
            move |notice| {
                if let FleetNotice::Changed(snapshot) = notice {
                    captured
                        .lock()
                        .expect("snapshots")
                        .push((*snapshot).clone());
                    if let Some(row) = snapshot
                        .rows
                        .iter()
                        .find(|row| row.key.session_id == "session-1")
                    {
                        talaria_app::acp::trace_fixture_event(
                            false,
                            match row.state {
                                talaria_core::fleet::RowState::Working => "fleet_working",
                                talaria_core::fleet::RowState::Done => "fleet_done",
                                talaria_core::fleet::RowState::Failed => "fleet_failed",
                                _ => "fleet_other",
                            },
                        );
                    }
                }
            },
        );
        Self {
            service: AcpService::start(
                AcpConfig {
                    agents: vec![agent],
                },
                fleet,
            ),
            snapshots,
        }
    }
}

pub fn expect(method: &str) -> Value {
    json!({"kind":"expect", "method":method})
}

// Temporary diagnostic output: fixed trace names and the existing public fixture log only.
fn retain_fixture_trace(root: &Path) -> bool {
    use std::io::Read;
    fn bounded(path: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
        let meta = std::fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.len() > cap as u64 {
            return Err(std::io::Error::other("invalid trace file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owner = std::fs::metadata(path.parent().expect("fixture parent"))?.uid();
            if meta.nlink() != 1 || meta.uid() != owner {
                return Err(std::io::Error::other("trace custody"));
            }
        }
        let mut data = Vec::new();
        std::fs::File::open(path)?
            .take((cap + 1) as u64)
            .read_to_end(&mut data)?;
        if data.len() > cap {
            return Err(std::io::Error::other("trace oversize"));
        }
        Ok(data)
    }
    fn allowed(event: &str) -> bool {
        matches!(
            event,
            "prompt_ack"
                | "prompt_returned"
                | "mock_write_begin"
                | "mock_write_flushed"
                | "app_update_received"
                | "prompt_response_received"
                | "prompt_finish_before"
                | "prompt_finish_done"
                | "prompt_finish_other"
                | "fleet_working"
                | "fleet_done"
                | "fleet_failed"
                | "fleet_other"
                | "checkpoint_working"
                | "checkpoint_done"
                | "checkpoint_other"
        )
    }
    let mut complete = true;
    for role in ["app", "mock"] {
        let mut valid = true;
        let mut count = 0;
        match bounded(&root.join(format!("acp-trace-{role}-status")), 64) {
            Ok(data) => {
                let text = String::from_utf8_lossy(&data);
                let fields: Vec<_> = text.trim_end_matches('\n').split('\t').collect();
                if fields.len() == 3
                    && fields[0] == role
                    && matches!(fields[2], "complete" | "incomplete")
                    && text.ends_with('\n')
                    && data.iter().filter(|&&b| b == b'\n').count() == 1
                {
                    count = fields[1].parse::<usize>().unwrap_or(0);
                    valid &= fields[2] == "complete";
                    valid &= (1..=32).contains(&count) && fields[1] == count.to_string();
                } else {
                    valid = false;
                }
            }
            Err(_) => valid = false,
        }
        for suffix in ["incomplete", "pending"] {
            match std::fs::symlink_metadata(root.join(format!("acp-trace-{role}-{suffix}"))) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => valid = false,
            }
        }
        let mut previous = 0u128;
        for sequence in 0..32 {
            match bounded(&root.join(format!("acp-trace-{role}-{sequence:02}")), 128) {
                Ok(data) => {
                    let text = String::from_utf8_lossy(&data);
                    let fields: Vec<_> = text.trim_end_matches('\n').split('\t').collect();
                    let micros = fields.get(2).and_then(|x| x.parse::<u128>().ok());
                    let good = fields.len() == 4
                        && fields[0] == role
                        && fields[1] == sequence.to_string()
                        && micros.is_some_and(|n| n >= previous && fields[2] == n.to_string())
                        && allowed(fields[3])
                        && text.ends_with('\n')
                        && data.iter().filter(|&&b| b == b'\n').count() == 1;
                    if good {
                        valid &= sequence < count;
                        previous = micros.unwrap_or(0);
                        eprintln!("ACP_TRACE {}", text.trim_end());
                    } else {
                        valid = false;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && sequence >= count => {
                }
                Err(_) => valid = false,
            }
        }
        eprintln!(
            "ACP_TRACE_ROLE {role} {}",
            if valid { "complete" } else { "incomplete" }
        );
        complete &= valid;
    }
    match bounded(&root.join("step-log.jsonl"), 64 * 1024) {
        Ok(data) => eprintln!("ACP_PUBLIC_STEP_LOG {}", String::from_utf8_lossy(&data)),
        Err(_) => {
            complete = false;
            eprintln!("ACP_TRACE_INCOMPLETE step-log");
        }
    }
    eprintln!(
        "ACP_TRACE_RETAINED {}",
        if complete { "complete" } else { "incomplete" }
    );
    complete
}

pub fn response(id: i64) -> Value {
    json!({"kind":"expect", "response_id":id})
}
pub fn reply(result: Value) -> Value {
    json!({"kind":"reply", "result":result})
}
pub fn emit(message: Value) -> Value {
    json!({"kind":"emit", "message":message})
}
pub fn finish() -> Vec<Value> {
    vec![
        expect("session/close"),
        reply(json!({})),
        json!({"kind":"exit"}),
    ]
}
pub fn hello(result: Value) -> Vec<Value> {
    vec![
        expect("initialize"),
        reply(result),
        expect("session/new"),
        reply(json!({"sessionId":"session-1"})),
    ]
}
pub fn standard_hello() -> Vec<Value> {
    hello(json!({"protocolVersion":1,"agentInfo":{"name":"Mock Agent"}}))
}
pub fn update(update: Value) -> Value {
    emit(
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"session-1","update":update}}),
    )
}
pub fn permission(id: i64) -> Value {
    json!({"kind":"ask_permission","message":{"jsonrpc":"2.0","id":id,"method":"session/request_permission","params":{"sessionId":"session-1","toolCall":{"toolCallId":"tool-1","title":"Run"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"reject","name":"Reject","kind":"reject_once"}]}}})
}
pub fn seen() -> Vec<Value> {
    let text = std::fs::read_to_string(root().join("step-log.jsonl")).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
pub fn messages(method: &str) -> Vec<Value> {
    seen()
        .into_iter()
        .filter_map(|entry| entry.get("seen").cloned())
        .filter(|message| message["method"] == method)
        .collect()
}
pub fn responses(id: i64) -> Vec<Value> {
    seen()
        .into_iter()
        .filter_map(|entry| entry.get("seen").cloned())
        .filter(|message| message.get("method").is_none() && message["id"] == id)
        .collect()
}
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}
pub fn process_alive() -> bool {
    let pid = std::fs::read_to_string(root().join("pid")).expect("agent pid");
    Command::new("kill")
        .arg("-0")
        .arg(pid.trim())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
