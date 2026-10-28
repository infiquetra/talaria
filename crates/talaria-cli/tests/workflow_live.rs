use hermes_client::{
    CallOptions, Client, DiscoverOptions, EnsurePolicy, Purpose, Scope, connect, default_home,
    discover_with, mint_ticket,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn skip(reason: &str) {
    println!("skipped: {reason} not met");
    assert_ne!(
        std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref(),
        Some("1"),
        "live precondition not met"
    );
}
fn frames(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn outbound_for(records: &[Value], method: &str, field: &str, key: &str) -> usize {
    records
        .iter()
        .filter(|r| {
            r["kind"] == "frame"
                && r["dir"] == "out"
                && r["frame"]["method"] == method
                && r["frame"]["params"][field]
                    .as_str()
                    .is_some_and(|id| id.contains(key))
        })
        .count()
}

#[tokio::test]
#[ignore]
async fn live_detached_review_run() {
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let endpoint = match discover_with(
        &home,
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    {
        Ok(endpoint) => endpoint,
        Err(_) => {
            skip("gateway");
            return;
        }
    };
    if hermes_contract::pinned::live_skip_reason(endpoint.code_sha()).is_some() {
        skip("pinned gateway revision");
        return;
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(p) => p,
        Err(_) => {
            skip("fixture profile");
            return;
        }
    };
    let ticket = match mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(_) => {
            skip("fixture profile ticket");
            return;
        }
    };
    let connection = match connect(&endpoint, ticket).await {
        Ok(c) => c,
        Err(_) => {
            skip("fixture connection");
            return;
        }
    };
    let (client, _inbound) = Client::start(connection.into_parts());
    let roster = match client
        .call_raw(
            "model.options",
            json!({}),
            CallOptions {
                timeout: Some(Duration::from_secs(15)),
            },
        )
        .await
    {
        Ok(value) => value,
        Err(_) => {
            skip("model roster");
            return;
        }
    };
    let available = roster["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| {
            p["slug"] == "ollama-cloud"
                && p["authenticated"] == true
                && p["models"]
                    .as_array()
                    .is_some_and(|models| models.iter().any(|m| m == "minimax-m3.1-flash"))
        });
    if !available {
        skip("free fixture model");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let script = temp.path().join("review.js");
    fs::write(&script,"export const meta={name:'review',description:'live fixture'};\nreturn await agent('Reply with the single word READY.',{label:'review',model:'minimax-m3.1-flash',provider:'ollama-cloud'});\n").unwrap();
    let initial_record = temp.path().join("initial.jsonl");
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "workflow",
            "run",
            "--detach",
            "--json",
            "--profile",
            "fixture",
            "--record",
        ])
        .arg(&initial_record)
        .arg("--runs-dir")
        .arg(&runs)
        .arg(&script)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "detached start failed");
    let detached = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|v| v["detached"] == true)
        .unwrap();
    let id = detached["run_id"].as_str().unwrap();
    let folder = PathBuf::from(detached["run_dir"].as_str().unwrap());
    let deadline = Instant::now() + Duration::from_secs(180);
    let journal = loop {
        let rows: Vec<Value> = fs::read_to_string(folder.join("journal.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if rows.iter().any(|r| r["t"] == "run_finished") {
            break rows;
        }
        assert!(Instant::now() < deadline, "detached run did not finish");
        thread::sleep(Duration::from_millis(250));
    };
    assert!(
        journal
            .iter()
            .any(|r| r["t"] == "run_finished" && r["outcome"] == "completed")
    );
    let key = journal.iter().find(|r| r["t"] == "call_intent").unwrap()["key"]
        .as_str()
        .unwrap();
    let initial = frames(&initial_record);
    assert!(
        initial
            .iter()
            .any(|r| r["kind"] == "frame" && r["dir"] == "in")
    );
    assert!(
        initial
            .iter()
            .any(|r| r["kind"] == "frame" && r["dir"] == "out")
    );
    assert!(!initial.iter().any(|r| r["kind"] == "dropped"));
    assert_eq!(
        outbound_for(&initial, "session.create", "request_id", key),
        1
    );
    assert_eq!(
        outbound_for(&initial, "prompt.submit", "submission_id", key),
        1
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    let replay_record = temp.path().join("replay.jsonl");
    let replay = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(["workflow", "resume", "--json", "--record"])
        .arg(&replay_record)
        .arg("--runs-dir")
        .arg(&runs)
        .arg(id)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(replay.status.success(), "completed replay failed");
    let result = String::from_utf8(replay.stdout)
        .unwrap()
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|v| v["event"] == "run_finished")
        .unwrap();
    assert_eq!(result["result"], saved["result"]);
    let replay_frames = frames(&replay_record);
    assert!(!replay_frames.iter().any(|r| r["kind"] == "dropped"));
    assert_eq!(
        outbound_for(&replay_frames, "session.create", "request_id", key),
        0
    );
    assert_eq!(
        outbound_for(&replay_frames, "prompt.submit", "submission_id", key),
        0
    );
}

#[path = "workflow_builtins_live.rs"]
mod builtins_contract;
