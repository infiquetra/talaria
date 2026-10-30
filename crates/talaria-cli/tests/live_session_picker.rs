//! Live session picker runs against a running Hermes gateway. Ignored unless a person asks
//! for them. A skip is not a pass: with `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//!
//! Run by hand, one test at a time, with the gateway of the home named by `HERMES_HOME`:
//! `TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_session_picker --
//! --ignored --nocapture --test-threads=1`

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use talaria_app::app::{
    ComposerDeadline, Outcome, RunSetup, Screen, StatusHost, UserAction, View, run,
};
use talaria_app::context::ContextBlock;
use talaria_app::directory::DirectoryService;
use talaria_app::launch::{Launch, StartMode};
use talaria_core::session::Cell;
use tokio::sync::mpsc;

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(hermes_client::default_home)
}

fn skip_reason() -> Option<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .ok()?;
    let found = runtime.block_on(async {
        let options = hermes_client::DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: hermes_client::EnsurePolicy::Never,
        };
        hermes_client::discover_with(&hermes_home(), &options)
            .await
            .map(|endpoint| endpoint.code_sha().map(str::to_owned))
            .map_err(|error| error.to_string())
    });
    match found {
        Ok(sha) => hermes_contract::pinned::live_skip_reason(sha.as_deref()),
        Err(reason) => Some(reason),
    }
}

/// The five tests share one gateway, so they run one at a time.
static LIVE_LOCK: Mutex<()> = Mutex::new(());

fn begin() -> bool {
    if let Some(reason) = skip_reason() {
        println!("LIVE_SESSION_PICKER_RESULT: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return false;
    }
    true
}

fn passed() {
    println!("LIVE_SESSION_PICKER_RESULT: PASSED");
}

#[derive(Default)]
struct ScreenLog {
    lines: Vec<String>,
    session: Option<String>,
    restored: bool,
    running: bool,
    cells: usize,
}

type Log = Arc<Mutex<ScreenLog>>;

fn locked(log: &Log) -> MutexGuard<'_, ScreenLog> {
    log.lock().expect("the log is open")
}

struct Probe {
    log: Log,
}

impl ComposerDeadline for Probe {
    fn tick(&mut self, _now: Instant) {}

    fn next_tick(&self) -> Option<Instant> {
        None
    }
}

impl StatusHost for Probe {
    fn on_tick(&mut self, _now: Instant) {}

    fn next_wake(&self, _now: Instant) -> Option<Instant> {
        None
    }

    fn shutdown(&mut self) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }
}

impl Screen for Probe {
    type Input = UserAction;

    fn commit_context(&mut self, _block: &ContextBlock) {}

    fn input(&mut self, input: UserAction, _view: &View<'_>, _now: Instant) -> Vec<UserAction> {
        vec![input]
    }

    fn draw(&mut self, view: &View<'_>) {
        let mut log = locked(&self.log);
        log.session = view.session_id.map(str::to_string);
        log.running = view.turn_running();
    }

    fn commit_lines(&mut self, lines: Vec<String>) {
        locked(&self.log).lines.extend(lines);
    }

    fn commit_cells(&mut self, cells: &[Cell]) {
        locked(&self.log).cells += cells.len();
    }

    fn load_draft(&mut self, _text: &str) {}

    fn restore(&mut self) {
        locked(&self.log).restored = true;
    }
}

struct Driver {
    inputs: mpsc::UnboundedSender<UserAction>,
    log: Log,
}

impl Driver {
    fn submit(&self, text: &str) {
        self.inputs
            .send(UserAction::Submit(text.to_string()))
            .expect("the loop is running");
    }

    fn picker(&self, input: talaria_core::directory::state::PickerInput) {
        self.inputs
            .send(UserAction::Picker(input))
            .expect("the loop is running");
    }

    fn quit(&self) {
        self.inputs
            .send(UserAction::Quit)
            .expect("the loop is running");
    }

    async fn line(&self, text: &str) {
        let log = self.log.clone();
        let text = text.to_string();
        let deadline = Instant::now() + Duration::from_secs(120);
        while !locked(&log).lines.iter().any(|line| line.contains(&text)) {
            assert!(Instant::now() < deadline, "timed out waiting for {text}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn cells(&self) -> usize {
        locked(&self.log).cells
    }

    /// Wait until this many cells are committed and no turn is running.
    async fn turn_done(&self, min_cells: usize) {
        let log = self.log.clone();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            {
                let log = locked(&log);
                if log.cells >= min_cells && !log.running {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the turn to finish"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn launch_in(folder: PathBuf, mode: StartMode) -> Launch {
    Launch {
        mode,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_string(),
        folder,
        record: None,
        bench_replay: None,
        no_ensure: true,
    }
}

async fn drive_live<F, Fut>(launch: Launch, script: F) -> (Outcome, Log)
where
    F: FnOnce(Driver) -> Fut,
    Fut: Future<Output = ()>,
{
    let log = Log::default();
    let mut screen = Probe { log: log.clone() };
    let (inputs, receiver) = mpsc::unbounded_channel();
    // Mutation request ids start from the launch id, and Hermes refuses a reused one
    // that carries a different title.
    static LAUNCHES: AtomicU64 = AtomicU64::new(0);
    let number = LAUNCHES.fetch_add(1, Ordering::Relaxed) + 1;
    let setup = RunSetup {
        herdr: None,
        home: hermes_home(),
        launch_id: format!("live-picker-{}-{number}", run_suffix()),
        preopened: None,
        list_title: None,
        notices: Vec::new(),
        keys: BTreeMap::new(),
        fleet: None,
    };
    let driver = Driver {
        inputs,
        log: log.clone(),
    };
    let outcome = tokio::join!(run(launch, setup, &mut screen, receiver), async {
        // A command sent before the session opens finds no session.
        let deadline = Instant::now() + Duration::from_secs(30);
        while locked(&driver.log).session.is_none() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for the session to open"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        script(driver).await;
    })
    .0;
    (outcome, log)
}

/// Unique to this run: the test gateway keeps the titles of earlier runs.
fn run_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_millis()
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(future)
}

fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a project");
    let folder = std::fs::canonicalize(dir.path()).expect("canonical");
    (dir, folder)
}

#[test]
#[ignore]
fn live_picker_lists_healthy_gateway() {
    let _guard = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !begin() {
        return;
    }
    block_on(async {
        let mut times = Vec::new();
        for round in 1..=5 {
            let mut service = DirectoryService::new();
            let start = Instant::now();
            let scope = service
                .open(&hermes_home(), "fixture")
                .await
                .unwrap_or_else(|error| panic!("round {round}: no side connection: {error:?}"));
            let profiles = match scope {
                talaria_app::side_connection::SideScope::Host { profiles } => profiles.len(),
                talaria_app::side_connection::SideScope::ProfileOnly { .. } => 1,
            };
            let (tx, mut rx) = mpsc::channel(16);
            service.start_list(tx);
            for _ in 0..profiles * 2 {
                tokio::time::timeout(Duration::from_secs(30), rx.recv())
                    .await
                    .expect("the lists land")
                    .expect("the channel stays open");
            }
            let ms = start.elapsed().as_millis();
            println!("LIVE_PICKER_OPEN_MS round {round}: {ms}");
            times.push(ms);
            service.close().await;
        }
        times.sort_unstable();
        println!("picker_open_median_ms: {}", times[2]);
        assert!(
            times[2] <= 500,
            "the median open time is over 500 ms: {times:?}"
        );
    });
    passed();
}

#[test]
#[ignore]
fn live_picker_round_trip_and_taken_title() {
    let _guard = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !begin() {
        return;
    }
    block_on(async {
        let (_dir, folder) = project();
        let run = run_suffix();
        let title_b = format!("Live picker B {run}");
        let title_a2 = format!("Live picker A2 {run}");
        // Two sessions with titles.
        let rename_a = format!("/rename Live picker A {run}");
        let (_, log) = drive_live(
            launch_in(folder.clone(), StartMode::New),
            |driver| async move {
                driver.submit(&rename_a);
                driver.line("Renamed to").await;
                driver.quit();
            },
        )
        .await;
        let session_a = locked(&log).session.clone().expect("session A opens");
        let rename_b = format!("/rename {title_b}");
        let (_, log) = drive_live(
            launch_in(folder.clone(), StartMode::New),
            |driver| async move {
                driver.submit(&rename_b);
                driver.line("Renamed to").await;
                driver.quit();
            },
        )
        .await;
        let _ = locked(&log).session.clone().expect("session B opens");

        // A rename round trip keeps the title on the next switch.
        let rename_a2 = format!("/rename {title_a2}");
        let (_outcome, log) = drive_live(
            launch_in(folder.clone(), StartMode::Resume(Some(session_a.clone()))),
            |driver| async move {
                driver.submit(&rename_a2);
                driver.line("Renamed to").await;
                driver.quit();
            },
        )
        .await;
        let lines = locked(&log).lines.clone();
        assert!(
            lines.iter().any(|line| line.contains(&title_a2)),
            "the round trip keeps the title: {lines:?}"
        );

        // Another session's title is refused as a gateway error.
        let rename_taken = format!("/rename {title_b}");
        let (_outcome, log) = drive_live(
            launch_in(folder.clone(), StartMode::Resume(Some(session_a))),
            |driver| async move {
                driver.submit(&rename_taken);
                driver
                    .line("Another session may already use that title")
                    .await;
                driver.quit();
            },
        )
        .await;
        let lines = locked(&log).lines.clone();
        assert!(
            lines.iter().any(|line| line
                == "Hermes could not rename the session. Another session may already use that title; titles must be unique."),
            "the taken-title text: {lines:?}"
        );
    });
    passed();
}

#[test]
#[ignore]
fn live_picker_sibling_routes() {
    let _guard = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !begin() {
        return;
    }
    block_on(async {
        let (_dir, folder) = project();
        // A new session and its branch child land side by side.
        let (_outcome, log) = drive_live(
            launch_in(folder.clone(), StartMode::New),
            |driver| async move {
                // Each turn commits the prompt and the reply.
                let before = driver.cells();
                driver.submit("Reply with only the word alpha.");
                driver.turn_done(before + 2).await;
                let before = driver.cells();
                driver.submit("Reply with only the word beta.");
                driver.turn_done(before + 2).await;
                driver.submit("/branch");
                driver.line("Branched into").await;
                driver.quit();
            },
        )
        .await;
        let lines = locked(&log).lines.clone();
        assert!(
            lines.iter().any(|line| line.contains("Branched into")),
            "the branch lands: {lines:?}"
        );

        // Both rows list with their titles.
        let mut service = DirectoryService::new();
        service
            .open(&hermes_home(), "fixture")
            .await
            .unwrap_or_else(|error| panic!("no side connection: {error:?}"));
        let (tx, mut rx) = mpsc::channel(64);
        service.start_list(tx);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut titles = Vec::new();
        while titles.len() < 2 && Instant::now() < deadline {
            let message = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("lists land")
                .expect("the channel stays open");
            if let talaria_app::app::AppMessage::Picker(
                talaria_core::directory::state::PickerInput::Listed { rows, .. },
            ) = message
            {
                titles.extend(rows.iter().map(|row| row.title.clone()));
            }
        }
        assert!(titles.len() >= 2, "both siblings list: {titles:?}");
        service.close().await;
    });
    passed();
}

#[test]
#[ignore]
fn live_picker_branch_and_switch_back() {
    let _guard = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !begin() {
        return;
    }
    block_on(async {
        let (_dir, folder) = project();
        let title = format!("After the return {}", run_suffix());
        let rename = format!("/rename {title}");
        let (_outcome, log) = drive_live(
            launch_in(folder.clone(), StartMode::New),
            |driver| async move {
                let before = driver.cells();
                driver.submit("Reply with only the word alpha.");
                driver.turn_done(before + 2).await;
                driver.submit("/branch");
                driver.line("Branched into").await;
                // Switch back to the parent through the picker.
                driver.submit("/resume");
                tokio::time::sleep(Duration::from_secs(2)).await;
                driver.picker(talaria_core::directory::state::PickerInput::Action(
                    talaria_core::directory::state::PickerAction::Down,
                ));
                driver.picker(talaria_core::directory::state::PickerInput::Action(
                    talaria_core::directory::state::PickerAction::Open,
                ));
                driver.line("Now showing").await;
                driver.submit(&rename);
                driver.line("Renamed to").await;
                driver.quit();
            },
        )
        .await;
        let lines = locked(&log).lines.clone();
        assert!(
            lines.iter().any(|line| line.contains("Branched into")),
            "the branch lands: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("Now showing")),
            "the return switch lands: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains(&title)),
            "the adopted fences commit: {lines:?}"
        );
    });
    passed();
}

#[test]
#[ignore]
fn live_picker_export_formats() {
    let _guard = LIVE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !begin() {
        return;
    }
    block_on(async {
        let (_dir, folder) = project();
        let run = run_suffix();
        for (action, extension) in [
            (
                talaria_core::directory::state::PickerAction::FormatMarkdown,
                "md",
            ),
            (
                talaria_core::directory::state::PickerAction::FormatJson,
                "json",
            ),
            (
                talaria_core::directory::state::PickerAction::FormatHtml,
                "html",
            ),
        ] {
            // Each of the three sessions needs its own title: Hermes refuses a taken one.
            let rename = format!("/rename Live exportable {extension} {run}");
            let (_outcome, log) = drive_live(
                launch_in(folder.clone(), StartMode::New),
                |driver| async move {
                    driver.submit(&rename);
                    driver.line("Renamed to").await;
                    driver.submit("/resume");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    driver.picker(talaria_core::directory::state::PickerInput::Action(
                        talaria_core::directory::state::PickerAction::Export,
                    ));
                    driver.picker(talaria_core::directory::state::PickerInput::Action(action));
                    driver.picker(talaria_core::directory::state::PickerInput::Enter);
                    driver.line("Exported").await;
                    driver.quit();
                },
            )
            .await;
            let lines = locked(&log).lines.clone();
            assert!(
                lines.iter().any(|line| line.contains("Exported")),
                "the {extension} export lands: {lines:?}"
            );
        }
        let mut found = [0, 0, 0];
        for entry in std::fs::read_dir(&folder).expect("the project reads") {
            let path = entry.expect("an entry").path();
            let slot = match path.extension().and_then(|ext| ext.to_str()) {
                Some("md") => 0,
                Some("json") => 1,
                Some("html") => 2,
                _ => continue,
            };
            let content = std::fs::read_to_string(&path).expect("the export reads");
            if content.contains("Live exportable") {
                found[slot] += 1;
            }
        }
        assert_eq!(found, [1, 1, 1], "each format holds the title");
    });
    passed();
}
