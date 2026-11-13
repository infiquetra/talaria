//! Stream format parsing and input refusals.

use clap::Parser;
use talaria_cli::args::{Cli, validate_print_options};

#[test]
fn stream_formats_parse() {
    let default = Cli::try_parse_from(["talaria", "-p", "hello"]).expect("default arguments");
    assert_eq!(default.input_format.as_deref().unwrap_or("text"), "text");
    assert_eq!(default.output_format.as_deref().unwrap_or("text"), "text");
    for args in [
        &["talaria", "-p", "hello", "--output-format", "stream-json"][..],
        &[
            "talaria",
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ][..],
        &["talaria", "-p", "hello", "--input-format", "text"][..],
    ] {
        let cli = Cli::try_parse_from(args).expect("stream grammar");
        validate_print_options(&cli).expect("valid stream format");
    }
}

#[test]
fn input_format_usage_errors() {
    let cases: &[(&[&str], &str, &str)] = &[
        (
            &["talaria", "--input-format", "text"],
            "--input-format only works with -p.",
            "add -p, or remove --input-format.",
        ),
        (
            &["talaria", "-p", "--input-format", "yaml"],
            "--input-format takes text or stream-json.",
            "use --input-format text or --input-format stream-json.",
        ),
        (
            &["talaria", "-p", "--input-format", "stream-json"],
            "--input-format stream-json needs --output-format stream-json.",
            "add --output-format stream-json.",
        ),
        (
            &[
                "talaria",
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "hi",
            ],
            "with --input-format stream-json, prompts come only from standard input.",
            "send {\"type\":\"user\",\"text\":\"...\"} lines on standard input.",
        ),
    ];
    for (args, what, next_step) in cases {
        let cli = Cli::try_parse_from(*args).expect("grammar parses");
        let err = validate_print_options(&cli).expect_err("usage must be refused");
        assert_eq!(&err.what, what, "{args:?}");
        assert_eq!(&err.next_step, next_step, "{args:?}");
    }
    use portable_pty::{PtySize, native_pty_system};
    use std::process::{Command, Stdio};
    let pair = native_pty_system()
        .openpty(PtySize::default())
        .expect("pty");
    let tty = pair.master.tty_name().expect("slave name");
    let stdin = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(tty)
        .expect("slave");
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ])
        .stdin(Stdio::from(stdin))
        .output()
        .expect("terminal input process");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria: --input-format stream-json reads standard input, which is a terminal.\nNext step: pipe JSON lines into talaria, or use --input-format text.\n"
    );
}
