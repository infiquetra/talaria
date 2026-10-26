//! Red-run tests 38 to 46 (#242): the dock, the panel and the block.
//!
//! Expected snapshots are literal strings written from the screen rules
//! before any code; no snapshot file or approval tool is used. Red: the stub
//! renderers return no lines, and every test asserts at least one drawn line
//! or a parsed frame. (The trees below build through the core stubs, so until
//! the tree lands these fail in the core; once it does, they fail in the
//! renderers.)

mod support;

use std::path::PathBuf;

use hermes_contract::events::EventFrame;
use ratatui::text::Line;
use serde_json::{Value, json};
use support::{ByteSink, Vt, probe};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Caps, Clock, FinishedBatch, Spawning, SubagentEvent, SubagentTree};
use talaria_tui::keys::{Action, KeyMap};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::subagents::{
    AgentsPanel, PanelHeader, SubagentDock, SubagentStyle, TailView, batch_block_lines,
    format_elapsed,
};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};
use unicode_width::UnicodeWidthStr;

fn key() -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-test".to_owned(),
    }
}

fn clock_ms(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

fn text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn texts(lines: &[Line<'static>]) -> Vec<String> {
    lines.iter().map(text).collect()
}

fn payload(id: &str, goal: &str) -> Value {
    json!({
        "subagent_id": id,
        "goal": goal,
        "task_index": 0,
        "task_count": 1,
        "model": "minimax-m3.1-flash",
    })
}

fn with(payload: Value, key: &str, value: Value) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

fn apply(tree: &mut SubagentTree, kind: &str, body: Value, ms: u64) {
    let params = json!({
        "type": kind,
        "session_id": "local-test",
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": body,
    });
    let frame = EventFrame::from_params(&params).expect("contract-shaped params");
    let event = SubagentEvent::from_notification(&frame.notification).expect("subagent event");
    tree.apply(&event, clock_ms(ms));
}

fn start(tree: &mut SubagentTree, id: &str, goal: &str, ms: u64) {
    apply(
        &mut *tree,
        "subagent.spawn_requested",
        payload(id, goal),
        ms,
    );
    apply(&mut *tree, "subagent.start", payload(id, goal), ms + 1);
}

/// The three rows of the screen rules' panel sample: two running, one
/// finished. Row one also carries the detail view's fields.
fn sample_tree() -> SubagentTree {
    let mut tree = SubagentTree::new();
    start(
        &mut tree,
        "child-1",
        "list the files in the current directory",
        1_000,
    );
    let tool = with(
        with(
            payload("child-1", "list the files in the current directory"),
            "tool_name",
            json!("Terminal"),
        ),
        "tool_preview",
        json!("ls -la"),
    );
    apply(
        &mut tree,
        "subagent.tool",
        with(tool, "tool_count", json!(3)),
        1_100,
    );
    let mut rich = payload("child-1", "list the files in the current directory");
    rich = with(rich, "toolsets", json!(["shell"]));
    rich = with(rich, "input_tokens", json!(100));
    rich = with(rich, "output_tokens", json!(50));
    rich = with(rich, "reasoning_tokens", json!(10));
    rich = with(rich, "api_calls", json!(7));
    rich = with(
        rich,
        "files_read",
        json!((1..=12).map(|n| format!("file{n}.txt")).collect::<Vec<_>>()),
    );
    rich = with(
        rich,
        "summary",
        json!("Found 4 files in the working directory and stopped there"),
    );
    rich = with(rich, "accepting_steer", json!(true));
    apply(&mut tree, "subagent.progress", rich, 1_200);

    start(&mut tree, "child-2", "read notes.txt", 5_000);
    let tool = with(
        with(
            payload("child-2", "read notes.txt"),
            "tool_name",
            json!("ReadFile"),
        ),
        "tool_preview",
        json!("notes.txt"),
    );
    apply(
        &mut tree,
        "subagent.tool",
        with(tool, "tool_count", json!(1)),
        5_100,
    );

    start(&mut tree, "child-3", "summarise the findings", 6_000);
    let mut done = with(
        payload("child-3", "summarise the findings"),
        "status",
        json!("completed"),
    );
    done = with(done, "tool_count", json!(4));
    done = with(done, "duration_seconds", json!(20.0));
    done = with(done, "summary", json!("Closed 4 issues"));
    apply(&mut tree, "subagent.complete", done, 6_100);
    tree
}

fn live_tree(rows: u32) -> SubagentTree {
    let mut tree = SubagentTree::new();
    for n in 1..=rows {
        start(
            &mut tree,
            &format!("child-{n}"),
            &format!("goal {n}"),
            u64::from(n) * 1_000,
        );
    }
    tree
}

fn header(spawning: Spawning) -> PanelHeader {
    PanelHeader {
        title: "fix the failing test".to_owned(),
        spawning,
        key_label: None,
    }
}

fn on_with_caps() -> Spawning {
    Spawning::On(Some(Caps {
        max_spawn_depth: 1,
        max_concurrent_children: 10,
    }))
}

#[test]
fn frames_file_matches_contract() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/subagent_frames.jsonl");
    let text = std::fs::read_to_string(&path).expect("the hand-built frames read");
    let mut kinds = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let params: Value = serde_json::from_str(line).expect("line parses");
        let frame = EventFrame::from_params(&params).expect("line decodes");
        let event = SubagentEvent::from_notification(&frame.notification).expect("subagent event");
        kinds.push(event.kind);
        assert_eq!(index + 1, kinds.len());
    }
    use talaria_core::subagents::SubagentEventKind::*;
    for kind in [SpawnRequested, Start, Progress, Thinking, Tool, Complete] {
        assert!(kinds.contains(&kind), "at least one frame of each kind");
    }
}

#[test]
fn dock_rows_overflow_and_short_terminal() {
    let style = SubagentStyle::default();
    let tree = live_tree(3);
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    assert_eq!(
        texts(&dock.lines(80)),
        vec![
            "⎇ Subagents · 3 running · alt+s to open".to_owned(),
            "  ◐ goal 1 · starting".to_owned(),
            "  ◐ goal 2 · starting".to_owned(),
            "  ◐ goal 3 · starting".to_owned(),
        ]
    );

    let tree = live_tree(4);
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(80));
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[4], "  … +1 more");

    let tree = live_tree(6);
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(80));
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[4], "  … +3 more");

    let tree = live_tree(2);
    let short = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        15,
    );
    assert_eq!(
        texts(&short.lines(80)),
        vec!["⎇ Subagents · 2 running · alt+s to open".to_owned()],
        "a terminal under 16 rows shows the header only"
    );
    let tall = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        16,
    );
    assert_eq!(texts(&tall.lines(80)).len(), 3);

    let hidden = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        0,
        48,
    );
    assert!(hidden.lines(80).is_empty(), "dock_rows 0 draws nothing");

    let mut mixed = SubagentTree::new();
    start(&mut mixed, "child-1", "list the files", 1_000);
    start(&mut mixed, "child-2", "read notes", 2_000);
    apply(
        &mut mixed,
        "subagent.spawn_requested",
        payload("child-3", "summarise"),
        3_000,
    );
    let dock = SubagentDock::new(
        &mixed,
        mixed.counts(),
        None,
        Spawning::Paused(None),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    assert_eq!(
        texts(&dock.lines(80))[0],
        "⎇ Subagents · 2 running · 1 queued · spawning paused · alt+s to open"
    );

    let mut queued_only = SubagentTree::new();
    apply(
        &mut queued_only,
        "subagent.spawn_requested",
        payload("child-1", "list the files"),
        1_000,
    );
    let dock = SubagentDock::new(
        &queued_only,
        queued_only.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(80));
    assert_eq!(
        lines[0],
        "⎇ Subagents · 0 running · 1 queued · alt+s to open"
    );
    assert_eq!(lines[1], "  ○ list the files · queued");

    // Narrow: the hint goes first, then the paused part, then the rest is cut.
    // (Hints render lowercase per the rules' own header wording.)
    let full = "⎇ Subagents · 2 running · 1 queued · spawning paused · alt+s to open";
    assert_eq!(full.width(), 68);
    let dock = SubagentDock::new(
        &mixed,
        mixed.counts(),
        None,
        Spawning::Paused(None),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    assert_eq!(texts(&dock.lines(68))[0], full);
    let at60 = texts(&dock.lines(60))[0].clone();
    assert!(!at60.contains("alt+s"), "{at60}");
    assert!(at60.contains("spawning paused"), "{at60}");
    let at40 = texts(&dock.lines(40))[0].clone();
    assert!(!at40.contains("spawning paused"), "{at40}");
    assert!(at40.contains("1 queued"), "{at40}");
    let at20 = texts(&dock.lines(20))[0].clone();
    assert!(at20.width() <= 20, "{at20}");
    assert!(at20.starts_with('⎇'), "{at20}");
}

#[test]
fn dock_shows_then_clears() {
    let style = SubagentStyle::default();
    let mut tree = live_tree(1);
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        24,
    );
    assert!(texts(&dock.lines(80)).len() >= 2);

    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        8,
        probe(true, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .expect("new terminal");
    let view = LiveView {
        sections: vec![&dock as &dyn LiveSection],
    };
    let first = term.draw_live(&view).expect("draw_live");
    assert!(first.drew);
    let bytes = sink.take();
    assert_eq!(first.bytes, bytes.len());
    let mut vt = Vt::new(80, 24);
    vt.feed(&bytes);
    assert!(
        vt.rows().iter().any(|row| row.contains("Subagents")),
        "the header reaches the screen"
    );
    let second = term.draw_live(&view).expect("draw_live");
    assert_eq!(
        second,
        talaria_tui::live::FrameStats {
            drew: false,
            bytes: 0
        },
        "drawing the live view twice with no change draws nothing"
    );

    let done = with(payload("child-1", "goal 1"), "status", json!("completed"));
    apply(&mut tree, "subagent.complete", done, 9_000);
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        24,
    );
    assert!(dock.lines(80).is_empty(), "after complete it draws none");
}

#[test]
fn panel_list_snapshot_160x48() {
    for (seconds, text) in [
        (0.0, "0s"),
        (12.0, "12s"),
        (20.0, "20s"),
        (59.0, "59s"),
        (60.0, "1m 00s"),
        (65.0, "1m 05s"),
        (3599.0, "59m 59s"),
        (3600.0, "1h 00m"),
        (3720.0, "1h 02m"),
    ] {
        assert_eq!(format_elapsed(seconds), text, "{seconds}s");
    }

    let tree = sample_tree();
    let now = clock_ms(13_000);
    let mut panel = AgentsPanel::new(key());
    panel.set_flash(
        "Steer queued for \"list the files in the current directory\". It reaches the subagent at its next step."
            .to_owned(),
    );
    let lines = texts(&panel.render(&tree, &header(on_with_caps()), None, 160, 48, now));
    let left = "Subagents · fix the failing test";
    let right = "2 running · 1 finished · spawning on · depth ≤1 · ≤10 at once";
    assert_eq!(left.width(), 32);
    assert_eq!(right.width(), 61);
    let head = format!(
        "{left}{:width$}{right}",
        "",
        width = 160 - left.width() - right.width()
    );
    assert_eq!(head.width(), 160);
    let goal1 = format!("{:<81}", "list the files in the current directory");
    let goal2 = format!("{:<81}", "read notes.txt");
    let goal3 = format!("{:<81}", "summarise the findings");
    let model = format!("{:<24}", "minimax-m3.1-flash");
    assert_eq!(
        lines,
        vec![
            head,
            format!("▸ ◐ {goal1}  {model}      12s  3 tools   Terminal(\"ls -la\")"),
            format!("  ◐ {goal2}  {model}       8s  1 tool    ReadFile(\"notes.txt\")"),
            format!("  ✓ {goal3}  {model}      20s  4 tools   Closed 4 issues"),
            "Steer queued for \"list the files in the current directory\". It reaches the subagent at its next step.".to_owned(),
            // Hints render lowercase with shift+x as X, per the rules' sample.
            "enter tail · d detail · e steer · x stop · X stop all · p pause spawns · tab session · esc close".to_owned(),
        ]
    );

    let lines = texts(&panel.render(&tree, &header(on_with_caps()), None, 99, 48, now));
    let head = format!(
        "{left}{:width$}{right}",
        "",
        width = 99 - left.width() - right.width()
    );
    assert_eq!(
        lines,
        vec![
            head,
            format!("▸ ◐ list the files in the current directory  {:>7}", "12s"),
            format!("  ◐ read notes.txt  {:>7}", "8s"),
            format!("  ✓ summarise the findings  {:>7}", "20s"),
            "Steer queued for \"list the files in the current directory\". It reaches the subagent at its next step.".to_owned(),
            "enter tail · d detail · e steer · x stop · X stop all · p pause spawns · tab session · esc close".to_owned(),
        ]
    );
}

#[test]
fn panel_detail_tail_and_steer_snapshots() {
    let tree = sample_tree();
    let now = clock_ms(13_000);
    let head = header(on_with_caps());

    let mut panel = AgentsPanel::new(key());
    assert_eq!(panel.handle(Action::AgentsDetail, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 120, 30, now));
    assert_eq!(
        lines[0],
        format!(
            "Subagents · fix the failing test{:width$}2 running · 1 finished · spawning on · depth ≤1 · ≤10 at once",
            "",
            width = 120
                - "Subagents · fix the failing test".width()
                - "2 running · 1 finished · spawning on · depth ≤1 · ≤10 at once".width(),
        )
    );
    // Detail labels use the house `Label: value` style.
    for expected in [
        "Goal: list the files in the current directory",
        "Status: running",
        "Id: child-1",
        "Model: minimax-m3.1-flash",
        "Toolsets: shell",
        "Started: 12s ago",
        "Tools: 3, last Terminal",
        "Tokens: in 100 · out 50 · reasoning 10",
        "API calls: 7",
        "Steering: accepted",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "missing {expected:?} in {lines:?}"
        );
    }
    assert!(
        !lines.iter().any(|line| line.starts_with("Parent:")),
        "absent fields are left out"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("Duration:")),
        "absent fields are left out"
    );
    let joined = lines.join("\n");
    assert!(
        joined.contains("file1.txt"),
        "files read list the first files"
    );
    assert!(
        joined.contains("… +2 more"),
        "twelve files read cut with a count"
    );
    assert!(
        !joined.contains("file12.txt"),
        "the cut files are not shown"
    );
    assert!(
        joined.contains("Found 4 files in the working directory and stopped there"),
        "the summary wraps, it is never cut"
    );
    let first = joined
        .find("Terminal(\"ls -la\")")
        .expect("the tool activity");
    assert!(first < joined.len());

    let mut panel = AgentsPanel::new(key());
    assert_eq!(panel.handle(Action::AgentsTail, &tree), None);
    let tail = TailView {
        text: Some("line two\nline three\n".to_owned()),
        truncated: true,
        ever_failed: false,
    };
    let lines = texts(&panel.render(&tree, &head, Some(&tail), 120, 30, now));
    assert!(lines.iter().any(|line| line
        == "Live transcript · \"list the files in the current directory\" (latest 16 KB; older lines cut)"),
        "{lines:?}");
    assert!(lines.iter().any(|line| line == "line two"), "{lines:?}");
    assert!(lines.iter().any(|line| line == "line three"), "{lines:?}");

    let tail = TailView {
        text: None,
        truncated: false,
        ever_failed: false,
    };
    let lines = texts(&panel.render(&tree, &head, Some(&tail), 120, 30, now));
    assert!(
        lines.iter().any(|line| line == "No live transcript yet."),
        "{lines:?}"
    );
    let tail = TailView {
        text: None,
        truncated: false,
        ever_failed: true,
    };
    let lines = texts(&panel.render(&tree, &head, Some(&tail), 120, 30, now));
    assert!(
        lines
            .iter()
            .any(|line| line == "Live transcript unavailable."),
        "{lines:?}"
    );

    let mut panel = AgentsPanel::new(key());
    assert_eq!(panel.handle(Action::AgentsSteer, &tree), None);
    panel.type_char('g');
    panel.type_char('o');
    let rendered = panel.render(&tree, &head, None, 120, 30, now);
    let flash = rendered
        .iter()
        .find(|line| {
            text(line).starts_with("Steer \"list the files in the current directory\": go")
        })
        .expect("the steer box shows the draft");
    assert!(
        flash.spans.iter().any(|span| span
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::REVERSED)),
        "the caret is reversed"
    );

    // Steer refusals flash on the flash line.
    let mut tree = sample_tree();
    apply(
        &mut tree,
        "subagent.spawn_requested",
        payload("child-q", "queued goal"),
        20_000,
    );
    let mut panel = AgentsPanel::new(key());
    panel.handle(Action::AgentsDown, &tree);
    panel.handle(Action::AgentsDown, &tree);
    panel.handle(Action::AgentsDown, &tree);
    assert_eq!(panel.handle(Action::AgentsSteer, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 120, 30, now));
    assert!(
        lines
            .iter()
            .any(|line| line == "\"queued goal\" has not started yet; stop it once it is running."),
        "a queued child cannot be steered: {lines:?}"
    );
}

#[test]
fn panel_keys_drive_modes() {
    use crossterm::event::{KeyCode as TermCode, KeyEvent, KeyModifiers};
    use talaria_app::subagents::SubagentAction;
    use talaria_tui::keys::{Key, KeyContext};

    let tree = sample_tree();
    let now = clock_ms(13_000);
    let head = header(on_with_caps());
    let mut panel = AgentsPanel::new(key());

    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[1].starts_with("▸ "), "the first row starts selected");
    assert_eq!(panel.handle(Action::AgentsDown, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[2].starts_with("▸ "), "down moves the selection");
    assert_eq!(panel.handle(Action::AgentsUp, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[1].starts_with("▸ "), "up moves it back");
    assert_eq!(panel.handle(Action::AgentsBottom, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[3].starts_with("▸ "), "bottom jumps to the last row");
    assert_eq!(panel.handle(Action::AgentsTop, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[1].starts_with("▸ "), "top jumps to the first row");

    assert_eq!(
        panel.handle(Action::AgentsStop, &tree),
        Some(SubagentAction::Stop("child-1".to_owned()))
    );
    assert_eq!(
        panel.handle(Action::AgentsStopAll, &tree),
        Some(SubagentAction::StopAll)
    );
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(
        lines
            .last()
            .is_some_and(|footer| footer.contains("X again to stop all")),
        "with X armed the hint reads X again: {lines:?}"
    );
    // The second press confirms: ConfirmStopAll is only producible here.
    assert_eq!(
        panel.handle(Action::AgentsStopAll, &tree),
        Some(SubagentAction::ConfirmStopAll)
    );

    assert_eq!(panel.handle(Action::AgentsTail, &tree), None);
    let tail = TailView {
        text: Some("line two\n".to_owned()),
        truncated: false,
        ever_failed: false,
    };
    let lines = texts(&panel.render(&tree, &head, Some(&tail), 160, 48, now));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("Live transcript · ")),
        "{lines:?}"
    );
    assert_eq!(
        panel.handle(Action::AgentsStop, &tree),
        Some(SubagentAction::Stop("child-1".to_owned())),
        "in Tail, x stops the shown row"
    );
    assert_eq!(panel.handle(Action::AgentsBack, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(
        lines[1].starts_with("▸ ◐ "),
        "esc goes back to List: {lines:?}"
    );

    // p switches to the confirmation; y and n answer it.
    assert_eq!(
        panel.handle(Action::AgentsPause, &tree),
        Some(SubagentAction::RequestPause)
    );
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(
        lines.iter().any(|line| line
            == "Pause new subagents for every session on this Hermes gateway? 2 running now will continue. y/n"),
        "the pause confirmation names the running count: {lines:?}"
    );
    // y, n and esc answer through typed characters and return to List. The
    // delivery of ConfirmPause(true) to the hub needs a channel the panel
    // signatures do not name yet; the mode half is pinned here.
    panel.type_char('n');
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(
        lines[1].starts_with("▸ ◐ "),
        "n cancels back to List: {lines:?}"
    );

    // The panel never sees tab: moving across sessions belongs to the run loop,
    // which owns every attached key. With one key tab does nothing anywhere.
    assert_eq!(panel.handle(Action::AgentsNextSession, &tree), None);
    // Close keys return nothing: with no Close control, the run loop owns
    // closing too.
    assert_eq!(panel.handle(Action::AgentsClose, &tree), None);
    assert_eq!(panel.handle(Action::AgentsBack, &tree), None);

    // No other action may hold alt+s.
    let map = KeyMap::defaults();
    let pressed = Key::from(KeyEvent::new(TermCode::Char('s'), KeyModifiers::ALT));
    let holders: Vec<Action> = Action::ALL
        .iter()
        .filter(|action| {
            map.chords(**action)
                .iter()
                .any(|chord| chord.matches(&pressed))
        })
        .copied()
        .collect();
    assert_eq!(holders, vec![Action::AgentsPanel]);
    assert_eq!(
        map.resolve(&pressed, KeyContext::Composer),
        Some(Action::AgentsPanel)
    );
}

const EVIL_GOAL: &str = "evil \u{1b} goal \u{202e} end";
const EVIL_SHOWN: &str = "evil ␛ goal ‹U+202E› end";

#[test]
fn untrusted_text_is_literal() {
    let style = SubagentStyle::default();
    let mut tree = SubagentTree::new();
    start(&mut tree, "child-1", EVIL_GOAL, 1_000);
    let tool = with(
        with(
            payload("child-1", EVIL_GOAL),
            "tool_name",
            json!("Terminal"),
        ),
        "tool_preview",
        json!(EVIL_GOAL),
    );
    apply(&mut tree, "subagent.tool", tool, 1_100);

    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        Some("alt+s"),
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(100));
    assert!(
        lines.iter().any(|line| line.contains(EVIL_SHOWN)),
        "dock: {lines:?}"
    );
    assert!(
        !lines.join("\n").contains('\u{1b}'),
        "no raw escape survives"
    );

    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        160,
        48,
        clock_ms(2_000),
    ));
    assert!(
        lines.iter().any(|line| line.contains(EVIL_SHOWN)),
        "panel: {lines:?}"
    );

    let mut panel = AgentsPanel::new(key());
    panel.handle(Action::AgentsDetail, &tree);
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        120,
        30,
        clock_ms(2_000),
    ));
    assert!(
        lines
            .iter()
            .any(|line| line == &format!("Goal: {EVIL_SHOWN}")),
        "detail: {lines:?}"
    );

    let done = with(payload("child-1", EVIL_GOAL), "status", json!("completed"));
    apply(
        &mut tree,
        "subagent.complete",
        with(done, "summary", json!(EVIL_GOAL)),
        3_000,
    );
    let batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    let lines = texts(&batch_block_lines(&batches[0], 100, &style));
    assert!(
        lines.iter().any(|line| line.contains(EVIL_SHOWN)),
        "block: {lines:?}"
    );
}

fn finished_batch() -> FinishedBatch {
    let mut tree = SubagentTree::new();
    let mut a = payload("child-a", "list the files in the current directory");
    a = with(a, "delegation_id", json!("deleg-1"));
    apply(&mut tree, "subagent.spawn_requested", a.clone(), 1_000);
    apply(&mut tree, "subagent.start", a, 1_001);
    let mut done_a = with(
        payload("child-a", "list the files in the current directory"),
        "status",
        json!("completed"),
    );
    done_a = with(done_a, "delegation_id", json!("deleg-1"));
    done_a = with(done_a, "duration_seconds", json!(6.0));
    done_a = with(done_a, "tool_count", json!(3));
    done_a = with(done_a, "summary", json!("Found 4 files\nplus more"));
    apply(&mut tree, "subagent.complete", done_a, 2_000);

    let mut b = payload("child-b", "read notes.txt");
    b = with(b, "delegation_id", json!("deleg-1"));
    apply(&mut tree, "subagent.spawn_requested", b.clone(), 3_000);
    apply(&mut tree, "subagent.start", b, 3_001);
    let mut done_b = with(
        payload("child-b", "read notes.txt"),
        "status",
        json!("completed"),
    );
    done_b = with(done_b, "delegation_id", json!("deleg-1"));
    done_b = with(done_b, "duration_seconds", json!(5.0));
    done_b = with(done_b, "tool_count", json!(2));
    done_b = with(done_b, "summary", json!("Read it twice"));
    apply(&mut tree, "subagent.complete", done_b, 4_000);

    let mut c = payload("child-c", "summarise the findings");
    c = with(c, "delegation_id", json!("deleg-1"));
    apply(&mut tree, "subagent.spawn_requested", c.clone(), 5_000);
    apply(&mut tree, "subagent.start", c, 5_001);
    let mut done_c = with(
        payload("child-c", "summarise the findings"),
        "status",
        json!("failed"),
    );
    done_c = with(done_c, "delegation_id", json!("deleg-1"));
    done_c = with(done_c, "duration_seconds", json!(8.0));
    done_c = with(done_c, "tool_count", json!(1));
    done_c = with(done_c, "summary", json!("notes.txt not found"));
    apply(&mut tree, "subagent.complete", done_c, 6_000);

    let mut batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    batches.pop().expect("one batch")
}

#[test]
fn batch_block_exact_lines() {
    let style = SubagentStyle::default();
    let batch = finished_batch();
    assert_eq!(
        texts(&batch_block_lines(&batch, 100, &style)),
        vec![
            "⎇ Subagents · 2 finished · 1 failed · 8s".to_owned(),
            "  ✓ list the files in the current directory · 6s · 3 tools · Found 4 files".to_owned(),
            "  ✓ read notes.txt · 5s · 2 tools · Read it twice".to_owned(),
            "  ✗ summarise the findings · 8s · 1 tool · failed: notes.txt not found".to_owned(),
        ]
    );

    let mut tree = SubagentTree::new();
    for n in 0..25 {
        let id = format!("child-{n:02}");
        let mut body = payload(&id, &format!("goal {n:02}"));
        body = with(body, "delegation_id", json!("deleg-25"));
        apply(
            &mut tree,
            "subagent.spawn_requested",
            body.clone(),
            1_000 + n * 10,
        );
        apply(&mut tree, "subagent.start", body, 1_001 + n * 10);
        let mut done = with(
            payload(&id, &format!("goal {n:02}")),
            "status",
            json!("completed"),
        );
        done = with(done, "delegation_id", json!("deleg-25"));
        apply(&mut tree, "subagent.complete", done, 2_000 + n * 10);
    }
    let batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    let lines = texts(&batch_block_lines(&batches[0], 100, &style));
    assert_eq!(lines.len(), 22, "the header, 20 rows, the more line");
    assert_eq!(lines[21], "  … +5 more");

    // Nested batches indent two spaces per level past the root.
    let mut tree = SubagentTree::new();
    let mut root = payload("root", "root goal");
    root = with(root, "delegation_id", json!("deleg-nest"));
    apply(&mut tree, "subagent.spawn_requested", root.clone(), 1_000);
    apply(&mut tree, "subagent.start", root, 1_001);
    let mut child = with(payload("child", "child goal"), "parent_id", json!("root"));
    child = with(child, "delegation_id", json!("deleg-nest"));
    apply(&mut tree, "subagent.spawn_requested", child.clone(), 2_000);
    apply(&mut tree, "subagent.start", child, 2_001);
    let mut done_child = with(payload("child", "child goal"), "status", json!("completed"));
    done_child = with(done_child, "delegation_id", json!("deleg-nest"));
    apply(&mut tree, "subagent.complete", done_child, 3_000);
    let mut done_root = with(payload("root", "root goal"), "status", json!("completed"));
    done_root = with(done_root, "delegation_id", json!("deleg-nest"));
    apply(&mut tree, "subagent.complete", done_root, 4_000);
    let batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    let lines = texts(&batch_block_lines(&batches[0], 100, &style));
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("  ✓ root goal"), "{}", lines[1]);
    assert!(lines[2].starts_with("    ✓ child goal"), "{}", lines[2]);
}

mod widths {
    use proptest::prelude::*;
    use unicode_width::UnicodeWidthStr;

    use super::{clock_ms, header, live_tree, on_with_caps, sample_tree, texts};
    use talaria_tui::live::LiveSection;
    use talaria_tui::subagents::{AgentsPanel, SubagentDock, SubagentStyle, batch_block_lines};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn no_row_wider_than_terminal(width in 0..200u16, height in 1..60u16) {
            let style = SubagentStyle::default();
            let tree = sample_tree();
            let live = live_tree(4);
            let key = super::key();

            let dock = SubagentDock::new(
                &live,
                live.counts(),
                None,
                on_with_caps(),
                Some("alt+s"),
                &style,
                3,
                height,
            );
            for line in dock.lines(width) {
                assert!(text_width(&line) <= width, "dock overflows at {width}");
            }
            if width >= 20 && height >= 3 {
                assert!(!dock.lines(width).is_empty(), "the dock draws at {width}x{height}");
            }

            let panel = AgentsPanel::new(key);
            let rendered = panel.render(&tree, &header(on_with_caps()), None, width, height, clock_ms(13_000));
            for line in &rendered {
                assert!(text_width(line) <= width, "panel overflows at {width}");
            }
            if width >= 20 && height >= 3 {
                assert!(!rendered.is_empty(), "the panel draws at {width}x{height}");
            }

            let batch = super::finished_batch();
            let block = batch_block_lines(&batch, width, &style);
            for line in &block {
                assert!(text_width(line) <= width, "block overflows at {width}");
            }
            if width >= 20 {
                assert!(!block.is_empty(), "the block draws at {width}");
            }
            let _ = dock.height(width);
            let _ = texts(&block);
        }
    }

    fn text_width(line: &ratatui::text::Line<'static>) -> u16 {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        u16::try_from(text.width()).unwrap_or(u16::MAX)
    }
}
