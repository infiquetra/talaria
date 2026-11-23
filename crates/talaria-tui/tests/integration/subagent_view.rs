//! Red-run tests 38 to 46 (#242): the dock, the panel and the block.
//!
//! Expected snapshots are literal strings written from the screen rules
//! before any code; no snapshot file or approval tool is used. Red: the stub
//! renderers return no lines, and every test asserts at least one drawn line
//! or a parsed frame. (The trees below build through the core stubs, so until
//! the tree lands these fail in the core; once it does, they fail in the
//! renderers.)

use crate::support;

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
    apply(&mut tree, "subagent.progress", rich, 1_200);
    // Steering acceptance is a list field, not an event payload field.
    tree.reconcile(
        &serde_json::from_value(json!({"subagents": [{
            "subagent_id": "child-1", "status": "running", "accepting_steer": true
        }], "delegations": []}))
        .expect("contract-shaped snapshot"),
        clock_ms(1_200),
    );

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
    let setup_bytes = sink.take();
    let view = LiveView {
        sections: vec![&dock as &dyn LiveSection],
    };
    let first = term.draw_live(&view).expect("draw_live");
    assert!(first.drew);
    let bytes = sink.take();
    assert_eq!(first.bytes, bytes.len());
    let mut vt = Vt::new(80, 24);
    vt.feed(&setup_bytes);
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
    let right = "2 running · 1 finished";
    let head = |width: usize| {
        format!(
            "{left}{:fill$}{right}",
            "",
            fill = width - left.width() - right.width()
        )
    };
    let status = "Spawning on · up to 10 at once · max nesting 1 · model minimax-m3.1-flash";
    let goal1 = format!("{:<39}", "list the files in the current directory");
    let goal2 = format!("{:<39}", "read notes.txt");
    let goal3 = format!("{:<39}", "summarise the findings");
    let selected = format!(
        "{:<160}",
        format!(
            "▸ ◐ {goal1}  {:>4}  {:>5}  Terminal(\"ls -la\")",
            "12s", "3"
        )
    );
    assert_eq!(
        lines,
        vec![
            head(160),
            status.to_owned(),
            format!("    {:<39}  {:>4}  {:>5}  Activity", "Task", "Time", "Tools"),
            selected,
            format!("  ◐ {goal2}  {:>4}  {:>5}  ReadFile(\"notes.txt\")", "8s", "1"),
            format!("  ✓ {goal3}  {:>4}  {:>5}  Closed 4 issues", "20s", "4"),
            "Steer queued for \"list the files in the current directory\". It reaches the subagent at its next step.".to_owned(),
            // Hints are grouped: view, control, then navigation.
            "enter tail · d detail · o new pane   e steer · x stop · X stop all · p pause spawns   tab session · esc close".to_owned(),
        ]
    );

    // Narrow: no model, tool or activity column; time stays right aligned.
    let lines = texts(&panel.render(&tree, &header(on_with_caps()), None, 99, 48, now));
    let selected = format!("{:<99}", format!("▸ ◐ {goal1}  {:>4}", "12s"));
    assert_eq!(
        lines,
        vec![
            head(99),
            status.to_owned(),
            format!("    {:<39}  {:>4}", "Task", "Time"),
            selected,
            format!("  ◐ {goal2}  {:>4}", "8s"),
            format!("  ✓ {goal3}  {:>4}", "20s"),
            "Steer queued for \"list the files in the current directory\". It reaches the subagent at its next st…".to_owned(),
            // Too wide for one line, so the groups run together before the cut.
            "enter tail · d detail · o new pane · e steer · x stop · X stop all · p pause spawns · tab session …".to_owned(),
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
            "Subagents · fix the failing test{:width$}2 running · 1 finished",
            "",
            width =
                120 - "Subagents · fix the failing test".width() - "2 running · 1 finished".width(),
        )
    );
    assert_eq!(
        lines[1],
        "Spawning on · up to 10 at once · max nesting 1 · model minimax-m3.1-flash"
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
    assert!(lines[3].starts_with("▸ "), "the first row starts selected");
    assert_eq!(panel.handle(Action::AgentsDown, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[4].starts_with("▸ "), "down moves the selection");
    assert_eq!(panel.handle(Action::AgentsUp, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[3].starts_with("▸ "), "up moves it back");
    assert_eq!(panel.handle(Action::AgentsBottom, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[5].starts_with("▸ "), "bottom jumps to the last row");
    assert_eq!(panel.handle(Action::AgentsTop, &tree), None);
    let lines = texts(&panel.render(&tree, &head, None, 160, 48, now));
    assert!(lines[3].starts_with("▸ "), "top jumps to the first row");

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
        lines[3].starts_with("▸ ◐ "),
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
        lines[3].starts_with("▸ ◐ "),
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
            "⎇ Subagents · 3 finished · 1 failed · 8s".to_owned(),
            "  ✓ list the files in the current directory · 6s · 3 tools · Found 4 files".to_owned(),
            "  ✓ read notes.txt · 5s · 2 tools · Read it twice".to_owned(),
            "  ✗ summarise the findings · 8s · 1 tool · failed: notes.txt not found".to_owned(),
        ]
    );

    let mut mixed = batch.clone();
    mixed.rows[0].0.state = talaria_core::subagents::RowState::Interrupted;
    mixed.rows[1].0.state = talaria_core::subagents::RowState::Gone;
    assert_eq!(
        texts(&batch_block_lines(&mixed, 100, &style))[0],
        "⎇ Subagents · 3 finished · 2 failed · 1 stopped · 8s",
        "finished counts every terminal child; failed and stopped are subsets"
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

const NAMES: [&str; 10] = [
    "riemann",
    "p-vs-np",
    "navier-stokes",
    "collatz",
    "goldbach",
    "abc",
    "bsd",
    "erdos-straus",
    "twin-primes",
    "perfect",
];

fn shared_goal(name: &str) -> String {
    format!("Run and test the `{name}` demonstration of the calc CLI, then report the result.")
}

const SHORT_GOAL: &str = "Run the `riemann` demonstration and report back.";

const SHARED_CAPTION: &str =
    "Task: Run and test the … demonstration of the calc CLI, then report the result.";

/// Ten live children whose goals differ in one word, like a fan-out run.
fn shared_goal_tree() -> SubagentTree {
    let mut tree = SubagentTree::new();
    for (n, name) in NAMES.iter().enumerate() {
        start(
            &mut tree,
            &format!("child-{n}"),
            &shared_goal(name),
            1_000 + n as u64 * 10,
        );
    }
    tree
}

#[test]
fn panel_list_shows_the_differing_part_of_shared_goals() {
    let tree = shared_goal_tree();
    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(17_000),
    ));
    assert_eq!(
        lines.iter().filter(|line| line.contains("Task: ")).count(),
        1,
        "the shared words appear once: {lines:?}"
    );
    assert!(lines.contains(&SHARED_CAPTION.to_owned()), "{lines:?}");
    for name in NAMES {
        let rows: Vec<_> = lines
            .iter()
            .filter(|line| line.contains(&format!("`{name}`")))
            .collect();
        assert_eq!(rows.len(), 1, "one row for {name}: {lines:?}");
        assert!(!rows[0].contains("Run and test"), "{}", rows[0]);
        assert!(!rows[0].contains("…"), "no cut hides the name: {}", rows[0]);
    }
}

#[test]
fn panel_goals_that_share_nothing_keep_their_full_text() {
    // A single row has no shared text to drop.
    let mut tree = SubagentTree::new();
    start(&mut tree, "only", SHORT_GOAL, 1_000);
    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(2_000),
    ));
    assert!(
        lines.iter().any(|line| line.contains(SHORT_GOAL)),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("Task: ")),
        "{lines:?}"
    );

    // Rows with identical goals stay whole too.
    let mut tree = SubagentTree::new();
    for n in 0..3 {
        start(&mut tree, &format!("c{n}"), SHORT_GOAL, 1_000 + n);
    }
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(2_000),
    ));
    let whole = lines
        .iter()
        .filter(|line| line.contains(SHORT_GOAL))
        .count();
    assert_eq!(whole, 3, "{lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("Task: ")),
        "{lines:?}"
    );
}

#[test]
fn panel_nested_rows_keep_indent_and_columns_aligned() {
    let mut tree = SubagentTree::new();
    start(
        &mut tree,
        "root",
        "Plan the demonstration run for all of the demo tools",
        1_000,
    );
    for (n, name) in ["alpha", "beta"].into_iter().enumerate() {
        let body = with(
            payload(&format!("kid-{n}"), &format!("Check `{name}` sample")),
            "parent_id",
            json!("root"),
        );
        apply(
            &mut tree,
            "subagent.spawn_requested",
            body.clone(),
            2_000 + n as u64,
        );
        apply(&mut tree, "subagent.start", body, 2_100 + n as u64);
    }
    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(9_000),
    ));
    let row = |needle: &str| {
        lines
            .iter()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle}: {lines:?}"))
            .clone()
    };
    let root = row("Plan the demonstration");
    let alpha = row("`alpha`");
    let beta = row("`beta`");
    assert!(root.starts_with("▸ ◐ Plan"), "{root}");
    assert!(alpha.starts_with("    ◐ Check `alpha` sample"), "{alpha}");
    assert!(beta.starts_with("    ◐ Check `beta` sample"), "{beta}");
    // The activity column starts at the same place on every row.
    let column = |line: &str| line.find("starting").map(|at| line[..at].width());
    assert!(column(&root).is_some());
    assert_eq!(column(&root), column(&alpha), "{root} / {alpha}");
    assert_eq!(column(&alpha), column(&beta));
    assert!(
        lines.iter().all(|line| !line.contains("Task: ")),
        "the root goal shares no text with its children, so no caption: {lines:?}"
    );
}

#[test]
fn panel_activity_skips_batched_progress() {
    let mut tree = SubagentTree::new();
    start(&mut tree, "a", "list the files", 1_000);
    let tool = with(
        with(
            payload("a", "list the files"),
            "tool_name",
            json!("terminal"),
        ),
        "tool_preview",
        json!("./calc run riemann"),
    );
    apply(&mut tree, "subagent.tool", tool, 1_100);
    let batch = with(
        payload("a", "list the files"),
        "text",
        json!("🔀 [set 1 · 3/10] terminal, read_file"),
    );
    apply(&mut tree, "subagent.progress", batch, 1_200);
    // A second child has only the batched line so far.
    start(&mut tree, "b", "read the notes", 2_000);
    let batch = with(
        payload("b", "read the notes"),
        "text",
        json!("🔀 [set 1 · 4/10] terminal, read_file"),
    );
    apply(&mut tree, "subagent.progress", batch, 2_100);

    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(5_000),
    ));
    let first = lines
        .iter()
        .find(|l| l.contains("list the files"))
        .expect("row a");
    assert!(
        first
            .trim_end()
            .ends_with("terminal(\"./calc run riemann\")"),
        "{first}"
    );
    let second = lines
        .iter()
        .find(|l| l.contains("read the notes"))
        .expect("row b");
    assert!(
        second.trim_end().ends_with("terminal, read_file"),
        "{second}"
    );
    for line in [first, second] {
        assert!(!line.contains("[set"), "{line}");
        assert!(!line.contains('🔀'), "{line}");
    }

    let mut panel = AgentsPanel::new(key());
    panel.handle(Action::AgentsDetail, &tree);
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        120,
        40,
        clock_ms(5_000),
    ));
    assert!(
        lines.iter().any(|l| l == "terminal, read_file"),
        "{lines:?}"
    );
    assert!(lines.iter().all(|l| !l.contains("[set")), "{lines:?}");

    // The dock shows the same readable line.
    let style = SubagentStyle::default();
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        None,
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(100));
    assert!(
        lines
            .iter()
            .all(|l| !l.contains("[set") && !l.contains('🔀')),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("terminal(\"./calc run riemann\")")),
        "{lines:?}"
    );
}

#[test]
fn panel_selected_row_keeps_glyph_colour() {
    use ratatui::style::Modifier;
    let tree = sample_tree();
    let style = SubagentStyle::default();
    let panel = AgentsPanel::new(key());
    let lines = panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        160,
        48,
        clock_ms(13_000),
    );
    let selected = lines
        .iter()
        .find(|line| text(line).starts_with("▸ "))
        .expect("selected row");
    let glyph = selected
        .spans
        .iter()
        .position(|span| span.content == "◐")
        .expect("the glyph has a span of its own");
    assert_eq!(selected.spans[glyph].style.fg, style.running.fg);
    assert!(
        !selected.spans[glyph]
            .style
            .add_modifier
            .contains(Modifier::REVERSED)
    );
    for span in &selected.spans[..glyph] {
        assert!(
            !span.style.add_modifier.contains(Modifier::REVERSED),
            "{span:?}"
        );
    }
    assert!(glyph + 1 < selected.spans.len());
    for span in &selected.spans[glyph + 1..] {
        assert!(
            span.style.add_modifier.contains(Modifier::REVERSED),
            "{span:?}"
        );
    }
    assert_eq!(selected.width(), 160, "the bar runs to the edge");
    for line in lines.iter().filter(|l| text(l).starts_with("  ◐ ")) {
        assert!(
            line.spans
                .iter()
                .all(|s| !s.style.add_modifier.contains(Modifier::REVERSED)),
            "only the selected row is a bar"
        );
    }
}

#[test]
fn panel_hides_model_column_when_shared() {
    let mut tree = SubagentTree::new();
    start(&mut tree, "a", "list the files", 1_000);
    start(&mut tree, "b", "read the notes", 1_100);
    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(5_000),
    ));
    assert!(lines.iter().all(|l| !l.contains("Model")), "{lines:?}");
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("minimax-m3.1-flash"))
            .count(),
        1,
        "the shared model is named once, in the header: {lines:?}"
    );
    assert!(
        lines[1].ends_with("model minimax-m3.1-flash"),
        "{}",
        lines[1]
    );

    // Once the rows differ the column is back and the header stops naming one.
    let other = with(
        payload("c", "scan the logs"),
        "model",
        json!("other-model-2"),
    );
    apply(&mut tree, "subagent.spawn_requested", other.clone(), 2_000);
    apply(&mut tree, "subagent.start", other, 2_001);
    let lines = texts(&panel.render(
        &tree,
        &header(on_with_caps()),
        None,
        148,
        48,
        clock_ms(5_000),
    ));
    assert!(!lines[1].contains("model "), "{}", lines[1]);
    assert!(lines[2].contains("Model"), "{}", lines[2]);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("scan the logs") && l.contains("other-model-2"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("list the files") && l.contains("minimax-m3.1-flash"))
    );
}

#[test]
fn panel_header_counts_only_what_is_nonzero() {
    let tree = sample_tree();
    let panel = AgentsPanel::new(key());
    let lines = texts(&panel.render(
        &tree,
        &header(Spawning::Unknown),
        None,
        120,
        48,
        clock_ms(5_000),
    ));
    assert!(lines[0].ends_with("2 running · 1 finished"), "{}", lines[0]);
    assert_eq!(
        lines[1],
        "Spawning state unknown · model minimax-m3.1-flash"
    );
    let lines = texts(&panel.render(
        &tree,
        &header(Spawning::Paused(None)),
        None,
        120,
        48,
        clock_ms(5_000),
    ));
    assert!(
        lines[1].starts_with("Spawning paused for all sessions"),
        "{}",
        lines[1]
    );
}

fn batch_of(rows: &[(&str, &str, &str)]) -> FinishedBatch {
    let mut tree = SubagentTree::new();
    for (n, (goal, status, summary)) in rows.iter().enumerate() {
        let id = format!("child-{n}");
        let mut body = with(payload(&id, goal), "delegation_id", json!("deleg-x"));
        apply(
            &mut tree,
            "subagent.spawn_requested",
            body.clone(),
            1_000 + n as u64 * 10,
        );
        apply(
            &mut tree,
            "subagent.start",
            body.clone(),
            1_001 + n as u64 * 10,
        );
        body = with(body, "status", json!(status));
        body = with(body, "duration_seconds", json!(6.0));
        body = with(body, "tool_count", json!(3));
        body = with(body, "summary", json!(summary));
        apply(&mut tree, "subagent.complete", body, 2_000 + n as u64 * 10);
    }
    let mut batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    batches.pop().expect("one batch")
}

#[test]
fn batch_block_shows_verdict_not_raw_json() {
    let style = SubagentStyle::default();
    let batch = batch_of(&[
        (
            "list the files",
            "completed",
            "{\n  \"item\": \"alpha\",\n  \"verdict\": \"pass\",\n  \"evidence\": [\"cut mid",
        ),
        (
            "read the notes",
            "completed",
            "{\n  \"problem_id\": \"p\",\n  \"notes\": \"cut",
        ),
        (
            "scan the logs",
            "failed",
            "{\"verdict\": \"fail\", \"why\": \"cut",
        ),
        (
            "write a poem",
            "completed",
            "Wrote the poem. Then checked the rhyme.\nMore",
        ),
    ]);
    let lines = texts(&batch_block_lines(&batch, 100, &style));
    assert_eq!(
        lines[1],
        "  ✓ list the files · 6s · 3 tools · verdict: pass"
    );
    assert_eq!(lines[2], "  ✓ read the notes · 6s · 3 tools · JSON result");
    assert_eq!(
        lines[3],
        "  ✗ scan the logs · 6s · 3 tools · failed: verdict: fail"
    );
    assert_eq!(
        lines[4],
        "  ✓ write a poem · 6s · 3 tools · Wrote the poem."
    );
    assert!(lines.iter().all(|l| !l.contains('{')), "{lines:?}");
}

#[test]
fn batch_block_drops_shared_goal_words() {
    let style = SubagentStyle::default();
    let goals: Vec<String> = ["alpha", "beta", "gamma"]
        .iter()
        .map(|n| shared_goal(n))
        .collect();
    let batch = batch_of(&[
        (&goals[0], "completed", "Passed"),
        (&goals[1], "completed", "Passed"),
        (&goals[2], "failed", "Broke"),
    ]);
    let lines = texts(&batch_block_lines(&batch, 100, &style));
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert_eq!(lines[0], "⎇ Subagents · 3 finished · 1 failed · 6s");
    assert_eq!(lines[1], format!("  {SHARED_CAPTION}"));
    assert_eq!(lines[2], "  ✓ `alpha` · 6s · 3 tools · Passed");
    assert_eq!(lines[3], "  ✓ `beta` · 6s · 3 tools · Passed");
    assert_eq!(lines[4], "  ✗ `gamma` · 6s · 3 tools · failed: Broke");
}

#[test]
fn dock_shows_short_labels_and_one_caption() {
    let style = SubagentStyle::default();
    let tree = shared_goal_tree();
    let dock = SubagentDock::new(
        &tree,
        tree.counts(),
        None,
        on_with_caps(),
        None,
        &style,
        3,
        48,
    );
    let lines = texts(&dock.lines(100));
    assert_eq!(lines[0], "⎇ Subagents · 10 running");
    assert_eq!(lines[1], format!("  {SHARED_CAPTION}"));
    assert_eq!(lines[2], "  ◐ `riemann` · starting");
    assert_eq!(lines[3], "  ◐ `p-vs-np` · starting");
    assert_eq!(lines[4], "  ◐ `navier-stokes` · starting");
    assert_eq!(lines[5], "  … +7 more");
}

/// Goals with wide characters and a batched progress line.
fn wide_progress_tree() -> SubagentTree {
    let mut tree = SubagentTree::new();
    for (n, name) in ["甲乙", "👨\u{200d}👩\u{200d}👧", "gamma"]
        .into_iter()
        .enumerate()
    {
        let goal = format!("检查 the `{name}` sample of the demo tool, then 汇报 back");
        let id = format!("w{n}");
        start(&mut tree, &id, &goal, 1_000 + n as u64);
        let batch = with(
            payload(&id, &goal),
            "text",
            json!("🔀 [set 1 · 3/10] terminal, read_file"),
        );
        apply(&mut tree, "subagent.progress", batch, 1_500 + n as u64);
    }
    tree
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
            let shared = super::shared_goal_tree();
            let wide = super::wide_progress_tree();
            for tree in [&shared, &wide] {
                let dock = SubagentDock::new(
                    tree,
                    tree.counts(),
                    None,
                    on_with_caps(),
                    Some("alt+s"),
                    &style,
                    3,
                    height,
                );
                for line in dock.lines(width) {
                    assert!(text_width(&line) <= width, "shared dock overflows at {width}");
                }
                let panel = AgentsPanel::new(super::key());
                for line in panel.render(tree, &header(on_with_caps()), None, width, height, clock_ms(13_000)) {
                    assert!(text_width(&line) <= width, "shared panel overflows at {width}");
                }
            }
            let batch = super::batch_of(&[
                ("Check 👨‍👩‍👧 sample 一 of the demo tool, then report back", "completed", "{\"verdict\": \"pass\"}"),
                ("Check 👨‍👦 sample 二 of the demo tool, then report back", "failed", "{\"x\": 1"),
            ]);
            for line in batch_block_lines(&batch, width, &style) {
                assert!(text_width(&line) <= width, "shared block overflows at {width}");
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
