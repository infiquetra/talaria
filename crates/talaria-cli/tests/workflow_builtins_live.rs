use serde_json::Value;
use std::{
    fs,
    net::{SocketAddr, TcpStream},
    path::Path,
    process::Command,
    time::Duration,
};

fn command_ok(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}
fn files_contain_forbidden(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_contain_forbidden(&path);
        } else {
            assert!(!String::from_utf8_lossy(&fs::read(&path).unwrap()).contains("unattended"));
        }
    }
}
fn run_live(name: &str, mode: &str) {
    for (program, arg) in [("git", "--version"), ("python3", "--version")] {
        if !command_ok(program, arg) {
            eprintln!("skipped: {program} is unavailable");
            return;
        }
    }
    let address: SocketAddr = "127.0.0.1:18642".parse().unwrap();
    if TcpStream::connect_timeout(&address, Duration::from_millis(500)).is_err() {
        eprintln!("skipped: fixture gateway is not reachable");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let runs = temp.path().join("runs");
    let config = temp.path().join("config");
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/builtins");
    let fixture = Command::new("sh")
        .arg(data.join("make-sample-repo.sh"))
        .arg(&repo)
        .arg(mode)
        .output()
        .unwrap();
    assert!(
        fixture.status.success(),
        "{}",
        String::from_utf8_lossy(&fixture.stderr)
    );
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo)
        .output()
        .unwrap()
        .stdout;
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_talaria"));
        c.current_dir(&repo)
            .env("TALARIA_RUNS_DIR", &runs)
            .env("TALARIA_CONFIG_DIR", &config);
        c
    };
    let output = command()
        .args([
            "workflow",
            "run",
            name,
            "--profile",
            "fixture",
            "--staffing",
        ])
        .arg(data.join("staffing-fixture.toml"))
        .arg("--json")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        && (stderr.contains("profile")
            && (stderr.contains("not found") || stderr.contains("not served")))
    {
        eprintln!("skipped: fixture profile is not served");
        return;
    }
    assert!(output.status.success(), "workflow failed: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let finished = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .rfind(|v| v["event"] == "run_finished")
        .expect("final run_finished event");
    let result = &finished["result"];
    let status = result["status"].as_str().expect("result status");
    assert!(if mode == "review" {
        status == "reviewed"
    } else {
        ["fixed-all", "partial"].contains(&status)
    });
    let run_id = finished["run_id"].as_str().expect("run id");
    let listing = command()
        .args(["workflow", "list", "--json"])
        .output()
        .unwrap();
    assert!(listing.status.success());
    let list: Value = serde_json::from_slice(&listing.stdout).unwrap();
    let entries = list
        .as_array()
        .or_else(|| list["runs"].as_array())
        .expect("run list");
    let entry = entries
        .iter()
        .find(|v| v["run_id"] == run_id)
        .expect("run in list");
    assert!(entry["agents_finished"].as_u64().unwrap() >= if mode == "review" { 5 } else { 3 });
    assert_eq!(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout,
        head
    );
    assert!(
        Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout
            .is_empty()
    );
    files_contain_forbidden(&runs.join(run_id));
    if mode == "failing" {
        let attempts = result["attempted"].as_array().unwrap();
        let verified: Vec<_> = attempts
            .iter()
            .filter(|v| v["status"] == "verified")
            .collect();
        assert!(!verified.is_empty(), "positive fixture must verify a fix");
        for (i, item) in verified.iter().enumerate() {
            let branch = item["branch"].as_str().unwrap();
            let checkout = temp.path().join(format!("verified-{i}"));
            let add = Command::new("git")
                .args(["worktree", "add", "--detach"])
                .arg(&checkout)
                .arg(branch)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                add.status.success(),
                "{}",
                String::from_utf8_lossy(&add.stderr)
            );
            let one = Command::new("sh")
                .args(["-c", item["run_one"].as_str().unwrap()])
                .current_dir(&checkout)
                .output()
                .unwrap();
            assert!(
                one.status.success(),
                "{}",
                String::from_utf8_lossy(&one.stderr)
            );
            let suite = Command::new("python3")
                .args(["-m", "unittest", "-v"])
                .current_dir(&checkout)
                .output()
                .unwrap();
            let baseline = result["baseline"].as_array().unwrap();
            let text = String::from_utf8_lossy(&suite.stderr);
            for line in text
                .lines()
                .filter(|s| s.starts_with("FAIL: ") || s.starts_with("ERROR: "))
            {
                assert!(
                    baseline
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|id| line.contains(id)),
                    "new failure: {line}"
                );
            }
        }
    }
}
#[test]
#[ignore = "requires the fixture gateway and operator preconditions"]
fn review_branch_live_fixture() {
    run_live("review-branch", "review");
}
#[test]
#[ignore = "requires the fixture gateway and operator preconditions"]
fn fix_failing_tests_live_fixture() {
    run_live("fix-failing-tests", "failing");
}
