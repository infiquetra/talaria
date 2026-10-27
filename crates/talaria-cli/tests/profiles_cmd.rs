//! `talaria profiles`: arguments, outputs and exits. Issue #253.

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use mock_gateway::StandaloneGateway;
use serde_json::json;
use talaria_cli::profiles_cmd::{ProfilesArgs, parse, render_json, render_text};
use talaria_core::profiles::{ProfileRow, ServeState};

fn argv(tail: &[&str]) -> Vec<String> {
    let mut args = vec!["talaria".to_owned(), "profiles".to_owned()];
    args.extend(tail.iter().map(ToString::to_string));
    args
}

fn served_row(name: &str, model: Option<(&str, &str)>) -> ProfileRow {
    ProfileRow {
        name: name.to_owned(),
        display_name: name.to_owned(),
        is_default: name == "default",
        model: model.map(|(model, _)| model.to_owned()),
        provider: model.map(|(_, provider)| provider.to_owned()),
        in_roster: true,
        state: ServeState::Served,
    }
}

fn run_profiles(home: &std::path::Path, tail: &[&str]) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_talaria");
    let child = std::process::Command::new(binary)
        .arg("profiles")
        .args(tail)
        .env("HERMES_HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the command runs");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the command ends")
        .expect("the command is read")
}

#[test]
fn profiles_command_output() {
    // Arguments: the flag, normalised names, duplicates dropped.
    assert_eq!(
        parse(&argv(&[])).expect("bare"),
        ProfilesArgs {
            json: false,
            names: vec![]
        }
    );
    assert_eq!(
        parse(&argv(&["--json"])).expect("json"),
        ProfilesArgs {
            json: true,
            names: vec![]
        }
    );
    assert_eq!(
        parse(&argv(&[" Fixture ", "fixture", "nosuchprofile", "--json"]))
            .expect("names")
            .names,
        ["fixture", "nosuchprofile"]
    );
    let error = parse(&argv(&["--bogus"])).expect_err("an unknown flag fails");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);

    // The text table, byte for byte: widths 7, 6 and 33, typed order kept.
    let rows = vec![
        served_row("fixture", None),
        served_row("default", Some(("minimax-m3.1-flash", "ollama-cloud"))),
    ];
    assert_eq!(
        render_text(&rows),
        "NAME     STATE   MODEL                              NEXT STEP\n\
         fixture  served  -                                  talaria --profile fixture\n\
         default  served  minimax-m3.1-flash · ollama-cloud  talaria --profile default\n"
    );

    // The JSON document, byte for byte: reasons only where they exist, models
    // only when known, one line without a trailing newline.
    let rows = vec![
        ProfileRow {
            name: "fixture-b".to_owned(),
            display_name: "fixture-b".to_owned(),
            is_default: false,
            model: None,
            provider: None,
            in_roster: true,
            state: ServeState::Parked {
                reason: "session store unusable: <home>/profiles/fixture-b/state.db".to_owned(),
            },
        },
        ProfileRow {
            name: "fixture-c".to_owned(),
            display_name: "fixture-c".to_owned(),
            is_default: false,
            model: None,
            provider: None,
            in_roster: true,
            state: ServeState::Stopped,
        },
        ProfileRow {
            name: "fixture-d".to_owned(),
            display_name: "fixture-d".to_owned(),
            is_default: false,
            model: None,
            provider: None,
            in_roster: true,
            state: ServeState::NeverDiscovered,
        },
    ];
    assert_eq!(
        render_json(&rows, &[]),
        "{\"profiles\":[{\"name\":\"fixture-b\",\"state\":\"parked\",\"reason\":\"session store unusable: <home>/profiles/fixture-b/state.db\",\"next_step\":\"fix the reason, then rescan with R in /profiles\"},{\"name\":\"fixture-c\",\"state\":\"stopped\",\"next_step\":\"hermes -p fixture-c gateway start\"},{\"name\":\"fixture-d\",\"state\":\"not_discovered\",\"next_step\":\"rescan with R in /profiles\"}],\"warnings\":[]}"
    );

    // Against a gateway the table prints and the exit is 0.
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let roster = json!({
        "profiles": [
            {"name": "default", "path": "<home>", "is_default": true,
             "model": "minimax-m3.1-flash", "provider": "ollama-cloud",
             "description": "", "display_name": "default", "skill_count": 44,
             "ui_meta_revisions": {}, "has_avatar": false},
            {"name": "fixture", "path": "<home>/profiles/<profile>", "is_default": false,
             "model": "", "provider": "", "description": "", "display_name": "fixture",
             "skill_count": 0},
        ],
        "bot_mode_protocol": true,
        "install_id": "[redacted]",
    });
    gateway.push_matched_result(
        "profiles.list",
        json!({"include_sessions": false}),
        roster.clone(),
    );
    let output = run_profiles(home.path(), &["fixture", "default"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "NAME     STATE   MODEL                              NEXT STEP\n\
         fixture  served  -                                  talaria --profile fixture\n\
         default  served  minimax-m3.1-flash · ollama-cloud  talaria --profile default\n"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("talaria profiles: Some reasons are unavailable"),
        "the topology soft failure shows: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Unknown names exit 2 with one line each, in typed order, and no output.
    gateway.push_matched_result("profiles.list", json!({"include_sessions": false}), roster);
    let output = run_profiles(home.path(), &["nosuchprofile", "fixture"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria profiles: no profile named nosuchprofile\n"
    );

    // Without a gateway the exit is 1, nothing prints, and nothing is written.
    let empty = tempfile::tempdir().expect("an empty home");
    let output = run_profiles(empty.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria profiles: no Hermes gateway is running for this home\n\
         Next step: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.\n"
    );
    assert!(
        std::fs::read_dir(empty.path())
            .expect("read home")
            .next()
            .is_none(),
        "the command wrote nothing"
    );
}
