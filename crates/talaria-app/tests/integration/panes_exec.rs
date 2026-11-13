//! Pane execution against fake multiplexer binaries. Issue #246.
//!
//! Unix only: the fakes are `/bin/sh` scripts in temporary folders.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use talaria_app::panes::{
    CommandRunner, Direction, Multiplexer, PaneEnv, PaneOutcome, PaneRefusal, PaneRequest,
    PaneSettings, STEP_TIMEOUT, open_in_pane, parse_new_pane, split_command, type_commands,
};
use talaria_core::directory::rows::RowKey;

const LINE: &str = "'/t/talaria' attach 'local-ab' --profile 'fixture'";

fn key() -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-ab".to_owned(),
    }
}

fn herdr_env(bin: &Path) -> PaneEnv {
    PaneEnv {
        herdr_env: Some("1".to_owned()),
        herdr_pane_id: Some("w1:p1".to_owned()),
        herdr_bin_path: Some(bin.to_string_lossy().into_owned()),
        tmux: None,
        tmux_pane: None,
    }
}

fn request(exe: PathBuf, cwd: Option<PathBuf>) -> PaneRequest {
    PaneRequest {
        key: key(),
        title: "build api".to_owned(),
        exe,
        cwd,
        cols: 160,
        rows: 48,
    }
}

/// Write a fake multiplexer binary: it appends its arguments to a log file
/// (one per line, a blank line after each call), prints the reply, and exits
/// with the code. Returns the script and the log.
fn write_fake(
    dir: &Path,
    name: &str,
    reply: &str,
    code: i32,
    prologue: &str,
) -> (PathBuf, PathBuf) {
    let script = dir.join(name);
    let log = dir.join(format!("{name}.log"));
    let body = format!(
        "#!/bin/sh\nlog='{}'\n{{\n{}\nfor a in \"$@\"; do echo \"$a\"; done\necho \"\"\n}} >> \"$log\"\nprintf '%s' '{}'\nexit {}\n",
        log.display(),
        prologue,
        reply,
        code
    );
    std::fs::write(&script, body).expect("the fake script");
    let mut permissions = std::fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("the fake is executable");
    (script, log)
}

/// The log as one argument list per call.
fn calls(log: &Path) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    text.split("\n\n")
        .map(|block| block.lines().map(str::to_owned).collect())
        .filter(|block: &Vec<String>| !block.is_empty())
        .collect()
}

#[tokio::test]
async fn herdr_fake_split_then_run() {
    let dir = tempfile::tempdir().expect("a folder");
    let cwd = dir.path().join("work");
    std::fs::create_dir(&cwd).expect("a folder");
    // The Herdr fake also reports standard input and the environment, so the
    // test sees stdin is null and the environment inherited.
    let prologue = "if read -r _; then echo stdin=data; else echo stdin=eof; fi\nif [ -n \"$PATH\" ]; then echo PATH=set; else echo PATH=empty; fi";
    let (fake, log) = write_fake(
        dir.path(),
        "herdr",
        "{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}",
        0,
        prologue,
    );
    let runner = talaria_app::panes::SystemRunner;
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&fake),
        &PaneSettings::default(),
        &request(fake.clone(), Some(cwd.clone())),
    )
    .await;
    assert_eq!(
        outcome,
        PaneOutcome::Opened {
            multiplexer: "Herdr"
        }
    );

    let made = calls(&log);
    assert_eq!(made.len(), 2, "split, then one run: {made:?}");
    let cwd_arg = cwd.to_string_lossy().into_owned();
    assert_eq!(
        made[0],
        [
            "stdin=eof",
            "PATH=set",
            "pane",
            "split",
            "--current",
            "--direction",
            "right",
            "--cwd",
            cwd_arg.as_str(),
            "--no-focus",
        ]
    );
    assert_eq!(made[1][0], "stdin=eof");
    assert_eq!(made[1][1], "PATH=set");
    let run: Vec<&str> = made[1][2..5].iter().map(String::as_str).collect();
    assert_eq!(run, ["pane", "run", "w1:p7"]);
    assert_eq!(made[1].len(), 6, "the line is one argument: {:?}", made[1]);
    assert!(
        made[1][5].contains("attach 'local-ab' --profile 'fixture'"),
        "the typed line: {:?}",
        made[1][5]
    );

    // A folder deleted before the call leaves the split without `--cwd`.
    let gone = dir.path().join("gone");
    std::fs::create_dir(&gone).expect("a folder");
    std::fs::remove_dir(&gone).expect("deleted");
    let (fake, log) = write_fake(
        dir.path(),
        "herdr2",
        "{\"result\":{\"pane\":{\"pane_id\":\"w1:p8\"}}}",
        0,
        "",
    );
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&fake),
        &PaneSettings::default(),
        &request(fake.clone(), Some(gone)),
    )
    .await;
    assert_eq!(
        outcome,
        PaneOutcome::Opened {
            multiplexer: "Herdr"
        }
    );
    let made = calls(&log);
    assert!(
        !made[0].iter().any(|arg| arg == "--cwd"),
        "a deleted folder still passed: {made:?}"
    );
}

#[tokio::test]
async fn tmux_fake_split_then_two_send_keys() {
    let dir = tempfile::tempdir().expect("a folder");
    let cwd = dir.path().join("work");
    std::fs::create_dir(&cwd).expect("a folder");
    let (fake, log) = write_fake(dir.path(), "tmux", "%12", 0, "");
    let mux = Multiplexer::Tmux {
        bin: fake.clone(),
        pane: "%1".to_owned(),
    };
    let runner = talaria_app::panes::SystemRunner;

    let split = split_command(&mux, Direction::Right, Some(&cwd), false);
    assert_eq!(split.program, fake);
    let out = runner.run(&split, STEP_TIMEOUT).await;
    assert_eq!(out.code, Some(0));
    assert!(!out.timed_out);
    let pane = parse_new_pane(&mux, &out.stdout).expect("the new pane id");
    assert_eq!(pane, "%12");
    let typed = type_commands(&mux, &pane, LINE);
    assert_eq!(typed.len(), 2);
    for spec in &typed {
        let out = runner.run(spec, STEP_TIMEOUT).await;
        assert_eq!(out.code, Some(0));
    }

    let made = calls(&log);
    assert_eq!(made.len(), 3, "split, then two send-keys: {made:?}");
    let cwd_arg = cwd.to_string_lossy().into_owned();
    assert_eq!(
        made[0],
        [
            "split-window",
            "-h",
            "-d",
            "-t",
            "%1",
            "-c",
            cwd_arg.as_str(),
            "-P",
            "-F",
            "#{pane_id}",
        ]
    );
    assert_eq!(made[1], ["send-keys", "-t", "%12", "-l", LINE]);
    assert_eq!(made[2], ["send-keys", "-t", "%12", "Enter"]);
}

#[tokio::test]
async fn split_failure_and_timeout() {
    let dir = tempfile::tempdir().expect("a folder");
    let runner = talaria_app::panes::SystemRunner;

    // Exit 1 with a reason on standard error.
    let script = dir.path().join("herdr-fail");
    let log = dir.path().join("herdr-fail.log");
    let body = format!(
        "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done >> '{}'\necho split refused >&2\nexit 1\n",
        log.display()
    );
    std::fs::write(&script, body).expect("the fake script");
    let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("executable");
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&script),
        &PaneSettings::default(),
        &request(script.clone(), None),
    )
    .await;
    assert_eq!(
        outcome,
        PaneOutcome::SplitFailed {
            multiplexer: "Herdr",
            reason: "split refused".to_owned(),
        }
    );

    // A split that never answers times out, and its child is gone.
    let slow = dir.path().join("herdr-slow");
    let pid_file = dir.path().join("slow.pid");
    let body = format!(
        "#!/bin/sh\necho $$ > '{}'\nexec sleep 10\n",
        pid_file.display()
    );
    std::fs::write(&slow, body).expect("the slow fake");
    let mut permissions = std::fs::metadata(&slow).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&slow, permissions).expect("executable");
    let started = Instant::now();
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&slow),
        &PaneSettings::default(),
        &request(slow.clone(), None),
    )
    .await;
    assert_eq!(
        outcome,
        PaneOutcome::SplitFailed {
            multiplexer: "Herdr",
            reason: "timed out after 5 s".to_owned(),
        }
    );
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "the timeout took {:?}",
        started.elapsed()
    );
    let pid: String = std::fs::read_to_string(&pid_file)
        .expect("the child pid")
        .trim()
        .to_owned();
    let probed = Command::new("kill")
        .args(["-0", &pid])
        .output()
        .expect("kill -0 runs");
    assert!(
        !probed.status.success(),
        "the timed-out child {pid} is still alive"
    );
}

#[tokio::test]
async fn type_failure_leaves_pane_and_shows_command() {
    let dir = tempfile::tempdir().expect("a folder");
    let script = dir.path().join("herdr");
    let log = dir.path().join("herdr.log");
    // The split works; typing fails.
    let body = format!(
        "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done >> '{}'\necho \"\" >> '{}'\nif [ \"$2\" = \"split\" ]; then\nprintf '%s' '{{\"result\":{{\"pane\":{{\"pane_id\":\"w1:p7\"}}}}}}'\nexit 0\nfi\necho type refused >&2\nexit 1\n",
        log.display(),
        log.display()
    );
    std::fs::write(&script, body).expect("the fake script");
    let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("executable");
    let runner = talaria_app::panes::SystemRunner;
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&script),
        &PaneSettings::default(),
        &request(script.clone(), None),
    )
    .await;
    match outcome {
        PaneOutcome::TypeFailed {
            multiplexer,
            reason,
            command,
        } => {
            assert_eq!(multiplexer, "Herdr");
            assert_eq!(reason, "type refused");
            assert!(
                command.contains("attach 'local-ab' --profile 'fixture'"),
                "the shown command: {command:?}"
            );
        }
        other => panic!("typing failed but got {other:?}"),
    }
    let made = calls(&log);
    assert_eq!(made.len(), 2, "no pane was closed: {made:?}");
}

#[tokio::test]
async fn binary_gone_refused() {
    let dir = tempfile::tempdir().expect("a folder");
    let (fake, log) = write_fake(
        dir.path(),
        "herdr",
        "{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}",
        0,
        "",
    );
    let runner = talaria_app::panes::SystemRunner;
    let outcome = open_in_pane(
        &runner,
        &herdr_env(&fake),
        &PaneSettings::default(),
        &request(dir.path().join("no-such-talaria"), None),
    )
    .await;
    assert_eq!(outcome, PaneOutcome::Refused(PaneRefusal::BinaryGone));
    assert!(
        std::fs::read_to_string(&log).unwrap_or_default().is_empty(),
        "nothing may run before the binary check"
    );
}
