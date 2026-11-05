#[path = "pager_support.rs"]
mod support;

use ratatui::{buffer::Buffer, layout::Rect, style::Modifier, text::Line};
use serde_json::json;
use talaria_core::{checklist::Checklist, layout::PanelVisibility};
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

#[test]
fn workspace_snapshots_at_80x24_and_120x40() {
    let section =
        PlainTextSection::new("composer top\ncomposer draft\ncomposer bottom\nstatus voice");
    let footer = LiveView {
        sections: vec![&section],
    };
    for (width, height, panel, expected) in [
        (
            80,
            24,
            PanelVisibility::OverlayOpen,
            include_str!("snapshots/workspace/80x24.txt"),
        ),
        (
            120,
            40,
            PanelVisibility::Docked,
            include_str!("snapshots/workspace/120x40.txt"),
        ),
    ] {
        let view = WorkspaceView {
            entries: &[],
            details: Details::Expanded,
            panel,
            checklist: None,
            checklist_expanded: false,
            menu: workspace::menu::MenuState::hidden(),
            footer: &footer,
        };
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
        assert_eq!(
            format!("{}\n", text(&buffer)),
            expected,
            "{width}x{height}: no menu row; four unchanged footer rows"
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
            menu: workspace::menu::MenuState::hidden(),
            footer: &footer,
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
        menu: workspace::menu::MenuState::hidden(),
        footer: &footer,
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
                menu: workspace::menu::MenuState::hidden(),
                footer: &footer,
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

const MENU_HEADING: &str = "Talaria   Session   Model   View   Voice   Settings   Help";

fn raw_row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn frame_with(
    width: u16,
    height: u16,
    panel: PanelVisibility,
    menu: workspace::menu::MenuState,
) -> Buffer {
    let footer = LiveView { sections: vec![] };
    let view = WorkspaceView {
        entries: &[],
        details: Details::Expanded,
        panel,
        menu,
        checklist: None,
        checklist_expanded: false,
        footer: &footer,
    };
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
    buffer
}

#[test]
fn menu_bar_frames_at_80x24_and_120x40() {
    for (width, height, panel) in [
        (80, 24, PanelVisibility::OverlayOpen),
        (80, 24, PanelVisibility::Hidden),
        (120, 40, PanelVisibility::Docked),
        (120, 40, PanelVisibility::Hidden),
    ] {
        let buffer = frame_with(width, height, panel, workspace::menu::MenuState::shown());
        let row = raw_row(&buffer, 0);
        assert!(
            row.starts_with(MENU_HEADING),
            "{width}x{height} {panel:?}: {row}"
        );
        assert!(
            row[MENU_HEADING.len()..].chars().all(|ch| ch == ' '),
            "{row}"
        );
        if matches!(
            panel,
            PanelVisibility::Docked | PanelVisibility::OverlayOpen
        ) {
            assert!(
                raw_row(&buffer, 1).contains("Work"),
                "{width}x{height} {panel:?}: the panel starts under the bar"
            );
            let divider = if width == 120 { 87 } else { 47 };
            let heading_at_divider = MENU_HEADING.chars().nth(divider as usize).unwrap_or(' ');
            assert_eq!(
                buffer[(divider, 0)].symbol(),
                heading_at_divider.to_string(),
                "the bar covers the panel column"
            );
            assert_eq!(
                buffer[(divider, 1)].symbol(),
                "│",
                "the divider starts on the next row"
            );
        }
    }
}

#[test]
fn view_menu_open_and_focused() {
    for (width, height, panel) in [
        (80, 24, PanelVisibility::OverlayOpen),
        (80, 24, PanelVisibility::Hidden),
        (120, 40, PanelVisibility::Docked),
        (120, 40, PanelVisibility::Hidden),
    ] {
        let mut menu = workspace::menu::MenuState::shown();
        menu.dropdown_open = true;
        menu.menu_index = 2;
        menu.panel_on = matches!(
            panel,
            PanelVisibility::Docked | PanelVisibility::OverlayOpen
        );
        let buffer = frame_with(width, height, panel, menu.clone());
        let frame = text(&buffer);
        assert!(
            frame.contains("Right work panel"),
            "{width}x{height} {panel:?}: {frame}"
        );
        let line = frame
            .lines()
            .find(|line| line.contains("Right work panel"))
            .expect("item line");
        let checked = line.contains('✓') || line.contains("[x]") || line.contains("[X]");
        assert_eq!(checked, menu.panel_on, "{line}");
        let row = raw_row(&buffer, 0);
        let start = row.find("View").unwrap_or(0);
        let marked = row.contains('>')
            || row.contains('▸')
            || (start..start + "View".len()).any(|index| {
                buffer[(index as u16, 0)]
                    .modifier
                    .contains(Modifier::REVERSED)
            });
        assert!(marked, "focus needs reverse video or a text marker: {row}");
    }
}
