//! Everything the phase 3 steps send: no forbidden text, only the allowed lines and keys.
//!
//! Each test collects what the run sends over the passing run, every failing shape, the
//! retry shape and the skip run. All fail until the stub `run_smoke_3` is replaced.

mod support;

use std::path::PathBuf;

use support::scripts3::{
    Attach, BgLaunch, Exec3, FleetList, FleetQuiet, FleetView, LocalCommands, NeedsYou, OpenInPane,
    Shapes, Step, WorktreeLaunch, exec_3, script_3,
};
use support::{Call, FailBuilder, ScriptBuilder};

/// One sent input, in order.
enum Event {
    Run(String),
    Text(String),
    Keys(Vec<String>),
}

/// All inputs the run sent, in order.
struct Sent {
    events: Vec<Event>,
}

fn collect_script(script: &ScriptBuilder) -> Sent {
    let exec: Exec3 = exec_3(script, |_| {}, |term| term);
    let mut sent = Sent { events: Vec::new() };
    for recorded in exec.term.calls() {
        match &recorded.call {
            Call::Run { text, .. } => sent.events.push(Event::Run(text.clone())),
            Call::SendText { text, .. } => sent.events.push(Event::Text(text.clone())),
            Call::SendKeys { keys, .. } => sent.events.push(Event::Keys(keys.clone())),
            _ => {}
        }
    }
    sent
}

fn collect_one(shapes: &Shapes, stop: Step) -> Sent {
    let script = script_3(shapes, stop);
    collect_script(&script)
}

fn texts(sent: &Sent) -> Vec<&str> {
    sent.events
        .iter()
        .filter_map(|event| match event {
            Event::Run(text) | Event::Text(text) => Some(text.as_str()),
            Event::Keys(_) => None,
        })
        .collect()
}

fn keys(sent: &Sent) -> Vec<&str> {
    sent.events
        .iter()
        .filter_map(|event| match event {
            Event::Keys(keys) => Some(keys.iter().map(String::as_str).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn push(list: &mut Vec<(Shapes, Step)>, shapes: Shapes, stop: Step) {
    list.push((shapes, stop));
}

/// Every path of the step tests: the passing run, every failing shape, the retry and the skip.
fn collect_all() -> Vec<Sent> {
    let mut shapes_list: Vec<(Shapes, Step)> = Vec::new();
    push(&mut shapes_list, Shapes::default(), Step::NeedsYou);
    for shape in [
        FleetQuiet::Pass,
        FleetQuiet::Exit(7),
        FleetQuiet::Narrow,
        FleetQuiet::Busy,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                fleet_quiet: shape,
                ..Default::default()
            },
            Step::FleetQuiet,
        );
    }
    for shape in [
        BgLaunch::Pass,
        BgLaunch::Exit(1),
        BgLaunch::Output,
        BgLaunch::Slow,
        BgLaunch::RefusalExit(1),
        BgLaunch::RefusalText,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                bg_launch: shape,
                ..Default::default()
            },
            Step::BgLaunch,
        );
    }
    for shape in [
        WorktreeLaunch::Pass,
        WorktreeLaunch::Exit(1),
        WorktreeLaunch::Text,
        WorktreeLaunch::TreeMissing,
        WorktreeLaunch::NotFresh,
        WorktreeLaunch::CheckoutDirty,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                worktree_launch: shape,
                ..Default::default()
            },
            Step::WorktreeLaunch,
        );
    }
    for shape in [
        FleetList::Pass,
        FleetList::Exit(1),
        FleetList::NeverDone,
        FleetList::WorktreeMissing,
        FleetList::Unserved,
        FleetList::NoGateway,
        FleetList::NoGatewayStarted,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                fleet_list: shape,
                ..Default::default()
            },
            Step::FleetList,
        );
    }
    for shape in [
        Attach::Pass,
        Attach::NoResumed,
        Attach::Ambiguous,
        Attach::Exit,
        Attach::NoMatch,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                attach: shape,
                ..Default::default()
            },
            Step::Attach,
        );
    }
    for shape in [
        LocalCommands::Pass,
        LocalCommands::Tasks,
        LocalCommands::AgentsOption,
        LocalCommands::Processes,
        LocalCommands::BgUsage,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                local_commands: shape,
                ..Default::default()
            },
            Step::LocalCommands,
        );
    }
    for shape in [
        FleetView::Pass,
        FleetView::Open,
        FleetView::NoMatch,
        FleetView::Close,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                fleet_view: shape,
                ..Default::default()
            },
            Step::FleetView,
        );
    }
    for shape in [OpenInPane::Pass, OpenInPane::Notice, OpenInPane::NotInHerdr] {
        push(
            &mut shapes_list,
            Shapes {
                open_in_pane: shape,
                ..Default::default()
            },
            Step::OpenInPane,
        );
    }
    for shape in [
        NeedsYou::Pass,
        NeedsYou::Launch,
        NeedsYou::NeverListed,
        NeedsYou::TooOld,
        NeedsYou::Notice,
        NeedsYou::Segment,
        NeedsYou::Peek,
        NeedsYou::NoDeny,
        NeedsYou::NotCleared,
        NeedsYou::CommandRan,
        NeedsYou::SegmentStays,
        NeedsYou::SkipUnasked,
        NeedsYou::RetryThenFail,
    ] {
        push(
            &mut shapes_list,
            Shapes {
                needs_you: shape,
                ..Default::default()
            },
            Step::NeedsYou,
        );
    }
    push(
        &mut shapes_list,
        Shapes {
            still_after_slash: true,
            ..Default::default()
        },
        Step::NeedsYou,
    );
    let mut all: Vec<Sent> = shapes_list
        .iter()
        .map(|(shapes, stop)| collect_one(shapes, *stop))
        .collect();
    let rejected = script_3(&Shapes::default(), Step::NeedsYou)
        .fail(FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key"));
    all.push(collect_script(&rejected));
    all
}

/// Files under the smoke crate's source that hold the forbidden word.
fn forbidden_word_in_src() -> Vec<String> {
    let mut hits = Vec::new();
    let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if text.to_lowercase().contains("unattended") {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    hits
}

#[test]
fn nothing_typed_contains_unattended() {
    for sent in collect_all() {
        for text in texts(&sent) {
            assert!(
                !text.to_lowercase().contains("unattended"),
                "no sent text may hold the forbidden word: {text:?}"
            );
        }
        for key in keys(&sent) {
            assert!(
                !key.to_lowercase().contains("unattended"),
                "no sent key may hold the forbidden word: {key:?}"
            );
        }
    }
    let hits = forbidden_word_in_src();
    assert!(
        hits.is_empty(),
        "tools/smoke/src holds the forbidden word: {hits:?}"
    );
}

#[test]
fn no_dangerous_slash_commands_are_typed() {
    let forbidden = ["/model", "/profile", "/permissions", "/yolo", "/compact"];
    let fleet_letters = ["r", "x", "p"];
    for sent in collect_all() {
        for text in texts(&sent) {
            for needle in forbidden {
                assert!(
                    !text.starts_with(needle),
                    "no sent text may start {needle}: {text:?}"
                );
            }
            assert!(
                !text.contains("hermes config") && !text.contains("hermes update"),
                "no sent text changes Hermes: {text:?}"
            );
            if text.contains("--ensure") {
                assert!(
                    text.contains("--no-ensure"),
                    "the suite never starts a gateway: {text:?}"
                );
            }
        }
        for event in &sent.events {
            if let Event::Text(text) = event {
                assert!(
                    !fleet_letters.contains(&text.as_str()),
                    "the fleet view never gets r, x or p: {text:?}"
                );
            }
        }
        for key in keys(&sent) {
            assert_ne!(key, "shift+p", "push is not sent");
            assert_ne!(key, "alt+s", "the subagent key is not sent");
            assert_ne!(key, "shift+x", "stop is not sent");
        }
    }
}

#[test]
fn every_launch_line_carries_no_ensure() {
    for sent in collect_all() {
        for text in texts(&sent) {
            if text.contains("fleet list") {
                assert!(
                    !text.contains("--no-ensure"),
                    "fleet list has no such option: {text}"
                );
                continue;
            }
            let launch = text.contains("--bg")
                || text.contains(" attach ")
                || text.contains("--provider ollama-cloud");
            if launch {
                assert!(
                    text.contains("--no-ensure"),
                    "every talaria launch carries --no-ensure: {text}"
                );
            }
        }
    }
}

#[test]
fn only_allowed_keys_are_sent() {
    let allowed = ["enter", "esc", "ctrl+c", "alt+a"];
    let recorded = ["alt+s", "shift+p", "shift+x", "ctrl+s"];
    for sent in collect_all() {
        for key in keys(&sent) {
            assert!(
                allowed.contains(&key),
                "unexpected key {key:?}; allowed {allowed:?}"
            );
            assert!(
                !recorded.contains(&key),
                "a recorded key name is not sent: {key}"
            );
        }
    }
}

#[test]
fn needs_you_types_only_the_digit_found_on_screen() {
    let sent = collect_one(&Shapes::default(), Step::NeedsYou);
    let digits: Vec<&str> = texts(&sent)
        .into_iter()
        .filter(|text| text.len() == 1 && text.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .collect();
    assert_eq!(
        digits,
        ["1"],
        "the only typed digit is the 1 beside deny on the card"
    );
}

#[test]
fn slash_lines_get_no_submit_fallback() {
    let shapes = Shapes {
        still_after_slash: true,
        ..Default::default()
    };
    let sent = collect_one(&shapes, Step::NeedsYou);
    let slash = [
        "/tasks",
        "/agents frobnicate",
        "/processes",
        "/bg",
        "/fleet",
    ];
    for (index, event) in sent.events.iter().enumerate() {
        let Event::Run(text) = event else {
            continue;
        };
        if !slash.contains(&text.as_str()) {
            continue;
        }
        if let Some(Event::Keys(pressed)) = sent.events.get(index + 1) {
            assert!(
                !pressed.iter().any(|key| key == "enter"),
                "no submit fallback after {text}"
            );
        }
    }
}
