//! The pane flow through the app loop. Issue #246.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fake_fleet::{FakeFleetSink, FleetCall};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_app::app::{self, AppEffect, AppMessage, AppState, UserAction};
use talaria_app::fleet_actions;
use talaria_app::panes::{
    CommandRunner, CommandSpec, PaneEnv, PaneOutcome, PaneRequest, PaneSettings, RunResult,
    open_in_pane,
};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, Outcome, ProfileStatus, RowState};
use talaria_core::fleet_view::{FleetAction, FleetViewEffect, FleetViewInput, FleetViewState};
use talaria_tui::fleet_view::{FleetTheme, FleetView};

const NOW: f64 = 1_792_700_000.0;
const PAGE: usize = 4;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, state: RowState) -> FleetRow {
    FleetRow {
        key: key(profile, id),
        title: title.to_owned(),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 2,
        state,
        state_since: NOW - 20.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
    }
}

fn overlay() -> (FleetViewState, FleetRow) {
    let mut foreground = row("fixture", "s-front", "compile", RowState::Working);
    foreground.foreground = true;
    let working = row("fixture", "s-work", "migrate", RowState::Working);
    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    talaria_core::fleet_view::update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: vec![working.clone(), foreground, done],
            queue: Vec::new(),
            profiles: vec![(
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            )],
            scope_note: None,
            link_note: None,
        },
        PAGE,
    );
    (state, working)
}

/// A `CommandRunner` that records its calls and answers from a script.
struct FakeRunner {
    calls: Mutex<Vec<CommandSpec>>,
    script: Mutex<VecDeque<RunResult>>,
    delay: Duration,
}

impl FakeRunner {
    fn new(script: Vec<RunResult>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            script: Mutex::new(script.into()),
            delay: Duration::ZERO,
        }
    }

    fn calls(&self) -> Vec<CommandSpec> {
        self.calls.lock().expect("runner lock").clone()
    }
}

impl CommandRunner for FakeRunner {
    fn run(
        &self,
        spec: &CommandSpec,
        _timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = RunResult> + Send + '_>> {
        self.calls.lock().expect("runner lock").push(spec.clone());
        let out = self
            .script
            .lock()
            .expect("runner lock")
            .pop_front()
            .unwrap_or(RunResult {
                code: Some(1),
                stdout: String::new(),
                stderr: "unscripted call".to_owned(),
                timed_out: false,
            });
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            out
        })
    }
}

fn ok(stdout: &str) -> RunResult {
    RunResult {
        code: Some(0),
        stdout: stdout.to_owned(),
        stderr: String::new(),
        timed_out: false,
    }
}

/// A Herdr binary for the pane environment: a fake script on Unix, a name that
/// detects but never runs elsewhere. Detection reads strings only.
fn herdr_test_env(dir: &tempfile::TempDir) -> PaneEnv {
    PaneEnv {
        herdr_env: Some("1".to_owned()),
        herdr_pane_id: Some("w1:p1".to_owned()),
        herdr_bin_path: Some(herdr_test_bin(dir)),
        tmux: None,
        tmux_pane: None,
    }
}

#[cfg(unix)]
fn herdr_test_bin(dir: &tempfile::TempDir) -> String {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.path().join("herdr");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s' '{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}'\nexit 0\n",
    )
    .expect("the fake");
    let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).expect("executable");
    script.to_string_lossy().into_owned()
}

#[cfg(not(unix))]
fn herdr_test_bin(_dir: &tempfile::TempDir) -> String {
    "no-such-herdr".to_owned()
}

/// An exe path that exists and passes the command-line checks on any platform.
fn test_exe() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::current_exe()
            .expect("the test binary")
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

fn direct_request() -> PaneRequest {
    PaneRequest {
        key: key("fixture", "local-ab"),
        title: "build api".to_owned(),
        exe: test_exe(),
        cwd: None,
        cols: 160,
        rows: 48,
    }
}

fn committed(effects: &[AppEffect]) -> String {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.join("\n")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw_rows(state: &FleetViewState, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                state,
                None,
                frame.area(),
                frame.buffer_mut(),
                &FleetTheme::default(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
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

#[tokio::test]
async fn o_shows_opening_then_opened_and_acknowledges() {
    let dir = tempfile::tempdir().expect("a folder");
    let (fleet, working) = overlay();
    let mut app = AppState::new("launch");
    app.fleet = Some(fleet);
    let sink = Arc::new(FakeFleetSink::new());
    let trait_sink: Arc<dyn talaria_app::peek::FleetSink> = sink.clone();
    app.pane_sink = Some(trait_sink);
    app.pane_env = herdr_test_env(&dir);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.sender = Some(tx);

    let inputs =
        fleet_actions::execute(&mut app, FleetViewEffect::OpenInPane(working.key.clone())).await;
    assert_eq!(
        inputs,
        vec![FleetViewInput::Notice(
            "Opening \"migrate\" in a new Herdr pane…".to_owned()
        )]
    );

    // The direct open with a fake runner proves the outcome half.
    let runner = FakeRunner::new(vec![
        ok("{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}"),
        ok(""),
    ]);
    let direct = open_in_pane(
        &runner,
        &herdr_test_env(&dir),
        &PaneSettings::default(),
        &direct_request(),
    )
    .await;
    assert_eq!(
        direct,
        PaneOutcome::Opened {
            multiplexer: "Herdr"
        }
    );
    assert_eq!(runner.calls().len(), 2);

    // The completion the action's own task sent, or the direct one where no
    // fake binary can run: either way the overlay shows T2 and the row is
    // acknowledged once.
    #[cfg(unix)]
    let completed = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("the spawned open completes")
        .expect("a completion");
    #[cfg(not(unix))]
    let completed = {
        let _ = &mut rx;
        AppMessage::PaneOpened(working.key.clone(), direct)
    };
    let (done_key, outcome) = match completed {
        AppMessage::PaneOpened(done_key, outcome) => (done_key, outcome),
        _ => panic!("the task sent no pane completion"),
    };
    assert_eq!(done_key, working.key);
    app::update(&mut app, AppMessage::PaneOpened(done_key, outcome));
    assert_eq!(
        app.fleet.as_ref().expect("overlay").notice(),
        Some("Opened \"migrate\" in a new Herdr pane.")
    );
    assert_eq!(
        sink.calls(),
        vec![FleetCall::Acknowledge(working.key.clone())]
    );
}

#[tokio::test]
async fn no_multiplexer_shows_command_and_runs_nothing() {
    let (fleet, working) = overlay();
    let mut app = AppState::new("launch");
    app.fleet = Some(fleet);
    let sink = Arc::new(FakeFleetSink::new());
    let trait_sink: Arc<dyn talaria_app::peek::FleetSink> = sink.clone();
    app.pane_sink = Some(trait_sink);

    let runner = FakeRunner::new(Vec::new());
    let request = direct_request();
    let outcome = open_in_pane(
        &runner,
        &PaneEnv::default(),
        &PaneSettings::default(),
        &request,
    )
    .await;
    assert!(
        runner.calls().is_empty(),
        "no multiplexer, but the runner ran"
    );
    let (command, off) = match outcome {
        PaneOutcome::NoMultiplexer { command, off } => (command, off),
        other => panic!("expected no multiplexer, got {other:?}"),
    };
    assert!(!off);
    assert!(
        command.ends_with("attach 'local-ab' --profile 'fixture'"),
        "the shown command: {command:?}"
    );
    app::update(
        &mut app,
        AppMessage::PaneOpened(
            working.key.clone(),
            PaneOutcome::NoMultiplexer {
                command: command.clone(),
                off,
            },
        ),
    );
    let expected = format!(
        "Not inside Herdr or tmux. To watch \"migrate\" side by side, run this in another terminal: {command}"
    );
    assert_eq!(
        app.fleet.as_ref().expect("overlay").notice(),
        Some(expected.as_str())
    );
    assert!(sink.calls().is_empty(), "nothing opened, nothing acked");
}

#[tokio::test]
async fn second_o_while_in_flight_is_refused() {
    let dir = tempfile::tempdir().expect("a folder");
    let (fleet, working) = overlay();
    let mut app = AppState::new("launch");
    app.fleet = Some(fleet);
    let sink = Arc::new(FakeFleetSink::new());
    let trait_sink: Arc<dyn talaria_app::peek::FleetSink> = sink.clone();
    app.pane_sink = Some(trait_sink);
    app.pane_env = herdr_test_env(&dir);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.sender = Some(tx);

    let _first =
        fleet_actions::execute(&mut app, FleetViewEffect::OpenInPane(working.key.clone())).await;
    let second =
        fleet_actions::execute(&mut app, FleetViewEffect::OpenInPane(working.key.clone())).await;
    assert_eq!(
        second,
        vec![FleetViewInput::Notice(
            "Still opening the last pane. Try again in a moment.".to_owned()
        )]
    );

    // The first open completes alone: one acknowledgement, one completion.
    app::update(
        &mut app,
        AppMessage::PaneOpened(
            working.key.clone(),
            PaneOutcome::Opened {
                multiplexer: "Herdr",
            },
        ),
    );
    assert_eq!(
        sink.calls(),
        vec![FleetCall::Acknowledge(working.key.clone())]
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let first_completion = loop {
        if let Ok(completed) = rx.try_recv() {
            break completed;
        }
        assert!(Instant::now() < deadline, "the first open never completed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(
        matches!(first_completion, AppMessage::PaneOpened(ref done_key, _) if done_key == &working.key),
        "the first completion was not the pane opening"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        rx.try_recv().is_err(),
        "the refused second open sent a completion"
    );
}

#[tokio::test]
async fn outcome_after_overlay_closed_goes_to_main_screen() {
    let dir = tempfile::tempdir().expect("a folder");
    let (fleet, working) = overlay();
    let mut app = AppState::new("launch");
    app.fleet = Some(fleet);
    let sink = Arc::new(FakeFleetSink::new());
    let trait_sink: Arc<dyn talaria_app::peek::FleetSink> = sink.clone();
    app.pane_sink = Some(trait_sink);
    app.pane_env = herdr_test_env(&dir);

    // The open starts with the overlay up, so its title is known; then the
    // overlay closes before the completion lands.
    let _started =
        fleet_actions::execute(&mut app, FleetViewEffect::OpenInPane(working.key.clone())).await;
    app.fleet = None;
    let effects = app::update(
        &mut app,
        AppMessage::PaneOpened(
            working.key.clone(),
            PaneOutcome::Opened {
                multiplexer: "Herdr",
            },
        ),
    );
    let heard = committed(&effects);
    assert!(
        heard.contains("Opened \"migrate\" in a new Herdr pane."),
        "the main screen never heard: {heard:?}"
    );
    assert_eq!(
        sink.calls(),
        vec![FleetCall::Acknowledge(working.key.clone())]
    );
}

#[tokio::test]
async fn panel_target_uses_supplied_title_and_acknowledges_only_opened() {
    let dir = tempfile::tempdir().expect("a folder");
    let mut app = AppState::new("launch");
    let sink = Arc::new(FakeFleetSink::new());
    app.pane_sink = Some(sink.clone());
    app.pane_env = herdr_test_env(&dir);
    app.pane_runner = Some(Arc::new(FakeRunner::new(vec![
        ok("{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}"),
        ok(""),
    ])));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.sender = Some(tx);
    let child = key("fixture", "local-child");
    let effects = app::update(
        &mut app,
        AppMessage::User(UserAction::OpenInPane {
            key: child.clone(),
            title: "child task".to_owned(),
        }),
    );
    assert!(committed(&effects).contains("Opening \"child task\" in a new Herdr pane…"));
    assert!(sink.calls().is_empty());
    let completion = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("pane task finishes")
        .expect("completion");
    let effects = app::update(&mut app, completion);
    assert!(committed(&effects).contains("Opened \"child task\" in a new Herdr pane."));
    assert_eq!(sink.calls(), [FleetCall::Acknowledge(child)]);
}

#[tokio::test]
async fn slow_split_never_blocks_the_loop() {
    let dir = tempfile::tempdir().expect("a folder");
    let (fleet, working) = overlay();
    let mut app = AppState::new("launch");
    app.fleet = Some(fleet);
    app.pane_env = herdr_test_env(&dir);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.sender = Some(tx);
    let mut runner = FakeRunner::new(vec![
        ok("{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}"),
        ok(""),
    ]);
    runner.delay = Duration::from_secs(2);
    let runner = Arc::new(runner);
    app.pane_runner = Some(runner.clone());

    // The opening notice is in the frame of the key itself, before the spawned
    // open can finish.
    let effects = app::update(
        &mut app,
        AppMessage::FleetView(FleetViewInput::Action(FleetAction::OpenInPane)),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
    );
    let rows = draw_rows(app.fleet.as_ref().expect("overlay"), 80, 24);
    assert!(
        rows.iter()
            .any(|row| row.contains("Opening \"migrate\" in a new Herdr pane…")),
        "the key's frame has no opening notice: {rows:?}"
    );
    tokio::time::timeout(Duration::from_millis(500), async {
        while runner.calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the split starts on its task");

    // Fifty cursor keys, timed from key to frame: the 48th smallest (the 95th
    // percentile of 50) fits the 16 ms keystroke budget.
    let mut times: Vec<Duration> = Vec::new();
    for _ in 0..50 {
        let started = Instant::now();
        app::update(
            &mut app,
            AppMessage::FleetView(FleetViewInput::Action(FleetAction::Down)),
        );
        let _ = draw_rows(app.fleet.as_ref().expect("overlay"), 80, 24);
        times.push(started.elapsed());
    }
    times.sort();
    assert!(
        times[47] <= Duration::from_millis(16),
        "the loop stalled behind the open: {:?}",
        times[47]
    );
    assert_eq!(runner.calls().len(), 1, "the slow split is still in flight");
    assert_eq!(
        app.fleet
            .as_ref()
            .expect("overlay")
            .find_row(&working.key)
            .expect("working row")
            .title,
        "migrate"
    );
}

#[tokio::test]
async fn agents_panel_receives_pane_notices_without_writing_under_the_overlay() {
    use talaria_app::app::UserAction;
    use talaria_app::subagents::{SubagentHub, SubagentsConfig};
    let dir = tempfile::tempdir().expect("a folder");
    let mut app = AppState::new("panel");
    app.pane_env = herdr_test_env(&dir);
    app.pane_runner = Some(Arc::new(FakeRunner::new(vec![
        ok("{\"result\":{\"pane\":{\"pane_id\":\"w1:p7\"}}}"),
        ok(""),
    ])));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    app.sender = Some(tx.clone());
    let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
    hub.open_panel(key("fixture", "owner"));
    app.subagents = Some(hub);
    let effects = app::update(
        &mut app,
        AppMessage::User(UserAction::OpenInPane {
            key: key("fixture", "local-child"),
            title: "child task".to_owned(),
        }),
    );
    assert!(committed(&effects).is_empty());
    assert_eq!(
        app.subagents.as_ref().expect("panel").flash(),
        Some("Opening \"child task\" in a new Herdr pane…")
    );
    let completion = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("completion deadline")
        .expect("completion");
    let effects = app::update(&mut app, completion);
    assert!(committed(&effects).is_empty());
    assert_eq!(
        app.subagents.as_ref().expect("panel").flash(),
        Some("Opened \"child task\" in a new Herdr pane.")
    );
}
