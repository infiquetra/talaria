//! Driving the application loop for the directory tests (#231).

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use talaria_app::app::{
    AppEffect, ComposerDeadline, Outcome, RunSetup, Screen, StatusHost, UserAction, View, run,
    update,
};
use talaria_app::context::ContextBlock;
use talaria_app::launch::{Launch, StartMode};
use talaria_core::directory::state::PickerInput;
use talaria_core::session::Cell;
use tokio::sync::mpsc;

use super::mock_gateway::StandaloneGateway;

pub async fn within<T>(seconds: u64, what: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .unwrap_or_else(|_| panic!("timed out after {seconds} s: {what}"))
}

pub async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A short wait for loopback traffic: fast to fail in the red run.
pub async fn settle_short() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

#[derive(Default)]
pub struct ScreenLog {
    pub draws: usize,
    pub draw_at: Vec<Instant>,
    pub lines: Vec<String>,
    pub cells: Vec<String>,
    pub drafts: Vec<String>,
    pub session: Option<String>,
    pub restored: bool,
}

pub type Log = Arc<Mutex<ScreenLog>>;

pub fn locked(log: &Log) -> MutexGuard<'_, ScreenLog> {
    log.lock().expect("the log is open")
}

pub fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::User(cell) => format!("user: {}", cell.text),
        Cell::Assistant(cell) => format!("assistant: {}", cell.text),
        Cell::Reasoning(cell) => format!("reasoning: {}", cell.text),
        Cell::Tool(cell) => format!("tool: {}", cell.name),
        Cell::Notice(cell) => format!("notice: {}", cell.text),
        Cell::Prompt(cell) => format!("prompt: {}", cell.command.clone().unwrap_or_default()),
        Cell::Shell(cell) => format!("shell: {}", cell.command),
        Cell::Side(cell) => format!("side: {}", cell.question),
        Cell::HermesCommand(cell) => format!("command: {}", cell.name),
    }
}

pub struct Probe {
    pub log: Log,
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
        log.draws += 1;
        log.draw_at.push(Instant::now());
        log.session = view.session_id.map(str::to_string);
    }

    fn commit_lines(&mut self, lines: Vec<String>) {
        locked(&self.log).lines.extend(lines);
    }

    fn commit_cells(&mut self, cells: &[Cell]) {
        locked(&self.log).cells.extend(cells.iter().map(cell_text));
    }

    fn load_draft(&mut self, text: &str) {
        locked(&self.log).drafts.push(text.to_owned());
    }

    fn restore(&mut self) {
        locked(&self.log).restored = true;
    }
}

/// What a test does while `run` is going.
pub struct Driver {
    inputs: mpsc::UnboundedSender<UserAction>,
    log: Log,
}

impl Driver {
    pub fn send(&self, action: UserAction) {
        self.inputs.send(action).expect("the loop is running");
    }

    pub fn submit(&self, text: &str) {
        self.send(UserAction::Submit(text.to_string()));
    }

    pub fn picker(&self, input: PickerInput) {
        self.send(UserAction::Picker(input));
    }

    pub fn quit(&self) {
        self.send(UserAction::Quit);
    }

    /// Wait until the loop has drawn an opened session.
    pub async fn opened(&self) {
        let log = self.log.clone();
        eventually("the session to open", move || {
            locked(&log).session.is_some()
        })
        .await;
    }

    /// Wait until a committed line contains this text.
    pub async fn line(&self, text: &str) {
        let log = self.log.clone();
        let text = text.to_string();
        eventually(&format!("the line {text}"), move || {
            locked(&log).lines.iter().any(|line| line.contains(&text))
        })
        .await;
    }

    /// Draws so far.
    pub fn draws(&self) -> usize {
        locked(&self.log).draws
    }

    /// Committed lines and cells so far.
    pub fn counts(&self) -> (usize, usize) {
        let log = locked(&self.log);
        (log.lines.len(), log.cells.len())
    }

    /// Wait until the loop has given the terminal back, which is the last thing it does.
    pub async fn finished(&self) {
        let log = self.log.clone();
        eventually("the loop to finish", move || locked(&log).restored).await;
    }
}

/// A gateway on a loopback port with its own home, and a folder to launch in.
pub struct Rig {
    pub gateway: StandaloneGateway,
    pub folder: PathBuf,
    _home: tempfile::TempDir,
    _folder: tempfile::TempDir,
}

impl Rig {
    pub fn start() -> Self {
        let home = tempfile::tempdir().expect("home");
        let folder = tempfile::tempdir().expect("folder");
        let gateway = StandaloneGateway::start(home.path());
        let canonical = std::fs::canonicalize(folder.path()).expect("canonical folder");
        Self {
            gateway,
            folder: canonical,
            _home: home,
            _folder: folder,
        }
    }

    pub fn launch(&self, mode: StartMode) -> Launch {
        Launch {
            mode,
            model: None,
            provider: None,
            effort: None,
            profile: "fixture".to_string(),
            folder: self.folder.clone(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        }
    }

    pub fn setup(&self) -> RunSetup {
        RunSetup {
            home: self.gateway.home().to_path_buf(),
            launch_id: "launch-1".to_string(),
            preopened: None,
            list_title: None,
            notices: Vec::new(),
            keys: BTreeMap::new(),
            fleet: None,
        }
    }

    /// Run the loop for this launch while the script drives it.
    pub async fn drive<F, Fut>(&self, mode: StartMode, script: F) -> (Outcome, Log)
    where
        F: FnOnce(Driver) -> Fut,
        Fut: Future<Output = ()>,
    {
        self.drive_launch(self.launch(mode), script).await
    }

    pub async fn drive_launch<F, Fut>(&self, launch: Launch, script: F) -> (Outcome, Log)
    where
        F: FnOnce(Driver) -> Fut,
        Fut: Future<Output = ()>,
    {
        let log = Log::default();
        let mut screen = Probe { log: log.clone() };
        let (inputs, receiver) = mpsc::unbounded_channel();
        let driver = Driver {
            inputs,
            log: log.clone(),
        };
        let outcome = tokio::join!(run(launch, self.setup(), &mut screen, receiver), async {
            script(driver).await;
        })
        .0;
        (outcome, log)
    }
}

pub fn snapshot_of(value: &Value) -> hermes_contract::schemas::CanonicalSnapshot {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("the snapshot does not decode: {error}"))
}

/// The recorded resume snapshot with no open prompt and this folder, as the mock answers
/// it by default.
pub fn resume_snapshot(cwd: &str) -> Value {
    let mut snapshot = super::mock_gateway::recorded_resume_snapshot();
    snapshot["prompts"] = json!([]);
    snapshot["info"]["cwd"] = json!(cwd);
    snapshot
}

pub fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

pub fn committed_text(log: &Log) -> Vec<String> {
    locked(log).lines.clone()
}

pub fn state_with_opened(
    state: &mut talaria_app::app::AppState,
    opened: talaria_app::sessions::OpenedSession,
) -> Vec<AppEffect> {
    update(
        state,
        talaria_app::app::AppMessage::Opened {
            opened,
            notice: None,
        },
    )
}
