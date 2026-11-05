#[path = "pager_support.rs"]
mod support;

use hermes_contract::events::EventFrame;
use ratatui::{buffer::Buffer, layout::Rect, text::Line};
use serde_json::json;
use talaria_core::automation::GoalView;
use talaria_core::subagents::{Clock, SubagentEvent, SubagentTree};
use talaria_core::{checklist::Checklist, layout::PanelVisibility};
use talaria_tui::status::TurnState;
use talaria_tui::workspace::panel::{WaitingRequest, WorkContents};
use talaria_tui::{
    history::PlainCellLines,
    live::{LiveView, PlainTextSection},
    pager::{Details, Pager, SearchIndex, TranscriptEntry, draw_transcript_body},
    workspace::{self, WorkspaceView},
};

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| support::row_text(buffer, y))
        .collect::<Vec<_>>()
        .join("\n")
}

fn child(tree: &mut SubagentTree, id: &str, goal: &str, model: Option<&str>, ms: u64) {
    let mut payload = json!({
        "subagent_id": id,
        "goal": goal,
        "task_index": 0,
        "task_count": 1,
    });
    if let Some(model) = model {
        payload["model"] = json!(model);
    }
    let frame = EventFrame::from_params(&json!({
        "type": "subagent.start",
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

fn panel_column(buffer: &Buffer, divider: u16, rows: u16) -> Vec<String> {
    let start = divider + 1;
    (0..rows)
        .map(|y| {
            let line: String = (start..start + 32)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect()
}

#[test]
fn workspace_snapshots_at_80x24_and_120x40() {
    let section =
        PlainTextSection::new("composer top\ncomposer draft\ncomposer bottom\nstatus voice");
    let footer = LiveView {
        sections: vec![&section],
    };
    let goal = GoalView {
        title: "keep the interface light".to_owned(),
        status: "active".to_owned(),
        created_ms: Some(1),
        last_verdict: None,
        last_reason: None,
        turns_used: 1,
        max_turns: 20,
        subgoals: 0,
        gates: 0,
        paused_reason: None,
        wait: None,
    };
    let checklist = Checklist::from_values(
        1,
        &[
            json!({"id": "1", "content": "Read inputs", "status": "completed"}),
            json!({"id": "2", "content": "Compare layouts", "status": "in_progress"}),
            json!({"id": "3", "content": "Review screens", "status": "pending"}),
        ],
    );
    let waiting = [WaitingRequest {
        seq: 1,
        label: "review: needs you".to_owned(),
        foreground: false,
    }];
    let mut agents = SubagentTree::new();
    child(
        &mut agents,
        "layout",
        "layout: compare",
        Some("GPT-6.1 Terra"),
        1_000,
    );
    let work = WorkContents {
        needs_you: 1,
        goal: Some(&goal),
        turn: TurnState::Streaming,
        verdict_previous_turn: false,
        checklist: Some(&checklist),
        checklist_expanded: true,
        waiting: &waiting,
        agents: &agents,
        parent_model: Some("parent-model"),
        recent_limit: 2,
        voice_phase: Some("listening"),
        read_aloud: Some("read-aloud"),
        voice_target: Some("Herdr:review"),
    };
    for (width, height, panel, divider, expected) in [
        (
            80,
            24,
            PanelVisibility::OverlayOpen,
            47,
            include_str!("snapshots/workspace/80x24.txt"),
        ),
        (
            120,
            40,
            PanelVisibility::Docked,
            87,
            include_str!("snapshots/workspace/120x40.txt"),
        ),
    ] {
        let view = WorkspaceView {
            entries: &[],
            details: Details::Expanded,
            panel,
            checklist: Some(&checklist),
            checklist_expanded: true,
            footer: &footer,
            work: Some(&work),
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
        assert_eq!(
            buffer[(divider, 0)].symbol(),
            "│",
            "divider at {width}x{height}"
        );
        let left: String = (0..divider).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(
            left.trim_end().starts_with("No messages yet"),
            "{width}x{height}: no menu row: {left}"
        );
        assert_eq!(support::row_text(&buffer, height - 4), "composer top");
        assert_eq!(support::row_text(&buffer, height - 3), "composer draft");
        assert_eq!(support::row_text(&buffer, height - 2), "composer bottom");
        assert_eq!(support::row_text(&buffer, height - 1), "status voice");
        let body = height - 4;
        let mut rendered = panel_column(&buffer, divider, body);
        assert_eq!(rendered.len(), usize::from(body));
        while rendered.last().is_some_and(String::is_empty) {
            rendered.pop();
        }
        assert_eq!(
            rendered,
            expected.lines().map(str::to_owned).collect::<Vec<_>>(),
            "{width}x{height}: panel contents"
        );
    }
}

#[test]
fn pager_and_workspace_share_identical_body_rows() {
    for details in [Details::Expanded, Details::Compact] {
        for width in [47, 80, 87, 120] {
            let cells = vec![
                support::user(1, "literal [bold] **text** 会話"),
                support::assistant(2, "answer\n\nsecond paragraph", true),
            ];
            let entries = cells.iter().map(TranscriptEntry::Cell).collect::<Vec<_>>();
            let area = Rect::new(0, 0, width, 18);
            let mut shared = Buffer::empty(area);
            draw_transcript_body(&entries, details, area, &mut shared, &PlainCellLines);
            let mut workspace = Buffer::empty(area);
            workspace::draw_body(&entries, details, area, &mut workspace, &PlainCellLines);
            assert_eq!(
                shared, workspace,
                "both views use the same body at width {width}"
            );
            let context = support::context(&cells);
            let mut index = SearchIndex::new();
            let mut pager = Pager::open(&context, width, 20, &mut index);
            if details == Details::Compact {
                pager.handle_action(
                    talaria_tui::pager::PagerAction::ToggleDetails,
                    &context,
                    &mut index,
                );
            }
            let mut full = Buffer::empty(Rect::new(0, 0, width, 20));
            pager.draw(full.area, &mut full, &context, &PlainCellLines);
            for y in 0..18 {
                assert_eq!(
                    support::row_text(&shared, y),
                    support::row_text(&full, y + 1),
                    "existing pager body row {y}, width {width}, {details:?}"
                );
            }
        }
    }
}

#[test]
fn command_output_is_visible_in_the_workspace() {
    let before = support::user(1, "before cell");
    let after = support::assistant(2, "after cell", true);
    let outputs = [
        "command output",
        "context block",
        "checklist block",
        "subagent batch",
    ]
    .map(|s| vec![Line::from(s)]);
    let mut entries = vec![TranscriptEntry::Cell(&before)];
    entries.extend(outputs.iter().map(|lines| TranscriptEntry::Lines(lines)));
    entries.push(TranscriptEntry::Cell(&after));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 87, 36));
    workspace::draw_body(
        &entries,
        Details::Expanded,
        buffer.area,
        &mut buffer,
        &PlainCellLines,
    );
    let actual = text(&buffer);
    let mut previous = None;
    for wanted in [
        "before cell",
        "command output",
        "context block",
        "checklist block",
        "subagent batch",
        "after cell",
    ] {
        assert_eq!(actual.matches(wanted).count(), 1, "{wanted}: {actual}");
        let at = actual.find(wanted).unwrap();
        assert!(
            previous.is_none_or(|before| before < at),
            "output interleaving: {actual}"
        );
        previous = Some(at);
    }
}

#[test]
fn panel_geometry_matches_wide_and_compact_screens() {
    let footer = LiveView { sections: vec![] };
    for (width, height, panel, divider) in [
        (120, 40, PanelVisibility::Docked, 87),
        (80, 24, PanelVisibility::OverlayOpen, 47),
    ] {
        let view = WorkspaceView {
            entries: &[],
            details: Details::Expanded,
            panel,
            checklist: None,
            checklist_expanded: false,
            footer: &footer,
            work: None,
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
        assert_eq!(
            buffer[(divider, 0)].symbol(),
            "│",
            "divider at {width}x{height}"
        );
        assert!(
            support::row_text(&buffer, 0).contains("Work"),
            "panel header occupies the first body row"
        );
    }
}

#[test]
fn footer_matches_the_inline_footer() {
    let composer = PlainTextSection::new("composer draft");
    let status = PlainTextSection::new("model · listening · read-aloud");
    let footer = LiveView {
        sections: vec![&composer, &status],
    };
    let view = WorkspaceView {
        entries: &[],
        details: Details::Expanded,
        panel: PanelVisibility::Docked,
        checklist: None,
        checklist_expanded: false,
        footer: &footer,
        work: None,
    };
    let mut buffer = Buffer::empty(Rect::new(0, 0, 120, 40));
    workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
    assert_eq!(support::row_text(&buffer, 38), "composer draft");
    assert_eq!(
        support::row_text(&buffer, 39),
        "model · listening · read-aloud"
    );
    assert!(
        !text(&buffer)
            .lines()
            .take(38)
            .any(|line| line.contains("listening") || line.contains("read-aloud"))
    );
}

#[test]
fn checklist_is_hosted_once() {
    let items = (0..50)
        .map(|n| json!({"id":n.to_string(),"content":format!("task {n}"),"status":"pending"}))
        .collect::<Vec<_>>();
    let checklist = Checklist::from_values(1, &items);
    let footer = LiveView { sections: vec![] };
    for panel in [
        PanelVisibility::Docked,
        PanelVisibility::OverlayOpen,
        PanelVisibility::Dismissed,
    ] {
        for expanded in [true, false] {
            let view = WorkspaceView {
                entries: &[],
                details: Details::Expanded,
                panel,
                checklist: Some(&checklist),
                checklist_expanded: expanded,
                footer: &footer,
                work: None,
            };
            let mut buffer = Buffer::empty(Rect::new(0, 0, 120, 40));
            workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
            let actual = text(&buffer);
            assert_eq!(
                actual.matches("Tasks").count(),
                1,
                "{panel:?} expanded={expanded}: {actual}"
            );
            if expanded {
                assert!(
                    actual.contains("more"),
                    "overflow must be counted: {actual}"
                );
            }
        }
    }
}

#[test]
fn hostile_text_is_literal_in_both_kinds_of_entry() {
    let cell = support::user(1, "[red]literal\u{1b}[2J\u{202e}suffix");
    let local = vec![Line::from("[blue]local\u{1b}[2J\u{202e}suffix")];
    let entries = [TranscriptEntry::Cell(&cell), TranscriptEntry::Lines(&local)];
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 20));
    workspace::draw_body(
        &entries,
        Details::Expanded,
        buffer.area,
        &mut buffer,
        &PlainCellLines,
    );
    let actual = text(&buffer);
    assert!(
        actual.contains("[red]literal") && actual.contains("[blue]local"),
        "{actual}"
    );
    assert!(!actual.contains(['\u{1b}', '\u{202e}']));
    assert_eq!(actual.matches("suffix").count(), 2);
}

#[test]
fn reused_cell_ids_from_separate_sources_stay_distinct() {
    let first = support::user(1, "first source");
    let second = support::assistant(1, "second source", true);
    let output = vec![Line::from("local command output")];
    let entries = [
        TranscriptEntry::Cell(&first),
        TranscriptEntry::Lines(&output),
        TranscriptEntry::Cell(&second),
    ];
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 16));
    workspace::draw_body(
        &entries,
        Details::Expanded,
        buffer.area,
        &mut buffer,
        &PlainCellLines,
    );
    let actual = text(&buffer);
    let mut previous = None;
    for wanted in ["first source", "local command output", "second source"] {
        assert_eq!(actual.matches(wanted).count(), 1, "{wanted}: {actual}");
        let at = actual.find(wanted).unwrap();
        assert!(previous.is_none_or(|before| before < at), "{actual}");
        previous = Some(at);
    }
}
