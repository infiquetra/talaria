//! Work panel rows at 32 content columns.

use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::automation::{GoalView, WaitView};
use talaria_core::checklist::Checklist;
use talaria_core::subagents::{Clock, SubagentEvent, SubagentTree};
use talaria_tui::status::{Segment, StatusInputs, TurnState, plan};
use talaria_tui::workspace::panel::{WaitingRequest, WorkContents, content_lines};
use unicode_width::UnicodeWidthStr;

fn goal(title: &str, status: &str, verdict: Option<&str>) -> GoalView {
    GoalView {
        title: title.to_owned(),
        status: status.to_owned(),
        created_ms: Some(1),
        last_verdict: verdict.map(str::to_owned),
        last_reason: None,
        turns_used: 1,
        max_turns: 20,
        subgoals: 0,
        gates: 0,
        paused_reason: None,
        wait: None,
    }
}

fn checklist(items: &[(&str, &str)]) -> Checklist {
    let values = items
        .iter()
        .enumerate()
        .map(|(index, (content, status))| {
            json!({
                "id": (index + 1).to_string(),
                "content": content,
                "status": status,
            })
        })
        .collect::<Vec<_>>();
    Checklist::from_values(1, &values)
}

fn apply(tree: &mut SubagentTree, kind: &str, payload: Value, ms: u64) {
    let frame = EventFrame::from_params(&json!({
        "type": kind,
        "session_id": "local-test",
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": payload,
    }))
    .expect("contract-shaped child");
    let event = SubagentEvent::from_notification(&frame.notification).expect("child event");
    tree.apply(
        &event,
        Clock {
            mono_ms: ms,
            wall_s: ms as f64 / 1000.0,
        },
    );
}

fn child(id: &str, goal: &str, model: Option<&str>) -> Value {
    let mut payload = json!({
        "subagent_id": id,
        "goal": goal,
        "task_index": 0,
        "task_count": 1,
    });
    if let Some(model) = model {
        payload["model"] = json!(model);
    }
    payload
}

fn work<'a>(
    goal: Option<&'a GoalView>,
    tasks: Option<&'a Checklist>,
    waiting: &'a [WaitingRequest],
    agents: &'a SubagentTree,
) -> WorkContents<'a> {
    WorkContents {
        needs_you: 0,
        goal,
        turn: TurnState::Streaming,
        verdict_previous_turn: false,
        checklist: tasks,
        checklist_expanded: true,
        waiting,
        agents,
        parent_model: None,
        recent_limit: 4,
        voice_phase: None,
        read_aloud: None,
        voice_target: None,
    }
}

fn paint(work: &WorkContents<'_>, height: u16) -> Vec<String> {
    let lines = content_lines(work, 32, height);
    assert!(
        u16::try_from(lines.len()).unwrap_or(u16::MAX) <= height,
        "rows exceed the panel height: {}",
        lines.len()
    );
    for line in &lines {
        assert!(
            UnicodeWidthStr::width(line.as_str()) <= 32,
            "wider than 32 columns: {line}"
        );
    }
    lines
}

fn position(lines: &[String], needle: &str) -> usize {
    lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle} missing from {lines:?}"))
}

fn status_needs_you(count: u32) -> Option<String> {
    let inputs = StatusInputs {
        needs_you: count,
        ..StatusInputs::default()
    };
    plan(&inputs, None, 120)
        .pieces
        .into_iter()
        .find(|(segment, _, _)| *segment == Segment::NeedsYou)
        .map(|(_, text, _)| text)
}

#[test]
fn sections_appear_in_the_approved_order() {
    let goal = goal("ship the notes", "active", None);
    let tasks = checklist(&[("file the checklist", "pending")]);
    let waiting = [WaitingRequest {
        seq: 1,
        label: "review the diff".to_owned(),
        foreground: false,
    }];
    let mut agents = SubagentTree::new();
    apply(
        &mut agents,
        "subagent.start",
        child("layout", "layout child", None),
        1_000,
    );
    let mut shown = work(Some(&goal), Some(&tasks), &waiting, &agents);
    shown.needs_you = 1;
    let lines = paint(&shown, 24);
    let joined = lines.join("\n");
    let attention = position(&lines, "1 needs you");
    let goal_at = position(&lines, "Goal:");
    let turn_at = position(&lines, "Turn:");
    let verdict_at = position(&lines, "Verdict:");
    let tasks_at = position(&lines, "Tasks");
    let agents_at = position(&lines, "Agents");
    assert!(attention < goal_at && goal_at < turn_at && turn_at < verdict_at);
    assert!(verdict_at < tasks_at && tasks_at < agents_at);
    for banned in [
        "listening",
        "read-aloud",
        "heartbeat",
        "YOLO",
        "Talaria   Session",
    ] {
        assert!(!joined.contains(banned), "{banned} is not a panel section");
    }
}

#[test]
fn attention_matches_the_status_row() {
    let agents = SubagentTree::new();
    let waiting = (0..4)
        .map(|seq| WaitingRequest {
            seq,
            label: format!("need {seq}"),
            foreground: false,
        })
        .collect::<Vec<_>>();
    let mut shown = work(None, None, &waiting, &agents);
    shown.needs_you = 1;
    let one = paint(&shown, 8);
    let status = status_needs_you(1).expect("the status row counts one");
    let phrase = status.trim_start_matches('⚑').trim();
    assert!(
        one.iter().any(|line| line.contains(phrase)),
        "panel {one:?} must repeat the status row's {phrase}"
    );
    assert!(
        !one.iter().any(|line| line.contains("4 need")),
        "the panel must not recount the fleet queue: {one:?}"
    );

    shown.needs_you = 2;
    let two = paint(&shown, 8);
    let status = status_needs_you(2).expect("the status row counts two");
    let phrase = status.trim_start_matches('⚑').trim();
    assert!(
        two.iter().any(|line| line.contains(phrase)),
        "panel {two:?} must repeat {phrase}"
    );

    shown.needs_you = 0;
    let quiet = paint(&shown, 8);
    let joined = quiet.join("\n");
    assert!(
        status_needs_you(0).is_none(),
        "zero draws no status segment"
    );
    assert!(
        !joined.contains("need you") && !joined.contains('⚑'),
        "zero draws no attention row: {quiet:?}"
    );
}

#[test]
fn verdict_is_explicit_or_pending_or_unknown() {
    let agents = SubagentTree::new();
    let waiting: &[WaitingRequest] = &[];

    let active = goal("ship the notes", "active", None);
    let lines = paint(&work(Some(&active), None, waiting, &agents), 16);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Goal:") && line.contains("ship the notes")),
        "active goal: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("Turn: streaming")),
        "turn uses the status word: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("Verdict: pending")),
        "a running goal with no verdict is pending: {lines:?}"
    );

    let mut parked = goal("wait for review", "active", None);
    parked.wait = Some(WaitView::Until {
        until_ms: 9,
        reason: "coffee".to_owned(),
    });
    let lines = paint(&work(Some(&parked), None, waiting, &agents), 16);
    let joined = lines.join("\n");
    assert!(
        joined.contains("parked")
            && joined.contains("wait for review")
            && joined.contains("coffee"),
        "parked goal: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("Verdict: pending")),
        "a parked goal with no verdict is pending: {lines:?}"
    );

    let mut paused = goal("hold the notes", "paused", None);
    paused.paused_reason = Some("waiting for input".to_owned());
    let lines = paint(&work(Some(&paused), None, waiting, &agents), 16);
    let joined = lines.join("\n");
    assert!(
        joined.contains("paused") && joined.contains("waiting for input"),
        "paused goal: {lines:?}"
    );

    let finished = goal("ship the notes", "done", Some("needs-revision"));
    let lines = paint(&work(Some(&finished), None, waiting, &agents), 16);
    let verdict = lines
        .iter()
        .find(|line| line.contains("Verdict:"))
        .expect("a verdict line");
    assert!(
        verdict.contains("needs-revision"),
        "an explicit verdict is shown as reported: {verdict}"
    );
    assert!(
        !verdict.contains("success")
            && !verdict.contains("failure")
            && !verdict.contains("previous"),
        "the verdict is not mapped or relabelled: {verdict}"
    );

    let mut previous = work(Some(&finished), None, waiting, &agents);
    previous.verdict_previous_turn = true;
    previous.turn = TurnState::Streaming;
    let lines = paint(&previous, 16);
    let verdict = lines
        .iter()
        .find(|line| line.contains("Verdict:"))
        .expect("a verdict line");
    assert!(
        verdict.contains("needs-revision")
            && lines.iter().any(|line| line.contains("previous turn")),
        "a verdict from the earlier turn is labelled: {lines:?}"
    );

    let none = goal("ship the notes", "done", None);
    let lines = paint(&work(Some(&none), None, waiting, &agents), 16);
    let verdict = lines
        .iter()
        .find(|line| line.contains("Verdict:"))
        .expect("a verdict line");
    assert!(
        verdict.contains("unknown") || verdict.contains("pending"),
        "a finished goal with no verdict is pending or unknown: {verdict}"
    );
    assert!(
        !verdict.contains("success") && !verdict.contains("failure"),
        "a missing verdict is not success: {verdict}"
    );

    let lines = paint(&work(None, None, waiting, &agents), 12);
    assert!(
        lines.iter().all(|line| !line.contains("Goal:")),
        "no goal draws no goal row: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("Verdict: unknown")),
        "nothing known is unknown: {lines:?}"
    );
}

#[test]
fn finished_checklist_is_not_success() {
    let agents = SubagentTree::new();
    let tasks = checklist(&[
        ("first step", "completed"),
        ("second step", "completed"),
        ("third step", "completed"),
    ]);
    let finished = goal("ship the notes", "done", None);
    let lines = paint(&work(Some(&finished), Some(&tasks), &[], &agents), 16);
    assert!(
        lines.iter().any(|line| line.contains("first step")),
        "the completed checklist is still the task list: {lines:?}"
    );
    let verdict = lines
        .iter()
        .find(|line| line.contains("Verdict:"))
        .expect("a verdict line");
    assert!(
        verdict.contains("unknown") || verdict.contains("pending"),
        "a completed checklist does not invent a verdict: {verdict}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("success") || line.contains("failure")),
        "a completed checklist is not success: {lines:?}"
    );
}

#[test]
fn agents_are_ordered_waiting_running_queued_recent() {
    let mut agents = SubagentTree::new();
    apply(
        &mut agents,
        "subagent.start",
        child("run", "running-goal", None),
        1_000,
    );
    apply(
        &mut agents,
        "subagent.spawn_requested",
        child("wait", "queued-goal", None),
        2_000,
    );
    for (id, goal, at) in [
        ("older-done", "older-done", 3_000u64),
        ("newer-done", "newer-done", 4_000),
    ] {
        apply(&mut agents, "subagent.start", child(id, goal, None), at);
        let mut done = child(id, goal, None);
        done["status"] = json!("completed");
        apply(&mut agents, "subagent.complete", done, at + 10);
    }
    let waiting = [
        WaitingRequest {
            seq: 5,
            label: "later-need".to_owned(),
            foreground: false,
        },
        WaitingRequest {
            seq: 1,
            label: "earlier-need".to_owned(),
            foreground: false,
        },
    ];
    let mut shown = work(None, None, &waiting, &agents);
    shown.recent_limit = 2;
    let lines = paint(&shown, 24);
    let earlier = position(&lines, "earlier-need");
    let later = position(&lines, "later-need");
    let running = position(&lines, "running-goal");
    let queued = position(&lines, "queued-goal");
    let older = position(&lines, "older-done");
    let newer = position(&lines, "newer-done");
    assert!(earlier < later && later < running && running < queued);
    assert!(queued < older && older < newer, "{lines:?}");
}

#[test]
fn waiting_rows_match_alt_a_order() {
    let agents = SubagentTree::new();
    let waiting = [
        WaitingRequest {
            seq: 4,
            label: "foreground-need".to_owned(),
            foreground: true,
        },
        WaitingRequest {
            seq: 3,
            label: "third-need".to_owned(),
            foreground: false,
        },
        WaitingRequest {
            seq: 1,
            label: "first-need".to_owned(),
            foreground: false,
        },
        WaitingRequest {
            seq: 2,
            label: "second-need".to_owned(),
            foreground: false,
        },
    ];
    let lines = paint(&work(None, None, &waiting, &agents), 16);
    let joined = lines.join("\n");
    assert!(
        !joined.contains("foreground-need"),
        "the focused session's own prompt is not repeated: {lines:?}"
    );
    let first = position(&lines, "first-need");
    let second = position(&lines, "second-need");
    let third = position(&lines, "third-need");
    assert!(first < second && second < third, "{lines:?}");
}

#[test]
fn model_and_effort_are_reported_or_unknown() {
    let mut agents = SubagentTree::new();
    apply(
        &mut agents,
        "subagent.start",
        child("named", "named-child", Some("child-model")),
        1_000,
    );
    apply(
        &mut agents,
        "subagent.start",
        child("bare", "bare-child", None),
        2_000,
    );
    let mut shown = work(None, None, &[], &agents);
    shown.parent_model = Some("parent-model");
    let lines = paint(&shown, 16);
    let joined = lines.join("\n");
    assert!(
        joined.contains("model: child-model"),
        "a reported model is shown: {lines:?}"
    );
    assert!(
        joined.contains("model: unknown"),
        "a missing model is unknown: {lines:?}"
    );
    assert!(
        joined.contains("effort: unknown"),
        "effort is unknown for every child: {lines:?}"
    );
    assert!(
        !joined.contains("parent-model") && !joined.contains("medium"),
        "neither the parent nor a default effort is borrowed: {lines:?}"
    );
}

#[test]
fn overflow_is_counted_and_bounded() {
    let agents = SubagentTree::new();
    let items = (0..6)
        .map(|index| (format!("task {index}"), "pending"))
        .collect::<Vec<_>>();
    let borrowed = items
        .iter()
        .map(|(content, status)| (content.as_str(), *status))
        .collect::<Vec<_>>();
    let tasks = checklist(&borrowed);
    let lines = paint(&work(None, Some(&tasks), &[], &agents), 3);
    assert!(
        lines.iter().any(|line| line.contains("Tasks")),
        "a section with content shows its heading: {lines:?}"
    );
    let hidden = lines.iter().find_map(|line| {
        let rest = line.trim().strip_prefix('+')?;
        let count = rest.split_whitespace().next()?.parse::<usize>().ok()?;
        line.contains("more").then_some(count)
    });
    let hidden = hidden.expect("overflow ends with +N more");
    let visible = lines
        .iter()
        .filter(|line| line.contains("[ ]") || line.contains("[x]") || line.contains("[>]"))
        .count();
    assert_eq!(
        visible + hidden,
        6,
        "the hidden count is the true N: {lines:?}"
    );

    let mut agents = SubagentTree::new();
    apply(
        &mut agents,
        "subagent.start",
        child("run", "running-goal", None),
        1_000,
    );
    let lines = paint(&work(None, Some(&tasks), &[], &agents), 8);
    if let (Some(tasks_at), Some(agents_at)) = (
        lines.iter().position(|line| line.contains("Tasks")),
        lines.iter().position(|line| line.contains("Agents")),
    ) {
        assert!(
            tasks_at < agents_at,
            "overflow does not reorder sections: {lines:?}"
        );
    }
}

#[test]
fn hostile_text_is_literal() {
    let title = format!("keep\u{1b}light\u{202e}end {}", "A".repeat(40));
    let mut hostile = goal(&title, "active", Some("ship\u{1b}it"));
    let mut agents = SubagentTree::new();
    apply(
        &mut agents,
        "subagent.start",
        child("odd", "child\u{202e}goal", Some("model\u{1b}name")),
        1_000,
    );
    hostile.last_verdict = Some("ship\u{1b}it".to_owned());
    let lines = paint(&work(Some(&hostile), None, &[], &agents), 16);
    let joined = lines.join("\n");
    assert!(
        joined.contains("keep") && joined.contains("light") && joined.contains("end"),
        "the goal's readable words survive: {lines:?}"
    );
    assert!(
        joined.contains("child") && joined.contains("goal") && joined.contains("model"),
        "child goal and model stay readable: {lines:?}"
    );
    assert!(!joined.contains(['\u{1b}', '\u{202e}']));
    assert!(
        joined.contains('\u{241b}') && joined.contains("U+202E"),
        "controls and overrides become visible stand-ins: {lines:?}"
    );
    assert!(!joined.contains(&"A".repeat(40)));
    assert!(
        lines.iter().any(|line| line.contains('…')),
        "a long word is cut to the content width: {lines:?}"
    );
}

#[test]
fn panel_has_no_voice_text() {
    let goal = goal("ship the notes", "active", None);
    let agents = SubagentTree::new();
    let mut shown = work(Some(&goal), None, &[], &agents);
    shown.voice_phase = Some("listening");
    shown.read_aloud = Some("read-aloud");
    shown.voice_target = Some("Herdr:review");
    let lines = paint(&shown, 12);
    let joined = lines.join("\n");
    assert!(
        joined.contains("ship the notes"),
        "the panel still shows the goal: {lines:?}"
    );
    for banned in ["listening", "read-aloud", "Herdr:review"] {
        assert!(!joined.contains(banned), "{banned} stays out of the panel");
    }
}
