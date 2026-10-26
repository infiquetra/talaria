//! Live background launches. Ignored unless someone asks for them.
//!
//! A skip is not a pass: with `TALARIA_LIVE_REQUIRE=1` a skip fails the test. A test that passes
//! prints `LIVE_BG_RESULT: PASSED`; one that fails prints `LIVE_BG_RESULT: FAILED`.

#[path = "../../talaria-app/tests/support/git_fixture.rs"]
mod git_fixture;

mod support;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use hermes_client::{DiscoverOptions, EnsurePolicy, default_home, discover_with};
use serde_json::{Value, json};
use talaria_app::bg::{
    LaunchConn, LaunchLink, LaunchNotice, LaunchOrigin, LaunchOutcome, LaunchRequest, LaunchService,
};
use talaria_app::worktree::GitRunner;

const PROFILE: &str = "fixture";
const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn sanitize(reason: &str) -> String {
    let account = std::env::var("USER").unwrap_or_default();
    reason
        .split_whitespace()
        .map(|word| {
            let path = word
                .split(['\'', '"', '(', ')', '=', ':'])
                .any(|piece| Path::new(piece).is_absolute());
            let named = !account.is_empty() && word.contains(&account);
            if path || named { "<path>" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn skip(reason: &str) -> bool {
    let reason = sanitize(reason);
    println!("LIVE_BG_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    false
}

fn begin() -> bool {
    let output = Command::new("git").arg("version").output();
    let git_ok = match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let version = text.split_whitespace().nth(2).unwrap_or("");
            let mut parts = version.split(['.', '-']);
            let major: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
            let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
            major > 2 || (major == 2 && minor >= 28)
        }
        _ => false,
    };
    if !git_ok {
        return skip("git 2.28 or later is required");
    }
    let home = hermes_home();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return skip(&error.to_string()),
    };
    let discovered = runtime.block_on(async {
        discover_with(
            &home,
            &DiscoverOptions {
                control_timeout: Duration::from_secs(5),
                ensure: EnsurePolicy::Never,
            },
        )
        .await
        .map(|endpoint| endpoint.code_sha().map(str::to_owned))
        .map_err(|error| sanitize(&error.to_string()))
    });
    match discovered {
        Err(reason) => skip(&reason),
        Ok(sha) => match hermes_contract::pinned::live_skip_reason(sha.as_deref()) {
            Some(reason) => skip(&reason),
            None => true,
        },
    }
}

fn failed(detail: &str) -> ! {
    println!("LIVE_BG_RESULT: FAILED");
    panic!("{}", sanitize(detail));
}

fn talaria_command(cwd: &Path, args: &[&str]) -> Command {
    let home = hermes_home();
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("HOME", &home)
        .env("HERMES_HOME", &home)
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command
}

fn run_bg(cwd: &Path, args: &[&str]) -> std::process::Output {
    support::captured(talaria_command(cwd, args), Duration::from_secs(8))
}

/// The launch's one JSON line, or a failed test with the command's own words.
fn launch_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) else {
        failed(&format!(
            "the launch printed no JSON (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    };
    if value["status"] != "launched" || output.status.code() != Some(0) {
        failed(&format!(
            "the launch did not start a session (exit {:?}): {}",
            output.status.code(),
            value["error"]
        ));
    }
    value
}

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    match value[key].as_str() {
        Some(text) if !text.is_empty() => text,
        _ => failed(&format!("the launch JSON has no {key}")),
    }
}

/// `talaria fleet list --watch --json`, read a line at a time. The child is killed on drop.
struct Watch {
    child: Child,
    lines: Receiver<String>,
}

impl Watch {
    fn start(cwd: &Path) -> Self {
        let mut command = talaria_command(
            cwd,
            &[
                "fleet",
                "list",
                "--watch",
                "--json",
                "--profile",
                PROFILE,
                "--for",
                "120",
            ],
        );
        let mut child = command.spawn().expect("spawn the fleet watch");
        let stdout = child.stdout.take().expect("watch stdout");
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    /// Lines parsed as JSON until `accept` returns true, or `limit` passes.
    fn until(&self, limit: Duration, accept: impl Fn(&Value) -> bool) -> Option<Value> {
        let end = Instant::now() + limit;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && accept(&value)
                    {
                        return Some(value);
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One `talaria fleet list --json` snapshot of the fixture profile.
fn fleet_once(cwd: &Path) -> Value {
    let output = support::captured(
        talaria_command(cwd, &["fleet", "list", "--json", "--profile", PROFILE]),
        Duration::from_secs(20),
    );
    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(value) => value,
        Err(_) => failed(&format!(
            "fleet list printed no snapshot (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

/// Open a link as the fixture profile and run `work` on it, then close it.
fn on_own_connection<T>(work: impl AsyncFnOnce(&LaunchConn) -> T) -> T {
    runtime().block_on(async {
        let link = match LaunchLink::open(&hermes_home(), PROFILE).await {
            Ok(link) => link,
            Err(error) => failed(&format!("could not open a link: {}", error.what)),
        };
        let conn = LaunchConn::Link(link);
        let result = work(&conn).await;
        conn.close().await;
        result
    })
}

const CALL_LIMIT: Duration = Duration::from_secs(20);

async fn call(conn: &LaunchConn, method: &'static str, params: Value) -> Value {
    match conn.call(method, params, CALL_LIMIT).await {
        Ok(value) => value,
        Err(error) => failed(&format!("{method} failed: {error:?}")),
    }
}

/// Resume a session on `conn`, returning the snapshot (which subscribes the connection).
async fn resume(conn: &LaunchConn, session_id: &str) -> Value {
    call(conn, "session.resume", json!({ "session_id": session_id })).await
}

async fn detach(conn: &LaunchConn, session_id: &str, snapshot: &Value) {
    let params = json!({
        "session_id": session_id,
        "subscription_id": snapshot["subscription_id"],
    });
    call(conn, "session.detach", params).await;
}

fn passed() {
    println!("LIVE_BG_RESULT: PASSED");
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_runs_after_exit() {
    if !begin() {
        return;
    }
    let repo = git_fixture::GitRepo::new();
    let watch = Watch::start(&repo.repo);
    if watch
        .until(Duration::from_secs(30), |line| line["kind"] == "snapshot")
        .is_none()
    {
        failed("the fleet watch printed no snapshot within 30 s");
    }
    let started = Instant::now();
    let output = run_bg(
        &repo.repo,
        &[
            "--bg",
            "--output-format",
            "json",
            "--profile",
            PROFILE,
            "--model",
            MODEL,
            "--provider",
            PROVIDER,
            "--effort",
            "low",
            "Reply with the single word: ready",
        ],
    );
    let elapsed = started.elapsed();
    println!("launch_ms={}", elapsed.as_millis());
    let launch = launch_json(&output);
    if elapsed > Duration::from_secs(5) {
        failed("the launch took longer than 5 s");
    }
    let model = text_of(&launch, "model");
    if model != MODEL && !model.ends_with(&format!("/{MODEL}")) {
        failed(&format!("the launch reported model {model}"));
    }
    let session_id = text_of(&launch, "session_id").to_owned();
    let done = watch.until(Duration::from_secs(120), |line| {
        line["kind"] == "row"
            && line["row"]["session_id"] == session_id.as_str()
            && line["row"]["state"] == "done"
    });
    if done.is_none() {
        failed("the watch never showed the launched session as done within 120 s");
    }
    passed();
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_worktree_isolates_edits() {
    if !begin() {
        return;
    }
    let repo = git_fixture::GitRepo::new();
    let output = run_bg(
        &repo.repo,
        &[
            "--bg",
            "--worktree",
            "--name",
            "live-probe",
            "--output-format",
            "json",
            "--profile",
            PROFILE,
            "--model",
            MODEL,
            "--provider",
            PROVIDER,
            "Create a file named hello.txt containing the word hi in the current folder. Do nothing else.",
        ],
    );
    let launch = launch_json(&output);
    let tree = PathBuf::from(text_of(&launch["worktree"], "path"));
    let head = repo.ok(&tree, &["rev-parse", "HEAD"]);
    if head != repo.origin_main {
        failed("the tree does not start at origin's newest commit");
    }
    let reported = PathBuf::from(text_of(&launch, "cwd"));
    println!("launch cwd: {}", reported.display());
    println!("sent folder: {}", tree.display());
    let canonical = |path: &Path| std::fs::canonicalize(path).ok();
    if canonical(&reported).is_none() || canonical(&reported) != canonical(&tree) {
        failed("the reported folder is not the tree");
    }
    // The folder Hermes holds for the session, read on a connection of our own.
    let session_id = text_of(&launch, "session_id").to_owned();
    let info_cwd = on_own_connection(async |conn| {
        let snapshot = resume(conn, &session_id).await;
        let cwd = snapshot["info"]["cwd"].as_str().map(PathBuf::from);
        detach(conn, &session_id, &snapshot).await;
        cwd
    });
    let Some(info_cwd) = info_cwd else {
        failed("the resumed session reported no folder");
    };
    println!("info.cwd: {}", info_cwd.display());
    if canonical(&info_cwd).is_none() || canonical(&info_cwd) != canonical(&tree) {
        failed("info.cwd is not the folder that was sent");
    }
    let made = tree.join("hello.txt");
    let end = Instant::now() + Duration::from_secs(180);
    while !made.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(500));
    }
    if !made.exists() {
        failed("hello.txt did not appear in the tree within 180 s");
    }
    if repo.repo.join("hello.txt").exists() {
        failed("hello.txt appeared in the main checkout");
    }
    if !repo.ok(&repo.repo, &["status", "--porcelain"]).is_empty() {
        failed("the main checkout is not clean");
    }
    passed();
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_approval_waits_in_queue() {
    if !begin() {
        return;
    }
    let repo = git_fixture::GitRepo::new();
    let probe = repo.repo.join("talaria-bg-probe");
    std::fs::create_dir(&probe).expect("probe folder");
    let output = run_bg(
        &repo.repo,
        &[
            "--bg",
            "--output-format",
            "json",
            "--profile",
            PROFILE,
            "--model",
            MODEL,
            "--provider",
            PROVIDER,
            "Run this exact shell command: rm -rf ./talaria-bg-probe",
        ],
    );
    let launch = launch_json(&output);
    let session_id = text_of(&launch, "session_id").to_owned();
    let end = Instant::now() + Duration::from_secs(60);
    let need = loop {
        let snapshot = fleet_once(&repo.repo);
        let found = snapshot["needs_you"].as_array().and_then(|needs| {
            needs.iter().find(|need| {
                need["session_id"] == session_id.as_str() && need["prompt_id"].is_string()
            })
        });
        if let Some(need) = found {
            break need.clone();
        }
        if Instant::now() >= end {
            failed("no needs_you item for the session within 60 s");
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    if !probe.exists() {
        failed("the probe folder was removed before the approval was answered");
    }
    let prompt_id = text_of(&need, "prompt_id").to_owned();
    on_own_connection(async |conn| {
        let snapshot = resume(conn, &session_id).await;
        let Some(generation) = snapshot["execution_generation"].as_i64() else {
            failed("the resumed session reported no execution_generation");
        };
        let params = json!({
            "session_id": session_id,
            "execution_generation": generation,
            "prompt_id": prompt_id,
            "choice": "deny",
        });
        call(conn, "approval.respond", params).await;
        detach(conn, &session_id, &snapshot).await;
    });
    std::thread::sleep(Duration::from_secs(2));
    if !probe.exists() {
        failed("the probe folder was removed after the approval was denied");
    }
    passed();
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_host_link_other_profile() {
    if !begin() {
        return;
    }
    runtime().block_on(host_link_other_profile());
}

async fn host_link_other_profile() {
    let repo = git_fixture::GitRepo::new();
    let folder = std::fs::canonicalize(&repo.repo).expect("canonical folder");
    let notices = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let slot = std::sync::Arc::clone(&notices);
    let service = LaunchService::new(hermes_home(), GitRunner::from_env(), move |notice| {
        slot.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(notice);
    });
    let request = LaunchRequest {
        prompt: "Reply with the single word: ready".to_owned(),
        profile: PROFILE.to_owned(),
        folder: folder.clone(),
        model: Some(MODEL.to_owned()),
        provider: Some(PROVIDER.to_owned()),
        effort: Some("low".to_owned()),
        worktree: None,
        origin: LaunchOrigin::FleetView,
    };
    if service.launch(request, "default".to_owned(), None).is_err() {
        failed("the service refused the launch");
    }
    let end = Instant::now() + Duration::from_secs(30);
    let outcome = loop {
        let finished = notices
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .find_map(|notice| match notice {
                LaunchNotice::Finished { outcome, .. } => Some(outcome.clone()),
                LaunchNotice::Progress { .. } => None,
            });
        if let Some(outcome) = finished {
            break outcome;
        }
        if Instant::now() >= end {
            failed("no finished notice within 30 s");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let launched = match outcome {
        LaunchOutcome::Launched(launched) => launched,
        LaunchOutcome::NotStarted { error, .. }
            if error.what.contains("profile_mismatch")
                || error.what.contains("different profile") =>
        {
            failed("stop condition: a host ticket cannot launch in another served profile")
        }
        other => failed(&format!("the launch did not start a session: {other:?}")),
    };
    let session_id = launched.session_id;
    let listed = tokio::task::spawn_blocking(move || {
        let snapshot = fleet_once(&folder);
        snapshot["rows"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["session_id"] == session_id.as_str())
        })
    })
    .await
    .unwrap_or(false);
    if !listed {
        failed("the session is not in the fixture profile's fleet list");
    }
    passed();
}
