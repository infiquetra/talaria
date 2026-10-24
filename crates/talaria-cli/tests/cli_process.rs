mod support;

use std::process::Command;
use std::time::{Duration, Instant};

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("home")
}

#[test]
fn not_a_terminal_exits_2_without_escape_bytes() {
    let home = home();
    let output = support::captured(support::talaria(&[], home.path()), Duration::from_secs(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert_eq!(
        stderr,
        "talaria: a terminal is required on standard input and standard output.\n\
         Next step: Run it in a terminal pane. Scripted use (talaria -p) arrives in a later release.\n"
    );
    assert_eq!(
        stderr
            .bytes()
            .filter(|byte| *byte < 0x20 && *byte != b'\n')
            .count(),
        0
    );
    assert!(output.stdout.is_empty(), "stdout");
}

#[test]
fn no_gateway_exits_1_and_restores_the_terminal() {
    let home = home();
    let missing = support::captured(
        support::talaria(&["--no-ensure"], home.path()),
        Duration::from_secs(2),
    );
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert_eq!(missing.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("talaria: "), "{stderr}");
    assert!(stderr.contains("\nNext step: "), "{stderr}");
    assert_eq!(
        stderr
            .bytes()
            .filter(|byte| *byte < 0x20 && *byte != b'\n')
            .count(),
        0
    );

    let record = home.path().join("session.jsonl");
    std::fs::write(&record, "already\n").expect("record");
    let existing = support::captured(
        support::talaria(
            &["--no-ensure", "--record", record.to_str().expect("utf8")],
            home.path(),
        ),
        Duration::from_secs(2),
    );
    let stderr = String::from_utf8_lossy(&existing.stderr);
    assert_eq!(existing.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("the recording file already exists."),
        "{stderr}"
    );
    assert!(!stderr.contains("Connecting"), "{stderr}");
}

#[test]
fn first_frame_before_the_gateway_answers_and_typing_kept() {
    let home = home();
    let started = Instant::now();
    let screen = support::pty_run(
        &["--no-ensure"],
        home.path(),
        30,
        100,
        "hello",
        Duration::from_millis(500),
    );
    assert!(
        screen.text.contains("Connecting"),
        "first frame missing after {:?}: {}",
        started.elapsed(),
        screen.text
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert!(
        screen.text.contains("hello"),
        "typed text was dropped: {}",
        screen.text
    );
}

#[test]
fn quit_restores_terminal_and_prints_resume_hint() {
    let home = home();
    let screen = support::pty_run(
        &["--no-ensure", "--profile", "fixture"],
        home.path(),
        30,
        100,
        "/quit\r",
        Duration::from_secs(2),
    );
    assert_eq!(
        screen.exit_code,
        Some(0),
        "{}\n{}\n{:?}",
        screen.text,
        screen.termios,
        screen.signal
    );
    assert!(
        screen.text.contains("Resume this session with:"),
        "{}",
        screen.text
    );
    assert!(screen.text.contains("talaria --resume "), "{}", screen.text);
    assert!(screen.text.contains("--profile fixture"), "{}", screen.text);
    assert!(!screen.text.contains('\u{1b}'), "{}", screen.text);
}

#[test]
fn sigterm_and_sighup_restore_the_terminal() {
    assert_eq!(signalled(&["--no-ensure"], "-TERM"), Some(143));
    assert_eq!(signalled(&["--no-ensure"], "-HUP"), Some(129));

    let home = home();
    let bin = env!("CARGO_BIN_EXE_talaria");
    let script = format!("{bin} --resume; echo status=$?");
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(script)
        .current_dir(home.path())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home.path())
        .env("HERMES_HOME", home.path());
    let output = support::captured(command, Duration::from_secs(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("status=130"), "{stdout}");
}

fn signalled(args: &[&str], signal: &str) -> Option<i32> {
    let home = home();
    let mut child = support::talaria(args, home.path()).spawn().expect("spawn");
    std::thread::sleep(Duration::from_millis(50));
    if child.try_wait().expect("try_wait").is_none() {
        support::signal(child.id(), signal);
    }
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status.code();
        }
        if started.elapsed() > Duration::from_secs(2) {
            let _ = child.kill();
            return child.wait().ok().and_then(|status| status.code());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
