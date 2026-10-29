//! Rewind overlay state. Tests drive it through `update`. Issue 233.

use serde_json::Value;

use crate::session::NoticeKind;

use super::checkpoints::{Checkpoint, CheckpointList, NoMatch, checkpoint_for_turn, clock_text};
use super::text::{self, DiffDoc, RestoreOutcome};
use super::turns::{RewindTurn, is_active_row, lossless_text, rewind_turns};

pub use super::text::{CallFailure, CallKind};

/// One gateway call the application performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// Read the session.
    ReadSession,
    /// List checkpoints.
    ListCheckpoints,
    /// Diff one checkpoint.
    CheckpointDiff { hash: String },
    /// Diff since the oldest kept checkpoint.
    SessionDiff,
    /// Rewind the conversation.
    Mutate {
        row_id: i64,
        expected_revision: i64,
        expected_generation: i64,
    },
    /// Restore files.
    Restore {
        hash: String,
        file_path: Option<String>,
        all: bool,
    },
}

/// A fresh session read.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRead {
    /// The raw resume result.
    pub raw: Value,
    /// `revision`.
    pub revision: i64,
    /// `execution_generation`.
    pub execution_generation: i64,
    /// A turn is running.
    pub running: bool,
    /// Pending entries whose status is queued, started, or unknown.
    pub pending: usize,
    /// Raw message rows.
    pub messages: Vec<Value>,
}

/// A committed rewind.
#[derive(Debug, Clone, PartialEq)]
pub struct MutateOutcome {
    /// New revision, when the reply carried one.
    pub revision: Option<i64>,
    /// Rows deactivated, the target included.
    pub rewound_count: u64,
    /// `target_message.content`, when present.
    pub target_content: Option<Value>,
}

/// A reply for one ticket.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// A session read.
    SessionRead(SessionRead),
    /// A checkpoint list.
    Checkpoints(CheckpointList),
    /// A diff document.
    DiffLoaded(DiffDoc),
    /// A committed rewind.
    Mutated(MutateOutcome),
    /// A restore result.
    Restored(RestoreOutcome),
    /// The call failed.
    Failed(CallFailure),
}

/// A key the overlay understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Esc,
    Close,
    C,
    F,
    D,
    R,
}

/// Which screen is showing. `Working` from a typed command is not an open overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindScreen {
    Closed,
    Loading,
    Turns,
    Actions,
    Confirm,
    Checkpoints,
    CheckpointConfirm,
    Diff,
    Working,
}

/// Where a diff was opened from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// `/diff`.
    Session { stat_only: bool },
    /// A checkpoint target.
    Checkpoint(String),
}

/// One input to [`update`].
#[derive(Debug, Clone, PartialEq)]
pub enum RewindInput {
    /// `/rewind`, `/checkpoint`, or Esc twice.
    Open { turn_running: bool },
    /// `/rollback` with no target.
    OpenCheckpoints,
    /// `/diff` or `/rollback diff`.
    OpenDiff(DiffSource),
    /// `/undo`.
    Undo { n: usize, turn_running: bool },
    /// `/retry`.
    Retry { turn_running: bool },
    /// `/rollback <target>`.
    TypedRestore {
        target: String,
        file_path: Option<String>,
        all: bool,
        turn_running: bool,
    },
    /// A key.
    Key(RewindKey),
    /// How many rows the renderer can show.
    Viewport(u16),
    /// A gateway reply.
    Reply { ticket: u64, reply: Reply },
    /// A `session.updated` frame.
    SessionUpdated {
        operation: String,
        revision: Option<i64>,
        rewound_count: Option<u64>,
    },
}

/// What the application should do after `update`.
#[derive(Debug, Clone, PartialEq)]
pub enum RewindEffect {
    /// Perform one call.
    Call { ticket: u64, call: Call },
    /// Replace the foreground session and commit `notice`.
    Rebuild {
        snapshot: Value,
        notice: Vec<String>,
    },
    /// Put the prompt back into an empty composer.
    ReturnPrompt { text: String, attachments: bool },
    /// Submit this text with a fresh submission id.
    Submit(String),
    /// Show a notice.
    Notice(NoticeKind, String),
}

/// How a body line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Normal,
    Dim,
    Warning,
    Accent,
}

/// One row of the turn or checkpoint list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewRow<'a> {
    /// The row is selected.
    pub mark: bool,
    /// `#3` or the checkpoint index.
    pub label: String,
    /// Relative age, padded by the renderer.
    pub age: String,
    /// `HH:MM`, or empty.
    pub clock: String,
    /// Preview or checkpoint message.
    pub text: &'a str,
    /// `files`, `·`, or empty.
    pub tail: &'static str,
}

/// One body line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    /// The line's text.
    pub text: String,
    /// How to draw it.
    pub style: LineStyle,
}

/// Everything the overlay draw reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewindView<'a> {
    /// Current screen.
    pub screen: RewindScreen,
    /// Title line.
    pub title: String,
    /// Turn or checkpoint rows.
    pub rows: Vec<ViewRow<'a>>,
    /// Selected index.
    pub selected: usize,
    /// First row the viewport shows.
    pub first_visible: usize,
    /// Body lines.
    pub body: Vec<ViewLine>,
    /// Diff document, when the screen is a diff.
    pub diff: Option<&'a DiffDoc>,
    /// First diff line the viewport shows.
    pub diff_scroll: usize,
    /// Footer, the last row.
    pub footer: String,
}

/// Rows the list shows when the renderer has not said.
const DEFAULT_VIEWPORT_ROWS: usize = 10;

/// Lines of the prompt the Actions screen shows.
const PROMPT_LINES: usize = 6;

/// Why the overlay is loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Rewind,
    Checkpoints,
    TypedRestore,
    TypedDiff,
}

/// What a confirm screen will do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Conversation,
    Files,
}

/// Where Esc leaves a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Back {
    Actions,
    Checkpoints,
    Close,
}

/// One step of a write or its follow-up reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    ConfirmRead,
    Mutating {
        row_id: i64,
        sent_generation: i64,
        after_restore: bool,
    },
    Restoring {
        whole_folder: bool,
    },
    AfterRestoreRead,
    RebuildRead,
    Verifying {
        row_id: i64,
        sent_generation: i64,
        after_restore: bool,
    },
    LostRestoreRead,
    ExternalRebuildRead,
}

/// Which command or screen started the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Conversation,
    Files,
    Undo,
    Retry,
    CheckpointRestore,
    TypedRestore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Closed,
    Loading(Purpose),
    Turns,
    Actions,
    Confirm(Kind),
    Checkpoints,
    CheckpointConfirm,
    Diff {
        back: Back,
        loading: bool,
    },
    Working(Step),
}

/// What the loaded session read said about the session.
#[derive(Debug, Clone, Copy, Default)]
struct LoadedRead {
    running: bool,
    pending: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct PendingExternal {
    revision: Option<i64>,
    removed: Option<u64>,
    is_rewind: bool,
    read_after_event: Option<u64>,
}

impl LoadedRead {
    fn blocked(self) -> bool {
        self.running || self.pending > 0
    }
}

/// A typed `/rollback` or `/rollback diff` target.
#[derive(Debug, Clone)]
struct Typed {
    target: String,
    file_path: Option<String>,
    all: bool,
}

/// The checkpoint a screen names.
#[derive(Debug, Clone)]
struct ChosenCheckpoint {
    checkpoint: Checkpoint,
    oldest_kept: bool,
}

/// Overlay state. The default is closed.
#[derive(Debug, Default)]
pub struct RewindState {
    mode: Mode,
    origin: Option<Origin>,
    undo_n: usize,
    next_ticket: u64,
    awaited: Option<(u64, CallKind)>,
    viewport: Option<usize>,
    loaded: LoadedRead,
    turns: Vec<RewindTurn>,
    checkpoints: Option<CheckpointList>,
    selected: usize,
    first_visible: usize,
    chosen: Option<RewindTurn>,
    chosen_checkpoint: Option<ChosenCheckpoint>,
    changed_warning: bool,
    confirm_turns: Vec<RewindTurn>,
    confirm_revision: Option<i64>,
    typed: Option<Typed>,
    restore: Option<RestoreOutcome>,
    mutation: Option<MutateOutcome>,
    older_removed: Option<String>,
    diff_doc: Option<DiffDoc>,
    diff_title: String,
    diff_stat_only: Option<bool>,
    diff_scroll: usize,
    external_removed: Option<u64>,
    external_is_rewind: bool,
    pending_external: Option<PendingExternal>,
    external_sequence: u64,
    read_sequence: u64,
    reload_pending: bool,
    last_rebuild_revision: Option<i64>,
}

fn warn(text: impl Into<String>) -> RewindEffect {
    RewindEffect::Notice(NoticeKind::Warning, text.into())
}

fn system(text: impl Into<String>) -> RewindEffect {
    RewindEffect::Notice(NoticeKind::System, text.into())
}

fn view_line(text: impl Into<String>, style: LineStyle) -> ViewLine {
    ViewLine {
        text: text.into(),
        style,
    }
}

static OFF_LIST: CheckpointList = CheckpointList::Off;

impl RewindState {
    /// Cancel session-owned work without reusing tickets from earlier sessions.
    pub fn change_session(&mut self) {
        *self = Self {
            next_ticket: self.next_ticket,
            ..Self::default()
        };
    }

    /// An overlay is showing. A typed command's working step is not an overlay.
    #[must_use]
    pub fn is_open(&self) -> bool {
        match self.mode {
            Mode::Closed => false,
            Mode::Working(Step::ExternalRebuildRead) => false,
            Mode::Working(_) => matches!(
                self.origin,
                Some(Origin::Conversation | Origin::Files | Origin::CheckpointRestore)
            ),
            _ => true,
        }
    }

    /// The current screen.
    #[must_use]
    pub fn screen(&self) -> RewindScreen {
        match self.mode {
            Mode::Closed => RewindScreen::Closed,
            Mode::Loading(_) => RewindScreen::Loading,
            Mode::Turns => RewindScreen::Turns,
            Mode::Actions => RewindScreen::Actions,
            Mode::Confirm(_) => RewindScreen::Confirm,
            Mode::Checkpoints => RewindScreen::Checkpoints,
            Mode::CheckpointConfirm => RewindScreen::CheckpointConfirm,
            Mode::Diff { .. } => RewindScreen::Diff,
            Mode::Working(_) => RewindScreen::Working,
        }
    }

    /// The view to draw at `now_ms`.
    #[must_use]
    pub fn view(&self, now_ms: u64) -> RewindView<'_> {
        let mut view = RewindView {
            screen: self.screen(),
            title: String::new(),
            rows: Vec::new(),
            selected: 0,
            first_visible: 0,
            body: Vec::new(),
            diff: None,
            diff_scroll: 0,
            footer: String::new(),
        };
        match self.mode {
            Mode::Closed => {}
            Mode::Loading(purpose) => self.fill_loading(&mut view, purpose),
            Mode::Turns => self.fill_turns(&mut view, now_ms),
            Mode::Actions => self.fill_actions(&mut view, now_ms),
            Mode::Confirm(kind) => self.fill_confirm(&mut view, kind, now_ms),
            Mode::Checkpoints => self.fill_checkpoints(&mut view, now_ms),
            Mode::CheckpointConfirm => self.fill_checkpoint_confirm(&mut view),
            Mode::Diff { loading, .. } => self.fill_diff(&mut view, loading),
            Mode::Working(_) => {
                view.title = match self.origin {
                    Some(Origin::CheckpointRestore) => text::CHECKPOINTS_TITLE,
                    _ => text::TURNS_TITLE,
                }
                .to_string();
                view.body = vec![view_line(text::WORKING, LineStyle::Dim)];
            }
        }
        view
    }

    fn list(&self) -> &CheckpointList {
        self.checkpoints.as_ref().unwrap_or(&OFF_LIST)
    }

    fn fill_loading(&self, view: &mut RewindView<'_>, purpose: Purpose) {
        let (title, body) = match purpose {
            Purpose::Rewind => (text::TURNS_TITLE.to_string(), text::LOADING_CONVERSATION),
            Purpose::Checkpoints | Purpose::TypedRestore => (
                text::CHECKPOINTS_TITLE.to_string(),
                text::LOADING_CONVERSATION,
            ),
            Purpose::TypedDiff => (self.diff_title.clone(), text::COMPARING_FILES),
        };
        view.title = title;
        view.body = vec![view_line(body, LineStyle::Dim)];
        view.footer = text::LOADING_FOOTER.to_string();
    }

    fn fill_turns<'a>(&'a self, view: &mut RewindView<'a>, now_ms: u64) {
        view.title = text::TURNS_TITLE.to_string();
        let list = self.list();
        view.rows = self
            .turns
            .iter()
            .enumerate()
            .map(|(index, turn)| ViewRow {
                mark: index == self.selected,
                label: format!("#{}", turn.ordinal),
                age: turn_age(turn, now_ms),
                clock: String::new(),
                text: &turn.preview,
                tail: match checkpoint_for_turn(turn, list) {
                    Ok(_) => "files",
                    Err(_) => match list {
                        CheckpointList::Rows(_) | CheckpointList::Empty => "·",
                        CheckpointList::Off | CheckpointList::Unavailable(_) => "",
                    },
                },
            })
            .collect();
        view.selected = self.selected;
        view.first_visible = self.first_visible;
        let note = match list {
            CheckpointList::Off => Some(text::FOOTER_CHECKPOINTS_OFF.to_string()),
            CheckpointList::Unavailable(reason) => Some(text::footer_checkpoints_unknown(reason)),
            CheckpointList::Rows(_) | CheckpointList::Empty => None,
        };
        view.footer = text::turns_footer(note.as_deref());
    }

    fn fill_actions(&self, view: &mut RewindView<'_>, now_ms: u64) {
        let Some(chosen) = &self.chosen else {
            return;
        };
        view.title = text::actions_title(chosen.ordinal, &turn_age(chosen, now_ms));
        let prompt = chosen.prompt_text.as_deref().unwrap_or(&chosen.text_parts);
        let mut lines: Vec<&str> = prompt.lines().take(PROMPT_LINES).collect();
        if lines.iter().all(|line| line.trim().is_empty()) {
            lines = vec![chosen.preview.as_str()];
        }
        for line in lines {
            view.body.push(view_line(line, LineStyle::Normal));
        }
        view.body.push(view_line("", LineStyle::Normal));
        view.body
            .push(view_line(text::ACTION_REWIND, LineStyle::Normal));
        match self.match_for(chosen) {
            Ok(found) => {
                let checkpoint = &found.checkpoint;
                view.body.push(view_line(
                    text::action_restore_line(
                        &checkpoint.short,
                        &clock_text(&checkpoint.raw_time),
                        &checkpoint.message,
                    ),
                    LineStyle::Normal,
                ));
                view.body
                    .push(view_line(text::ACTION_DIFF, LineStyle::Normal));
            }
            Err(reason) => view.body.push(view_line(
                text::action_restore_unavailable(text::no_match_text(&reason)),
                LineStyle::Dim,
            )),
        }
        view.body.push(view_line(text::KEYS_BACK, LineStyle::Dim));
    }

    fn fill_confirm(&self, view: &mut RewindView<'_>, kind: Kind, now_ms: u64) {
        let Some(chosen) = &self.chosen else {
            return;
        };
        view.title = text::actions_title(chosen.ordinal, &turn_age(chosen, now_ms));
        view.body.push(view_line(
            text::confirm_remove_line(chosen.ordinal, chosen.later_count),
            LineStyle::Normal,
        ));
        match (kind, &self.chosen_checkpoint) {
            (Kind::Files, Some(found)) => {
                let checkpoint = &found.checkpoint;
                let clock = clock_text(&checkpoint.raw_time);
                view.body.push(view_line(
                    text::confirm_files_line(&checkpoint.short, &clock, &checkpoint.message),
                    LineStyle::Normal,
                ));
                if found.oldest_kept {
                    view.body.push(view_line(
                        text::oldest_checkpoint_warning(&clock),
                        LineStyle::Warning,
                    ));
                }
                view.body.push(view_line(
                    text::CHECKPOINT_LIMITS_WARNING.replace("<n>", &checkpoint.index.to_string()),
                    LineStyle::Warning,
                ));
            }
            _ => view
                .body
                .push(view_line(text::FILES_STAY, LineStyle::Normal)),
        }
        if self.changed_warning {
            view.body.push(view_line(
                text::CONVERSATION_CHANGED_WARNING,
                LineStyle::Warning,
            ));
        }
        if self.loaded.blocked() {
            view.body.push(view_line(
                text::confirm_blocked_line(self.loaded.pending),
                LineStyle::Warning,
            ));
            view.body.push(view_line(text::KEYS_BACK, LineStyle::Dim));
        } else {
            view.body
                .push(view_line(text::KEYS_CONFIRM, LineStyle::Dim));
        }
    }

    fn fill_checkpoints<'a>(&'a self, view: &mut RewindView<'a>, now_ms: u64) {
        view.title = text::CHECKPOINTS_TITLE.to_string();
        if let CheckpointList::Rows(rows) = self.list() {
            // The label holds the index, padded to the widest, then the short hash (3.7).
            let width = rows
                .iter()
                .map(|row| row.index.to_string().len())
                .max()
                .unwrap_or(1);
            view.rows = rows
                .iter()
                .enumerate()
                .map(|(index, row)| ViewRow {
                    mark: index == self.selected,
                    label: format!("{:<width$}  {}", row.index, row.short),
                    age: row
                        .time
                        .map(|time| text::relative_age(time as f64, now_ms))
                        .unwrap_or_default(),
                    clock: clock_text(&row.raw_time),
                    text: &row.message,
                    tail: "",
                })
                .collect();
        }
        view.selected = self.selected;
        view.first_visible = self.first_visible;
        view.footer = text::CHECKPOINTS_FOOTER.to_string();
    }

    fn fill_checkpoint_confirm(&self, view: &mut RewindView<'_>) {
        view.title = text::CHECKPOINTS_TITLE.to_string();
        let Some(found) = &self.chosen_checkpoint else {
            return;
        };
        let checkpoint = &found.checkpoint;
        view.body = vec![
            view_line(
                text::checkpoint_confirm_line(
                    checkpoint.index,
                    &checkpoint.short,
                    &clock_text(&checkpoint.raw_time),
                    &checkpoint.message,
                ),
                LineStyle::Normal,
            ),
            view_line(text::CHECKPOINT_RESTORE_REMOVES_EXCHANGE, LineStyle::Normal),
            view_line(
                text::CHECKPOINT_LIMITS_WARNING.replace("<n>", &checkpoint.index.to_string()),
                LineStyle::Warning,
            ),
            view_line(text::KEYS_CONFIRM, LineStyle::Dim),
        ];
    }

    fn fill_diff<'a>(&'a self, view: &mut RewindView<'a>, loading: bool) {
        view.footer = text::DIFF_FOOTER.to_string();
        view.diff_scroll = self.diff_scroll;
        match (&self.diff_doc, loading) {
            (Some(doc), false) => {
                view.title = match doc {
                    DiffDoc::Text { title, .. } => title.clone(),
                    _ => self.diff_title.clone(),
                };
                view.diff = Some(doc);
            }
            _ => {
                view.title = self.diff_title.clone();
                view.body = vec![view_line(text::COMPARING_FILES, LineStyle::Dim)];
            }
        }
    }

    /// The checkpoint a prompt maps to, with its row.
    fn match_for(&self, turn: &RewindTurn) -> Result<ChosenCheckpoint, NoMatch> {
        let list = self.list();
        let found = checkpoint_for_turn(turn, list)?;
        let CheckpointList::Rows(rows) = list else {
            return Err(NoMatch::NoChanges);
        };
        rows.iter()
            .find(|row| row.index == found.index)
            .map(|row| ChosenCheckpoint {
                checkpoint: row.clone(),
                oldest_kept: found.oldest_kept,
            })
            .ok_or(NoMatch::NoChanges)
    }

    fn rows(&self) -> usize {
        self.viewport.unwrap_or(DEFAULT_VIEWPORT_ROWS).max(1)
    }

    /// Forget everything about the current work, keeping what outlives it.
    fn close(&mut self) {
        *self = Self {
            next_ticket: self.next_ticket,
            viewport: self.viewport,
            reload_pending: self.reload_pending,
            last_rebuild_revision: self.last_rebuild_revision,
            pending_external: self.pending_external,
            external_sequence: self.external_sequence,
            ..Self::default()
        };
    }

    fn close_with(&mut self, effects: Vec<RewindEffect>) -> Vec<RewindEffect> {
        self.close();
        effects
    }

    fn call(&mut self, call: Call) -> RewindEffect {
        if matches!(&call, Call::ReadSession) {
            self.read_sequence = self.external_sequence;
        }
        let kind = match &call {
            Call::ReadSession => CallKind::ReadSession,
            Call::ListCheckpoints => CallKind::ListCheckpoints,
            Call::CheckpointDiff { .. } => CallKind::CheckpointDiff,
            Call::SessionDiff => CallKind::SessionDiff,
            Call::Mutate { .. } => CallKind::Mutate,
            Call::Restore { .. } => CallKind::Restore,
        };
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        self.awaited = Some((ticket, kind));
        RewindEffect::Call { ticket, call }
    }

    fn record_revision(&mut self, revision: i64) {
        self.last_rebuild_revision = Some(
            self.last_rebuild_revision
                .map_or(revision, |last| last.max(revision)),
        );
    }

    /// A rebuild. Any rebuild serves a pending reload.
    fn rebuild(&mut self, snapshot: Value, notice: Vec<String>) -> RewindEffect {
        self.reload_pending = false;
        RewindEffect::Rebuild { snapshot, notice }
    }

    /// The reload marker rebuild, when a failed reload is still owed.
    fn reload_rebuild(&mut self, read: &SessionRead) -> Option<RewindEffect> {
        if !self.reload_pending {
            return None;
        }
        self.record_revision(read.revision);
        Some(self.rebuild(read.raw.clone(), vec![text::RELOADED_MARKER.to_string()]))
    }

    fn set_loaded(&mut self, read: &SessionRead) {
        self.loaded = LoadedRead {
            running: read.running,
            pending: read.pending,
        };
        self.turns = rewind_turns(&read.messages);
    }

    fn open(&mut self, turn_running: bool) -> Vec<RewindEffect> {
        if self.mode != Mode::Closed {
            return Vec::new();
        }
        if turn_running {
            return vec![warn(text::TURN_RUNNING)];
        }
        self.mode = Mode::Loading(Purpose::Rewind);
        vec![self.call(Call::ReadSession)]
    }

    fn open_checkpoints(&mut self) -> Vec<RewindEffect> {
        if self.mode != Mode::Closed {
            return Vec::new();
        }
        self.mode = Mode::Loading(Purpose::Checkpoints);
        vec![self.call(Call::ListCheckpoints)]
    }

    fn open_diff(&mut self, source: DiffSource) -> Vec<RewindEffect> {
        if self.mode != Mode::Closed {
            return Vec::new();
        }
        match source {
            DiffSource::Session { stat_only } => {
                self.diff_title = text::SESSION_DIFF_TITLE.to_string();
                self.diff_stat_only = Some(stat_only);
                self.mode = Mode::Diff {
                    back: Back::Close,
                    loading: true,
                };
                vec![self.call(Call::SessionDiff)]
            }
            DiffSource::Checkpoint(target) => {
                self.diff_title = text::checkpoint_diff_title(&target);
                self.typed = Some(Typed {
                    target,
                    file_path: None,
                    all: false,
                });
                self.mode = Mode::Loading(Purpose::TypedDiff);
                vec![self.call(Call::ListCheckpoints)]
            }
        }
    }

    fn open_typed_rewind(
        &mut self,
        origin: Origin,
        n: usize,
        turn_running: bool,
    ) -> Vec<RewindEffect> {
        if self.mode != Mode::Closed {
            return Vec::new();
        }
        if turn_running {
            return vec![warn(text::TURN_RUNNING)];
        }
        self.origin = Some(origin);
        self.undo_n = n.max(1);
        self.mode = Mode::Working(Step::ConfirmRead);
        vec![self.call(Call::ReadSession)]
    }

    fn open_typed_restore(
        &mut self,
        target: String,
        file_path: Option<String>,
        all: bool,
        turn_running: bool,
    ) -> Vec<RewindEffect> {
        if self.mode != Mode::Closed {
            return Vec::new();
        }
        if file_path.is_none() && turn_running {
            return vec![warn(text::RESTORE_NEEDS_IDLE)];
        }
        self.typed = Some(Typed {
            target,
            file_path,
            all,
        });
        self.mode = Mode::Loading(Purpose::TypedRestore);
        vec![self.call(Call::ListCheckpoints)]
    }

    fn session_updated(
        &mut self,
        operation: &str,
        revision: Option<i64>,
        rewound_count: Option<u64>,
    ) -> Vec<RewindEffect> {
        if operation != "rewind" {
            return Vec::new();
        }
        if !matches!(self.mode, Mode::Working(_))
            && let (Some(revision), Some(last)) = (revision, self.last_rebuild_revision)
            && revision <= last
        {
            return Vec::new();
        }
        self.remember_external(revision, rewound_count, true);
        if matches!(self.mode, Mode::Working(_)) {
            return Vec::new();
        }
        self.close();
        self.external_removed = rewound_count;
        self.external_is_rewind = true;
        self.mode = Mode::Working(Step::ExternalRebuildRead);
        vec![self.call(Call::ReadSession)]
    }

    fn remember_external(&mut self, revision: Option<i64>, removed: Option<u64>, is_rewind: bool) {
        self.external_sequence += 1;
        let pending = self
            .pending_external
            .get_or_insert_with(PendingExternal::default);
        if let Some(revision) = revision {
            if pending.revision.is_none_or(|old| revision >= old) {
                pending.revision = Some(revision);
                pending.removed = removed;
                pending.is_rewind |= is_rewind;
            }
        } else {
            pending.read_after_event = Some(self.external_sequence);
            pending.removed = removed;
            pending.is_rewind |= is_rewind;
        }
    }

    fn key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        match self.mode {
            Mode::Closed | Mode::Working(_) => Vec::new(),
            _ if key == RewindKey::Close => self.close_with(Vec::new()),
            Mode::Loading(_) => {
                if key == RewindKey::Esc {
                    self.close();
                }
                Vec::new()
            }
            Mode::Turns => self.turns_key(key),
            Mode::Actions => self.actions_key(key),
            Mode::Confirm(kind) => self.confirm_key(key, kind),
            Mode::Checkpoints => self.checkpoints_key(key),
            Mode::CheckpointConfirm => self.checkpoint_confirm_key(key),
            Mode::Diff { back, .. } => self.diff_key(key, back),
        }
    }

    fn move_selection(&mut self, key: RewindKey, len: usize) {
        if len == 0 {
            return;
        }
        let page = self.rows();
        self.selected = match key {
            RewindKey::Up => self.selected.saturating_sub(1),
            RewindKey::Down => (self.selected + 1).min(len - 1),
            RewindKey::PageUp => self.selected.saturating_sub(page),
            RewindKey::PageDown => (self.selected + page).min(len - 1),
            RewindKey::Home => 0,
            RewindKey::End => len - 1,
            _ => self.selected,
        };
        self.keep_selection_visible();
    }

    fn keep_selection_visible(&mut self) {
        let rows = self.rows();
        if self.selected < self.first_visible {
            self.first_visible = self.selected;
        } else if self.selected >= self.first_visible + rows {
            self.first_visible = self.selected + 1 - rows;
        }
    }

    fn turns_key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        match key {
            RewindKey::Esc => self.close_with(Vec::new()),
            RewindKey::Enter => {
                if let Some(turn) = self.turns.get(self.selected) {
                    self.chosen = Some(turn.clone());
                    self.mode = Mode::Actions;
                }
                Vec::new()
            }
            _ => {
                self.move_selection(key, self.turns.len());
                Vec::new()
            }
        }
    }

    fn actions_key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        let Some(chosen) = self.chosen.clone() else {
            return self.close_with(Vec::new());
        };
        match key {
            RewindKey::Esc => {
                self.mode = Mode::Turns;
                Vec::new()
            }
            RewindKey::C => {
                self.chosen_checkpoint = None;
                self.mode = Mode::Confirm(Kind::Conversation);
                Vec::new()
            }
            RewindKey::F => {
                if let Ok(found) = self.match_for(&chosen) {
                    self.chosen_checkpoint = Some(found);
                    self.mode = Mode::Confirm(Kind::Files);
                }
                Vec::new()
            }
            RewindKey::D => match self.match_for(&chosen) {
                Ok(found) => self.start_checkpoint_diff(found.checkpoint, Back::Actions),
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn start_checkpoint_diff(&mut self, checkpoint: Checkpoint, back: Back) -> Vec<RewindEffect> {
        self.diff_title = text::checkpoint_diff_title(&checkpoint.short);
        self.diff_stat_only = None;
        self.diff_doc = None;
        self.diff_scroll = 0;
        self.mode = Mode::Diff {
            back,
            loading: true,
        };
        vec![self.call(Call::CheckpointDiff {
            hash: checkpoint.hash,
        })]
    }

    fn confirm_key(&mut self, key: RewindKey, kind: Kind) -> Vec<RewindEffect> {
        match key {
            RewindKey::Esc => {
                self.changed_warning = false;
                self.mode = Mode::Actions;
                Vec::new()
            }
            RewindKey::Enter if !self.loaded.blocked() => {
                self.changed_warning = false;
                self.origin = Some(match kind {
                    Kind::Conversation => Origin::Conversation,
                    Kind::Files => Origin::Files,
                });
                self.mode = Mode::Working(Step::ConfirmRead);
                vec![self.call(Call::ReadSession)]
            }
            _ => Vec::new(),
        }
    }

    fn checkpoints_key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        let selected = match self.list() {
            CheckpointList::Rows(rows) => rows.get(self.selected).cloned(),
            _ => None,
        };
        let len = match self.list() {
            CheckpointList::Rows(rows) => rows.len(),
            _ => 0,
        };
        match key {
            RewindKey::Esc => self.close_with(Vec::new()),
            RewindKey::Enter | RewindKey::D => match selected {
                Some(checkpoint) => self.start_checkpoint_diff(checkpoint, Back::Checkpoints),
                None => Vec::new(),
            },
            RewindKey::R => {
                if let Some(checkpoint) = selected {
                    self.chosen_checkpoint = Some(ChosenCheckpoint {
                        checkpoint,
                        oldest_kept: false,
                    });
                    self.mode = Mode::CheckpointConfirm;
                }
                Vec::new()
            }
            _ => {
                self.move_selection(key, len);
                Vec::new()
            }
        }
    }

    fn checkpoint_confirm_key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        match key {
            RewindKey::Esc => {
                self.mode = Mode::Checkpoints;
                Vec::new()
            }
            RewindKey::Enter => {
                self.origin = Some(Origin::CheckpointRestore);
                self.mode = Mode::Working(Step::ConfirmRead);
                vec![self.call(Call::ReadSession)]
            }
            _ => Vec::new(),
        }
    }

    fn diff_key(&mut self, key: RewindKey, back: Back) -> Vec<RewindEffect> {
        if key == RewindKey::Esc {
            self.awaited = None;
            self.diff_doc = None;
            self.diff_scroll = 0;
            self.mode = match back {
                Back::Actions => Mode::Actions,
                Back::Checkpoints => Mode::Checkpoints,
                Back::Close => {
                    self.close();
                    return Vec::new();
                }
            };
            return Vec::new();
        }
        let last = self.diff_line_total().saturating_sub(1);
        let page = self.rows();
        self.diff_scroll = match key {
            RewindKey::Up => self.diff_scroll.saturating_sub(1),
            RewindKey::Down => (self.diff_scroll + 1).min(last),
            RewindKey::PageUp => self.diff_scroll.saturating_sub(page),
            RewindKey::PageDown => (self.diff_scroll + page).min(last),
            RewindKey::Home => 0,
            RewindKey::End => last,
            _ => self.diff_scroll,
        };
        Vec::new()
    }

    /// Lines of the loaded diff document, an upper bound for scrolling.
    fn diff_line_total(&self) -> usize {
        match &self.diff_doc {
            Some(DiffDoc::Text {
                stat,
                diff,
                stat_only,
                ..
            }) => {
                text::python_line_count(stat)
                    + if *stat_only {
                        0
                    } else {
                        text::python_line_count(diff)
                    }
            }
            _ => 0,
        }
    }

    fn reply(&mut self, ticket: u64, reply: Reply) -> Vec<RewindEffect> {
        let Some((awaited, kind)) = self.awaited else {
            return Vec::new();
        };
        if awaited != ticket || !reply_matches(kind, &reply) {
            return Vec::new();
        }
        self.awaited = None;
        match self.mode {
            Mode::Loading(purpose) => self.loading_reply(purpose, kind, reply),
            Mode::Diff { loading: true, .. } => self.diff_reply(kind, reply),
            Mode::Working(step) => self.working_reply(step, reply),
            _ => Vec::new(),
        }
    }

    fn loading_reply(
        &mut self,
        purpose: Purpose,
        kind: CallKind,
        reply: Reply,
    ) -> Vec<RewindEffect> {
        match (purpose, reply) {
            (Purpose::Rewind, Reply::SessionRead(read)) => {
                let mut effects: Vec<RewindEffect> =
                    self.reload_rebuild(&read).into_iter().collect();
                self.set_loaded(&read);
                // The table lists checkpoints first; an empty conversation closes with T3 on
                // the list reply (`show_turns`).
                effects.push(self.call(Call::ListCheckpoints));
                effects
            }
            (Purpose::Rewind, Reply::Checkpoints(list)) => {
                self.checkpoints = Some(list);
                self.show_turns()
            }
            (Purpose::Rewind, Reply::Failed(failure)) if kind == CallKind::ListCheckpoints => {
                self.checkpoints = Some(CheckpointList::Unavailable(text::short_failure_reason(
                    &failure,
                )));
                self.show_turns()
            }
            (Purpose::Checkpoints, Reply::Checkpoints(list)) => match list {
                CheckpointList::Off => self.close_with(vec![system(text::CHECKPOINTS_OFF)]),
                CheckpointList::Empty => self.close_with(vec![system(text::NO_CHECKPOINTS)]),
                CheckpointList::Unavailable(reason) => {
                    self.close_with(vec![warn(text::footer_checkpoints_unknown(&reason))])
                }
                CheckpointList::Rows(rows) => {
                    self.checkpoints = Some(CheckpointList::Rows(rows));
                    self.selected = 0;
                    self.first_visible = 0;
                    self.mode = Mode::Checkpoints;
                    Vec::new()
                }
            },
            (Purpose::TypedDiff, Reply::Checkpoints(list)) => match list {
                CheckpointList::Off => self.close_with(vec![system(text::CHECKPOINTS_OFF)]),
                _ => {
                    let hash = self
                        .typed
                        .as_ref()
                        .map(|typed| typed.target.clone())
                        .unwrap_or_default();
                    self.mode = Mode::Diff {
                        back: Back::Close,
                        loading: true,
                    };
                    vec![self.call(Call::CheckpointDiff { hash })]
                }
            },
            (Purpose::TypedRestore, Reply::Checkpoints(list)) => match list {
                CheckpointList::Off => self.close_with(vec![system(text::CHECKPOINTS_OFF)]),
                _ => self.start_typed_restore(),
            },
            (_, Reply::Failed(failure)) => {
                self.close_with(vec![warn(text::failure_text(&failure, kind))])
            }
            _ => Vec::new(),
        }
    }

    fn show_turns(&mut self) -> Vec<RewindEffect> {
        if self.turns.is_empty() {
            return self.close_with(vec![system(text::NOTHING_TO_REWIND)]);
        }
        self.selected = 0;
        self.first_visible = 0;
        self.mode = Mode::Turns;
        Vec::new()
    }

    fn start_typed_restore(&mut self) -> Vec<RewindEffect> {
        let Some(typed) = self.typed.clone() else {
            return self.close_with(Vec::new());
        };
        self.origin = Some(Origin::TypedRestore);
        match typed.file_path {
            None => {
                self.mode = Mode::Working(Step::ConfirmRead);
                vec![self.call(Call::ReadSession)]
            }
            Some(file_path) => {
                self.mode = Mode::Working(Step::Restoring {
                    whole_folder: false,
                });
                vec![self.call(Call::Restore {
                    hash: typed.target,
                    file_path: Some(file_path),
                    all: false,
                })]
            }
        }
    }

    fn diff_reply(&mut self, kind: CallKind, reply: Reply) -> Vec<RewindEffect> {
        match reply {
            Reply::DiffLoaded(mut doc) => {
                if let (DiffDoc::Text { stat_only, .. }, Some(asked)) =
                    (&mut doc, self.diff_stat_only)
                {
                    *stat_only = asked;
                }
                if let Mode::Diff { back, .. } = self.mode {
                    self.mode = Mode::Diff {
                        back,
                        loading: false,
                    };
                }
                self.diff_doc = Some(doc);
                self.diff_scroll = 0;
                Vec::new()
            }
            Reply::Failed(failure) => {
                self.close_with(vec![warn(text::failure_text(&failure, kind))])
            }
            _ => Vec::new(),
        }
    }

    fn working_reply(&mut self, step: Step, reply: Reply) -> Vec<RewindEffect> {
        match (step, reply) {
            (Step::ConfirmRead, Reply::SessionRead(read)) => self.confirm_read(read),
            (Step::ConfirmRead, Reply::Failed(failure)) => self.close_with(vec![warn(
                text::failure_text(&failure, CallKind::ReadSession),
            )]),
            (Step::Mutating { .. }, Reply::Mutated(outcome)) => {
                if let Some(revision) = outcome.revision {
                    self.record_revision(revision);
                }
                self.mutation = Some(outcome);
                self.mode = Mode::Working(Step::RebuildRead);
                vec![self.call(Call::ReadSession)]
            }
            (
                Step::Mutating {
                    row_id,
                    sent_generation,
                    after_restore,
                },
                Reply::Failed(failure),
            ) => self.mutate_failed(row_id, sent_generation, after_restore, failure),
            (Step::Restoring { whole_folder }, Reply::Restored(outcome)) => {
                self.restored(whole_folder, outcome)
            }
            (Step::Restoring { whole_folder }, Reply::Failed(failure)) => {
                self.restore_failed(whole_folder, failure)
            }
            (Step::AfterRestoreRead, Reply::SessionRead(read)) => self.after_restore_read(read),
            (Step::AfterRestoreRead, Reply::Failed(failure)) => {
                let mut effects = self.restore_summary_notices();
                effects.push(warn(text::reload_failed(&text::short_failure_reason(
                    &failure,
                ))));
                self.close();
                self.reload_pending = true;
                effects
            }
            (Step::RebuildRead, Reply::SessionRead(read)) => self.rebuild_read(read),
            (Step::RebuildRead, Reply::Failed(failure)) => {
                let origin = self.origin;
                let content = self
                    .mutation
                    .as_ref()
                    .and_then(|outcome| outcome.target_content.clone());
                let mut effects = vec![warn(text::reload_failed(&text::short_failure_reason(
                    &failure,
                )))];
                if let Some(origin) = origin {
                    effects.extend(self.prompt_effects(origin, content.as_ref()));
                }
                self.close();
                self.reload_pending = true;
                effects
            }
            (
                Step::Verifying {
                    row_id,
                    sent_generation,
                    after_restore,
                },
                Reply::SessionRead(read),
            ) => self.verify_read(row_id, sent_generation, after_restore, read),
            (Step::Verifying { .. }, Reply::Failed(_)) => {
                self.close_with(vec![warn(text::LOST_REPLY_UNCHECKED)])
            }
            (Step::LostRestoreRead, Reply::SessionRead(read)) => self.lost_restore_read(read),
            (Step::LostRestoreRead, Reply::Failed(_)) => {
                self.close_with(vec![warn(text::LOST_REPLY_RESTORE_FOLDER)])
            }
            (Step::ExternalRebuildRead, Reply::SessionRead(read)) => {
                self.record_revision(read.revision);
                let notice = if self.external_is_rewind {
                    vec![text::rewound_elsewhere_marker(self.external_removed)]
                } else {
                    vec![text::RELOADED_MARKER.to_string()]
                };
                let effect = self.rebuild(read.raw, notice);
                self.close_with(vec![effect])
            }
            (Step::ExternalRebuildRead, Reply::Failed(failure)) => {
                let effects = vec![warn(text::reload_failed(&text::short_failure_reason(
                    &failure,
                )))];
                self.close();
                self.reload_pending = true;
                effects
            }
            _ => Vec::new(),
        }
    }

    /// The first read of a write: decide whether to send it.
    fn confirm_read(&mut self, read: SessionRead) -> Vec<RewindEffect> {
        let mut effects: Vec<RewindEffect> = self.reload_rebuild(&read).into_iter().collect();
        self.record_revision(read.revision);
        let turns = rewind_turns(&read.messages);
        let busy = read.running || read.pending > 0;
        let Some(origin) = self.origin else {
            return self.close_with(effects);
        };
        match origin {
            Origin::Conversation | Origin::Files => {
                let Some(chosen) = self.chosen.clone() else {
                    return self.close_with(effects);
                };
                let Some(target) = turns.iter().find(|turn| turn.row_id == chosen.row_id) else {
                    effects.push(warn(text::PROMPT_GONE));
                    return self.close_with(effects);
                };
                if busy {
                    effects.push(warn(busy_text(origin, &read)));
                    return self.close_with(effects);
                }
                if target.later_count != chosen.later_count {
                    let target = target.clone();
                    self.selected = turns
                        .iter()
                        .position(|turn| turn.row_id == target.row_id)
                        .unwrap_or(0);
                    self.keep_selection_visible();
                    self.loaded = LoadedRead::default();
                    self.chosen = Some(target);
                    self.turns = turns;
                    self.changed_warning = true;
                    self.origin = None;
                    self.mode = Mode::Confirm(if origin == Origin::Files {
                        Kind::Files
                    } else {
                        Kind::Conversation
                    });
                    return effects;
                }
                let target = target.clone();
                self.confirm_turns = turns;
                self.confirm_revision = Some(read.revision);
                self.chosen = Some(target.clone());
                if origin == Origin::Files {
                    let Some(found) = self.chosen_checkpoint.clone() else {
                        return self.close_with(effects);
                    };
                    effects.push(self.start_restore(found.checkpoint.hash, false));
                } else {
                    effects.push(self.start_mutate(target.row_id, &read, false));
                }
                effects
            }
            Origin::Undo | Origin::Retry => {
                if busy {
                    effects.push(warn(text::REWIND_BLOCKED_BUSY));
                    return self.close_with(effects);
                }
                let target = if origin == Origin::Undo {
                    match turns.get(self.undo_n - 1) {
                        Some(target) => target.clone(),
                        None => {
                            effects.push(warn(text::too_few_prompts_to_undo(turns.len())));
                            return self.close_with(effects);
                        }
                    }
                } else {
                    match turns.first() {
                        None => {
                            effects.push(warn(text::NOTHING_TO_RETRY));
                            return self.close_with(effects);
                        }
                        Some(turn) => match turn.prompt_text.as_deref() {
                            None => {
                                effects.push(warn(text::RETRY_HAS_ATTACHMENTS));
                                return self.close_with(effects);
                            }
                            Some("") => {
                                effects.push(warn(text::NOTHING_TO_RETRY));
                                return self.close_with(effects);
                            }
                            Some(_) => turn.clone(),
                        },
                    }
                };
                self.confirm_turns = turns;
                self.confirm_revision = Some(read.revision);
                effects.push(self.start_mutate(target.row_id, &read, false));
                self.chosen = Some(target);
                effects
            }
            Origin::CheckpointRestore | Origin::TypedRestore => {
                if read.running {
                    effects.push(warn(text::RESTORE_NEEDS_IDLE));
                    return self.close_with(effects);
                }
                if read.pending > 0 {
                    effects.push(warn(text::RESTORE_BLOCKED_BY_QUEUE));
                    return self.close_with(effects);
                }
                self.confirm_turns = turns;
                self.confirm_revision = Some(read.revision);
                let (hash, all) = if origin == Origin::TypedRestore {
                    match &self.typed {
                        Some(typed) => (typed.target.clone(), typed.all),
                        None => return self.close_with(effects),
                    }
                } else {
                    match &self.chosen_checkpoint {
                        Some(found) => (found.checkpoint.hash.clone(), false),
                        None => return self.close_with(effects),
                    }
                };
                effects.push(self.start_restore(hash, all));
                effects
            }
        }
    }

    fn start_mutate(
        &mut self,
        row_id: i64,
        read: &SessionRead,
        after_restore: bool,
    ) -> RewindEffect {
        self.mode = Mode::Working(Step::Mutating {
            row_id,
            sent_generation: read.execution_generation,
            after_restore,
        });
        self.call(Call::Mutate {
            row_id,
            expected_revision: read.revision,
            expected_generation: read.execution_generation,
        })
    }

    fn start_restore(&mut self, hash: String, all: bool) -> RewindEffect {
        self.mode = Mode::Working(Step::Restoring { whole_folder: true });
        self.call(Call::Restore {
            hash,
            file_path: None,
            all,
        })
    }

    fn mutate_failed(
        &mut self,
        row_id: i64,
        sent_generation: i64,
        after_restore: bool,
        failure: CallFailure,
    ) -> Vec<RewindEffect> {
        if let CallFailure::Refused {
            reason, revision, ..
        } = &failure
            && matches!(
                reason.as_str(),
                "revision_conflict"
                    | "stale_generation"
                    | "revision_mismatch"
                    | "generation_mismatch"
            )
        {
            self.remember_external(*revision, None, false);
        }
        let reason = match &failure {
            CallFailure::Refused {
                reason,
                field,
                detail,
                ..
            } => Some(text::mutate_refusal_text(
                reason,
                field.as_deref(),
                detail.as_deref(),
            )),
            CallFailure::NotSent(_) => Some(text::NOT_CONNECTED.to_string()),
            CallFailure::Unknown | CallFailure::Unreadable => None,
        };
        let Some(reason) = reason else {
            self.mode = Mode::Working(Step::Verifying {
                row_id,
                sent_generation,
                after_restore,
            });
            return vec![self.call(Call::ReadSession)];
        };
        let mut effects = Vec::new();
        if after_restore {
            effects.push(warn(self.restored_but_failed(&reason)));
            if let Some(older) = self.older_removed.take() {
                effects.push(warn(older));
            }
        } else {
            effects.push(warn(reason));
        }
        self.close_with(effects)
    }

    /// Text T14 for the current restore and chosen prompt.
    fn restored_but_failed(&self, refusal: &str) -> String {
        let short = self.restored_short();
        let ordinal = self.chosen.as_ref().map_or(0, |turn| turn.ordinal);
        text::restored_but_rewind_failed(&short, ordinal, refusal)
    }

    fn restored_short(&self) -> String {
        self.restore
            .as_ref()
            .and_then(|outcome| outcome.restored_to.clone())
            .or_else(|| {
                self.chosen_checkpoint
                    .as_ref()
                    .map(|found| found.checkpoint.short.clone())
            })
            .unwrap_or_default()
    }

    /// The label `restore_summary` puts in its hint: the typed target or the row index.
    fn restore_label(&self) -> String {
        match self.origin {
            Some(Origin::TypedRestore) => self
                .typed
                .as_ref()
                .map(|typed| typed.target.clone())
                .unwrap_or_default(),
            _ => self
                .chosen_checkpoint
                .as_ref()
                .map(|found| found.checkpoint.index.to_string())
                .unwrap_or_default(),
        }
    }

    fn restore_lines(&self) -> Vec<String> {
        self.restore
            .as_ref()
            .map(|outcome| text::restore_summary(outcome, &self.restore_label()))
            .unwrap_or_default()
    }

    fn restore_summary_notices(&self) -> Vec<RewindEffect> {
        self.restore_lines().into_iter().map(system).collect()
    }

    fn restored(&mut self, whole_folder: bool, outcome: RestoreOutcome) -> Vec<RewindEffect> {
        if !outcome.success {
            let error = outcome.error.as_deref().unwrap_or("no reason given");
            return self.close_with(vec![warn(text::restore_not_done(error))]);
        }
        self.restore = Some(outcome);
        if whole_folder {
            self.mode = Mode::Working(Step::AfterRestoreRead);
            return vec![self.call(Call::ReadSession)];
        }
        let effects = self.restore_summary_notices();
        self.close_with(effects)
    }

    fn restore_failed(&mut self, whole_folder: bool, failure: CallFailure) -> Vec<RewindEffect> {
        match failure {
            CallFailure::Refused { reason, detail, .. } => self.close_with(vec![warn(
                text::restore_refusal_text(whole_folder, &reason, detail.as_deref()),
            )]),
            CallFailure::Unknown | CallFailure::Unreadable => {
                if whole_folder {
                    self.mode = Mode::Working(Step::LostRestoreRead);
                    vec![self.call(Call::ReadSession)]
                } else {
                    self.close_with(vec![warn(text::LOST_REPLY_RESTORE_FILE)])
                }
            }
            CallFailure::NotSent(_) => self.close_with(vec![warn(text::NOT_CONNECTED)]),
        }
    }

    fn after_restore_read(&mut self, read: SessionRead) -> Vec<RewindEffect> {
        self.record_revision(read.revision);
        let removed = self
            .restore
            .as_ref()
            .and_then(|outcome| outcome.history_removed)
            .unwrap_or(0)
            .max(0)
            .unsigned_abs();
        match self.origin {
            Some(Origin::Files) => {
                let Some(chosen) = self.chosen.clone() else {
                    return self.close_with(Vec::new());
                };
                self.older_removed = self.older_prompt_removed(&chosen, &read);
                let still_there = rewind_turns(&read.messages)
                    .iter()
                    .any(|turn| turn.row_id == chosen.row_id);
                if still_there {
                    return vec![self.start_mutate(chosen.row_id, &read, true)];
                }
                let mut notice = vec![self.files_marker(&chosen, removed)];
                notice.extend(self.restore_lines());
                notice.extend(self.older_removed.take());
                let mut effects = vec![self.rebuild(read.raw, notice)];
                effects.extend(self.return_prompt());
                self.close_with(effects)
            }
            _ => {
                let lines = self.restore_lines();
                let history_removed = removed > 0;
                let effects = if history_removed {
                    vec![self.rebuild(read.raw, lines)]
                } else {
                    lines.into_iter().map(system).collect()
                };
                self.close_with(effects)
            }
        }
    }

    /// Text T4f, first line, for the chosen checkpoint and prompt.
    fn files_marker(&self, chosen: &RewindTurn, removed: u64) -> String {
        let message = self
            .chosen_checkpoint
            .as_ref()
            .map(|found| found.checkpoint.message.clone())
            .unwrap_or_default();
        text::restored_and_rewound_marker(
            &self.restored_short(),
            &message,
            chosen.ordinal,
            &chosen.preview,
            removed,
        )
    }

    /// Text T32: the newest prompt older than `chosen` that the restore also removed.
    fn older_prompt_removed(&self, chosen: &RewindTurn, read: &SessionRead) -> Option<String> {
        self.confirm_turns
            .iter()
            .filter(|turn| turn.ordinal < chosen.ordinal)
            .filter(|turn| !is_active_row(&read.messages, turn.row_id))
            .max_by_key(|turn| turn.ordinal)
            .map(|turn| text::restore_removed_older_prompt(turn.ordinal, &turn.preview))
    }

    fn rebuild_read(&mut self, read: SessionRead) -> Vec<RewindEffect> {
        self.record_revision(read.revision);
        let Some(origin) = self.origin else {
            return self.close_with(Vec::new());
        };
        let Some(chosen) = self.chosen.clone() else {
            return self.close_with(Vec::new());
        };
        let outcome = self.mutation.take().unwrap_or(MutateOutcome {
            revision: None,
            rewound_count: 0,
            target_content: None,
        });
        let notice = if origin == Origin::Files {
            let mut notice = vec![self.files_marker(&chosen, outcome.rewound_count)];
            notice.extend(self.restore_lines());
            notice.extend(self.older_removed.take());
            notice
        } else {
            vec![text::rewound_marker(
                chosen.ordinal,
                &chosen.preview,
                outcome.rewound_count,
            )]
        };
        let mut effects = vec![self.rebuild(read.raw, notice)];
        effects.extend(self.prompt_effects(origin, outcome.target_content.as_ref()));
        self.close_with(effects)
    }

    /// Put the prompt back, or send it again for `/retry`.
    fn prompt_effects(&self, origin: Origin, target_content: Option<&Value>) -> Vec<RewindEffect> {
        let Some(chosen) = &self.chosen else {
            return Vec::new();
        };
        if origin == Origin::Retry {
            let text = target_content
                .and_then(lossless_text)
                .filter(|text| !text.is_empty())
                .or_else(|| chosen.prompt_text.clone())
                .unwrap_or_default();
            return vec![RewindEffect::Submit(text)];
        }
        self.return_prompt()
    }

    fn return_prompt(&self) -> Vec<RewindEffect> {
        let Some(chosen) = &self.chosen else {
            return Vec::new();
        };
        vec![RewindEffect::ReturnPrompt {
            text: chosen
                .prompt_text
                .clone()
                .unwrap_or_else(|| chosen.text_parts.clone()),
            attachments: chosen.attachments,
        }]
    }

    fn verify_read(
        &mut self,
        row_id: i64,
        sent_generation: i64,
        after_restore: bool,
        read: SessionRead,
    ) -> Vec<RewindEffect> {
        self.record_revision(read.revision);
        let Some(chosen) = self.chosen.clone() else {
            return self.close_with(Vec::new());
        };
        let context = if after_restore {
            self.restore_lines()
        } else {
            Vec::new()
        };
        if is_active_row(&read.messages, row_id) {
            let effects = if after_restore {
                vec![warn(
                    self.restored_but_failed(text::LOST_REPLY_AFTER_RESTORE),
                )]
            } else {
                vec![warn(text::LOST_REPLY_REWIND_DID_NOT_HAPPEN)]
            };
            return self.close_with(effects);
        }
        if read.execution_generation > sent_generation {
            let mut notice = vec![
                text::LOST_REPLY_REWIND_WENT_THROUGH.to_string(),
                text::conversation_changed_marker(chosen.ordinal, &chosen.preview),
            ];
            notice.extend(context);
            let mut effects = vec![self.rebuild(read.raw, notice)];
            effects.extend(self.return_prompt());
            return self.close_with(effects);
        }
        let mut notice = vec![text::lost_reply_prompt_gone(chosen.ordinal)];
        notice.extend(context);
        let effects = vec![self.rebuild(read.raw, notice)];
        self.close_with(effects)
    }

    fn lost_restore_read(&mut self, read: SessionRead) -> Vec<RewindEffect> {
        let revision_rose = self
            .confirm_revision
            .is_some_and(|confirmed| read.revision > confirmed);
        self.record_revision(read.revision);
        if !revision_rose {
            return self.close_with(vec![warn(text::LOST_REPLY_RESTORE_FOLDER)]);
        }
        let mut notice = vec![text::LOST_REPLY_RESTORE_FOLDER.to_string()];
        if let Some(gone) = self
            .confirm_turns
            .iter()
            .filter(|turn| !is_active_row(&read.messages, turn.row_id))
            .min_by_key(|turn| turn.ordinal)
        {
            notice.push(text::conversation_changed_marker(
                gone.ordinal,
                &gone.preview,
            ));
        }
        let effects = vec![self.rebuild(read.raw, notice)];
        self.close_with(effects)
    }
}

/// The age text of a prompt, empty without a time.
fn turn_age(turn: &RewindTurn, now_ms: u64) -> String {
    turn.timestamp
        .map(|time| text::relative_age(time, now_ms))
        .unwrap_or_default()
}

/// The refusal for a confirm read that shows a reply running or prompts queued.
fn busy_text(origin: Origin, read: &SessionRead) -> &'static str {
    match origin {
        Origin::Files | Origin::CheckpointRestore | Origin::TypedRestore => {
            if read.running {
                text::RESTORE_NEEDS_IDLE
            } else {
                text::RESTORE_BLOCKED_BY_QUEUE
            }
        }
        Origin::Conversation | Origin::Undo | Origin::Retry => text::REWIND_BLOCKED_BUSY,
    }
}

/// The reply type a call kind gets back, a failure included.
fn reply_matches(kind: CallKind, reply: &Reply) -> bool {
    matches!(
        (kind, reply),
        (_, Reply::Failed(_))
            | (CallKind::ReadSession, Reply::SessionRead(_))
            | (CallKind::ListCheckpoints, Reply::Checkpoints(_))
            | (
                CallKind::CheckpointDiff | CallKind::SessionDiff,
                Reply::DiffLoaded(_)
            )
            | (CallKind::Mutate, Reply::Mutated(_))
            | (CallKind::Restore, Reply::Restored(_))
    )
}

/// Apply one input. `now_ms` is supplied by the application.
pub fn update(state: &mut RewindState, input: RewindInput, now_ms: u64) -> Vec<RewindEffect> {
    // The view reads the clock; no transition does.
    let _ = now_ms;
    let external_read_failed = matches!(state.mode, Mode::Working(Step::ExternalRebuildRead))
        && matches!(
            &input,
            RewindInput::Reply {
                ticket,
                reply: Reply::Failed(_),
            } if state.awaited == Some((*ticket, CallKind::ReadSession))
        );
    let read_sequence = matches!(&input, RewindInput::Reply { ticket, .. }
        if state.awaited == Some((*ticket, CallKind::ReadSession)))
    .then_some(state.read_sequence);
    let mut effects = match input {
        RewindInput::Open { turn_running } => state.open(turn_running),
        RewindInput::OpenCheckpoints => state.open_checkpoints(),
        RewindInput::OpenDiff(source) => state.open_diff(source),
        RewindInput::Undo { n, turn_running } => {
            state.open_typed_rewind(Origin::Undo, n, turn_running)
        }
        RewindInput::Retry { turn_running } => {
            state.open_typed_rewind(Origin::Retry, 1, turn_running)
        }
        RewindInput::TypedRestore {
            target,
            file_path,
            all,
            turn_running,
        } => state.open_typed_restore(target, file_path, all, turn_running),
        RewindInput::Key(key) => state.key(key),
        RewindInput::Viewport(rows) => {
            state.viewport = Some(usize::from(rows).max(1));
            state.keep_selection_visible();
            Vec::new()
        }
        RewindInput::Reply { ticket, reply } => state.reply(ticket, reply),
        RewindInput::SessionUpdated {
            operation,
            revision,
            rewound_count,
        } => state.session_updated(&operation, revision, rewound_count),
    };
    if external_read_failed {
        state.pending_external = None;
    }
    if let Some(pending) = state.pending_external {
        let covered = effects.iter().any(|effect| match effect {
            RewindEffect::Rebuild { snapshot, .. } => {
                pending.revision.is_none_or(|wanted| {
                    snapshot
                        .get("revision")
                        .and_then(Value::as_i64)
                        .is_some_and(|actual| actual >= wanted)
                }) && pending
                    .read_after_event
                    .is_none_or(|event| read_sequence.is_some_and(|read| read >= event))
            }
            _ => false,
        });
        if covered {
            state.pending_external = None;
        } else if !matches!(state.mode, Mode::Working(_)) {
            state.pending_external = None;
            state.close();
            state.external_removed = pending.removed;
            state.external_is_rewind = pending.is_rewind;
            state.pending_external = Some(pending);
            state.mode = Mode::Working(Step::ExternalRebuildRead);
            effects.push(state.call(Call::ReadSession));
        }
    }
    effects
}
