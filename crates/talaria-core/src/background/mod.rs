//! Background processes and detached prompt tasks. Pure: no clock, no input or output,
//! no logging, no terminal library, and no client.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::side::RequestId;

/// How often a foreground session with a running process is listed, in milliseconds.
pub const PROCESS_POLL_MS: u64 = 5_000;
/// How often the open process view lists while its selection is running.
pub const OVERLAY_POLL_MS: u64 = 2_000;
/// How long a finished process stays in the live row.
pub const FINISHED_ROW_MS: u64 = 60_000;
/// Minimum gap between list requests caused by a chunk for an unknown id.
pub const UNKNOWN_CHUNK_LIST_MS: u64 = 1_000;
/// Output kept for one process, in Unicode scalar values.
pub const OUTPUT_CAP_CHARS: usize = 256_000;
/// Output kept for every process of one session.
pub const OUTPUT_TOTAL_CHARS: usize = 2_000_000;
/// Output buffers kept for one session.
pub const MAX_TRACKED_OUTPUTS: usize = 24;
/// Chunk buffers kept for ids that are not listed yet.
pub const MAX_PENDING_UNKNOWN: usize = 8;
/// Answers held while a task is still waiting for its reply.
pub const MAX_EARLY_RESULTS: usize = 16;
/// Longest poll interval after repeated list failures.
pub const MAX_POLL_BACKOFF_MS: u64 = 60_000;

/// One background process id.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(pub String);

/// What a listed process is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Not exited.
    Running,
    /// Exited with code 0.
    Done,
    /// Exited with a non-zero code.
    Failed { code: i64 },
    /// The process never started.
    FailedToStart,
    /// The process was killed.
    Killed,
    /// The gateway lost the process.
    Lost,
    /// Exited, and the exit code was not reported.
    Ended,
}

/// One process row as the gateway lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedProcess {
    /// Process id.
    pub id: ProcessId,
    /// Command text. Empty when the gateway omitted it.
    pub command: String,
    /// Status word from the gateway.
    pub status_raw: String,
    /// Exit code, when the gateway sent one.
    pub exit_code: Option<i64>,
    /// Start time in wall-clock seconds.
    pub started_epoch_s: Option<f64>,
    /// Exit time in wall-clock seconds.
    pub exited_at_s: Option<f64>,
    /// Why the process ended. Empty while it runs.
    pub completion_reason: Option<String>,
    /// Tail of the process output.
    pub output_tail: Option<String>,
}

impl ListedProcess {
    /// Read one gateway process entry. Missing fields take the empty defaults.
    #[must_use]
    pub fn from_entry(entry: &hermes_contract::schemas::ProcessEntry) -> ListedProcess {
        let _ = entry;
        todo!()
    }
}

/// Map one listed entry to a status.
#[must_use]
pub fn status_of(listed: &ListedProcess) -> ProcessStatus {
    let _ = listed;
    todo!()
}

/// A kill that is idle, in flight, or showing a note.
#[derive(Clone, Debug, PartialEq)]
pub enum KillState {
    /// No kill is in flight and no note is showing.
    Idle,
    /// A kill call is in flight.
    Sending { request: RequestId },
    /// The last kill left this note.
    Note { text: String },
}

/// One process the session is showing.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
    /// Process id.
    pub id: ProcessId,
    /// Command text.
    pub command: String,
    /// Derived status.
    pub status: ProcessStatus,
    /// Exit code, when known.
    pub exit_code: Option<i64>,
    /// Start time in wall-clock seconds.
    pub started_epoch_s: Option<f64>,
    /// Exit time in wall-clock seconds.
    pub exited_at_s: Option<f64>,
    /// When this row was first seen, in app milliseconds.
    pub first_seen_ms: u64,
    /// When the row left `Running`, in app milliseconds.
    pub finished_seen_ms: Option<u64>,
    /// How many lists in a row have omitted a running row.
    pub misses: u8,
    /// Kill progress for this row.
    pub kill: KillState,
    /// The agent hid a graphical view. The process keeps running.
    pub agent_closed_view: bool,
    /// The live row's last line. Recomputed on the clock only.
    pub live_line: String,
}

/// Output kept for one process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessOutput {
    raw: String,
    fence: String,
    trimmed_front: bool,
    last_write_ms: u64,
}

impl ProcessOutput {
    /// Append one output chunk and apply the per-process cap.
    pub fn push_chunk(&mut self, chunk: &str, now_ms: u64) {
        let _ = (chunk, now_ms);
        todo!()
    }

    /// Merge one polled tail into the buffer.
    pub fn merge_snapshot(&mut self, tail: &str, now_ms: u64) {
        let _ = (tail, now_ms);
        todo!()
    }

    /// The raw buffer.
    #[must_use]
    pub fn raw(&self) -> &str {
        todo!()
    }

    /// Whether the front of the output was dropped.
    #[must_use]
    pub fn trimmed_front(&self) -> bool {
        todo!()
    }

    /// The latest non-empty line, cleaned and cut to 200 characters.
    #[must_use]
    pub fn last_line(&self) -> String {
        todo!()
    }

    /// Cleaned text for drawing, at most `max_chars` characters.
    #[must_use]
    pub fn visible_tail(&self, max_chars: usize) -> String {
        let _ = max_chars;
        todo!()
    }
}

/// Where a detached prompt task is in its round trip.
#[derive(Clone, Debug, PartialEq)]
pub enum BgState {
    /// The start call has not been answered.
    Starting,
    /// The gateway accepted the task.
    Running { task_id: String, accepted_ms: u64 },
    /// The start call's outcome is unknown. The answer can still arrive.
    ReplyLost,
}

/// One detached prompt task started from this window.
#[derive(Clone, Debug, PartialEq)]
pub struct BgTask {
    /// Id of the start call.
    pub request: RequestId,
    /// Prompt text.
    pub text: String,
    /// When the command was sent, in app milliseconds.
    pub sent_ms: u64,
    /// Round-trip state.
    pub state: BgState,
}

/// Whether the last process list is missing, fresh, stale, or stopped.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ListState {
    /// No list has arrived.
    #[default]
    Never,
    /// The last list succeeded.
    Fresh { at_ms: u64 },
    /// The last list is being shown after a failure.
    Stale { since_ms: u64, why: String },
    /// Listing has stopped until the next attach.
    Stopped { why: ListStop },
}

/// Why listing stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListStop {
    /// The session is not live.
    SessionNotLive,
    /// The session runs in a managed worker.
    Unsupported,
    /// The gateway has no process list method.
    MethodMissing,
    /// This window may not list the session.
    PermissionDenied,
}

/// Counts later units can read. Nothing in this unit displays them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackgroundSummary {
    /// Running processes.
    pub running: usize,
    /// Finished processes still inside the live window.
    pub finished_recently: usize,
    /// Detached tasks still waiting.
    pub bg_running: usize,
}

/// What the overlay asked the application to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessesIntent {
    /// Kill the selected process.
    Kill(ProcessId),
    /// Stop every running process of this session.
    StopAll,
    /// Hide a finished process.
    Dismiss(ProcessId),
    /// List again.
    Refresh,
    /// Close the overlay.
    Close,
    /// The selection moved to this process.
    Select(ProcessId),
    /// The key did not ask for a call.
    None,
}

/// Background processes and detached tasks for one session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackgroundWork {
    rows: Vec<ProcessRow>,
    outputs: HashMap<ProcessId, ProcessOutput>,
    output_total_chars: usize,
    pending_unknown: Vec<(String, ProcessOutput)>,
    closed_view_ids: HashSet<String>,
    dismissed: HashSet<ProcessId>,
    selected: Option<ProcessId>,
    overlay_open: bool,
    overlay_note: Option<String>,
    foreground: bool,
    list_state: ListState,
    list_in_flight: Option<RequestId>,
    list_again: bool,
    last_list_sent_ms: Option<u64>,
    last_unknown_list_ms: Option<u64>,
    stop_in_flight: Option<(RequestId, StopOrigin)>,
    bg_tasks: Vec<BgTask>,
    early_results: VecDeque<(String, String)>,
    shown_task_ids: HashSet<String>,
    unknown_status_logged: bool,
    logged_list_reasons: HashSet<String>,
    consecutive_list_failures: u32,
    next_request: u64,
    version: u64,
}

impl BackgroundWork {
    /// Process rows in overlay order.
    pub fn rows(&self) -> impl Iterator<Item = &ProcessRow> {
        self.rows.iter()
    }

    /// Output for one process, when a buffer is kept.
    #[must_use]
    pub fn output(&self, id: &ProcessId) -> Option<&ProcessOutput> {
        self.outputs.get(id)
    }

    /// Detached tasks, oldest first.
    #[must_use]
    pub fn bg_tasks(&self) -> &[BgTask] {
        &self.bg_tasks
    }

    /// The list's freshness.
    #[must_use]
    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    /// The overlay's note row, when one is set.
    #[must_use]
    pub fn overlay_note(&self) -> Option<&str> {
        self.overlay_note.as_deref()
    }

    /// Whether the process overlay is open for this session.
    #[must_use]
    pub fn overlay_open(&self) -> bool {
        self.overlay_open
    }

    /// Counts for later units.
    #[must_use]
    pub fn summary(&self) -> BackgroundSummary {
        BackgroundSummary::default()
    }

    /// Rises by one exactly when something visible changed.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// One input to [`update`].
#[derive(Clone, Debug, PartialEq)]
pub enum BackgroundEvent {
    /// `agent.terminal.output`.
    TerminalOutput {
        process_id: String,
        chunk: String,
        now_ms: u64,
    },
    /// `terminal.close`. Hides a view and does not kill.
    TerminalClosed { process_id: String },
    /// A tool finished. The name decides whether to list.
    ToolCompleted { name: String, now_ms: u64 },
    /// `background.complete`.
    BgCompleted {
        task_id: String,
        text: String,
        now_ms: u64,
    },
    /// The app clock.
    Clock { now_ms: u64, wall_s: f64 },
    /// This session became or left the foreground.
    ForegroundChanged { foreground: bool, now_ms: u64 },
    /// A subscription exists again.
    Attached { now_ms: u64 },
    /// A process list arrived.
    Listed {
        request: RequestId,
        processes: Vec<ListedProcess>,
        now_ms: u64,
    },
    /// A process list failed.
    ListFailed {
        request: RequestId,
        failure: CallFailure,
        now_ms: u64,
    },
    /// A kill call finished.
    KillReplied {
        request: RequestId,
        process_id: ProcessId,
        outcome: KillOutcome,
        now_ms: u64,
    },
    /// A stop call finished.
    StopReplied {
        request: RequestId,
        outcome: StopOutcome,
        now_ms: u64,
    },
    /// A detached task was accepted.
    BgAccepted {
        request: RequestId,
        task_id: String,
        now_ms: u64,
    },
    /// A detached task was not accepted.
    BgFailed {
        request: RequestId,
        failure: CallFailure,
        now_ms: u64,
    },
    /// The person ran the background command.
    BgCommand { text: String, now_ms: u64 },
    /// The person ran stop.
    StopCommand { turn_running: bool, now_ms: u64 },
    /// The person confirmed a kill.
    KillRequested { id: ProcessId, now_ms: u64 },
    /// The person confirmed stop-all from the overlay.
    StopAllRequested { now_ms: u64 },
    /// The person dismissed a finished process.
    Dismiss { id: ProcessId },
    /// The overlay selection moved.
    Selected { id: ProcessId },
    /// The person asked for a fresh list.
    RefreshRequested { now_ms: u64 },
    /// The process overlay opened.
    OverlayOpened { now_ms: u64 },
    /// The process overlay closed.
    OverlayClosed,
}

/// Why a call did not return a usable result.
#[derive(Clone, Debug, PartialEq)]
pub enum CallFailure {
    /// The gateway refused the call.
    Refused {
        reason: String,
        detail: Option<String>,
    },
    /// The method is not on this gateway.
    MethodMissing,
    /// The gateway could not reach its store.
    Storage,
    /// The outcome is unknown.
    Unknown,
    /// Nothing was written.
    NotSent,
    /// The reply did not decode.
    Unreadable,
}

/// How a kill call ended.
#[derive(Clone, Debug, PartialEq)]
pub enum KillOutcome {
    /// The process was killed.
    Killed,
    /// The process had already exited.
    AlreadyExited { scope_stop_failed: bool },
    /// The gateway does not know the id.
    NotFound,
    /// The gateway reported an error string.
    Error { text: String },
    /// The call itself failed.
    Failed(CallFailure),
}

/// How a stop call ended.
#[derive(Clone, Debug, PartialEq)]
pub enum StopOutcome {
    /// The gateway stopped this many processes and delegations.
    Stopped { killed: i64, delegations: i64 },
    /// The call itself failed.
    Failed(CallFailure),
}

/// One follow-up from [`update`].
#[derive(Clone, Debug, PartialEq)]
pub enum BackgroundEffect {
    /// `process.list`.
    ListProcesses { request: RequestId },
    /// `process.kill`.
    KillProcess { request: RequestId, id: ProcessId },
    /// `process.stop`.
    StopProcesses { request: RequestId },
    /// `prompt.background`.
    StartBackground { request: RequestId, text: String },
    /// Stop the running turn through the session's interrupt path.
    Interrupt,
    /// A transcript notice.
    InsertNotice { level: NoticeLevel, text: String },
    /// A line for the application log. Never drawn.
    Log { text: String },
    /// One final result cell.
    InsertResult { cell: BackgroundCell },
    /// A signal for a later unit. Nothing here executes it.
    Signal(BackgroundSignal),
    /// The screen should redraw.
    Redraw,
}

/// How loud a notice is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLevel {
    /// Ordinary information.
    Info,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// Whether a stop was the command or the overlay key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopOrigin {
    /// The stop command.
    Command,
    /// The overlay key.
    Overlay,
}

/// A process or task finished.
#[derive(Clone, Debug, PartialEq)]
pub enum BackgroundSignal {
    /// A process left the running state.
    ProcessFinished {
        id: ProcessId,
        status: ProcessStatus,
    },
    /// A detached task produced a cell.
    BgFinished { task_id: String, failed: bool },
}

/// Where a detached task was started.
#[derive(Clone, Debug, PartialEq)]
pub enum BgOrigin {
    /// This window started it.
    Here,
    /// Another window started it, or this window had not attached yet.
    Elsewhere,
}

/// The body of a result cell.
#[derive(Clone, Debug, PartialEq)]
pub enum BgResult {
    /// The task's answer.
    Answer(String),
    /// The task failed. The string is the text after the error prefix.
    Failed(String),
}

/// One final cell for a detached task.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundCell {
    /// Gateway task id.
    pub task_id: String,
    /// Prompt text, when this window knows it.
    pub prompt: Option<String>,
    /// Where the task was started.
    pub origin: BgOrigin,
    /// Answer or failure.
    pub result: BgResult,
}

/// Apply one event. Reads no clock.
#[must_use]
pub fn update(work: &mut BackgroundWork, event: BackgroundEvent) -> Vec<BackgroundEffect> {
    let _ = (work, event);
    todo!()
}

/// The next app-clock millisecond when background work needs a wake-up.
#[must_use]
pub fn next_deadline(work: &BackgroundWork, now_ms: u64) -> Option<u64> {
    let _ = (work, now_ms);
    todo!()
}

/// Keep tasks, rows, and output across a session rebuild.
pub fn carry_over(old: &BackgroundWork, new: &mut BackgroundWork) {
    let _ = (old, new);
    todo!()
}

/// Whether a finished tool should refresh the process list.
#[must_use]
pub fn is_process_tool(name: &str) -> bool {
    let _ = name;
    todo!()
}

/// Whole seconds as `4s`, `3m05s`, or `1h02m`.
#[must_use]
pub fn format_elapsed(secs: u64) -> String {
    let _ = secs;
    todo!()
}

/// The sentence for a failed detached-task start.
#[must_use]
pub fn bg_failure_text(failure: &CallFailure) -> String {
    let _ = failure;
    todo!()
}

/// The list note for a stale or stopped list, when one should show.
#[must_use]
pub fn list_failure_text(state: &ListState, now_ms: u64) -> Option<String> {
    let _ = (state, now_ms);
    todo!()
}

/// The overlay note for one kill outcome.
#[must_use]
pub fn kill_note_text(outcome: &KillOutcome) -> String {
    let _ = outcome;
    todo!()
}

/// The notice level and text for one stop outcome.
#[must_use]
pub fn stop_notice_text(outcome: &StopOutcome) -> (NoticeLevel, String) {
    let _ = outcome;
    todo!()
}
