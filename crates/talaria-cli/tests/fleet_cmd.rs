//! `talaria fleet list` against the mock gateway.

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use mock_gateway::StandaloneGateway;

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary home")
}

fn run(home: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("HERMES_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the talaria binary could not be started");
    let pid = child.id();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result.expect("the talaria binary could not be read"),
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("the fleet command did not exit");
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn text_and_json_output() {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_fleet_profiles(vec!["fixture".into()]);
    let hostile = format!("wide 宽 \u{1b}[31mred\u{202e}title");
    gateway.push_result(serde_json::json!({"sessions": [{
        "session_id": "local-3f2a",
        "id": "local-3f2a",
        "title": hostile,
        "source": "tui",
        "status": "idle",
        "message_count": 1,
        "model": "minimax-m3.1-flash",
        "preview": ""
    }]}));
    let output = run(home.path(), &["fleet", "list"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "exit");
    assert!(
        stdout.contains('\u{241b}'),
        "a colour introducer is shown as its stand-in"
    );
    assert!(
        stdout.contains("‹U+202E›"),
        "a direction override is shown as its stand-in"
    );
    assert!(
        !stdout.contains('\u{1b}'),
        "the raw colour introducer is not printed"
    );
    assert!(
        !stdout.contains('\u{202e}'),
        "the raw direction override is not printed"
    );
    assert!(stdout.contains('宽'), "a wide title is printed");
    assert!(
        stdout.contains("IDLE") || stdout.contains("idle"),
        "an idle group is printed"
    );
    let json_out = run(home.path(), &["fleet", "list", "--json"]);
    let json_text = text(&json_out.stdout);
    assert!(
        json_text.contains("\"format\":\"talaria.fleet.v1\""),
        "json uses the fleet list format"
    );
    assert!(
        json_text.contains("\"kind\":\"snapshot\""),
        "one snapshot object"
    );
    let _ = gateway;
}

#[test]
fn exit_codes_and_messages() {
    let empty = home();
    let missing = run(empty.path(), &["fleet", "list"]);
    let stderr = text(&missing.stderr);
    assert_eq!(missing.status.code(), Some(1), "no gateway exits 1");
    assert_eq!(
        stderr.trim_end(),
        "talaria fleet: no Hermes gateway is running for this Hermes home. Start one with \"hermes gateway ensure\", or start talaria."
    );
    assert!(text(&missing.stdout).is_empty(), "stdout stays empty");

    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_parked_profile("fixture-2", "draining");
    let unknown = run(home.path(), &["fleet", "list", "--profile", "fixture-9"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(
        text(&unknown.stderr).trim_end(),
        "talaria fleet: profile \"fixture-9\" is not served by this gateway (2 profiles are served; run without --profile to see them)."
    );
    let parked = run(home.path(), &["fleet", "list", "--profile", "fixture-2"]);
    assert_eq!(parked.status.code(), Some(2));
    assert!(
        text(&parked.stderr).contains("is parked by this gateway:"),
        "a parked profile names the gateway's reason"
    );

    let usage = run(home.path(), &["fleet", "list", "--for", "1"]);
    let usage_err = text(&usage.stderr);
    assert_eq!(
        usage.status.code(),
        Some(2),
        "--for without --watch exits 2"
    );
    assert!(
        usage_err.starts_with("error:"),
        "a usage error starts with error:"
    );
    let bad = run(home.path(), &["fleet", "list", "--not-a-flag"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(text(&bad.stderr).starts_with("error:"));
}

#[test]
fn profile_filter_hides_other_profiles() {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_fleet_profiles(vec!["default".into(), "fixture".into()]);
    let output = run(home.path(), &["fleet", "list", "--profile", "fixture"]);
    let stdout = text(&output.stdout);
    assert!(stdout.contains("fixture"), "the named profile is shown");
    assert!(
        !stdout.contains("default"),
        "another profile's name is not shown"
    );
}

#[test]
fn watch_lines_and_flag_rules() {
    let home = home();
    let _gateway = StandaloneGateway::start(home.path());
    let bare_for = run(home.path(), &["fleet", "list", "--for", "5"]);
    assert_eq!(
        bare_for.status.code(),
        Some(2),
        "--for without --watch exits 2"
    );

    let watched = run(home.path(), &["fleet", "list", "--watch", "--for", "1"]);
    let stdout = text(&watched.stdout);
    assert_eq!(watched.status.code(), Some(0), "--watch --for 1 exits 0");
    assert!(
        stdout.contains("needs you:"),
        "a new queue item prints needs you:"
    );
    assert!(stdout.contains("link:"), "a link change prints link:");
    assert!(stdout.contains("new ->"), "a new row prints new ->");
    assert!(
        stdout.contains("(untitled)"),
        "an empty title prints (untitled)"
    );
    assert!(
        stdout.lines().any(|line| {
            line.len() >= 12 && line.as_bytes()[2] == b':' && line.as_bytes()[5] == b':'
        }),
        "a watch line starts with the local time"
    );

    let json_watch = run(
        home.path(),
        &["fleet", "list", "--json", "--watch", "--for", "1"],
    );
    let json_text = text(&json_watch.stdout);
    for kind in [
        "snapshot",
        "row",
        "row_removed",
        "need_opened",
        "need_closed",
        "attention",
        "profile",
        "link",
    ] {
        assert!(
            json_text.contains(&format!("\"kind\":\"{kind}\"")),
            "json watch includes {kind}"
        );
    }
}
