//! The headless command line: forms, print-only options, and refusals.

use clap::Parser;
use talaria_cli::args::{Cli, validate_print_options};

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("the form parses")
}

#[test]
fn print_forms_parse() {
    for args in [
        ["talaria", "-p", "hello"].as_slice(),
        ["talaria", "--print", "hello"].as_slice(),
        ["talaria", "-p"].as_slice(),
        ["talaria", "-p", "hi", "--output-format", "json"].as_slice(),
        ["talaria", "-p", "hi", "--output-format", "text"].as_slice(),
        ["talaria", "-p", "hi", "--timeout", "30"].as_slice(),
        ["talaria", "-p", "-c", "hello"].as_slice(),
        ["talaria", "-p", "--resume", "local-session-1", "hello"].as_slice(),
        [
            "talaria",
            "-p",
            "--model",
            "m",
            "--provider",
            "p",
            "--effort",
            "high",
            "hi",
        ]
        .as_slice(),
        [
            "talaria",
            "-p",
            "--profile",
            "fixture",
            "--no-ensure",
            "--record",
            "rec.jsonl",
            "hi",
        ]
        .as_slice(),
    ] {
        let cli = parse(args);
        assert!(cli.print, "{args:?}");
        validate_print_options(&cli).expect("a valid form");
    }
}

#[test]
fn print_only_options_need_print() {
    for (args, option) in [
        (
            ["talaria", "hello"].as_slice(),
            "a prompt on the command line",
        ),
        (
            ["talaria", "--output-format", "json"].as_slice(),
            "--output-format",
        ),
        (["talaria", "--timeout", "5"].as_slice(), "--timeout"),
    ] {
        let cli = parse(args);
        let error = validate_print_options(&cli).expect_err("needs -p");
        assert_eq!(error.what, format!("{option} only works with -p."));
        assert_eq!(error.next_step, format!("Add -p, or drop {option}."));
    }
}

#[test]
fn print_refusals_exit_2() {
    // Bare --resume under -p needs an id.
    let cli = parse(&["talaria", "-p", "--resume"]);
    let error = validate_print_options(&cli).expect_err("resume needs an id");
    assert_eq!(error.what, "talaria -p needs a session id after --resume.");
    // Streaming output has its own refusal.
    let cli = parse(&["talaria", "-p", "--output-format", "stream-json", "hi"]);
    let error = validate_print_options(&cli).expect_err("stream-json refused");
    assert_eq!(error.what, "streaming JSON output is not available yet.");
    assert_eq!(error.next_step, "Use --output-format json.");
    // Other bad formats and timeouts are refused too.
    for args in [
        ["talaria", "-p", "--output-format", "yaml", "hi"].as_slice(),
        ["talaria", "-p", "--timeout", "0", "hi"].as_slice(),
        ["talaria", "-p", "--timeout", "99999", "hi"].as_slice(),
        ["talaria", "-p", "--timeout", "soon", "hi"].as_slice(),
        ["talaria", "-p", "--timeout", "-5", "hi"].as_slice(),
    ] {
        let cli = parse(args);
        validate_print_options(&cli).expect_err("a bad value");
    }
}
