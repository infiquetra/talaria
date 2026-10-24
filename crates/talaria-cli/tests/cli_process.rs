mod support;

// The mock gateway lives with the application crate's tests; the binary tests reach it through
// this include and use its standalone mode.
#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::path::Path;
use std::time::Duration;

use mock_gateway::{Behavior, Fixture, StandaloneGateway, WrittenKind, fixture_events, list_row};
use support::{Launch, Terminal, cooked_mode, local_flags, signal};

const FIRST_FRAME_BOUND: Duration = Duration::from_millis(500);
/// Measurement slack for comparing stamps taken by different threads of the test process.
const STAMP_SLACK: Duration = Duration::from_millis(10);
const COMPOSER_ROW: &str = "Type a message";

/// Type a line and press Enter after a pause. Text that arrives in one burst is a paste to the
/// composer, and an Enter inside the burst is part of it, so a person's separate Enter is
/// sent separately here too.
fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    press_enter(terminal);
}

fn press_enter(terminal: &mut Terminal) {
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("home")
}

fn non_empty_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn assert_restored(screen: &support::Screen) {
    assert!(
        cooked_mode(&screen.termios),
        "line editing and echo are not back on: {:?}\n{}",
        local_flags(&screen.termios),
        screen.text
    );
    assert!(
        !screen.text.contains(COMPOSER_ROW) && !screen.text.contains("Connecting"),
        "the composer was left on the screen:\n{}",
        screen.text
    );
}

fn first_frame(terminal: &mut Terminal) -> Duration {
    terminal
        .wait_first_frame(Duration::from_secs(3))
        .unwrap_or_else(|| {
            panic!(
                "no frame was drawn:\n{}",
                String::from_utf8_lossy(&terminal.raw_output())
            )
        })
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
fn no_gateway_exits_1_after_restoring_the_terminal() {
    let home = home();
    let screen = support::pty_run(
        &["--no-ensure"],
        home.path(),
        30,
        100,
        "",
        Duration::from_secs(5),
    );
    assert_eq!(
        screen.exit_code,
        Some(1),
        "{}\n{:?}",
        screen.text,
        screen.signal
    );
    assert!(screen.signal.is_none(), "{:?}", screen.signal);
    assert_restored(&screen);
    let lines = non_empty_lines(&screen.text);
    assert!(lines.len() >= 2, "{}", screen.text);
    let tail = &lines[lines.len() - 2..];
    assert!(tail[0].starts_with("talaria: "), "{}", screen.text);
    assert!(tail[1].starts_with("Next step: "), "{}", screen.text);
    assert!(!screen.text.contains('\u{1b}'), "{}", screen.text);
}

#[test]
fn recording_over_an_existing_file_fails_before_the_terminal_is_touched() {
    let home = home();
    let record = home.path().join("session.jsonl");
    std::fs::write(&record, "already\n").expect("record");
    let record_text = record.to_str().expect("utf8");

    let existing = support::captured(
        support::talaria(&["--no-ensure", "--record", record_text], home.path()),
        Duration::from_secs(2),
    );
    let stderr = String::from_utf8_lossy(&existing.stderr);
    assert_eq!(existing.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("the recording file already exists."),
        "{stderr}"
    );
    assert!(!stderr.contains("Connecting"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&record).expect("kept"), "already\n");

    let mut terminal = Launch::new(home.path())
        .args(&["--no-ensure", "--record", record_text])
        .spawn();
    let exit = terminal.wait_exit(Duration::from_secs(3));
    assert_eq!(exit.code, Some(1), "{}", terminal.screen_text());
    terminal.settle();
    let raw = String::from_utf8_lossy(&terminal.raw_output()).into_owned();
    assert!(raw.contains("the recording file already exists."), "{raw}");
    assert!(
        !raw.contains("Connecting") && !raw.contains(COMPOSER_ROW),
        "a frame was drawn: {raw}"
    );
}

#[test]
fn first_frame_before_the_gateway_answers_and_typing_is_kept() {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_delay("session.create", Some(Duration::from_secs(3)));

    let mut terminal = Launch::new(home.path()).args(&["--no-ensure"]).spawn();
    let elapsed = first_frame(&mut terminal);
    let first_screen = terminal.first_screen_at().expect("first frame time");
    assert!(
        terminal.screen_text().contains("Connecting"),
        "first frame missing after {elapsed:?}:\n{}",
        terminal.screen_text()
    );
    assert!(
        elapsed < FIRST_FRAME_BOUND,
        "the first frame took {elapsed:?}"
    );

    // Typed after raw mode is on, so the terminal's own echo cannot show or eat it.
    terminal.type_keys("hello");
    assert!(
        terminal.wait_for_text("hello", Duration::from_secs(1)),
        "typed text was dropped:\n{}",
        terminal.screen_text()
    );
    press_enter(&mut terminal);

    let create = gateway
        .wait_request("session.create", 0, Duration::from_secs(3))
        .expect("the binary never asked the gateway to create a session");
    let connected = gateway.first_control_connection_at().expect("connection");
    assert!(
        connected + STAMP_SLACK >= first_screen,
        "the gateway was contacted {:?} before the first frame",
        first_screen.saturating_duration_since(connected)
    );
    assert!(create.at + STAMP_SLACK >= first_screen);
    assert!(
        gateway.requests("prompt.submit").is_empty(),
        "a prompt was sent before the session existed"
    );

    let submit = gateway
        .wait_request("prompt.submit", 0, Duration::from_secs(8))
        .expect("the held prompt never reached the gateway");
    assert_eq!(submit.params["text"], "hello");
    let answered = gateway
        .written()
        .into_iter()
        .find(|frame| frame.kind == WrittenKind::Reply && frame.method == "session.create")
        .expect("create reply");
    assert!(
        submit.at >= answered.at,
        "the held prompt went out before the session opened"
    );
    assert!(
        answered.at.duration_since(create.at) >= Duration::from_millis(2900),
        "the mock answered early"
    );

    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(5));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
}

#[test]
fn continue_asks_git_for_the_top_level_after_the_first_frame() {
    let home = home();
    let bin = home.path().join("bin");
    std::fs::create_dir(&bin).expect("bin");
    let log = bin.join("git.log");
    mock_gateway::write_logging_stand_in(&bin, "git", &log, "echo /work/proj");
    let git = support::FileWatch::start(&log);
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_delay("session.resume", Some(Duration::from_secs(3)));

    let mut terminal = Launch::new(home.path())
        .args(&["--no-ensure", "-c"])
        .path_prefix(&bin)
        .spawn();
    let elapsed = first_frame(&mut terminal);
    let first_screen = terminal.first_screen_at().expect("first frame time");
    assert!(
        elapsed < FIRST_FRAME_BOUND,
        "the first frame took {elapsed:?}"
    );

    let resume = gateway
        .wait_request("session.resume", 0, Duration::from_secs(3))
        .expect("the binary never asked the gateway to resume");
    let called = (0..300)
        .find_map(|_| {
            git.seen_at().or_else(|| {
                std::thread::sleep(Duration::from_millis(10));
                None
            })
        })
        .expect("git was never called");
    assert!(
        called + STAMP_SLACK >= first_screen,
        "git was called {:?} before the first frame",
        first_screen.saturating_duration_since(called)
    );
    let connected = gateway.first_control_connection_at().expect("connection");
    assert!(connected + STAMP_SLACK >= first_screen);
    assert!(
        called < resume.at + Duration::from_secs(2),
        "git waited for the gateway's answer"
    );
    assert_eq!(resume.params["workspace"], "/work/proj");
    assert_eq!(resume.params["latest"], "tui");

    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(8));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    gateway.assert_no_violations();
}

#[test]
fn quit_restores_terminal_and_prints_resume_hint() {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.replay_after("prompt.submit", fixture_events(Fixture::PlainAnswer));

    let mut terminal = Launch::new(home.path())
        .args(&["--no-ensure", "--profile", "fixture"])
        .spawn();
    first_frame(&mut terminal);
    type_line(&mut terminal, "What files are in this directory?");
    let submit = gateway
        .wait_request("prompt.submit", 0, Duration::from_secs(5))
        .unwrap_or_else(|| panic!("no prompt was sent:\n{}", terminal.screen_text()));
    assert_eq!(submit.params["text"], "What files are in this directory?");
    assert!(
        terminal.wait_for_text("pong", Duration::from_secs(5)),
        "the answer never reached the screen:\n{}",
        terminal.screen_text()
    );

    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(5));
    assert_eq!(
        screen.exit_code,
        Some(0),
        "{}\n{}\n{:?}",
        screen.text,
        screen.termios,
        screen.signal
    );
    assert_restored(&screen);
    let lines = non_empty_lines(&screen.text);
    assert!(lines.len() >= 2, "{}", screen.text);
    assert_eq!(
        lines[lines.len() - 2..],
        [
            "Resume this session with:",
            "  talaria --resume stored-1 --profile fixture"
        ],
        "{}",
        screen.text
    );
    assert!(!screen.text.contains('\u{1b}'), "{}", screen.text);

    let detaches = gateway.detaches();
    assert_eq!(detaches.len(), 1, "{detaches:?}");
    assert_eq!(detaches[0].session_id, "local-session-1");
    assert_eq!(detaches[0].subscription_id, "sub-1");
    assert!(!detaches[0].had_end_reason);
    assert!(detaches[0].at >= gateway.submissions()[0].at);
    gateway.assert_no_violations();
}

/// Start the binary against a gateway that never answers the create, wait until it has asked,
/// send the signal to the binary itself, and read how it ended.
fn signalled(name: &str) -> support::Screen {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_behavior("session.create", Behavior::Silent);
    let mut terminal = Launch::new(home.path()).args(&["--no-ensure"]).spawn();
    first_frame(&mut terminal);
    gateway
        .wait_request("session.create", 0, Duration::from_secs(3))
        .unwrap_or_else(|| {
            panic!(
                "the binary never reached the gateway:\n{}",
                terminal.screen_text()
            )
        });
    signal(terminal.pid(), name);
    terminal.finish(Duration::from_secs(5))
}

#[test]
fn sigterm_restores_the_terminal_and_exits_143() {
    let screen = signalled("-TERM");
    assert_eq!(screen.exit_code, Some(143), "{}", screen.text);
    assert!(
        screen.signal.is_none(),
        "the process died by the signal instead of restoring the terminal: {:?}",
        screen.signal
    );
    assert_restored(&screen);
}

#[test]
fn sighup_restores_the_terminal_and_exits_129() {
    let screen = signalled("-HUP");
    assert_eq!(screen.exit_code, Some(129), "{}", screen.text);
    assert!(
        screen.signal.is_none(),
        "the process died by the signal instead of restoring the terminal: {:?}",
        screen.signal
    );
    assert_restored(&screen);
}

fn list_gateway(home: &Path) -> StandaloneGateway {
    let gateway = StandaloneGateway::start(home);
    gateway.set_list_rows(
        "live",
        vec![list_row(
            "local-session-9",
            "Earlier work",
            None,
            Some("minimax-m3.1-flash"),
            4,
            false,
        )],
    );
    gateway
}

#[test]
fn sigint_at_the_resume_list_ends_the_process_by_the_signal() {
    let home = home();
    let gateway = list_gateway(home.path());
    let script = format!(
        "{} --no-ensure --resume; echo status=$?",
        env!("CARGO_BIN_EXE_talaria")
    );
    let mut terminal = Launch::new(home.path()).script(&script).spawn();
    assert!(
        terminal.wait_for_text("Number or id (Enter to cancel):", Duration::from_secs(5)),
        "the list never asked for a choice:\n{}",
        terminal.screen_text()
    );
    assert!(terminal.screen_text().contains("Earlier work"));

    // A terminal interrupt would reach the shell as well and end the script; the signal goes
    // to the binary alone, as it does for a job a shell started in the foreground.
    let shell = terminal.pid();
    let children = terminal.children_of(shell);
    assert_eq!(
        children.len(),
        1,
        "expected the binary under the shell: {children:?}"
    );
    signal(children[0], "-INT");

    assert!(
        terminal.wait_for_text("status=130", Duration::from_secs(3)),
        "the shell did not report the signal:\n{}",
        terminal.screen_text()
    );
    let screen = terminal.finish(Duration::from_secs(3));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    assert!(
        gateway.requests("session.create").is_empty()
            && gateway.requests("session.resume").is_empty(),
        "no session should have been opened"
    );
}
