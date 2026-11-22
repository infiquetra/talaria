//! The one-line workflow dock above the composer names the run and the key that opens it.

use ratatui::{Terminal, backend::TestBackend, style::Style};
use talaria_tui::live::LiveSection;
use talaria_tui::workflow_view::{WorkflowDock, WorkflowTheme, WorkflowView};

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
        "Turn Ultracode on with /ultracode on.",
        "With Ultracode on, a plain prompt goes to the talaria-workflows skill,",
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
