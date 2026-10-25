//! Application loop: one channel, a pure update, and the effect executor.
//!
//! The loop owns the session state, the link and the services. It reaches the terminal only
//! through [`Screen`], which `talaria-cli` implements with `talaria-tui`, so this crate never
//! depends on a terminal library. Every gateway call runs in a spawned task and comes back as an
//! [`AppMessage`]; the loop itself never waits on the network.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hermes_client::liveness::{Link, LinkEvent, LinkState};
use hermes_client::record::{RecordError, Recorder, RecorderOptions};
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_client::{Inbound, ServerRequest};
use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::attachments::{Chip, CollapseRule, Expanded, reference_notice};
use talaria_core::checklist::{Checklist, ChecklistCommit, ChecklistTracker};
use talaria_core::commands::local;
use talaria_core::commands::{
    CommandOutcome, CommandsEffect, CommandsInput, CommandsState, LocalAction, LocalCommandTable,
    PaletteContext, PaletteEffect, PaletteInput, PaletteState, palette_update, parse_slash_line,
    state_update,
};
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::{compact_end, compact_payload, interpret, settle_as};
use talaria_core::context::tracker::{CompactingLine, ContextTracker, HeldBlocks};
use talaria_core::context::usage::UsageReport;
use talaria_core::models::{
    ConfirmKey, ModelCommandOrEffort, ModelsEffect, ModelsInput, ModelsState, NoticeLevel,
    PickerKey, PickerView, SavedModels, parse_effort_command, parse_model_command,
};
use talaria_core::mutation::{Confirmation, MutationLedger, MutationOutcome, MutationRequest};
use talaria_core::permissions::{
    ApprovalSegmentText, CallFailure as PermissionsFailure, PermissionsEffect, PermissionsInput,
    PermissionsState, hermes_command_finished, parse_permissions_command,
};
use talaria_core::rewind::{
    CallFailure, DiffCommand, DiffSource, DoubleEsc, Reply, RewindEffect, RewindInput, RewindKey,
    RewindState, RewindView, RollbackCommand, parse_diff as parse_diff_command, parse_rollback,
    parse_undo, text as rewind_text, update as rewind_update,
};
use talaria_core::session::{
    Cell, Effect, Event, NoticeKind, PromptKey, PromptKind, PromptState, SessionId, SessionState,
    TurnState,
};
use talaria_core::side::{
    NO_SESSION_NOTICE, SHELL_EMPTY_HINT, SIDE_USAGE, Submission, btw_command, carry_over, classify,
    next_tick,
};
use talaria_core::workspace::{
    CdContext, CdEffect, CdInput, CdState, FolderCheck, SessionFolder, parse_cd_command,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::attachments::{AttachmentEffect, AttachmentEvent, AttachmentsState};
use crate::commands::{
    AppAction, CommandReply, CommandService, no_session_notice, outcome_actions,
};
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, AppearanceState, Polarity, PolaritySource, TerminalColors,
    ThemeReport,
};

use crate::appearance::{self, AppearanceService, AppearanceView, theme_lines};

/// Presentation-only attachment data for the terminal adapter.
#[derive(Clone, Default)]
pub struct AttachmentView {
    pub mention: Option<talaria_core::attachments::MentionView>,
    pub highlights: Vec<(Range<usize>, crate::attachments::LabelStyle)>,
}
use crate::connection::{ConnectionService, OpenConnection, find_git_toplevel};
use crate::context::{ContextBlock, ContextService, ContextTimeouts};
use crate::failure::{self, LaunchFailure};
use crate::launch::{
    Belongs, Launch, ListRow, MAX_HELD_PROMPTS, RESUME_HISTORY_CELLS, SessionRequest, StartMode,
    belongs, merge_rows, new_request_id, plain, plan, workspace_key,
};
use crate::mutation::MutationClient;
use crate::permissions::PermissionsService;
use crate::prompts::{MaskedRequestInfo, PromptService, ServerRequestPolicy, policy_for};
use crate::sessions::{
    DetachOutcome, ExecuteOutcome, OpenError, OpenedSession, SessionInfoSummary, SessionService,
    accept_event, submission_id,
};
use crate::side::{self, SideService};
use crate::workspace::WorkspaceService;

/// How long a quit waits for a create or resume that is still in flight.
pub const QUIT_WAIT: Duration = Duration::from_secs(5);
/// The bound passed to the prompt service's deny before an interrupt (it caps it there too).
pub const DENY_WAIT: Duration = Duration::from_secs(2);
/// Titles in the resume separator are cut to this many characters, with no ellipsis.
pub const TITLE_CHARS: usize = 60;
/// How notices name the approvals toggle until the screen reports its own binding (#237).
pub const DEFAULT_TOGGLE_LABEL: &str = "Shift+Tab";
/// Unsent prompts printed after a quit are cut to this many characters.
const HELD_PRINT_CHARS: usize = 80;
/// At most this many ready messages are applied after the one that woke the loop, before
/// the screen is drawn.
const MESSAGE_BATCH: usize = 64;

pub const SESSION_GONE_NOTICE: &str = "This session no longer exists on the gateway.";
pub const STILL_WORKING: &str = "The agent is still working on it in the background.";
pub const HELD_FULL_NOTICE: &str =
    "Too many prompts are waiting for Hermes. Wait a moment, then send this one again.";
pub const REFRESHED_NOTICE: &str = "Reconnected. The transcript was refreshed.";
pub const STALE_SUBSCRIPTION: &str =
    "Hermes no longer knew this connection's subscription; the session is unaffected.";
const LINK_STOPPED_NOTICE: &str = "This prompt was not sent: the connection to Hermes has stopped.";
const REFRESH_FAILED_NOTICE: &str = "The transcript could not be refreshed after the reconnect.";

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
    /// A key for the open rewind overlay. Issue #233.
    Rewind(RewindKey),
    /// How many list rows the rewind overlay shows.
    RewindViewport(u16),
    /// Esc reached the composer. `eligible` when the composer was empty and idle: two eligible
    /// presses close together open the rewind overlay.
    Escape {
        eligible: bool,
    },
    OpenModels,
    ModelPicker(PickerKey),
    ModelConfirm(ConfirmKey),
    /// The composer sent this text.
    Submit(String),
    /// Stop the running turn (Esc, or Ctrl+C on a card or an empty draft).
    Interrupt,
    /// `/quit` or the composer's exit request.
    Quit,
    /// A key the transcript screen owns.
    Input(InputKey),
    /// An approval card was answered.
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    /// A clarification card was answered.
    Clarify {
        key: PromptKey,
        answer: String,
    },
    /// A masked card was answered; an empty value declines it.
    Masked {
        request_id: String,
        value: Zeroizing<String>,
    },
    /// A line the screen wants shown.
    Notice(String),
    /// The draft changed, or a completion-list key was pressed. Issue #223.
    Palette(PaletteInput),
    /// A composer edit, including an explicit history recall.
    DraftEdited {
        text: String,
        caret: usize,
        typed: bool,
        recalled: bool,
        now: Instant,
    },
    /// A key consumed before the composer edits the draft.
    AttachmentKey {
        action: crate::attachments::Action,
        text: String,
        caret: usize,
        now: Instant,
    },
    /// A bracketed paste after the composer's safe-text rule.
    Paste {
        text: String,
        draft: String,
        caret: usize,
    },
    /// Flip the task checklist panel. Never reaches the composer.
    ToggleTasks,
    /// The approvals toggle key, with the context that owned the key (#237).
    ApprovalsToggle {
        /// Which screen owned the key.
        context: ToggleContext,
    },
}

/// Which screen owned the approvals toggle key (#237).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToggleContext {
    /// The draft box.
    Composer,
    /// A completion list over the draft box.
    Completion,
    /// A focused approval card with an empty draft.
    Card,
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
    /// The slash palette under the composer.
    pub palette: &'a PaletteState,
    /// The rewind overlay, while it is open. Issue #233.
    pub rewind: Option<RewindView<'a>>,
    /// Whether the context figure must stay hidden for this session.
    pub context_hidden: bool,
    /// The compacting line, while a compaction runs.
    pub compacting_line: Option<CompactingLine>,
    /// The milliseconds since the loop started, for the compacting line.
    /// App-clock milliseconds of the latest message, for the side-tasks section.
    pub now_ms: u64,
    /// The transcript screen is open, so every key belongs to it.
    pub pager_open: bool,
    /// The transcript screen is taking a search query.
    pub pager_typing: bool,
    /// Bumped when the session state was replaced.
    pub source_epoch: u64,
    /// The agent's task checklist while the turn runs.
    pub checklist: Option<&'a Checklist>,
    /// Whether the checklist panel shows every row.
    pub checklist_expanded: bool,
    /// The approvals segment, when the permissions state shows one (#237).
    pub approvals: Option<ApprovalSegmentText>,
    /// The foreground session's folder and branch, the status line's only copy (#237).
    pub folder: Option<&'a SessionFolder>,
}

impl View<'_> {
    /// A turn is running in the session.
    #[must_use]
    pub fn turn_running(&self) -> bool {
        self.state
            .is_some_and(|state| *state.turn() == talaria_core::session::TurnState::Running)
    }
}

/// The terminal side of the loop.
pub trait Screen: ComposerDeadline + StatusHost {
    /// Synchronize model overlays and cards without passing terminal state into the app.
    fn sync_models(
        &mut self,
        _picker: Option<PickerView<'_>>,
        _confirmation: Option<&Confirmation>,
    ) {
    }
    fn model_list_height(&self) -> usize {
        10
    }
    /// One raw input: a key, a paste, a resize, or a status command result.
    type Input: Send + 'static;
    /// Route one input and redraw; returns what the person asked for.
    fn input(&mut self, input: Self::Input, view: &View<'_>, now: Instant) -> Vec<UserAction>;
    /// Draw the live region.
    fn draw(&mut self, view: &View<'_>);
    /// Open, close, or drive the transcript screen.
    fn transcript(&mut self, _action: TranscriptAction) {}
    /// Write plain lines into the scrollback, once.
    fn commit_lines(&mut self, lines: Vec<String>);
    /// Write a context block into the scrollback, once.
    fn commit_context(&mut self, block: &ContextBlock);
    /// Write the agent's task checklist into the scrollback, once.
    fn commit_checklist(&mut self, _commit: &ChecklistCommit) {}
    /// Render cells into the scrollback, once.
    fn commit_cells(&mut self, cells: &[Cell]);
    /// Put text back into the composer.
    fn load_draft(&mut self, text: &str);
    /// Take a draft the composer recorded on Enter back out of Up-arrow recall.
    fn unrecord_submit(&mut self, _text: &str) {}
    /// Give the terminal back. Later draws write nothing.
    fn restore(&mut self);
    /// Emit an attention alert between frames.
    fn alert(&mut self, _alert: &talaria_core::attention::Alert) {}
    /// Report the pane without blocking the application.
    fn report_pane(
        &mut self,
        _pane: &talaria_core::attention::PaneReport,
        _session: Option<(&str, &str)>,
    ) {
    }
    /// Configured alert channels for this interactive screen.
    fn attention_config(&self) -> talaria_core::attention::AttentionConfig {
        talaria_core::attention::AttentionConfig {
            bell: vec![
                talaria_core::attention::AlertKind::NeedsYou,
                talaria_core::attention::AlertKind::Failed,
            ],
            desktop: vec![
                talaria_core::attention::AlertKind::NeedsYou,
                talaria_core::attention::AlertKind::Done,
                talaria_core::attention::AlertKind::Failed,
            ],
            done_after: Duration::from_secs(20),
        }
    }

    /// The start-up probe colours and the polarity decided from them, captured once.
    fn startup_appearance(&self) -> Option<(TerminalColors, (Polarity, PolaritySource))> {
        None
    }
    /// The skin or the session polarity changed: resolve the theme again for the composer, the
    /// status line and cells that start from now on. Scrollback is never redrawn.
    fn appearance_changed(&mut self, _view: &AppearanceView<'_>) {}
    /// What the resolved theme reports, for `/theme`.
    fn theme_report(&self) -> Option<ThemeReport> {
        None
    }
    /// Apply one programmatic edit and report its sanitized draft and caret.
    fn edit_draft(&mut self, _range: Range<usize>, _text: &str) -> Option<(String, usize)> {
        None
    }
    /// Read the draft after a burst was flushed by the composer timer.
    fn draft_snapshot(&self) -> Option<(String, usize)> {
        None
    }
    /// Commit buffered raw keystrokes before deriving an asynchronous insertion range.
    fn flush_draft(&mut self) -> Option<(String, usize)> {
        self.draft_snapshot()
    }
    /// Refresh immutable completion rows and chip styling.
    fn sync_attachments(&mut self, _view: AttachmentView) {}
    /// The configured paste thresholds.
    fn attachment_rule(&self) -> CollapseRule {
        CollapseRule::HERMES_DEFAULT
    }
    /// How notices name the approvals toggle key: its first binding, or `None` when the
    /// person unbound it (#237).
    fn toggle_label(&self) -> Option<String> {
        Some(DEFAULT_TOGGLE_LABEL.to_string())
    }
    /// How the `/cd` refusal names the interrupt key; `None` lets it say Esc (#237).
    fn interrupt_label(&self) -> Option<String> {
        None
    }
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
    pub models: ModelsState,
    model_service: Option<Arc<crate::models::ModelService>>,
    model_notice_counter: u64,
    model_warnings: BTreeSet<String>,
    attention: talaria_core::attention::AttentionTracker,
    attention_at: Instant,
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
    /// Composer draft. The slash palette reads it. Issue #223.
    pub draft: String,
    /// Slash palette. Closed until a slash draft opens it. Issue #223.
    pub palette: PaletteState,
    /// The catalogue store for this connection.
    pub commands: CommandsState,
    /// Commands Talaria runs itself.
    pub local: LocalCommandTable,
    /// The rewind overlay and the typed rewind commands. Issue #233.
    rewind: RewindState,
    /// Two quick eligible Esc presses open the rewind overlay.
    double_esc: DoubleEsc,
    /// The `/retry` submission still waiting for its outcome, with its text.
    retry: Option<(String, String)>,
    /// What each session's context readings mean.
    pub context: ContextTracker,
    /// The mutation requests kept for an exact resend.
    pub ledger: MutationLedger,
    /// The context blocks that wait for the turn to end.
    pub context_blocks: HeldBlocks<ContextBlock>,
    /// The milliseconds since the loop started, stamped before every message.
    /// App-clock milliseconds since the loop started, as of the message being applied. The loop
    /// sets it; [`update`] only reads it, for the start time of a shell command or side question.
    pub now_ms: u64,
    /// The Hermes skin and the session's light or dark override.
    pub appearance: AppearanceService,
    /// Bindings from the keys table. Issue 222 reads these while routing the transcript.
    pub key_bindings: BTreeMap<String, Vec<String>>,
    /// The transcript pager is open.
    pub pager_open: bool,
    /// The transcript screen is taking a search query.
    pub pager_typing: bool,
    /// A search has been confirmed and is still showing.
    pub pager_confirmed: bool,
    /// A search was confirmed at least once since the transcript opened.
    pub pager_has_last_confirmed: bool,
    /// Whether a search was confirmed when the prompt started taking keys.
    pub pager_confirmed_before_typing: bool,
    /// Characters typed into the search prompt that is open now.
    pub pager_typed_len: usize,
    /// Bumped when the session state is replaced.
    pub source_epoch: u64,
    pub checklist: ChecklistTracker,
    /// How notices name the approvals toggle key; `None` when it is unbound (#237).
    pub toggle_label: Option<String>,
    open_keys: Vec<PromptKey>,
    /// The approval settings Hermes last confirmed, made when the first session opens.
    permissions: Option<PermissionsState>,
    /// The calls behind the permissions effects; a new one with every connection.
    permissions_service: Option<Arc<PermissionsService>>,
    /// The `/cd` reducer: at most one folder check or move at a time (#237).
    cd: CdState,
    /// The moves' own request ids and fences, apart from the model picker's (#237).
    cd_ledger: MutationLedger,
    /// The foreground session's folder and branch (#237).
    folder: SessionFolder,
    /// The home folder `~` stands for in `/cd` (#237).
    pub home: Option<PathBuf>,
    /// How the `/cd` refusal names the interrupt key (#237).
    pub interrupt_label: Option<String>,
    /// The folder check and the move; a new one with every connection (#237).
    workspace_service: Option<Arc<WorkspaceService>>,
    counter: Arc<AtomicU64>,
    service: Option<SessionService>,
    link: Option<Link>,
    prompts: Option<Arc<Mutex<PromptService>>>,
    pending: Option<JoinHandle<Result<Ready, LaunchFailure>>>,
    command_service: Option<CommandService>,
    /// Mentions, paste labels and staged images for the open draft.
    pub attachments: AttachmentsState,
    caret: usize,
    attachment_epoch: u64,
    attachment_home: PathBuf,
    attachment_folder: PathBuf,
    submitted_attachments: BTreeMap<String, (String, SubmittedAttachments)>,
    held_attachments: VecDeque<SubmittedAttachments>,
    failed_attachments: VecDeque<(String, SubmittedAttachments, bool)>,
}

struct SubmittedAttachments {
    expanded: Expanded,
    media: Vec<Chip>,
}

impl AppState {
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        let launch_id = launch_id.into();
        let ledger = MutationLedger::new(&launch_id);
        let cd_ledger = MutationLedger::new(&format!("{launch_id}-cd"));
        Self {
            models: ModelsState::new(&launch_id, Ok(SavedModels::empty())),
            model_service: None,
            model_notice_counter: 0,
            model_warnings: BTreeSet::new(),
            attention: talaria_core::attention::AttentionTracker::new(
                talaria_core::attention::AttentionConfig {
                    bell: vec![
                        talaria_core::attention::AlertKind::NeedsYou,
                        talaria_core::attention::AlertKind::Failed,
                    ],
                    desktop: vec![
                        talaria_core::attention::AlertKind::NeedsYou,
                        talaria_core::attention::AlertKind::Done,
                        talaria_core::attention::AlertKind::Failed,
                    ],
                    done_after: Duration::from_secs(20),
                },
            ),
            attention_at: Instant::now(),
            opened: None,
            held: VecDeque::new(),
            launch_id,
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
            draft: String::new(),
            palette: PaletteState::closed(),
            commands: CommandsState::default(),
            local: LocalCommandTable::builtin(),
            rewind: RewindState::default(),
            double_esc: DoubleEsc::default(),
            retry: None,
            context: ContextTracker::new(),
            ledger,
            context_blocks: HeldBlocks::new(),
            now_ms: 0,
            appearance: AppearanceService::new(
                TerminalColors::default(),
                (Polarity::Dark, PolaritySource::Default),
            ),
            key_bindings: BTreeMap::new(),
            pager_open: false,
            pager_typing: false,
            pager_confirmed: false,
            pager_has_last_confirmed: false,
            pager_confirmed_before_typing: false,
            pager_typed_len: 0,
            source_epoch: 0,
            checklist: ChecklistTracker::new(),
            open_keys: Vec::new(),
            counter: Arc::new(AtomicU64::new(0)),
            toggle_label: Some(DEFAULT_TOGGLE_LABEL.to_string()),
            permissions: None,
            permissions_service: None,
            cd: CdState::new(),
            cd_ledger,
            folder: SessionFolder::default(),
            home: None,
            interrupt_label: None,
            workspace_service: None,
            service: None,
            link: None,
            prompts: None,
            pending: None,
            command_service: None,
            attachments: AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024),
            caret: 0,
            attachment_epoch: 0,
            attachment_home: PathBuf::new(),
            attachment_folder: PathBuf::new(),
            submitted_attachments: BTreeMap::new(),
            held_attachments: VecDeque::new(),
            failed_attachments: VecDeque::new(),
        }
    }

    /// The view the screen draws from.
    #[must_use]
    pub fn view(&self) -> View<'_> {
        let opened = self.opened.as_ref();
        View {
            state: opened.map(|opened| &opened.state),
            session_id: opened.map(|opened| opened.session_id.as_str()),
            title: opened
                .and_then(|opened| opened.title.as_deref())
                .or(self.list_title.as_deref()),
            info: opened.map(|opened| &opened.info),
            link: self.link_state,
            profile: &self.profile,
            model: self
                .model
                .as_deref()
                .or_else(|| opened.and_then(|opened| opened.info.model.as_deref())),
            effort: self.effort.as_deref(),
            masked: &self.masked,
            held: self.held.len(),
            palette: &self.palette,
            rewind: self.rewind.is_open().then(|| self.rewind.view(wall_ms())),
            context_hidden: self
                .opened
                .as_ref()
                .is_some_and(|opened| self.context.context_hidden(&opened.session_id)),
            compacting_line: self
                .opened
                .as_ref()
                .and_then(|opened| self.context.compacting_line(&opened.session_id)),
            now_ms: self.now_ms,
            pager_open: self.pager_open,
            pager_typing: self.pager_typing,
            source_epoch: self.source_epoch,
            checklist: self.checklist.live(),
            checklist_expanded: self.checklist.expanded(),
            approvals: self
                .permissions
                .as_ref()
                .and_then(PermissionsState::segment),
            folder: opened.map(|_| &self.folder),
        }
    }

    fn next_submission_id(&mut self) -> String {
        self.prompt_counter = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        submission_id(&self.launch_id, self.prompt_counter)
    }

    fn has_user_message(&self) -> bool {
        self.opened.as_ref().is_some_and(|opened| {
            opened
                .state
                .cells()
                .iter()
                .any(|cell| matches!(cell, Cell::User(_)))
        })
    }
}

/// One key the terminal delivered. Issue 222 routes it to the transcript when that screen is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputKey {
    /// The key, without modifiers.
    pub code: InputCode,
    /// Control was held.
    pub ctrl: bool,
    /// Alt was held.
    pub alt: bool,
    /// Shift was held.
    pub shift: bool,
}

/// The key half of [`InputKey`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCode {
    /// A character, including space.
    Char(char),
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
}

/// What the application asks of the transcript screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptAction {
    /// Open the transcript.
    Open,
    /// Close the transcript.
    Close,
    /// Apply one key to the transcript.
    Key(InputKey),
    /// The session changed while the transcript was open.
    Changed,
}

/// One input to [`update`].
#[allow(clippy::large_enum_variant)]
pub enum AppMessage {
    Models(ModelsInput),
    User(UserAction),
    /// A key from the terminal.
    Input(InputKey),
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
    /// A key while Talaria is in the foreground. Issue #223 routes the open palette here.
    ForegroundKey(UserAction),
    /// A catalogue fetch or a completion finished.
    Commands(CommandReply),
    /// A slash command finished; `typed` is the line as it was run.
    CommandDone {
        typed: String,
        outcome: CommandOutcome,
    },
    /// A rewind call's reply, or another rewind input. Issue #233.
    Rewind(talaria_core::rewind::RewindInput),
    /// A local command's answer came back.
    ContextDone {
        block: ContextBlock,
    },
    /// A compaction call's answer came back.
    CompactDone {
        request: MutationRequest,
        outcome: MutationOutcome,
        typed: String,
    },
    /// One app-clock wake-up while side work may be pending.
    Tick {
        /// App-clock time now.
        now_ms: u64,
    },
    /// What a shell command or side question came back with.
    SideDone(Vec<Event>),
    /// A `/skin` command or a reconnect describe finished.
    AppearanceDone {
        /// The skin state it produced, when it changed one.
        skin: Option<AppearanceState>,
        /// Notice lines to show.
        lines: Vec<String>,
        /// The connection generation of a reconnect describe.
        generation: Option<u64>,
    },
    /// A clipboard, drop, or completion result.
    Attachment(crate::attachments::AttachmentEvent),
    AttachmentResult {
        epoch: u64,
        event: AttachmentEvent,
    },
    AttachmentTick(Instant),
    DraftSynced {
        text: String,
        caret: usize,
    },
    /// A permissions reducer input (#237).
    Permissions(PermissionsInput),
    /// A `/cd` folder check or move finished (#237).
    Cd(CdInput),
}

/// What the executor does after one update. Ranges index the open session's cells.
#[allow(clippy::large_enum_variant)]
pub enum AppEffect {
    Models(ModelsEffect),
    /// Run one rewind call; its reply comes back as [`AppMessage::Rewind`]. Issue #233.
    Rewind {
        ticket: u64,
        call: talaria_core::rewind::Call,
    },
    CommitLines(Vec<String>),
    /// Read the context or the usage of the open session.
    ContextRead {
        action: LocalAction,
        argument: String,
        typed: String,
    },
    /// Send a compaction request.
    CompactRequest {
        request: MutationRequest,
        typed: String,
    },
    /// Write a context block into the scrollback.
    CommitContext(ContextBlock),
    /// The task checklist, written into the scrollback as a block.
    CommitChecklist(ChecklistCommit),
    CommitCells(Range<usize>),
    Redraw,
    /// The transcript screen is open, closed, or takes a key.
    Transcript(TranscriptAction),
    LoadDraft(String),
    /// Take the draft back out of Up-arrow recall: the composer recorded it on Enter, but it
    /// was a hint or a usage notice, not a prompt.
    UnrecordSubmit(String),
    /// `prompt.submit`.
    Send(Effect),
    /// `shell.exec` or `prompt.btw`, started at once: never queued behind a prompt or a turn.
    Side(Effect),
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
    /// Settle a server request that Hermes withdrew.
    CancelRequest {
        id: String,
        method: String,
        reason: String,
    },
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
    /// The skin or polarity changed: the screen resolves the theme again.
    Appearance,
    /// Print the `/theme` report.
    ThemeReport,
    /// Run `/skin`, with its name argument if any.
    Skin(Option<String>),
    /// Describe again after a reconnect of this generation.
    Describe(u64),
    Quit,
    /// Run a Hermes slash command (`slash.exec`) in the background.
    RunCommand {
        session_id: String,
        typed: String,
    },
    /// `commands.catalog`, after `after`.
    FetchCatalog {
        request_id: u64,
        session_id: Option<String>,
        after: Duration,
    },
    /// `complete.slash` for the draft, once it has been still for `after`.
    RequestCompletion {
        request_id: u64,
        text: String,
        after: Duration,
        session_id: Option<String>,
    },
    /// Drop the pending completion request.
    CancelCompletion,
    /// Report the foreground pane state.
    ReportPane(talaria_core::attention::PaneReport),
    /// Emit a configured foreground alert.
    Alert(talaria_core::attention::Alert),
    Attachment(AttachmentEffect),
    /// Write the session bypass (#237).
    SetSessionOff {
        /// The session to change.
        session_id: String,
        /// The generation of this write.
        generation: u64,
        /// Whether the bypass should be on.
        off: bool,
    },
    /// Read an approval setting or write the profile's mode (#237).
    Permissions(PermissionsEffect),
    /// Check a `/cd` folder or send the move (#237).
    Cd(CdEffect),
}

fn notice_lines(text: &str) -> AppEffect {
    AppEffect::CommitLines(text.lines().map(|line| format!("\u{b7} {line}")).collect())
}

fn model_card_blocked(state: &AppState) -> bool {
    !state.masked.is_empty()
        || state
            .opened
            .as_ref()
            .is_some_and(|s| s.state.open_prompts().any(|p| p.state == PromptState::Open))
}

fn sync_models<S: Screen>(state: &mut AppState, screen: &mut S) {
    state.models.set_list_height(screen.model_list_height());
    screen.sync_models(
        state.models.view(),
        if model_card_blocked(state) {
            None
        } else {
            state.models.confirmation()
        },
    );
}

fn models_input(state: &mut AppState, input: ModelsInput) -> Vec<AppEffect> {
    let current = state.opened.as_ref().map(|s| s.session_id.as_str());
    match &input {
        ModelsInput::OptionsLoaded { session_id, result }
            if Some(session_id.as_str()) == current =>
        {
            state.model = Some(result.model.clone());
            if let Some(opened) = &mut state.opened {
                opened.info.provider = Some(result.provider.clone());
            }
        }
        ModelsInput::EffortRead { session_id, value } if Some(session_id.as_str()) == current => {
            state.effort = Some(value.clone().unwrap_or_else(|| "medium".into()));
        }
        ModelsInput::SessionEvent {
            session_id,
            payload,
            type_name,
        } if Some(session_id.as_str()) == current
            && type_name == "session.updated"
            && payload.get("status").and_then(Value::as_str) == Some("confirmation_required") =>
        {
            if let Some(model) = payload.get("committed_model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
        }
        ModelsInput::SessionEvent {
            session_id,
            payload,
            type_name,
        } if Some(session_id.as_str()) == current && type_name == "session.info" => {
            if let Some(model) = payload.get("model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
            if let Some(provider) = payload.get("provider").and_then(Value::as_str)
                && let Some(opened) = &mut state.opened
            {
                opened.info.provider = Some(provider.into());
            }
            if let Some(effort) = payload
                .get("reasoning_effort")
                .or_else(|| payload.get("reasoning"))
                .and_then(Value::as_str)
            {
                state.effort = Some(effort.into());
            }
        }
        ModelsInput::MutationReturned {
            request,
            outcome: talaria_core::mutation::MutationOutcome::Committed(receipt),
        } if Some(request.session_id.as_str()) == current => {
            if let Some(model) = receipt.get("model").and_then(Value::as_str) {
                state.model = Some(model.into());
            }
            if let Some(effort) = receipt.get("effort").and_then(Value::as_str) {
                state.effort = Some(effort.into());
            }
            if let Some(provider) = receipt.get("provider").and_then(Value::as_str)
                && let Some(opened) = &mut state.opened
            {
                opened.info.provider = Some(provider.into());
            }
        }
        _ => {}
    }
    state
        .models
        .update(input)
        .into_iter()
        .map(AppEffect::Models)
        .collect()
}

/// How a foreground rebuild was caused.
pub enum RebuildMode {
    /// Gap recovery, unchanged.
    AfterGap,
    /// A rewind or a restore finished. `notice` is committed and nothing above it is reprinted.
    AfterRewind { notice: Vec<String> },
}

/// Replace the foreground session from a fresh snapshot.
///
/// `AfterGap` calls the existing gap recovery. `AfterRewind` is issue 233.
pub fn rebuild_foreground(
    state: &mut AppState,
    snapshot: Value,
    mode: RebuildMode,
) -> Vec<AppEffect> {
    match mode {
        RebuildMode::AfterGap => rebuild_after_gap(state, snapshot),
        RebuildMode::AfterRewind { notice } => rebuild_after_rewind(state, snapshot, notice),
    }
}

fn rebuild_after_gap(state: &mut AppState, snapshot: Value) -> Vec<AppEffect> {
    let result =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => Ok(crate::sessions::OpenedSession::from_snapshot(decoded)),
            Err(_) => Err(crate::sessions::OpenError::Link(
                "the session reply did not decode".to_string(),
            )),
        };
    recovered(state, result)
}

/// Replace the foreground state after a rewind or a restore without printing anything again.
///
/// Cells already in scrollback are matched in order and skipped; the notice is committed, then
/// any final cell this pane never showed (another window's new turn), once.
fn rebuild_after_rewind(
    state: &mut AppState,
    snapshot: Value,
    notice: Vec<String>,
) -> Vec<AppEffect> {
    let fresh =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => crate::sessions::OpenedSession::from_snapshot(decoded),
            Err(_) => {
                return vec![notice_lines(&rewind_text::reload_failed(
                    "the session reply did not decode",
                ))];
            }
        };
    let mut old = state.opened.take();
    let shown: &[Cell] = old.as_ref().map_or(&[], |old| {
        &old.state.cells()[..state.committed_cells.min(old.state.cells().len())]
    });
    let cells = fresh.state.cells();
    let total = final_prefix(cells);
    let mut position = 0;
    let mut unseen: Vec<usize> = Vec::new();
    for (index, cell) in cells[..total].iter().enumerate() {
        match shown[position..].iter().position(|seen| seen == cell) {
            Some(offset) => position += offset + 1,
            None => unseen.push(index),
        }
    }
    let mut effects = Vec::new();
    if !notice.is_empty() {
        effects.push(AppEffect::CommitLines(notice));
    }
    let mut start = 0;
    while start < unseen.len() {
        let mut end = start + 1;
        while end < unseen.len() && unseen[end] == unseen[end - 1] + 1 {
            end += 1;
        }
        effects.push(AppEffect::CommitCells(unseen[start]..unseen[end - 1] + 1));
        start = end;
    }
    state.committed_cells = total;
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    let mut fresh = fresh;
    if let Some(old) = &mut old {
        carry_over(&mut old.state, &mut fresh.state);
    }
    if fresh.title.is_none() {
        fresh.title = old.and_then(|old| old.title);
    }
    state.folder =
        SessionFolder::from_snapshot(fresh.info.cwd.as_deref(), fresh.info.branch.as_deref());
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    state.opened = Some(fresh);
    state.source_epoch = state.source_epoch.wrapping_add(1);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    effects
}

/// Wall-clock milliseconds, for the double Esc window and the prompts' ages.
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// One of the five rewind commands, typed or picked. Issue #233.
fn rewind_command(state: &mut AppState, action: LocalAction, argument: &str) -> Vec<AppEffect> {
    let name = match action {
        LocalAction::Rewind => "rewind",
        LocalAction::Undo => "undo",
        LocalAction::Retry => "retry",
        LocalAction::Rollback => "rollback",
        _ => "diff",
    };
    if state.link_failed {
        return vec![notice_lines(&format!(
            "Talaria is not connected to Hermes, so /{name} did not run."
        ))];
    }
    let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
        return vec![notice_lines(&no_session_notice(name))];
    };
    let turn_running = *opened.state.turn() == talaria_core::session::TurnState::Running;
    let input = match action {
        LocalAction::Rewind => RewindInput::Open { turn_running },
        LocalAction::Undo => match parse_undo(argument) {
            Ok(n) => RewindInput::Undo { n, turn_running },
            Err(()) => return vec![notice_lines(rewind_text::UNDO_USAGE)],
        },
        LocalAction::Retry => RewindInput::Retry { turn_running },
        LocalAction::Rollback => match parse_rollback(argument) {
            RollbackCommand::List => RewindInput::OpenCheckpoints,
            RollbackCommand::Diff(target) => RewindInput::OpenDiff(DiffSource::Checkpoint(target)),
            RollbackCommand::Restore {
                target,
                file_path,
                all,
            } => RewindInput::TypedRestore {
                target,
                file_path,
                all,
                turn_running,
            },
            RollbackCommand::BadTarget(target) => {
                return vec![notice_lines(&rewind_text::no_such_checkpoint(&target))];
            }
            RollbackCommand::Usage => return vec![notice_lines(rewind_text::ROLLBACK_USAGE)],
        },
        _ => match parse_diff_command(argument) {
            DiffCommand::Session { stat_only } => {
                RewindInput::OpenDiff(DiffSource::Session { stat_only })
            }
            DiffCommand::GitView => return vec![notice_lines(rewind_text::GIT_DIFF_HINT)],
        },
    };
    rewind_input(state, input)
}

/// Apply one rewind input and carry out what it asks for.
fn rewind_input(state: &mut AppState, input: RewindInput) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in rewind_update(&mut state.rewind, input, wall_ms()) {
        match effect {
            RewindEffect::Call { ticket, call } => effects.push(AppEffect::Rewind { ticket, call }),
            RewindEffect::Rebuild { snapshot, notice } => effects.extend(rebuild_foreground(
                state,
                snapshot,
                RebuildMode::AfterRewind { notice },
            )),
            RewindEffect::ReturnPrompt { text, attachments } => {
                effects.extend(return_prompt(state, text, attachments));
            }
            RewindEffect::Submit(text) => {
                let submission_id = state.next_submission_id();
                state.retry = Some((submission_id.clone(), text.clone()));
                effects.extend(reduce(
                    state,
                    Event::Submit {
                        display: text.clone(),
                        text,
                        submission_id,
                        attachments: Vec::new(),
                    },
                ));
            }
            RewindEffect::Notice(_, text) => effects.push(notice_lines(&text)),
        }
    }
    effects.push(AppEffect::Redraw);
    effects
}

/// Put a rewound prompt back, but only into an empty composer; a draft is never overwritten.
fn return_prompt(state: &mut AppState, text: String, attachments: bool) -> Vec<AppEffect> {
    let mut effects = if !state.draft.is_empty() {
        vec![notice_lines(&rewind_text::draft_kept(&text))]
    } else if text.is_empty() {
        Vec::new()
    } else {
        load_draft(state, text)
    };
    if attachments {
        effects.push(notice_lines(rewind_text::ATTACHMENTS_NOT_RESTORED));
    }
    effects
}

/// Apply one message. No input or output: the effects say what to do.
pub fn update(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let mut snapshot = attention_snapshot(state);
    let previous_session = state
        .opened
        .as_ref()
        .map(|o| (o.session_id.clone(), o.title.clone()));
    let restored = matches!(&message, AppMessage::Opened { .. });
    let mut redraw = match &message {
        AppMessage::PromptDone { events, notices, masked } => {
            !notices.is_empty() || masked.is_some() || events.iter().any(|event| {
                !matches!(event, Event::Session { frame, .. } if frame.type_name() == Some("todo.updated"))
            })
        }
        AppMessage::Link(LinkEvent::Inbound { item: Inbound::Event(event), .. }) => {
            event.frame.type_name() != Some("todo.updated")
        }
        AppMessage::Input(key) if !state.pager_open && tasks_key(state, key) => false,
        _ => true,
    };
    let mut effects = match message {
        AppMessage::Models(input) => models_input(state, input),
        // The toggle asks for its own redraw: one when a panel is there, none otherwise.
        AppMessage::User(UserAction::ToggleTasks)
        | AppMessage::ForegroundKey(UserAction::ToggleTasks) => {
            redraw = false;
            user_action(state, UserAction::ToggleTasks)
        }
        AppMessage::User(action) => user_action(state, action),
        AppMessage::Link(event) => {
            let mut effects = skin_changed(state, &event);
            if is_session_less_skin(&event) {
                // A skin equal to the current one draws nothing.
                if effects.is_empty() {
                    return effects;
                }
                effects.push(AppEffect::Redraw);
                return effects;
            }
            effects.extend(link_event(state, event));
            effects
        }
        AppMessage::Input(key) => input_key(state, key),
        AppMessage::Opened { opened, notice } => opened_session(state, opened, notice),
        AppMessage::SubmitDone {
            submission_id,
            outcome,
        } => submit_done(state, submission_id, outcome),
        AppMessage::InterruptDone { outcome, notices } => interrupt_done(state, outcome, notices),
        AppMessage::PromptDone {
            events,
            notices,
            masked,
        } => {
            if let Some(masked) = masked {
                state.masked = masked;
            }
            let mut effects: Vec<AppEffect> =
                notices.iter().map(|text| notice_lines(text)).collect();
            for event in events {
                effects.extend(reduce(state, event));
            }
            effects
        }
        AppMessage::Recovered(result) => recovered(state, result),
        AppMessage::Rewind(input) => rewind_input(state, input),
        AppMessage::ForegroundKey(action) => foreground_key(state, action),
        AppMessage::Commands(CommandReply::Commands(input)) => commands_input(state, input),
        AppMessage::Commands(CommandReply::Palette(input)) => {
            let (effects, _) = palette(state, input);
            effects
        }
        AppMessage::CommandDone { typed, outcome } => command_done(state, &typed, outcome),
        AppMessage::ContextDone { block } => context_done(state, block),
        AppMessage::CompactDone {
            request,
            outcome,
            typed,
        } => compact_done(state, request, outcome, typed),
        AppMessage::Tick { now_ms } => {
            state.now_ms = now_ms;
            let pending = state
                .opened
                .as_ref()
                .is_some_and(|opened| !opened.state.side().pending.is_empty());
            // Idle means zero frames: with nothing pending a wake-up changes nothing.
            if !pending {
                return Vec::new();
            }
            reduce(state, Event::SideClock { now_ms })
        }
        AppMessage::SideDone(events) => {
            let mut effects = Vec::new();
            for event in events {
                effects.extend(reduce(state, event));
            }
            effects
        }
        AppMessage::AppearanceDone {
            skin,
            lines,
            generation,
        } => {
            let mut effects = Vec::new();
            if !lines.is_empty() {
                effects.push(notice_lines(&lines.join("\n")));
            }
            let current =
                generation.is_none_or(|generation| generation >= state.appearance.generation());
            if let Some(skin) = skin.filter(|_| current)
                && state.appearance.adopt(skin) != AppearanceChange::Unchanged
            {
                effects.push(AppEffect::Appearance);
            }
            effects
        }
        AppMessage::Attachment(event) => attachment_event(state, event),
        AppMessage::AttachmentResult { epoch, event } => {
            if epoch == state.attachment_epoch {
                attachment_event(state, event)
            } else {
                Vec::new()
            }
        }
        AppMessage::AttachmentTick(now) => {
            let Some(opened) = &state.opened else {
                state.attachments.mention.close();
                return vec![AppEffect::Redraw];
            };
            let effects = state
                .attachments
                .on_tick(&SessionId::new(&opened.session_id), now);
            attachment_effects(effects)
        }
        AppMessage::DraftSynced { text, caret } => {
            state.draft = text;
            state.caret = caret;
            let mut effects = attachment_effects(state.attachments.sync_draft(&state.draft));
            if state.palette.draft != state.draft {
                effects.extend(palette_input(
                    state,
                    PaletteInput::DraftChanged {
                        text: state.draft.clone(),
                        caret,
                    },
                ));
            }
            effects
        }
        AppMessage::Permissions(input) => permissions(state, input),
        AppMessage::Cd(input) => cd_input(state, input),
    };
    if model_card_blocked(state) {
        effects.extend(models_input(state, ModelsInput::CardOpened));
    }
    if let Some(opened) = &state.opened {
        let session = (opened.session_id.clone(), opened.title.clone());
        if previous_session.as_ref() != Some(&session) {
            if previous_session.as_ref().map(|(id, _)| id) != Some(&opened.session_id) {
                snapshot = crate::comforts::AttentionSnapshot::from(
                    &SessionState::empty(SessionId::new("foreground")),
                    &[],
                );
            }
            let label = opened
                .title
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    opened
                        .info
                        .cwd
                        .as_deref()
                        .and_then(|cwd| std::path::Path::new(cwd).file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "session".into());
            let out = state
                .attention
                .observe(talaria_core::attention::Activity::SessionChanged {
                    session_id: opened.session_id.as_str().into(),
                    label,
                });
            if let Some(pane) = out.pane {
                effects.push(AppEffect::ReportPane(pane));
            } else {
                effects.push(AppEffect::ReportPane(state.attention.pane()));
            }
        }
    }
    for activity in crate::comforts::activities_between(
        &snapshot,
        &attention_snapshot(state),
        restored,
        state.attention_at,
    ) {
        let out = state.attention.observe(activity);
        if let Some(pane) = out.pane {
            effects.push(AppEffect::ReportPane(pane));
        }
        if let Some(alert) = out.alert {
            effects.push(AppEffect::Alert(alert));
        }
    }
    effects.extend(commit_ready(state));
    if !turn_running(state) {
        for block in state.context_blocks.release() {
            effects.push(AppEffect::CommitContext(block));
        }
    }
    effects.extend(checklist_effects(state));
    if redraw
        && !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
    {
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// Observe checklist changes after committing the turn's last cell.
fn checklist_effects(state: &mut AppState) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let observed = state
        .checklist
        .observe(opened.state.todo(), opened.state.turn());
    let mut effects: Vec<_> = observed
        .commit
        .map(AppEffect::CommitChecklist)
        .into_iter()
        .collect();
    if observed.live_changed {
        effects.push(AppEffect::Redraw);
    }
    effects
}

fn attention_snapshot(state: &AppState) -> crate::comforts::AttentionSnapshot {
    let empty = SessionState::empty(talaria_core::session::SessionId::new("foreground"));
    let masked: Vec<_> = state
        .masked
        .iter()
        .filter(|m| {
            m.session_id.as_deref().is_none_or(|id| {
                state
                    .opened
                    .as_ref()
                    .is_none_or(|o| o.session_id.as_str() == id)
            })
        })
        .cloned()
        .collect();
    crate::comforts::AttentionSnapshot::from(
        state.opened.as_ref().map_or(&empty, |o| &o.state),
        &masked,
    )
}

/// Whether the open session's turn is running.
fn turn_running(state: &AppState) -> bool {
    state
        .opened
        .as_ref()
        .is_some_and(|opened| *opened.state.turn() == TurnState::Running)
}

/// Offer a context block: it waits for the turn to end, or is committed at once.
fn commit_context_block(state: &mut AppState, block: ContextBlock) -> Vec<AppEffect> {
    let running = turn_running(state);
    state
        .context_blocks
        .push(block, running)
        .into_iter()
        .map(AppEffect::CommitContext)
        .collect()
}

/// A local command's answer came back: the block waits for the turn to end.
fn context_done(state: &mut AppState, block: ContextBlock) -> Vec<AppEffect> {
    commit_context_block(state, block)
}

/// A compaction call's answer came back.
fn compact_done(
    state: &mut AppState,
    request: MutationRequest,
    outcome: MutationOutcome,
    typed: String,
) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let session_id = opened.session_id.clone();
    // A fence refusal is retried once with the fresh fences.
    if let Some(retry) = state.ledger.retry_after_fence(&request, &outcome) {
        return vec![AppEffect::CompactRequest {
            request: retry,
            typed,
        }];
    }
    state.ledger.settle(&request, &settle_as(&outcome));
    state
        .context
        .finish_compact(&session_id, compact_end(&outcome));
    let result = interpret(&outcome, &session_id);
    commit_context_block(
        state,
        ContextBlock::Compact {
            typed: typed.clone(),
            result,
        },
    )
}

/// A local command of the typed line, when it names one.
fn local_line(text: &str) -> Option<Result<(LocalAction, String), String>> {
    let trimmed = text.trim();
    let name = trimmed.strip_prefix('/')?;
    let name = name.split_whitespace().next().unwrap_or(name);
    local::lookup(name)
        .filter(|command| command.action != LocalAction::Btw)
        .map(|_| local::parse(trimmed))
}

/// The effect a local command asks for.
fn local_effect(
    state: &mut AppState,
    action: LocalAction,
    argument: String,
    typed: String,
) -> Vec<AppEffect> {
    if action == LocalAction::Tasks {
        return crate::commands::local_action(action)
            .map_or_else(Vec::new, |action| user_action(state, action));
    }
    match action {
        LocalAction::Quit => {
            state.quitting = true;
            return vec![AppEffect::Quit];
        }
        LocalAction::Model
        | LocalAction::Effort
        | LocalAction::Skin
        | LocalAction::Theme
        | LocalAction::Permissions
        | LocalAction::Cd => {
            return model_command(state, action, &argument);
        }
        LocalAction::Rewind
        | LocalAction::Undo
        | LocalAction::Retry
        | LocalAction::Rollback
        | LocalAction::Diff => return rewind_command(state, action, &argument),
        _ => {}
    }
    if action != LocalAction::Compact {
        return vec![AppEffect::ContextRead {
            action,
            argument,
            typed,
        }];
    }
    let Some(opened) = state.opened.as_ref() else {
        return vec![notice_lines("No session is open.")];
    };
    let session_id = opened.session_id.clone();
    // The compaction is prepared against the session's own fences.
    state
        .ledger
        .fences_mut(&session_id)
        .adopt(Some(opened.revision), Some(opened.execution_generation));
    if state
        .context
        .begin_compact(&session_id, state.now_ms)
        .is_err()
    {
        return commit_context_block(
            state,
            ContextBlock::Compact {
                typed,
                result: talaria_core::context::compact::CompactResult::Refused {
                    text: talaria_core::context::text::T_C8.to_owned(),
                },
            },
        );
    }
    match state
        .ledger
        .build(&session_id, "compress", compact_payload(&argument), true)
    {
        Ok(request) => vec![AppEffect::CompactRequest { request, typed }],
        Err(outcome) => {
            state
                .context
                .finish_compact(&session_id, compact_end(&outcome));
            commit_context_block(
                state,
                ContextBlock::Compact {
                    typed,
                    result: interpret(&outcome, &session_id),
                },
            )
        }
    }
}

/// Feed the tracker one event and collect the notices it raises.
fn observe_context(state: &mut AppState, event: &Event) -> Vec<AppEffect> {
    let Event::Session { frame, raw, .. } = event else {
        return Vec::new();
    };
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let session_id = opened.session_id.clone();
    let Some(name) = frame.type_name() else {
        return Vec::new();
    };
    // The compaction is prepared against the session's fences, so the ledger follows
    // the revision and the generation the gateway reports.
    state
        .ledger
        .fences_mut(&session_id)
        .observe(&session_id, name, &raw["payload"]);
    let notices = state
        .context
        .observe(&session_id, name, &raw["payload"], state.now_ms);
    notices
        .into_iter()
        .flat_map(|notice| commit_context_block(state, ContextBlock::Notice(notice)))
        .collect()
}

fn attachment_effects(effects: Vec<AttachmentEffect>) -> Vec<AppEffect> {
    effects
        .into_iter()
        .map(|effect| match effect {
            AttachmentEffect::Notice(text) => notice_lines(&text),
            effect => AppEffect::Attachment(effect),
        })
        .collect()
}

fn attachment_event(state: &mut AppState, event: AttachmentEvent) -> Vec<AppEffect> {
    let Some(opened) = &state.opened else {
        return Vec::new();
    };
    let effects = state.attachments.on_event(
        event,
        &SessionId::new(&opened.session_id),
        &state.draft,
        state.caret,
    );
    let inserting = effects
        .iter()
        .any(|effect| matches!(effect, AttachmentEffect::EditDraft { .. }));
    let mut effects = attachment_effects(effects);
    if !inserting {
        effects.extend(restore_failed_attachments(state));
    }
    effects
}

/// Feed the reducer and turn its effects into executor effects.
fn reduce(state: &mut AppState, event: Event) -> Vec<AppEffect> {
    let observed = observe_context(state, &event);
    let Some(opened) = state.opened.as_mut() else {
        return Vec::new();
    };
    let produced = talaria_core::session::update(&mut opened.state, event);
    let mut effects = route(state, produced, None);
    effects.extend(observed);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    effects
}

fn route(
    state: &mut AppState,
    produced: Vec<Effect>,
    mut note: Option<(PromptKey, String)>,
) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            Effect::SendPrompt { .. } => effects.push(AppEffect::Send(effect)),
            Effect::Interrupt { .. } => {
                if !state.interrupting {
                    state.interrupting = true;
                    effects.push(AppEffect::Interrupt(effect));
                }
            }
            Effect::RespondApproval { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => effects.push(AppEffect::Respond {
                effect,
                note: note.take(),
            }),
            Effect::RunShell { .. } | Effect::AskSide { .. } => {
                effects.push(AppEffect::Side(effect));
            }
            // Issue 232. Queue and steer. The queue service will run these calls.
            Effect::CancelQueued { .. }
            | Effect::ReplaceQueued { .. }
            | Effect::SteerQueued { .. }
            | Effect::Steer { .. }
            | Effect::ResolveUnknown { .. } => {}
        }
    }
    effects
}

/// Feed the permissions reducer and turn its effects into executor effects. Before the first
/// session opens there is no permissions state, and inputs change nothing.
fn permissions(state: &mut AppState, input: PermissionsInput) -> Vec<AppEffect> {
    let label = state.toggle_label.clone();
    let Some(permissions) = state.permissions.as_mut() else {
        return Vec::new();
    };
    permissions
        .update(input, label.as_deref())
        .into_iter()
        .filter_map(|effect| match effect {
            PermissionsEffect::SetSessionOff {
                session_id,
                generation,
                off,
            } => Some(AppEffect::SetSessionOff {
                session_id,
                generation,
                off,
            }),
            PermissionsEffect::Notice { text, .. } => Some(notice_lines(&text)),
            // `update` ends every message with a redraw.
            PermissionsEffect::Redraw => None,
            effect => Some(AppEffect::Permissions(effect)),
        })
        .collect()
}

/// `/permissions`: a parsed command goes to the reducer; anything else prints the usage line.
fn permissions_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    match parse_permissions_command(argument) {
        Ok(command) => permissions(
            state,
            PermissionsInput::Command {
                command,
                now: Instant::now(),
            },
        ),
        Err(usage) => vec![notice_lines(&usage)],
    }
}

/// Feed the `/cd` reducer with the foreground session's folder, and turn its effects into
/// executor effects. A confirmed move replaces the one folder the status line draws.
fn cd_input(state: &mut AppState, input: CdInput) -> Vec<AppEffect> {
    let foreground = state
        .opened
        .as_ref()
        .filter(|_| !state.session_gone)
        .map(|opened| opened.session_id.clone());
    let turn_running = state
        .opened
        .as_ref()
        .is_some_and(|opened| *opened.state.turn() == talaria_core::session::TurnState::Running);
    let ctx = CdContext {
        session_id: foreground.as_deref(),
        turn_running,
        folder: &state.folder,
        home: state.home.as_deref(),
        interrupt_label: state.interrupt_label.as_deref(),
    };
    let produced = state.cd.update(&ctx, &mut state.cd_ledger, input);
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            CdEffect::UpdateFolder {
                session_id,
                cwd,
                branch,
            } => {
                if foreground.as_deref() == Some(session_id.as_str()) {
                    state.folder.apply_move(&cwd, branch.as_deref());
                }
            }
            CdEffect::Notice { kind, text } if state.opened.is_some() => {
                effects.extend(reduce(state, Event::Notice { kind, text }));
            }
            CdEffect::Notice { text, .. } => effects.push(notice_lines(&text)),
            effect @ (CdEffect::CheckFolder { .. } | CdEffect::Send(_)) => {
                effects.push(AppEffect::Cd(effect));
            }
        }
    }
    effects
}

fn user_action(state: &mut AppState, action: UserAction) -> Vec<AppEffect> {
    match action {
        UserAction::Rewind(key) => rewind_input(state, RewindInput::Key(key)),
        UserAction::RewindViewport(rows) => rewind_input(state, RewindInput::Viewport(rows)),
        UserAction::Escape { eligible } => {
            if state.double_esc.press(wall_ms(), eligible) && state.opened.is_some() {
                rewind_input(
                    state,
                    RewindInput::Open {
                        turn_running: false,
                    },
                )
            } else {
                Vec::new()
            }
        }
        UserAction::OpenModels => {
            state.attachments.mention.dismiss();
            models_input(
                state,
                ModelsInput::OpenPicker {
                    session_id: state.opened.as_ref().map(|s| s.session_id.clone()),
                    profile: state.profile.clone(),
                },
            )
        }
        UserAction::ModelPicker(key) => models_input(state, ModelsInput::Picker(key)),
        UserAction::ModelConfirm(key) => models_input(state, ModelsInput::Confirm(key)),
        UserAction::Submit(text) => {
            // The composer has taken the draft.
            let mut effects = if state.draft.is_empty() && !state.palette.open {
                Vec::new()
            } else {
                palette_input(
                    state,
                    PaletteInput::DraftChanged {
                        text: String::new(),
                        caret: 0,
                    },
                )
            };
            effects.extend(submit(state, text));
            effects
        }
        UserAction::Interrupt => {
            if state.interrupting {
                return Vec::new();
            }
            reduce(state, Event::Interrupt)
        }
        UserAction::Quit => {
            state.quitting = true;
            vec![AppEffect::Quit]
        }
        UserAction::Approve { key, choice, note } => {
            let Some(opened) = state.opened.as_mut() else {
                return Vec::new();
            };
            let note = note.map(|note| (key.clone(), note));
            let produced = talaria_core::session::update(
                &mut opened.state,
                Event::AnswerApproval { key, choice },
            );
            route(state, produced, note)
        }
        UserAction::Clarify { key, answer } => reduce(state, Event::AnswerClarify { key, answer }),
        UserAction::Masked { request_id, value } => {
            vec![AppEffect::AnswerMasked { request_id, value }]
        }
        UserAction::Notice(text) => vec![notice_lines(&text)],
        UserAction::Palette(input) => palette_input(state, input),
        UserAction::DraftEdited {
            text,
            caret,
            typed,
            recalled,
            now,
        } => {
            if text.is_empty() && !state.draft.is_empty() {
                state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
            }
            state.draft = text;
            state.caret = caret;
            let effects = if recalled {
                state.attachments.on_recall(&state.draft, caret, now)
            } else {
                state.attachments.on_edit(&state.draft, caret, typed, now)
            };
            let mut effects = attachment_effects(effects);
            effects.extend(palette_input(
                state,
                PaletteInput::DraftChanged {
                    text: state.draft.clone(),
                    caret,
                },
            ));
            effects.extend(restore_failed_attachments(state));
            effects
        }
        UserAction::AttachmentKey {
            action,
            text,
            caret,
            now,
        } => {
            state.draft = text;
            state.caret = caret;
            if state.opened.is_none() {
                return vec![notice_lines("No session is open yet.")];
            }
            let ctx = crate::attachments::RouteContext {
                turn_running: state.view().turn_running(),
                palette_open: state.palette.open,
            };
            match state
                .attachments
                .on_key(Some(action), &state.draft, caret, ctx, now)
            {
                crate::attachments::KeyRoute::Consumed(mut effects) => {
                    if action == crate::attachments::Action::CompletionAccept {
                        let session = SessionId::new(
                            &state.opened.as_ref().expect("open session").session_id,
                        );
                        effects.extend(state.attachments.complete_now(&session, now));
                    }
                    attachment_effects(effects)
                }
                crate::attachments::KeyRoute::PassToComposer => Vec::new(),
            }
        }
        UserAction::Paste { text, draft, caret } => {
            state.draft = draft;
            state.caret = caret;
            let Some(opened) = &state.opened else {
                return vec![AppEffect::Attachment(AttachmentEffect::EditDraft {
                    range: caret..caret,
                    text,
                })];
            };
            let folder = state
                .folder
                .cwd
                .as_deref()
                .or(opened.info.cwd.as_deref())
                .map(Path::new)
                .unwrap_or(&state.attachment_folder);
            let rule = state.attachments.rule();
            let effects = state
                .attachments
                .on_paste(&text, &SessionId::new(&opened.session_id), folder, &rule)
                .into_iter()
                .map(|effect| match effect {
                    AttachmentEffect::EditDraft { text, .. } => AttachmentEffect::EditDraft {
                        range: caret..caret,
                        text,
                    },
                    effect => effect,
                })
                .collect();
            attachment_effects(effects)
        }
        UserAction::Input(key) => input_key(state, key),
        UserAction::ToggleTasks => {
            // The panel form flips whatever has focus; the composer never sees the key.
            if !state.pager_open && state.checklist.toggle() {
                vec![AppEffect::Redraw]
            } else {
                Vec::new()
            }
        }
        // A completion list or a focused card keeps the key for itself (#237).
        UserAction::ApprovalsToggle { context } => match context {
            ToggleContext::Composer => permissions(state, PermissionsInput::ToggleKey),
            ToggleContext::Completion | ToggleContext::Card => Vec::new(),
        },
    }
}

/// Keys in the foreground: an open palette takes Enter and Esc before the composer and the
/// turn do, so Esc closes the palette and never interrupts a running turn.
fn foreground_key(state: &mut AppState, action: UserAction) -> Vec<AppEffect> {
    if !state.palette.open {
        return user_action(state, action);
    }
    match action {
        UserAction::Interrupt => palette_input(state, PaletteInput::Close),
        UserAction::Submit(text) => palette_enter(state, text),
        other => user_action(state, other),
    }
}

/// Apply one palette input and turn its completion request into an executor effect. The
/// draft and run effects come back for the caller, which knows what the composer holds.
fn palette(state: &mut AppState, input: PaletteInput) -> (Vec<AppEffect>, Vec<PaletteEffect>) {
    let pending = state.palette.request_id;
    let ctx = PaletteContext {
        catalog: state.commands.catalog.as_ref(),
        catalog_status: &state.commands.status,
        local: &state.local,
        refused: &state.commands.refused,
    };
    let produced = palette_update(&mut state.palette, input, &ctx);
    let mut effects = Vec::new();
    let mut rest = Vec::new();
    let mut requested = false;
    for effect in produced {
        match effect {
            PaletteEffect::RequestCompletion {
                request_id,
                text,
                after,
            } => {
                requested = true;
                effects.push(AppEffect::RequestCompletion {
                    request_id,
                    text,
                    after,
                    session_id: state.commands.bound_session.clone(),
                });
            }
            PaletteEffect::Redraw => {}
            other => rest.push(other),
        }
    }
    if pending != 0 && !requested && state.palette.request_id != pending {
        effects.insert(0, AppEffect::CancelCompletion);
    }
    (effects, rest)
}

/// The composer will hold `text`: load it and let the palette follow.
fn load_draft(state: &mut AppState, text: String) -> Vec<AppEffect> {
    state.draft.clone_from(&text);
    let caret = text.len();
    let (mut effects, _) = palette(
        state,
        PaletteInput::DraftChanged {
            text: text.clone(),
            caret,
        },
    );
    effects.insert(0, AppEffect::LoadDraft(text));
    effects
}

/// Follow what a palette key asked for: run a line (the composer is left empty), or put a
/// completed draft into the composer.
fn follow(state: &mut AppState, produced: Vec<PaletteEffect>) -> Vec<AppEffect> {
    let mut replace = None;
    for effect in produced {
        match effect {
            PaletteEffect::Run { line } => {
                state.draft.clear();
                let (mut effects, _) = palette(
                    state,
                    PaletteInput::DraftChanged {
                        text: String::new(),
                        caret: 0,
                    },
                );
                effects.extend(submit(state, line));
                return effects;
            }
            PaletteEffect::ReplaceDraft { text } => replace = Some(text),
            PaletteEffect::RequestCompletion { .. } | PaletteEffect::Redraw => {}
        }
    }
    replace.map_or_else(Vec::new, |text| load_draft(state, text))
}

fn palette_input(state: &mut AppState, input: PaletteInput) -> Vec<AppEffect> {
    if let PaletteInput::DraftChanged { text, .. } = &input {
        state.draft.clone_from(text);
    }
    let (mut effects, produced) = palette(state, input);
    effects.extend(follow(state, produced));
    effects
}

/// Enter while the palette is open. The composer has already taken the draft, so when the
/// palette neither runs nor completes anything the draft goes back.
fn palette_enter(state: &mut AppState, text: String) -> Vec<AppEffect> {
    let (mut effects, produced) = palette(state, PaletteInput::Enter);
    if produced.is_empty() {
        effects.push(AppEffect::LoadDraft(text));
        return effects;
    }
    effects.extend(follow(state, produced));
    effects
}

/// Apply one catalogue store input and turn its effects into executor effects.
fn commands_input(state: &mut AppState, input: CommandsInput) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in state_update(&mut state.commands, input) {
        match effect {
            CommandsEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            } => effects.push(AppEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            }),
            CommandsEffect::CatalogUpdated => {
                let (refreshed, _) = palette(state, PaletteInput::CatalogChanged);
                effects.extend(refreshed);
            }
        }
    }
    effects
}

/// The catalogue store follows the connection only once the session service exists.
fn commands_follow(state: &mut AppState, input: CommandsInput) -> Vec<AppEffect> {
    if state.service.is_none() {
        return Vec::new();
    }
    commands_input(state, input)
}

/// Apply a finished command's outcome: notices into the transcript, a directive's prompt
/// through the normal submission path with a fresh id, a refused bare name dimmed.
fn command_done(state: &mut AppState, typed: &str, outcome: CommandOutcome) -> Vec<AppEffect> {
    // Hermes's own `/yolo` and `/approvals` push no event, so both settings are read again (#237).
    let name = parse_slash_line(typed)
        .map(|line| line.name)
        .unwrap_or_default();
    let reread = state
        .opened
        .as_ref()
        .and_then(|opened| hermes_command_finished(&opened.session_id, &name, &outcome));
    let mut effects = Vec::new();
    for action in outcome_actions(outcome, typed) {
        match action {
            AppAction::Event(Event::SubmitCommand { text, display, .. }) => {
                let submission_id = state.next_submission_id();
                effects.extend(reduce(
                    state,
                    Event::SubmitCommand {
                        text,
                        display,
                        submission_id,
                    },
                ));
            }
            AppAction::Event(Event::Notice { kind, text }) if state.opened.is_some() => {
                effects.extend(reduce(state, Event::Notice { kind, text }));
            }
            AppAction::Event(Event::Notice { text, .. }) => effects.push(notice_lines(&text)),
            AppAction::Event(event) => effects.extend(reduce(state, event)),
            AppAction::LoadDraft(text) => effects.extend(load_draft(state, text)),
            AppAction::Refused { name } => {
                effects.extend(commands_input(state, CommandsInput::Refused { name }));
            }
            AppAction::Key(action) => effects.extend(user_action(state, action)),
            AppAction::CatalogUpdated => {
                let (refreshed, _) = palette(state, PaletteInput::CatalogChanged);
                effects.extend(refreshed);
            }
        }
    }
    if let Some(input) = reread {
        effects.extend(permissions(state, input));
    }
    effects
}

/// A slash line that names a Hermes command: run it through the session's gateway, or say why
/// it cannot run. Nothing is sent without a session.
fn run_command(state: &mut AppState, typed: String) -> Vec<AppEffect> {
    let name = parse_slash_line(&typed)
        .map(|line| line.name)
        .unwrap_or_default();
    if state.link_failed {
        return vec![notice_lines(&format!(
            "Talaria is not connected to Hermes, so /{name} did not run."
        ))];
    }
    let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
        return vec![notice_lines(&no_session_notice(&name))];
    };
    vec![AppEffect::RunCommand {
        session_id: opened.session_id.clone(),
        typed,
    }]
}

/// The model picker's own commands, which the local table names and the loop turns into
/// one model input.
fn model_command(state: &mut AppState, action: LocalAction, argument: &str) -> Vec<AppEffect> {
    let command = match action {
        LocalAction::Skin => {
            let name = Some(argument.trim().to_owned()).filter(|name| !name.is_empty());
            return vec![AppEffect::Skin(name)];
        }
        LocalAction::Theme => return theme_command(state, argument.trim()),
        LocalAction::Model => parse_model_command(argument).map(ModelCommandOrEffort::Model),
        LocalAction::Effort => parse_effort_command(argument).map(ModelCommandOrEffort::Effort),
        LocalAction::Rewind
        | LocalAction::Undo
        | LocalAction::Retry
        | LocalAction::Rollback
        | LocalAction::Diff => return rewind_command(state, action, argument),
        LocalAction::Permissions => return permissions_command(state, argument),
        LocalAction::Cd => return cd_input(state, CdInput::Command(parse_cd_command(argument))),
        _ => return Vec::new(),
    };
    models_input(
        state,
        ModelsInput::Command {
            session_id: state.opened.as_ref().map(|s| s.session_id.clone()),
            profile: state.profile.clone(),
            command,
        },
    )
}

/// One key binding, read from the configuration file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Chord {
    /// The key, with the letter in lower case.
    code: InputCode,
    /// Control was held.
    ctrl: bool,
    /// Alt was held.
    alt: bool,
    /// Shift was held.
    shift: bool,
}

impl Chord {
    /// Read a chord such as `ctrl+o` or `q`.
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('+').map(str::trim);
        let key = parts.next_back()?;
        let mut chord = Self {
            code: InputCode::Char(' '),
            ctrl: false,
            alt: false,
            shift: false,
        };
        for modifier in parts {
            match modifier.to_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                _ => return None,
            }
        }
        chord.code = key_code(key, &mut chord.shift)?;
        Some(chord)
    }

    /// Whether this chord is the key.
    fn matches(&self, key: &InputKey) -> bool {
        if self.ctrl != key.ctrl || self.alt != key.alt {
            return false;
        }
        match (self.code, key.code) {
            (InputCode::Char(want), InputCode::Char(got)) => {
                let shifted = key.shift || got.is_uppercase();
                lower(got) == want && self.shift == shifted
            }
            (want, got) => want == got,
        }
    }
}

/// The key of a chord, with an uppercase letter meaning shift.
fn key_code(text: &str, shift: &mut bool) -> Option<InputCode> {
    let name = text.to_lowercase();
    let code = match name.as_str() {
        "enter" => InputCode::Enter,
        "esc" | "escape" => InputCode::Esc,
        "backspace" => InputCode::Backspace,
        "up" => InputCode::Up,
        "down" => InputCode::Down,
        "pageup" => InputCode::PageUp,
        "pagedown" => InputCode::PageDown,
        "home" => InputCode::Home,
        "end" => InputCode::End,
        "space" => InputCode::Char(' '),
        _ => {
            let mut characters = text.chars();
            let character = characters.next()?;
            if characters.next().is_some() {
                return None;
            }
            if character.is_uppercase() {
                *shift = true;
            }
            InputCode::Char(lower(character))
        }
    };
    Some(code)
}

/// The lower-case form of a character, when it is one character.
fn lower(character: char) -> char {
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

/// The chords bound to one action, with the built-in default when the file names none.
fn chords(state: &AppState, name: &str, default: &str) -> Vec<Chord> {
    let texts: Vec<&str> = match state.key_bindings.get(name) {
        Some(texts) => texts.iter().map(String::as_str).collect(),
        None => vec![default],
    };
    texts.iter().filter_map(|text| Chord::parse(text)).collect()
}

/// Route one key to the transcript screen.
///
/// The screen opens with the transcript key. While a search is being typed, the keys belong to
/// the query, so a letter that would otherwise close the screen is typed instead. Escape clears
/// a confirmed search and leaves the screen open. A second Escape, the close key, the transcript
/// key, and Control-C close it. Control-C never interrupts the turn.
fn input_key(state: &mut AppState, key: InputKey) -> Vec<AppEffect> {
    let open = chords(state, "transcript_open", "ctrl+o");
    let close = chords(state, "pager_close", "q");
    let search = chords(state, "pager_search", "/");
    let is_open = open.iter().any(|chord| chord.matches(&key));
    let is_close = close.iter().any(|chord| chord.matches(&key));
    let is_search = search.iter().any(|chord| chord.matches(&key));
    let interrupt = key.ctrl && matches!(key.code, InputCode::Char('c'));

    if !state.pager_open {
        if is_open {
            state.pager_open = true;
            state.pager_typing = false;
            state.pager_confirmed = false;
            // A fresh pager has no remembered query.
            state.pager_has_last_confirmed = false;
            state.pager_confirmed_before_typing = false;
            state.pager_typed_len = 0;
            return vec![AppEffect::Transcript(TranscriptAction::Open)];
        }
        if tasks_key(state, &key) {
            return user_action(state, UserAction::ToggleTasks);
        }
        return Vec::new();
    }

    if interrupt {
        state.pager_open = false;
        state.pager_typing = false;
        state.pager_confirmed = false;
        return vec![AppEffect::Transcript(TranscriptAction::Close)];
    }
    if state.pager_typing {
        match key.code {
            InputCode::Enter => {
                state.pager_typing = false;
                if state.pager_typed_len > 0 || state.pager_has_last_confirmed {
                    state.pager_confirmed = true;
                    state.pager_has_last_confirmed = true;
                } else {
                    state.pager_confirmed = state.pager_confirmed_before_typing;
                }
            }
            InputCode::Esc => {
                state.pager_typing = false;
                state.pager_confirmed = state.pager_confirmed_before_typing;
            }
            InputCode::Backspace => {
                if state.pager_typed_len == 0 {
                    state.pager_typing = false;
                    state.pager_confirmed = state.pager_confirmed_before_typing;
                } else {
                    state.pager_typed_len -= 1;
                }
            }
            InputCode::Char(_) if !key.ctrl && !key.alt && state.pager_typed_len < 256 => {
                state.pager_typed_len += 1;
            }
            _ => {}
        }
        return vec![AppEffect::Transcript(TranscriptAction::Key(key))];
    }
    if is_open || is_close {
        state.pager_open = false;
        state.pager_confirmed = false;
        return vec![AppEffect::Transcript(TranscriptAction::Close)];
    }
    if key.code == InputCode::Esc {
        if state.pager_confirmed {
            state.pager_confirmed = false;
            return vec![AppEffect::Transcript(TranscriptAction::Key(key))];
        }
        state.pager_open = false;
        return vec![AppEffect::Transcript(TranscriptAction::Close)];
    }
    if is_search {
        state.pager_typing = true;
        state.pager_confirmed_before_typing = state.pager_confirmed;
        state.pager_typed_len = 0;
    }
    vec![AppEffect::Transcript(TranscriptAction::Key(key))]
}

fn tasks_key(state: &AppState, key: &InputKey) -> bool {
    chords(state, "toggle_tasks", "ctrl+t")
        .iter()
        .any(|chord| chord.matches(key))
}

/// Route a submitted draft. A `!` command is recognised first, on the raw draft, then a side
/// question, then slash lines, then the held-prompt rule and `prompt.submit`. Neither a command
/// nor a side question is ever held or queued behind a turn.
fn submit(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if let Some(command) = local_line(&text) {
        return match command {
            Ok((action, argument)) => {
                let typed = text.trim().to_owned();
                local_effect(state, action, argument, typed)
            }
            Err(reason) => vec![notice_lines(&reason), AppEffect::LoadDraft(text)],
        };
    }
    match classify(&text) {
        Submission::ShellEmpty => {
            return local_notice(state, Event::ShellEmpty, SHELL_EMPTY_HINT, text);
        }
        Submission::Shell(command) => {
            let started_ms = state.now_ms;
            return side_request(
                state,
                text,
                Event::ShellRequested {
                    command,
                    started_ms,
                },
            );
        }
        Submission::Prompt => {}
    }
    match slash_line(&text) {
        Slash::Btw(argument) => {
            return match btw_command(&argument, state.now_ms) {
                event @ Event::SideAsked { .. } => side_request(state, text, event),
                event => local_notice(state, event, SIDE_USAGE, text),
            };
        }
        Slash::Quit => {
            state.quitting = true;
            return vec![AppEffect::Quit];
        }
        Slash::Local(action, argument) => {
            let typed = text.trim().to_owned();
            return local_effect(state, action, argument, typed);
        }
        Slash::Notice(notice) => {
            return match state.opened {
                Some(_) => reduce(
                    state,
                    Event::Notice {
                        kind: NoticeKind::Error,
                        text: notice,
                    },
                ),
                None => vec![notice_lines(&notice)],
            };
        }
        Slash::Command(line) => return run_command(state, line),
        Slash::Nothing => return Vec::new(),
        Slash::NotSlash => {}
    }
    if state.session_gone || state.link_failed {
        let notice = if state.session_gone {
            SESSION_GONE_NOTICE
        } else {
            LINK_STOPPED_NOTICE
        };
        return vec![notice_lines(notice), AppEffect::LoadDraft(text)];
    }
    if state.opened.is_none() {
        return match hold_prompt(state, &text) {
            Ok(()) => match state.attachments.on_submit(&text) {
                Ok(expanded) => {
                    let media = state.attachments.chips.take_draft_payloads(&text);
                    state
                        .held_attachments
                        .push_back(SubmittedAttachments { expanded, media });
                    clear_submitted_draft(state);
                    Vec::new()
                }
                Err(notice) => {
                    state.held.pop_back();
                    vec![notice_lines(&notice), AppEffect::LoadDraft(text)]
                }
            },
            Err(notice) => vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
        };
    }
    send_text(state, text)
}

/// A blank-command hint or the usage line: a notice with or without a session, and no call.
/// The composer is already empty, and `draft` leaves Up-arrow recall: it was not a prompt.
fn local_notice(state: &mut AppState, event: Event, text: &str, draft: String) -> Vec<AppEffect> {
    let mut effects = if state.opened.is_some() {
        reduce(state, event)
    } else {
        vec![notice_lines(text)]
    };
    effects.push(AppEffect::UnrecordSubmit(draft));
    effects
}

/// A command or side question needs a session. Without one nothing is sent and the draft stays.
fn side_request(state: &mut AppState, draft: String, event: Event) -> Vec<AppEffect> {
    if state.opened.is_none() {
        return vec![notice_lines(NO_SESSION_NOTICE), AppEffect::LoadDraft(draft)];
    }
    reduce(state, event)
}

fn send_text(state: &mut AppState, text: String) -> Vec<AppEffect> {
    let expanded = match state.attachments.on_submit(&text) {
        Ok(expanded) => expanded,
        Err(notice) => return vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
    };
    let media = state.attachments.chips.take_draft_payloads(&text);
    send_expanded(state, SubmittedAttachments { expanded, media })
}

fn clear_submitted_draft(state: &mut AppState) {
    state.draft.clear();
    state.caret = 0;
    state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
}

fn send_expanded(state: &mut AppState, payload: SubmittedAttachments) -> Vec<AppEffect> {
    let submission_id = state.next_submission_id();
    let expanded = payload.expanded.clone();
    let owner = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
        .unwrap_or_default();
    state
        .submitted_attachments
        .insert(submission_id.clone(), (owner, payload));
    clear_submitted_draft(state);
    let mut effects = reduce(
        state,
        Event::Submit {
            display: expanded.display,
            text: expanded.text,
            submission_id,
            attachments: expanded.attachments,
        },
    );
    if let Some(notice) = reference_notice(expanded.reference_count) {
        effects.push(notice_lines(notice));
    }
    effects.extend(restore_failed_attachments(state));
    effects
}

/// `/theme` with its argument: the report, or a background override for this session.
fn theme_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if argument.is_empty() {
        return vec![AppEffect::ThemeReport];
    }
    let (lines, change) = state.appearance.theme_command(argument);
    let mut effects = vec![notice_lines(&lines.join("\n"))];
    if change != AppearanceChange::Unchanged {
        effects.push(AppEffect::Appearance);
    }
    effects
}

/// The `skin.changed` event in this link event, as `(params, replayed)`.
fn skin_event(event: &LinkEvent) -> Option<(&Value, bool)> {
    let LinkEvent::Inbound {
        item: Inbound::Event(inbound),
        replayed,
    } = event
    else {
        return None;
    };
    (inbound.raw.get("type").and_then(Value::as_str) == Some("skin.changed"))
        .then_some((&inbound.raw, *replayed))
}

/// The watcher's broadcast carries no session id; it is not a session event.
fn is_session_less_skin(event: &LinkEvent) -> bool {
    skin_event(event).is_some_and(|(raw, _)| {
        raw.get("session_id")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    })
}

/// Apply a `skin.changed` event; a replayed one is older than the describe after a reconnect.
fn skin_changed(state: &mut AppState, event: &LinkEvent) -> Vec<AppEffect> {
    let Some((raw, replayed)) = skin_event(event) else {
        return Vec::new();
    };
    let change = state.appearance.update(AppearanceInput::SkinChanged {
        payload: raw.get("payload").cloned().unwrap_or(Value::Null),
        replayed,
    });
    if change == AppearanceChange::Unchanged {
        Vec::new()
    } else {
        vec![AppEffect::Appearance]
    }
}

fn event_position(raw: &Value) -> (Option<&str>, Option<u64>) {
    (
        raw.get("replay_epoch").and_then(Value::as_str),
        raw.get("seq").and_then(Value::as_u64),
    )
}

fn is_open_session(state: &AppState, session_id: &str) -> bool {
    state
        .opened
        .as_ref()
        .is_some_and(|opened| opened.session_id == session_id)
}

fn link_event(state: &mut AppState, event: LinkEvent) -> Vec<AppEffect> {
    match event {
        LinkEvent::State(LinkState::Connected { generation }) => {
            state.link_state = LinkView::Connected;
            let mut effects = vec![AppEffect::ConnectionOpened(generation)];
            // A reconnect describes again: the skin may have changed while the link was down.
            if generation >= 2 {
                effects.push(AppEffect::Describe(generation));
            }
            if let Some(opened) = state.opened.as_mut() {
                let produced = note_connection(&mut opened.state, generation);
                effects.extend(route(state, produced, None));
            }
            effects.extend(commands_follow(state, CommandsInput::Connected));
            effects.extend(permissions(state, PermissionsInput::ConnectionOpened));
            effects
        }
        LinkEvent::State(LinkState::Reconnecting { .. }) => {
            state.link_state = LinkView::Reconnecting;
            Vec::new()
        }
        LinkEvent::State(LinkState::Closed) => {
            state.link_state = LinkView::Closed;
            Vec::new()
        }
        event @ LinkEvent::State(LinkState::Failed { .. }) => {
            state.link_state = LinkView::Failed;
            state.link_failed = true;
            link_notice(&event)
                .map(|text| vec![notice_lines(&text)])
                .unwrap_or_default()
        }
        LinkEvent::Inbound { item, .. } => match item {
            // A skill root moved: refetch the catalogue. It is not a session event.
            Inbound::Event(inbound)
                if inbound.raw.get("type").and_then(Value::as_str) == Some("skills.changed") =>
            {
                commands_follow(state, CommandsInput::SkillsChanged)
            }
            Inbound::Event(inbound) => {
                let Some(opened) = state.opened.as_mut() else {
                    return Vec::new();
                };
                let session = inbound.raw.get("session_id").and_then(Value::as_str);
                let (epoch, seq) = event_position(&inbound.raw);
                if session != Some(opened.session_id.as_str())
                    || !accept_event(&opened.cursor, epoch, seq)
                {
                    return Vec::new();
                }
                let session_id = opened.session_id.clone();
                let mut effects = Vec::new();
                match &inbound.frame.notification {
                    hermes_contract::events::Notification::RequestCancel(payload) => {
                        effects.push(AppEffect::CancelRequest {
                            id: payload.id.clone(),
                            method: payload.method.clone(),
                            reason: payload.reason.clone(),
                        })
                    }
                    hermes_contract::events::Notification::SessionTitle(payload)
                        if payload.session_id == opened.session_id.as_str() =>
                    {
                        opened.title = Some(payload.title.clone())
                    }
                    _ => {}
                }
                let type_name = inbound
                    .raw
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let payload = inbound.raw.get("payload").cloned().unwrap_or(Value::Null);
                let completed = type_name == "message.complete";
                if let Some(sequence) = seq {
                    opened.cursor.last_sequence = opened.cursor.last_sequence.max(sequence);
                }
                // `session.updated` still reaches the session reducer, which keeps the replay
                // cursor moving; a rewind from another window also rebuilds. Issue #233.
                let rewound = (type_name == "session.updated"
                    && payload.get("operation").and_then(Value::as_str) == Some("rewind"))
                .then(|| RewindInput::SessionUpdated {
                    operation: "rewind".to_string(),
                    revision: payload.get("revision").and_then(Value::as_i64),
                    rewound_count: payload.get("rewound_count").and_then(Value::as_u64),
                });
                let produced =
                    apply_inbound(&mut opened.state, &session_id, inbound.frame, inbound.raw);
                effects.extend(route(state, produced, None));
                // The move's fences and the folder come from this session's own events (#237).
                state
                    .cd_ledger
                    .fences_mut(&session_id)
                    .observe(&session_id, &type_name, &payload);
                if type_name == "session.info"
                    && let Some(cwd) = payload.get("cwd").and_then(Value::as_str)
                {
                    state.folder.apply_info_cwd(cwd);
                }
                if let Some(input) = rewound {
                    effects.extend(rewind_input(state, input));
                }
                if state.pager_open {
                    effects.push(AppEffect::Transcript(TranscriptAction::Changed));
                }
                effects.extend(models_input(
                    state,
                    ModelsInput::SessionEvent {
                        session_id: session_id.clone(),
                        type_name,
                        payload,
                    },
                ));
                if completed {
                    // Hermes pushes no event when an approval setting changes, so a finished
                    // turn reads both again (#237).
                    effects.extend(permissions(
                        state,
                        PermissionsInput::TurnCompleted { session_id },
                    ));
                }
                effects
            }
            Inbound::ServerRequest(request) => vec![AppEffect::ServerRequest(request)],
            Inbound::Malformed(_) | Inbound::Closed(_) => Vec::new(),
        },
        LinkEvent::Gap(gap) => {
            if is_open_session(state, &gap.session_id) {
                vec![AppEffect::Recover(gap)]
            } else {
                Vec::new()
            }
        }
        LinkEvent::Reattached {
            session_id,
            subscription_id,
        } => {
            if let Some(opened) = state
                .opened
                .as_mut()
                .filter(|opened| opened.session_id == session_id)
            {
                opened.subscription_id = subscription_id;
            }
            Vec::new()
        }
        event @ LinkEvent::SessionGone { .. } => {
            let LinkEvent::SessionGone { session_id } = &event else {
                return Vec::new();
            };
            if !is_open_session(state, session_id) {
                return Vec::new();
            }
            state.session_gone = true;
            let mut effects = link_notice(&event)
                .map(|text| vec![notice_lines(&text)])
                .unwrap_or_default();
            effects.extend(permissions(state, PermissionsInput::SessionClosed));
            effects
        }
    }
}

fn opened_session(
    state: &mut AppState,
    opened: OpenedSession,
    notice: Option<String>,
) -> Vec<AppEffect> {
    state.model = opened.info.model.clone();
    state.effort = opened.effort.clone();
    let mut effects = models_input(
        state,
        ModelsInput::ForegroundChanged {
            session_id: Some(opened.session_id.clone()),
        },
    );
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: opened.session_id.clone(),
            revision: Some(opened.revision),
            execution_generation: Some(opened.execution_generation),
        },
    ));
    for text in &opened.launch_notices {
        if !text.starts_with("~/.talaria/models.json could not be read (")
            || state.model_warnings.insert(text.clone())
        {
            effects.push(notice_lines(text));
        }
    }
    let title = session_title(
        opened.title.as_deref(),
        state.list_title.as_deref(),
        opened.state.cells(),
    );
    let (header, range) = resume_header(opened.state.cells(), &title);
    if !header.is_empty() {
        effects.push(AppEffect::CommitLines(header));
        effects.push(AppEffect::CommitCells(range.clone()));
    }
    state.committed_cells = range.end;
    effects.push(AppEffect::Track {
        session_id: opened.session_id.clone(),
        cursor: opened.cursor.clone(),
    });
    let session_id = opened.session_id.clone();
    state
        .failed_attachments
        .retain(|(owner, _, _)| owner == &session_id);
    state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
    effects.extend(attachment_effects(
        state
            .attachments
            .on_session_change(&SessionId::new(&opened.session_id)),
    ));
    state.folder =
        SessionFolder::from_snapshot(opened.info.cwd.as_deref(), opened.info.branch.as_deref());
    state
        .cd_ledger
        .fences_mut(&session_id)
        .adopt(Some(opened.revision), Some(opened.execution_generation));
    state.opened = Some(opened);
    state.link_state = LinkView::Connected;
    effects.extend(commands_follow(
        state,
        CommandsInput::ForegroundSession(Some(session_id.clone())),
    ));
    state.source_epoch += 1;
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    // The snapshot's plan: live while the turn runs, a block in the scrollback when the
    // session is idle, because there is no turn to show it in.
    let running = state
        .opened
        .as_ref()
        .is_some_and(|opened| *opened.state.turn() == TurnState::Running);
    let todo = state.opened.as_ref().and_then(|opened| opened.state.todo());
    if let Some(commit) = state.checklist.attach(todo, running) {
        effects.push(AppEffect::CommitChecklist(commit));
    }
    if let Some(notice) = notice.filter(|notice| !notice.is_empty()) {
        effects.push(notice_lines(&notice));
    }
    // The snapshot's `yolo` is never read: it is false after a gateway restart until the next
    // turn (#237). `install` made the state when the connection opened; a session that opens
    // without it makes it here.
    let profile = state.profile.clone();
    state
        .permissions
        .get_or_insert_with(|| PermissionsState::new(profile));
    effects.extend(permissions(
        state,
        PermissionsInput::SessionOpened { session_id },
    ));
    while let Some(text) = state.held.pop_front() {
        if state
            .held_attachments
            .front()
            .is_some_and(|payload| payload.expanded.display == text)
        {
            let payload = state
                .held_attachments
                .pop_front()
                .expect("checked held payload");
            effects.extend(send_expanded(state, payload));
        } else {
            effects.extend(send_text(state, text));
        }
    }
    effects
}

fn restore_failed_attachments(state: &mut AppState) -> Vec<AppEffect> {
    if !state.draft.is_empty() || state.attachments.has_pending_work() {
        return Vec::new();
    }
    while let Some((owner, payload, invalid_images)) = state.failed_attachments.pop_front() {
        if state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.as_str())
            != Some(owner.as_str())
        {
            continue;
        }
        state.attachments.chips.restore_payloads(payload.media);
        state
            .attachments
            .chips
            .restore_pastes(&payload.expanded.display);
        let mut effects = Vec::new();
        if invalid_images && !payload.expanded.attachments.is_empty() {
            state
                .attachments
                .chips
                .invalidate_images(&payload.expanded.attachments);
            effects.push(notice_lines(
                "Hermes no longer has the attached images; attach them again.",
            ));
        }
        state.draft = payload.expanded.display;
        state.caret = state.draft.len();
        state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
        effects.push(AppEffect::LoadDraft(state.draft.clone()));
        return effects;
    }
    Vec::new()
}

fn submit_done(state: &mut AppState, id: String, outcome: ExecuteOutcome) -> Vec<AppEffect> {
    let retried = state
        .retry
        .take_if(|(retry_id, _)| *retry_id == id)
        .map(|(_, text)| text);
    let mut effects = submit_outcome(state, id, outcome.clone());
    // `/retry` already removed the prompt, so a send that did not go through returns it.
    if let Some(text) = retried {
        match outcome {
            ExecuteOutcome::Refused { reason } => {
                effects.push(notice_lines(&rewind_text::resend_failed(&reason)));
                effects.extend(return_prompt(state, text, false));
            }
            ExecuteOutcome::OutcomeUnknown => effects.extend(return_prompt(state, text, false)),
            ExecuteOutcome::Sent | ExecuteOutcome::VoiceStopped => {}
        }
    }
    effects
}

fn submit_outcome(state: &mut AppState, id: String, outcome: ExecuteOutcome) -> Vec<AppEffect> {
    let mut attachment_notices = Vec::new();
    if let Some((owner, payload)) = state.submitted_attachments.remove(&id) {
        if state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.as_str())
            != Some(owner.as_str())
        {
            return Vec::new();
        }
        state.attachments.forget_submission(&payload.expanded);
        if matches!(
            outcome,
            ExecuteOutcome::Refused { .. } | ExecuteOutcome::OutcomeUnknown
        ) {
            let invalid_images = matches!(&outcome, ExecuteOutcome::Refused { reason } if reason == "invalid_params");
            state
                .failed_attachments
                .push_back((owner, payload, invalid_images));
            if !state.draft.is_empty() || state.attachments.has_pending_work() {
                attachment_notices.push(notice_lines(
                    "The earlier prompt is kept until the current draft is sent or cleared.",
                ));
            }
        }
    }
    let voice_stopped = matches!(outcome, ExecuteOutcome::VoiceStopped);
    let (reason, notice) = match outcome {
        ExecuteOutcome::Sent => return attachment_notices,
        ExecuteOutcome::Refused { reason } => ("refused", failure::prompt_refused_notice(&reason)),
        ExecuteOutcome::OutcomeUnknown => ("connection dropped", failure::prompt_unknown_notice()),
        ExecuteOutcome::VoiceStopped => ("voice stop", failure::voice_stop_notice()),
    };
    let mut effects = reduce(
        state,
        Event::SubmitFailed {
            submission_id: id,
            reason: reason.to_string(),
        },
    );
    effects.push(notice_lines(&notice));
    effects.extend(attachment_notices);
    effects.extend(restore_failed_attachments(state));
    if voice_stopped {
        effects.retain(|effect| !matches!(effect, AppEffect::LoadDraft(_)));
    }
    effects
}

fn interrupt_done(
    state: &mut AppState,
    outcome: ExecuteOutcome,
    notices: Vec<String>,
) -> Vec<AppEffect> {
    state.interrupting = false;
    let mut effects: Vec<AppEffect> = notices.iter().map(|text| notice_lines(text)).collect();
    let reason = match outcome {
        ExecuteOutcome::Sent => return effects,
        ExecuteOutcome::Refused { reason } => reason,
        ExecuteOutcome::OutcomeUnknown | ExecuteOutcome::VoiceStopped => {
            "connection_dropped".to_string()
        }
    };
    effects.extend(reduce(state, Event::InterruptFailed));
    effects.push(notice_lines(&failure::interrupt_refused_notice(&reason)));
    effects
}

fn recovered(state: &mut AppState, result: Result<OpenedSession, OpenError>) -> Vec<AppEffect> {
    let fresh = match result {
        Ok(fresh) => fresh,
        Err(OpenError::NotFound) => {
            state.session_gone = true;
            let mut effects = vec![notice_lines(SESSION_GONE_NOTICE)];
            effects.extend(permissions(state, PermissionsInput::SessionClosed));
            return effects;
        }
        Err(_) => return vec![notice_lines(REFRESH_FAILED_NOTICE)],
    };
    let Some(mut old) = state.opened.take() else {
        return Vec::new();
    };
    let shown = state.committed_cells;
    let mut effects = Vec::new();
    let keeps_prefix = fresh.state.cells().len() >= shown
        && old.state.cells().len() >= shown
        && fresh.state.cells()[..shown] == old.state.cells()[..shown];
    if !keeps_prefix {
        let cells = fresh.state.cells();
        let total = final_prefix(cells);
        let start = total.saturating_sub(RESUME_HISTORY_CELLS);
        let mut lines = vec!["\u{2500}\u{2500} Transcript refreshed \u{2500}\u{2500}".to_string()];
        if start > 0 {
            lines.push(format!("({start} earlier messages not shown)"));
        }
        effects.push(AppEffect::CommitLines(lines));
        effects.push(AppEffect::CommitCells(start..total));
        state.committed_cells = total;
    }
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    let mut fresh = fresh;
    // A snapshot holds no shell or side work; pending rows keep their ids and timeout.
    carry_over(&mut old.state, &mut fresh.state);
    if fresh.title.is_none() {
        fresh.title = old.title;
    }
    state.folder =
        SessionFolder::from_snapshot(fresh.info.cwd.as_deref(), fresh.info.branch.as_deref());
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    state.opened = Some(fresh);
    state.source_epoch = state.source_epoch.wrapping_add(1);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    effects.push(notice_lines(REFRESHED_NOTICE));
    effects
}

/// Commit cells that became final, and tell the prompt service when the open prompts changed.
fn commit_ready(state: &mut AppState) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let mut effects = Vec::new();
    let ready = opened.state.final_prefix_len();
    if ready > state.committed_cells {
        effects.push(AppEffect::CommitCells(state.committed_cells..ready));
    }
    state.committed_cells = state
        .committed_cells
        .max(ready)
        .min(opened.state.cells().len());
    let open: Vec<PromptKey> = opened
        .state
        .open_prompts()
        .map(|prompt| prompt.key.clone())
        .collect();
    if open != state.open_keys {
        state.open_keys.clone_from(&open);
        effects.push(AppEffect::PromptsChanged(open));
    }
    effects
}

fn final_prefix(cells: &[Cell]) -> usize {
    cells.iter().take_while(|cell| cell.is_final()).count()
}

fn cut(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// The separator title: the session's title, else the picked list row's, else the first line
/// of the first user message, else `untitled session`; cut to [`TITLE_CHARS`].
#[must_use]
pub fn session_title(info_title: Option<&str>, list_title: Option<&str>, cells: &[Cell]) -> String {
    let first_user = cells.iter().find_map(|cell| match cell {
        Cell::User(user) => user.text.lines().next().map(str::to_string),
        _ => None,
    });
    [
        info_title.map(str::to_string),
        list_title.map(str::to_string),
        first_user,
    ]
    .into_iter()
    .flatten()
    .map(|title| plain(&title))
    .find(|title| !title.trim().is_empty())
    .map_or_else(
        || "untitled session".to_string(),
        |title| cut(&title, TITLE_CHARS),
    )
}

/// The separator and marker for an opened session, and the cells to commit after them. A new
/// session with no cells gets nothing.
#[must_use]
pub fn resume_header(cells: &[Cell], title: &str) -> (Vec<String>, Range<usize>) {
    let total = final_prefix(cells);
    if total == 0 {
        return (Vec::new(), 0..0);
    }
    let start = total.saturating_sub(RESUME_HISTORY_CELLS);
    let mut lines = vec![format!(
        "\u{2500}\u{2500} Resumed: {title} \u{2500}\u{2500}"
    )];
    if start > 0 {
        lines.push(format!("({start} earlier messages not shown)"));
    }
    (lines, start..total)
}

/// One plain line for a cell, its first line of text.
fn cell_line(cell: &Cell) -> String {
    let text = match cell {
        Cell::User(user) => user.text.clone(),
        Cell::Assistant(assistant) => assistant.text.clone(),
        Cell::Reasoning(reasoning) => reasoning.text.clone(),
        Cell::Tool(tool) => tool.name.clone(),
        Cell::Notice(notice) => notice.text.clone(),
        Cell::Prompt(prompt) => prompt.command.clone().unwrap_or_default(),
        Cell::Shell(shell) => format!("! {}", shell.command),
        Cell::Side(side) => format!("btw \u{b7} {}", side.question),
    };
    plain(text.lines().next().unwrap_or_default())
}

/// The resumed history as plain lines: separator, marker, then one line per shown cell.
#[must_use]
pub fn resume_history(cells: &[Cell], title: &str) -> Vec<String> {
    let (mut lines, range) = resume_header(cells, title);
    lines.extend(cells[range].iter().map(cell_line));
    lines
}

/// The notice for a session resumed by id whose folder is another one.
#[must_use]
pub fn other_folder_notice(cwd: &str) -> String {
    format!(
        "This session works in {}, not in the current folder.",
        plain(cwd)
    )
}

/// [`other_folder_notice`] only when the session's folder is known and outside the workspace.
#[must_use]
pub fn folder_notice(session_cwd: Option<&str>, workspace: &Path) -> Option<String> {
    match (session_cwd, belongs(session_cwd, workspace)) {
        (Some(cwd), Belongs::No) => Some(other_folder_notice(cwd)),
        _ => None,
    }
}

/// The notices of `~/.talaria/config.toml` under `home`. The key map's own notices are added
/// by the screen, which owns the key map.
#[must_use]
pub fn startup_notices(home: &Path) -> Vec<String> {
    crate::config::load_user_config(home).notices
}

/// Run the composer's timer when it is due.
pub fn drive_composer(deadline: &mut impl ComposerDeadline, now: Instant) {
    if deadline.next_tick().is_some_and(|due| due <= now) {
        deadline.tick(now);
    }
}

/// The status command's last step at quit: offer the final document, then stop the command and
/// wait for it. A redaction check that would let `api_key` through is never used to run it.
pub async fn drive_status(host: &mut impl StatusHost, is_credential_name: fn(&str) -> bool) {
    if is_credential_name("api_key") {
        host.on_tick(Instant::now());
    }
    host.shutdown().await;
}

/// A session event for the reducer, with all of `params` as `raw`.
pub fn apply_inbound(
    state: &mut SessionState,
    session_id: &str,
    frame: EventFrame,
    raw: Value,
) -> Vec<Effect> {
    talaria_core::session::update(
        state,
        Event::Session {
            session_id: SessionId::new(session_id),
            frame,
            raw,
        },
    )
}

/// A new connection, for the reducer.
pub fn note_connection(state: &mut SessionState, generation: u64) -> Vec<Effect> {
    talaria_core::session::update(state, Event::ConnectionOpened { generation })
}

/// Keep a prompt typed before the session exists; the eleventh is refused with a notice.
pub fn hold_prompt(state: &mut AppState, text: &str) -> Result<(), String> {
    if state.held.len() >= MAX_HELD_PROMPTS {
        return Err(HELD_FULL_NOTICE.to_string());
    }
    state.held.push_back(text.to_string());
    Ok(())
}

/// What a submitted line is, before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slash {
    /// `/quit` or `/exit`: detach and leave.
    Quit,
    /// A local command this client runs itself, with its action and its argument.
    Local(LocalAction, String),
    /// A local command refused locally, with the notice to show.
    Notice(String),
    /// A Hermes command, as typed. It goes to `slash.exec`.
    Command(String),
    /// `/btw`, with its argument: a side question, sent with `prompt.btw` and never to
    /// `slash.exec`.
    Btw(String),
    /// A bare `/`: nothing runs.
    Nothing,
    /// Ordinary text for the model.
    NotSlash,
}

/// Sort a submitted line: local commands first (they win over Hermes commands of the same
/// name and send nothing), then Hermes commands, then prose. The grammar is
/// [`talaria_core::commands::parse_slash_line`].
#[must_use]
pub fn slash_line(text: &str) -> Slash {
    let Some(line) = parse_slash_line(text) else {
        return Slash::NotSlash;
    };
    if line.name.is_empty() {
        return Slash::Nothing;
    }
    if let Some(command) = LocalCommandTable::builtin().find(&line.name) {
        if !command.takes_argument && !line.arg.is_empty() {
            return Slash::Notice(format!("/{} does not take an argument.", line.name));
        }
        return match command.action {
            LocalAction::Quit => Slash::Quit,
            LocalAction::Btw => Slash::Btw(line.arg.clone()),
            _ => Slash::Local(command.action, line.arg),
        };
    }
    Slash::Command(text.to_string())
}

/// The immediate answer this client writes for a server request of kind `name`: the
/// `-32601` refusal for a kind it does not support, or `Null` when a card holds the request
/// for the person.
pub async fn answer_server_request(name: &str, params: Value) -> Result<Value, String> {
    if !params.is_object() {
        return Err("a server request carries an object of parameters".to_string());
    }
    Ok(match policy_for(name) {
        ServerRequestPolicy::Masked(_) => Value::Null,
        ServerRequestPolicy::Refuse => json!({
            "code": -32601,
            "message": format!("talaria does not support server request: {name}"),
        }),
    })
}

/// The notice for a link event the person must see.
#[must_use]
pub fn link_notice(event: &LinkEvent) -> Option<String> {
    match event {
        LinkEvent::SessionGone { .. } => Some(SESSION_GONE_NOTICE.to_string()),
        LinkEvent::State(LinkState::Failed { reason, next_step }) => Some(format!(
            "{}\nNext step: {}",
            plain(reason),
            plain(next_step)
        )),
        _ => None,
    }
}

/// Leave the session: untrack, detach without ending it, close the link. Returns the detach
/// outcome, the exit code and the text for standard output (the resume hint, only when the
/// session has a user message). After `SessionGone` or a failed link nothing is detached and
/// the exit code is 1; the hint follows only a failed link.
pub async fn quit(state: &mut AppState) -> (DetachOutcome, u8, String) {
    state.quitting = true;
    let broken = state.session_gone || state.link_failed;
    let mut outcome = DetachOutcome::Skipped;
    if !broken
        && let (Some(opened), Some(service)) = (state.opened.as_ref(), state.service.as_ref())
    {
        if let Some(link) = &state.link {
            link.untrack(&opened.session_id);
        }
        outcome = service.detach(opened).await;
    }
    if let Some(link) = state.link.take() {
        link.close().await;
    }
    let code = u8::from(broken);
    let mut hint = String::new();
    if !state.session_gone
        && state.has_user_message()
        && let Some(opened) = &state.opened
    {
        if opened.state.turn() == &talaria_core::session::TurnState::Running {
            hint.push_str(STILL_WORKING);
            hint.push('\n');
        }
        hint.push_str(&crate::launch::exit_hint(
            &opened.stored_session_id,
            &state.profile,
        ));
    }
    (outcome, code, hint)
}

/// Quit while the create or resume is in flight: wait up to [`QUIT_WAIT`] and detach a session
/// that arrives late. Never a hint: no prompt was sent.
pub async fn quit_while_opening(state: &mut AppState) -> (DetachOutcome, u8, String) {
    state.quitting = true;
    let Some(mut pending) = state.pending.take() else {
        return (DetachOutcome::Skipped, 0, String::new());
    };
    match tokio::time::timeout(QUIT_WAIT, &mut pending).await {
        Ok(Ok(Ok(ready))) => {
            ready.connection.link.untrack(&ready.opened.session_id);
            let outcome = ready.service.detach(&ready.opened).await;
            ready.connection.link.close().await;
            (outcome, 0, String::new())
        }
        Ok(_) => (DetachOutcome::Skipped, 0, String::new()),
        Err(_) => {
            pending.abort();
            (DetachOutcome::Skipped, 0, String::new())
        }
    }
}

/// Deny open approvals first (the prompt service bounds that wait), then interrupt, even when
/// the deny failed.
async fn interrupt_sequence(
    service: SessionService,
    prompts: Option<Arc<Mutex<PromptService>>>,
    snapshot: SessionState,
    effect: Effect,
) -> (ExecuteOutcome, Vec<String>) {
    let mut notices = Vec::new();
    let open_approval = snapshot
        .open_prompts()
        .any(|prompt| prompt.kind == PromptKind::Approval && prompt.state == PromptState::Open);
    if let (true, Some(prompts), Some(client), Effect::Interrupt { session_id, .. }) =
        (open_approval, &prompts, service.client(), &effect)
    {
        let denied = prompts
            .lock()
            .await
            .deny_before_interrupt(client, &snapshot, session_id.as_str(), DENY_WAIT)
            .await;
        notices.extend(denied.into_iter().map(|notice| notice.text));
    }
    (service.execute(effect).await, notices)
}

/// Stop the running turn now, waiting for the answer. Without a session, a running turn or a
/// connection nothing is sent.
pub async fn interrupt(state: &mut AppState) -> ExecuteOutcome {
    let refused = |reason: &str| ExecuteOutcome::Refused {
        reason: reason.to_string(),
    };
    if state.interrupting {
        return refused("interrupt_in_flight");
    }
    let (Some(opened), Some(service)) = (state.opened.as_mut(), state.service.clone()) else {
        return refused("no_session");
    };
    let effect = talaria_core::session::update(&mut opened.state, Event::Interrupt)
        .into_iter()
        .find(|effect| matches!(effect, Effect::Interrupt { .. }));
    let Some(effect) = effect else {
        return refused("no_running_turn");
    };
    let snapshot = opened.state.clone();
    let (outcome, _) = interrupt_sequence(service, state.prompts.clone(), snapshot, effect).await;
    if outcome != ExecuteOutcome::Sent
        && let Some(opened) = state.opened.as_mut()
    {
        talaria_core::session::update(&mut opened.state, Event::InterruptFailed);
    }
    outcome
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use talaria_core::attachments::Attachment;

    #[derive(Default)]
    struct Editor {
        text: String,
        caret: usize,
    }
    impl ComposerDeadline for Editor {
        fn tick(&mut self, _: Instant) {}
        fn next_tick(&self) -> Option<Instant> {
            None
        }
    }
    impl StatusHost for Editor {
        fn on_tick(&mut self, _: Instant) {}
        fn next_wake(&self, _: Instant) -> Option<Instant> {
            None
        }
        fn shutdown(&mut self) -> impl Future<Output = ()> + Send {
            std::future::ready(())
        }
    }
    impl Screen for Editor {
        type Input = ();
        fn input(&mut self, _: (), _: &View<'_>, _: Instant) -> Vec<UserAction> {
            Vec::new()
        }
        fn draw(&mut self, _: &View<'_>) {}
        fn commit_lines(&mut self, _: Vec<String>) {}
        fn commit_context(&mut self, _block: &ContextBlock) {}
        fn commit_cells(&mut self, _: &[Cell]) {}
        fn load_draft(&mut self, text: &str) {
            self.text = text.into();
            self.caret = text.len();
        }
        fn restore(&mut self) {}
        fn edit_draft(&mut self, range: Range<usize>, text: &str) -> Option<(String, usize)> {
            self.caret = range.start + text.len();
            self.text.replace_range(range, text);
            self.draft_snapshot()
        }
        fn draft_snapshot(&self) -> Option<(String, usize)> {
            Some((self.text.clone(), self.caret))
        }
    }

    fn opened() -> AppState {
        let mut state = AppState::new("launch");
        state.opened = Some(OpenedSession {
            state: SessionState::empty(SessionId::new("session")),
            session_id: "session".into(),
            stored_session_id: "session".into(),
            subscription_id: "subscription".into(),
            title: None,
            revision: 0,
            execution_generation: 0,
            launch_notices: Vec::new(),
            effort: None,
            info: SessionInfoSummary {
                model: None,
                provider: None,
                cwd: None,
                branch: None,
            },
            cursor: Cursor {
                replay_epoch: "epoch".into(),
                last_sequence: 0,
            },
        });
        state
    }

    fn image(state: &mut AppState, path: &str) -> String {
        let (chip, label) = state
            .attachments
            .chips
            .begin_image("", &SessionId::new("session"))
            .unwrap();
        state
            .attachments
            .chips
            .image_ready(chip, path.into(), "image/png".into());
        label
    }

    #[test]
    fn a_receipt_releases_only_its_own_media_after_a_new_draft_begins() {
        let mut state = opened();
        let label = image(&mut state, "old-image");
        let effects = submit(&mut state, label.clone());
        let id = state.submitted_attachments.keys().next().unwrap().clone();
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Send(Effect::SendPrompt { attachments, .. })
            if attachments == &vec![Attachment { path:"old-image".into(), mime:"image/png".into() }])));
        let new_label = image(&mut state, "new-image");
        assert_eq!(new_label, "[Image #1]");
        assert_eq!(
            state
                .attachments
                .chips
                .expand(&new_label)
                .unwrap()
                .attachments[0]
                .path,
            "new-image"
        );
        submit_done(&mut state, id, ExecuteOutcome::Sent);
        assert_eq!(
            state
                .attachments
                .chips
                .expand(&new_label)
                .unwrap()
                .attachments[0]
                .path,
            "new-image"
        );
    }

    #[test]
    fn an_unknown_submit_restores_its_owned_image_and_display() {
        let mut state = opened();
        let label = image(&mut state, "original-image");
        submit(&mut state, label.clone());
        let id = state.submitted_attachments.keys().next().unwrap().clone();
        assert!(
            state
                .attachments
                .chips
                .expand(&label)
                .unwrap()
                .attachments
                .is_empty()
        );
        let effects = submit_done(&mut state, id, ExecuteOutcome::OutcomeUnknown);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::LoadDraft(text) if text == &label))
        );
        assert_eq!(
            state.attachments.chips.expand(&label).unwrap().attachments[0].path,
            "original-image"
        );
    }

    #[test]
    fn a_stale_file_result_cannot_insert_into_a_cleared_draft() {
        let mut state = opened();
        state.attachment_epoch = 2;
        let effects = update(
            &mut state,
            AppMessage::AttachmentResult {
                epoch: 1,
                event: AttachmentEvent::FileAttached {
                    session: SessionId::new("session"),
                    name: "notes.txt".into(),
                    ref_text: "@file:notes.txt".into(),
                    uploaded: true,
                    remainder: String::new(),
                },
            },
        );
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, AppEffect::Attachment(_)))
        );
        assert!(state.draft.is_empty());
    }

    #[test]
    fn an_earlier_failure_waits_for_the_new_draft_without_replacing_its_media() {
        for outcome in [
            ExecuteOutcome::OutcomeUnknown,
            ExecuteOutcome::Refused {
                reason: "busy".into(),
            },
        ] {
            let mut state = opened();
            let old = image(&mut state, "old-image");
            submit(&mut state, old.clone());
            let id = state.submitted_attachments.keys().next().unwrap().clone();
            let new = image(&mut state, "new-image");
            assert_eq!(old, new);
            state.draft = format!("new {new}");
            let effects = submit_done(&mut state, id, outcome);
            assert_eq!(state.draft, format!("new {new}"));
            assert!(
                effects
                    .iter()
                    .all(|effect| !matches!(effect, AppEffect::LoadDraft(_)))
            );
            assert_eq!(
                state.attachments.chips.expand(&new).unwrap().attachments[0].path,
                "new-image"
            );
            let draft = state.draft.clone();
            let effects = submit(&mut state, draft);
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, AppEffect::LoadDraft(text) if text == &old))
            );
            assert_eq!(state.draft, old);
            assert_eq!(
                state.attachments.chips.expand(&old).unwrap().attachments[0].path,
                "old-image"
            );
        }
    }

    #[test]
    fn accepting_a_folder_emits_its_next_request_without_waiting() {
        let mut state = opened();
        let now = Instant::now();
        state.attachments.on_edit("@sr", 3, true, now);
        let request = state
            .attachments
            .mention
            .tick(now + Duration::from_millis(60))
            .unwrap();
        state.attachments.mention.on_items(
            request.generation,
            vec![hermes_contract::schemas::CompletionItem {
                text: "@folder:src/".into(),
                display: "src".into(),
                meta: String::new(),
                kind: None,
            }],
        );
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::AttachmentKey {
                action: crate::attachments::Action::CompletionAccept,
                text: "@sr".into(),
                caret: 3,
                now,
            }),
        );
        assert!(effects.iter().any(|effect| matches!(effect, AppEffect::Attachment(AttachmentEffect::Complete { request, .. }) if request.word == "@src/")));
    }

    #[test]
    fn opening_the_model_picker_closes_mentions_without_replacing_the_draft() {
        let mut state = opened();
        state.draft = "explain @calc".into();
        state
            .attachments
            .on_edit(&state.draft, state.draft.len(), true, Instant::now());
        assert!(state.attachments.mention.is_open());
        user_action(&mut state, UserAction::OpenModels);
        assert!(!state.attachments.mention.is_open());
        assert_eq!(state.draft, "explain @calc");
    }

    #[test]
    fn a_new_file_drop_keeps_the_destination_until_its_upload_finishes() {
        let mut state = opened();
        submit(&mut state, "earlier prompt".into());
        let id = state.submitted_attachments.keys().next().unwrap().clone();
        user_action(
            &mut state,
            UserAction::Paste {
                text: "/srv/project/notes.txt".into(),
                draft: String::new(),
                caret: 0,
            },
        );
        submit_done(&mut state, id, ExecuteOutcome::OutcomeUnknown);
        let effects = attachment_event(
            &mut state,
            AttachmentEvent::DropFile {
                session: SessionId::new("session"),
                path: PathBuf::from("/srv/project/notes.txt"),
                remainder: String::new(),
            },
        );
        assert!(state.draft.is_empty());
        assert!(state.attachments.has_pending_work());
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, AppEffect::LoadDraft(_)))
        );
        let effects = attachment_event(
            &mut state,
            AttachmentEvent::FileAttached {
                session: SessionId::new("session"),
                name: "notes.txt".into(),
                ref_text: "@file:notes.txt".into(),
                uploaded: true,
                remainder: String::new(),
            },
        );
        assert!(!state.attachments.has_pending_work());
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Attachment(AttachmentEffect::EditDraft { .. })
        )));
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, AppEffect::LoadDraft(_)))
        );
    }

    #[test]
    fn a_failed_submission_cannot_restore_media_into_a_different_session() {
        let mut state = opened();
        let label = image(&mut state, "old-session-image");
        submit(&mut state, label.clone());
        let id = state.submitted_attachments.keys().next().unwrap().clone();
        state.opened.as_mut().unwrap().session_id = "another-session".into();
        assert!(submit_done(&mut state, id, ExecuteOutcome::OutcomeUnknown).is_empty());
        assert!(
            state
                .attachments
                .chips
                .expand(&label)
                .unwrap()
                .attachments
                .is_empty()
        );
    }

    #[test]
    fn an_admitted_paste_is_restored_by_history_but_not_by_typing_its_label() {
        let mut state = opened();
        let text = "one\ntwo\nthree\nfour\nfive";
        let label = state.attachments.chips.collapse_paste(text);
        let effects = submit(&mut state, label.clone());
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Send(Effect::SendPrompt { text: sent, .. }) if sent == text)));
        let id = state.submitted_attachments.keys().next().unwrap().clone();
        submit_done(&mut state, id, ExecuteOutcome::Sent);
        let now = Instant::now();
        state.attachments.on_edit(&label, label.len(), true, now);
        assert_eq!(state.attachments.chips.expand(&label).unwrap().text, label);
        state.attachments.on_recall(&label, label.len(), now);
        assert_eq!(state.attachments.chips.expand(&label).unwrap().text, text);
    }

    #[tokio::test]
    async fn completion_keeps_the_caret_before_trailing_text_and_acknowledges_immediately() {
        let mut state = opened();
        let now = Instant::now();
        let text = "see @ca trailing";
        state.attachments.on_edit(text, 7, true, now);
        state.attachments.mention.on_items(
            1,
            vec![hermes_contract::schemas::CompletionItem {
                text: "@file:src/calc.py".into(),
                display: "calc.py".into(),
                meta: "src".into(),
                kind: None,
            }],
        );
        let mut editor = Editor {
            text: text.into(),
            caret: 7,
        };
        let (sender, mut messages) = mpsc::unbounded_channel();
        let executor = Executor::new(sender);
        apply(
            &mut state,
            AppMessage::User(UserAction::AttachmentKey {
                action: crate::attachments::Action::CompletionAccept,
                text: text.into(),
                caret: 7,
                now,
            }),
            &mut editor,
            &executor,
        );
        assert_eq!(editor.text, "see @file:src/calc.py trailing");
        assert_eq!(editor.caret, editor.text.find(" trailing").unwrap());
        assert_eq!(
            (state.draft.as_str(), state.caret),
            (editor.text.as_str(), editor.caret)
        );
        assert!(messages.try_recv().is_err());
    }

    #[tokio::test]
    async fn consecutive_file_results_use_the_last_acknowledged_caret() {
        let mut state = opened();
        state.draft = "x ".into();
        state.caret = 2;
        let mut editor = Editor {
            text: "x ".into(),
            caret: 2,
        };
        let (sender, mut messages) = mpsc::unbounded_channel();
        let executor = Executor::new(sender);
        for name in ["first.txt", "second.txt"] {
            apply(
                &mut state,
                AppMessage::Attachment(AttachmentEvent::FileAttached {
                    session: SessionId::new("session"),
                    name: name.into(),
                    ref_text: format!("@file:{name}"),
                    uploaded: false,
                    remainder: String::new(),
                }),
                &mut editor,
                &executor,
            );
        }
        assert_eq!(editor.text, "x @file:first.txt @file:second.txt ");
        assert_eq!(state.draft, editor.text);
        assert_eq!(state.caret, editor.text.len());
        assert!(messages.try_recv().is_err());
    }
}

/// Refuse a `--record` path that exists or whose folder does not, before any drawing.
pub fn check_record_path(path: &Path) -> Result<(), LaunchFailure> {
    if path.symlink_metadata().is_ok() {
        return Err(failure::record_exists());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if parent.is_dir() {
        Ok(())
    } else {
        Err(failure::record_unwritable())
    }
}

/// Create the recording file (never overwriting one).
pub fn create_recorder(path: &Path) -> Result<Recorder, LaunchFailure> {
    Recorder::create(path, RecorderOptions::default()).map_err(|error| match error {
        RecordError::AlreadyExists => failure::record_exists(),
        _ => failure::record_unwritable(),
    })
}

/// Connect for the launch: discovery, profile, link.
pub async fn connect(launch: &Launch, home: &Path) -> Result<OpenConnection, LaunchFailure> {
    let recorder = launch.record.as_deref().map(create_recorder).transpose()?;
    ConnectionService::open(launch, home, recorder).await
}

/// The `--resume` list: live rows, then stored rows when that call works, merged.
pub async fn list_sessions(
    connection: &OpenConnection,
    launch_id: &str,
) -> Result<Vec<ListRow>, LaunchFailure> {
    let service = SessionService::with_client(launch_id, connection.link.client());
    let live = service.list("live").await.map_err(LaunchFailure::from)?;
    let stored = service.list("stored").await.unwrap_or_default();
    Ok(merge_rows(live, stored))
}

fn open_failure(error: OpenError, launch: &Launch) -> LaunchFailure {
    match (error, &launch.mode) {
        (OpenError::NotFound | OpenError::NotInFolder { .. }, StartMode::Continue) => {
            failure::nothing_to_continue()
        }
        (OpenError::NotFound, StartMode::Resume(Some(id))) => {
            failure::resume_not_found(id.trim(), &launch.profile)
        }
        (OpenError::NotFound, _) => failure::other_refusal("not_found"),
        (OpenError::NotInFolder { .. }, _) => failure::nothing_to_continue(),
        (OpenError::FolderUnknown, _) => failure::folder_unknown(),
        (OpenError::Refused { reason }, _) => failure::refusal(&reason),
        (OpenError::Unknown, StartMode::New) => failure::create_lost_twice(),
        (OpenError::Unknown, _) => {
            failure::link_failure("the connection dropped before Hermes answered")
        }
        (OpenError::Link(what), _) => failure::link_failure(&what),
    }
}

async fn open_session(
    launch: &Launch,
    connection: &OpenConnection,
    git: Option<JoinHandle<Option<PathBuf>>>,
    launch_id: &str,
) -> Result<(SessionService, OpenedSession, Option<String>), LaunchFailure> {
    let request =
        plan(launch, new_request_id(), &connection.describe).map_err(LaunchFailure::from)?;
    let service = SessionService::with_client(launch_id, connection.link.client())
        .with_create_defaults(
            &connection.profile_name,
            &connection.describe.session_create.parameters,
        );
    let git_toplevel = match git {
        Some(handle) => handle.await.ok().flatten(),
        None => None,
    };
    let workspace = workspace_key(&launch.folder, git_toplevel.as_deref());
    let (request, by_id) = match request {
        SessionRequest::Latest { .. } => (
            SessionRequest::Latest {
                workspace: workspace.clone(),
            },
            false,
        ),
        request @ SessionRequest::ById { .. } => (request, true),
        request => (request, false),
    };
    let opened = service
        .open(request)
        .await
        .map_err(|error| open_failure(error, launch))?;
    let notice = if by_id {
        folder_notice(opened.info.cwd.as_deref(), &workspace)
    } else {
        None
    };
    Ok((service, opened, notice))
}

/// Background start-up: connect (unless the `--resume` list already did), plan, open. `git`
/// runs at the same time as the connection for continue and resume.
pub async fn connect_and_open(
    launch: Launch,
    home: PathBuf,
    launch_id: String,
    preopened: Option<OpenConnection>,
) -> Result<Ready, LaunchFailure> {
    let git = matches!(
        launch.mode,
        StartMode::Continue | StartMode::Resume(Some(_))
    )
    .then(|| {
        let folder = launch.folder.clone();
        tokio::task::spawn_blocking(move || find_git_toplevel(&folder))
    });
    let connection = match preopened {
        Some(connection) => connection,
        None => connect(&launch, &home).await?,
    };
    match open_session(&launch, &connection, git, &launch_id).await {
        Ok((service, opened, notice)) => Ok(Ready {
            connection,
            service,
            opened,
            notice,
        }),
        Err(failure) => {
            connection.link.close().await;
            Err(failure)
        }
    }
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
    /// The key table of the configuration file.
    pub keys: BTreeMap<String, Vec<String>>,
}

fn held_text(held: &VecDeque<String>) -> String {
    if held.is_empty() {
        return String::new();
    }
    let mut text = if held.len() == 1 {
        "1 typed prompt was never sent:\n".to_string()
    } else {
        format!("{} typed prompts were never sent:\n", held.len())
    };
    for prompt in held {
        text.push_str("  ");
        text.push_str(&cut(&plain(prompt), HELD_PRINT_CHARS));
        text.push('\n');
    }
    text
}

/// The executor side of the loop: everything it needs to start a call.
struct Executor {
    sender: mpsc::UnboundedSender<AppMessage>,
    /// Time zero of the app clock that side work is timed by.
    started: Instant,
    /// Prompts go through one worker, one call at a time, so they reach the gateway in the
    /// order they were submitted.
    prompts: mpsc::UnboundedSender<(SessionService, Effect)>,
    model_files: mpsc::UnboundedSender<ModelFileJob>,
}

enum ModelFileJob {
    Save(Arc<crate::models::ModelService>, SavedModels),
    Flush(tokio::sync::oneshot::Sender<bool>),
}

impl Executor {
    fn new(sender: mpsc::UnboundedSender<AppMessage>) -> Self {
        let (prompts, mut queue) = mpsc::unbounded_channel::<(SessionService, Effect)>();
        let results = sender.clone();
        tokio::spawn(async move {
            while let Some((service, effect)) = queue.recv().await {
                let Effect::SendPrompt { submission_id, .. } = &effect else {
                    continue;
                };
                let submission_id = submission_id.clone();
                let outcome = service.execute(effect).await;
                if results
                    .send(AppMessage::SubmitDone {
                        submission_id,
                        outcome,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let (model_files, mut files) = mpsc::unbounded_channel();
        let results = sender.clone();
        tokio::spawn(async move {
            let mut failed = false;
            while let Some(job) = files.recv().await {
                match job {
                    ModelFileJob::Save(service, saved) => {
                        let inputs = service.execute(ModelsEffect::SaveFile(saved)).await;
                        failed = inputs
                            .iter()
                            .any(|input| matches!(input, ModelsInput::FileFailed { .. }));
                        for input in inputs {
                            let _ = results.send(AppMessage::Models(input));
                        }
                    }
                    ModelFileJob::Flush(reply) => {
                        let _ = reply.send(failed);
                    }
                }
            }
        });
        Self {
            sender,
            prompts,
            model_files,
            started: Instant::now(),
        }
    }

    async fn flush_model_files(&self) -> bool {
        let (reply, waiting) = tokio::sync::oneshot::channel();
        if self.model_files.send(ModelFileJob::Flush(reply)).is_err() {
            return true;
        }
        !matches!(
            tokio::time::timeout(QUIT_WAIT, waiting).await,
            Ok(Ok(false))
        )
    }

    /// App-clock milliseconds now.
    fn now_ms(&self) -> u64 {
        clock_ms(self.started)
    }

    fn send_later(&self, task: impl Future<Output = AppMessage> + Send + 'static) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.send(task.await);
        });
    }

    fn prompt_task<F, Fut>(&self, prompts: &Arc<Mutex<PromptService>>, task: F)
    where
        F: FnOnce(Arc<Mutex<PromptService>>) -> Fut + Send + 'static,
        Fut: Future<Output = (Vec<Event>, Vec<String>)> + Send + 'static,
    {
        let prompts = Arc::clone(prompts);
        self.send_later(async move {
            let (events, notices) = task(Arc::clone(&prompts)).await;
            let masked = Some(prompts.lock().await.masked_requests());
            AppMessage::PromptDone {
                events,
                notices,
                masked,
            }
        });
    }
}

/// Read the context or the usage of the open session, and answer with a block.
fn context_task(
    state: &AppState,
    action: LocalAction,
    argument: String,
    typed: String,
) -> impl Future<Output = AppMessage> + Send + 'static {
    let client = state
        .service
        .as_ref()
        .and_then(|service| service.client())
        .cloned();
    let opened = state.opened.as_ref();
    let session_id = opened
        .map(|opened| opened.session_id.clone())
        .unwrap_or_default();
    let stored = opened.map(|opened| opened.state.usage().clone());
    let hidden = opened.is_some_and(|opened| state.context.context_hidden(&opened.session_id));
    async move {
        let Some(client) = client else {
            return AppMessage::ContextDone {
                block: ContextBlock::Error {
                    typed,
                    command: command_of(action).to_owned(),
                    method: method_of(action).to_owned(),
                    sid: session_id,
                    error: talaria_core::context::text::ReadError::NotConnected,
                },
            };
        };
        let service = ContextService::new(client, ContextTimeouts::default());
        let block = match action {
            LocalAction::Usage => {
                let (stats, plan) = tokio::join!(service.stats(&session_id), service.plan());
                let stored = stored.clone().unwrap_or_default();
                let report = match stats {
                    Ok(stats) => UsageReport::build(Ok(&stats), &stored, plan.as_ref(), hidden),
                    Err(error) => UsageReport::build(Err(&error), &stored, plan.as_ref(), hidden),
                };
                ContextBlock::Usage { typed, report }
            }
            _ => match service.breakdown(&session_id).await {
                Ok(result) => {
                    let breakdown = Breakdown::from_result(&result);
                    if breakdown.is_prebuild() {
                        ContextBlock::Prebuild {
                            typed,
                            last: last_figure(stored.as_ref(), hidden),
                        }
                    } else {
                        ContextBlock::Breakdown { typed, breakdown }
                    }
                }
                Err(error) => ContextBlock::Error {
                    typed,
                    command: command_of(action).to_owned(),
                    method: method_of(action).to_owned(),
                    sid: session_id,
                    error,
                },
            },
        };
        let _ = argument;
        AppMessage::ContextDone { block }
    }
}

/// The context figure of the stored reading, when it is known and not hidden.
fn last_figure(
    stored: Option<&talaria_core::session::Usage>,
    hidden: bool,
) -> Option<talaria_core::context::usage::ContextFigure> {
    if hidden {
        return None;
    }
    let stored = stored?;
    let used = stored.context_used?;
    let max = stored.context_max?;
    let percent = stored.context_percent?;
    if max <= 0 {
        return None;
    }
    Some(talaria_core::context::usage::ContextFigure {
        used: u64::try_from(used)
            .unwrap_or(0)
            .min(u64::try_from(max).unwrap_or(0)),
        max: u64::try_from(max).unwrap_or(0),
        percent: percent.round().clamp(0.0, 100.0) as u8,
        estimated: false,
    })
}

/// The command a local action stands for.
fn command_of(action: LocalAction) -> &'static str {
    match action {
        LocalAction::Context => "/context",
        LocalAction::Compact => "/compact",
        LocalAction::Usage => "/usage",
        _ => "",
    }
}

/// The gateway method a local action reads.
fn method_of(action: LocalAction) -> &'static str {
    match action {
        LocalAction::Context => "session.context_breakdown",
        LocalAction::Compact => "session.mutate",
        LocalAction::Usage => "session.stats",
        _ => "",
    }
}

/// Send a compaction request and answer with its outcome.
fn compact_task(
    state: &AppState,
    request: MutationRequest,
    typed: String,
) -> impl Future<Output = AppMessage> + Send + 'static {
    let client = state
        .service
        .as_ref()
        .and_then(|service| service.client())
        .cloned();
    let session_id = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
        .unwrap_or_default();
    let timeout = ContextTimeouts::default().compact;
    async move {
        let Some(client) = client else {
            return AppMessage::CompactDone {
                request,
                outcome: MutationOutcome::NotSent {
                    reason: "not connected".to_owned(),
                },
                typed,
            };
        };
        let mutation = crate::mutation::MutationClient::new(client);
        let _ = &session_id;
        let outcome = mutation
            .send_with(
                &request,
                hermes_client::rpc::CallOptions {
                    timeout: Some(timeout),
                },
            )
            .await;
        AppMessage::CompactDone {
            request,
            outcome,
            typed,
        }
    }
}

fn clock_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// When the side-tasks section next changes, while anything is pending.
fn side_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    let opened = state.opened.as_ref()?;
    let due = next_tick(opened.state.side(), executor.now_ms())?;
    Some(executor.started + Duration::from_millis(due))
}

fn notice_texts(notices: Vec<crate::prompts::PromptNotice>) -> Vec<String> {
    notices.into_iter().map(|notice| notice.text).collect()
}

/// Apply one message and run its effects. Returns whether the screen needs a draw.
fn apply<S: Screen>(
    state: &mut AppState,
    message: AppMessage,
    screen: &mut S,
    executor: &Executor,
) -> bool {
    state.attention_at = Instant::now();
    let mut redraw = false;
    state.now_ms = executor.now_ms();
    if matches!(&message, AppMessage::AttachmentResult { epoch, .. } if *epoch != state.attachment_epoch)
    {
        return false;
    }
    if matches!(
        &message,
        AppMessage::Attachment(_)
            | AppMessage::AttachmentResult { .. }
            | AppMessage::SubmitDone { .. }
    ) && let Some((text, caret)) = screen.flush_draft()
        && (text != state.draft || caret != state.caret)
    {
        redraw |= apply(
            state,
            AppMessage::User(UserAction::DraftEdited {
                text,
                caret,
                typed: true,
                recalled: false,
                now: Instant::now(),
            }),
            screen,
            executor,
        );
    }
    for effect in update(state, message) {
        match effect {
            AppEffect::Rewind { ticket, call } => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned();
                let session_id = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone());
                executor.send_later(async move {
                    let input = match (client, session_id) {
                        (Some(client), Some(session_id)) => {
                            crate::rewind::RewindService::new(client, session_id)
                                .execute(ticket, call)
                                .await
                        }
                        _ => RewindInput::Reply {
                            ticket,
                            reply: Reply::Failed(CallFailure::NotSent(
                                "no session is open".to_string(),
                            )),
                        },
                    };
                    AppMessage::Rewind(input)
                });
            }
            AppEffect::Models(effect) => match effect {
                ModelsEffect::OpenOverlay | ModelsEffect::CloseOverlay => redraw = true,
                ModelsEffect::Notice { level, text, .. } => {
                    if text.starts_with("~/.talaria/models.json could not be read (")
                        && !state.model_warnings.insert(text.clone())
                    {
                        continue;
                    }
                    state.model_notice_counter = state.model_notice_counter.saturating_add(1);
                    screen.commit_cells(&[Cell::Notice(talaria_core::session::NoticeCell {
                        id: talaria_core::session::CellId::new(
                            u64::MAX - state.model_notice_counter,
                        ),
                        kind: match level {
                            NoticeLevel::Info => talaria_core::session::NoticeKind::Diagnostic,
                            NoticeLevel::Warning => talaria_core::session::NoticeKind::Warning,
                            NoticeLevel::Error => talaria_core::session::NoticeKind::Error,
                        },
                        text,
                        detail: None,
                    })]);
                }
                ModelsEffect::SaveFile(saved) => {
                    if let Some(service) = state.model_service.clone() {
                        let _ = executor
                            .model_files
                            .send(ModelFileJob::Save(service, saved));
                    }
                }
                effect => {
                    if let Some(service) = state.model_service.clone() {
                        let sender = executor.sender.clone();
                        tokio::spawn(async move {
                            for input in service.execute(effect).await {
                                let _ = sender.send(AppMessage::Models(input));
                            }
                        });
                    }
                }
            },
            AppEffect::CommitLines(lines) => screen.commit_lines(lines),
            AppEffect::CommitContext(block) => screen.commit_context(&block),
            AppEffect::ContextRead {
                action,
                argument,
                typed,
            } => executor.send_later(context_task(state, action, argument, typed)),
            AppEffect::CompactRequest { request, typed } => {
                executor.send_later(compact_task(state, request, typed));
            }
            AppEffect::CommitChecklist(commit) => screen.commit_checklist(&commit),
            AppEffect::CommitCells(range) => {
                if let Some(opened) = &state.opened {
                    let cells = opened.state.cells();
                    let end = range.end.min(cells.len());
                    screen.commit_cells(&cells[range.start.min(end)..end]);
                }
            }
            AppEffect::Redraw => redraw = true,
            AppEffect::UnrecordSubmit(text) => screen.unrecord_submit(&text),
            AppEffect::LoadDraft(text) => {
                screen.load_draft(&text);
                if let Some((text, caret)) = screen.draft_snapshot() {
                    redraw |= apply(
                        state,
                        AppMessage::DraftSynced { text, caret },
                        screen,
                        executor,
                    );
                }
            }
            AppEffect::Attachment(effect) => match effect {
                AttachmentEffect::EditDraft { range, text } => {
                    if let Some((text, caret)) = screen.edit_draft(range, &text) {
                        let _ = apply(
                            state,
                            AppMessage::DraftSynced { text, caret },
                            screen,
                            executor,
                        );
                    }
                    redraw = true;
                }
                AttachmentEffect::Highlight(_) => redraw = true,
                AttachmentEffect::Notice(text) => screen.commit_lines(vec![format!("· {text}")]),
                effect => {
                    let sender = executor.sender.clone();
                    let epoch = state.attachment_epoch;
                    crate::attachments::AttachmentService::new(
                        state
                            .service
                            .as_ref()
                            .and_then(SessionService::client)
                            .cloned(),
                        Box::new(move |event| {
                            let _ = sender.send(AppMessage::AttachmentResult { epoch, event });
                        }),
                        crate::clipboard::ClipboardImageReader::system(),
                        state.attachment_home.clone(),
                    )
                    .execute(effect);
                }
            },
            AppEffect::Transcript(action) => {
                screen.transcript(action);
                redraw = true;
            }
            AppEffect::Send(effect) => {
                let Some(service) = state.service.clone() else {
                    continue;
                };
                let _ = executor.prompts.send((service, effect));
            }
            AppEffect::Side(effect) => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned();
                let started = executor.started;
                executor.send_later(async move {
                    let events = match client {
                        Some(client) => {
                            SideService::new(client)
                                .execute(effect, move || clock_ms(started))
                                .await
                        }
                        None => side::not_sent(&effect),
                    };
                    AppMessage::SideDone(events)
                });
            }
            AppEffect::Interrupt(effect) => {
                let (Some(service), Some(opened)) = (state.service.clone(), &state.opened) else {
                    state.interrupting = false;
                    continue;
                };
                let task = interrupt_sequence(
                    service,
                    state.prompts.clone(),
                    opened.state.clone(),
                    effect,
                );
                executor.send_later(async move {
                    let (outcome, notices) = task.await;
                    AppMessage::InterruptDone { outcome, notices }
                });
            }
            AppEffect::Respond { effect, note } => {
                let (Some(prompts), Some(client)) = (
                    &state.prompts,
                    state
                        .service
                        .as_ref()
                        .and_then(SessionService::client)
                        .cloned(),
                ) else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let mut prompts = prompts.lock().await;
                    if let Some((key, note)) = note {
                        prompts.set_note(&key, note);
                    }
                    let executed = prompts.execute(&client, effect).await;
                    (executed.events, notice_texts(executed.notices))
                });
            }
            AppEffect::AnswerMasked { request_id, value } => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let result = prompts.lock().await.answer_masked(&request_id, value).await;
                    let notices = match result {
                        Ok(()) => Vec::new(),
                        Err(_) => vec![
                            "The answer could not be sent; Hermes may have moved on.".to_string(),
                        ],
                    };
                    (Vec::new(), notices)
                });
            }
            AppEffect::ServerRequest(request) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_server_request(request).await;
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::CancelRequest { id, method, reason } => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts
                        .lock()
                        .await
                        .on_request_cancel(&id, &method, &reason);
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::ConnectionOpened(generation) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_connection_replaced(generation);
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::PromptsChanged(open) => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let notices = prompts.lock().await.on_prompts_changed(&open);
                    (Vec::new(), notice_texts(notices))
                });
            }
            AppEffect::Track { session_id, cursor } => {
                if let Some(link) = &state.link {
                    link.track(&session_id, cursor);
                }
            }
            AppEffect::Resync { session_id, cursor } => {
                if let Some(link) = &state.link {
                    link.resync(&session_id, cursor);
                }
            }
            AppEffect::Recover(gap) => {
                let Some(service) = state.service.clone() else {
                    continue;
                };
                executor
                    .send_later(async move { AppMessage::Recovered(service.recover(gap).await) });
            }
            AppEffect::Quit => state.quitting = true,
            AppEffect::RunCommand { session_id, typed } => {
                let Some(commands) = &state.command_service else {
                    continue;
                };
                let mut runner = commands.runner(state.commands.catalog.clone());
                executor.send_later(async move {
                    let outcome = runner.dispatch(&session_id, &typed).await;
                    AppMessage::CommandDone { typed, outcome }
                });
            }
            AppEffect::FetchCatalog {
                request_id,
                session_id,
                after,
            } => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.start_fetch(request_id, session_id, after);
                }
            }
            AppEffect::RequestCompletion {
                request_id,
                text,
                after,
                session_id,
            } => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.request_completion(request_id, text, after, session_id);
                }
            }
            AppEffect::CancelCompletion => {
                if let Some(commands) = state.command_service.as_mut() {
                    commands.cancel_completion();
                }
            }
            AppEffect::ReportPane(pane) => screen.report_pane(
                &pane,
                state
                    .opened
                    .as_ref()
                    .map(|o| (o.session_id.as_str(), state.profile.as_str())),
            ),
            AppEffect::Alert(alert) => screen.alert(&alert),
            AppEffect::Appearance => {
                screen.appearance_changed(&state.appearance.view());
                redraw = true;
            }
            AppEffect::ThemeReport => {
                if let Some(report) = screen.theme_report()
                    && let AppEffect::CommitLines(lines) =
                        notice_lines(&theme_lines(&report).join("\n"))
                {
                    screen.commit_lines(lines);
                }
            }
            AppEffect::Skin(name) => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned();
                let session = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone());
                let profile = state.profile.clone();
                let mut service = state.appearance.clone();
                executor.send_later(async move {
                    let before = service.skin_state().clone();
                    let lines = match &client {
                        Some(client) => {
                            service
                                .skin_command(client, name.as_deref(), session.as_deref(), &profile)
                                .await
                        }
                        None => appearance::offline_skin_lines(name.as_deref()),
                    };
                    let skin =
                        (service.skin_state() != &before).then(|| service.skin_state().clone());
                    AppMessage::AppearanceDone {
                        skin,
                        lines,
                        generation: None,
                    }
                });
            }
            AppEffect::Describe(generation) => {
                let Some(client) = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned()
                else {
                    continue;
                };
                let mut service = state.appearance.clone();
                executor.send_later(async move {
                    let before = service.skin_state().clone();
                    service.connection_opened(&client, generation).await;
                    let skin =
                        (service.skin_state() != &before).then(|| service.skin_state().clone());
                    AppMessage::AppearanceDone {
                        skin,
                        lines: Vec::new(),
                        generation: Some(generation),
                    }
                });
            }
            AppEffect::SetSessionOff {
                session_id,
                generation,
                off,
            } => run_permissions(
                state,
                executor,
                PermissionsEffect::SetSessionOff {
                    session_id,
                    generation,
                    off,
                },
            ),
            AppEffect::Permissions(effect) => run_permissions(state, executor, effect),
            AppEffect::Cd(effect) => run_cd(state, executor, effect),
        }
    }
    sync_attachment_view(state, screen, Instant::now());
    redraw
}

fn sync_attachment_view<S: Screen>(state: &AppState, screen: &mut S, now: Instant) {
    let mention = state
        .attachments
        .mention
        .is_open()
        .then(|| state.attachments.mention.view(now));
    let highlights = state
        .attachments
        .chips
        .live(&state.draft)
        .into_iter()
        .map(|(chip, range)| {
            let style = if matches!(
                chip.kind,
                talaria_core::attachments::ChipKind::Image {
                    state: talaria_core::attachments::ImageState::Pending { .. },
                    ..
                }
            ) {
                crate::attachments::LabelStyle::Dimmed
            } else {
                crate::attachments::LabelStyle::Accent
            };
            (range, style)
        })
        .collect();
    screen.sync_attachments(AttachmentView {
        mention,
        highlights,
    });
}

/// Take the opened session's handles into the state and start forwarding link events.
fn install(state: &mut AppState, ready: Ready, executor: &Executor) -> AppMessage {
    let Ready {
        connection,
        service,
        opened,
        notice,
    } = ready;
    let OpenConnection {
        link,
        mut inbound,
        profile_name,
        describe,
        ..
    } = connection;
    // The launch skin is applied before any history is committed, so committed lines are
    // drawn with the profile's skin.
    let skin = describe
        .skin
        .as_ref()
        .and_then(|skin| serde_json::to_value(skin).ok());
    state.appearance.update(AppearanceInput::Handshake {
        ready: None,
        describe: skin,
    });
    // Made once, when the first connection opens, so notices can name the profile (#237).
    state
        .permissions
        .get_or_insert_with(|| PermissionsState::new(profile_name.clone()));
    state.profile = profile_name;
    let mut commands = CommandService::new(link.client());
    if let Some(mut replies) = commands.take_replies() {
        let sender = executor.sender.clone();
        tokio::spawn(async move {
            while let Some(reply) = replies.recv().await {
                if sender.send(AppMessage::Commands(reply)).is_err() {
                    break;
                }
            }
        });
    }
    state.command_service = Some(commands);
    state.link = Some(link);
    state.permissions_service = service
        .client()
        .map(|client| Arc::new(PermissionsService::new(Arc::new(client.clone()))));
    state.workspace_service = service
        .client()
        .map(|client| Arc::new(WorkspaceService::new(MutationClient::new(client.clone()))));
    state.service = Some(service);
    state.model_service = Some(Arc::new(crate::models::ModelService::new(
        state
            .service
            .as_ref()
            .and_then(SessionService::client)
            .expect("an installed connection has a client")
            .clone(),
        crate::config::models_path().unwrap_or_default(),
    )));
    let counter = Arc::clone(&state.counter);
    let launch_id = state.launch_id.clone();
    state.prompts = Some(Arc::new(Mutex::new(PromptService::new(Box::new(
        move || submission_id(&launch_id, counter.fetch_add(1, Ordering::SeqCst) + 1),
    )))));
    let sender = executor.sender.clone();
    tokio::spawn(async move {
        while let Some(event) = inbound.recv().await {
            if sender.send(AppMessage::Link(event)).is_err() {
                break;
            }
        }
    });
    AppMessage::Opened { opened, notice }
}

/// Run one permissions call on a task; its reply comes back as a message. With no connection
/// the call is not sent, and the reducer hears that at once.
fn run_permissions(state: &AppState, executor: &Executor, effect: PermissionsEffect) {
    let Some(service) = state.permissions_service.clone() else {
        if let Some(input) = not_sent(effect) {
            let _ = executor.sender.send(AppMessage::Permissions(input));
        }
        return;
    };
    let sender = executor.sender.clone();
    tokio::spawn(async move {
        if let Some(input) = service.execute(effect).await {
            let _ = sender.send(AppMessage::Permissions(input));
        }
    });
}

/// The reply to a permissions call that could not be sent.
fn not_sent(effect: PermissionsEffect) -> Option<PermissionsInput> {
    Some(match effect {
        PermissionsEffect::ReadProfileMode { generation } => PermissionsInput::ProfileModeRead {
            generation,
            result: Err(PermissionsFailure::NotSent),
        },
        PermissionsEffect::ReadSessionOff {
            session_id,
            generation,
        } => PermissionsInput::SessionOffRead {
            session_id,
            generation,
            result: Err(PermissionsFailure::NotSent),
        },
        PermissionsEffect::SetSessionOff {
            session_id,
            generation,
            off,
        } => PermissionsInput::SessionOffSet {
            session_id,
            generation,
            sent: off,
            result: Err(PermissionsFailure::NotSent),
        },
        PermissionsEffect::SetProfileMode { generation, mode } => {
            PermissionsInput::ProfileModeSet {
                generation,
                sent: mode,
                result: Err(PermissionsFailure::NotSent),
            }
        }
        PermissionsEffect::Notice { .. } | PermissionsEffect::Redraw => return None,
    })
}

/// Run one `/cd` check or move on a task; its answer comes back as a message. With no
/// connection the move is not sent and the folder counts as unreadable.
fn run_cd(state: &AppState, executor: &Executor, effect: CdEffect) {
    let sender = executor.sender.clone();
    let Some(service) = state.workspace_service.clone() else {
        let input = match effect {
            CdEffect::CheckFolder { session_id, target } => Some(CdInput::Checked {
                session_id,
                target,
                check: FolderCheck::Unreadable,
            }),
            CdEffect::Send(request) => Some(CdInput::Outcome {
                request,
                outcome: MutationOutcome::NotSent {
                    reason: "Talaria is not connected to Hermes".to_string(),
                },
            }),
            CdEffect::UpdateFolder { .. } | CdEffect::Notice { .. } => None,
        };
        if let Some(input) = input {
            let _ = sender.send(AppMessage::Cd(input));
        }
        return;
    };
    tokio::spawn(async move {
        if let Some(input) = service.execute(effect).await {
            let _ = sender.send(AppMessage::Cd(input));
        }
    });
}

fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// What ended the loop.
enum Ending {
    Quit,
    Signal(u8),
    Failed(LaunchFailure),
    InputClosed,
}

/// Run one launch: start-up in the background, the loop, and the quit. The first frame is
/// drawn before anything touches the network. The screen is restored before this returns;
/// the caller prints the outcome.
pub async fn run<S: Screen>(
    launch: Launch,
    mut setup: RunSetup,
    screen: &mut S,
    mut inputs: mpsc::UnboundedReceiver<S::Input>,
) -> Outcome {
    let mut state = AppState::new(setup.launch_id.clone());
    state.models = ModelsState::new(&setup.launch_id, crate::config::load_models());
    state
        .models
        .set_display_zone(crate::config::model_display_zone());
    if let Some(warning) = state.models.take_saved_warning() {
        state.model_warnings.insert(warning.clone());
        setup.notices.push(warning);
    }
    state.attention = talaria_core::attention::AttentionTracker::new(screen.attention_config());
    if let Some((colors, polarity)) = screen.startup_appearance() {
        state.appearance = AppearanceService::new(colors, polarity);
    }
    state.attachments = AttachmentsState::new(screen.attachment_rule(), 16 * 1024 * 1024);
    state.attachment_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    state.attachments.set_home(state.attachment_home.clone());
    state.attachment_folder = launch.folder.clone();
    state.key_bindings = setup.keys.clone();
    state.toggle_label = screen.toggle_label();
    state.interrupt_label = screen.interrupt_label();
    state.home = std::env::var_os("HOME").map(PathBuf::from);
    state.profile = launch.profile.trim().to_lowercase();
    state.list_title = setup.list_title;
    state.model = launch.model.clone();
    state.effort = launch.effort.clone();
    sync_models(&mut state, screen);
    screen.draw(&state.view());
    screen.report_pane(&state.attention.pane(), None);
    if !setup.notices.is_empty() {
        screen.commit_lines(
            setup
                .notices
                .iter()
                .map(|notice| format!("\u{b7} {notice}"))
                .collect(),
        );
        screen.draw(&state.view());
    }

    let (sender, mut messages) = mpsc::unbounded_channel();
    let executor = Executor::new(sender);
    let mut pending = Some(tokio::spawn(connect_and_open(
        launch,
        setup.home,
        setup.launch_id,
        setup.preopened,
    )));
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    let mut hangup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).ok();

    let ending = loop {
        let now = Instant::now();
        state.now_ms = executor.now_ms();
        let side_due = side_wake(&state, &executor);
        let deadline = earliest(
            earliest(screen.next_tick(), screen.next_wake(now)),
            side_due,
        );
        // A compacting line counts its seconds, so the screen wakes once a second
        // while one is shown.
        let deadline = if state
            .opened
            .as_ref()
            .is_some_and(|opened| state.context.compacting_line(&opened.session_id).is_some())
        {
            let wait = 1_000 - state.now_ms % 1_000;
            earliest(deadline, Some(now + Duration::from_millis(wait)))
        } else {
            deadline
        };
        let deadline = earliest(deadline, state.attachments.next_deadline());
        let mut redraw = false;
        let mut woke_on_message = false;
        tokio::select! {
            input = inputs.recv() => {
                let Some(input) = input else { break Ending::InputClosed };
                sync_models(&mut state, screen);
                let actions = screen.input(input, &state.view(), Instant::now());
                for action in actions {
                    redraw |= apply(&mut state, AppMessage::ForegroundKey(action), screen, &executor);
                }
            }
            Some(message) = messages.recv() => {
                redraw |= apply(&mut state, message, screen, &executor);
                woke_on_message = true;
            }
            joined = async { pending.as_mut().expect("guarded by the branch condition").await },
                if pending.is_some() => {
                pending = None;
                match joined {
                    Ok(Ok(ready)) => {
                        let message = install(&mut state, ready, &executor);
                        screen.appearance_changed(&state.appearance.view());
                        redraw |= apply(&mut state, message, screen, &executor);
                    }
                    Ok(Err(failure)) => break Ending::Failed(failure),
                    Err(_) => break Ending::Failed(failure::link_failure("the start-up task stopped")),
                }
            }
            () = async {
                match deadline {
                    Some(due) => tokio::time::sleep_until(due.into()).await,
                    None => std::future::pending().await,
                }
            } => {
                let now = Instant::now();
                drive_composer(screen, now);
                screen.on_tick(now);
                state.now_ms = executor.now_ms();
                if side_due.is_some_and(|due| due <= now) {
                    let now_ms = executor.now_ms();
                    apply(&mut state, AppMessage::Tick { now_ms }, screen, &executor);
                }
                if let Some((text, caret)) = screen.draft_snapshot()
                    && (text != state.draft || caret != state.caret) {
                        let _ = apply(&mut state, AppMessage::User(UserAction::DraftEdited {
                            text, caret, typed: true, recalled: false, now,
                        }), screen, &executor);
                }
                let _ = apply(&mut state, AppMessage::AttachmentTick(now), screen, &executor);
                redraw = true;
            }
            Some(()) = async { match terminate.as_mut() { Some(stream) => stream.recv().await, None => std::future::pending().await } } => {
                break Ending::Signal(143);
            }
            Some(()) = async { match hangup.as_mut() { Some(stream) => stream.recv().await, None => std::future::pending().await } } => {
                break Ending::Signal(129);
            }
        }
        // A fast stream queues deltas faster than a frame is drawn. Keys that are already
        // waiting go first, so Esc is never held behind the stream, and ready messages are
        // applied together and drawn once instead of one frame each.
        let mut drained = 0;
        loop {
            while !state.quitting {
                let Ok(input) = inputs.try_recv() else { break };
                sync_models(&mut state, screen);
                let actions = screen.input(input, &state.view(), Instant::now());
                for action in actions {
                    redraw |= apply(
                        &mut state,
                        AppMessage::ForegroundKey(action),
                        screen,
                        &executor,
                    );
                }
            }
            if !woke_on_message || state.quitting || drained == MESSAGE_BATCH {
                break;
            }
            let Ok(message) = messages.try_recv() else {
                break;
            };
            redraw |= apply(&mut state, message, screen, &executor);
            drained += 1;
        }
        if state.quitting {
            break Ending::Quit;
        }
        if redraw {
            sync_models(&mut state, screen);
            screen.draw(&state.view());
            screen.on_tick(Instant::now());
        }
    };

    let mut outcome = Outcome::default();
    match ending {
        Ending::Quit | Ending::InputClosed => {
            let (detached, code) = if pending.is_some() {
                state.pending = pending.take();
                let (detached, code, _) = quit_while_opening(&mut state).await;
                (detached, code)
            } else {
                let (detached, code, hint) = quit(&mut state).await;
                outcome.stdout = hint;
                (detached, code)
            };
            outcome.code = code;
            if detached == DetachOutcome::StaleSubscription {
                outcome.stderr.push_str(STALE_SUBSCRIPTION);
                outcome.stderr.push('\n');
            }
        }
        Ending::Signal(code) => {
            if let Some(handle) = pending.take() {
                handle.abort();
            }
            if let Some(link) = state.link.take() {
                link.close().await;
            }
            outcome.code = code;
        }
        Ending::Failed(failure) => {
            outcome.code = failure.exit_code;
            outcome.stderr.push_str(&failure.render());
        }
    }
    if executor.flush_model_files().await {
        outcome
            .stderr
            .push_str("Some model choices could not be saved before exit.\n");
    }
    drive_status(screen, hermes_client::is_suspicious_key).await;
    screen.restore();
    outcome.stderr.push_str(&held_text(&state.held));
    outcome
}

#[cfg(test)]
mod comfort_event_tests {
    use super::*;

    fn state() -> AppState {
        let snapshot =
            serde_json::from_str(include_str!("../tests/data/open_approval.json")).unwrap();
        let mut state = AppState::new("comfort-test");
        state.opened = Some(OpenedSession::from_snapshot(snapshot));
        state
    }

    fn inbound(state: &AppState, kind: &str, payload: Value) -> AppMessage {
        let opened = state.opened.as_ref().unwrap();
        let raw = json!({"session_id":opened.session_id,"replay_epoch":opened.cursor.replay_epoch,"seq":opened.cursor.last_sequence+1,"type":kind,"payload":payload});
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(hermes_client::channel::InboundEvent {
                generation: hermes_client::channel::Generation(1),
                frame: EventFrame::from_params(&raw).unwrap(),
                raw,
            }),
            replayed: false,
        })
    }

    #[test]
    fn accepted_cancellation_is_forwarded_to_the_prompt_service() {
        let mut state = state();
        let payload = hermes_contract::schemas::RequestCancelPayload {
            id: "masked-1".into(),
            method: "sudo".into(),
            reason: "timeout".into(),
        };
        let message = inbound(
            &state,
            "request.cancel",
            serde_json::to_value(payload).unwrap(),
        );
        let effects = update(&mut state, message);
        assert!(effects.iter().any(|effect|matches!(effect,AppEffect::CancelRequest {id,method,reason} if id=="masked-1" && method=="sudo" && reason=="timeout")));
    }

    #[test]
    fn accepted_title_updates_attention_label() {
        let mut state = state();
        let id = state.opened.as_ref().unwrap().session_id.clone();
        let payload = hermes_contract::schemas::SessionTitlePayload {
            session_id: id,
            title: "Current session title".into(),
        };
        let message = inbound(
            &state,
            "session.title",
            serde_json::to_value(payload).unwrap(),
        );
        update(&mut state, message);
        assert_eq!(
            state.opened.as_ref().unwrap().title.as_deref(),
            Some("Current session title")
        );
        let now = Instant::now();
        state
            .attention
            .observe(talaria_core::attention::Activity::TurnStarted { at: now });
        let output = state
            .attention
            .observe(talaria_core::attention::Activity::TurnEnded {
                outcome: talaria_core::attention::TurnOutcome::Complete,
                at: now + Duration::from_secs(25),
            });
        assert_eq!(output.alert.unwrap().text, "done: Current session title");
    }
}

#[cfg(test)]
mod checklist_command_tests {
    use super::*;
    use talaria_core::session::TodoState;

    #[test]
    fn tasks_command_uses_the_local_table_and_toggles_without_a_gateway_call() {
        let command = LocalCommandTable::builtin()
            .find("tasks")
            .copied()
            .expect("local tasks");
        assert_eq!(command.action, LocalAction::Tasks);
        assert_eq!(
            slash_line("/tasks"),
            Slash::Local(LocalAction::Tasks, String::new())
        );
        assert!(matches!(slash_line("/tasks extra"), Slash::Notice(_)));
        let mut state = AppState::new("0123456789abcdef");
        state.checklist.attach(
            Some(&TodoState {
                revision: 1,
                todos: vec![json!({"id":"1", "content":"A step", "status":"pending"})],
            }),
            true,
        );
        let expanded = state.checklist.expanded();
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Submit("/tasks".into())),
        );
        assert_eq!(state.checklist.expanded(), !expanded);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], AppEffect::Redraw));
    }
}
