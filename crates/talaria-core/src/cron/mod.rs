//! Scheduled jobs: list, form, run now, and history. Pure state, no terminal crate.

use std::time::{Duration, Instant};

mod form;
mod history;
mod list;
mod run_now;
mod text;
mod time;

pub use form::validate_form;
pub use history::{run_status, run_status_text};
pub use list::{job_state_label, last_run_label};
pub use run_now::stale_run_notice;
pub use text::{parse_arguments, rest_reason, split_and_sanitise};
pub use time::{duration_text, parse_iso_instant, relative};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CronOpen {
    List,
    NewJob,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsoInstant {
    pub unix: i64,
    pub wall: String,
}

#[allow(dead_code)]
pub struct CronState {
    profile: String,
}

impl CronState {
    pub fn new(profile: String, rest_available: Result<(), String>) -> CronState {
        let _ = (profile, rest_available);
        todo!()
    }

    pub fn is_open(&self) -> bool {
        todo!()
    }

    pub fn history_focused(&self) -> bool {
        todo!()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CronAction {
    New,
    RunNow,
    TogglePause,
    Remove,
    Reload,
    Save,
    OpenRun,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CronKey {
    Action(CronAction),
    Char(char),
    Paste(String),
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Tab,
    BackTab,
    Esc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CronInput {
    Open(CronOpen),
    Key(CronKey),
    Listed {
        seq: u64,
        result: Result<ListReply, CallFailure>,
    },
    Added(Result<AddReply, CallFailure>),
    PauseSet {
        job_id: String,
        result: Result<JobReply, CallFailure>,
    },
    Removed {
        job_id: String,
        result: Result<RemoveReply, CallFailure>,
    },
    Triggered {
        job_id: String,
        outcome: TriggerOutcome,
    },
    RunsLoaded {
        job_id: String,
        seq: u64,
        result: Result<Vec<RunRow>, RestFailure>,
    },
    TargetsLoaded(Result<Vec<DeliveryTarget>, RestFailure>),
    Changed,
    Tick,
    Connection {
        up: bool,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CronEffect {
    List { seq: u64 },
    Add(AddRequest),
    SetPaused { job_id: String, paused: bool },
    Remove { job_id: String },
    Trigger { job_id: String, name: String },
    Runs { job_id: String, seq: u64 },
    DeliveryTargets,
    OpenSession { session_id: String },
    Notice(String),
    ScheduleTick { after: Duration },
    CloseOverlay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListReply {
    pub jobs: Vec<JobRow>,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddReply {
    pub job_id: String,
    pub message: String,
    pub next_run_at: Option<String>,
    pub job: Option<JobRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobReply {
    pub job: JobRow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoveReply {
    pub job_id: String,
    pub name: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRow {
    pub job_id: String,
    pub name: String,
    pub schedule: String,
    pub repeat: String,
    pub deliver: String,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
    pub last_delivery_error: Option<String>,
    pub last_fire_error: Option<FireError>,
    pub enabled: bool,
    pub state: String,
    pub paused_at: Option<String>,
    pub paused_reason: Option<String>,
    pub prompt_preview: String,
    pub skills: Vec<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub pinned: bool,
    pub workdir: Option<String>,
    pub script: bool,
    pub no_agent: bool,
    pub continuity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FireError {
    pub at: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunRow {
    pub id: String,
    pub source: String,
    pub title: Option<String>,
    pub preview: Option<String>,
    pub started_at: Option<f64>,
    pub ended_at: Option<f64>,
    pub end_reason: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub message_count: u64,
    pub tool_call_count: u64,
    pub model: Option<String>,
    pub is_active: bool,
    pub scheduler_owned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Finished(Option<String>),
    Unclosed,
    Output(Option<String>),
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryTarget {
    pub id: String,
    pub name: String,
    pub home_target_set: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormField {
    Name,
    Schedule,
    Prompt,
    Repeat,
    Deliver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddForm {
    pub name: String,
    pub schedule: String,
    pub prompt: String,
    pub repeat: String,
    pub deliver: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    pub field: FormField,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddRequest {
    pub name: Option<String>,
    pub schedule: String,
    pub prompt: String,
    pub repeat: Option<u32>,
    pub deliver: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFailure {
    Tool {
        error: String,
        job_saved: bool,
        job_id: Option<String>,
        matches: usize,
    },
    Rpc {
        code: i64,
        message: String,
    },
    Unknown,
    NotSent(String),
    BadResult(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestFailure {
    Forbidden(String),
    NotFound,
    Refused { status: u16, detail: String },
    Server(u16),
    Unauthorized,
    Unknown,
    NotSent(String),
    BadBody(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerOutcome {
    Accepted,
    Failed(RestFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronView {
    pub title: String,
    pub banner: Option<String>,
    pub body: CronBody,
    pub notice: Option<String>,
    pub question: Option<String>,
    pub hints: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CronBody {
    Message(String),
    List(ListView),
    Detail(DetailView),
    Form(FormView),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListView {
    pub filter: Option<String>,
    pub status: Option<String>,
    pub rows: Vec<RowView>,
    pub selected: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowView {
    pub job_id: String,
    pub name: String,
    pub schedule: String,
    pub state: String,
    pub last_run: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailView {
    pub fields: Vec<(String, Vec<String>)>,
    pub runs: RunsView,
    pub run_card: Option<Vec<(String, Vec<String>)>>,
    pub focus: CardFocus,
    pub fields_scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardFocus {
    Fields,
    History,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunsView {
    Loading,
    Unavailable(String),
    Failed(String),
    Empty,
    Rows {
        pending: Option<String>,
        rows: Vec<RunRowView>,
        selected: Option<usize>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRowView {
    pub start: String,
    pub status: String,
    pub duration: String,
    pub tokens: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormView {
    pub fields: Vec<FieldView>,
    pub focus: FormField,
    pub targets: Vec<(String, bool)>,
    pub deliver_index: usize,
    pub status: Vec<String>,
    pub locked: bool,
    pub cursor: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldView {
    pub field: FormField,
    pub label: String,
    pub value: String,
    pub hint: Option<String>,
    pub error: Option<String>,
}

pub fn update(
    state: &mut CronState,
    input: CronInput,
    now: Instant,
    wall_unix: i64,
) -> Vec<CronEffect> {
    let _ = (state, input, now, wall_unix);
    todo!()
}

pub fn view(state: &CronState, wall_unix: i64) -> CronView {
    let _ = (state, wall_unix);
    todo!()
}
