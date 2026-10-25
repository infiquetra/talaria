//! The headless binary with pipes: failures, JSON, usage, hints, stops.

mod support;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("home")
}

fn print_command(args: &[&str], home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .args(args)
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("HERMES_HOME", home)
        .env("TMPDIR", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn piped_command(args: &[&str], home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .args(args)
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("HERMES_HOME", home)
        .env("TMPDIR", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

#[test]
fn no_gateway_exit_4_without_terminal() {
    let home = home();
    let output = support::captured(
        print_command(&["-p", "--no-ensure", "hello"], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(
        output.status.code(),
        Some(4),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stderr.contains(&0x1b));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("talaria: "));
    assert!(stderr.contains("Next step: "));
}

#[test]
fn json_mode_one_object_on_failure() {
    let home = home();
    let output = support::captured(
        print_command(
            &["-p", "--no-ensure", "--output-format", "json", "hello"],
            home.path(),
        ),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(4));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON object");
    assert_eq!(value["status"], "not_started");
    assert_eq!(value["exit_code"], 4);
    let home_text = home.path().display().to_string();
    assert!(!stdout.contains(&home_text));
    assert!(!stdout.contains("ticket"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(&home_text));
    assert!(!stderr.contains("ticket"));
}

#[test]
fn usage_errors_leave_stdout_empty() {
    let home = home();
    for args in [&["-p", "--no-ensure", ""][..], &["--timeout", "5"][..]] {
        let output = support::captured(print_command(args, home.path()), Duration::from_secs(10));
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn interactive_no_terminal_hint_names_print() {
    let home = home();
    let output = support::captured(
        print_command(&["--no-ensure"], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr,
        "talaria: a terminal is required on standard input and standard output.\n\
         Next step: Run it in a terminal pane, or use talaria -p \"<prompt>\" in scripts.\n"
    );
}

#[test]
fn stop_while_reading_stdin() {
    let home = home();
    // An open pipe that never sends: SIGINT ends the read at once.
    let mut child = piped_command(&["-p", "--no-ensure"], home.path())
        .spawn()
        .expect("spawn");
    let _stdin = child.stdin.take().expect("stdin");
    std::thread::sleep(Duration::from_millis(500));
    support::signal(child.id(), "-INT");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(130));
    // The same open pipe: --timeout bounds the read.
    let mut child = piped_command(&["-p", "--no-ensure", "--timeout", "1"], home.path())
        .spawn()
        .expect("spawn");
    let _stdin = child.stdin.take().expect("stdin");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(124));
}
