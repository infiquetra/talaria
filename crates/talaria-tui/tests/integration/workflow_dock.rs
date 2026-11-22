//! The one-line workflow dock above the composer names the run and the key that opens it. While
//! Hivecode builds a workflow, the building block in the same place says which step it is on.

use ratatui::{Terminal, backend::TestBackend, style::Style};
use talaria_core::hivecode::authoring::{AuthoringProgress, AuthoringStage};
use talaria_tui::live::LiveSection;
use talaria_tui::workflow_view::{AuthoringDock, WorkflowDock, WorkflowTheme, WorkflowView};

fn text(dock: &WorkflowDock<'_>, width: u16) -> Vec<String> {
    dock.lines(width)
        .into_iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

#[test]
fn dock_line_names_run_counts_and_key() {
    let dock = WorkflowDock::new(
        "review-branch",
        "running",
        3,
        10,
        Some("Alt+W"),
        Style::new(),
        Style::new(),
    );
    assert_eq!(
        text(&dock, 100),
        ["⚙ Workflow review-branch · running · 3/10 agents · Alt+W to open"]
    );
    assert_eq!(dock.height(100), 1);
    let narrow = text(&dock, 50);
    assert_eq!(narrow, ["⚙ Workflow review-branch · running · 3/10 agents"]);
    let tiny = text(&dock, 12);
    assert_eq!(tiny.len(), 1);
    assert!(tiny[0].chars().count() <= 12, "{tiny:?}");
    let unbound = WorkflowDock::new(
        "review-branch",
        "parked",
        0,
        2,
        None,
        Style::new(),
        Style::new(),
    );
    assert_eq!(
        text(&unbound, 100),
        ["⚙ Workflow review-branch · parked · 0/2 agents · /workflows to open"]
    );
}

#[test]
fn dock_hidden_without_room() {
    let dock = WorkflowDock::new(
        "review-branch",
        "running",
        1,
        1,
        Some("Alt+W"),
        Style::new(),
        Style::new(),
    );
    assert!(text(&dock, 0).is_empty());
    assert_eq!(dock.height(0), 0);
}

fn authoring(
    stage: AuthoringStage,
    script: Option<&str>,
    elapsed_ms: u64,
) -> talaria_core::hivecode::authoring::AuthoringView {
    talaria_core::hivecode::authoring::AuthoringView {
        progress: AuthoringProgress {
            stage,
            script: script.map(str::to_owned),
        },
        elapsed_ms,
    }
}

/// What the section draws into `width` columns, row by row, with no colour at all.
fn drawn(section: &dyn LiveSection, width: u16) -> Vec<String> {
    let height = section.height(width);
    let area = ratatui::layout::Rect::new(0, 0, width, height);
    let mut buffer = ratatui::buffer::Buffer::empty(area);
    section.render(area, &mut buffer);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn building_block_names_the_step_script_and_time_in_plain_words() {
    let view = authoring(AuthoringStage::Writing, Some("fix-parser.workflow"), 65_000);
    let block = AuthoringDock::new(&view, 2, Style::new(), Style::new());
    assert_eq!(
        drawn(&block, 80),
        [
            "⚙ Hivecode · building a workflow · 1m 05s",
            "  Step 2 of 5: writing the workflow script · fix-parser.workflow",
        ]
    );
    let planning = authoring(AuthoringStage::Planning, None, 4_000);
    let block = AuthoringDock::new(&planning, 2, Style::new(), Style::new());
    assert_eq!(
        drawn(&block, 80),
        [
            "⚙ Hivecode · building a workflow · 4s",
            "  Step 1 of 5: planning the workflow",
        ]
    );
}

#[test]
fn building_block_fits_sixty_columns_and_narrower() {
    let view = authoring(
        AuthoringStage::Waiting,
        Some("a-rather-long-workflow-name.workflow"),
        65_000,
    );
    let block = AuthoringDock::new(&view, 2, Style::new(), Style::new());
    let rows = drawn(&block, 60);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0], "⚙ Hivecode · building a workflow · 1m 05s");
    assert!(
        rows[1].starts_with("  Step 4 of 5: waiting for your approval · a-rather"),
        "{rows:?}"
    );
    assert!(rows[1].ends_with('…'), "the script name is cut: {rows:?}");
    for width in [60, 30, 12, 1] {
        for row in drawn(&block, width) {
            assert!(
                unicode_width::UnicodeWidthStr::width(row.as_str()) <= usize::from(width),
                "{row:?} is wider than {width}"
            );
        }
    }
    assert!(drawn(&block, 0).is_empty());
}

#[test]
fn building_block_is_one_row_on_a_short_screen() {
    let view = authoring(AuthoringStage::Checking, Some("fix-parser.workflow"), 9_000);
    let block = AuthoringDock::new(&view, 1, Style::new(), Style::new());
    assert_eq!(drawn(&block, 60), ["⚙ Hivecode · checking the script · 9s"]);
}

#[test]
fn empty_picker_says_how_runs_start_and_lists_keys() {
    let directory = tempfile::tempdir().unwrap();
    let picker = talaria_workflow::runner::ProgressPicker::load(directory.path());
    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    terminal
        .draw(|frame| {
            WorkflowView::render_picker(
                frame.area(),
                frame.buffer_mut(),
                &picker,
                &WorkflowTheme::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let screen = rows.join("\n");
    for phrase in [
        "no runs yet — talaria workflow run starts one",
        "Turn Hivecode on with /hivecode on.",
        "With Hivecode on, a plain prompt goes to the talaria-workflows skill,",
        "and the agent proposes a workflow and asks before starting it.",
        "esc close",
    ] {
        assert!(screen.contains(phrase), "{phrase:?}:\n{screen}");
    }
    assert!(
        !screen.contains("Runs start when"),
        "a prompt does not start a run by itself:\n{screen}"
    );
}
