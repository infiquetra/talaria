//! Everything the phase 2b steps send: no forbidden text, only the allowed lines.
//!
//! Each test collects all text and keys the run sends over every path of the step tests:
//! the passing run, every failing shape, the retry shapes and the skip run. All fail until
//! the stub `run_smoke_2b` is replaced.

mod support;

use smoke::SmokeConfig;
use support::Call;
use support::scripts2b::{
    Cd, Context, ContextEmpty, Exec2b, Headless, HermesCommands, ModelPicker, ModelRefusal,
    Permission, Queue, Quit, RenameExport, Rewind, Shapes, Startup, SteerIdle, Step, exec_2b,
    script_2b,
};
use support::{FailBuilder, ScriptBuilder};

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
    let exec: Exec2b = exec_2b(script, |_: &mut SmokeConfig| {}, |term| term);
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
    let script = script_2b(shapes, stop);
    collect_script(&script)
}

/// Every submitted and typed text.
fn texts(sent: &Sent) -> Vec<&str> {
    sent.events
        .iter()
        .filter_map(|event| match event {
            Event::Run(text) => Some(text.as_str()),
            Event::Text(text) => Some(text.as_str()),
            Event::Keys(_) => None,
        })
        .collect()
}

/// Every sent key.
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

/// Every path of the step tests: the passing run, every failing shape, the retry shapes
/// and the skip run.
fn collect_all() -> Vec<Sent> {
    let mut shapes_list: Vec<(Shapes, Step)> = Vec::new();
    shapes_list.push((Shapes::default(), Step::Headless));
    for shape in [
        ContextEmpty::Pass,
        ContextEmpty::ContextLineMissing,
        ContextEmpty::UsageLineMissing,
    ] {
        shapes_list.push((
            Shapes {
                context_empty: shape,
                ..Default::default()
            },
            Step::ContextEmpty,
        ));
    }
    for shape in [
        Permission::Pass,
        Permission::NoOffSegment,
        Permission::NoOffNotice,
        Permission::NoRestore,
    ] {
        shapes_list.push((
            Shapes {
                permission: shape,
                ..Default::default()
            },
            Step::Permission,
        ));
    }
    shapes_list.push((
        Shapes {
            startup: Startup::OffAtStart,
            ..Default::default()
        },
        Step::Permission,
    ));
    for shape in [
        ModelPicker::Pass,
        ModelPicker::OpenMissing,
        ModelPicker::RowsMissing,
        ModelPicker::NoMatchMissing,
        ModelPicker::EscSteps,
    ] {
        shapes_list.push((
            Shapes {
                model_picker: shape,
                ..Default::default()
            },
            Step::ModelPicker,
        ));
    }
    for shape in [
        ModelRefusal::Pass,
        ModelRefusal::RefusalMissing,
        ModelRefusal::ModelChanged,
    ] {
        shapes_list.push((
            Shapes {
                model_refusal: shape,
                ..Default::default()
            },
            Step::ModelRefusal,
        ));
    }
    for shape in [
        Rewind::Pass,
        Rewind::ReplyTimeout,
        Rewind::OverlayMissing,
        Rewind::ListRows,
        Rewind::RewoundMissing,
        Rewind::ComposerNotRestored,
        Rewind::WrongCodeWord,
        Rewind::OverlayLeftText,
    ] {
        shapes_list.push((
            Shapes {
                rewind: shape,
                ..Default::default()
            },
            Step::Rewind,
        ));
    }
    for shape in [
        SteerIdle::Pass,
        SteerIdle::NoticeMissing,
        SteerIdle::ReplyMissing,
        SteerIdle::MissThenHit,
        SteerIdle::MissWhileWorking,
    ] {
        shapes_list.push((
            Shapes {
                steer_idle: shape,
                ..Default::default()
            },
            Step::SteerIdle,
        ));
    }
    for shape in [
        Context::Pass,
        Context::HeaderMissing,
        Context::RowsMissing,
        Context::UsageMissing,
        Context::AggressiveMissing,
        Context::ArgumentMissing,
        Context::CompressedAppeared,
    ] {
        shapes_list.push((
            Shapes {
                context: shape,
                ..Default::default()
            },
            Step::Context,
        ));
    }
    for shape in [
        HermesCommands::Pass,
        HermesCommands::FastMissing,
        HermesCommands::LoopMissing,
        HermesCommands::LoopLine,
    ] {
        shapes_list.push((
            Shapes {
                hermes_commands: shape,
                ..Default::default()
            },
            Step::HermesCommands,
        ));
    }
    for shape in [
        RenameExport::Pass,
        RenameExport::RenamedMissing,
        RenameExport::TitleMissing,
        RenameExport::ExportedMissing,
        RenameExport::FileBad,
        RenameExport::BadNameMissing,
        RenameExport::BadNameWrote,
    ] {
        shapes_list.push((
            Shapes {
                rename_export: shape,
                ..Default::default()
            },
            Step::RenameExport,
        ));
    }
    for shape in [
        Queue::Pass,
        Queue::IndicatorMissing,
        Queue::PreviewTwice,
        Queue::FirstMissing,
        Queue::OmegaMissing,
        Queue::OrderWrong,
        Queue::IndicatorLeft,
        Queue::MissThenHit,
        Queue::TwoMisses,
        Queue::MissWhileWorking,
    ] {
        shapes_list.push((
            Shapes {
                queue: shape,
                ..Default::default()
            },
            Step::Queue,
        ));
    }
    for shape in [
        Cd::Pass,
        Cd::MovedMissing,
        Cd::SegmentWrong,
        Cd::ShowMissing,
        Cd::MissingFolder,
    ] {
        shapes_list.push((
            Shapes {
                cd: shape,
                ..Default::default()
            },
            Step::Cd,
        ));
    }
    for shape in [Quit::Pass, Quit::TokenMissing] {
        shapes_list.push((
            Shapes {
                quit: shape,
                ..Default::default()
            },
            Step::Quit,
        ));
    }
    for shape in [
        Headless::Pass,
        Headless::TextRun,
        Headless::NoGatewayJson,
        Headless::NoGatewayText,
        Headless::CommandTimeout,
    ] {
        shapes_list.push((
            Shapes {
                headless: shape,
                ..Default::default()
            },
            Step::Headless,
        ));
    }
    let mut all: Vec<Sent> = shapes_list
        .iter()
        .map(|(shapes, stop)| collect_one(shapes, *stop))
        .collect();
    let skip = script_2b(&Shapes::default(), Step::Permission)
        .fail(FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key"));
    all.push(collect_script(&skip));
    all
}

#[test]
fn nothing_typed_contains_unattended() {
    for sent in collect_all() {
        for text in texts(&sent) {
            assert!(
                !text.to_lowercase().contains("unattended"),
                "no sent text may hold unattended: {text:?}"
            );
        }
        for key in keys(&sent) {
            assert!(
                !key.to_lowercase().contains("unattended"),
                "no sent key may hold unattended: {key:?}"
            );
        }
    }
}

#[test]
fn slash_lines_are_the_allowed_list() {
    let allowed = [
        "/quit",
        "/context",
        "/usage",
        "/model",
        "/model no-such-model-xyz --provider ollama-cloud",
        "/steer Reply with the word idle in capital letters.",
        "/compact --aggressive",
        "/context now",
        "/fast",
        "/loop 45m",
        "/rename",
        "/export",
        "/export ./notes.txt",
        "/cd ../b",
        "/cd",
        "/cd ./missing-folder-smoke",
        "/permissions session default",
    ];
    let is_allowed = |text: &str| {
        allowed.contains(&text)
            || text.starts_with("/rename fixture smoke ")
                && text["/rename fixture smoke ".len()..]
                    .chars()
                    .all(|c| !c.is_whitespace())
    };
    for sent in collect_all() {
        for event in &sent.events {
            match event {
                // Submits travel as runs; the launch line is not a slash line.
                Event::Run(text) if text.starts_with('/') && !text.contains("--no-ensure") => {
                    assert!(is_allowed(text), "unexpected slash line: {text:?}");
                }
                // Typed text is never a slash line: only zzzz and c are typed.
                Event::Text(text) => {
                    assert!(!text.starts_with('/'), "no typed slash line: {text:?}");
                }
                _ => {}
            }
        }
    }
}

#[test]
fn permissions_line_is_only_session_default() {
    for sent in collect_all() {
        let mut count = 0;
        for text in texts(&sent) {
            if text.starts_with("/permissions") {
                count += 1;
                assert_eq!(text, "/permissions session default", "the one cleanup line");
            }
        }
        assert!(
            count <= 1,
            "the cleanup line is sent at most once per run: {count}"
        );
    }
}

#[test]
fn no_line_restores_files_or_changes_the_profile() {
    let forbidden = [
        "/permissions off",
        "/permissions manual",
        "/permissions smart",
        "/yolo",
        "/rollback",
        "/undo",
        "/retry",
        "/branch",
        "/resume",
        "/skin",
        "hermes update",
        "hermes config",
    ];
    let allowed_keys = ["shift+tab", "backtab", "esc", "enter", "ctrl+c"];
    for sent in collect_all() {
        for text in texts(&sent) {
            for needle in forbidden {
                assert!(
                    !text.contains(needle),
                    "no sent text may hold {needle}: {text:?}"
                );
            }
            if text.contains("/compact") {
                assert!(
                    text.contains("/compact --aggressive"),
                    "only the refused aggressive form: {text:?}"
                );
            }
            if text.contains("talaria") {
                assert!(
                    text.contains("--no-ensure"),
                    "no text starts the gateway: {text:?}"
                );
            }
        }
        for key in keys(&sent) {
            assert!(allowed_keys.contains(&key), "unexpected key: {key:?}");
            assert_ne!(key, "f", "the rewind list never restores files");
        }
    }
}

#[test]
fn prompts_are_the_documented_prompts() {
    let code = [
        "Remember the code word heron. Reply ok.",
        "Now remember the code word lark instead. Reply ok.",
        "Which code word should you remember? One word.",
    ];
    let long = "Run the shell command sleep 25, then reply with the word first in capital letters.";
    let omega = "Reply with the word omega in capital letters.";
    let token_prefix = "Without using any tools, join these two words with a hyphen and reply with only the result: ";
    for sent in collect_all() {
        for event in &sent.events {
            let Event::Run(text) = event else {
                continue;
            };
            if text.starts_with('/') || text.contains("--no-ensure --profile") {
                continue;
            }
            if text.starts_with('\'') || text.starts_with("HERMES_HOME=") {
                assert!(
                    text.contains("1234 plus 4321") || text.contains("--output-format json"),
                    "the two headless command lines: {text:?}"
                );
                continue;
            }
            let is_token =
                text.starts_with(token_prefix) && text[token_prefix.len()..].contains(" and ");
            assert!(
                code.contains(&text.as_str()) || text == long || text == omega || is_token,
                "unexpected prompt: {text:?}"
            );
        }
    }
}
