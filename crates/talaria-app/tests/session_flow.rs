//! The session flows of the first usable binary, driven against the in-process Hermes mock.
//!
//! Three levels are used. The session service is called with a client attached to the mock
//! (`MockGateway::start`). The pure `update` function is fed messages with no network. And the
//! whole application loop, `run`, is driven with a screen that only records what it is told, on
//! a link that reaches the mock through its control socket and loopback port
//! (`StandaloneGateway`). Every test uses real time and real sockets; a call that could hang is
//! wrapped in `within`, so a wrong implementation fails instead of waiting.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use proptest::prelude::*;

use hermes_client::InboundStream;
use hermes_client::liveness::{LinkEvent, LinkState};
use hermes_client::replay::Cursor;
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use mock_gateway::{Behavior, MockGateway, Reply, StandaloneGateway};
use serde_json::{Value, json};
use talaria_app::app::{
    AppEffect, AppMessage, AppState, ComposerDeadline, HELD_FULL_NOTICE, Outcome, REFRESHED_NOTICE,
    RunSetup, SESSION_GONE_NOTICE, STILL_WORKING, Screen, Slash, StatusHost, UserAction, View,
    apply_inbound, drive_composer, drive_status, folder_notice, hold_prompt, interrupt,
    link_notice, list_sessions, note_connection, other_folder_notice, quit, quit_while_opening,
    resume_history, run, slash_line, startup_notices, update,
};
use talaria_app::connection::{ConnectionService, ensure_invocations};
use talaria_app::failure::{
    create_lost_twice, folder_unknown, nothing_to_continue, resume_not_found, voice_stop_notice,
};
use talaria_app::launch::{
    Choice, Launch, ListRow, NewSession, SessionRequest, StartMode, format_rows, merge_rows,
    parse_choice,
};
use talaria_app::sessions::{
    DetachOutcome, ExecuteOutcome, OpenError, OpenedSession, SessionService, accept_event,
    detach_params,
};
use talaria_core::session::{
    Cell, Effect, Event, NoticeKind, PromptKind, SessionId, SessionState, TurnState,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

const LAUNCH_ID: &str = "0123456789abcdef";
const REQUEST_ID: &str = "0123456789abcdef0123456789abcdef";

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

fn new_request() -> SessionRequest {
    SessionRequest::New(NewSession {
        request_id: REQUEST_ID.to_string(),
        source: "tui".to_string(),
        cwd: PathBuf::from("/work/proj"),
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        reasoning: Some("high".to_string()),
    })
}

fn latest(workspace: &str) -> SessionRequest {
    SessionRequest::Latest {
        workspace: PathBuf::from(workspace),
    }
}

fn by_id(session_id: &str) -> SessionRequest {
    SessionRequest::ById {
        session_id: session_id.to_string(),
    }
}

/// Wait for a future for this many seconds of real time, then fail the test.
async fn within<T>(seconds: u64, what: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .unwrap_or_else(|_| panic!("timed out after {seconds} s: {what}"))
}

/// Poll a condition every 10 ms for up to 20 seconds.
async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Let the link task apply a command the loop has just sent it.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn snapshot_of(value: &Value) -> CanonicalSnapshot {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("the snapshot does not decode: {error}"))
}

fn user_lines(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::User(cell) => format!("user: {}", cell.text),
        Cell::Assistant(cell) => format!("assistant: {}", cell.text),
        Cell::Reasoning(cell) => format!("reasoning: {}", cell.text),
        Cell::Tool(cell) => format!("tool: {}", cell.name),
        Cell::Notice(cell) => format!("notice: {}", cell.text),
        Cell::Prompt(cell) => format!("prompt: {}", cell.command.clone().unwrap_or_default()),
        Cell::Shell(cell) => format!("shell: {}", cell.command),
        Cell::Side(cell) => format!("side: {}", cell.question),
    }
}

/// The recorded resume snapshot with no open prompt and this folder, as the mock answers it by
/// default.
fn resume_snapshot(cwd: &str) -> Value {
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["prompts"] = json!([]);
    snapshot["info"]["cwd"] = json!(cwd);
    snapshot
}

fn with_messages(mut snapshot: Value, messages: Vec<Value>) -> Value {
    snapshot["message_count"] = json!(messages.len());
    snapshot["messages"] = json!(messages.clone());
    snapshot["display_messages"] = json!(messages);
    snapshot
}

/// The open approval snapshot of this crate's test data, with a turn running.
fn approval_while_running(cwd: &str) -> Value {
    let mut snapshot = mock_gateway::data_snapshot("open_approval.json");
    snapshot["running"] = json!(true);
    snapshot["info"]["cwd"] = json!(cwd);
    snapshot
}

fn running_turn(cwd: &str) -> Value {
    let mut snapshot = mock_gateway::data_snapshot("running_turn.json");
    snapshot["info"]["cwd"] = json!(cwd);
    snapshot
}

/// The mock, and a session service calling through a client attached to it.
struct InProcess {
    mock: MockGateway,
    service: SessionService,
    _inbound: InboundStream,
}

async fn in_process() -> InProcess {
    let (mock, client, inbound) = MockGateway::start().await;
    InProcess {
        mock,
        service: SessionService::with_client(LAUNCH_ID, client),
        _inbound: inbound,
    }
}

impl InProcess {
    async fn open(&self, request: SessionRequest) -> Result<OpenedSession, OpenError> {
        within(15, "the session call", self.service.open(request)).await
    }

    /// Resume this snapshot by its id.
    async fn open_snapshot(&self, snapshot: Value) -> OpenedSession {
        let id = snapshot["session_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.mock.set_resume_snapshot(snapshot);
        self.open(by_id(&id)).await.expect("the snapshot opens")
    }
}

fn opened_effects(state: &mut AppState, opened: OpenedSession) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    )
}

/// Every line the effects commit, in order.
fn committed_lines(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The `(text, submission_id)` of every prompt the effects send.
fn sent_prompts(effects: &[AppEffect]) -> Vec<(String, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Send(Effect::SendPrompt {
                text,
                submission_id,
                ..
            }) => Some((text.clone(), submission_id.clone())),
            _ => None,
        })
        .collect()
}

fn submit(state: &mut AppState, text: &str) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::User(UserAction::Submit(text.to_string())),
    )
}

// ---------------------------------------------------------------------------------------------
// The whole application loop on a recording screen
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct ScreenLog {
    lines: Vec<String>,
    cells: Vec<String>,
    draws: usize,
    draws_after_restore: usize,
    session: Option<String>,
    restored: bool,
}

type Log = Arc<Mutex<ScreenLog>>;

fn locked(log: &Log) -> MutexGuard<'_, ScreenLog> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A screen that keeps what the loop commits and draws, and hands every input back as the
/// action it already is.
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

    fn input(&mut self, input: UserAction, _view: &View<'_>, _now: Instant) -> Vec<UserAction> {
        vec![input]
    }

    fn draw(&mut self, view: &View<'_>) {
        let mut log = locked(&self.log);
        log.draws += 1;
        if log.restored {
            log.draws_after_restore += 1;
        }
        log.session = view.session_id.map(str::to_string);
    }

    fn commit_lines(&mut self, lines: Vec<String>) {
        locked(&self.log).lines.extend(lines);
    }

    fn commit_context(&mut self, _block: &talaria_app::context::ContextBlock) {}

    fn commit_cells(&mut self, cells: &[Cell]) {
        locked(&self.log).cells.extend(cells.iter().map(cell_text));
    }

    fn load_draft(&mut self, _text: &str) {}

    fn restore(&mut self) {
        locked(&self.log).restored = true;
    }
}

/// What a test does while `run` is going.
struct Driver {
    inputs: mpsc::UnboundedSender<UserAction>,
    log: Log,
}

impl Driver {
    fn send(&self, action: UserAction) {
        self.inputs.send(action).expect("the loop is running");
    }

    fn submit(&self, text: &str) {
        self.send(UserAction::Submit(text.to_string()));
    }

    fn quit(&self) {
        self.send(UserAction::Quit);
    }

    /// Wait until the loop has drawn an opened session.
    async fn opened(&self) {
        let log = self.log.clone();
        eventually("the session to open", move || {
            locked(&log).session.is_some()
        })
        .await;
    }

    /// Wait until a committed line contains this text.
    async fn line(&self, text: &str) {
        let log = self.log.clone();
        let text = text.to_string();
        eventually(&format!("the line {text}"), move || {
            locked(&log).lines.iter().any(|line| line.contains(&text))
        })
        .await;
    }

    /// Wait until the loop has given the terminal back, which is the last thing it does.
    async fn finished(&self) {
        let log = self.log.clone();
        eventually("the loop to finish", move || locked(&log).restored).await;
    }
}

/// A gateway on a loopback port with its own home, and a folder to launch in.
struct Rig {
    gateway: StandaloneGateway,
    folder: PathBuf,
    _home: tempfile::TempDir,
    _folder: tempfile::TempDir,
}

impl Rig {
    fn start() -> Self {
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

    fn launch(&self, mode: StartMode) -> Launch {
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

    fn setup(&self) -> RunSetup {
        RunSetup {
            home: self.gateway.home().to_path_buf(),
            launch_id: LAUNCH_ID.to_string(),
            preopened: None,
            list_title: None,
            notices: Vec::new(),
        }
    }

    /// Run the loop for this launch while the script drives it.
    async fn drive<F, Fut>(&self, mode: StartMode, script: F) -> (Outcome, Log)
    where
        F: FnOnce(Driver) -> Fut,
        Fut: Future<Output = ()>,
    {
        self.drive_launch(self.launch(mode), script).await
    }

    async fn drive_launch<F, Fut>(&self, launch: Launch, script: F) -> (Outcome, Log)
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
        let running = within(
            90,
            "the loop",
            run(launch, self.setup(), &mut screen, receiver),
        );
        let (outcome, ()) = tokio::join!(running, script(driver));
        (outcome, log)
    }
}

// ---------------------------------------------------------------------------------------------
// Creating and continuing
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn new_session_creates_with_cwd_and_tracks_cursor() {
    let rig = in_process().await;
    let opened = rig.open(new_request()).await.expect("created");

    let frames = rig.mock.requests("session.create");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        sorted_keys(&frames[0].params),
        [
            "cwd",
            "model",
            "provider",
            "reasoning",
            "request_id",
            "source"
        ]
    );
    assert_eq!(frames[0].params["request_id"], REQUEST_ID);
    assert_eq!(frames[0].params["source"], "tui");
    assert_eq!(frames[0].params["cwd"], "/work/proj");
    assert_eq!(frames[0].params["model"], "minimax-m3.1-flash");
    assert_eq!(frames[0].params["provider"], "ollama-cloud");
    assert_eq!(frames[0].params["reasoning"], "high");
    rig.mock.assert_no_violations();

    let recorded = mock_gateway::recorded_create_snapshot();
    assert_eq!(opened.session_id, "local-session-1");
    assert_eq!(
        opened.stored_session_id,
        recorded["stored_session_id"].as_str().unwrap_or_default()
    );
    assert!(!opened.subscription_id.is_empty());
    assert_eq!(opened.info.cwd.as_deref(), Some("/work/proj"));
    assert_eq!(opened.info.model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(opened.info.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(
        opened.cursor.replay_epoch,
        recorded["replay_epoch"].as_str().unwrap_or_default()
    );
    assert_eq!(
        Some(opened.cursor.last_sequence),
        recorded["last_sequence"].as_u64()
    );
    let expected = SessionState::from_snapshot(snapshot_of(&recorded));
    assert_eq!(opened.state.cells(), expected.cells());

    // The loop hands that cursor to the link when the session arrives.
    let mut state = AppState::new(LAUNCH_ID);
    let effects = opened_effects(&mut state, opened.clone());
    let tracked: Vec<(String, Cursor)> = effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Track { session_id, cursor } => Some((session_id.clone(), cursor.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(tracked.len(), 1);
    assert_eq!(tracked[0].0, "local-session-1");
    assert_eq!(tracked[0].1.replay_epoch, opened.cursor.replay_epoch);
    assert_eq!(tracked[0].1.last_sequence, opened.cursor.last_sequence);
}

// Slow: the link redials on its back-off ladder, about a second for each lost reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_retry_after_unknown_reuses_request_id() {
    // One lost reply: the session was made, the socket dropped, and the retry finds the same one.
    let rig = Rig::start();
    let connection = within(
        30,
        "connecting",
        ConnectionService::open(&rig.launch(StartMode::New), rig.gateway.home(), None),
    )
    .await
    .unwrap_or_else(|failure| panic!("no connection: {}", failure.render()));
    let service = SessionService::with_client(LAUNCH_ID, connection.link.client());
    rig.gateway
        .queue_behavior("session.create", Behavior::DropBeforeReply);
    let opened = within(60, "the create and its retry", service.open(new_request()))
        .await
        .expect("the retry is answered");
    let frames = rig.gateway.requests("session.create");
    assert_eq!(frames.len(), 2, "one frame and one retry");
    assert_eq!(frames[0].params, frames[1].params);
    assert_eq!(frames[0].params["request_id"], REQUEST_ID);
    assert_ne!(frames[0].connection, frames[1].connection);
    assert_eq!(
        rig.gateway.created_sessions().len(),
        1,
        "one session, not two"
    );
    assert_eq!(opened.session_id, rig.gateway.created_sessions()[0]);
    rig.gateway.assert_no_violations();

    // Two lost replies: the launch fails and the text says an empty session may remain.
    let rig = Rig::start();
    let connection = within(
        30,
        "connecting",
        ConnectionService::open(&rig.launch(StartMode::New), rig.gateway.home(), None),
    )
    .await
    .unwrap_or_else(|failure| panic!("no connection: {}", failure.render()));
    let service = SessionService::with_client(LAUNCH_ID, connection.link.client());
    rig.gateway
        .queue_behavior("session.create", Behavior::DropBeforeReply);
    rig.gateway
        .queue_behavior("session.create", Behavior::DropBeforeReply);
    let error = within(60, "the create and its retry", service.open(new_request()))
        .await
        .expect_err("a second loss fails");
    assert_eq!(error, OpenError::Unknown);
    assert_eq!(
        rig.gateway.requests("session.create").len(),
        2,
        "no third try"
    );
    let failure = create_lost_twice();
    assert_eq!(failure.exit_code, 1);
    assert_eq!(
        failure.render(),
        "talaria: the connection dropped while the session was being created; Hermes may have \
         created an empty session.\nNext step: Run the command again.\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continue_sends_latest_with_workspace() {
    let rig = in_process().await;
    let opened = rig
        .open(latest("/work/proj"))
        .await
        .expect("the session of this folder resumes");
    let frames = rig.mock.requests("session.resume");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        sorted_keys(&frames[0].params),
        ["display_projection", "latest", "workspace"]
    );
    assert_eq!(frames[0].params["latest"], "tui");
    assert_eq!(frames[0].params["workspace"], "/work/proj");
    assert_eq!(frames[0].params["display_projection"], true);
    rig.mock.assert_no_violations();
    assert_eq!(opened.info.cwd.as_deref(), Some("/work/proj"));
    let expected = SessionState::from_snapshot(snapshot_of(&resume_snapshot("/work/proj")));
    assert!(!expected.cells().is_empty());
    assert_eq!(opened.state.cells(), expected.cells());
    assert!(
        rig.mock.detaches().is_empty(),
        "a session of this folder is kept"
    );

    // From a subfolder of a repository the workspace is the repository's top level.
    let rig = Rig::start();
    let top = rig.folder.clone();
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&top)
        .status()
        .expect("git runs");
    assert!(status.success(), "git init failed");
    let sub = top.join("sub");
    std::fs::create_dir(&sub).expect("subfolder");
    rig.gateway.set_resume_cwd(Some(&sub.display().to_string()));
    let mut launch = rig.launch(StartMode::Continue);
    launch.folder = std::fs::canonicalize(&sub).expect("canonical subfolder");
    let (outcome, _log) = rig
        .drive_launch(launch, |driver| async move {
            driver.opened().await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    let frames = rig.gateway.requests("session.resume");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].params["workspace"], top.display().to_string());
    rig.gateway.assert_no_violations();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continue_refuses_other_or_unknown_folder_and_detaches() {
    // The service: a session of another folder, then one with no folder, each detached again.
    let rig = in_process().await;
    rig.mock.set_resume_cwd(Some("/work/other"));
    let error = rig
        .open(latest("/work/proj"))
        .await
        .expect_err("outside the workspace");
    assert_eq!(
        error,
        OpenError::NotInFolder {
            session_cwd: "/work/other".to_string()
        }
    );
    rig.mock.set_resume_cwd(None);
    let error = rig
        .open(latest("/work/proj"))
        .await
        .expect_err("no folder is reported");
    assert_eq!(error, OpenError::FolderUnknown);

    let detaches = rig.mock.detaches();
    let attaches = rig.mock.attaches();
    assert_eq!(detaches.len(), 2, "each refused session is detached");
    assert_eq!(attaches.len(), 2);
    for (detach, attach) in detaches.iter().zip(&attaches) {
        assert_eq!(detach.session_id, "local-session-1");
        assert_eq!(detach.subscription_id, attach.subscription_id);
        assert!(!detach.had_end_reason);
        assert!(detach.detached);
    }
    for frame in rig.mock.requests("session.detach") {
        assert_eq!(
            sorted_keys(&frame.params),
            ["session_id", "subscription_id"]
        );
    }
    rig.mock.assert_no_violations();
    assert!(
        detach_params("local-session-1", "sub-1")
            .get("end_reason")
            .is_none()
    );

    // The launch: exit 1, the folder text, and nothing about the other session in any output.
    for (cwd, failure) in [
        (Some("/work/other"), nothing_to_continue()),
        (None, folder_unknown()),
    ] {
        let rig = Rig::start();
        rig.gateway.set_resume_cwd(cwd);
        let (outcome, log) = rig
            .drive(StartMode::Continue, |driver| async move {
                driver.finished().await;
            })
            .await;
        assert_eq!(outcome.code, 1);
        assert_eq!(outcome.stderr, failure.render());
        assert!(outcome.stdout.is_empty(), "{}", outcome.stdout);
        for text in ["/work/other", "local-session-1", "stored-1"] {
            assert!(!outcome.stderr.contains(text), "{}", outcome.stderr);
            assert!(!locked(&log).lines.iter().any(|line| line.contains(text)));
        }
        assert!(
            locked(&log).cells.is_empty(),
            "no history of the other session"
        );
        assert_eq!(
            rig.gateway.detaches().len(),
            1,
            "the refused session is detached"
        );
        assert!(!rig.gateway.detaches()[0].had_end_reason);
    }
}

#[test]
fn not_found_texts_are_plain() {
    let missing = nothing_to_continue();
    assert_eq!(missing.exit_code, 1);
    assert_eq!(
        missing.render(),
        "talaria: no earlier session found for this folder.\nNext step: Start a new one with \
         talaria, or choose one with talaria --resume.\n"
    );
    let unknown = resume_not_found("local-session-1", "fixture");
    assert_eq!(unknown.exit_code, 1);
    assert_eq!(
        unknown.render(),
        "talaria: no session with the id local-session-1 in the profile fixture.\nNext step: \
         Session ids belong to one profile. Check --profile, or list sessions with talaria \
         --resume.\n"
    );
    assert_eq!(
        folder_unknown().render(),
        "talaria: the most recent session cannot be checked against this folder.\nNext step: \
         Choose a session with talaria --resume, or start a new one with talaria.\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn not_found_is_reported_with_the_right_text_for_each_form() {
    let rig = Rig::start();
    rig.gateway.set_latest_missing(true);
    let (outcome, _log) = rig
        .drive(StartMode::Continue, |driver| async move {
            driver.finished().await;
        })
        .await;
    assert_eq!(outcome.code, 1);
    assert_eq!(outcome.stderr, nothing_to_continue().render());

    let rig = Rig::start();
    let (outcome, _log) = rig
        .drive(
            StartMode::Resume(Some("no-such-session".to_string())),
            |driver| async move {
                driver.finished().await;
            },
        )
        .await;
    assert_eq!(outcome.code, 1);
    assert_eq!(
        outcome.stderr,
        resume_not_found("no-such-session", "fixture").render()
    );
}

// ---------------------------------------------------------------------------------------------
// Resuming by id
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_by_id_notice_when_outside_workspace() {
    assert_eq!(
        other_folder_notice("/work/other"),
        "This session works in /work/other, not in the current folder."
    );
    let workspace = Path::new("/work/proj");
    assert_eq!(
        folder_notice(Some("/work/other"), workspace).as_deref(),
        Some("This session works in /work/other, not in the current folder.")
    );
    assert_eq!(folder_notice(Some("/work/proj"), workspace), None);
    assert_eq!(folder_notice(Some("/work/proj/sub"), workspace), None);
    assert!(folder_notice(Some("/work/proj2"), workspace).is_some());
    assert_eq!(folder_notice(None, workspace), None);

    // In the loop the notice is committed once, only for a session outside the launch folder.
    for case in ["outside", "inside", "unknown"] {
        let rig = Rig::start();
        let cwd = match case {
            "outside" => Some("/work/other".to_string()),
            "inside" => Some(rig.folder.join("sub").display().to_string()),
            _ => None,
        };
        rig.gateway.set_resume_cwd(cwd.as_deref());
        let (outcome, log) = rig
            .drive(
                StartMode::Resume(Some("local-session-1".to_string())),
                |driver| async move {
                    driver.opened().await;
                    settle().await;
                    driver.quit();
                },
            )
            .await;
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
        let notices = locked(&log)
            .lines
            .iter()
            .filter(|line| line.contains("works in"))
            .count();
        let expected = usize::from(case == "outside");
        assert_eq!(notices, expected, "{case}: {:?}", locked(&log).lines);
        if case == "outside" {
            assert!(locked(&log).lines.iter().any(|line| {
                line.contains("This session works in /work/other, not in the current folder.")
            }));
        }
    }
}

#[tokio::test]
async fn resume_by_id_builds_the_same_cells_as_replay() {
    let rig = in_process().await;
    let snapshot = resume_snapshot("/work/proj");
    let restored = SessionState::from_snapshot(snapshot_of(&snapshot));
    assert!(!user_lines(&restored).is_empty());
    rig.mock.set_resume_snapshot(snapshot);
    let opened = rig
        .open(by_id("local-session-1"))
        .await
        .expect("the session resumes");
    let frames = rig.mock.requests("session.resume");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        sorted_keys(&frames[0].params),
        ["display_projection", "session_id"]
    );
    assert_eq!(frames[0].params["session_id"], "local-session-1");
    assert_eq!(frames[0].params["display_projection"], true);
    rig.mock.assert_no_violations();
    assert_eq!(user_lines(&opened.state), user_lines(&restored));
    assert_eq!(opened.state.cells(), restored.cells());
}

#[tokio::test]
async fn resume_restores_open_approval_and_running_turn() {
    let rig = in_process().await;
    let opened = rig
        .open_snapshot(approval_while_running("/work/proj"))
        .await;
    let prompts: Vec<_> = opened.state.open_prompts().collect();
    assert_eq!(prompts.len(), 1, "one open approval");
    assert_eq!(prompts[0].kind, PromptKind::Approval);
    assert_eq!(prompts[0].command.as_deref(), Some("rm -rf .pytest_cache"));
    assert_eq!(prompts[0].choices, ["once", "session", "always", "deny"]);
    assert_eq!(opened.state.turn(), &TurnState::Running);

    let rig = in_process().await;
    let opened = rig.open_snapshot(running_turn("/work/proj")).await;
    assert_eq!(opened.state.turn(), &TurnState::Running);
    assert_eq!(opened.state.open_prompts().count(), 0);

    // A queued admission alone makes no card.
    let rig = in_process().await;
    let mut snapshot = running_turn("/work/proj");
    snapshot["pending"] = json!([{
        "admission_id": "adm-1",
        "authority_epoch": 1,
        "execution_generation": null,
        "input_id": "input-1",
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": "local-session-1"},
        "sequence": 1,
        "status": "queued",
        "text": "queued sentence that is not a question",
    }]);
    let opened = rig.open_snapshot(snapshot).await;
    assert_eq!(opened.state.open_prompts().count(), 0);
}

#[tokio::test]
async fn resume_history_is_capped_with_a_marker() {
    let messages: Vec<Value> = (0..120)
        .map(|index| json!({"role": "user", "text": format!("line {index}")}))
        .collect();
    let mut snapshot = with_messages(running_turn("/work/proj"), messages);
    snapshot["running"] = json!(false);
    let state = SessionState::from_snapshot(snapshot_of(&snapshot));
    let lines = resume_history(state.cells(), "untitled session");
    assert_eq!(
        lines[0],
        "\u{2500}\u{2500} Resumed: untitled session \u{2500}\u{2500}"
    );
    assert_eq!(lines[1], "(70 earlier messages not shown)");
    assert_eq!(lines.len(), 52, "{lines:?}");
    assert!(lines[2].contains("line 70"), "{lines:?}");
    assert!(lines[51].contains("line 119"), "{lines:?}");

    // The same through the loop: the separator and marker first, then the last 50 cells.
    let rig = in_process().await;
    let opened = rig.open_snapshot(snapshot.clone()).await;
    let mut app = AppState::new(LAUNCH_ID);
    let effects = opened_effects(&mut app, opened);
    let committed = committed_lines(&effects);
    assert_eq!(
        committed,
        [
            "\u{2500}\u{2500} Resumed: line 0 \u{2500}\u{2500}".to_string(),
            "(70 earlier messages not shown)".to_string()
        ]
    );
    let ranges: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitCells(range) => Some(range.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ranges.len(), 1);
    assert_eq!((ranges[0].start, ranges[0].end), (70, 120));
    assert!(matches!(effects[0], AppEffect::CommitLines(_)));
    assert!(matches!(effects[1], AppEffect::CommitCells(_)));
    assert_eq!(app.committed_cells, 120);
}

#[tokio::test]
async fn resume_separator_title_comes_from_the_session_then_the_list_then_the_first_prompt() {
    let rig = in_process().await;
    let long = format!("{}\nsecond line", "x".repeat(80));
    let separator = |title: &str| format!("\u{2500}\u{2500} Resumed: {title} \u{2500}\u{2500}");

    // The session's own title wins.
    let mut snapshot = mock_gateway::data_snapshot("open_approval.json");
    snapshot["info"]["cwd"] = json!("/work/proj");
    let opened = rig.open_snapshot(snapshot).await;
    let mut app = AppState::new(LAUNCH_ID);
    app.list_title = Some("From the list".to_string());
    let lines = committed_lines(&opened_effects(&mut app, opened));
    assert_eq!(lines[0], separator("Cache"));

    // Then the picked list row's title.
    let messages = vec![json!({"role": "user", "text": long})];
    let mut snapshot = with_messages(running_turn("/work/proj"), messages);
    snapshot["running"] = json!(false);
    let opened = rig.open_snapshot(snapshot.clone()).await;
    let mut app = AppState::new(LAUNCH_ID);
    app.list_title = Some("From the list".to_string());
    let lines = committed_lines(&opened_effects(&mut app, opened));
    assert_eq!(lines[0], separator("From the list"));

    // Then the first line of the first user message, cut at 60 characters with no ellipsis.
    let opened = rig.open_snapshot(snapshot).await;
    let mut app = AppState::new(LAUNCH_ID);
    let lines = committed_lines(&opened_effects(&mut app, opened));
    assert_eq!(lines[0], separator(&"x".repeat(60)));

    // Then a fixed name.
    let messages = vec![json!({"role": "assistant", "text": "hello"})];
    let mut snapshot = with_messages(running_turn("/work/proj"), messages);
    snapshot["running"] = json!(false);
    let opened = rig.open_snapshot(snapshot).await;
    let mut app = AppState::new(LAUNCH_ID);
    let lines = committed_lines(&opened_effects(&mut app, opened));
    assert_eq!(lines[0], separator("untitled session"));

    // A new session with no cells commits nothing.
    let created = in_process().await;
    let opened = created.open(new_request()).await.expect("created");
    let mut app = AppState::new(LAUNCH_ID);
    let effects = opened_effects(&mut app, opened);
    assert!(committed_lines(&effects).is_empty());
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::CommitCells(_)))
    );
}

proptest::proptest! {
    #![proptest_config(proptest_config())]
    #[test]
    fn events_at_or_below_snapshot_sequence_are_dropped(seq in 0u64..40u64, last in 0u64..40u64) {
        let cursor = Cursor {
            replay_epoch: "epoch-1".to_string(),
            last_sequence: last,
        };
        let keep = accept_event(&cursor, Some("epoch-1"), Some(seq));
        if seq <= last {
            prop_assert!(!keep);
        } else {
            prop_assert!(keep);
        }
        prop_assert!(accept_event(&cursor, None, Some(seq)));
        prop_assert!(accept_event(&cursor, Some("epoch-1"), None));
        prop_assert!(accept_event(&cursor, Some("epoch-2"), Some(0)));
    }
}

fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 8,
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}

// ---------------------------------------------------------------------------------------------
// The resume list
// ---------------------------------------------------------------------------------------------

fn row(id: &str, title: Option<&str>, started: Option<f64>, running: bool) -> ListRow {
    ListRow {
        session_id: id.to_string(),
        title: title.map(str::to_string),
        started_at: started,
        model: None,
        message_count: 2,
        running,
    }
}

#[test]
fn resume_list_merges_live_and_stored() {
    let live = ListRow {
        session_id: "same".to_string(),
        title: Some("Live".to_string()),
        started_at: Some(50.0),
        model: Some("minimax-m3.1-flash".to_string()),
        message_count: 2,
        running: true,
    };
    let stored = ListRow {
        session_id: "same".to_string(),
        title: Some("Stored".to_string()),
        started_at: Some(10.0),
        model: None,
        message_count: 9,
        running: false,
    };
    let merged = merge_rows(vec![live], vec![stored]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].title.as_deref(), Some("Live"));
    assert!(merged[0].running);
    assert_eq!(parse_choice("", merged.len()), Choice::Cancel);
    assert!(matches!(parse_choice("9", 1), Choice::OutOfRange(9)));

    // Newest first, a missing time last, at most twenty.
    let many: Vec<ListRow> = (0..30)
        .map(|index| row(&format!("s{index}"), None, Some(f64::from(index)), false))
        .collect();
    let merged = merge_rows(many, vec![row("none", None, None, false)]);
    assert_eq!(merged.len(), 20);
    assert_eq!(merged[0].session_id, "s29");
    assert_eq!(merged[19].session_id, "s10");
    let merged = merge_rows(
        vec![
            row("a", None, None, false),
            row("b", None, Some(1.0), false),
        ],
        Vec::new(),
    );
    assert_eq!(merged[0].session_id, "b");

    // Ages, placeholders and the working mark.
    let now = 1_000_000.0;
    let rows = vec![
        row("r-now", Some("Fresh"), Some(now - 30.0), true),
        row("r-minutes", None, Some(now - 300.0), false),
        row("r-hours", None, Some(now - 3.0 * 3600.0), false),
        row("r-days", None, Some(now - 2.0 * 86400.0), false),
        row("r-none", None, None, false),
        row("r-future", None, Some(now + 500.0), false),
    ];
    let text = format_rows(&rows, now);
    let line = |id: &str| {
        text.lines()
            .find(|line| line.ends_with(id))
            .unwrap_or_else(|| panic!("no row for {id} in {text}"))
            .to_string()
    };
    assert!(line("r-now").contains("just now"));
    assert!(line("r-now").contains("Fresh"));
    assert!(line("r-now").contains("working"));
    assert!(line("r-minutes").contains("5m ago"));
    assert!(line("r-minutes").contains("(untitled)"));
    assert!(!line("r-minutes").contains("working"));
    assert!(line("r-hours").contains("3h ago"));
    assert!(line("r-days").contains("2d ago"));
    assert!(line("r-none").contains("  -  "));
    assert!(line("r-future").contains("just now"));
    assert_eq!(text.lines().count(), rows.len());
    assert_eq!(parse_choice("2", 6), Choice::Row(2));
    assert_eq!(parse_choice("0", 6), Choice::OutOfRange(0));
    assert_eq!(
        parse_choice("abc-123", 6),
        Choice::Id("abc-123".to_string())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_list_reads_both_scopes_and_survives_a_failed_stored_call() {
    let rig = Rig::start();
    rig.gateway.set_list_rows(
        "live",
        vec![mock_gateway::list_row(
            "same",
            "Live",
            Some(50.0),
            Some("m"),
            2,
            true,
        )],
    );
    rig.gateway.set_list_rows(
        "stored",
        vec![
            mock_gateway::list_row("same", "Stored", Some(10.0), None, 9, false),
            mock_gateway::list_row("older", "Older", Some(5.0), None, 1, false),
        ],
    );
    let connection = within(
        30,
        "connecting",
        ConnectionService::open(
            &rig.launch(StartMode::Resume(None)),
            rig.gateway.home(),
            None,
        ),
    )
    .await
    .unwrap_or_else(|failure| panic!("no connection: {}", failure.render()));
    let rows = within(15, "the list", list_sessions(&connection, LAUNCH_ID))
        .await
        .unwrap_or_else(|failure| panic!("no list: {}", failure.render()));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].session_id, "same");
    assert_eq!(rows[0].title.as_deref(), Some("Live"));
    assert!(rows[0].running);
    assert_eq!(rows[1].session_id, "older");
    let frames = rig.gateway.requests("session.list");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].params, json!({"scope": "live", "limit": 20}));
    assert_eq!(frames[1].params, json!({"scope": "stored", "limit": 20}));
    rig.gateway.assert_no_violations();

    // The stored call fails: the live rows are used.
    rig.gateway
        .set_list_error("stored", Some("permission_denied"));
    let rows = within(15, "the list", list_sessions(&connection, LAUNCH_ID))
        .await
        .unwrap_or_else(|failure| panic!("no list: {}", failure.render()));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id, "same");

    // The live call fails: the launch fails.
    rig.gateway.set_list_error("live", Some("runtime_draining"));
    let failure = within(15, "the list", list_sessions(&connection, LAUNCH_ID))
        .await
        .expect_err("the live call failed");
    assert_eq!(failure.exit_code, 1);
}

// ---------------------------------------------------------------------------------------------
// Quitting
// ---------------------------------------------------------------------------------------------

const HINT: &str = "Resume this session with:\n  talaria --resume stored-1 --profile fixture\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quit_detaches_without_end_reason() {
    // With no session there is nothing to detach and nothing to resume: no hint, exit 0.
    let mut empty = AppState::new(LAUNCH_ID);
    assert_eq!(
        quit(&mut empty).await,
        (DetachOutcome::Skipped, 0, String::new())
    );
    assert!(empty.quitting);

    // A session that has a prompt but no connection to detach through: the hint still names
    // the stored id, and the profile when it is not the default.
    let rig = in_process().await;
    let opened = rig.open_snapshot(resume_snapshot("/work/proj")).await;
    let mut state = AppState::new(LAUNCH_ID);
    state.profile = "fixture".to_string();
    opened_effects(&mut state, opened.clone());
    let (_detached, code, hint) = quit(&mut state).await;
    assert_eq!(code, 0);
    assert_eq!(hint, HINT);

    // After the session is gone: exit 1 and no hint. After a failed link: exit 1 and the hint.
    let mut gone = AppState::new(LAUNCH_ID);
    gone.profile = "fixture".to_string();
    opened_effects(&mut gone, opened.clone());
    gone.session_gone = true;
    assert_eq!(
        quit(&mut gone).await,
        (DetachOutcome::Skipped, 1, String::new())
    );
    let mut failed = AppState::new(LAUNCH_ID);
    failed.profile = "fixture".to_string();
    opened_effects(&mut failed, opened);
    failed.link_failed = true;
    assert_eq!(
        quit(&mut failed).await,
        (DetachOutcome::Skipped, 1, HINT.to_string())
    );

    // A session with a prompt: detach, then the exact hint, and exit 0.
    let rig = Rig::start();
    let gateway = &rig.gateway;
    let (outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("hello");
            eventually("the prompt to be admitted", || {
                gateway.submissions().len() == 1
            })
            .await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, HINT);
    assert!(outcome.stderr.is_empty(), "{}", outcome.stderr);
    let detaches = rig.gateway.detaches();
    assert_eq!(detaches.len(), 1);
    assert_eq!(detaches[0].session_id, "local-session-1");
    assert_eq!(
        detaches[0].subscription_id,
        rig.gateway.attaches()[0].subscription_id
    );
    assert!(!detaches[0].had_end_reason);
    assert!(detaches[0].detached);
    let frames = rig.gateway.requests("session.detach");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        sorted_keys(&frames[0].params),
        ["session_id", "subscription_id"]
    );
    rig.gateway.assert_no_violations();
    assert!(locked(&log).restored);
    assert!(locked(&log).draws > 0);
    assert_eq!(
        locked(&log).draws_after_restore,
        0,
        "nothing is drawn after the restore"
    );

    // A session nobody wrote to is detached but leaves no hint.
    let rig = Rig::start();
    let (outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, "");
    assert_eq!(rig.gateway.detaches().len(), 1);

    // A turn still running is said so above the hint.
    let rig = Rig::start();
    rig.gateway.set_resume_snapshot(running_turn("/work/proj"));
    let (outcome, _log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                driver.quit();
            },
        )
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, format!("{STILL_WORKING}\n{HINT}"));
    assert_eq!(rig.gateway.detaches().len(), 1);
    assert!(!rig.gateway.detaches()[0].had_end_reason);
}

// Slow: the link redials after the drop, about a second.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quit_after_session_gone_skips_the_detach_and_exits_1() {
    let rig = Rig::start();
    let gateway = &rig.gateway;
    let (outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("hello");
            eventually("the prompt to be admitted", || {
                gateway.submissions().len() == 1
            })
            .await;
            settle().await;
            // The re-attach after the reconnect finds no such session.
            gateway.queue_behavior("session.resume", Behavior::Refuse("not_found".to_string()));
            gateway.kill_connection().await;
            driver.line(SESSION_GONE_NOTICE).await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 1, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, "", "no hint for a session that is gone");
    assert!(rig.gateway.detaches().is_empty(), "nothing is detached");
}

// Slow: the link tries to redial after the drop and gives up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quit_after_a_failed_link_skips_the_detach_but_prints_the_hint() {
    let rig = Rig::start();
    let gateway = &rig.gateway;
    let socket = rig.gateway.home().join("gateway.sock");
    let (outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("hello");
            eventually("the prompt to be admitted", || {
                gateway.submissions().len() == 1
            })
            .await;
            settle().await;
            // A gateway node that is no longer a socket is something the link will not retry.
            std::fs::remove_file(&socket).expect("the socket is there");
            std::fs::write(&socket, b"").expect("a file in its place");
            gateway.kill_connection().await;
            driver.line("Next step:").await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 1, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, HINT, "the session is still on the gateway");
    assert!(rig.gateway.detaches().is_empty(), "nothing is detached");
    assert!(locked(&log).restored);
}

// Slow: the second case waits the full five seconds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quit_while_connecting_detaches_a_late_session() {
    // Nothing is in flight: nothing to wait for, nothing to detach, no hint.
    let mut idle = AppState::new(LAUNCH_ID);
    assert_eq!(
        quit_while_opening(&mut idle).await,
        (DetachOutcome::Skipped, 0, String::new())
    );

    // The reply arrives after two seconds: the quit waits for it, detaches, and prints no hint.
    let rig = Rig::start();
    rig.gateway
        .queue_behavior("session.create", Behavior::Delay(Duration::from_secs(2)));
    let gateway = &rig.gateway;
    let started = Instant::now();
    let (outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            gateway.wait_for_request("session.create", 0).await;
            driver.submit("typed before the session exists");
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert!(
        started.elapsed() < Duration::from_secs(9),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, "", "no prompt was sent, so no hint");
    let detaches = rig.gateway.detaches();
    assert_eq!(detaches.len(), 1, "the late session is detached");
    assert_eq!(detaches[0].session_id, "local-session-1");
    assert!(!detaches[0].had_end_reason);
    assert!(
        rig.gateway.submissions().is_empty(),
        "a prompt typed before the session existed is not sent after a quit"
    );
    assert!(
        outcome.stderr.contains("1 typed prompt was never sent:"),
        "{}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("  typed before the session exists"));
    assert!(locked(&log).restored);

    // No reply at all: the quit gives up after five seconds and detaches nothing.
    let rig = Rig::start();
    rig.gateway
        .queue_behavior("session.create", Behavior::Silent);
    let gateway = &rig.gateway;
    let started = Instant::now();
    let (outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            gateway.wait_for_request("session.create", 0).await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(4) && waited < Duration::from_secs(9),
        "waited {waited:?}"
    );
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(outcome.stdout, "");
    assert!(rig.gateway.detaches().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Gaps
// ---------------------------------------------------------------------------------------------

/// Tell the app that the gateway dropped events it cannot replay: the link raises a gap for the
/// tracked session.
async fn overflow(gateway: &MockGateway) {
    let epoch = mock_gateway::recorded_resume_snapshot()["replay_epoch"].clone();
    gateway
        .send_event_params(json!({
            "type": "session.replay_gap",
            "session_id": gateway.session_id(),
            "replay_epoch": epoch,
            "payload": {"latest_seq": 100, "replay_epoch": epoch},
        }))
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gap_resumes_rebuilds_resyncs_without_duplicates() {
    // A gap whose snapshot starts with the cells already shown: only the new cell is committed.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    let first = resume_snapshot(&folder);
    let mut messages = first["display_messages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    messages.push(json!({"role": "user", "text": "And one more thing."}));
    let extended = with_messages(first.clone(), messages);
    rig.gateway.set_resume_snapshot(first.clone());
    let gateway = &rig.gateway;
    let next = extended.clone();
    let (outcome, log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                gateway.queue_resume_snapshot(next);
                overflow(gateway).await;
                driver.line(REFRESHED_NOTICE).await;
                settle().await;
                driver.quit();
            },
        )
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    let resumes = rig.gateway.requests("session.resume");
    assert_eq!(resumes.len(), 2, "the launch's resume and one recovery");
    assert_eq!(
        resumes[1].params,
        json!({"session_id": "local-session-1", "display_projection": true})
    );
    let shown: Vec<String> = SessionState::from_snapshot(snapshot_of(&extended))
        .cells()
        .iter()
        .map(cell_text)
        .collect();
    assert_eq!(
        locked(&log).cells,
        shown,
        "each cell is committed once, in order"
    );
    assert!(
        !locked(&log)
            .lines
            .iter()
            .any(|line| line.contains("Transcript refreshed"))
    );
    assert_eq!(
        locked(&log)
            .lines
            .iter()
            .filter(|line| line.contains(REFRESHED_NOTICE))
            .count(),
        1
    );
    // The later detach names the subscription the recovery received.
    let detaches = rig.gateway.detaches();
    assert_eq!(detaches.len(), 1);
    assert_eq!(rig.gateway.attaches().len(), 2);
    assert_eq!(
        detaches[0].subscription_id,
        rig.gateway.attaches()[1].subscription_id
    );
    rig.gateway.assert_no_violations();

    // A gap whose snapshot does not start with what was shown: the transcript is shown again.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    let first = resume_snapshot(&folder);
    let mut messages = first["display_messages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    messages[0] = json!({"role": "user", "text": "A different first message."});
    let rewritten = with_messages(first.clone(), messages);
    rig.gateway.set_resume_snapshot(first.clone());
    let gateway = &rig.gateway;
    let next = rewritten.clone();
    let (_outcome, log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                gateway.queue_resume_snapshot(next);
                overflow(gateway).await;
                driver.line(REFRESHED_NOTICE).await;
                settle().await;
                driver.quit();
            },
        )
        .await;
    let before: Vec<String> = SessionState::from_snapshot(snapshot_of(&first))
        .cells()
        .iter()
        .map(cell_text)
        .collect();
    let after: Vec<String> = SessionState::from_snapshot(snapshot_of(&rewritten))
        .cells()
        .iter()
        .map(cell_text)
        .collect();
    let committed = locked(&log).cells.clone();
    assert_eq!(committed.len(), before.len() + after.len());
    assert_eq!(committed[..before.len()], before);
    assert_eq!(committed[before.len()..], after);
    let lines = locked(&log).lines.clone();
    let marker = lines
        .iter()
        .position(|line| line.contains("\u{2500}\u{2500} Transcript refreshed \u{2500}\u{2500}"))
        .unwrap_or_else(|| panic!("no refresh marker in {lines:?}"));
    assert!(
        marker
            < lines
                .iter()
                .position(|line| line.contains(REFRESHED_NOTICE))
                .unwrap_or(0)
    );
}

// ---------------------------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn refused_or_unknown_submit_tells_the_reducer_and_is_not_resent() {
    let cases: [(Behavior, &str, &str); 3] = [
        (
            Behavior::Refuse("permission_denied".to_string()),
            "Hermes refused the prompt (permission_denied). The connection is not attached to \
             this session; it will reconnect.",
            "refused",
        ),
        (
            Behavior::DropBeforeReply,
            "The connection dropped while this prompt was being sent. It is shown above if \
             Hermes received it; send it again if it is not.",
            "unknown",
        ),
        (
            Behavior::VoiceStopped,
            "Hermes took that text as the stop phrase of a voice chat on this session, so it \
             was not sent as a prompt.",
            "voice",
        ),
    ];
    let mut notices = Vec::new();
    for (behavior, expected, kind) in cases {
        let rig = in_process().await;
        let opened = rig.open(new_request()).await.expect("created");
        let mut app = AppState::new(LAUNCH_ID);
        opened_effects(&mut app, opened);

        // The reducer places the prompt and asks for it to be sent.
        let effects = submit(&mut app, "hello");
        let sends = sent_prompts(&effects);
        assert_eq!(sends.len(), 1);
        let id = sends[0].1.clone();
        let effect = effects
            .into_iter()
            .find_map(|effect| match effect {
                AppEffect::Send(effect) => Some(effect),
                _ => None,
            })
            .expect("a send");

        // The service sends it exactly once.
        rig.mock.queue_behavior("prompt.submit", behavior);
        let outcome = within(15, "the submit", rig.service.execute(effect)).await;
        let frames = rig.mock.requests("prompt.submit");
        assert_eq!(frames.len(), 1, "{kind}: one frame");
        assert_eq!(
            sorted_keys(&frames[0].params),
            ["session_id", "submission_id", "text"]
        );
        assert_eq!(frames[0].params["submission_id"], id.as_str());
        match kind {
            "refused" => assert_eq!(
                outcome,
                ExecuteOutcome::Refused {
                    reason: "permission_denied".to_string()
                }
            ),
            "unknown" => assert_eq!(outcome, ExecuteOutcome::OutcomeUnknown),
            _ => assert_eq!(outcome, ExecuteOutcome::VoiceStopped),
        }
        assert!(
            rig.mock
                .no_new_frames_within(Duration::from_millis(400))
                .await,
            "{kind}: nothing is sent again"
        );
        assert_eq!(rig.mock.requests("prompt.submit").len(), 1);
        rig.mock.assert_no_violations();

        // The loop turns the outcome into the reducer's failure event and its own notice.
        let effects = update(
            &mut app,
            AppMessage::SubmitDone {
                submission_id: id,
                outcome,
            },
        );
        let lines = committed_lines(&effects);
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "{kind}: {lines:?}"
        );
        assert!(sent_prompts(&effects).is_empty(), "{kind}: no resend");
        let cells = app
            .opened
            .as_ref()
            .map(|opened| opened.state.cells().to_vec());
        assert!(
            cells.unwrap_or_default().iter().any(
                |cell| matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::Error)
            ),
            "{kind}: the reducer was told"
        );
        notices.push(lines.join("\n"));
    }
    assert_ne!(notices[0], notices[1]);
    assert_ne!(notices[1], notices[2]);
    assert_ne!(notices[0], notices[2]);
    assert!(voice_stop_notice().contains("stop phrase"));

    // Another refusal gets the plain sentence without the reconnect remark.
    let rig = in_process().await;
    let opened = rig.open(new_request()).await.expect("created");
    let mut app = AppState::new(LAUNCH_ID);
    opened_effects(&mut app, opened);
    let sends = sent_prompts(&submit(&mut app, "hello"));
    let effects = update(
        &mut app,
        AppMessage::SubmitDone {
            submission_id: sends[0].1.clone(),
            outcome: ExecuteOutcome::Refused {
                reason: "runtime_draining".to_string(),
            },
        },
    );
    let lines = committed_lines(&effects);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Hermes refused the prompt (runtime_draining)."))
    );
    assert!(!lines.iter().any(|line| line.contains("it will reconnect")));
}

#[tokio::test]
async fn enter_during_a_running_turn_sends_prompt_submit() {
    let rig = in_process().await;
    let opened = rig.open_snapshot(running_turn("/work/proj")).await;
    assert_eq!(opened.state.turn(), &TurnState::Running);
    let mut app = AppState::new(LAUNCH_ID);
    opened_effects(&mut app, opened);

    let effects = submit(&mut app, "and then");
    let sends = sent_prompts(&effects);
    assert_eq!(
        sends,
        [(
            "and then".to_string(),
            "talaria-0123456789abcdef-1".to_string()
        )]
    );
    let effect = effects
        .into_iter()
        .find_map(|effect| match effect {
            AppEffect::Send(effect) => Some(effect),
            _ => None,
        })
        .expect("a send");
    assert_eq!(
        effect,
        Effect::SendPrompt {
            session_id: SessionId::new("local-session-1"),
            text: "and then".to_string(),
            submission_id: "talaria-0123456789abcdef-1".to_string(),
        }
    );
    assert_eq!(
        within(15, "the submit", rig.service.execute(effect)).await,
        ExecuteOutcome::Sent
    );
    let frames = rig.mock.requests("prompt.submit");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0].params,
        json!({
            "session_id": "local-session-1",
            "text": "and then",
            "submission_id": "talaria-0123456789abcdef-1",
        })
    );
    assert_eq!(rig.mock.submissions().len(), 1);
    rig.mock.assert_no_violations();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompts_held_until_ready_in_order_with_cap() {
    let rig = in_process().await;
    let opened = rig.open(new_request()).await.expect("created");
    let mut app = AppState::new(LAUNCH_ID);

    // Three typed before the session exists are kept, not sent.
    for text in ["first", "second", "third"] {
        let effects = submit(&mut app, text);
        assert!(sent_prompts(&effects).is_empty());
    }
    assert_eq!(app.held, ["first", "second", "third"]);

    // They go out in order, each with its own number, when the session arrives.
    let effects = opened_effects(&mut app, opened);
    let sends = sent_prompts(&effects);
    assert_eq!(
        sends,
        [
            (
                "first".to_string(),
                "talaria-0123456789abcdef-1".to_string()
            ),
            (
                "second".to_string(),
                "talaria-0123456789abcdef-2".to_string()
            ),
            (
                "third".to_string(),
                "talaria-0123456789abcdef-3".to_string()
            ),
        ]
    );
    assert!(app.held.is_empty());

    // The eleventh before the session exists is refused with a notice and left in the composer.
    let mut app = AppState::new(LAUNCH_ID);
    for index in 0..10 {
        assert!(
            submit(&mut app, &format!("prompt {index}"))
                .iter()
                .all(|effect| { !matches!(effect, AppEffect::LoadDraft(_)) })
        );
    }
    let effects = submit(&mut app, "eleventh");
    assert_eq!(app.held.len(), 10);
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains(HELD_FULL_NOTICE))
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::LoadDraft(text) if text == "eleventh"))
    );
    let refused = hold_prompt(&mut app, "twelfth").expect_err("the cap holds");
    assert!(refused.contains("Too many prompts are waiting for Hermes."));
    assert_eq!(app.held.len(), 10);

    // A blank line does nothing at all.
    let mut app = AppState::new(LAUNCH_ID);
    assert!(sent_prompts(&submit(&mut app, "   ")).is_empty());
    assert!(app.held.is_empty());

    // In the loop: typed while the create reply is late, sent in order once it arrives, and
    // never before it. Ten prompts, the most that are held, so a reordering shows.
    let rig = Rig::start();
    rig.gateway
        .queue_behavior("session.create", Behavior::Delay(Duration::from_secs(1)));
    let gateway = &rig.gateway;
    let (outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            gateway.wait_for_request("session.create", 0).await;
            for index in 0..10 {
                driver.submit(&format!("prompt {index}"));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(gateway.requests("prompt.submit").is_empty());
            eventually("all prompts to be sent", || {
                gateway.requests("prompt.submit").len() == 10
            })
            .await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    let sent = rig.gateway.submissions();
    assert_eq!(sent.len(), 10);
    for (index, submission) in sent.iter().enumerate() {
        assert_eq!(submission.text, format!("prompt {index}"));
        assert_eq!(
            submission.submission_id,
            format!("talaria-0123456789abcdef-{}", index + 1)
        );
    }
    rig.gateway.assert_no_violations();
}

// Slow: the reconnect cases wait for the link's back-off.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupt_with_open_approval_denies_first() {
    // With no session, no running turn and no connection nothing is sent.
    let mut empty = AppState::new(LAUNCH_ID);
    assert!(matches!(
        interrupt(&mut empty).await,
        ExecuteOutcome::Refused { .. }
    ));
    let none = update(&mut empty, AppMessage::User(UserAction::Interrupt));
    assert!(
        !none
            .iter()
            .any(|effect| matches!(effect, AppEffect::Interrupt(_)))
    );

    // With an approval open the frames are the deny, then the interrupt.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    rig.gateway
        .set_resume_snapshot(approval_while_running(&folder));
    let gateway = &rig.gateway;
    let (outcome, _log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                driver.send(UserAction::Interrupt);
                gateway.wait_for_request("session.interrupt", 0).await;
                driver.quit();
            },
        )
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    let methods: Vec<String> = rig
        .gateway
        .seen()
        .iter()
        .filter_map(|frame| frame.method.clone())
        .filter(|method| method == "approval.respond" || method == "session.interrupt")
        .collect();
    assert_eq!(methods, ["approval.respond", "session.interrupt"]);
    let deny = &rig.gateway.requests("approval.respond")[0].params;
    assert_eq!(deny["choice"], "deny");
    assert_eq!(deny["session_id"], "local-session-1");
    assert_eq!(deny["prompt_id"], "prompt-1");
    assert_eq!(
        rig.gateway.requests("session.interrupt")[0].params,
        json!({"session_id": "local-session-1", "execution_generation": 1})
    );
    rig.gateway.assert_no_violations();

    // A deny that fails does not stop the interrupt.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    rig.gateway
        .set_resume_snapshot(approval_while_running(&folder));
    rig.gateway
        .set_respond_reply(Reply::Error("permission_denied"));
    let gateway = &rig.gateway;
    let (_outcome, _log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                driver.send(UserAction::Interrupt);
                gateway.wait_for_request("session.interrupt", 0).await;
                driver.quit();
            },
        )
        .await;
    assert_eq!(rig.gateway.requests("approval.respond").len(), 1);
    assert_eq!(rig.gateway.requests("session.interrupt").len(), 1);

    // A refusal for a stale number shows its notice and clears the latch: the next press sends
    // an interrupt again, with the number the reducer holds by then.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    rig.gateway.set_resume_snapshot(running_turn(&folder));
    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                // The gateway has moved on to generation 3; the snapshot said 2.
                gateway.track_generation("local-session-1", 3);
                driver.send(UserAction::Interrupt);
                driver
                    .line("Hermes did not stop the turn (stale_generation). If it is still running, press Esc again.")
                    .await;
                assert_eq!(gateway.requests("session.interrupt").len(), 1);
                // A new turn starts at the number the gateway holds.
                gateway
                    .send_event("message.start", json!({"execution_generation": 3}))
                    .await;
                settle().await;
                driver.send(UserAction::Interrupt);
                gateway.wait_for_request("session.interrupt", 1).await;
                settle().await;
                driver.quit();
            },
        )
        .await;
    let frames = rig.gateway.requests("session.interrupt");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].params["execution_generation"], 2);
    assert_eq!(frames[1].params["execution_generation"], 3);
    assert_eq!(
        locked(&log)
            .lines
            .iter()
            .filter(|line| line.contains("press Esc again"))
            .count(),
        1,
        "only the refused one has a notice"
    );

    // Presses while one sequence is in flight send nothing more.
    let rig = Rig::start();
    let folder = rig.folder.display().to_string();
    rig.gateway.set_resume_snapshot(running_turn(&folder));
    rig.gateway
        .set_delay("session.interrupt", Some(Duration::from_millis(800)));
    let (_outcome, _log) = rig
        .drive(
            StartMode::Resume(Some("local-session-1".to_string())),
            |driver| async move {
                driver.opened().await;
                settle().await;
                driver.send(UserAction::Interrupt);
                driver.send(UserAction::Interrupt);
                driver.send(UserAction::Interrupt);
                tokio::time::sleep(Duration::from_millis(1500)).await;
                driver.quit();
            },
        )
        .await;
    assert_eq!(rig.gateway.requests("session.interrupt").len(), 1);
    assert_eq!(
        talaria_app::failure::interrupt_refused_notice("stale_generation"),
        "Hermes did not stop the turn (stale_generation). If it is still running, press Esc again."
    );
}

#[tokio::test]
async fn other_slash_lines_are_not_sent() {
    assert_eq!(slash_line("/quit"), Slash::Quit);
    assert_eq!(slash_line("/quit "), Slash::Quit);

    // `/quit`, with or without trailing space, quits and sends nothing.
    for text in ["/quit", "/quit "] {
        let mut app = AppState::new(LAUNCH_ID);
        let effects = submit(&mut app, text);
        assert!(app.quitting, "{text:?}");
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Quit))
        );
        assert!(sent_prompts(&effects).is_empty());
    }
}

// ---------------------------------------------------------------------------------------------
// Server requests and link notices
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_server_request_gets_one_answer() {
    let rig = Rig::start();
    let gateway = &rig.gateway;
    let (outcome, _log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            settle().await;
            let sent = gateway.every_server_request().await;
            assert_eq!(sent.len(), 16);
            let mut ids: Vec<&String> = sent.iter().map(|(_, id)| id).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), 16, "distinct ids");

            // Kinds Talaria does not support are refused at once.
            let masked: Vec<&(String, String)> = sent
                .iter()
                .filter(|(kind, _)| {
                    matches!(
                        talaria_app::prompts::policy_for(kind),
                        talaria_app::prompts::ServerRequestPolicy::Masked(_)
                    )
                })
                .collect();
            assert_eq!(masked.len(), 4, "the four masked kinds");
            eventually("the refusals", || gateway.answers().len() >= 12).await;
            for (kind, id) in &sent {
                if masked.iter().any(|(_, masked_id)| masked_id == id) {
                    continue;
                }
                let answers = gateway.answers_for(id);
                assert_eq!(answers.len(), 1, "{kind}");
                assert!(answers[0].is_error, "{kind}");
                assert_eq!(answers[0].params["code"], -32601, "{kind}");
            }

            // The masked kinds wait for the person; declining each one answers it.
            for (_, id) in &masked {
                driver.send(UserAction::Masked {
                    request_id: id.clone(),
                    value: Zeroizing::new(String::new()),
                });
            }
            eventually("the declines", || gateway.answers().len() >= 16).await;
            settle().await;
            driver.quit();
        })
        .await;
    assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    assert_eq!(
        rig.gateway.answers().len(),
        16,
        "exactly one answer for each"
    );
    rig.gateway.assert_no_duplicate_answers();
    rig.gateway.assert_no_violations();
}

#[tokio::test]
async fn link_notices_session_gone_and_failed() {
    let gone = LinkEvent::SessionGone {
        session_id: "local-session-1".to_string(),
    };
    let notice = link_notice(&gone).expect("gone");
    assert_eq!(notice, SESSION_GONE_NOTICE);
    let failed = LinkEvent::State(LinkState::Failed {
        reason: "the gateway closed the connection".to_string(),
        next_step: "Try again.".to_string(),
    });
    let notice = link_notice(&failed).expect("failed");
    assert!(notice.contains("the gateway closed the connection"));
    assert!(notice.contains("Next step: Try again."));

    // A session that is gone: the notice is shown and nothing more is sent.
    let rig = in_process().await;
    let opened = rig.open(new_request()).await.expect("created");
    let mut app = AppState::new(LAUNCH_ID);
    opened_effects(&mut app, opened);
    let effects = update(&mut app, AppMessage::Link(gone));
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains("This session no longer exists on the gateway."))
    );
    assert!(app.session_gone);
    let effects = submit(&mut app, "anything");
    assert!(sent_prompts(&effects).is_empty());
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains("This session no longer exists on the gateway."))
    );

    // A failed link: both lines are shown, the screen stays open, and nothing more is sent.
    let mut app = AppState::new(LAUNCH_ID);
    let opened = rig.open(latest("/work/proj")).await.expect("resumed");
    opened_effects(&mut app, opened);
    let effects = update(&mut app, AppMessage::Link(failed));
    let lines = committed_lines(&effects);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("the gateway closed the connection"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Next step: Try again."))
    );
    assert!(!app.quitting);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Quit))
    );
    assert!(app.link_failed);
    let effects = submit(&mut app, "anything");
    assert!(sent_prompts(&effects).is_empty());
    assert!(!app.quitting);
}

// ---------------------------------------------------------------------------------------------
// Ensure
// ---------------------------------------------------------------------------------------------

#[test]
fn ensure_runs_at_most_once_per_launch() {
    assert_eq!(ensure_invocations(false), 1);
    assert_eq!(ensure_invocations(true), 0);

    // The real count needs a stand-in `hermes` first on the search path, so each case runs in a
    // child of this test binary with that path.
    if let Ok(case) = std::env::var("TALARIA_FLOW_ENSURE_CASE") {
        ensure_child(&case);
        return;
    }
    for (case, runs) in [("ensure", 1usize), ("never", 0usize)] {
        let root = tempfile::tempdir().expect("root");
        let home = root.path().join("home");
        let bin = root.path().join("bin");
        std::fs::create_dir(&home).expect("home");
        std::fs::create_dir(&bin).expect("bin");
        let counter = root.path().join("ensure-count");
        mock_gateway::write_ensure_stand_in(&bin, &counter, &home);
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "ensure_runs_at_most_once_per_launch",
                "--exact",
                "--nocapture",
            ])
            .env("TALARIA_FLOW_ENSURE_CASE", case)
            .env("TALARIA_FLOW_ENSURE_HOME", &home)
            .env("TALARIA_FLOW_ENSURE_COUNTER", &counter)
            .env("PATH", path)
            .output()
            .expect("the child runs");
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(mock_gateway::ensure_runs(&counter), runs, "{case}");
    }
}

/// The body of one ensure case, run in a child process.
fn ensure_child(case: &str) {
    let home = PathBuf::from(std::env::var("TALARIA_FLOW_ENSURE_HOME").expect("home"));
    let counter = PathBuf::from(std::env::var("TALARIA_FLOW_ENSURE_COUNTER").expect("counter"));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let gateway = StandaloneGateway::start_absent(&home, &counter);
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_string(),
        folder: home.clone(),
        record: None,
        bench_replay: None,
        no_ensure: case == "never",
    };
    let opened = runtime.block_on(ConnectionService::open(&launch, &home, None));
    if case == "never" {
        let failure = opened.err().expect("no gateway, and ensure is off");
        assert_eq!(failure.exit_code, 1);
        assert!(!gateway.is_present());
        return;
    }
    let connection = opened.unwrap_or_else(|failure| panic!("{}", failure.render()));
    // A forced reconnect redials without running ensure again.
    runtime.block_on(gateway.kill_connection());
    let deadline = Instant::now() + Duration::from_secs(20);
    while gateway.websocket_connections() < 2 {
        assert!(Instant::now() < deadline, "the link did not redial");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(mock_gateway::ensure_runs(&counter), 1);
    runtime.block_on(connection.link.close());
}

// ---------------------------------------------------------------------------------------------
// Start-up pieces
// ---------------------------------------------------------------------------------------------

#[test]
fn user_config_and_key_map_notices_are_committed_at_start() {
    let write = |home: &Path, text: &str| {
        let directory = home.join(".talaria");
        std::fs::create_dir_all(&directory).expect("dir");
        std::fs::write(directory.join("config.toml"), text).expect("config");
    };

    // A table Talaria does not read gives a notice from the file reader. [theme] is read since
    // #227, so a typo table stands in.
    let home = tempfile::tempdir().expect("home");
    write(home.path(), "[theme_typo]\nname = \"x\"\n");
    let notices = startup_notices(home.path());
    assert!(
        notices
            .iter()
            .any(|notice| notice.contains("Ignoring [theme_typo]")),
        "{notices:?}"
    );

    // A key spelling the key map cannot read gives a notice from the key map, over the entries
    // the file reader passes on.
    let home = tempfile::tempdir().expect("home");
    write(home.path(), "[keys]\nsubmit = [\"ctrl-j\"]\n");
    let loaded = talaria_app::config::load_user_config(home.path());
    assert_eq!(startup_notices(home.path()), loaded.notices);
    let (_map, key_notices) = talaria_tui::keys::KeyMap::from_entries(&loaded.config.keys);
    assert!(!key_notices.is_empty(), "{key_notices:?}");

    // A valid custom key is applied and raises nothing.
    let home = tempfile::tempdir().expect("home");
    write(home.path(), "[keys]\nsubmit = [\"ctrl+g\"]\n");
    let loaded = talaria_app::config::load_user_config(home.path());
    assert!(startup_notices(home.path()).is_empty());
    assert_eq!(loaded.config.keys["submit"], ["ctrl+g"]);
    let (_map, key_notices) = talaria_tui::keys::KeyMap::from_entries(&loaded.config.keys);
    assert!(key_notices.is_empty(), "{key_notices:?}");
}

struct ComposerProbe {
    ticks: u32,
    due: Option<Instant>,
}

impl ComposerDeadline for ComposerProbe {
    fn tick(&mut self, _now: Instant) {
        self.ticks += 1;
    }

    fn next_tick(&self) -> Option<Instant> {
        self.due
    }
}

#[test]
fn composer_tick_runs_at_next_tick_deadline() {
    let due = Instant::now();
    let mut probe = ComposerProbe {
        ticks: 0,
        due: Some(due),
    };
    drive_composer(&mut probe, due);
    assert_eq!(probe.ticks, 1);
    drive_composer(&mut probe, due - Duration::from_secs(1));
    assert_eq!(probe.ticks, 1);
}

struct StatusProbe {
    ticks: u32,
    shut: bool,
}

impl StatusHost for StatusProbe {
    fn on_tick(&mut self, _now: Instant) {
        self.ticks += 1;
    }

    fn next_wake(&self, now: Instant) -> Option<Instant> {
        Some(now + Duration::from_secs(5))
    }

    fn shutdown(&mut self) -> impl Future<Output = ()> + Send {
        self.shut = true;
        std::future::ready(())
    }
}

#[tokio::test]
async fn status_command_service_ticks_with_redaction_and_shuts_down_on_quit() {
    let mut probe = StatusProbe {
        ticks: 0,
        shut: false,
    };
    drive_status(&mut probe, hermes_client::is_suspicious_key).await;
    assert!(probe.ticks >= 1);
    assert!(probe.shut);
    assert!(hermes_client::is_suspicious_key("api_key"));
    assert!(!hermes_client::is_suspicious_key("model"));
}

#[test]
fn inbound_session_event_passes_full_params_as_raw_to_reducer() {
    let params = json!({
        "type": "not.a.real.event",
        "session_id": "local-session-1",
        "seq": 1,
        "replay_epoch": "epoch-1",
        "payload": {"marker": "kept"}
    });
    let frame = EventFrame::from_params(&params).expect("unknown event decodes");
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    let _ = apply_inbound(&mut state, "local-session-1", frame.clone(), params.clone());
    assert!(
        state
            .unknown_event_types()
            .iter()
            .any(|name| name == "not.a.real.event")
    );
    let direct = talaria_core::session::update(
        &mut SessionState::empty(SessionId::new("local-session-1")),
        Event::Session {
            session_id: SessionId::new("local-session-1"),
            frame,
            raw: params,
        },
    );
    assert!(direct.is_empty());
}

#[test]
fn new_connection_feeds_connection_opened_generation_to_reducer() {
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    let params = json!({
        "type": "not.a.real.event",
        "session_id": "local-session-1",
        "seq": 1,
        "replay_epoch": "epoch-1",
        "payload": {}
    });
    let frame = EventFrame::from_params(&params).expect("frame");
    talaria_core::session::update(
        &mut state,
        Event::Session {
            session_id: SessionId::new("local-session-1"),
            frame,
            raw: params,
        },
    );
    assert!(!state.unknown_event_types().is_empty());
    note_connection(&mut state, 2);
    assert!(state.unknown_event_types().is_empty());
}
