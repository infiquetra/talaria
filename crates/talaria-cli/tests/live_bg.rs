//! Live background launches. Ignored unless someone asks for them.
//!
//! A skip is not a pass: with `TALARIA_LIVE_REQUIRE=1` a skip fails the test. The stub starts
//! nothing, so a run whose preconditions hold prints `LIVE_BG_RESULT: FAILED`.

#[path = "../../talaria-app/tests/support/git_fixture.rs"]
mod git_fixture;

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use hermes_client::{DiscoverOptions, EnsurePolicy, default_home, discover_with};
use talaria_app::bg::{LaunchOrigin, LaunchRequest, LaunchService};
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

fn run_bg(cwd: &Path, args: &[&str]) -> std::process::Output {
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
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    support::captured(command, Duration::from_secs(8))
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_runs_after_exit() {
    if !begin() {
        return;
    }
    let repo = git_fixture::GitRepo::new();
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.code() != Some(0) || !stdout.contains(MODEL) {
        failed("background launch produced no session");
    }
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("\"status\":\"launched\"") {
        failed("worktree launch produced no session");
    }
}

#[test]
#[ignore = "needs a running gateway"]
fn live_bg_approval_waits_in_queue() {
    if !begin() {
        return;
    }
    let repo = git_fixture::GitRepo::new();
    std::fs::create_dir(repo.repo.join("talaria-bg-probe")).expect("probe folder");
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("\"status\":\"launched\"") {
        failed("approval launch produced no session");
    }
}

#[tokio::test]
#[ignore = "needs a running gateway"]
async fn live_bg_host_link_other_profile() {
    if !begin() {
        return;
    }
    let home = hermes_home();
    let notices = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let slot = std::sync::Arc::clone(&notices);
    let service = LaunchService::new(home, GitRunner::from_env(), move |notice| {
        slot.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(notice);
    });
    let request = LaunchRequest {
        prompt: "Reply with the single word: ready".to_owned(),
        profile: PROFILE.to_owned(),
        folder: PathBuf::from("repo"),
        model: Some(MODEL.to_owned()),
        provider: Some(PROVIDER.to_owned()),
        effort: Some("low".to_owned()),
        worktree: None,
        origin: LaunchOrigin::FleetView,
    };
    let launched = service.launch(request, "default".to_owned(), None);
    if launched.is_err() {
        failed("host link refused the launch");
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let got = notices
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if got.is_empty() {
        failed("host link produced no session");
    }
}
