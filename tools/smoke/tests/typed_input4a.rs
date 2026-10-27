//! Input sent by the configuration suite is restricted to its read-only script.
#[path = "support/scripts4a.rs"]
mod scripts4a;
mod support;

use scripts4a::{exec_4a, script_4a};
use support::Call;

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Run(String),
    Text(String),
    Key(String),
}

fn input() -> Vec<Event> {
    let script = script_4a(&[]);
    let exec = exec_4a(&script, false, false, 1.0, 900);
    exec.term
        .calls()
        .into_iter()
        .flat_map(|record| match record.call {
            Call::Run { text, .. } => vec![Event::Run(text)],
            Call::SendText { text, .. } => vec![Event::Text(text)],
            Call::SendKeys { keys, .. } => keys.into_iter().map(Event::Key).collect(),
            _ => vec![],
        })
        .collect()
}
fn junk_key() -> String {
    format!("sk-or-v1-{}", "0".repeat(64))
}
fn strings(events: &[Event]) -> impl Iterator<Item = &str> {
    events.iter().filter_map(|event| match event {
        Event::Run(s) | Event::Text(s) => Some(s.as_str()),
        Event::Key(_) => None,
    })
}
fn screen_events(events: &[Event]) -> &[Event] {
    let start = events
        .iter()
        .position(|e| matches!(e, Event::Text(s) if s == "/config"))
        .expect("first screen command");
    let end = events
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, e)| matches!(e, Event::Text(s) if s == "/quit").then_some(i))
        .expect("quit after screens");
    &events[start..end]
}
fn expected_screen_events() -> Vec<Event> {
    use Event::{Key as K, Text as T};
    let junk = junk_key();
    let raw: Vec<(&str, &str)> = vec![
        ("t", "/config"),
        ("k", "enter"),
        ("k", "right"),
        ("k", "right"),
        ("t", "/"),
        ("t", "compact"),
        ("k", "esc"),
        ("t", "/"),
        ("t", "api_key"),
        ("k", "esc"),
        ("k", "esc"),
        ("t", "/config"),
        ("k", "enter"),
        ("t", "/"),
        ("t", "resume_exchanges"),
        ("k", "enter"),
        ("k", "enter"),
        ("t", "abc"),
        ("k", "enter"),
        ("k", "esc"),
        ("t", "a"),
        ("k", "esc"),
        ("t", "/config delegation.api_key=not-a-real-key"),
        ("k", "enter"),
        ("k", "up"),
        ("k", "ctrl+c"),
        ("t", "/providers"),
        ("k", "enter"),
        ("k", "tab"),
        ("k", "tab"),
        ("k", "esc"),
        ("t", "/providers"),
        ("k", "enter"),
        ("t", "openrouter"),
        ("k", "enter"),
        ("t", &junk),
        ("k", "esc"),
        ("k", "esc"),
        ("t", "/models"),
        ("k", "enter"),
        ("t", "2"),
        ("t", "3"),
        ("t", "4"),
        ("t", "5"),
        ("k", "esc"),
        ("t", "/models"),
        ("k", "enter"),
        ("t", "5"),
        ("k", "left"),
        ("t", "a"),
        ("t", "uat-test"),
        ("k", "enter"),
        ("k", "right"),
        ("t", "a"),
        ("k", "esc"),
        ("k", "ctrl+s"),
        ("k", "esc"),
        ("t", "d"),
        ("t", "/models"),
        ("k", "enter"),
        ("t", "5"),
        ("k", "esc"),
        ("t", "/profiles"),
        ("k", "enter"),
        ("t", "/fixture"),
        ("k", "enter"),
        ("k", "esc"),
        ("k", "esc"),
        ("t", "/profiles"),
        ("k", "enter"),
        ("t", "n"),
        ("t", "Bad Name!"),
        ("k", "enter"),
        ("k", "ctrl+u"),
        ("t", "default"),
        ("k", "enter"),
        ("k", "ctrl+u"),
        ("t", " Fixture "),
        ("k", "enter"),
        ("k", "esc"),
        ("k", "esc"),
        ("t", "/doctor"),
        ("k", "enter"),
        ("k", "down"),
        ("k", "down"),
        ("k", "enter"),
        ("k", "esc"),
    ];
    raw.into_iter()
        .map(|(kind, value)| {
            if kind == "t" {
                T(value.to_owned())
            } else {
                K(value.to_owned())
            }
        })
        .collect()
}
fn assert_script(events: &[Event]) {
    assert_eq!(screen_events(events), expected_screen_events());
}
#[test]
fn nothing_typed_contains_unattended() {
    let events = input();
    assert_script(&events);
    assert!(strings(&events).all(|line| !line.contains("unattended")));
}
#[test]
fn slash_lines_are_the_allowed_list() {
    let events = input();
    assert_script(&events);
    let lines: Vec<_> = strings(&events)
        .filter(|line| line.starts_with('/') && !line.contains(' '))
        .collect();
    assert_eq!(
        lines,
        [
            "/config",
            "/",
            "/",
            "/config",
            "/",
            "/providers",
            "/providers",
            "/models",
            "/models",
            "/models",
            "/profiles",
            "/fixture",
            "/profiles",
            "/doctor",
            "/quit",
            "/quit"
        ]
    );
    assert_eq!(
        strings(&events)
            .filter(|s| s.starts_with("/config "))
            .collect::<Vec<_>>(),
        ["/config delegation.api_key=not-a-real-key"]
    );
}
#[test]
fn no_step_passes_restart_or_confirms_a_review() {
    let events = input();
    assert_script(&events);
    assert!(strings(&events).all(|s| !s.contains("--restart")));
}
#[test]
fn junk_key_is_the_only_secret_typed() {
    let events = input();
    assert_script(&events);
    let key = junk_key();
    let candidates: Vec<_> = strings(&events)
        .filter(|s| s.contains("sk-or-v1-"))
        .collect();
    assert_eq!(candidates, [key.as_str()]);
    assert_eq!(
        strings(&events)
            .filter(|s| s.contains("not-a-real-key"))
            .count(),
        1
    );
}
#[test]
fn no_enter_is_sent_while_a_key_card_is_open() {
    let events = input();
    assert_script(&events);
    let start = events
        .iter()
        .position(|e| matches!(e, Event::Text(s) if s == "openrouter"))
        .expect("provider filter");
    assert_eq!(events.get(start + 1), Some(&Event::Key("enter".into())));
    assert_eq!(events.get(start + 2), Some(&Event::Text(junk_key())));
    assert_eq!(events.get(start + 3), Some(&Event::Key("esc".into())));
}
#[test]
fn no_line_runs_hermes_update_or_config_set() {
    let events = input();
    assert_script(&events);
    assert!(strings(&events).all(|s| !s.contains("hermes update") && !s.contains("config set")));
}
