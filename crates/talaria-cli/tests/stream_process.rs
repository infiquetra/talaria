//! Stream mode at the process boundary.

mod support;

use portable_pty::{PtySize, native_pty_system};
use std::process::Stdio;
use std::time::Duration;

#[test]
fn not_started_writes_one_result_record() {
    let home = tempfile::tempdir().expect("home");
    let output = support::captured(
        support::talaria(
            &[
                "-p",
                "--no-ensure",
                "--output-format",
                "stream-json",
                "hello",
            ],
            home.path(),
        ),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(4));
    let text = std::str::from_utf8(&output.stdout).expect("UTF-8");
    assert_eq!(text.lines().count(), 1);
    let value: serde_json::Value = serde_json::from_str(text).expect("result");
    assert_eq!(value["type"], "result");
    assert_eq!(value["status"], "not_started");
    assert_eq!(value["exit_code"], 4);
}

#[test]
fn usage_errors_leave_stdout_empty() {
    let home = tempfile::tempdir().expect("home");
    let cases: &[(&[&str], &str)] = &[
        (
            &["--input-format", "text"],
            "talaria: --input-format only works with -p.\nNext step: add -p, or remove --input-format.\n",
        ),
        (
            &["-p", "--input-format", "bad"],
            "talaria: --input-format takes text or stream-json.\nNext step: use --input-format text or --input-format stream-json.\n",
        ),
        (
            &["-p", "--input-format", "stream-json"],
            "talaria: --input-format stream-json needs --output-format stream-json.\nNext step: add --output-format stream-json.\n",
        ),
        (
            &[
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "hello",
            ],
            "talaria: with --input-format stream-json, prompts come only from standard input.\nNext step: send {\"type\":\"user\",\"text\":\"...\"} lines on standard input.\n",
        ),
    ];
    for (args, expected) in cases {
        let output =
            support::captured(support::talaria(args, home.path()), Duration::from_secs(10));
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            *expected,
            "{args:?}"
        );
    }
    let pair = native_pty_system()
        .openpty(PtySize::default())
        .expect("pty");
    let stdin = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(pair.master.tty_name().expect("slave name"))
        .expect("slave");
    let mut command = support::talaria(
        &[
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ],
        home.path(),
    );
    command.stdin(Stdio::from(stdin));
    let output = support::captured(command, Duration::from_secs(10));
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria: --input-format stream-json reads standard input, which is a terminal.\nNext step: pipe JSON lines into talaria, or use --input-format text.\n"
    );
}

#[test]
fn help_names_stream_json() {
    let home = tempfile::tempdir().expect("home");
    let output = support::captured(
        support::talaria(&["--help"], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("stream-json"));
    assert!(help.contains("stream-v1.md"));
}
