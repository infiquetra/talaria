//! Application loop: one channel, a pure update, and the effect executor.

use std::collections::VecDeque;
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hermes_client::ServerRequest;
use hermes_client::liveness::LinkEvent;
use hermes_client::record::Recorder;
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_contract::events::EventFrame;
use serde_json::Value;
use talaria_core::session::{Cell, Effect, Event, PromptKey, SessionState};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::connection::OpenConnection;
use crate::failure::LaunchFailure;
use crate::launch::{Launch, ListRow};
use crate::prompts::MaskedRequestInfo;
use crate::sessions::{
    DetachOutcome, ExecuteOutcome, OpenError, OpenedSession, SessionInfoSummary, SessionService,
};

/// How long a quit waits for a create or resume that is still in flight.
pub const QUIT_WAIT: Duration = Duration::from_secs(5);
/// The bound passed to the prompt service's deny before an interrupt (it caps it there too).
pub const DENY_WAIT: Duration = Duration::from_secs(2);
/// Titles in the resume separator are cut to this many characters, with no ellipsis.
pub const TITLE_CHARS: usize = 60;

pub const SESSION_GONE_NOTICE: &str = "This session no longer exists on the gateway.";
pub const STILL_WORKING: &str = "The agent is still working on it in the background.";
pub const SLASH_NOTICE: &str = "Only /quit works in this version. To send text that starts with a slash, put a space before it.";
pub const HELD_FULL_NOTICE: &str =
    "Too many prompts are waiting for Hermes. Wait a moment, then send this one again.";
pub const REFRESHED_NOTICE: &str = "Reconnected. The transcript was refreshed.";
pub const STALE_SUBSCRIPTION: &str =
    "Hermes no longer knew this connection's subscription; the session is unaffected.";

/// What the link is doing, as the screen shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkView {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

/// What a person asked for, after the screen routed a key or a paste.
pub enum UserAction {
    /// The composer sent this text.
    Submit(String),
    /// Stop the running turn (Esc, or Ctrl+C on a card or an empty draft).
    Interrupt,
    /// `/quit` or the composer's exit request.
    Quit,
    /// An approval card was answered.
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    /// A clarification card was answered.
    Clarify { key: PromptKey, answer: String },
    /// A masked card was answered; an empty value declines it.
    Masked {
        request_id: String,
        value: Zeroizing<String>,
    },
    /// A line the screen wants shown.
    Notice(String),
}

/// Everything the screen draws from, borrowed for one draw.
pub struct View<'a> {
    pub state: Option<&'a SessionState>,
    pub session_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub info: Option<&'a SessionInfoSummary>,
    pub link: LinkView,
    pub profile: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    pub masked: &'a [MaskedRequestInfo],
    pub held: usize,
}

impl View<'_> {
    /// A turn is running in the session.
    #[must_use]
    pub fn turn_running(&self) -> bool {
        todo!("binary")
    }
}

/// The terminal side of the loop.
pub trait Screen: ComposerDeadline + StatusHost {
    /// One raw input: a key, a paste, a resize, or a status command result.
    type Input: Send + 'static;
    /// Route one input and redraw; returns what the person asked for.
    fn input(&mut self, input: Self::Input, view: &View<'_>, now: Instant) -> Vec<UserAction>;
    /// Draw the live region.
    fn draw(&mut self, view: &View<'_>);
    /// Write plain lines into the scrollback, once.
    fn commit_lines(&mut self, lines: Vec<String>);
    /// Render cells into the scrollback, once.
    fn commit_cells(&mut self, cells: &[Cell]);
    /// Put text back into the composer.
    fn load_draft(&mut self, text: &str);
    /// Give the terminal back. Later draws write nothing.
    fn restore(&mut self);
}

pub trait ComposerDeadline {
    fn tick(&mut self, now: Instant);
    fn next_tick(&self) -> Option<Instant>;
}

pub trait StatusHost {
    fn on_tick(&mut self, now: Instant);
    fn next_wake(&self, now: Instant) -> Option<Instant>;
    fn shutdown(&mut self) -> impl Future<Output = ()> + Send;
}

/// A session opened by the background start-up task.
pub struct Ready {
    pub connection: OpenConnection,
    pub service: SessionService,
    pub opened: OpenedSession,
    /// The "works in another folder" notice of a resume by id.
    pub notice: Option<String>,
}

pub struct AppState {
    pub opened: Option<OpenedSession>,
    pub held: VecDeque<String>,
    pub launch_id: String,
    pub prompt_counter: u64,
    pub quitting: bool,
    pub committed_cells: usize,
    pub link_failed: bool,
    pub session_gone: bool,
    /// The profile the resume hint names.
    pub profile: String,
    pub link_state: LinkView,
    /// An interrupt sequence is in flight; further presses send nothing.
    pub interrupting: bool,
    /// The title of the row picked from the `--resume` list.
    pub list_title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub masked: Vec<MaskedRequestInfo>,
}

impl AppState {
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        Self {
            opened: None,
            held: VecDeque::new(),
            launch_id: launch_id.into(),
            prompt_counter: 0,
            quitting: false,
            committed_cells: 0,
            link_failed: false,
            session_gone: false,
            profile: "default".to_string(),
            link_state: LinkView::Connecting,
            interrupting: false,
            list_title: None,
            model: None,
            effort: None,
            masked: Vec::new(),
        }
    }

    /// The view the screen draws from.
    #[must_use]
    pub fn view(&self) -> View<'_> {
        todo!("binary")
    }
}

/// One input to [`update`].
#[allow(clippy::large_enum_variant)]
pub enum AppMessage {
    User(UserAction),
    Link(LinkEvent),
    /// The session is open: commit its history, track it, send held prompts.
    Opened {
        opened: OpenedSession,
        notice: Option<String>,
    },
    SubmitDone {
        submission_id: String,
        outcome: ExecuteOutcome,
    },
    InterruptDone {
        outcome: ExecuteOutcome,
        notices: Vec<String>,
    },
    PromptDone {
        events: Vec<Event>,
        notices: Vec<String>,
        masked: Option<Vec<MaskedRequestInfo>>,
    },
    Recovered(Result<OpenedSession, OpenError>),
}

/// What the executor does after one update. Ranges index the open session's cells.
pub enum AppEffect {
    CommitLines(Vec<String>),
    CommitCells(Range<usize>),
    Redraw,
    LoadDraft(String),
    /// `prompt.submit`.
    Send(Effect),
    /// Deny open approvals, then `session.interrupt`.
    Interrupt(Effect),
    /// An approval or clarification answer, with the note to attach first.
    Respond {
        effect: Effect,
        note: Option<(PromptKey, String)>,
    },
    AnswerMasked {
        request_id: String,
        value: Zeroizing<String>,
    },
    ServerRequest(ServerRequest),
    ConnectionOpened(u64),
    PromptsChanged(Vec<PromptKey>),
    Track {
        session_id: String,
        cursor: Cursor,
    },
    Resync {
        session_id: String,
        cursor: Cursor,
    },
    Recover(ReplayGap),
    Quit,
}

/// Apply one message. No input or output: the effects say what to do.
pub fn update(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let _ = (state, message);
    todo!("binary")
}

/// The separator title: the session's title, else the picked list row's, else the first line
/// of the first user message, else `untitled session`; cut to [`TITLE_CHARS`].
#[must_use]
pub fn session_title(info_title: Option<&str>, list_title: Option<&str>, cells: &[Cell]) -> String {
    let _ = (info_title, list_title, cells);
    todo!("binary")
}

/// The separator and marker for an opened session, and the cells to commit after them. A new
/// session with no cells gets nothing.
#[must_use]
pub fn resume_header(cells: &[Cell], title: &str) -> (Vec<String>, Range<usize>) {
    let _ = (cells, title);
    todo!("binary")
}

/// The resumed history as plain lines: separator, marker, then one line per shown cell.
#[must_use]
pub fn resume_history(cells: &[Cell], title: &str) -> Vec<String> {
    let _ = (cells, title);
    todo!("binary")
}

/// The notice for a session resumed by id whose folder is another one.
#[must_use]
pub fn other_folder_notice(cwd: &str) -> String {
    let _ = cwd;
    todo!("binary")
}

/// [`other_folder_notice`] only when the session's folder is known and outside the workspace.
#[must_use]
pub fn folder_notice(session_cwd: Option<&str>, workspace: &Path) -> Option<String> {
    let _ = (session_cwd, workspace);
    todo!("binary")
}

/// The notices of `~/.talaria/config.toml` under `home`.
#[must_use]
pub fn startup_notices(home: &Path) -> Vec<String> {
    let _ = home;
    todo!("binary")
}

/// Run the composer's timer when it is due.
pub fn drive_composer(deadline: &mut impl ComposerDeadline, now: Instant) {
    let _ = (deadline, now);
    todo!("binary")
}

/// The status command's last step at quit.
pub async fn drive_status(host: &mut impl StatusHost, is_credential_name: fn(&str) -> bool) {
    let _ = (host, is_credential_name);
    todo!("binary")
}

/// A session event for the reducer, with all of `params` as `raw`.
pub fn apply_inbound(
    state: &mut SessionState,
    session_id: &str,
    frame: EventFrame,
    raw: Value,
) -> Vec<Effect> {
    let _ = (state, session_id, frame, raw);
    todo!("binary")
}

/// A new connection, for the reducer.
pub fn note_connection(state: &mut SessionState, generation: u64) -> Vec<Effect> {
    let _ = (state, generation);
    todo!("binary")
}

/// Keep a prompt typed before the session exists; the eleventh is refused with a notice.
pub fn hold_prompt(state: &mut AppState, text: &str) -> Result<(), String> {
    let _ = (state, text);
    todo!("binary")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
    Quit,
    Notice(String),
    NotSlash,
}

/// A line whose first character is `/` and whose second is a letter is a slash line.
#[must_use]
pub fn slash_line(text: &str) -> Slash {
    let _ = text;
    todo!("binary")
}

/// The immediate answer this client writes for a server request of kind `name`.
pub async fn answer_server_request(name: &str, params: Value) -> Result<Value, String> {
    let _ = (name, params);
    todo!("binary")
}

/// The notice for a link event the person must see.
#[must_use]
pub fn link_notice(event: &LinkEvent) -> Option<String> {
    let _ = event;
    todo!("binary")
}

/// Leave the session: untrack, detach without ending it, close the link. Returns the detach
/// outcome, the exit code and the text for standard output.
pub async fn quit(state: &mut AppState) -> (DetachOutcome, u8, String) {
    let _ = state;
    todo!("binary")
}

/// Quit while the create or resume is in flight.
pub async fn quit_while_opening(state: &mut AppState) -> (DetachOutcome, u8, String) {
    let _ = state;
    todo!("binary")
}

/// Stop the running turn now, waiting for the answer.
pub async fn interrupt(state: &mut AppState) -> ExecuteOutcome {
    let _ = state;
    todo!("binary")
}

/// Refuse a `--record` path that exists or whose folder does not, before any drawing.
pub fn check_record_path(path: &Path) -> Result<(), LaunchFailure> {
    let _ = path;
    todo!("binary")
}

/// Create the recording file (never overwriting one).
pub fn create_recorder(path: &Path) -> Result<Recorder, LaunchFailure> {
    let _ = path;
    todo!("binary")
}

/// Connect for the launch: discovery, profile, link.
pub async fn connect(launch: &Launch, home: &Path) -> Result<OpenConnection, LaunchFailure> {
    let _ = (launch, home);
    todo!("binary")
}

/// The `--resume` list: live rows, then stored rows when that call works, merged.
pub async fn list_sessions(
    connection: &OpenConnection,
    launch_id: &str,
) -> Result<Vec<ListRow>, LaunchFailure> {
    let _ = (connection, launch_id);
    todo!("binary")
}

/// Background start-up: connect (unless the `--resume` list already did), plan, open.
pub async fn connect_and_open(
    launch: Launch,
    home: PathBuf,
    launch_id: String,
    preopened: Option<OpenConnection>,
) -> Result<Ready, LaunchFailure> {
    let _ = (launch, home, launch_id, preopened);
    todo!("binary")
}

/// What the process prints after the terminal is restored, and its exit code.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

/// What `run` needs beyond the launch.
pub struct RunSetup {
    /// The Hermes home.
    pub home: PathBuf,
    pub launch_id: String,
    /// The connection the `--resume` list opened, reused here.
    pub preopened: Option<OpenConnection>,
    pub list_title: Option<String>,
    /// Configuration and key map notices, committed with the first frame.
    pub notices: Vec<String>,
}

/// Run one launch: start-up in the background, the loop, and the quit.
pub async fn run<S: Screen>(
    launch: Launch,
    setup: RunSetup,
    screen: &mut S,
    inputs: mpsc::UnboundedReceiver<S::Input>,
) -> Outcome {
    let _ = (launch, setup, screen, inputs);
    todo!("binary")
}
