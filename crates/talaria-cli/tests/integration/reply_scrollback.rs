//! A reply taller than the screen, through the binary: while it streams, its closed blocks
//! scroll up into the terminal's scrollback above the live region, and once it ends every one
//! of its rows is in scrollback exactly once.

use crate::support;

use crate::mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::{Fixture, StandaloneGateway, fixture_event};
use serde_json::{Value, json};
use support::Launch;

const ROWS: u16 = 24;
const COLS: u16 = 80;

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

/// A table of nine rows, then twenty closed paragraphs, then one still open.
fn tall_reply() -> String {
    let mut text = String::from("| Step | What happens |\n|---|---|\n");
    for row in 1..=9 {
        text.push_str(&format!("| {row} | Table row {row:02} of the plan |\n"));
    }
    for n in 1..=20 {
        text.push_str(&format!("\nParagraph {n:02} of the reply.\n"));
    }
    text.push_str("\nParagraph 21 is still being written");
    text
}

/// The turn's start, then the reply in deltas of 50 bytes.
fn streamed(text: &str) -> Vec<Value> {
    let mut frames = vec![fixture_event(Fixture::PlainAnswer, "message.start")];
    let delta = fixture_event(Fixture::PlainAnswer, "message.delta");
    for piece in text.as_bytes().chunks(50) {
        let mut frame = delta.clone();
        frame["params"]["payload"]["text"] = json!(std::str::from_utf8(piece).expect("ascii"));
        frames.push(frame);
    }
    frames
}

fn wait_for(
    terminal: &support::Terminal,
    limit: Duration,
    check: impl Fn(&[String]) -> bool,
) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if check(&terminal.history_and_screen()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn count(rows: &[String], needle: &str) -> usize {
    rows.iter().filter(|row| row.contains(needle)).count()
}

#[test]
fn a_tall_reply_scrolls_up_into_scrollback_while_it_streams() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    gateway.replay_after("prompt.submit", streamed(&tall_reply()));
    // Interrupting ends the turn the way Hermes does, with a completed message.
    gateway.replay_after(
        "session.interrupt",
        vec![fixture_event(Fixture::Interrupted, "message.complete")],
    );

    let mut terminal = Launch::new(home.path())
        .args(&["--no-ensure", "--profile", "fixture"])
        .size(ROWS, COLS)
        .spawn();
    assert!(
        terminal.wait_first_frame(Duration::from_secs(10)).is_some(),
        "no frame was drawn"
    );
    terminal.type_keys("Plan the work");
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    assert!(
        gateway
            .wait_request("prompt.submit", 0, Duration::from_secs(10))
            .is_some(),
        "no prompt was sent:\n{}",
        terminal.screen_text()
    );

    // Mid-stream: the last paragraph is open, so the reply is not final. Everything before it
    // is already in scrollback, scrolled off the top of the screen.
    let reached = wait_for(&terminal, Duration::from_secs(10), |rows| {
        count(rows, "Paragraph 20 of the reply.") == 1
    });
    let rows = terminal.history_and_screen();
    assert!(reached, "the stream never arrived:\n{rows:#?}");
    let history = &rows[..rows.len() - usize::from(ROWS)];
    for wanted in [
        "Table row 01 of the plan",
        "Table row 09 of the plan",
        "Paragraph 01 of the reply.",
    ] {
        assert_eq!(
            count(history, wanted),
            1,
            "{wanted:?} is not in scrollback once while the reply streams:\n{rows:#?}"
        );
    }

    // Esc asks for a second press before it interrupts.
    terminal.type_keys("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\x1b");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "the interrupt key sent no interrupt"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |rows| {
            count(rows, "Paragraph 21 is still being written") == 1
                && !rows.iter().any(|row| row.contains("\u{273b} Working"))
        }),
        "the turn did not end:\n{:#?}",
        terminal.history_and_screen()
    );
    terminal.settle();

    // Every row of the reply is in scrollback exactly once, in order.
    let rows = terminal.history_and_screen();
    let mut wanted: Vec<String> = (1..=9)
        .map(|row| format!("Table row {row:02} of the plan"))
        .collect();
    wanted.extend((1..=20).map(|n| format!("Paragraph {n:02} of the reply.")));
    wanted.push("Paragraph 21 is still being written".into());
    let mut previous = 0;
    for needle in &wanted {
        let at: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.contains(needle.as_str()))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            at.len(),
            1,
            "{needle:?} is not on exactly one row:\n{rows:#?}"
        );
        assert!(at[0] >= previous, "{needle:?} is out of order:\n{rows:#?}");
        previous = at[0];
    }

    terminal.type_keys("/quit");
    assert!(terminal.wait_for_text("/quit", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    let screen = terminal.finish(Duration::from_secs(10));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
}
