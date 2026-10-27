//! Scheduled jobs: list, form, run now, and history. Pure state, no terminal crate.

use std::collections::BTreeMap;
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

#[derive(Clone, Debug, PartialEq)]
pub struct CronState {
    profile: String,
    rest_available: Result<(), String>,
    open: bool,
    connected: bool,
    loaded: bool,
    jobs: Vec<JobRow>,
    warning: Option<String>,
    list_error: Option<String>,
    filter: String,
    selected: Option<String>,
    mode: Mode,
    question: Option<Question>,
    notice: Option<String>,
    seq: u64,
    list_in_flight: Option<u64>,
    queued_list: Option<u64>,
    fence: u64,
    write: Option<Write>,
    unknown_write: bool,
    runs: BTreeMap<String, Attempt>,
    deadlines: Deadlines,
    timer: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq)]
enum Mode {
    List,
    Form(form::Editor),
    Detail(Detail),
}
#[derive(Clone, Debug, PartialEq)]
enum Question {
    Discard,
    Remove(JobRow),
    Run(JobRow),
}
#[derive(Clone, Debug, PartialEq)]
enum Write {
    Add { name: String },
    Pause(String),
    Remove(String),
}
#[derive(Clone, Debug, PartialEq)]
enum AttemptStage {
    Waiting,
    Settle { seq: u64, not_found: bool },
}
#[derive(Clone, Debug, PartialEq)]
struct Attempt {
    name: String,
    baseline: Option<String>,
    clicked_wall: i64,
    clicked: Instant,
    pending: bool,
    stage: AttemptStage,
}
#[derive(Clone, Debug, PartialEq)]
struct Detail {
    job_id: String,
    focus: CardFocus,
    scroll: usize,
    runs: Option<Result<Vec<RunRow>, RestFailure>>,
    selected: usize,
    run_card: Option<String>,
    latest_seq: u64,
    in_flight: Option<u64>,
    queued: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
struct Deadlines {
    debounce: Option<Instant>,
    backstop: Option<Instant>,
    minute: Option<Instant>,
    history_poll: Option<Instant>,
    history_backstop: Option<Instant>,
    pending_expiry: Option<Instant>,
}

impl Deadlines {
    fn earliest(&self) -> Option<Instant> {
        [
            self.debounce,
            self.backstop,
            self.minute,
            self.history_poll,
            self.history_backstop,
            self.pending_expiry,
        ]
        .into_iter()
        .flatten()
        .min()
    }
}

impl CronState {
    pub fn new(profile: String, rest_available: Result<(), String>) -> CronState {
        Self {
            profile,
            rest_available,
            open: false,
            connected: true,
            loaded: false,
            jobs: Vec::new(),
            warning: None,
            list_error: None,
            filter: String::new(),
            selected: None,
            mode: Mode::List,
            question: None,
            notice: None,
            seq: 0,
            list_in_flight: None,
            queued_list: None,
            fence: 0,
            write: None,
            unknown_write: false,
            runs: BTreeMap::new(),
            deadlines: Deadlines::default(),
            timer: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn history_focused(&self) -> bool {
        self.open && matches!(&self.mode, Mode::Detail(d) if d.focus == CardFocus::History)
    }

    /// Keep transport availability current when the same profile reconnects.
    pub fn set_rest_status(&mut self, rest_available: Result<(), String>) {
        self.rest_available = rest_available;
        if self.rest_available.is_err() {
            self.deadlines.history_poll = None;
            self.deadlines.history_backstop = None;
            self.deadlines.pending_expiry = None;
            if let Mode::Form(editor) = &mut self.mode {
                editor.load_targets(Ok(Vec::new()));
            }
        }
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
    let mut effects = Vec::new();
    match input {
        CronInput::Open(mode) => {
            state.open = true;
            state.filter.clear();
            state.question = None;
            state.notice = None;
            state.mode = Mode::List;
            state.deadlines.minute = Some(next_minute(now, wall_unix));
            state.deadlines.backstop = Some(now + Duration::from_secs(60));
            state.request_list(&mut effects);
            if mode == CronOpen::NewJob {
                state.new_form(&mut effects);
            }
            state.fix_selection(None);
        }
        CronInput::Close => state.close(),
        CronInput::Key(key) if state.open => state.key(key, now, wall_unix, &mut effects),
        CronInput::Key(_) => {}
        CronInput::Changed if state.open => {
            if state.deadlines.debounce.is_none() {
                state.deadlines.debounce = Some(now + Duration::from_millis(300));
            }
        }
        CronInput::Changed => {}
        CronInput::Tick => state.tick(now, wall_unix, &mut effects),
        CronInput::Connection { up } => {
            state.connected = up;
            if up && (state.open || state.unknown_write) {
                state.unknown_write = false;
                state.request_list(&mut effects);
            }
        }
        CronInput::Listed { seq, result } => state.listed(seq, result, now, &mut effects),
        CronInput::Added(result) => {
            if matches!(state.write, Some(Write::Add { .. }))
                && let Some(Write::Add { name }) = state.write.take()
            {
                match result {
                    Ok(reply) => {
                        if let Some(job) = reply.job {
                            state.jobs.push(job);
                            list::sort_jobs(&mut state.jobs);
                        }
                        state.mode = Mode::List;
                        state.filter.clear();
                        state.selected = Some(reply.job_id);
                        let notice = with_next(
                            text::clean(&reply.message),
                            reply.next_run_at.as_deref(),
                            wall_unix,
                            false,
                        );
                        state.notify(notice, &mut effects);
                    }
                    Err(CallFailure::Tool {
                        job_saved: true,
                        error,
                        job_id,
                        ..
                    }) => {
                        let id = job_id.unwrap_or_default();
                        let name = if name.is_empty() { &id } else { &name };
                        state.mode = Mode::List;
                        state.filter.clear();
                        state.selected = Some(id.clone());
                        state.notify(saved_notice(name, &id, &error), &mut effects);
                    }
                    Err(failure) => {
                        state.note_unknown(&failure);
                        let reason = text::call_reason(&failure, &state.profile);
                        if let Mode::Form(editor) = &mut state.mode {
                            editor.locked = false;
                            editor.status = if let CallFailure::Tool { error, .. } = &failure {
                                let mut lines = vec!["Hermes did not create the job:".into()];
                                lines.extend(split_and_sanitise(error, 8));
                                lines
                            } else {
                                vec![reason.clone()]
                            };
                        }
                        state.notify(reason, &mut effects);
                    }
                }
                state.after_write(&mut effects);
            }
        }
        CronInput::PauseSet { job_id, result } => {
            if matches!(&state.write,Some(Write::Pause(id)) if *id == job_id) {
                state.write = None;
                match result {
                    Ok(reply) => {
                        let job = reply.job;
                        let notice = if job.state == "paused" {
                            format!("Paused {}.", job_name(&job))
                        } else {
                            with_next(
                                format!("Resumed {}.", job_name(&job)),
                                job.next_run_at.as_deref(),
                                wall_unix,
                                true,
                            )
                        };
                        let old = state.selected_position();
                        if job.job_id == job_id
                            && let Some(row) = state.jobs.iter_mut().find(|j| j.job_id == job_id)
                        {
                            *row = job;
                        }
                        list::sort_jobs(&mut state.jobs);
                        state.fix_selection(old);
                        state.notify(notice, &mut effects);
                    }
                    Err(f) => {
                        state.note_unknown(&f);
                        state.notify(text::call_reason(&f, &state.profile), &mut effects);
                    }
                }
                state.after_write(&mut effects);
            }
        }
        CronInput::Removed { job_id, result } => {
            if matches!(&state.write,Some(Write::Remove(id)) if *id == job_id) {
                state.write = None;
                match result {
                    Ok(reply) => {
                        let name = if !reply.name.is_empty() {
                            text::clean(&reply.name)
                        } else {
                            state
                                .jobs
                                .iter()
                                .find(|j| j.job_id == job_id)
                                .map_or_else(|| text::clean(&job_id), job_name)
                        };
                        let old = state.selected_position();
                        state.jobs.retain(|j| j.job_id != job_id);
                        if matches!(&state.mode,Mode::Detail(d) if d.job_id == job_id) {
                            state.close_detail();
                        }
                        state.fix_selection(old);
                        state.notify(format!("Removed {name}."), &mut effects);
                    }
                    Err(f) => {
                        state.note_unknown(&f);
                        state.notify(text::call_reason(&f, &state.profile), &mut effects);
                    }
                }
                state.after_write(&mut effects);
            }
        }
        CronInput::Triggered { job_id, outcome } => {
            state.triggered(job_id, outcome, now, &mut effects)
        }
        CronInput::RunsLoaded {
            job_id,
            seq,
            result,
        } => state.runs_loaded(job_id, seq, result, now, &mut effects),
        CronInput::TargetsLoaded(result) => {
            if let Mode::Form(editor) = &mut state.mode {
                editor.load_targets(result);
            }
        }
    }
    let earliest = state.deadlines.earliest();
    if earliest != state.timer {
        state.timer = earliest;
        if let Some(deadline) = earliest {
            effects.push(CronEffect::ScheduleTick {
                after: deadline.saturating_duration_since(now),
            });
        }
    }
    effects
}

pub fn view(state: &CronState, wall_unix: i64) -> CronView {
    state.view(wall_unix)
}

fn next_minute(now: Instant, wall: i64) -> Instant {
    now + Duration::from_secs((60 - wall.rem_euclid(60)) as u64)
}
fn job_name(job: &JobRow) -> String {
    text::clean(if job.name.is_empty() {
        &job.job_id
    } else {
        &job.name
    })
}
fn with_next(mut notice: String, next: Option<&str>, wall: i64, resumed: bool) -> String {
    if let Some(next) = next.and_then(parse_iso_instant) {
        if resumed && next.unix.saturating_sub(wall) <= 60 {
            notice.push_str(" Next run: due now");
        } else {
            notice.push_str(&format!(
                " Next run: {} ({}).",
                next.wall,
                relative(wall, next.unix)
            ));
        }
    }
    notice
}
fn saved_notice(name: &str, id: &str, error: &str) -> String {
    format!(
        "Hermes saved {} ({}) but could not register it with its scheduler: {}. Do not create it again; check the list.",
        text::clean(name),
        text::clean(id),
        text::cut(error, 400)
    )
}

impl CronState {
    fn next_seq(&mut self) -> u64 {
        self.seq = self.seq.saturating_add(1);
        self.seq
    }
    fn request_list(&mut self, effects: &mut Vec<CronEffect>) -> u64 {
        if let Some(seq) = self.queued_list {
            return seq;
        }
        let seq = self.next_seq();
        if self.list_in_flight.is_some() {
            self.queued_list = Some(seq);
        } else if self.connected {
            self.list_in_flight = Some(seq);
            effects.push(CronEffect::List { seq });
        } else {
            self.unknown_write = true;
        }
        seq
    }
    fn after_write(&mut self, effects: &mut Vec<CronEffect>) {
        self.fence = self.request_list(effects);
    }
    fn note_unknown(&mut self, failure: &CallFailure) {
        if *failure == CallFailure::Unknown {
            self.unknown_write = true;
        }
    }
    fn notify(&mut self, notice: String, effects: &mut Vec<CronEffect>) {
        if self.open {
            self.notice = Some(notice);
        } else {
            effects.push(CronEffect::Notice(notice));
        }
    }
    fn close(&mut self) {
        self.open = false;
        self.question = None;
        self.deadlines = Deadlines::default();
        self.timer = None;
        if self.write.is_none() {
            self.mode = Mode::List;
        }
    }
    fn close_detail(&mut self) {
        self.mode = Mode::List;
        self.deadlines.history_backstop = None;
        self.deadlines.history_poll = None;
        self.deadlines.pending_expiry = None;
    }
    fn new_form(&mut self, effects: &mut Vec<CronEffect>) {
        if self.write.is_some() {
            self.notify("Waiting for Hermes.".into(), effects);
            return;
        }
        self.close_detail();
        self.mode = Mode::Form(form::Editor::new());
        if self.rest_available.is_ok() {
            effects.push(CronEffect::DeliveryTargets);
        }
    }
    fn visible_jobs(&self) -> Vec<&JobRow> {
        let words: Vec<_> = self
            .filter
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.jobs
            .iter()
            .filter(|j| {
                let haystack = format!(
                    "{} {} {} {}",
                    j.name, j.schedule, j.prompt_preview, j.deliver
                )
                .to_lowercase();
                words.iter().all(|w| haystack.contains(w))
            })
            .collect()
    }
    fn selected_position(&self) -> Option<usize> {
        let rows = self.visible_jobs();
        self.selected
            .as_ref()
            .and_then(|id| rows.iter().position(|j| j.job_id == *id))
    }
    fn fix_selection(&mut self, old: Option<usize>) {
        let rows = self.visible_jobs();
        if self
            .selected
            .as_ref()
            .is_some_and(|id| rows.iter().any(|j| j.job_id == *id))
        {
            return;
        }
        self.selected = rows
            .get(
                old.unwrap_or(0)
                    .saturating_sub(usize::from(old.is_some()))
                    .min(rows.len().saturating_sub(1)),
            )
            .map(|j| j.job_id.clone());
    }
    fn selected_job(&self) -> Option<&JobRow> {
        let id = match &self.mode {
            Mode::Detail(d) => Some(&d.job_id),
            _ => self.selected.as_ref(),
        }?;
        self.jobs.iter().find(|j| &j.job_id == id)
    }
    fn move_selection(&mut self, key: &CronKey) {
        let rows = self.visible_jobs();
        let pos = self.selected_position().unwrap_or(0);
        let next = navigate(pos, rows.len(), key);
        self.selected = rows.get(next).map(|j| j.job_id.clone());
    }
    fn listed(
        &mut self,
        seq: u64,
        result: Result<ListReply, CallFailure>,
        now: Instant,
        effects: &mut Vec<CronEffect>,
    ) {
        if self.list_in_flight != Some(seq) {
            return;
        }
        self.list_in_flight = None;
        if self.open {
            self.deadlines.backstop = Some(now + Duration::from_secs(60));
        }
        if seq >= self.fence {
            match &result {
                Ok(reply) => {
                    let old = self.selected_position();
                    self.jobs = reply.jobs.clone();
                    list::sort_jobs(&mut self.jobs);
                    self.warning = reply.warning.clone();
                    self.list_error = None;
                    self.loaded = true;
                    if let Mode::Detail(d) = &self.mode
                        && !self.jobs.iter().any(|j| j.job_id == d.job_id)
                    {
                        let name = text::clean(&d.job_id);
                        self.close_detail();
                        self.notify(
                            format!(
                                "{name} is no longer in profile {}.",
                                text::clean(&self.profile)
                            ),
                            effects,
                        );
                    }
                    self.fix_selection(old);
                }
                Err(f) => {
                    self.list_error = Some(text::call_reason(f, &self.profile));
                }
            }
        }
        let settled: Vec<_> = self
            .runs
            .iter()
            .filter_map(|(id, run)| match run.stage {
                AttemptStage::Settle { seq: minimum, .. }
                    if seq >= minimum && seq >= self.fence =>
                {
                    Some(id.clone())
                }
                _ => None,
            })
            .collect();
        for id in settled {
            if let Some(run) = self.runs.remove(&id) {
                let message = match &result {
                    Err(f) => format!(
                        "Talaria could not check the result of the run of {}: {}. ctrl+r reloads.",
                        text::clean(&run.name),
                        text::call_reason(f, &self.profile)
                    ),
                    Ok(reply) => {
                        let job = reply.jobs.iter().find(|j| j.job_id == id);
                        if matches!(
                            run.stage,
                            AttemptStage::Settle {
                                not_found: true,
                                ..
                            }
                        ) {
                            if job.is_some() {
                                format!(
                                    "Hermes answered 404 for the run of {}, but the job is still listed; this Hermes may not serve run now.",
                                    text::clean(&run.name)
                                )
                            } else {
                                format!(
                                    "{} is no longer in profile {}.",
                                    text::clean(&run.name),
                                    text::clean(&self.profile)
                                )
                            }
                        } else {
                            run_now::result_notice(&run.name, &run.baseline, job)
                        }
                    }
                };
                self.notify(message, effects);
            }
        }
        self.refresh_pending(now);
        if let Some(seq) = self.queued_list.take() {
            if self.connected {
                self.list_in_flight = Some(seq);
                effects.push(CronEffect::List { seq });
            } else {
                self.unknown_write = true;
            }
        }
    }
}

fn navigate(pos: usize, len: usize, key: &CronKey) -> usize {
    match key {
        CronKey::Up => pos.saturating_sub(1),
        CronKey::Down => (pos + 1).min(len.saturating_sub(1)),
        CronKey::PageUp => pos.saturating_sub(10),
        CronKey::PageDown => pos.saturating_add(10).min(len.saturating_sub(1)),
        CronKey::Home => 0,
        CronKey::End => len.saturating_sub(1),
        _ => pos,
    }
}

impl CronState {
    fn key(&mut self, key: CronKey, now: Instant, wall: i64, effects: &mut Vec<CronEffect>) {
        if matches!(&self.mode,Mode::Form(e) if e.locked) {
            return;
        }
        if self.question.is_some() {
            match key {
                CronKey::Char('n') | CronKey::Esc => self.question = None,
                CronKey::Char('y') => match self.question.take() {
                    Some(Question::Discard) => self.mode = Mode::List,
                    Some(Question::Remove(job)) => {
                        if self.write.is_some() {
                            self.notify("Waiting for Hermes.".into(), effects);
                        } else {
                            self.write = Some(Write::Remove(job.job_id.clone()));
                            effects.push(CronEffect::Remove { job_id: job.job_id });
                        }
                    }
                    Some(Question::Run(job)) => {
                        if let Err(reason) = &self.rest_available {
                            self.notify(format!("Run now needs Hermes's dashboard routes, which are not available here: {}.", text::clean(reason)), effects);
                        } else if self.runs.contains_key(&job.job_id) {
                            self.notify(
                                format!("{} is already running from here.", job_name(&job)),
                                effects,
                            );
                        } else {
                            let name = job_name(&job);
                            self.runs.insert(
                                job.job_id.clone(),
                                Attempt {
                                    name: name.clone(),
                                    baseline: job.last_run_at,
                                    clicked_wall: wall,
                                    clicked: now,
                                    pending: true,
                                    stage: AttemptStage::Waiting,
                                },
                            );
                            effects.push(CronEffect::Trigger {
                                job_id: job.job_id,
                                name,
                            });
                            self.refresh_pending(now);
                        }
                    }
                    None => {}
                },
                _ => {}
            }
            return;
        }
        if matches!(&self.mode, Mode::Form(_)) {
            match key {
                CronKey::Esc => {
                    if matches!(&self.mode,Mode::Form(e) if e.dirty()) {
                        self.question = Some(Question::Discard);
                    } else {
                        self.mode = Mode::List;
                    }
                }
                CronKey::Action(CronAction::Save) => {
                    if self.write.is_some() {
                        self.notify("Waiting for Hermes.".into(), effects);
                    } else if let Mode::Form(editor) = &mut self.mode {
                        match validate_form(&editor.form) {
                            Ok(request) => {
                                editor.errors.clear();
                                editor.status = vec!["Saving…".into()];
                                editor.locked = true;
                                self.write = Some(Write::Add {
                                    name: request.name.clone().unwrap_or_default(),
                                });
                                effects.push(CronEffect::Add(request));
                            }
                            Err(errors) => editor.errors = errors,
                        }
                    }
                }
                _ => {
                    if let Mode::Form(editor) = &mut self.mode {
                        editor.edit(key);
                    }
                }
            }
            return;
        }
        match key {
            CronKey::Action(CronAction::New) if matches!(self.mode, Mode::List) => {
                self.new_form(effects)
            }
            CronKey::Action(CronAction::Reload) => {
                self.request_list(effects);
                self.request_runs(effects);
            }
            CronKey::Action(CronAction::TogglePause) | CronKey::Action(CronAction::Remove) => {
                if self.write.is_some() {
                    self.notify("Waiting for Hermes.".into(), effects);
                    return;
                }
                let Some(job) = self.selected_job().cloned() else {
                    return;
                };
                if key == CronKey::Action(CronAction::Remove) {
                    self.question = Some(Question::Remove(job));
                } else if job.state == "completed" {
                    self.notify(
                        format!(
                            "{} has finished; it cannot be paused or resumed.",
                            job_name(&job)
                        ),
                        effects,
                    );
                } else {
                    self.write = Some(Write::Pause(job.job_id.clone()));
                    effects.push(CronEffect::SetPaused {
                        job_id: job.job_id,
                        paused: job.state != "paused",
                    });
                }
            }
            CronKey::Action(CronAction::RunNow) => {
                if let Err(reason) = &self.rest_available {
                    self.notify(format!("Run now needs Hermes's dashboard routes, which are not available here: {}.",text::clean(reason)),effects);
                } else if let Some(job) = self.selected_job().cloned() {
                    if self.runs.contains_key(&job.job_id) {
                        self.notify(
                            format!("{} is already running from here.", job_name(&job)),
                            effects,
                        );
                    } else {
                        self.question = Some(Question::Run(job));
                    }
                }
            }
            CronKey::Action(CronAction::OpenRun) if self.history_focused() => {
                let row = if let Mode::Detail(d) = &self.mode {
                    d.runs
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .and_then(|rows| rows.get(d.selected))
                        .cloned()
                } else {
                    None
                };
                if let Some(row) = row
                    && matches!(run_status(&row), RunStatus::Finished(_))
                {
                    effects.push(CronEffect::OpenSession { session_id: row.id });
                } else {
                    self.notify("Only a finished agent run can be opened.".into(), effects);
                }
            }
            CronKey::Action(_) => {}
            CronKey::Esc => {
                if let Mode::Detail(d) = &mut self.mode {
                    if d.run_card.is_some() {
                        d.run_card = None;
                    } else {
                        self.close_detail();
                    }
                } else if !self.filter.is_empty() {
                    self.filter.clear();
                    self.fix_selection(None);
                } else {
                    self.close();
                    effects.push(CronEffect::CloseOverlay);
                }
            }
            CronKey::Enter if matches!(self.mode, Mode::List) => {
                if let Some(job) = self.selected_job() {
                    self.mode = Mode::Detail(Detail {
                        job_id: job.job_id.clone(),
                        focus: CardFocus::Fields,
                        scroll: 0,
                        runs: None,
                        selected: 0,
                        run_card: None,
                        latest_seq: 0,
                        in_flight: None,
                        queued: false,
                    });
                    self.request_runs(effects);
                    self.refresh_pending(now);
                }
            }
            _ if matches!(self.mode, Mode::Detail(_)) => self.detail_key(key, wall),
            CronKey::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.fix_selection(None);
            }
            CronKey::Paste(text) => {
                self.filter
                    .push_str(&text.replace("\r\n", " ").replace(['\r', '\n'], " "));
                self.fix_selection(None);
            }
            CronKey::Backspace => {
                self.filter.pop();
                self.fix_selection(None);
            }
            _ => self.move_selection(&key),
        }
    }
    fn detail_key(&mut self, key: CronKey, wall: i64) {
        let field_lines = self.selected_job().map_or(0, |job| {
            self.detail_fields(job, wall)
                .iter()
                .map(|(_, lines)| lines.len() + 1)
                .sum()
        });
        let Mode::Detail(d) = &mut self.mode else {
            return;
        };
        if d.run_card.is_some() {
            return;
        }
        match key {
            CronKey::Tab | CronKey::BackTab => {
                d.focus = if d.focus == CardFocus::Fields {
                    CardFocus::History
                } else {
                    CardFocus::Fields
                }
            }
            CronKey::Enter if d.focus == CardFocus::History => {
                if let Some(Ok(rows)) = &d.runs
                    && let Some(row) = rows.get(d.selected)
                {
                    d.run_card = Some(row.id.clone());
                }
            }
            _ if d.focus == CardFocus::History => {
                let len = d
                    .runs
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map_or(0, Vec::len);
                d.selected = navigate(d.selected, len, &key);
            }
            _ => {
                d.scroll = navigate(d.scroll, field_lines, &key);
            }
        }
    }
    fn request_runs(&mut self, effects: &mut Vec<CronEffect>) {
        if self.rest_available.is_err() || !self.open {
            return;
        }
        let Mode::Detail(d) = &mut self.mode else {
            return;
        };
        if d.in_flight.is_some() {
            d.queued = true;
            return;
        }
        let job_id = d.job_id.clone();
        let seq = self.next_seq();
        if let Mode::Detail(d) = &mut self.mode {
            d.in_flight = Some(seq);
            d.latest_seq = seq;
        }
        effects.push(CronEffect::Runs { job_id, seq });
    }
    fn runs_loaded(
        &mut self,
        job_id: String,
        seq: u64,
        result: Result<Vec<RunRow>, RestFailure>,
        now: Instant,
        effects: &mut Vec<CronEffect>,
    ) {
        if !self.open {
            return;
        }
        let Mode::Detail(d) = &mut self.mode else {
            return;
        };
        if d.job_id != job_id || d.latest_seq != seq || d.in_flight != Some(seq) {
            return;
        }
        d.in_flight = None;
        if seq >= self.fence {
            let old_id = d
                .runs
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|rows| rows.get(d.selected))
                .map(|r| r.id.clone());
            if let Ok(rows) = &result {
                if let Some(attempt) = self.runs.get_mut(&job_id)
                    && rows.iter().any(|r| {
                        r.started_at
                            .is_some_and(|s| s >= attempt.clicked_wall.saturating_sub(2) as f64)
                    })
                {
                    attempt.pending = false;
                }
                d.selected = old_id
                    .and_then(|id| rows.iter().position(|r| r.id == id))
                    .unwrap_or(d.selected)
                    .min(rows.len().saturating_sub(1));
                if d.run_card
                    .as_ref()
                    .is_some_and(|id| !rows.iter().any(|r| r.id == *id))
                {
                    d.run_card = None;
                }
            }
            d.runs = Some(result);
            self.deadlines.history_backstop = Some(now + Duration::from_secs(60));
        } else {
            d.queued = true;
        }
        let queued = d.queued;
        d.queued = false;
        self.refresh_pending(now);
        if queued {
            self.request_runs(effects);
        }
    }
    fn refresh_pending(&mut self, now: Instant) {
        self.deadlines.history_poll = None;
        self.deadlines.pending_expiry = None;
        if !self.open {
            return;
        }
        if let Mode::Detail(d) = &self.mode
            && let Some(attempt) = self.runs.get_mut(&d.job_id)
        {
            let expires = attempt.clicked + Duration::from_secs(90);
            if now >= expires {
                attempt.pending = false;
            }
            if attempt.pending && self.rest_available.is_ok() {
                self.deadlines.history_poll = Some(now + Duration::from_secs(2));
                self.deadlines.pending_expiry = Some(expires);
            }
        }
    }
    fn triggered(
        &mut self,
        job_id: String,
        outcome: TriggerOutcome,
        now: Instant,
        effects: &mut Vec<CronEffect>,
    ) {
        if !self
            .runs
            .get(&job_id)
            .is_some_and(|r| matches!(r.stage, AttemptStage::Waiting))
        {
            return;
        }
        match outcome {
            TriggerOutcome::Accepted | TriggerOutcome::Failed(RestFailure::NotFound) => {
                let not_found = matches!(outcome, TriggerOutcome::Failed(RestFailure::NotFound));
                let seq = self.request_list(effects);
                if let Some(run) = self.runs.get_mut(&job_id) {
                    run.stage = AttemptStage::Settle { seq, not_found };
                }
            }
            TriggerOutcome::Failed(failure) => {
                if let Some(run) = self.runs.remove(&job_id) {
                    let name = text::clean(&run.name);
                    let notice = match &failure {
                        RestFailure::Refused {
                            status: 409,
                            detail,
                        } => format!("{name} did not run: {}.", text::cut(detail, 200)),
                        RestFailure::Unknown => {
                            self.unknown_write = true;
                            format!(
                                "Talaria lost the reply for the run of {name}; it may still be running. Its history will show the result."
                            )
                        }
                        RestFailure::NotSent(reason) => format!(
                            "Could not ask Hermes to run {name}: {}.",
                            text::clean(reason)
                        ),
                        _ => format!("Hermes did not run {name}: {}.", rest_reason(&failure)),
                    };
                    self.notify(notice, effects);
                }
                self.refresh_pending(now);
            }
        }
    }
    fn tick(&mut self, now: Instant, wall: i64, effects: &mut Vec<CronEffect>) {
        if !self.open {
            return;
        }
        if self.deadlines.pending_expiry.is_some_and(|d| d <= now) {
            self.refresh_pending(now);
        }
        if self.deadlines.debounce.is_some_and(|d| d <= now) {
            self.deadlines.debounce = None;
            self.request_list(effects);
            self.request_runs(effects);
        }
        if self.deadlines.backstop.is_some_and(|d| d <= now) {
            self.deadlines.backstop = None;
            self.request_list(effects);
        }
        if self.deadlines.minute.is_some_and(|d| d <= now) {
            self.deadlines.minute = Some(next_minute(now, wall));
        }
        let history_due = self.deadlines.history_backstop.is_some_and(|d| d <= now)
            || self.deadlines.history_poll.is_some_and(|d| d <= now);
        if history_due {
            if self.deadlines.history_backstop.is_some_and(|d| d <= now) {
                self.deadlines.history_backstop = None;
            }
            self.request_runs(effects);
            self.refresh_pending(now);
        }
    }
}

impl CronState {
    fn view(&self, wall: i64) -> CronView {
        let body = match &self.mode {
            Mode::Form(editor) => CronBody::Form(editor.view()),
            Mode::Detail(detail) => {
                if let Some(job) = self.jobs.iter().find(|j| j.job_id == detail.job_id) {
                    let pending = self
                        .runs
                        .get(&detail.job_id)
                        .filter(|r| r.pending)
                        .map(|r| {
                            format!("starting… (requested {})", relative(wall, r.clicked_wall))
                        });
                    let runs = if let Err(reason) = &self.rest_available {
                        RunsView::Unavailable(format!(
                            "Run history needs Hermes's dashboard routes, which are not available here: {}.",
                            text::clean(reason)
                        ))
                    } else {
                        match (&detail.runs, pending) {
                            (Some(Err(f)), None) => RunsView::Failed(format!(
                                "Could not load the run history: {}. ctrl+r tries again.",
                                rest_reason(f)
                            )),
                            (None, None) => RunsView::Loading,
                            (Some(Ok(rows)), None) if rows.is_empty() => RunsView::Empty,
                            (result, pending) => {
                                let rows: Vec<_> = result
                                    .as_ref()
                                    .and_then(|r| r.as_ref().ok())
                                    .map_or_else(Vec::new, |rows| {
                                        rows.iter().map(|r| history::row_view(r, wall)).collect()
                                    });
                                let selected = (!rows.is_empty())
                                    .then_some(detail.selected.min(rows.len().saturating_sub(1)));
                                RunsView::Rows {
                                    pending,
                                    rows,
                                    selected,
                                }
                            }
                        }
                    };
                    let run_card = detail
                        .run_card
                        .as_ref()
                        .and_then(|id| {
                            detail
                                .runs
                                .as_ref()
                                .and_then(|r| r.as_ref().ok())
                                .and_then(|rows| rows.iter().find(|r| r.id == *id))
                        })
                        .map(|r| history::run_card(r, wall));
                    CronBody::Detail(DetailView {
                        fields: self.detail_fields(job, wall),
                        runs,
                        run_card,
                        focus: detail.focus,
                        fields_scroll: detail.scroll,
                    })
                } else {
                    self.list_body(wall)
                }
            }
            Mode::List => self.list_body(wall),
        };
        let question = match &self.question {
            Some(Question::Discard) => {
                Some("Discard this new job?  y discard · n keep editing".into())
            }
            Some(Question::Remove(job)) => Some(format!(
                "Remove {} from profile {}? This cannot be undone.  y remove · n cancel",
                job_name(job),
                text::clean(&self.profile)
            )),
            Some(Question::Run(job)) => Some(run_now::confirmation(job, &self.profile)),
            None => None,
        };
        let hints = match &self.mode {
            Mode::Form(_) => "tab next field · ctrl+s save · esc cancel",
            Mode::Detail(d) if d.run_card.is_some() => "esc back",
            Mode::Detail(_) => "tab fields/runs · enter run details · o open run · ctrl+t run now · ctrl+e pause/resume · ctrl+d remove · ctrl+r reload · esc back",
            Mode::List => "enter details · ctrl+n new · ctrl+t run now · ctrl+e pause/resume · ctrl+d remove · ctrl+r reload · esc close",
        }.into();
        CronView {
            title: format!("Scheduled jobs · profile {}", text::clean(&self.profile)),
            banner: self.warning.as_ref().map(|s| text::clean(s)),
            body,
            notice: self.notice.as_ref().map(|s| text::clean(s)),
            question,
            hints,
        }
    }
    fn list_body(&self, wall: i64) -> CronBody {
        if !self.connected {
            return CronBody::Message(text::NOT_CONNECTED.into());
        }
        let status = self
            .list_error
            .as_ref()
            .map(|reason| format!("Could not load scheduled jobs: {reason}. ctrl+r tries again."));
        if self.jobs.is_empty() {
            return CronBody::Message(status.unwrap_or_else(|| {
                if self.loaded {
                    format!(
                        "No scheduled jobs in profile {}. ctrl+n creates one.",
                        text::clean(&self.profile)
                    )
                } else {
                    "Loading scheduled jobs…".into()
                }
            }));
        }
        let jobs = self.visible_jobs();
        let rows: Vec<_> = jobs
            .iter()
            .map(|job| RowView {
                job_id: text::clean(&job.job_id),
                name: job_name(job),
                schedule: text::clean(&job.schedule),
                state: self.state_label(job, wall),
                last_run: last_run_label(job, wall),
            })
            .collect();
        let selected = self
            .selected
            .as_ref()
            .and_then(|id| jobs.iter().position(|j| j.job_id == *id));
        let status = status.or_else(|| {
            rows.is_empty()
                .then(|| format!("No job matches \"{}\".", text::clean(&self.filter)))
        });
        CronBody::List(ListView {
            filter: (!self.filter.is_empty()).then(|| text::clean(&self.filter)),
            status,
            rows,
            selected,
        })
    }
    fn state_label(&self, job: &JobRow, wall: i64) -> String {
        if self.runs.contains_key(&job.job_id) {
            "running now (started here)".into()
        } else {
            job_state_label(job, wall)
        }
    }
    fn detail_fields(&self, job: &JobRow, wall: i64) -> Vec<(String, Vec<String>)> {
        let mut result = Vec::new();
        let mut field = |label: &str, value: &str, max| {
            result.push((label.into(), split_and_sanitise(value, max)))
        };
        field("Name", &job_name(job), 1);
        field("Id", &job.job_id, 1);
        field("Schedule", &job.schedule, 1);
        field("Repeat", &job.repeat, 1);
        field("Deliver", &job.deliver, 1);
        field("State", &self.state_label(job, wall), 1);
        if let Some(paused) = &job.paused_at {
            field("Paused", &wall_text(paused, wall), 1);
        }
        if let Some(reason) = &job.paused_reason {
            field("Paused reason", reason, 6);
        }
        if let Some(next) = &job.next_run_at {
            field("Next run", &wall_text(next, wall), 1);
        }
        if job.last_run_at.is_some() {
            let last = last_run_label(job, wall);
            let value = if last.is_empty() {
                job.last_run_at.as_deref().unwrap_or("").into()
            } else {
                last
            };
            field("Last run", &value, 1);
        }
        if let Some(error) = &job.last_error {
            field("Last error", error, 6);
        }
        if let Some(error) = &job.last_delivery_error {
            field("Last delivery error", error, 6);
        }
        if let Some(error) = &job.last_fire_error {
            if let Some(at) = &error.at {
                field("Last fire error at", &wall_text(at, wall), 1);
            }
            field("Last fire error", &error.detail, 6);
        }
        field("Prompt preview", &job.prompt_preview, 6);
        if !job.skills.is_empty() {
            field("Skills", &job.skills.join(", "), 6);
        }
        if let Some(model) = &job.model {
            field(
                "Model",
                &format!("{}{}", model, if job.pinned { " (pinned)" } else { "" }),
                1,
            );
        }
        if let Some(provider) = &job.provider {
            field("Provider", provider, 1);
        }
        if let Some(workdir) = &job.workdir {
            field("Workdir", workdir, 6);
        }
        let tags: Vec<_> = [
            (job.script, "script"),
            (job.no_agent, "no agent"),
            (job.continuity, "continuity"),
        ]
        .into_iter()
        .filter_map(|(set, text)| set.then_some(text))
        .collect();
        if !tags.is_empty() {
            field("Tags", &tags.join(", "), 1);
        }
        result
    }
}

fn wall_text(raw: &str, wall: i64) -> String {
    parse_iso_instant(raw).map_or_else(
        || text::clean(raw),
        |date| format!("{} ({})", date.wall, relative(wall, date.unix)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALL: i64 = 1_792_713_600;

    fn job(id: &str, name: &str) -> JobRow {
        JobRow {
            job_id: id.into(),
            name: name.into(),
            schedule: "every 2h".into(),
            repeat: "forever".into(),
            deliver: "local".into(),
            next_run_at: None,
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_delivery_error: None,
            last_fire_error: None,
            enabled: true,
            state: "scheduled".into(),
            paused_at: None,
            paused_reason: None,
            prompt_preview: "Check the result.".into(),
            skills: Vec::new(),
            model: None,
            provider: None,
            pinned: false,
            workdir: None,
            script: false,
            no_agent: false,
            continuity: false,
        }
    }
    fn send(state: &mut CronState, input: CronInput, now: Instant) -> Vec<CronEffect> {
        update(state, input, now, WALL)
    }
    fn key(state: &mut CronState, key: CronKey, now: Instant) -> Vec<CronEffect> {
        send(state, CronInput::Key(key), now)
    }
    fn list_sequence(effects: &[CronEffect]) -> u64 {
        effects
            .iter()
            .find_map(|e| {
                if let CronEffect::List { seq } = e {
                    Some(*seq)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn runs_sequence(effects: &[CronEffect]) -> u64 {
        effects
            .iter()
            .find_map(|e| {
                if let CronEffect::Runs { seq, .. } = e {
                    Some(*seq)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn listed(state: &mut CronState, seq: u64, rows: Vec<JobRow>, now: Instant) -> Vec<CronEffect> {
        send(
            state,
            CronInput::Listed {
                seq,
                result: Ok(ListReply {
                    jobs: rows,
                    warning: None,
                }),
            },
            now,
        )
    }
    fn opened(rows: Vec<JobRow>, now: Instant) -> CronState {
        let mut state = CronState::new("sample".into(), Ok(()));
        let seq = list_sequence(&send(&mut state, CronInput::Open(CronOpen::List), now));
        listed(&mut state, seq, rows, now);
        state
    }
    fn start_run(state: &mut CronState, now: Instant) {
        key(state, CronKey::Action(CronAction::RunNow), now);
        assert!(
            key(state, CronKey::Char('y'), now)
                .iter()
                .any(|e| matches!(e, CronEffect::Trigger { .. }))
        );
    }

    #[test]
    fn stale_list_cannot_undo_write_or_replace_newer_request() {
        let now = Instant::now();
        let base = job("one", "Alpha");
        let mut state = opened(vec![base.clone()], now);
        let old = list_sequence(&key(&mut state, CronKey::Action(CronAction::Reload), now));
        key(&mut state, CronKey::Action(CronAction::TogglePause), now);
        let mut paused = base.clone();
        paused.state = "paused".into();
        paused.enabled = false;
        let response = send(
            &mut state,
            CronInput::PauseSet {
                job_id: "one".into(),
                result: Ok(JobReply { job: paused }),
            },
            now,
        );
        assert!(
            !response
                .iter()
                .any(|e| matches!(e, CronEffect::List { .. }))
        );
        let next = list_sequence(&listed(&mut state, old, vec![base.clone()], now));
        assert_eq!(state.jobs[0].state, "paused");
        assert!(next > old);
        listed(&mut state, old, vec![base.clone()], now);
        assert_eq!(state.list_in_flight, Some(next));
        listed(&mut state, next, vec![base], now);
        assert_eq!(state.jobs[0].state, "scheduled");
    }

    #[test]
    fn changed_storm_reserves_one_list_after_the_current_reply() {
        let now = Instant::now();
        let mut state = CronState::new("sample".into(), Ok(()));
        send(&mut state, CronInput::Open(CronOpen::List), now);
        for ms in [0, 50, 100] {
            send(
                &mut state,
                CronInput::Changed,
                now + Duration::from_millis(ms),
            );
        }
        let tick = send(
            &mut state,
            CronInput::Tick,
            now + Duration::from_millis(300),
        );
        assert!(!tick.iter().any(|e| matches!(e, CronEffect::List { .. })));
        let queued = state.queued_list;
        send(
            &mut state,
            CronInput::Changed,
            now + Duration::from_millis(301),
        );
        send(
            &mut state,
            CronInput::Tick,
            now + Duration::from_millis(601),
        );
        assert_eq!(state.queued_list, queued);
        let fresh = listed(&mut state, 1, Vec::new(), now + Duration::from_millis(601));
        assert_eq!(
            fresh
                .iter()
                .filter(|e| matches!(e, CronEffect::List { .. }))
                .count(),
            1
        );
        send(&mut state, CronInput::Close, now);
        assert!(send(&mut state, CronInput::Changed, now).is_empty());
        assert!(send(&mut state, CronInput::Tick, now + Duration::from_secs(100)).is_empty());
    }

    #[test]
    fn each_run_waits_for_its_own_fresh_list_after_close() {
        let now = Instant::now();
        let a = job("a", "Alpha");
        let b = job("b", "Beta");
        let mut state = opened(vec![a.clone(), b.clone()], now);
        start_run(&mut state, now);
        key(&mut state, CronKey::Down, now);
        start_run(&mut state, now);
        let first = list_sequence(&send(
            &mut state,
            CronInput::Triggered {
                job_id: "a".into(),
                outcome: TriggerOutcome::Accepted,
            },
            now,
        ));
        let second_effects = send(
            &mut state,
            CronInput::Triggered {
                job_id: "b".into(),
                outcome: TriggerOutcome::Accepted,
            },
            now,
        );
        assert!(
            !second_effects
                .iter()
                .any(|e| matches!(e, CronEffect::List { .. }))
        );
        send(&mut state, CronInput::Close, now);
        let mut finished_a = a;
        finished_a.last_run_at = Some("2026-10-23T09:01Z".into());
        finished_a.last_status = Some("ok".into());
        let response = listed(&mut state, first, vec![finished_a.clone(), b.clone()], now);
        assert!(response.contains(&CronEffect::Notice("Ran Alpha: finished ok.".into())));
        assert!(state.runs.contains_key("b"));
        let second = list_sequence(&response);
        let mut finished_b = b;
        finished_b.last_run_at = Some("2026-10-23T09:02Z".into());
        finished_b.last_status = Some("blocked_config".into());
        let response = listed(&mut state, second, vec![finished_a, finished_b], now);
        assert!(response.contains(&CronEffect::Notice("Ran Beta: blocked config.".into())));
        assert!(state.runs.is_empty());
    }

    #[test]
    fn confirmations_accept_only_their_own_answer_and_keep_paused_warning() {
        let now = Instant::now();
        let mut a = job("a", "Alpha");
        a.state = "paused".into();
        let mut state = opened(vec![a, job("b", "Beta")], now);
        state.selected = Some("a".into());
        key(&mut state, CronKey::Action(CronAction::RunNow), now);
        let before = view(&state, WALL).question;
        assert!(
            before
                .as_deref()
                .unwrap()
                .contains("also resumes its schedule")
        );
        for k in [
            CronKey::Down,
            CronKey::Action(CronAction::TogglePause),
            CronKey::Action(CronAction::Remove),
        ] {
            assert!(key(&mut state, k, now).is_empty());
            assert_eq!(view(&state, WALL).question, before);
        }
        key(&mut state, CronKey::Esc, now);
        assert!(key(&mut state, CronKey::Char('y'), now).is_empty());
        assert_eq!(state.filter, "y");
        assert!(state.runs.is_empty());
    }

    #[test]
    fn unknown_add_keeps_fields_and_never_replays_when_connection_returns() {
        let now = Instant::now();
        let mut state = CronState::new("sample".into(), Ok(()));
        let seq = list_sequence(&send(&mut state, CronInput::Open(CronOpen::NewJob), now));
        listed(&mut state, seq, Vec::new(), now);
        for k in [
            CronKey::Paste("Alpha".into()),
            CronKey::Tab,
            CronKey::Paste("every 2h".into()),
            CronKey::Tab,
            CronKey::Paste("Check.".into()),
        ] {
            key(&mut state, k, now);
        }
        let saved = key(&mut state, CronKey::Action(CronAction::Save), now);
        assert_eq!(
            saved
                .iter()
                .filter(|e| matches!(e, CronEffect::Add(_)))
                .count(),
            1
        );
        assert!(key(&mut state, CronKey::Esc, now).is_empty());
        send(&mut state, CronInput::Connection { up: false }, now);
        send(&mut state, CronInput::Added(Err(CallFailure::Unknown)), now);
        let response = send(&mut state, CronInput::Connection { up: true }, now);
        assert!(
            response
                .iter()
                .any(|e| matches!(e, CronEffect::List { .. }))
        );
        assert!(!response.iter().any(|e| matches!(e, CronEffect::Add(_))));
        let Mode::Form(editor) = &state.mode else {
            panic!("form retained");
        };
        assert!(!editor.locked);
        assert_eq!(editor.form.prompt, "Check.");
        assert_eq!(state.notice.as_deref(), Some(text::UNKNOWN_WRITE));
    }

    #[test]
    fn history_ignores_old_sequence_and_stops_fast_poll_on_matching_run() {
        let now = Instant::now();
        let mut state = opened(vec![job("a", "Alpha")], now);
        let first = runs_sequence(&key(&mut state, CronKey::Enter, now));
        send(
            &mut state,
            CronInput::RunsLoaded {
                job_id: "a".into(),
                seq: first,
                result: Ok(Vec::new()),
            },
            now,
        );
        start_run(&mut state, now);
        let next = runs_sequence(&send(
            &mut state,
            CronInput::Tick,
            now + Duration::from_secs(2),
        ));
        let row = RunRow {
            id: "run".into(),
            source: "cron".into(),
            title: None,
            preview: None,
            started_at: Some((WALL - 2) as f64),
            ended_at: None,
            end_reason: None,
            input_tokens: 0,
            output_tokens: 0,
            message_count: 0,
            tool_call_count: 0,
            model: None,
            is_active: true,
            scheduler_owned: true,
        };
        send(
            &mut state,
            CronInput::RunsLoaded {
                job_id: "a".into(),
                seq: first,
                result: Ok(vec![row.clone()]),
            },
            now + Duration::from_secs(2),
        );
        assert!(state.runs["a"].pending);
        send(
            &mut state,
            CronInput::RunsLoaded {
                job_id: "a".into(),
                seq: next,
                result: Ok(vec![row]),
            },
            now + Duration::from_secs(2),
        );
        assert!(!state.runs["a"].pending);
        assert!(state.deadlines.history_poll.is_none());
        assert!(state.deadlines.pending_expiry.is_none());
        assert!(state.deadlines.history_backstop.is_some());
    }

    #[test]
    fn history_deadlines_survive_other_jobs_failure_and_expire_after_ninety_seconds() {
        let now = Instant::now();
        let mut state = opened(vec![job("a", "Alpha"), job("b", "Beta")], now);
        start_run(&mut state, now);
        key(&mut state, CronKey::Down, now);
        start_run(&mut state, now);
        key(&mut state, CronKey::Up, now);
        key(&mut state, CronKey::Enter, now);
        send(
            &mut state,
            CronInput::Triggered {
                job_id: "b".into(),
                outcome: TriggerOutcome::Failed(RestFailure::Server(500)),
            },
            now,
        );
        assert!(state.deadlines.history_poll.is_some());
        send(&mut state, CronInput::Tick, now + Duration::from_secs(90));
        assert!(!state.runs["a"].pending);
        assert!(state.deadlines.history_poll.is_none());
        assert!(state.deadlines.pending_expiry.is_none());
    }
}
