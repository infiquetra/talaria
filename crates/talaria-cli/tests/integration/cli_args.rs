use std::process::Command;

use clap::{CommandFactory, Parser};
use talaria_app::launch::{StartMode, parse_effort};
use talaria_cli::args::Cli;

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(args)
}

#[test]
fn forms_defaults_and_refusals() {
    for args in [
        &["talaria", "--model", ""][..],
        &["talaria", "--model", " "],
        &["talaria", "--provider", ""],
        &["talaria", "--profile", ""],
        &["talaria", "--record", ""],
        &["talaria", "--record", "   "],
        &["talaria", "--resume", ""],
        &["talaria", "--resume", "  "],
        &["talaria", "--effort", ""],
        &["talaria", "-c", "--resume"],
        &["talaria", "--continue", "--resume", "local-session-1"],
    ] {
        assert!(parse(args).is_err(), "{args:?} must be a usage error");
    }

    let plain = parse(&["talaria"]).expect("no arguments");
    assert_eq!(plain.profile, "default");
    assert!(plain.model.is_none());
    assert!(plain.provider.is_none());
    assert!(plain.effort.is_none());
    assert!(!plain.continue_session);
    assert!(plain.resume.is_none());
    assert!(!plain.no_ensure);
    let launch = plain.into_launch().expect("new session");
    assert_eq!(launch.mode, StartMode::New);
    assert_eq!(launch.profile, "default");
    assert!(launch.folder.is_absolute());

    let continued = parse(&[
        "talaria",
        "-c",
        "--model",
        "minimax-m3.1-flash",
        "--effort",
        "high",
    ])
    .expect("continue parses; the option clash is reported by the launch");
    let error = continued.into_launch().expect_err("new-session options");
    match error {
        talaria_app::launch::UsageError::NewSessionOptionsWithResume { given } => {
            assert_eq!(given, ["--model", "--effort"]);
        }
        other => panic!("expected the new-session option refusal, got {other:?}"),
    }

    let listed = parse(&["talaria", "--resume"]).expect("resume list");
    assert_eq!(listed.resume, Some(None));
    let listed = listed.into_launch().expect("list");
    assert_eq!(listed.mode, StartMode::Resume(None));

    let named = parse(&[
        "talaria",
        "--resume",
        " local-session-1 ",
        "--profile",
        "fixture",
    ])
    .expect("resume id");
    let named = named.into_launch().expect("resume");
    assert_eq!(
        named.mode,
        StartMode::Resume(Some("local-session-1".to_string()))
    );
    assert_eq!(named.profile, "fixture");
}

#[test]
fn effort_values_checked() {
    let levels = [
        "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
    ];
    for level in levels {
        let cli = parse(&["talaria", "--effort", level]).expect(level);
        assert_eq!(cli.effort.as_deref(), Some(level));
        assert_eq!(parse_effort(level).expect(level), level);
    }
    for refused in ["hgih", "HIGH", "false", "disabled", "High"] {
        let error = parse_effort(refused).expect_err(refused);
        let rendered = talaria_app::failure::LaunchFailure::from(error).render();
        let expected = format!(
            "talaria: --effort must be one of none, minimal, low, medium, high, xhigh, max, ultra (got {refused}).\n\
             Next step: Pick one of those values.\n"
        );
        assert_eq!(rendered, expected);
    }
}

#[test]
fn flags_help_and_neutralised_errors() {
    let help = Cli::command().render_long_help().to_string();
    for flag in [
        "--record",
        "--bench-replay",
        "--bench-speed",
        "--bench-report",
        "--no-ensure",
        "--effort",
    ] {
        assert!(help.contains(flag), "{flag} missing from help:\n{help}");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .arg("--bo\u{1b}[31mgus")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("spawn");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("--bo?[31mgus"), "{stderr}");
    assert_eq!(
        stderr
            .bytes()
            .filter(|byte| *byte < 0x20 && *byte != b'\n')
            .count(),
        0,
        "{stderr}"
    );
    assert_eq!(
        stderr.bytes().filter(|byte| *byte == b'\n').count(),
        1,
        "{stderr}"
    );

    let long = format!("--{}", "a".repeat(80));
    let long_output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .arg(&long)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("spawn");
    let long_err = String::from_utf8_lossy(&long_output.stderr);
    assert!(!long_err.contains(&"a".repeat(80)), "{long_err}");
    assert_eq!(long_output.status.code(), Some(2));

    let plain = parse(&["talaria"]).expect("plain");
    let _ = plain.into_launch().expect("still a stub");
}
