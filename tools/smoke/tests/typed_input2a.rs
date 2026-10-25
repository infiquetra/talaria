//! Everything the phase 2a steps send: no forbidden text, only the allowed lines.
//!
//! Each test collects all text and keys the run sends over every path of the step tests:
//! the happy run, every failing shape, and both skip runs. All fail until the stub
//! `run_smoke_2a` is replaced.

mod support;

use regex::Regex;
use smoke::SmokeConfig;
use support::Call;
use support::scripts2a::{
    Btw, Checklist, Editor, Exec2a, FileDrop, History, Mention, Pager, Palette, PaneState,
    PasteCollapse, PasteEditor, Resume, Shapes, Shell, Startup, Step, Theme, TmuxPane, TmuxQuit,
    ToolEdit, ToolShell, exec_2a, script_2a,
};

/// One sent input, in order, with whether the pager was open.
enum Event {
    Run(String),
    Text(String, bool),
    Keys(Vec<String>),
}

/// All inputs the run sent, in order, with the pager spans marked.
struct Sent {
    events: Vec<Event>,
    pastes: Vec<String>,
}

fn collect_one(shapes: &Shapes, stop: Step) -> Sent {
    let script = script_2a(shapes, stop);
    let exec: Exec2a = exec_2a(&script, |_: &mut SmokeConfig| {}, |term| term, |_| {});
    let mut sent = Sent {
        events: Vec::new(),
        pastes: exec.pastes().into_iter().map(|(_, text)| text).collect(),
    };
    let mut in_pager = false;
    for recorded in exec.term.calls() {
        match &recorded.call {
            Call::Run { text, .. } => sent.events.push(Event::Run(text.clone())),
            Call::SendText { text, .. } => sent.events.push(Event::Text(text.clone(), in_pager)),
            Call::SendKeys { keys, .. } => {
                sent.events.push(Event::Keys(keys.clone()));
                for key in keys {
                    if key == "ctrl+o" {
                        in_pager = true;
                    }
                    if key == "q" {
                        in_pager = false;
                    }
                }
            }
            _ => {}
        }
    }
    sent
}

/// Every submitted and typed text, with the pager flag of the typed ones.
fn texts(sent: &Sent) -> Vec<(&str, bool)> {
    sent.events
        .iter()
        .filter_map(|event| match event {
            Event::Run(text) => Some((text.as_str(), false)),
            Event::Text(text, in_pager) => Some((text.as_str(), *in_pager)),
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

/// Every path of the step tests: the happy run, every failing shape, both skip runs.
fn collect_all() -> Vec<Sent> {
    let mut shapes_list: Vec<(Shapes, Step)> = Vec::new();
    shapes_list.push((Shapes::default(), Step::TmuxQuit));
    for shape in [
        ToolShell::Happy,
        ToolShell::NoToken,
        ToolShell::NoHeader,
        ToolShell::NoOutput,
        ToolShell::OtherCmd,
    ] {
        shapes_list.push((
            Shapes {
                tool_shell: shape,
                ..Default::default()
            },
            Step::ToolShell,
        ));
    }
    for shape in [
        ToolEdit::Happy,
        ToolEdit::Debris,
        ToolEdit::NoStat,
        ToolEdit::NoDiffRows,
        ToolEdit::FileUnchanged,
        ToolEdit::RetryShell,
        ToolEdit::NoEditToolTwice,
        ToolEdit::NoToken,
    ] {
        shapes_list.push((
            Shapes {
                tool_edit: shape,
                ..Default::default()
            },
            Step::ToolEdit,
        ));
    }
    for shape in [
        Palette::Happy,
        Palette::NotOpen,
        Palette::NoBlock,
        Palette::GroupLacks,
        Palette::NotClosed,
    ] {
        shapes_list.push((
            Shapes {
                palette: shape,
                ..Default::default()
            },
            Step::Palette,
        ));
    }
    for shape in [
        Mention::Happy,
        Mention::NoRow,
        Mention::TabNothing,
        Mention::ReplyMisses,
    ] {
        shapes_list.push((
            Shapes {
                mention: shape,
                ..Default::default()
            },
            Step::Mention,
        ));
    }
    for shape in [
        Shell::Happy,
        Shell::NoOutput,
        Shell::NoRefusal,
        Shell::FolderRemoved,
    ] {
        shapes_list.push((
            Shapes {
                shell: shape,
                ..Default::default()
            },
            Step::Shell,
        ));
    }
    for shape in [
        Btw::Happy,
        Btw::NoCell,
        Btw::Empty,
        Btw::Failed,
        Btw::LacksWord,
    ] {
        shapes_list.push((
            Shapes {
                btw: shape,
                ..Default::default()
            },
            Step::Btw,
        ));
    }
    for shape in [
        Pager::Happy,
        Pager::NotOpen,
        Pager::NotFollowing,
        Pager::TokenNoMatch,
        Pager::ToolNoMatch,
        Pager::QStuck,
        Pager::Interrupt,
    ] {
        shapes_list.push((
            Shapes {
                pager: shape,
                ..Default::default()
            },
            Step::Pager,
        ));
    }
    for shape in [
        History::Happy,
        History::EmptyBox,
        History::LoadFail,
        History::ClearFail,
    ] {
        shapes_list.push((
            Shapes {
                history: shape,
                ..Default::default()
            },
            Step::History,
        ));
    }
    for shape in [Editor::Happy, Editor::DraftUnchanged, Editor::Sent] {
        shapes_list.push((
            Shapes {
                editor: shape,
                ..Default::default()
            },
            Step::Editor,
        ));
    }
    for shape in [
        Theme::Happy,
        Theme::ReportMissing,
        Theme::SkinUsage,
        Theme::ArgUsage,
        Theme::Refused,
    ] {
        shapes_list.push((
            Shapes {
                theme: shape,
                ..Default::default()
            },
            Step::Theme,
        ));
    }
    for shape in [
        PaneState::Happy,
        PaneState::Counting,
        PaneState::Working,
        PaneState::Idle,
    ] {
        shapes_list.push((
            Shapes {
                pane_state: shape,
                ..Default::default()
            },
            Step::PaneState,
        ));
    }
    for shape in [
        Checklist::Happy,
        Checklist::SkipTwice,
        Checklist::CollapseFail,
        Checklist::ExpandFail,
        Checklist::EndFail,
        Checklist::PanelLeft,
        Checklist::TwoRecords,
    ] {
        shapes_list.push((
            Shapes {
                checklist: shape,
                ..Default::default()
            },
            Step::Checklist,
        ));
    }
    for shape in [
        Resume::Happy,
        Resume::LocalCell,
        Resume::RecordTwice,
        Resume::PagerFail,
    ] {
        shapes_list.push((
            Shapes {
                resume: shape,
                ..Default::default()
            },
            Step::Resume,
        ));
    }
    shapes_list.push((
        Shapes {
            tmux_pane: TmuxPane::Happy,
            ..Default::default()
        },
        Step::TmuxPane,
    ));
    for shape in [
        PasteCollapse::Happy,
        PasteCollapse::NoLabel,
        PasteCollapse::Leak,
        PasteCollapse::NoReply,
    ] {
        shapes_list.push((
            Shapes {
                paste_collapse: shape,
                ..Default::default()
            },
            Step::PasteCollapse,
        ));
    }
    for shape in [FileDrop::Happy, FileDrop::NoLabel, FileDrop::NoReply] {
        shapes_list.push((
            Shapes {
                file_drop: shape,
                ..Default::default()
            },
            Step::FileDrop,
        ));
    }
    for shape in [
        PasteEditor::Happy,
        PasteEditor::NoLabel,
        PasteEditor::LabelLost,
        PasteEditor::NoReply,
    ] {
        shapes_list.push((
            Shapes {
                paste_editor: shape,
                ..Default::default()
            },
            Step::PasteEditor,
        ));
    }
    for shape in [TmuxQuit::Happy, TmuxQuit::ExitWrong] {
        shapes_list.push((
            Shapes {
                tmux_quit: shape,
                ..Default::default()
            },
            Step::TmuxQuit,
        ));
    }
    shapes_list.push((
        Shapes {
            startup: Startup::NoAgent,
            ..Default::default()
        },
        Step::TmuxQuit,
    ));
    shapes_list.push((
        Shapes {
            checklist: Checklist::SkipTwice,
            ..Default::default()
        },
        Step::TmuxQuit,
    ));
    shapes_list
        .iter()
        .map(|(shapes, stop)| collect_one(shapes, *stop))
        .collect()
}

#[test]
fn nothing_typed_contains_unattended() {
    for sent in collect_all() {
        for (text, _) in texts(&sent) {
            assert!(
                !text.to_lowercase().contains("unattended"),
                "no sent text may hold unattended: {text:?}"
            );
        }
        for text in &sent.pastes {
            assert!(
                !text.to_lowercase().contains("unattended"),
                "no pasted text may hold unattended: {text:?}"
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
    let allowed = ["/quit", "/vers", "/theme", "/theme purple", "/skin ../etc"];
    let is_allowed = |text: &str| allowed.contains(&text) || text.starts_with("/btw ");
    for sent in collect_all() {
        let mut index = 0;
        while index < sent.events.len() {
            match &sent.events[index] {
                // A submit always sends.
                Event::Run(text) if text.starts_with('/') => {
                    assert!(
                        is_allowed(text),
                        "unexpected submitted slash line: {text:?}"
                    );
                    assert!(
                        !text.starts_with("/version"),
                        "/version is completed from /vers, never typed: {text:?}"
                    );
                }
                // A typed slash line counts when Enter follows outside the pager. A bare `/`
                // that is cleared, and search text inside the pager, are exempt; a bare `/`
                // followed by Enter fails.
                Event::Text(text, in_pager) if text.starts_with('/') && !in_pager => {
                    let sent_with_enter = matches!(
                        sent.events.get(index + 1),
                        Some(Event::Keys(keys)) if keys.iter().any(|key| key == "enter")
                    );
                    if *text == "/" {
                        assert!(
                            !sent_with_enter,
                            "a bare / is typed and cleared, never sent"
                        );
                    } else if sent_with_enter {
                        assert!(is_allowed(text), "unexpected typed slash line: {text:?}");
                        assert!(
                            !text.starts_with("/version"),
                            "/version is completed from /vers, never typed: {text:?}"
                        );
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }
}

#[test]
fn only_destructive_line_is_the_refused_rm_r_keepme() {
    for sent in collect_all() {
        for (text, _) in texts(&sent) {
            if !text.starts_with('!') {
                continue;
            }
            assert!(
                text.starts_with("!echo ") || text == "!rm -r ./keepme",
                "the only destructive line is the refused one: {text:?}"
            );
        }
    }
}

#[test]
fn no_line_runs_hermes_update_or_config_set() {
    let absolute = Regex::new(r"/[A-Za-z0-9_.\-]+/[A-Za-z0-9_.\-/]*").expect("regex");
    let drop_path = Regex::new(r"^/.*/notes-[0-9a-f]+\.txt$").expect("regex");
    for sent in collect_all() {
        for (text, _) in texts(&sent) {
            for forbidden in ["hermes update", "config.set", "rm -rf"] {
                assert!(
                    !text.contains(forbidden),
                    "no line runs {forbidden}: {text:?}"
                );
            }
            // Launch lines hold the scratch path and the binary path; everything else must
            // hold no absolute path.
            if text.contains("--no-ensure") || text.contains("tmux new-session") {
                continue;
            }
            assert!(
                absolute.find(text).is_none(),
                "no absolute path outside the launch lines: {text:?}"
            );
        }
        for text in &sent.pastes {
            if text.starts_with('/') {
                assert!(
                    drop_path.is_match(text),
                    "the only pasted path is the drop file: {text:?}"
                );
            } else {
                assert!(
                    absolute.find(text).is_none(),
                    "no absolute path in pasted content: {text:?}"
                );
            }
        }
    }
}
