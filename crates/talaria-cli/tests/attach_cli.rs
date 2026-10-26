//! The `attach` subcommand through the binary. Issue #246.

mod support;

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use mock_gateway::{StandaloneGateway, list_row};
use serde_json::json;
use support::Launch;

const NOW: f64 = 1_792_700_000.0;

fn temp_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir(home.path().join(".talaria")).expect("config folder");
    std::fs::write(
        home.path().join(".talaria/config.toml"),
        "[fleet]\nenabled = false\n",
    )
    .expect("disable unrelated fleet polling");
    home
}

fn full_id(hex8: &str) -> String {
    format!("local-{hex8}{}", "0".repeat(56))
}

fn type_line(terminal: &mut support::Terminal, text: &str) {
    terminal.type_keys(text);
    assert!(
        terminal.wait_for_text(text, Duration::from_secs(10)),
        "draft was not drawn"
    );
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

#[test]
fn usage_errors_exit_2_with_empty_stdout() {
    // Each case: arguments, and the token standard error must name.
    let home = temp_home();
    let none: &[&str] = &["attach"];
    let clap_cases: Vec<(Vec<&str>, &str)> = vec![
        (none.to_vec(), "<SESSION>"),
        (vec!["attach", "probe", "--model", "x"], "--model"),
        (vec!["attach", "probe", "-c", "y"], "-c"),
        (vec!["attach", "probe", "--resume", "y"], "--resume"),
        (vec!["attach", "first-ref", "second-ref"], "second-ref"),
        (vec!["attach", "probe", "--profile", ""], "--profile"),
    ];
    for (args, token) in clap_cases {
        let output = support::captured(
            support::talaria(&args, home.path()),
            Duration::from_secs(10),
        );
        assert_eq!(output.status.code(), Some(2), "{args:?} must exit 2");
        assert!(output.stdout.is_empty(), "{args:?} wrote to stdout");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("error: "),
            "{args:?} stderr was {stderr:?}"
        );
        assert!(
            stderr.contains(token),
            "{args:?} stderr names no {token}: {stderr:?}"
        );
    }

    // A blank reference is the exact A1 text, not clap's.
    let output = support::captured(
        support::talaria(&["attach", "  "], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria: the session to attach to is empty.\nNext step: pass a session id, the start of one, or a title.\n"
    );
}

#[test]
fn no_terminal_exits_2_before_network() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let output = support::captured(
        support::talaria(&["attach", "abc"], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "talaria: a terminal is required on standard input and standard output.\nNext step: Run it in a terminal pane, or use talaria -p \"<prompt>\" in scripts.\n"
    );
    assert!(gateway.control_requests().is_empty());
    assert_eq!(gateway.websocket_connections(), 0);
}

#[test]
fn fast_path_lists_nothing_and_resumes() {
    let id = full_id("1a2b3c4d");
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let mut snap = mock_gateway::recorded_resume_snapshot();
    snap["prompts"] = json!([]);
    snap["session_id"] = json!(id);
    snap["stored_session_id"] = json!(id);
    gateway.set_resume_snapshot(snap);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    let resume = gateway
        .wait_request("session.resume", 0, Duration::from_secs(10))
        .expect("the fast path resumes");
    assert_eq!(
        resume.params,
        json!({"session_id": id, "display_projection": true})
    );
    assert!(
        gateway.requests("session.list").is_empty(),
        "the fast path listed"
    );
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "no resumed line on the screen"
    );
    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(10));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);

    // An unserved profile on the fast path is the resume path's own failure
    // (issue #209), not the unserved-profile refusal.
    for profile in ["nosuch", ".."] {
        let case_home = temp_home();
        let gateway = StandaloneGateway::start(case_home.path());
        let mut terminal = Launch::new(case_home.path())
            .args(&["attach", &id, "--profile", profile])
            .spawn();
        assert!(
            terminal.wait_for_text(
                &format!("never discovered a profile named {profile}"),
                Duration::from_secs(10),
            ),
            "no unserved-profile text for {profile}"
        );
        let exit = terminal.wait_exit(Duration::from_secs(10));
        assert_eq!(exit.code, Some(1), "profile {profile}");
        assert!(
            gateway.requests("session.list").is_empty(),
            "an unserved fast path listed"
        );
        assert!(
            gateway.requests("session.resume").is_empty(),
            "an unserved fast path resumed"
        );
    }
}

#[test]
fn prefix_resolves_then_opens_like_resume() {
    let home = temp_home();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).expect("bin");
    let counter = home.path().join("ensure.count");
    let gateway = StandaloneGateway::start_absent(home.path(), &counter);
    mock_gateway::write_ensure_stand_in(&bin, &counter, home.path());

    let id = full_id("9e8d7c6b");
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![list_row(
            &id,
            "attach probe",
            Some(NOW - 30.0),
            None,
            4,
            false,
        )],
    );
    let mut snap = mock_gateway::recorded_resume_snapshot();
    snap["prompts"] = json!([]);
    snap["session_id"] = json!(id);
    snap["stored_session_id"] = json!(id);
    snap["info"]["title"] = json!("attach probe");
    gateway.set_resume_snapshot(snap);

    let mut terminal = Launch::new(home.path())
        .path_prefix(&bin)
        .args(&["attach", "9e8d7c6b"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: attach probe ──", Duration::from_secs(20)),
        "the prefix never opened"
    );
    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(10));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);

    // Ensure ran once for the attach; the handover to the resume ran none.
    let runs = std::fs::read_to_string(&counter).expect("the counter file");
    assert_eq!(runs.lines().count(), 1, "ensure ran {runs:?}");
}

#[test]
fn empty_attached_session_announces_its_title() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = full_id("ab12cd34");
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["messages"] = json!([]);
    snapshot["display_messages"] = json!([]);
    snapshot["message_count"] = json!(0);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    snapshot["info"]["title"] = json!("empty attach");
    gateway.set_resume_snapshot(snapshot);
    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(terminal.wait_for_text("── Resumed: empty attach ──", Duration::from_secs(5)));
    type_line(&mut terminal, "/quit");
    assert_eq!(terminal.finish(Duration::from_secs(10)).exit_code, Some(0));
}

fn run_with_captured_stderr(
    home: &std::path::Path,
    args: &[&str],
    err_name: &str,
) -> (Option<u32>, String, String) {
    let err_file = home.join(err_name);
    let mut script = format!("exec '{}'", env!("CARGO_BIN_EXE_talaria"));
    for arg in args {
        script.push_str(&format!(" '{arg}'"));
    }
    script.push_str(&format!(" 2>'{}'", err_file.display()));
    let mut terminal = Launch::new(home).script(&script).spawn();
    let exit = terminal.wait_exit(Duration::from_secs(15));
    terminal.settle();
    let screen = terminal.screen_text();
    let stderr = std::fs::read_to_string(&err_file).unwrap_or_default();
    (exit.code, stderr, screen)
}

#[test]
fn not_found_and_ambiguous_texts_and_exit_codes() {
    // No match: A2 on standard error, exit 1, nothing on standard output.
    let home = temp_home();
    let _gateway = StandaloneGateway::start(home.path());
    let (code, stderr, screen) =
        run_with_captured_stderr(home.path(), &["attach", "zz-none-1"], "a2.txt");
    assert_eq!(code, Some(1));
    assert_eq!(
        stderr,
        "talaria: no session matches \"zz-none-1\" in the profiles default, fixture.\nNext step: run \"talaria fleet list\" or \"talaria --resume\" to see sessions, or pass the full id with --profile <name>.\n"
    );
    assert!(!screen.contains("talaria:"), "stdout held {screen:?}");

    // Several matches: A3 on standard error, exit 2.
    let home_many = temp_home();
    let gateway = StandaloneGateway::start(home_many.path());
    gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![
            list_row("local-aa01", "dup", None, None, 1, false),
            list_row("local-bb02", "dup", None, None, 1, false),
        ],
    );
    let (code, stderr, screen) =
        run_with_captured_stderr(home_many.path(), &["attach", "dup"], "a3.txt");
    assert_eq!(code, Some(2));
    assert_eq!(
        stderr,
        "talaria: 2 sessions match \"dup\":\n  fixture  local-aa01  \"dup\"  live  —\n  fixture  local-bb02  \"dup\"  live  —\nNext step: use more of the id, or add --profile <name>.\n"
    );
    assert!(!screen.contains("talaria:"), "stdout held {screen:?}");

    // A reference starting with `-` after `--` is a reference, not an option.
    let home_dash = temp_home();
    let _gateway = StandaloneGateway::start(home_dash.path());
    let (code, stderr, screen) = run_with_captured_stderr(
        home_dash.path(),
        &["attach", "--", "-zz-none-1"],
        "dash.txt",
    );
    assert_eq!(code, Some(1));
    assert!(
        stderr.starts_with("talaria: no session matches \"-zz-none-1\""),
        "dash reference was a usage error: {stderr:?}"
    );
    assert!(!screen.contains("talaria:"), "stdout held {screen:?}");
}
