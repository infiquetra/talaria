//! The one-line workflow dock above the composer names the run and the key that opens it. While
//! Hivecode builds a workflow, the building block in the same place says which step it is on.

use ratatui::{Terminal, backend::TestBackend, style::Style};
use talaria_core::hivecode::authoring::{AuthoringProgress, AuthoringStage};
use talaria_tui::live::LiveSection;
use talaria_tui::theme::Motion;
use talaria_tui::workflow_view::{
    AuthoringDock, RunClock, WorkflowAgentDock, WorkflowDock, WorkflowTheme, WorkflowView,
};

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

/// A started-at time of 5 minutes 51 seconds before `NOW`.
const NOW: u64 = 1_800_000_000_000;
const STARTED: i64 = (NOW - 351_000) as i64;

fn clocked(status: &str, clock: Option<RunClock>) -> WorkflowDock<'_> {
    WorkflowDock::new(
        "galaga-forge",
        status,
        1,
        4,
        Some("Alt+W"),
        Style::new(),
        Style::new(),
    )
    .with_clock(clock)
}

#[test]
fn a_running_run_shows_a_spinner_and_the_time_since_it_started() {
    let clock = RunClock::of_run(true, STARTED, NOW, 200, Motion::Full);
    assert_eq!(
        text(&clocked("running", clock), 100),
        ["⠹ Workflow galaga-forge · running 5m51s · 1/4 agents · Alt+W to open"]
    );
    // The spinner moves with the tick and the time with the wall clock.
    let later = RunClock::of_run(true, STARTED, NOW + 2_000, 280, Motion::Full);
    assert_eq!(
        text(&clocked("running", later), 100),
        ["⠸ Workflow galaga-forge · running 5m53s · 1/4 agents · Alt+W to open"]
    );
}

#[test]
fn reduced_motion_keeps_the_glyph_still_and_the_seconds_counting() {
    let clock = RunClock::of_run(true, STARTED, NOW, 200, Motion::Reduced);
    assert_eq!(
        text(&clocked("running", clock), 100),
        ["⚙ Workflow galaga-forge · running 5m51s · 1/4 agents · Alt+W to open"]
    );
    let later = RunClock::of_run(true, STARTED, NOW + 1_000, 99_999, Motion::Reduced);
    assert_eq!(
        text(&clocked("running", later), 100),
        ["⚙ Workflow galaga-forge · running 5m52s · 1/4 agents · Alt+W to open"]
    );
}

#[test]
fn an_unknown_start_time_shows_no_time_rather_than_a_wrong_one() {
    for started in [0, -5, (NOW + 60_000) as i64] {
        let clock = RunClock::of_run(true, started, NOW, 200, Motion::Full);
        assert_eq!(
            text(&clocked("running", clock), 100),
            ["⠹ Workflow galaga-forge · running · 1/4 agents · Alt+W to open"],
            "start {started}"
        );
    }
}

#[test]
fn a_run_that_is_not_running_holds_still() {
    // The caller gives no clock for any other state, so the line keeps its wording.
    for status in [
        "completed",
        "completed_with_failures",
        "failed",
        "aborted",
        "budget_exhausted",
        "parked",
        "interrupted",
    ] {
        assert_eq!(
            RunClock::of_run(false, STARTED, NOW, 200, Motion::Full),
            None
        );
        let shown = text(&clocked(status, None), 100);
        assert_eq!(
            shown,
            [format!(
                "⚙ Workflow galaga-forge · {} · 1/4 agents · Alt+W to open",
                status.replace('_', " ")
            )]
        );
    }
}

#[test]
fn a_narrow_line_drops_the_hint_then_the_time_and_keeps_the_agent_count() {
    let clock = RunClock::of_run(true, STARTED, NOW, 200, Motion::Full);
    let dock = clocked("running", clock);
    let full = "⠹ Workflow galaga-forge · running 5m51s · 1/4 agents";
    let width = u16::try_from(full.chars().count()).unwrap();
    assert_eq!(text(&dock, width), [full]);
    assert_eq!(
        text(&dock, width - 1),
        ["⠹ Workflow galaga-forge · running · 1/4 agents"]
    );
    let tiny = text(&dock, 12);
    assert_eq!(tiny.len(), 1);
    assert!(tiny[0].chars().count() <= 12, "{tiny:?}");
}

#[test]
fn the_line_asks_for_a_redraw_only_while_something_on_it_moves() {
    let full = RunClock::of_run(true, STARTED, NOW, 200, Motion::Full).unwrap();
    assert_eq!(full.wake_after_ms(), Some(talaria_tui::activity::FRAME_MS));
    // Reduced motion waits for the next whole second of the run.
    let reduced = RunClock::of_run(true, STARTED, NOW + 250, 0, Motion::Reduced).unwrap();
    assert_eq!(reduced.wake_after_ms(), Some(750));
    // Reduced motion with no known start has nothing that moves.
    let still = RunClock::of_run(true, 0, NOW, 0, Motion::Reduced).unwrap();
    assert_eq!(still.wake_after_ms(), None);
    // A run that is not running has no clock, so nothing asks for a redraw.
    assert_eq!(RunClock::of_run(false, STARTED, NOW, 0, Motion::Full), None);
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

#[test]
fn agent_dock_names_the_agent_its_state_and_the_key_to_the_run() {
    let banner = "Workflow agent review:gameplay · failed · this conversation is not tracked by the run · run review-branch (…8Y0A1C)";
    let lines = |dock: &WorkflowAgentDock<'_>, width: u16| -> Vec<String> {
        dock.lines(width)
            .into_iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    };
    let dock = WorkflowAgentDock::new(banner, Some("Alt+W"), Style::new(), Style::new());
    assert_eq!(
        lines(&dock, 200),
        [format!("⚙ {banner} · Alt+W opens the run")]
    );
    assert_eq!(dock.height(200), 1);
    // A narrow screen drops the key, then cuts the end: the agent and its state stay.
    let narrow = lines(&dock, 60);
    assert_eq!(narrow.len(), 1);
    assert!(
        narrow[0].starts_with("⚙ Workflow agent review:gameplay · failed"),
        "{narrow:?}"
    );
    assert!(narrow[0].chars().count() <= 60, "{narrow:?}");
    let unbound = WorkflowAgentDock::new(
        "Workflow agent a · running",
        None,
        Style::new(),
        Style::new(),
    );
    assert_eq!(
        lines(&unbound, 100),
        ["⚙ Workflow agent a · running · /workflows opens the run"]
    );
    assert!(lines(&dock, 0).is_empty());
    assert_eq!(dock.height(0), 0);
}

#[test]
fn agent_dock_text_from_the_journal_cannot_inject_escape_sequences() {
    let dock = WorkflowAgentDock::new(
        "Workflow agent \u{1b}[31mred\nline",
        None,
        Style::new(),
        Style::new(),
    );
    let rows: Vec<String> = dock
        .lines(100)
        .into_iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].contains('\u{1b}'), "{rows:?}");
}
