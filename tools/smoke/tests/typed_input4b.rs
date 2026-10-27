//! The browse suite sends only its prescribed text and keys.
#[path = "support/scripts4b.rs"]
mod scripts4b;
mod support;

use scripts4b::{exec_4b, script_4b};
use support::Call;

const SENTINEL: &str = "sk-or-v1-talaria-sentinel-0000000000000000";

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Run(String),
    Text(String),
    Key(String),
}

fn input() -> Vec<Event> {
    let run = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    assert_eq!(run.report.exit_code(), 0, "{}", run.report.step_lines());
    captured_input(&run)
}

fn captured_input(run: &scripts4b::Exec4b) -> Vec<Event> {
    run.term
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

fn strings(events: &[Event]) -> impl Iterator<Item = &str> {
    events.iter().filter_map(|event| match event {
        Event::Run(text) | Event::Text(text) => Some(text.as_str()),
        Event::Key(_) => None,
    })
}

fn screen_events(events: &[Event]) -> &[Event] {
    let start = events
        .iter()
        .position(|event| matches!(event, Event::Text(text) if text == "/mcp"))
        .expect("first screen command");
    let end = events
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, event)| {
            matches!(event, Event::Run(text) | Event::Text(text) if text == "/quit")
                .then_some(index)
        })
        .expect("quit after the screens");
    &events[start..end]
}

fn expected_screen_events() -> Vec<Event> {
    use Event::{Key as K, Text as T};
    vec![
        T("/mcp".into()),
        K("enter".into()),
        K("tab".into()),
        T("deepwiki".into()),
        K("esc".into()),
        K("esc".into()),
        T("/capabilities".into()),
        K("enter".into()),
        T("3".into()),
        T("4".into()),
        T("a".into()),
        K("esc".into()),
        T("/memory-settings".into()),
        K("enter".into()),
        K("esc".into()),
        T("/memory pending".into()),
        K("enter".into()),
        T("/curator status".into()),
        K("enter".into()),
        T("/cron".into()),
        K("enter".into()),
        K("esc".into()),
        T("/messaging".into()),
        K("enter".into()),
        T("/signal".into()),
        K("enter".into()),
        K("esc".into()),
        K("esc".into()),
        K("esc".into()),
        T("/connectors".into()),
        K("enter".into()),
        K("esc".into()),
        T(format!("/configure use my openrouter key {SENTINEL}")),
        K("enter".into()),
        K("up".into()),
        K("ctrl+c".into()),
    ]
}

fn assert_script(events: &[Event]) {
    assert_eq!(screen_events(events), expected_screen_events());
}

#[test]
fn nothing_typed_contains_unattended_or_finite() {
    let events = input();
    assert_script(&events);
    assert!(strings(&events).all(|line| !line.contains("unattended") && !line.contains("finite")));
}

#[test]
fn slash_lines_are_the_allowed_list() {
    let events = input();
    assert_script(&events);
    let lines: Vec<_> = strings(&events)
        .filter(|line| line.starts_with('/'))
        .collect();
    assert_eq!(
        lines,
        [
            "/mcp",
            "/capabilities",
            "/memory-settings",
            "/memory pending",
            "/curator status",
            "/cron",
            "/messaging",
            "/signal",
            "/connectors",
            &format!("/configure use my openrouter key {SENTINEL}"),
            "/quit",
            "/quit",
        ]
    );
}

#[test]
fn no_step_passes_restart_or_confirms_anything() {
    let events = input();
    assert_script(&events);
    assert!(strings(&events).all(|line| !line.contains("--restart")));
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::Key(key) if ["ctrl+s", "ctrl+y", "t", "y"].contains(&key.as_str())
    )));
}

#[test]
fn the_sentinel_is_the_only_secret_typed() {
    let events = input();
    assert_script(&events);
    let candidates: Vec<_> = strings(&events)
        .filter(|line| line.contains("sk-") || line.contains("api_key") || line.contains("token="))
        .collect();
    assert_eq!(
        candidates,
        [format!("/configure use my openrouter key {SENTINEL}")]
    );
}

#[test]
fn no_enter_is_sent_while_a_card_or_consent_is_open() {
    let events = input();
    assert_script(&events);
    let screens = screen_events(&events);
    let signal = screens
        .iter()
        .position(|event| matches!(event, Event::Text(text) if text == "/signal"))
        .expect("Signal filter");
    assert_eq!(screens[signal + 1], Event::Key("enter".into()));
    assert_eq!(
        &screens[signal + 2..signal + 5],
        [
            Event::Key("esc".into()),
            Event::Key("esc".into()),
            Event::Key("esc".into())
        ]
    );
    let consent = screens
        .iter()
        .position(|event| matches!(event, Event::Text(text) if text == "/connectors"))
        .expect("connectors command");
    assert_eq!(screens[consent + 1], Event::Key("enter".into()));
    assert_eq!(screens[consent + 2], Event::Key("esc".into()));

    let run = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    assert_eq!(run.report.exit_code(), 0, "{}", run.report.step_lines());
    for (index, recorded) in run.term.calls().iter().enumerate() {
        if let Call::SendKeys { pane, keys } = &recorded.call
            && keys.iter().any(|key| key == "enter")
        {
            let latest = run.reads.iter().rev().find(|read| {
                read.index < index
                    && read.pane == *pane
                    && read.source == smoke::ReadSource::Visible
            });
            if let Some(read) = latest {
                assert!(
                    !read.text.contains("SIGNAL_HTTP_URL")
                        && !read.text.contains(
                            "Connectors run through Nous Research's hosted tool gateway."
                        ),
                    "Enter was sent while a card or consent text was visible: {}",
                    read.text
                );
            }
        }
    }

    let fallback = exec_4b(
        &script_4b(&[("@submit-fallback", "", "")]),
        false,
        false,
        1.0,
        900,
    );
    assert_eq!(
        fallback.report.exit_code(),
        0,
        "{}",
        fallback.report.step_lines()
    );
    let mut expected = expected_screen_events();
    expected.insert(2, Event::Key("enter".into()));
    assert_eq!(screen_events(&captured_input(&fallback)), expected);
    assert_eq!(fallback.report.fallback_steps, ["mcp-browse"]);
    assert_eq!(
        fallback
            .report
            .step_lines()
            .lines()
            .filter(|line| *line == "NOTE submit fallback: sent Enter (step mcp-browse)")
            .count(),
        1
    );
    let calls = fallback.term.calls();
    let submit = calls
        .iter()
        .position(|recorded| {
            matches!(
                &recorded.call, Call::SendText { text, .. } if text == "/mcp"
            )
        })
        .expect("submitted command");
    let tab = calls
        .iter()
        .enumerate()
        .skip(submit)
        .find_map(|(index, recorded)| {
            matches!(&recorded.call, Call::SendKeys { keys, .. } if keys == &["tab"])
                .then_some(index)
        })
        .expect("catalogue tab key");
    let enters: Vec<_> = calls
        .iter()
        .enumerate()
        .take(tab)
        .skip(submit)
        .filter_map(|(index, recorded)| {
            matches!(&recorded.call, Call::SendKeys { keys, .. } if keys == &["enter"])
                .then_some(index)
        })
        .collect();
    assert_eq!(enters.len(), 2, "only one fallback Enter per submission");
    let before_fallback = fallback
        .reads
        .iter()
        .rev()
        .find(|read| read.index < enters[1] && read.source == smoke::ReadSource::Visible)
        .expect("composer read before the fallback");
    assert!(
        before_fallback
            .text
            .lines()
            .any(|line| line.trim() == "> /mcp")
    );
    for forbidden in [
        "MCP servers · profile",
        "Capabilities · profile",
        "Memory · profile",
        "Scheduled jobs · profile",
        "Connectors and messaging · profile",
        "SIGNAL_HTTP_URL",
        "enter continue · esc back",
        "That request contains",
        "agent:",
    ] {
        assert!(
            !before_fallback.text.contains(forbidden),
            "fallback while {forbidden} was visible"
        );
    }
}

#[test]
fn no_ctrl_s_and_no_ctrl_y_is_sent() {
    let events = input();
    assert_script(&events);
    assert!(!events.iter().any(|event| matches!(
        event, Event::Key(key) if key == "ctrl+s" || key == "ctrl+y"
    )));
}

#[test]
fn no_line_runs_hermes_update_or_config_set() {
    let events = input();
    assert_script(&events);
    assert!(
        strings(&events)
            .all(|line| !line.contains("hermes update") && !line.contains("config set"))
    );
}
