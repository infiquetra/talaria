//! `talaria fleet list` against the mock gateway.

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use mock_gateway::StandaloneGateway;
use talaria_app::fleet::{FleetLinkState, FleetSnapshot};
use talaria_app::side_connection::SideScope;
use talaria_cli::fleet_cmd;
use talaria_core::attention::{AlertKind, NeedSummary};
use talaria_core::fleet::{
    FleetRow, NeedDetail, NeedId, NeedItem, NeedSlot, ProfileStatus, RowKey, RowState,
};

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary home")
}

fn run(home: &Path, args: &[&str]) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_talaria"))
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
    let hostile = "wide 宽 \u{1b}[31mred\u{202e}title".to_string();
    gateway.set_fleet_session(
        "fixture",
        serde_json::json!({
            "id": "local-3f2a",
            "title": hostile,
            "source": "tui",
            "status": "idle",
            "message_count": 1,
            "model": "minimax-m3.1-flash",
            "started_at": 1_792_690_000.0
        }),
    );
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
    for profile in ["default", "fixture"] {
        gateway.set_fleet_session(
            profile,
            serde_json::json!({
                "id": format!("{profile}-session"),
                "title": "",
                "status": "idle",
                "message_count": 1,
                "model": "minimax-m3.1-flash"
            }),
        );
    }
    let output = run(home.path(), &["fleet", "list", "--profile", "fixture"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "exit");
    assert!(
        stdout.contains("  fixture"),
        "the named profile's row is shown"
    );
    assert!(
        !stdout.contains("default"),
        "another profile's name is not shown"
    );
    let json_out = run(
        home.path(),
        &["fleet", "list", "--json", "--profile", "fixture"],
    );
    let json_text = text(&json_out.stdout);
    assert!(json_text.contains("fixture-session"), "{json_text}");
    assert!(
        !json_text.contains("default"),
        "another profile's name is not in the json"
    );
}

fn row(session: &str, state: RowState, stale: bool) -> FleetRow {
    FleetRow {
        terminal_location: None,
        key: key(session),
        title: String::new(),
        model: Some("minimax-m3.1-flash".into()),
        source: Some("tui".into()),
        started_at: Some(1_792_690_000.0),
        message_count: 1,
        state,
        state_since: 1_792_690_000.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn key(session: &str) -> RowKey {
    RowKey {
        profile: "fixture".into(),
        session_id: session.into(),
    }
}

fn need(session: &str, seq: u64) -> NeedItem {
    NeedItem {
        id: NeedId {
            key: key(session),
            slot: NeedSlot::Prompt("a1b2".into()),
        },
        seq,
        since: 1_792_690_001.0,
        detail: NeedDetail::Known(NeedSummary::Approval {
            command: "rm -rf ./talaria-fleet-probe".into(),
            description: String::new(),
        }),
        choices: vec!["once".into(), "deny".into()],
        execution_generation: None,
        timeout_s: None,
        exact: false,
        foreground: false,
        external_options: Vec::new(),
    }
}

fn snapshot(rows: Vec<FleetRow>, queue: Vec<NeedItem>, link: FleetLinkState) -> FleetSnapshot {
    FleetSnapshot {
        version: 1,
        scope: SideScope::Host {
            profiles: vec!["fixture".into()],
        },
        link,
        profiles: vec![("fixture".into(), ProfileStatus::Listed { truncated: false })],
        rows,
        queue,
        counts: Default::default(),
        diagnostics: Vec::new(),
        as_of: 1_792_690_002.0,
    }
}

#[test]
fn watch_lines_and_flag_rules() {
    let clock = "12:00:00.000";
    let first = snapshot(
        vec![
            row("s1", RowState::Working, false),
            row("gone", RowState::Idle, false),
        ],
        Vec::new(),
        FleetLinkState::Connected,
    );
    let waiting = snapshot(
        vec![
            row("s1", RowState::NeedsYou, true),
            row("s2", RowState::Working, false),
        ],
        vec![need("s1", 1)],
        FleetLinkState::Reconnecting {
            reason: "connection closed".into(),
        },
    );
    let lines = fleet_cmd::watch_lines(&first, &waiting, clock);
    assert_eq!(
        lines,
        vec![
            "12:00:00.000 fixture (untitled) working -> needs you".to_string(),
            "12:00:00.000 fixture (untitled) new -> working".to_string(),
            "12:00:00.000 needs you: fixture (untitled) approval: rm -rf ./talaria-fleet-probe"
                .to_string(),
            "12:00:00.000 link: reconnecting: connection closed".to_string(),
        ],
        "a removed row and a stale flip print nothing"
    );
    let answered = snapshot(
        vec![
            row("s1", RowState::Working, true),
            row("s2", RowState::Working, false),
        ],
        Vec::new(),
        FleetLinkState::Connected,
    );
    assert_eq!(
        fleet_cmd::watch_lines(&waiting, &answered, clock),
        vec![
            "12:00:00.000 fixture (untitled) needs you -> working".to_string(),
            "12:00:00.000 cleared: fixture (untitled)".to_string(),
            "12:00:00.000 link: connected".to_string(),
        ]
    );
    let mut refined_from = snapshot(
        vec![row("s1", RowState::NeedsYou, false)],
        vec![need("s1", 4)],
        FleetLinkState::Connected,
    );
    refined_from.queue[0].id.slot = NeedSlot::Waiting;
    refined_from.queue[0].detail = NeedDetail::Waiting;
    let refined_to = snapshot(
        vec![row("s1", RowState::NeedsYou, false)],
        vec![need("s1", 4)],
        FleetLinkState::Connected,
    );
    assert!(
        fleet_cmd::watch_lines(&refined_from, &refined_to, clock).is_empty(),
        "a refined need prints no line"
    );

    let mut kinds: Vec<String> = fleet_cmd::watch_json(&first, &waiting)
        .into_iter()
        .chain(fleet_cmd::watch_json(&waiting, &answered))
        .chain(std::iter::once(fleet_cmd::attention_json(
            AlertKind::NeedsYou,
            &key("s1"),
        )))
        .chain(std::iter::once(fleet_cmd::snapshot_json(&first)))
        .map(|line| {
            assert_eq!(line["format"], "talaria.fleet.v1");
            line["kind"].as_str().unwrap_or_default().to_string()
        })
        .collect();
    let mut profile_changed = waiting.clone();
    profile_changed.profiles[0].1 = ProfileStatus::Loading;
    kinds.extend(
        fleet_cmd::watch_json(&waiting, &profile_changed)
            .into_iter()
            .map(|line| line["kind"].as_str().unwrap_or_default().to_string()),
    );
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
            kinds.iter().any(|seen| seen == kind),
            "json watch includes {kind}"
        );
    }

    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_fleet_session(
        "fixture",
        serde_json::json!({"id": "local-3f2a", "title": "", "status": "idle", "message_count": 1}),
    );
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
        stdout.contains("IDLE (1)"),
        "the first snapshot is printed: {stdout}"
    );
    let json_watch = run(
        home.path(),
        &["fleet", "list", "--json", "--watch", "--for", "1"],
    );
    assert_eq!(json_watch.status.code(), Some(0));
    let first_line = text(&json_watch.stdout);
    assert!(
        first_line
            .lines()
            .next()
            .is_some_and(|line| line.contains("\"kind\":\"snapshot\"")),
        "json watch starts with a snapshot: {first_line}"
    );
}
