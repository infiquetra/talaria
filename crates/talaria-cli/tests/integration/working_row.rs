//! The working row through the binary: it sits directly above the composer while a turn runs
//! and leaves once the turn completes.

use crate::support;

use crate::mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::{Behavior, Fixture, StandaloneGateway, fixture_event};
use serde_json::json;
use support::Launch;

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

/// The screen's working row and the row under it, if a working row is on screen.
fn working_row_and_next(screen: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines.iter().position(|line| {
        line.trim_start()
            .strip_prefix("✻ Working · ")
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    })?;
    let next = lines.get(at + 1).copied().unwrap_or_default();
    Some((lines[at].to_owned(), next.to_owned()))
}

fn wait_for(terminal: &support::Terminal, limit: Duration, check: impl Fn(&str) -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if check(&terminal.screen_text()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn working_row_sits_above_the_composer_until_the_turn_completes() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a11e{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);
    // Interrupting ends the turn the way Hermes does, with a completed message.
    gateway.replay_after(
        "session.interrupt",
        vec![fixture_event(Fixture::Interrupted, "message.complete")],
    );

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            working_row_and_next(screen).is_some()
        }),
        "no working row while the turn runs:\n{}",
        terminal.screen_text()
    );
    let screen = terminal.screen_text();
    let (row, next) = working_row_and_next(&screen).expect("the working row");
    assert!(
        row.contains("to interrupt"),
        "the row names the interrupt key: {row:?}"
    );
    assert!(
        next.starts_with('╭') && next.ends_with('╮'),
        "the composer title sits directly under the working row, saw {next:?}:\n{screen}"
    );

    // Esc asks for a second press: the first only arms the confirmation.
    terminal.type_keys("\x1b");
    wait_row_text(&terminal, "Esc again to interrupt");
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\x1b");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "the second press sent no interrupt"
    );
    assert_eq!(gateway.requests("session.interrupt").len(), 1);
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            !screen.contains("✻ Working")
        }),
        "the working row stayed after the turn completed:\n{}",
        terminal.screen_text()
    );
    // With the turn over the screen is idle again: nothing is drawn while nothing changes.
    std::thread::sleep(Duration::from_millis(500));
    let settled = terminal.raw_output().len();
    std::thread::sleep(Duration::from_millis(1_000));
    let idle = terminal.raw_output().len() - settled;
    assert_eq!(idle, 0, "the idle screen wrote {idle} bytes in a second");

    terminal.type_keys("/quit");
    assert!(terminal.wait_for_text("/quit", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    let screen = terminal.finish(Duration::from_secs(10));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
}

/// Whether the working row on screen offers the interrupt key. `None` without a working row.
fn row_offers_interrupt(screen: &str) -> Option<bool> {
    working_row_and_next(screen).map(|(row, _)| row.contains("to interrupt"))
}

/// Wait until the working row on screen contains `text`.
fn wait_row_text(terminal: &support::Terminal, text: &str) {
    assert!(
        wait_for(terminal, Duration::from_secs(10), |screen| {
            working_row_and_next(screen).is_some_and(|(row, _)| row.contains(text))
        }),
        "the working row never said {text:?}:\n{}",
        terminal.screen_text()
    );
}

fn wait_row(terminal: &support::Terminal, offers: bool, what: &str) {
    assert!(
        wait_for(terminal, Duration::from_secs(10), |screen| {
            row_offers_interrupt(screen) == Some(offers)
        }),
        "{what}:\n{}",
        terminal.screen_text()
    );
}

/// Search stays editable when the picker connects after Ctrl+C interrupts the foreground turn.
#[test]
fn session_search_keeps_its_query_while_interrupting_the_turn() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a124{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    let limit = Duration::from_secs(8);
    assert!(terminal.wait_for_text("── Resumed: ", limit));
    let lists_before_open = gateway.requests("session.list").len();
    let (_reached, release) = gateway.hold_second_handshake();
    terminal.type_keys("/resume");
    assert!(terminal.wait_for_text("/resume", limit));
    // Let the composer's fast-paste window close before sending a separate Enter press.
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\r");
    assert!(
        terminal.wait_for_text("/ search", limit),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("/needl");
    assert!(terminal.wait_for_text("needl", limit));
    assert!(terminal.wait_for_text("Esc clear", limit));
    terminal.type_keys("\x03");
    let interrupt = gateway.wait_request("session.interrupt", 0, limit);
    assert!(interrupt.is_some(), "{}", terminal.screen_text());

    // The first list request follows the picker's connection response.
    release.send(()).expect("release the picker connection");
    assert!(
        gateway
            .wait_request("session.list", lists_before_open, limit)
            .is_some(),
        "the picker never processed the new connection"
    );
    terminal.type_keys("e");
    assert!(
        terminal.wait_for_text("needle", limit),
        "search stopped accepting text after the picker connected:\n{}",
        terminal.screen_text()
    );
    assert!(terminal.screen_text().contains("Esc clear"));
}

/// While Esc would close a list or a selector instead of interrupting, the working row does not
/// say "Esc to interrupt", and Esc does what the open surface says, not interrupt.
#[test]
fn working_row_offers_esc_only_when_esc_interrupts() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a120{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    wait_row(
        &terminal,
        true,
        "the row offers Esc while nothing else takes it",
    );

    // History search: Esc closes the search.
    terminal.type_keys("\x12");
    wait_row(
        &terminal,
        false,
        "the row offered Esc with history search open",
    );
    terminal.type_keys("\x1b");
    wait_row(
        &terminal,
        true,
        "the hint did not come back after the search closed",
    );

    // The slash palette: Esc closes the list.
    terminal.type_keys("/");
    wait_row(
        &terminal,
        false,
        "the row offered Esc with the slash palette open",
    );
    terminal.type_keys("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\x7f");
    wait_row(
        &terminal,
        true,
        "the hint did not come back after the palette closed",
    );

    // The menu bar, focused with Alt+I: Esc closes the menu and returns to the composer.
    terminal.type_keys("\x1bi");
    assert!(
        terminal.wait_for_text("Settings", Duration::from_secs(10)),
        "the menu bar never opened:\n{}",
        terminal.screen_text()
    );
    wait_row(
        &terminal,
        false,
        "the row offered Esc with the menu bar focused",
    );
    terminal.type_keys("\x1b");
    wait_row(
        &terminal,
        true,
        "the hint did not come back after the menu closed",
    );

    // The /theme selector: Esc cancels the selector.
    terminal.type_keys("/theme");
    assert!(terminal.wait_for_text("/theme", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\r");
    assert!(
        terminal.wait_for_text("type to filter", Duration::from_secs(10)),
        "the theme selector never opened:\n{}",
        terminal.screen_text()
    );
    wait_row(
        &terminal,
        false,
        "the row offered Esc with the theme selector open",
    );
    terminal.type_keys("\x1b");
    wait_row(
        &terminal,
        true,
        "the hint did not come back after the selector closed",
    );

    assert!(
        gateway.requests("session.interrupt").is_empty(),
        "an Esc meant for a list or a selector interrupted the turn"
    );

    terminal.type_keys("\x1b");
    wait_row_text(&terminal, "Esc again to interrupt");
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\x1b");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "the interrupt key sent no interrupt once nothing else took it"
    );
    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}

/// While the link redials, the turn may still run on the gateway, so the row stays; but the
/// interrupt key cannot reach the gateway, so the row does not offer it until the link is back.
#[test]
fn working_row_drops_the_esc_hint_while_the_link_redials() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a122{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    wait_row(&terminal, true, "the row offers Esc while connected");

    let (_reached, release) = gateway.hold_second_handshake();
    gateway.block_on(gateway.kill_connection());
    wait_row(
        &terminal,
        false,
        "the row offered Esc while the link was redialling",
    );
    assert!(
        terminal.screen_text().contains("✻ Working"),
        "the row left while the turn may still run:\n{}",
        terminal.screen_text()
    );
    let _ = release.send(());

    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}

/// With the `/theme` selector open on a short terminal, the composer and the working row stay on
/// screen: the sample of the selected look is left out when it would push them off.
#[test]
fn theme_sample_never_pushes_the_composer_off_a_short_screen() {
    // The fourth value turns on fast mode, whose automation row sits under the selector and
    // takes a row the composer and the working row would otherwise keep.
    for (rows, cols, sample, fast) in [
        (16, 60, false, false),
        (16, 40, false, false),
        (19, 80, false, false),
        (20, 80, false, true),
        (24, 80, true, false),
    ] {
        let home = temp_home();
        let gateway = StandaloneGateway::start(home.path());
        let id = format!("local-5ee5a121{}", "0".repeat(56));
        let mut snapshot = mock_gateway::recorded_resume_snapshot();
        snapshot["session_id"] = json!(id);
        snapshot["stored_session_id"] = json!(id);
        snapshot["prompts"] = json!([]);
        snapshot["running"] = json!(true);
        gateway.set_resume_snapshot(snapshot);
        if fast {
            gateway.set_config_value("fast", json!("fast"));
        }

        let mut terminal = Launch::new(home.path())
            .args(&["attach", &id, "--profile", "fixture"])
            .size(rows, cols)
            .answer_queries()
            .spawn();
        assert!(
            wait_for(&terminal, Duration::from_secs(10), |screen| screen
                .contains("✻ Working")
                && (!fast || screen.contains("⚡ fast mode"))),
            "{rows}x{cols}: no working row while the turn runs:\n{}",
            terminal.screen_text()
        );
        terminal.type_keys("/theme");
        assert!(terminal.wait_for_text("/theme", Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(300));
        terminal.type_keys("\r");
        assert!(
            wait_for(&terminal, Duration::from_secs(10), |screen| {
                screen.contains("type to filter")
            }),
            "{rows}x{cols}: the theme selector never opened:\n{}",
            terminal.screen_text()
        );
        std::thread::sleep(Duration::from_millis(300));
        let screen = terminal.screen_text();
        let lines: Vec<&str> = screen.lines().collect();
        let working = lines.iter().position(|line| line.contains("✻ Working"));
        let composer = lines.windows(3).position(|group| {
            group[0].starts_with('╭')
                && group[0].contains("Message")
                && group[0].ends_with('╮')
                && group[1].starts_with('│')
                && group[1].contains("Type a message")
                && group[1].ends_with('│')
                && group[2].starts_with('╰')
                && group[2].ends_with('╯')
        });
        assert!(
            working.is_some() && composer.is_some() && working < composer,
            "{rows}x{cols}: the working row and the whole composer must stay on screen:\n{screen}"
        );
        assert_eq!(
            screen.contains("Transcript sample"),
            sample,
            "{rows}x{cols}: sample shown should be {sample}:\n{screen}"
        );
    }
}

/// A running session whose gateway restarts and no longer knows it: the working row leaves
/// with the turn, and the screen goes quiet.
// Slow: the link redials after the drop, about a second.
#[test]
fn working_row_leaves_when_the_gateway_loses_the_session() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a11f{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            working_row_and_next(screen).is_some()
        }),
        "no working row while the turn runs:\n{}",
        terminal.screen_text()
    );

    // The gateway restarts: the socket drops and the re-attach finds no such session.
    gateway.queue_behavior("session.resume", Behavior::Refuse("not_found".to_owned()));
    gateway.block_on(gateway.kill_connection());
    assert!(
        terminal.wait_for_text(
            "This session no longer exists on the gateway.",
            Duration::from_secs(15)
        ),
        "the session-gone notice never showed:\n{}",
        terminal.screen_text()
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(5), |screen| {
            !screen.contains("✻ Working")
        }),
        "the working row stayed after the session was gone:\n{}",
        terminal.screen_text()
    );
    std::thread::sleep(Duration::from_millis(500));
    let settled = terminal.raw_output().len();
    std::thread::sleep(Duration::from_millis(1_000));
    let idle = terminal.raw_output().len() - settled;
    assert_eq!(
        idle, 0,
        "the screen wrote {idle} bytes in a second after the session was gone"
    );

    terminal.type_keys("/quit");
    assert!(terminal.wait_for_text("/quit", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    let _ = terminal.finish(Duration::from_secs(10));
}

/// One Esc during a running turn sends nothing and changes the row's hint; a second inside two
/// seconds interrupts once; with no second press the hint returns on its own and the next
/// press arms again. Ctrl+C interrupts on a single press.
#[test]
fn esc_asks_for_a_second_press_and_the_hint_returns_on_time() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a123{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    wait_row_text(&terminal, "Esc twice to interrupt");

    terminal.type_keys("\x1b");
    wait_row_text(&terminal, "Esc again to interrupt");
    assert!(
        gateway.requests("session.interrupt").is_empty(),
        "one Esc interrupted the turn"
    );
    // No second press: the hint comes back with no key pressed.
    wait_row_text(&terminal, "Esc twice to interrupt");
    assert!(gateway.requests("session.interrupt").is_empty());

    // The next press arms again, then Ctrl+C interrupts at once.
    terminal.type_keys("\x1b");
    wait_row_text(&terminal, "Esc again to interrupt");
    terminal.type_keys("\x03");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "Ctrl+C sent no interrupt"
    );
    let _ = terminal.finish(Duration::from_secs(10));
}
