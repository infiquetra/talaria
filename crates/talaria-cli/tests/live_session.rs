//! Live sessions against a running Hermes gateway. Ignored unless a person asks for them. A skip
//! is not a pass: with `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//!
//! Run by hand, one test at a time, with the gateway of the home named by `HERMES_HOME`:
//! `TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_session -- --ignored
//! --nocapture --test-threads=1`

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hermes_client::{DiscoverOptions, EnsurePolicy, default_home, discover_with};
use support::{Launch, Terminal};

const ASK: &str = "What is 1234 plus 4321? Reply with only the number.";
const LONG_ASK: &str = "Count from 1 to 200, one number per line, then write the sum of 6000 and 1731 on the last line.";
const RM_ASK: &str =
    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache";
const PROFILE_FLAGS: [&str; 2] = ["--profile", "fixture"];
const ROWS: u16 = 60;
const COLS: u16 = 160;
const ANSWER_LIMIT: Duration = Duration::from_secs(90);

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

/// The commit the running gateway reports, read through discovery that never starts a gateway.
fn gateway_code_sha(home: &Path) -> Result<Option<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        discover_with(home, &options)
            .await
            .map(|endpoint| endpoint.code_sha().map(str::to_owned))
            .map_err(|error| error.to_string())
    })
}

fn skip_reason(release_only: bool) -> Option<String> {
    if release_only && cfg!(debug_assertions) {
        return Some("this budget is measured on a release binary".into());
    }
    match gateway_code_sha(&hermes_home()) {
        Err(reason) => Some(reason),
        Ok(sha) => hermes_contract::pinned::live_skip_reason(sha.as_deref()),
    }
}

/// True when the test may run. A skip prints its reason and, when a live gateway is required,
/// fails the test.
fn begin(release_only: bool) -> bool {
    if let Some(reason) = skip_reason(release_only) {
        println!("LIVE_SESSION_RESULT: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return false;
    }
    true
}

fn passed() {
    println!("LIVE_SESSION_RESULT: PASSED");
}

/// The binary in a pseudo-terminal, started in `project`, against the live gateway.
fn start(user_home: &Path, project: &Path, args: &[&str]) -> Terminal {
    Launch::new(user_home)
        .hermes_home(&hermes_home())
        .cwd(project)
        .size(ROWS, COLS)
        .answer_queries()
        .args(args)
        .spawn()
}

fn new_session(user_home: &Path, project: &Path) -> Terminal {
    let mut args = vec!["--no-ensure"];
    args.extend(PROFILE_FLAGS);
    start(user_home, project, &args)
}

fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

fn lines_equal_to(text: &str, wanted: &str) -> usize {
    text.lines().filter(|line| line.trim() == wanted).count()
}

/// Wait until a line of the screen is exactly `wanted` (a reasoning line that mentions the
/// text does not count).
fn wait_for_line(terminal: &mut Terminal, wanted: &str, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if lines_equal_to(&terminal.screen_text(), wanted) > 0 {
            return true;
        }
        if terminal.exited() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    lines_equal_to(&terminal.screen_text(), wanted) > 0
}

/// Wait until the status line has not shown `working` for this long.
fn wait_idle(terminal: &mut Terminal, quiet: Duration, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    let mut idle_since: Option<Instant> = None;
    while Instant::now() < deadline {
        if terminal.screen_text().contains("working") {
            idle_since = None;
        } else {
            let since = *idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= quiet {
                return true;
            }
        }
        if terminal.exited() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Type `/quit`, wait for the exit and return the screen. The session id of the resume hint is
/// in the text.
fn quit(mut terminal: Terminal) -> support::Screen {
    type_line(&mut terminal, "/quit");
    terminal.finish(Duration::from_secs(20))
}

/// The id after `talaria --resume ` in the resume hint.
fn resume_id(text: &str) -> String {
    let marker = "talaria --resume ";
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("no resume hint in the output:\n{text}"))
        + marker.len();
    text[start..]
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("the resume hint has no id:\n{text}"))
        .to_owned()
}

fn resume_args(id: &str) -> Vec<&str> {
    vec!["--resume", id, "--no-ensure", "--profile", "fixture"]
}

#[ignore = "needs a running gateway"]
#[test]
fn new_ask_quit_resume_roundtrip() {
    if !begin(false) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");

    let mut first = new_session(user_home.path(), project.path());
    assert!(first.wait_first_frame(Duration::from_secs(5)).is_some());
    type_line(&mut first, ASK);
    assert!(
        wait_for_line(&mut first, "5555", ANSWER_LIMIT),
        "no answer:\n{}",
        first.screen_text()
    );
    assert!(wait_idle(
        &mut first,
        Duration::from_secs(2),
        Duration::from_secs(30)
    ));
    let before_quit = first.screen_text();
    assert_eq!(
        lines_equal_to(&before_quit, "5555"),
        1,
        "the answer must be shown once:\n{before_quit}"
    );
    let screen = quit(first);
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    assert!(
        screen.text.contains("Resume this session with:"),
        "{}",
        screen.text
    );
    let id = resume_id(&screen.text);

    let mut resumed = start(user_home.path(), project.path(), &resume_args(&id));
    assert!(
        resumed.wait_for_text("Resumed:", Duration::from_secs(30)),
        "{}",
        resumed.screen_text()
    );
    assert!(
        resumed.screen_text().contains(ASK),
        "{}",
        resumed.screen_text()
    );
    assert_eq!(
        lines_equal_to(&resumed.screen_text(), "5555"),
        1,
        "the resumed transcript must hold the answer once:\n{}",
        resumed.screen_text()
    );
    let screen = quit(resumed);
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    assert_eq!(resume_id(&screen.text), id, "the same session");

    // Continue finds the same session in this folder, which also shows the gateway recorded the
    // working folder of the new session.
    let mut args = vec!["-c", "--no-ensure"];
    args.extend(PROFILE_FLAGS);
    let mut continued = start(user_home.path(), project.path(), &args);
    assert!(
        continued.wait_for_text("Resumed:", Duration::from_secs(30)),
        "{}",
        continued.screen_text()
    );
    let screen = quit(continued);
    assert_eq!(resume_id(&screen.text), id, "continue found the session");
    passed();
}

#[ignore = "needs a running gateway"]
#[test]
fn quit_mid_turn_leaves_the_turn_running() {
    if !begin(false) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");

    let mut session = new_session(user_home.path(), project.path());
    assert!(session.wait_first_frame(Duration::from_secs(5)).is_some());
    type_line(&mut session, LONG_ASK);
    assert!(
        session.wait_for_text("working", Duration::from_secs(60)),
        "the turn never showed as working:\n{}",
        session.screen_text()
    );
    let screen = quit(session);
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    assert!(
        screen
            .text
            .contains("The agent is still working on it in the background."),
        "{}",
        screen.text
    );
    let id = resume_id(&screen.text);

    let mut resumed = start(user_home.path(), project.path(), &resume_args(&id));
    assert!(
        wait_for_line(&mut resumed, "7731", ANSWER_LIMIT),
        "the answer is not there on resume:\n{}",
        resumed.screen_text()
    );
    let text = resumed.screen_text();
    assert_eq!(
        lines_equal_to(&text, "7731"),
        1,
        "the sum must appear on one line:\n{text}"
    );
    let _ = quit(resumed);
    passed();
}

#[ignore = "needs a running gateway"]
#[test]
fn continue_in_a_fresh_folder_is_refused() {
    if !begin(false) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");
    let mut args = vec!["-c", "--no-ensure"];
    args.extend(PROFILE_FLAGS);
    let terminal = start(user_home.path(), project.path(), &args);
    let screen = terminal.finish(Duration::from_secs(30));
    assert!(
        screen
            .text
            .contains("talaria: no earlier session found for this folder."),
        "{}",
        screen.text
    );
    assert!(screen.text.contains("Next step:"), "{}", screen.text);
    assert_eq!(screen.exit_code, Some(1), "{}", screen.text);
    passed();
}

#[ignore = "needs a running gateway"]
#[test]
fn approval_card_answers_rm_rf_pytest_cache() {
    if !begin(false) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");
    let cache = project.path().join(".pytest_cache");
    std::fs::create_dir(&cache).expect("cache");
    std::fs::write(cache.join("x"), "x").expect("cache file");

    let mut session = new_session(user_home.path(), project.path());
    assert!(session.wait_first_frame(Duration::from_secs(5)).is_some());
    // A first turn, so the approval comes on the second turn (the interrupt must carry the
    // right turn number there).
    type_line(&mut session, "Reply with only the word OK.");
    assert!(session.wait_for_text("OK", ANSWER_LIMIT));
    assert!(wait_idle(
        &mut session,
        Duration::from_secs(2),
        Duration::from_secs(30)
    ));

    // Second turn: Ctrl+C with the card open denies, then interrupts; the folder stays.
    type_line(&mut session, RM_ASK);
    assert!(
        session.wait_for_text("1. once", ANSWER_LIMIT),
        "no approval card:\n{}",
        session.screen_text()
    );
    assert!(session.screen_text().contains(".pytest_cache"));
    session.type_keys("\u{3}");
    assert!(
        wait_idle(
            &mut session,
            Duration::from_secs(4),
            Duration::from_secs(60)
        ),
        "the turn did not end after Ctrl+C:\n{}",
        session.screen_text()
    );
    assert!(cache.exists(), "a denied command must not run");

    // Third turn: approve with Enter (the highlight starts on `once`); the folder goes.
    type_line(&mut session, RM_ASK);
    assert!(
        session.wait_for_text("1. once", ANSWER_LIMIT),
        "no second approval card:\n{}",
        session.screen_text()
    );
    std::thread::sleep(Duration::from_millis(300));
    session.type_keys("\r");
    let deadline = Instant::now() + ANSWER_LIMIT;
    while cache.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(!cache.exists(), "the approved command did not run");
    let _ = quit(session);
    passed();
}

#[ignore = "needs a running gateway; measured on a release binary"]
#[test]
fn launch_to_usable_composer_within_budget() {
    if !begin(true) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let mut samples = Vec::new();
    for round in 0..16 {
        let project = tempfile::tempdir().expect("project");
        let mut terminal = Launch::new(user_home.path())
            .hermes_home(&hermes_home())
            .cwd(project.path())
            .size(48, 160)
            .answer_queries()
            .args(&["--no-ensure", "--profile", "fixture"])
            .spawn();
        let first = terminal
            .wait_first_frame(Duration::from_secs(5))
            .unwrap_or_else(|| panic!("launch {round} drew nothing:\n{}", terminal.screen_text()));
        let _ = quit(terminal);
        if round > 0 {
            samples.push(first);
        }
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    let slowest = samples.iter().copied().max().unwrap_or(Duration::ZERO);
    println!("launch to first frame: median {median:?}, slowest {slowest:?}");
    assert!(
        slowest <= Duration::from_millis(200),
        "slowest launch {slowest:?} (median {median:?})"
    );
    passed();
}

#[ignore = "needs a running gateway"]
#[test]
fn interrupt_and_resume_list() {
    if !begin(false) {
        return;
    }
    let user_home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");

    let mut session = new_session(user_home.path(), project.path());
    assert!(session.wait_first_frame(Duration::from_secs(5)).is_some());
    type_line(&mut session, "Reply with only the word OK.");
    assert!(session.wait_for_text("OK", ANSWER_LIMIT));
    assert!(wait_idle(
        &mut session,
        Duration::from_secs(2),
        Duration::from_secs(30)
    ));

    // Second turn: Esc stops a long answer.
    type_line(
        &mut session,
        "Count from 1 to 2000, one number per line, and do not stop early.",
    );
    assert!(
        session.wait_for_text("working", Duration::from_secs(60)),
        "{}",
        session.screen_text()
    );
    std::thread::sleep(Duration::from_secs(2));
    session.type_keys("\u{1b}");
    assert!(
        session.wait_for_text("Interrupted", Duration::from_secs(30)),
        "Esc did not interrupt the turn:\n{}",
        session.screen_text()
    );
    assert!(
        wait_idle(
            &mut session,
            Duration::from_secs(3),
            Duration::from_secs(30)
        ),
        "the turn still shows as working after Esc:\n{}",
        session.screen_text()
    );
    let screen = quit(session);
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    let id = resume_id(&screen.text);

    // The list: print what it shows, check the session is on it, and cancel.
    let mut list = start(
        user_home.path(),
        project.path(),
        &["--resume", "--no-ensure", "--profile", "fixture"],
    );
    assert!(
        list.wait_for_text("Number or id", Duration::from_secs(30)),
        "{}",
        list.screen_text()
    );
    let observed = list.screen_text();
    println!("resume list:\n{observed}");
    assert!(
        observed.contains(&id),
        "the interrupted session is not on the list:\n{observed}"
    );
    list.type_keys("\r");
    let screen = list.finish(Duration::from_secs(20));
    assert!(screen.text.contains("Cancelled."), "{}", screen.text);
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    passed();
}
