//! Application loop: one channel, a pure update, and the effect executor.
//!
//! The loop owns the session state, the link and the services. It reaches the terminal only
//! through [`Screen`], which `talaria-cli` implements with `talaria-tui`, so this crate never
//! depends on a terminal library. Every gateway call runs in a spawned task and comes back as an
//! [`AppMessage`]; the loop itself never waits on the network.

pub mod choice;

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hermes_client::liveness::{Link, LinkEvent, LinkState};
use hermes_client::record::{RecordError, Recorder, RecorderOptions};
use hermes_client::replay::{Cursor, ReplayGap};
use hermes_client::{Inbound, ServerRequest};
use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::SeatState;
use talaria_core::attachments::{Chip, CollapseRule, Expanded, reference_notice};
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationState, CommandResult, canonical_name,
};
use talaria_core::background::{self, BackgroundEffect, BackgroundEvent, ProcessesIntent};
use talaria_core::capabilities::{
    CapEffect, CapError, CapInput, CapTab, CapView, CapabilitiesState, SessionRef as CapSessionRef,
};
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
use talaria_core::cron::{
    self, CronBody, CronEffect, CronInput, CronKey, CronOpen, CronState, CronView,
};
use talaria_core::directory::export::ExportFormat;
use talaria_core::directory::rows::RowKey;
use talaria_core::directory::state::{PickerEffect, PickerInput, PickerState};
use talaria_core::doctor::{
    DoctorEffect, DoctorFacts, DoctorKey, DoctorView, RestartEffect, RestartInput, RestartOutcome,
    RestartState, ask_input, diagnose, doctor_view_update, restart_update,
};
use talaria_core::fleet_attention::{FleetAttentionConfig, FleetScope};
use talaria_core::fleet_view::FleetViewState;
use talaria_core::models::{
    ConfirmKey, ModelCommandOrEffort, ModelsEffect, ModelsInput, ModelsState, NoticeLevel,
    PickerKey, PickerView, SavedModels, parse_effort_command, parse_model_command,
};
use talaria_core::mutation::{Confirmation, MutationLedger, MutationOutcome, MutationRequest};
use talaria_core::permissions::{
    ApprovalSegmentText, CallFailure as PermissionsFailure, PermissionsEffect, PermissionsInput,
    PermissionsState, hermes_command_finished, parse_permissions_command,
};
use talaria_core::profiles::{ProfilesEffect, ProfilesInput, ProfilesState, ProfilesView, RawEdit};
use talaria_core::providers::{ProvidersEffect, ProvidersInput, ProvidersState, ProvidersView};
use talaria_core::queue::{
    IdSource, QueueAction, QueueIntent, RECONCILE_WAIT, SubmissionId, update_queue,
};
use talaria_core::rewind::{
    CallFailure, DiffCommand, DiffSource, DoubleEsc, InterruptConfirm, InterruptPress, Reply,
    RewindEffect, RewindInput, RewindKey, RewindState, RewindView, RollbackCommand,
    parse_diff as parse_diff_command, parse_rollback, parse_undo, text as rewind_text,
    update as rewind_update,
};
use talaria_core::session::{
    Cell, ConnectionChoice, Effect, Event, NoticeKind, PromptKey, PromptKind, PromptState,
    SessionId, SessionState, TurnState,
};
use talaria_core::side::{
    NO_SESSION_NOTICE, SHELL_EMPTY_HINT, SIDE_USAGE, Submission, btw_command, carry_over, classify,
    next_tick,
};
use talaria_core::workspace::{
    CdContext, CdEffect, CdInput, CdState, FolderCheck, SessionFolder, parse_cd_command,
};
use tokio::sync::{Mutex, MutexGuard, mpsc};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::attachments::{AttachmentEffect, AttachmentEvent, AttachmentsState};
use crate::automation::{AutomationService, AutomationTicker, automation_actions, wall_clock_ms};
use crate::commands::{
    AppAction, CommandReply, CommandService, no_session_notice, outcome_actions,
};
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, AppearanceState, Polarity, PolaritySource, TerminalColors,
    ThemeReport,
};

use crate::appearance::{self, AppearanceService, AppearanceView, ThemeFileWork, theme_lines};

/// Presentation-only attachment data for the terminal adapter.
#[derive(Clone, Default)]
pub struct AttachmentView {
    pub mention: Option<talaria_core::attachments::MentionView>,
    pub highlights: Vec<(Range<usize>, crate::attachments::LabelStyle)>,
}
use crate::connection::{ConnectionService, OpenConnection, find_git_toplevel};
use crate::context::{ContextBlock, ContextService, ContextTimeouts};
use crate::directory::{DirectoryService, ExportResult, ExportTarget};
use crate::failure::{self, LaunchFailure};
use crate::fleet_attention::{
    AttentionHub, HubOutput, NextNeed, StatusCounts, next_need, status_counts,
};
use crate::launch::{
    Belongs, Launch, ListRow, MAX_HELD_PROMPTS, RESUME_HISTORY_CELLS, SessionRequest, StartMode,
    belongs, merge_rows, new_request_id, plain, plan, workspace_key,
};
use crate::mutation::MutationClient;
use crate::panes::{CommandRunner, PaneEnv, PaneOutcome, PaneSettings};
use crate::peek::FleetSink;
use crate::permissions::PermissionsService;
use crate::prompts::{
    LiveMaskedTarget, MaskedRequestInfo, MaskedTarget, PromptService, ServerRequestPolicy,
    policy_for,
};
use crate::queue::{QueueCommand, QueueService, queue_command, steer_command};
pub use crate::rebuild::{RebuildMode, rebuild_foreground};
use crate::rebuild::{final_prefix, recover_retained, recovered};
use crate::sessions::{
    DetachOutcome, ExecuteOutcome, OpenError, OpenedSession, SessionInfoSummary, SessionService,
    accept_event, bind_approval_cancelled_session, submission_id,
};
use crate::side::{self, SideService};
use crate::side_connection::SideScope;
use crate::switch::{BranchOutcome, SwitchOutcome};
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
/// What `/thinking full` says. Scrollback cannot be redrawn, so only blocks drawn from here on
/// change.
const THINKING_FULL_NOTICE: &str = "Thinking now shows in full in this view. Set [ui] reasoning = \"full\" in ~/.talaria/config.toml to keep it.";
/// What `/thinking collapsed` says.
const THINKING_COLLAPSED_NOTICE: &str =
    "Thinking now shows as one line per block in this view; the transcript view shows it in full.";
pub(crate) const REFRESH_FAILED_NOTICE: &str =
    "The transcript could not be refreshed after the reconnect.";
/// The line a prompt gets while a switch or a branch runs.
pub const SWITCHING_NOTICE: &str = "Switching sessions: your prompt was not sent. It is back in the composer; submit it again when the new session is ready.";
/// The line a second session action gets while one runs.
pub const SESSION_ACTION_NOTICE: &str =
    "Another session action is still running. Try again when it finishes.";
/// The line when the foreground session already is the target.
pub const ALREADY_HERE_NOTICE: &str = "You are already in this session.";
/// `/fleet` with `[fleet] enabled = false`.
pub const FLEET_DISABLED: &str =
    "The fleet view is off: [fleet] enabled = false in Talaria's configuration.";
/// `/fleet` with an argument.
pub const FLEET_ARGUMENT: &str = "/fleet takes no arguments.";
/// How many rows Page Up and Page Down move in the fleet overlay.
const FLEET_PAGE: usize = 10;
/// How many rows the picker shows at once.
const PICKER_PAGE: usize = 20;
/// The line a rename that could not be sent gets.
const RENAME_FAILED: &str = "Hermes could not rename the session.";
/// The line a rename the session changed under gets.
const RENAME_FENCE: &str = "Hermes refused the rename: the session changed while the rename was being prepared. Try again.";
/// The line a rename to a title another session holds gets.
const RENAME_TAKEN: &str = "Hermes could not rename the session. Another session may already use that title; titles must be unique.";
/// The line a branch while a turn runs gets; nothing is sent.
const BRANCH_TURN_RUNNING: &str =
    "A turn is running. Wait for it to finish or press Esc twice to stop it, then branch.";
/// The line an export to a file Talaria cannot write in any format gets.
const EXPORT_BAD_EXTENSION: &str = "Use a file name ending in .md, .json or .html.";
/// The line an export with arguments Talaria does not know gets.
const EXPORT_USAGE: &str =
    "Use /export [--no-redact] [file]; the file name ends in .md, .json or .html.";

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
    /// In a workflow agent's session opened from the run view: go back to the session the
    /// person came from and open the run view again.
    WorkflowAgentBack,
    /// Toggle or reopen the client work panel.
    LayoutPanel,
    /// Toggle the menu preference.
    ToggleMenuBar,
    /// Focus the menu without changing its preference.
    FocusMenu,
    /// Synchronize the menu focus and dropdown after a key.
    MenuFocus {
        focused: bool,
        dropdown: bool,
        transient: bool,
    },
    /// Run an existing local command without submitting the composer's draft.
    MenuCommand(String),
    /// Dismiss only the narrow work-panel overlay.
    DismissWorkPanel,
    Kanban(talaria_core::kanban::BoardKey),
    /// Redraw an external permission selection.
    AcpSelect,
    /// Answer a permission owned by an external agent.
    AcpAnswer {
        key: RowKey,
        responder: String,
        tool_call_id: String,
        option_id: String,
    },
    /// Start or stop gateway-side push-to-talk.
    VoiceRecord,
    Workflow(talaria_workflow::progress::live::ProgressKey),
    /// Input owned by the configuration assistant overlay.
    Configure(Box<talaria_core::configure::ConfigureInput>),
    /// Input owned by the capabilities overlay.
    Capabilities(Box<CapInput>),
    Cron(Box<CronInput>),
    /// Input owned by the settings overlay.
    Settings(talaria_core::settings::SettingsInput),
    /// A key for the doctor overlay.
    Doctor(DoctorKey),
    /// An input captured by the profiles overlay.
    Profiles(ProfilesInput),
    /// The terminal resized while the profiles overlay was open.
    ProfilesLayout {
        width: u16,
        page_size: usize,
    },
    Providers(Box<ProvidersInput>),
    /// Input owned by the model roles page.
    ModelRoles(talaria_core::model_roles::RolesInput),
    /// Input owned by the MCP overlay, before the composer sees it.
    Mcp(talaria_core::mcp::McpInput),
    /// Input owned by the memory overlay.
    Memory(talaria_core::memory::MemoryInput),
    Connectors(talaria_core::connectors::ConnectorsInput),
    /// A key for the open fleet overlay.
    Fleet(talaria_core::fleet_view::FleetViewInput),
    /// The composer's key for the fleet overlay: open it as `/fleet` does.
    FleetOpen,
    /// Open a selected session from a panel in another multiplexer pane.
    OpenInPane {
        key: RowKey,
        title: String,
    },
    /// The peek's card was answered.
    FleetAnswer(crate::peek::CardAnswer),
    /// The terminal's focused fleet card changed.
    FleetAnswerEdited,
    /// Open or close the foreground subagents panel.
    ToggleAgents,
    /// Open the foreground session's workflow run, or the run list when it has none.
    WorkflowView,
    /// A control from the subagents overlay.
    Subagent(crate::subagents::SubagentAction),
    /// Open the panel for another attached owner.
    SubagentPanel(RowKey),
    /// Visit the next attached session with subagent rows.
    SubagentNextSession,
    /// A panel key clears its previous outcome.
    SubagentClearFlash,
    /// An action from the background-process overlay.
    Processes(ProcessesIntent),
    /// The fleet view's launch dialog, which the terminal owns, changed.
    LaunchDialog(crate::bg::dialog::DialogEvent),
    /// A key for the open review.
    Review(talaria_core::review::ReviewKey),
    /// A paste for the open review's commit editor.
    ReviewPaste(String),
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
    ModelFilterPaste(String),
    SelectModelRow(talaria_core::models::RowKey),
    ModelConfirm(ConfirmKey),
    /// The composer sent this text.
    Submit(String),
    /// Stop the running turn now (Ctrl+C, or Esc on a card).
    Interrupt,
    /// The interrupt key was pressed in the composer. While a turn runs the first press only
    /// arms a confirmation and a second press inside the window interrupts; with no turn
    /// running it does what [`UserAction::Interrupt`] does.
    InterruptKey,
    /// `/quit` or the composer's exit request.
    Quit,
    /// A key the transcript screen owns.
    Input(InputKey),
    /// A connection operation card was answered.
    Connect {
        session_id: String,
        op_id: String,
        choice: ConnectionChoice,
    },
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
        target: MaskedTarget,
        value: Zeroizing<String>,
    },
    /// A line the screen wants shown.
    Notice(String),
    /// A queue key in selection or edit mode. Issue #232.
    Queue(QueueIntent),
    /// The draft changed, or a completion-list key was pressed. Issue #223.
    Palette(PaletteInput),
    /// A key for the open `/theme` or `/skin` selector.
    Choice(talaria_core::commands::ChoiceKey),
    /// A key for the open `/effort` selector.
    EffortScale(talaria_core::models::ScaleKey),
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
    /// One input for the session picker (#231).
    Picker(talaria_core::directory::state::PickerInput),
    /// Jump to the oldest background request.
    AttentionNext,
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
    pub layout: &'a talaria_core::layout::LayoutState,
    pub menu_fleet_enabled: bool,
    pub menu_remote_gateway: bool,
    pub kanban: Option<&'a talaria_core::kanban::KanbanBoardState>,
    /// Configured external agents and the selected external transcript.
    pub acp_agents: &'a [(String, String)],
    pub acp_peek: Option<&'a talaria_core::fleet::acp::AcpMapping>,
    pub voice: Option<talaria_core::voice::VoiceView>,
    pub workflow_progress: Option<&'a talaria_workflow::progress::live::ProgressState>,
    pub workflow_picker: Option<&'a talaria_workflow::runner::ProgressPicker>,
    pub configure: Option<talaria_core::configure::ConfigureView>,
    pub configure_refining: bool,
    pub configure_session: Option<&'a SessionState>,
    pub capabilities: Option<CapView>,
    pub connectors: Option<talaria_core::connectors::ConnectorsView>,
    pub connectors_filtering: bool,
    pub settings: Option<talaria_core::settings::SettingsView>,
    pub doctor: Option<&'a DoctorView>,
    pub doctor_restart: &'a RestartState,
    /// The profiles overlay while open, kept independently of the foreground session.
    pub profiles: Option<ProfilesView>,
    /// A profiles text field owns printable keys.
    pub profiles_text_focus: bool,
    /// The model roles page, when it is open for the connection's profile.
    pub model_roles: Option<talaria_core::model_roles::RolesView>,
    pub memory: Option<talaria_core::memory::MemoryView>,
    pub subagents: Option<&'a crate::subagents::SubagentHub>,
    /// Gateway usage fallback until this session's tree is known.
    pub subagents_usage_active: u32,
    pub fleet_snapshot: Option<&'a crate::fleet::FleetSnapshot>,
    /// The foreground session's background-process overlay is open.
    pub processes_open: bool,
    pub state: Option<&'a SessionState>,
    pub session_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub info: Option<&'a SessionInfoSummary>,
    pub link: LinkView,
    pub profile: &'a str,
    /// Host of a non-local gateway. Prompt cards name it so a remote question is visible.
    pub gateway_host: Option<&'a str>,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    /// The provider-adjusted effort, when known.
    pub effort_wire: Option<&'a str>,
    /// Catalogue capability of the foreground route, when known.
    pub effort_offered: Option<bool>,
    pub masked: &'a [MaskedRequestInfo],
    pub held: usize,
    /// The slash palette under the composer.
    pub palette: &'a PaletteState,
    /// The `/theme` or `/skin` selector under the composer, while it is open.
    pub choice: Option<&'a talaria_core::commands::ChoiceList>,
    /// The `/effort` selector under the composer, while it is open.
    pub effort_scale: Option<&'a talaria_core::models::EffortScale>,
    /// The rewind overlay, while it is open. Issue #233.
    pub rewind: Option<RewindView<'a>>,
    /// Whether the context figure must stay hidden for this session.
    pub context_hidden: bool,
    /// The compacting line, while a compaction runs.
    pub compacting_line: Option<CompactingLine>,
    /// The standing automation rows above the composer. Issue #236.
    pub automation: &'a AutomationState,
    /// Epoch milliseconds used by standing automation countdowns.
    pub automation_now_ms: u64,
    /// Elapsed application milliseconds for side tasks and compaction.
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
    /// The session picker's state, while the overlay is open.
    pub picker: Option<&'a PickerState>,
    /// The fleet overlay, while it is open.
    pub fleet: Option<&'a FleetViewState>,
    /// The peeked session the overlay's panel draws.
    pub peek: Option<&'a crate::fleet_actions::PeekFrame>,
    /// What the launch dialog should show, with a version that rises on each change.
    pub launch_status: Option<(u64, &'a crate::bg::dialog::DialogStatus)>,
    /// The folder Talaria was started in: the launch dialog's folder with no session open.
    pub launch_folder: &'a Path,
    /// The open review, drawn in the overlay in place of the fleet list or over the screen.
    pub review: Option<talaria_core::review::ReviewView<'a>>,
    /// The foreground folder's pull-request label, for the status line.
    pub pull_request: Option<talaria_core::review::PrLabel>,
    /// Pull-request labels of fleet rows.
    pub pr_labels: BTreeMap<RowKey, talaria_core::review::PrLabel>,
    /// The background counters the status line shows.
    pub fleet_counts: StatusCounts,
    pub hivecode: bool,
    /// The foreground session's active workflow run, for the dock line and the status bar.
    pub workflow_run: Option<&'a talaria_workflow::runner::RunSummary>,
    /// What the journal says about the foreground session, when it is a workflow agent's.
    pub workflow_agent: Option<&'a talaria_workflow::progress::agent_session::AgentSessionNote>,
    /// One press of the interrupt key is waiting for a second: the working row says so.
    pub interrupt_armed: bool,
    /// The open session is a workflow agent's, opened from the run view, and the person can go
    /// back to the session they came from.
    pub agent_back: bool,
    /// The building block, while a routed turn builds a workflow and no run it started shows.
    pub authoring: Option<talaria_core::hivecode::authoring::AuthoringView>,
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
    /// Agent executables configured before the application loop starts.
    fn acp_config(&self) -> crate::acp::AcpConfig {
        crate::acp::AcpConfig::default()
    }
    /// Open or close the profiles overlay before other overlay synchronizers run.
    fn sync_profiles(&mut self, _view: Option<&ProfilesView>) {}
    /// The labels of the keys bound to the profiles actions the hints name.
    fn profiles_key_labels(&self) -> talaria_core::profiles::KeyLabels {
        talaria_core::profiles::KeyLabels::default()
    }
    /// The label of the key bound to reloading settings, named by the chooser's hints.
    fn settings_reload_key(&self) -> Option<String> {
        Some("Ctrl+R".into())
    }
    /// The number of profiles rows visible above notices in this terminal.
    fn profiles_page_size(&self, view: &ProfilesView) -> usize {
        let (width, height) = self.pane_size();
        if width >= 100 {
            usize::from(height.saturating_sub(2) / 2)
                .min(view.rows.len())
                .max(1)
        } else {
            usize::from(height.saturating_sub(1)).max(1)
        }
    }
    /// Hand the terminal to the raw soul editor; an asynchronous result returns as an input.
    fn edit_profile_soul(&mut self, _text: String) -> Option<RawEdit> {
        Some(RawEdit::Failed(
            "The external editor is unavailable.".to_string(),
        ))
    }
    fn sync_providers(&mut self, _view: Option<ProvidersView>) {}
    fn provider_secret(&self, _effect: &ProvidersEffect) -> Option<Zeroizing<String>> {
        None
    }
    fn forget_provider_secret(&mut self) {}
    fn copy_provider_text(&mut self, _text: &str) {}
    fn sync_mcp(&mut self, _view: Option<&talaria_core::mcp::McpView>) {}
    fn take_mcp_secret(
        &mut self,
        _effect: &talaria_core::mcp::McpEffect,
    ) -> Option<Zeroizing<String>> {
        None
    }
    fn forget_mcp_secrets(&mut self) {}
    fn open_mcp_url(&mut self, _url: &str) -> bool {
        false
    }
    fn copy_mcp_text(&mut self, _text: &str) {}
    fn sync_cron(&mut self, _view: Option<CronView>, _history_focused: bool) -> bool {
        false
    }
    fn edit_connector_secret(&mut self, _key: &str) {}
    fn forget_connector_secrets(&mut self) {}
    fn connector_secrets(
        &self,
        _effect: &talaria_core::connectors::ConnectorsEffect,
    ) -> Option<BTreeMap<String, zeroize::Zeroizing<String>>> {
        None
    }
    /// The health overlay's final integration consumes this handoff.
    fn open_connector_doctor(&mut self) {}
    /// Multiplexer pane settings from the screen's startup configuration.
    fn pane_settings(&self) -> PaneSettings {
        PaneSettings::default()
    }
    /// This terminal's current columns and rows.
    fn pane_size(&self) -> (u16, u16) {
        (80, 24)
    }
    /// Startup preference; later changes belong to the runtime view alone.
    fn work_panel(&self) -> bool {
        false
    }
    fn seat_reporting(&self) -> bool {
        true
    }
    fn menu_bar(&self) -> bool {
        false
    }
    fn mouse_navigation(&self) -> bool {
        false
    }
    /// Startup form of reasoning on the main screen; `/thinking` changes the view alone.
    fn reasoning_display(&self) -> talaria_core::layout::ReasoningDisplay {
        talaria_core::layout::ReasoningDisplay::Collapsed
    }
    /// Identify the source before ordered commit effects reach the screen. The
    /// counter locates the uncommitted tail; callback order locates local output.
    fn sync_workspace_transcript(&mut self, _source_epoch: u64, _committed_cells: usize) {}
    fn subagents_config(&self) -> crate::subagents::SubagentsConfig {
        crate::subagents::SubagentsConfig::default()
    }
    /// Commit one finished batch using the terminal's native scrollback.
    fn commit_subagent_batch(&mut self, _batch: &talaria_core::subagents::FinishedBatch) {}
    /// Synchronize model overlays and cards without passing terminal state into the app.
    fn sync_models(
        &mut self,
        _picker: Option<PickerView<'_>>,
        _confirmation: Option<&Confirmation>,
    ) {
    }
    /// Open, draw or close the session picker overlay.
    fn sync_picker(&mut self, _picker: Option<&PickerState>) {}
    /// Open, draw or close the fleet overlay: `view.fleet` is its state and `view.peek` the
    /// peeked background session.
    fn sync_fleet(&mut self, _view: &View<'_>) {}
    fn model_list_height(&self) -> usize {
        9
    }
    fn model_details_scroll_limit(&self) -> Option<usize> {
        None
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
    /// Save a line the slash palette ran to Up-arrow recall and the history file. A notice
    /// comes back when the file could not be written.
    fn record_submit(&mut self, _text: &str, _has_session: bool) -> Option<String> {
        None
    }
    /// Hand back a prompt that left the queue: it goes into the composer only when the composer
    /// is empty and no edit is open; the transcript already shows it (#232).
    fn offer_draft(&mut self, text: &str) {
        let _ = text;
    }
    /// Clear a submitted draft only if the composer still contains that text.
    fn clear_submitted_draft(&mut self, _text: &str) -> Option<(String, usize)> {
        None
    }
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
    fn report_seat(&mut self, _state: SeatState) {}
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
            speech: vec![],
            done_after: Duration::from_secs(20),
        }
    }

    /// The fleet's alert settings: the channels, the done threshold and the two scopes.
    fn fleet_attention_config(&self) -> FleetAttentionConfig {
        FleetAttentionConfig {
            bell: vec![
                talaria_core::attention::AlertKind::NeedsYou,
                talaria_core::attention::AlertKind::Failed,
            ],
            desktop: vec![
                talaria_core::attention::AlertKind::NeedsYou,
                talaria_core::attention::AlertKind::Done,
                talaria_core::attention::AlertKind::Failed,
            ],
            speech: vec![],
            done_after: Duration::from_secs(20),
            needs_you: FleetScope::All,
            finished: FleetScope::Watched,
        }
    }
    /// How the start-up notice names the jump key; `None` leaves the key out.
    fn attention_key_label(&self) -> Option<String> {
        None
    }
    /// How workflow run notices name the key that opens the run; `None` leaves the key out.
    fn workflow_key_label(&self) -> Option<String> {
        None
    }

    /// The start-up probe colours and the polarity decided from them, captured once.
    fn startup_appearance(&self) -> Option<(TerminalColors, (Polarity, PolaritySource))> {
        None
    }
    /// Theme file selection prepared before the application starts.
    fn startup_theme_files(&self) -> Option<appearance::ThemeFileSession> {
        None
    }
    /// The skin or the session polarity changed: resolve the theme again for the composer, the
    /// status line and cells that start from now on. Scrollback is never redrawn.
    fn appearance_changed(&mut self, _view: &AppearanceView<'_>) {}
    /// What the resolved theme reports, for `/theme`.
    fn theme_report(&self) -> Option<ThemeReport> {
        None
    }
    /// Additional report only for a selected theme file.
    fn theme_file_report(&self) -> Option<talaria_core::appearance::ThemeFileReport> {
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
    fn voice_config(&self) -> crate::config::VoiceConfig {
        crate::config::VoiceConfig::default()
    }
    fn configure_model(&self) -> Option<(String, String)> {
        None
    }
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

struct BackgroundBinding {
    service: Option<Arc<crate::background::BackgroundService>>,
    generation: u64,
}

#[derive(Default)]
struct CronBinding {
    profile: String,
    generation: u64,
    epoch: u64,
    state: Option<CronState>,
    service: Option<Arc<crate::cron::CronService>>,
    opening: Option<CronOpen>,
    initializing: bool,
    deadline: Option<Instant>,
    switching_run: bool,
    first_frame_pending: bool,
}

pub struct AppState {
    pub gateway_version: Option<String>,
    compat_notice_shown: bool,
    pub layout: talaria_core::layout::LayoutState,
    pub kanban: Option<talaria_core::kanban::KanbanBoardState>,
    kanban_service: Option<crate::kanban::KanbanService>,
    pub voice: crate::voice::VoiceService,
    client_voice: talaria_core::voice::client::ClientVoice,
    client_voice_config: crate::config::VoiceConfig,
    client_voice_target: Option<talaria_core::voice::target::RecordTarget>,
    client_voice_drafts: BTreeMap<RowKey, talaria_core::voice::target::RecordTarget>,
    client_voice_inflight: BTreeSet<RowKey>,
    client_speech_modes: BTreeMap<(String, String), talaria_core::voice::SpeechMode>,
    client_voice_enabled: bool,
    client_voice_worker: Option<Arc<crate::voice::relay::ClientAudio>>,
    client_voice_closing: Vec<tokio::task::JoinHandle<()>>,
    client_helper_status: &'static str,
    client_playback_status: &'static str,
    client_relay_status: &'static str,
    voice_last_tick_ms: u64,
    voice_connection_generation: u64,
    voice_submit_mode: Option<Option<String>>,
    voice_config_pending: bool,
    /// The launch dialed a remote gateway. Voice must not ask that gateway to record or speak.
    remote_gateway: bool,
    /// Host of that gateway, when it is not a loopback address.
    remote_host: Option<String>,
    remote_voice_noticed: BTreeSet<(String, String)>,
    voice_submissions: BTreeMap<String, String>,
    remembered_notes: BTreeMap<String, String>,
    pub workflow_views: crate::workflow_progress::ProgressViews,
    /// The open session, when it belongs to a workflow agent.
    pub(crate) workflow_agent: Option<crate::workflow_agent::WorkflowAgentSession>,
    /// Stored ids of workflow agent sessions the person has been warned about.
    workflow_agent_warned: HashSet<String>,
    /// Where the person came from when the run view opened the agent session in the window.
    pub(crate) workflow_origin: Option<crate::workflow_origin::WorkflowOrigin>,
    /// A trip back to that place that is under way: the run view opens when it lands.
    pub(crate) workflow_return: Option<crate::workflow_origin::WorkflowReturn>,
    /// Where workflow runs live, for finding the run a session's folder names.
    pub(crate) workflow_runs_dir: PathBuf,
    pub hivecode: crate::hivecode::HivecodeService,
    hivecode_requests: std::collections::HashMap<SessionId, u64>,
    /// Where the per-session switch is saved; none in tests.
    hivecode_file: Option<crate::hivecode_files::ModeFile>,
    /// The number of the last save of the switch file.
    hivecode_saves: u64,
    configure: Option<talaria_core::configure::ConfigureState>,
    configure_service: Option<Arc<crate::configure::Service>>,
    configure_generation: u64,
    configure_tick_at: Option<Instant>,
    configure_model: Option<(String, String)>,
    capabilities: Option<CapabilitiesState>,
    capabilities_service: Option<Arc<crate::capabilities::CapabilitiesService>>,
    capabilities_generation: u64,
    capabilities_started: Instant,
    memory: talaria_core::memory::MemoryState,
    memory_visible: bool,
    memory_profile: Option<String>,
    memory_service: Option<Arc<crate::memory::MemoryService>>,
    memory_generation: u64,
    cron: CronBinding,
    settings: talaria_core::settings::SettingsState,
    settings_service: Option<Arc<crate::settings::SettingsService>>,
    settings_generation: u64,
    /// Profiles whose harmful Hermes settings were already looked up in this run, so each profile
    /// is announced once.
    config_checked: BTreeSet<String>,
    /// The doctor overlay, while it is open.
    pub doctor: Option<DoctorView>,
    /// A restart continues after its overlay closes.
    pub doctor_restart: RestartState,
    /// Profiles and kept soul edits outlive foreground session changes.
    pub profiles: ProfilesState,
    profiles_ticks: BTreeMap<u64, JoinHandle<()>>,
    pub providers: ProvidersState,
    provider_service: Option<Arc<crate::providers::ProviderService>>,
    providers_model_return: Option<String>,
    provider_generation: u64,
    provider_initializing: bool,
    provider_pending_loads: Vec<ProvidersEffect>,
    model_roles: Option<talaria_core::model_roles::ModelRolesState>,
    model_roles_profile: String,
    /// The connected profile when the page opened; the page closes when the connection moves.
    model_roles_connection: String,
    /// The page was opened from /profiles and returns there when it closes.
    model_roles_return_to_profiles: bool,
    model_roles_service: Option<Arc<crate::model_roles::ModelRolesService>>,
    model_roles_generation: u64,
    pub mcp: talaria_core::mcp::McpState,
    mcp_service: Option<Arc<crate::mcp::McpService>>,
    mcp_binding: u64,
    mcp_preparing: bool,
    mcp_waiting: Vec<talaria_core::mcp::McpEffect>,
    pub models: ModelsState,
    model_service: Option<Arc<crate::models::ModelService>>,
    model_notice_counter: u64,
    model_warnings: BTreeSet<String>,
    attention: talaria_core::attention::AttentionTracker,
    /// Whether foreground seat changes are reported to the terminal.
    pub seat_reporting: bool,
    attention_at: Instant,
    /// The background attention hub: the fleet tracker, the shared gate and the holds.
    hub: AttentionHub,
    /// The counters the status line shows.
    status_counts: StatusCounts,
    /// The foreground session's open requests.
    foreground_open: u32,
    /// The instant the hub wants to be woken at, while one is pending.
    fleet_wake_at: Option<Instant>,
    /// The running wake task, aborted when a newer wake replaces it.
    fleet_wake_task: Option<JoinHandle<()>>,
    pub opened: Option<OpenedSession>,
    opened_profile: Option<String>,
    /// Whether the foreground session's process overlay is open.
    pub processes_open: bool,
    pub(crate) retained_sessions: BTreeMap<RowKey, OpenedSession>,
    connection_generation: u64,
    background_generation_base: u64,
    background_stop_bindings: BTreeMap<(String, u64), BackgroundBinding>,
    wall_s: f64,
    background_service: Option<Arc<crate::background::BackgroundService>>,
    background_after_interrupt: Vec<(SessionId, BackgroundEffect)>,
    pub held: VecDeque<String>,
    pub launch_id: String,
    pub prompt_counter: u64,
    pub quitting: bool,
    pub committed_cells: usize,
    pub link_failed: bool,
    pub session_gone: bool,
    /// The profile the resume hint names.
    pub profile: String,
    /// When false, `/fleet` shows a notice and opens nothing.
    pub fleet_enabled: bool,
    /// The fleet overlay, while `/fleet` has it open.
    pub fleet: Option<FleetViewState>,
    pub link_state: LinkView,
    /// An interrupt sequence is in flight; further presses send nothing.
    pub interrupting: bool,
    /// The title of the row picked from the `--resume` list.
    pub list_title: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub effort_wire: Option<String>,
    pub masked: Vec<MaskedRequestInfo>,
    /// Composer draft. The slash palette reads it. Issue #223.
    pub draft: String,
    /// Slash palette. Closed until a slash draft opens it. Issue #223.
    pub palette: PaletteState,
    /// The open `/theme` or `/skin` selector.
    pub choice: Option<choice::Selector>,
    /// A skin preview that changed, or may change, the profile's skin.
    pub skin_preview: Option<choice::SkinPreview>,
    /// The catalogue store for this connection.
    pub commands: CommandsState,
    /// Catalogue collisions already shown for a profile and stored session.
    command_collisions: BTreeSet<(RowKey, String)>,
    /// Commands Talaria runs itself.
    pub local: LocalCommandTable,
    /// The session picker's state, while the overlay is open.
    pub picker: Option<PickerState>,
    /// The profiles the side connection serves.
    pub picker_profiles: Vec<String>,
    /// The note the side connection's scope carries.
    pub picker_scope_note: Option<String>,
    /// The listing generation; one more than the last.
    pub picker_generation: u64,
    /// Takes the pane-open acknowledgement in tests. Issue #246.
    pub pane_sink: Option<Arc<dyn FleetSink>>,
    /// The environment pane detection reads. The process environment at
    /// startup; tests point it at fakes. Issue #246.
    pub pane_env: PaneEnv,
    /// Settings used when opening a fleet session beside this terminal.
    pub pane_settings: PaneSettings,
    /// This terminal's current columns and rows, supplied by the screen.
    pub pane_size: (u16, u16),
    /// Optional runner for application-loop tests; production uses `SystemRunner`.
    pub pane_runner: Option<Arc<dyn CommandRunner>>,
    /// The one pane opening still in flight, retaining its title if the overlay closes.
    pub(crate) pending_pane: Option<(RowKey, String)>,
    /// A switch or a branch is in flight.
    pub switching: bool,
    /// A rename, an export or a switch runs; one session action at a time.
    pub session_action: bool,
    /// The Hermes home, for opening a link for another profile.
    pub hermes_home: PathBuf,
    /// The folder the launch runs in; the default export target.
    pub launch_folder: PathBuf,
    /// The launch, rebuilt for another profile when the picker opens one.
    pub launch: Option<Launch>,
    /// The row a switch is running for.
    pub pending_switch: Option<RowKey>,
    /// Target events received before its resume snapshot is installed.
    pending_switch_events: Vec<hermes_client::InboundEvent>,
    /// The gateway's refusal reason of the last failed switch.
    pub switch_reason: Option<String>,
    /// The channel the loop reads; a new link forwards its events through it.
    pub sender: Option<mpsc::UnboundedSender<AppMessage>>,
    /// How many times a new link has replaced the foreground link. Each link's forwarder knows
    /// the count it was started for and stops forwarding once a later link is installed.
    pub link_epoch: Arc<AtomicU64>,
    /// The rewind overlay and the typed rewind commands. Issue #233.
    rewind: RewindState,
    /// Session that owns the rewind state and outstanding calls.
    rewind_owner: Option<RowKey>,
    /// Two quick eligible Esc presses open the rewind overlay.
    double_esc: DoubleEsc,
    /// The first press of the interrupt key while a turn runs waits here for a second one.
    interrupt_confirm: InterruptConfirm,
    /// Set by the loop around a tick while input is waiting to be read.
    keys_waiting: bool,
    /// Retry submissions still waiting for their outcome, keyed by submission id.
    retry: BTreeMap<String, (RowKey, String)>,
    /// Failed retries waiting to return to their original session's composer.
    retry_returns: BTreeMap<RowKey, VecDeque<String>>,
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
    pub(crate) cd_ledger: MutationLedger,
    /// The foreground session's folder and branch (#237).
    pub(crate) folder: SessionFolder,
    /// The home folder `~` stands for in `/cd` (#237).
    pub home: Option<PathBuf>,
    /// How the `/cd` refusal names the interrupt key (#237).
    pub interrupt_label: Option<String>,
    /// How workflow run notices name the key that opens the run.
    pub workflow_key_label: Option<String>,
    /// The folder check and the move; a new one with every connection (#237).
    workspace_service: Option<Arc<WorkspaceService>>,
    counter: Arc<AtomicU64>,
    pub(crate) service: Option<SessionService>,
    pub(crate) link: Option<Link>,
    prompts: Option<Arc<Mutex<PromptService>>>,
    pending: Option<JoinHandle<Result<Ready, LaunchFailure>>>,
    command_service: Option<CommandService>,
    /// The subagent hub, once a sender exists to build it with. Issue #242.
    pub subagents: Option<crate::subagents::SubagentHub>,
    pub subagents_usage_active: u32,
    /// Standing automation state.
    pub automation: AutomationState,
    automation_ticker: AutomationTicker,
    automation_deadline: Option<Instant>,
    /// Mentions, paste labels and staged images for the open draft.
    pub attachments: AttachmentsState,
    caret: usize,
    attachment_epoch: u64,
    attachment_home: PathBuf,
    attachment_folder: PathBuf,
    submitted_attachments: BTreeMap<String, (String, SubmittedAttachments)>,
    held_attachments: VecDeque<SubmittedAttachments>,
    failed_attachments: VecDeque<(String, SubmittedAttachments, bool)>,
    /// The fleet service of an interactive launch, once the first frame is drawn.
    fleet_service: Option<crate::fleet::FleetHandle>,
    acp_manager: Option<crate::fleet::ExternalHandle>,
    acp_agents: Vec<(String, String)>,
    acp_views: BTreeMap<RowKey, talaria_core::fleet::acp::AcpMapping>,
    /// The latest fleet snapshot.
    pub fleet_snapshot: Option<Arc<crate::fleet::FleetSnapshot>>,
    /// The foreground row and view last reported to the fleet service.
    fleet_reported: Option<(RowKey, talaria_core::fleet::SubscribedView)>,
    /// The worker that owns the peek, once the overlay first needed it.
    peek_worker: Option<crate::fleet_actions::PeekWorker>,
    /// How long the peek waits for each call.
    pub peek_timeouts: crate::peek::PeekTimeouts,
    /// The peeked session, as the worker last reported it.
    pub peek_frame: Option<crate::fleet_actions::PeekFrame>,
    /// The peeked row attached to `subagents` on behalf of the fleet peek.
    peek_subagent_key: Option<RowKey>,
    /// The foreground reply the overlay sent: its submission id and row.
    fleet_fg_reply: Option<(String, RowKey)>,
    /// The foreground row whose stop the overlay sent.
    fleet_fg_stop: Option<RowKey>,
    /// The fleet view's launch dialog and the launches it started.
    pub launch_ui: crate::bg::dialog::LaunchUi,
    /// The open review, if any, and the runners it uses.
    pub review_service: Option<crate::review::ReviewService>,
    /// Where the open review was opened from.
    review_origin: Option<ReviewOrigin>,
    /// Pull-request status, shared by the review, the fleet column and the status line.
    review_pr: Option<Arc<crate::review::PrStatusService>>,
    /// Repositories whose worktree records map branches to sessions.
    review_repos: Vec<PathBuf>,
    /// Seconds since 1970 at app-clock zero, from the latest fleet snapshot.
    fleet_clock: f64,
    /// The connectors overlay. Closed until a command opens it.
    pub connectors: talaria_core::connectors::ConnectorsState,
    /// Whether the connectors overlay is on screen.
    pub connectors_open: bool,
    /// Profiles that have agreed to a connectors call in this process.
    pub connectors_consent: crate::connectors::ConnectorsConsent,
    connectors_service: Option<Arc<crate::connectors::ConnectorsService>>,
    connectors_generation: u64,
    connectors_profile: Option<String>,
    connectors_deadline: Option<Instant>,
    connectors_reads: BTreeMap<String, u64>,
}

struct SubmittedAttachments {
    expanded: Expanded,
    media: Vec<Chip>,
}

impl AppState {
    #[inline(never)]
    fn new_boxed(launch_id: String) -> Box<Self> {
        Box::new(Self::new(launch_id))
    }

    #[inline(never)]
    #[must_use]
    pub fn new(launch_id: impl Into<String>) -> Self {
        let launch_id = launch_id.into();
        // Hermes keeps receipts by request id for the whole session, so each ledger numbers
        // its requests under its own prefix: a rename and a model pick never share an id.
        let ledger = MutationLedger::new(&launch_id);
        let cd_ledger = MutationLedger::new(&format!("{launch_id}-cd"));
        Self {
            gateway_version: None,
            compat_notice_shown: false,
            voice: crate::voice::VoiceService::new(),
            client_voice: talaria_core::voice::client::ClientVoice::default(),
            client_voice_config: crate::config::VoiceConfig::default(),
            client_voice_target: None,
            client_voice_drafts: BTreeMap::new(),
            client_voice_inflight: BTreeSet::new(),
            client_speech_modes: BTreeMap::new(),
            client_voice_enabled: false,
            client_voice_worker: None,
            client_voice_closing: Vec::new(),
            client_helper_status: "not checked",
            client_playback_status: "not checked",
            client_relay_status: "not checked",
            layout: talaria_core::layout::LayoutState::default(),
            voice_last_tick_ms: 0,
            voice_connection_generation: 0,
            voice_submit_mode: None,
            voice_config_pending: false,
            remote_gateway: false,
            remote_host: None,
            remote_voice_noticed: BTreeSet::new(),
            voice_submissions: BTreeMap::new(),
            remembered_notes: BTreeMap::new(),
            configure: None,
            configure_service: None,
            configure_generation: 0,
            configure_tick_at: None,
            configure_model: None,
            capabilities: None,
            capabilities_service: None,
            capabilities_generation: 0,
            capabilities_started: Instant::now(),
            memory: talaria_core::memory::MemoryState::new(),
            memory_visible: false,
            memory_profile: None,
            memory_service: None,
            memory_generation: 0,
            cron: CronBinding::default(),
            settings: talaria_core::settings::SettingsState::new(
                talaria_core::settings::SETTINGS_LINKS,
            ),
            settings_service: None,
            settings_generation: 0,
            config_checked: BTreeSet::new(),
            doctor: None,
            doctor_restart: RestartState::Idle,
            profiles: ProfilesState::new("default"),
            profiles_ticks: BTreeMap::new(),
            providers: ProvidersState::new(),
            provider_service: None,
            providers_model_return: None,
            provider_generation: 0,
            provider_initializing: false,
            provider_pending_loads: Vec::new(),
            model_roles: None,
            model_roles_profile: String::new(),
            model_roles_connection: String::new(),
            model_roles_return_to_profiles: false,
            model_roles_service: None,
            model_roles_generation: 0,
            mcp: talaria_core::mcp::McpState::default(),
            mcp_service: None,
            mcp_binding: 0,
            mcp_preparing: false,
            mcp_waiting: Vec::new(),
            models: ModelsState::new(&format!("{launch_id}-model"), Ok(SavedModels::empty())),
            model_service: None,
            model_notice_counter: 0,
            model_warnings: BTreeSet::new(),
            seat_reporting: true,
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
                    speech: vec![],
                    done_after: Duration::from_secs(20),
                },
            ),
            attention_at: Instant::now(),
            hub: AttentionHub::new(
                FleetAttentionConfig {
                    bell: vec![
                        talaria_core::attention::AlertKind::NeedsYou,
                        talaria_core::attention::AlertKind::Failed,
                    ],
                    desktop: vec![
                        talaria_core::attention::AlertKind::NeedsYou,
                        talaria_core::attention::AlertKind::Done,
                        talaria_core::attention::AlertKind::Failed,
                    ],
                    speech: vec![],
                    done_after: Duration::from_secs(20),
                    needs_you: FleetScope::All,
                    finished: FleetScope::Watched,
                },
                "default".to_string(),
                None,
            ),
            status_counts: StatusCounts::default(),
            foreground_open: 0,
            fleet_wake_at: None,
            fleet_wake_task: None,
            opened: None,
            opened_profile: None,
            processes_open: false,
            retained_sessions: BTreeMap::new(),
            connection_generation: 0,
            background_generation_base: 0,
            background_stop_bindings: BTreeMap::new(),
            wall_s: 0.0,
            background_service: None,
            background_after_interrupt: Vec::new(),
            held: VecDeque::new(),
            launch_id,
            prompt_counter: 0,
            quitting: false,
            committed_cells: 0,
            link_failed: false,
            session_gone: false,
            profile: "default".to_string(),
            fleet_enabled: true,
            fleet: None,
            link_state: LinkView::Connecting,
            interrupting: false,
            list_title: None,
            model: None,
            effort: None,
            effort_wire: None,
            masked: Vec::new(),
            draft: String::new(),
            palette: PaletteState::closed(),
            choice: None,
            skin_preview: None,
            hivecode: crate::hivecode::HivecodeService::default(),
            hivecode_requests: std::collections::HashMap::new(),
            hivecode_file: None,
            hivecode_saves: 0,
            commands: CommandsState::default(),
            command_collisions: BTreeSet::new(),
            local: LocalCommandTable::builtin(),
            picker: None,
            picker_profiles: Vec::new(),
            picker_scope_note: None,
            picker_generation: 0,
            pane_sink: None,
            pane_env: PaneEnv::from_process(),
            pane_settings: PaneSettings::default(),
            pane_size: (80, 24),
            pane_runner: None,
            pending_pane: None,
            switching: false,
            session_action: false,
            hermes_home: PathBuf::new(),
            launch_folder: PathBuf::new(),
            launch: None,
            pending_switch: None,
            pending_switch_events: Vec::new(),
            switch_reason: None,
            sender: None,
            link_epoch: Arc::new(AtomicU64::new(0)),
            rewind: RewindState::default(),
            rewind_owner: None,
            double_esc: DoubleEsc::default(),
            interrupt_confirm: InterruptConfirm::default(),
            keys_waiting: false,
            retry: BTreeMap::new(),
            retry_returns: BTreeMap::new(),
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
            workflow_key_label: None,
            workspace_service: None,
            service: None,
            link: None,
            prompts: None,
            pending: None,
            command_service: None,
            subagents: None,
            kanban: None,
            kanban_service: None,
            workflow_views: crate::workflow_progress::ProgressViews::default(),
            workflow_agent: None,
            workflow_agent_warned: HashSet::new(),
            workflow_origin: None,
            workflow_return: None,
            workflow_runs_dir: talaria_workflow::runner::runs_dir(None),
            subagents_usage_active: 0,
            attachments: AttachmentsState::new(CollapseRule::HERMES_DEFAULT, 16 * 1024 * 1024),
            caret: 0,
            attachment_epoch: 0,
            attachment_home: PathBuf::new(),
            attachment_folder: PathBuf::new(),
            fleet_service: None,
            acp_manager: None,
            acp_agents: Vec::new(),
            acp_views: BTreeMap::new(),
            fleet_snapshot: None,
            fleet_reported: None,
            peek_worker: None,
            peek_timeouts: crate::peek::PeekTimeouts::default(),
            peek_frame: None,
            peek_subagent_key: None,
            fleet_fg_reply: None,
            fleet_fg_stop: None,
            launch_ui: crate::bg::dialog::LaunchUi::default(),
            review_service: None,
            review_origin: None,
            review_pr: None,
            review_repos: Vec::new(),
            fleet_clock: 0.0,
            submitted_attachments: BTreeMap::new(),
            held_attachments: VecDeque::new(),
            failed_attachments: VecDeque::new(),
            automation: AutomationState::default(),
            automation_ticker: AutomationTicker::default(),
            automation_deadline: None,
            connectors: talaria_core::connectors::ConnectorsState::new(),
            connectors_open: false,
            connectors_consent: crate::connectors::ConnectorsConsent::default(),
            connectors_service: None,
            connectors_generation: 0,
            connectors_profile: None,
            connectors_deadline: None,
            connectors_reads: BTreeMap::new(),
        }
    }

    /// The view the screen draws from.
    #[inline(never)]
    /// Tell the application that input is waiting to be read, so a tick does not settle the
    /// interrupt key before that input is seen. The loop sets this around each tick.
    pub fn set_keys_waiting(&mut self, waiting: bool) {
        self.keys_waiting = waiting;
    }

    #[must_use]
    pub fn view(&self) -> View<'_> {
        let opened = self.opened.as_ref();
        View {
            layout: &self.layout,
            menu_fleet_enabled: self.fleet_enabled,
            menu_remote_gateway: self.remote_gateway,
            acp_agents: &self.acp_agents,
            acp_peek: self
                .fleet
                .as_ref()
                .and_then(|fleet| fleet.peek())
                .and_then(|(key, _)| self.acp_views.get(key)),
            voice: if self.client_voice_config.backend == "client-bridge" && opened.is_some() {
                let mut view = self.client_voice.view(
                    self.client_voice_enabled,
                    self.client_voice_config.read_aloud,
                );
                if self.client_voice.active()
                    && let Some(target) = &self.client_voice_target
                {
                    view.label = Some(target.key.session_id.clone());
                }
                Some(view)
            } else {
                opened
                    .and_then(|session| self.voice.get(&session.session_id))
                    .map(talaria_core::voice::VoiceState::view)
            },
            configure: self
                .configure
                .as_ref()
                .filter(|state| !state.is_closed())
                .map(|state| state.view(Instant::now())),
            configure_refining: self
                .configure
                .as_ref()
                .is_some_and(|state| state.refinement_active()),
            configure_session: self
                .configure
                .as_ref()
                .filter(|state| !state.is_closed())
                .and_then(|state| state.session()),
            capabilities: self
                .capabilities
                .as_ref()
                .filter(|state| state.is_open())
                .map(CapabilitiesState::view),
            memory: self
                .memory_profile
                .as_ref()
                .filter(|profile| self.memory_visible && *profile == &self.profile)
                .map(|_| self.memory.view(Instant::now())),
            connectors_filtering: self.connectors_open && self.connectors.is_filtering(),
            connectors: (self.connectors_open
                && self.connectors_profile.as_ref() == Some(&self.profile))
            .then(|| talaria_core::connectors::view(&self.connectors, Instant::now())),
            settings: self
                .settings
                .is_open()
                .then(|| self.settings.view())
                .filter(|view| view.profile == self.profile),
            doctor: self.doctor.as_ref(),
            doctor_restart: &self.doctor_restart,
            profiles: self.profiles.is_open().then(|| self.profiles.view()),
            profiles_text_focus: self.profiles.text_focus(),
            model_roles: self
                .model_roles
                .as_ref()
                .filter(|_| self.model_roles_connection == self.profile)
                .map(talaria_core::model_roles::ModelRolesState::view),
            processes_open: self.processes_open,
            state: opened.map(|opened| &opened.state),
            session_id: opened.map(|opened| opened.session_id.as_str()),
            title: opened
                .and_then(|opened| opened.title.as_deref())
                .or(self.list_title.as_deref()),
            info: opened.map(|opened| &opened.info),
            link: self.link_state,
            profile: &self.profile,
            gateway_host: self.remote_host.as_deref(),
            model: self
                .model
                .as_deref()
                .or_else(|| opened.and_then(|opened| opened.info.model.as_deref())),
            effort: self.effort.as_deref(),
            effort_wire: self.effort_wire.as_deref(),
            effort_offered: self.opened.as_ref().and_then(|opened| {
                self.models.effort_offered(
                    &opened.session_id,
                    opened.info.provider.as_deref()?,
                    self.model.as_deref()?,
                )
            }),
            masked: &self.masked,
            held: self.held.len(),
            palette: &self.palette,
            choice: choice::list(self),
            effort_scale: choice::scale(self),
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
            automation: &self.automation,
            automation_now_ms: wall_clock_ms(),
            picker: self.picker.as_ref(),
            fleet: self.fleet.as_ref(),
            peek: self.peek_frame.as_ref(),
            kanban: self.kanban.as_ref(),
            workflow_progress: self
                .workflow_views
                .active
                .as_ref()
                .and_then(|p| self.workflow_views.states.get(p)),
            workflow_picker: self.workflow_views.picker.as_ref(),
            subagents: self.subagents.as_ref(),
            subagents_usage_active: self.subagents_usage_active,
            fleet_snapshot: self.fleet_snapshot.as_deref(),
            launch_status: self.launch_ui.status(),
            launch_folder: &self.launch_folder,
            review: self
                .review_service
                .as_ref()
                .and_then(crate::review::ReviewService::view),
            pull_request: self.review_pr.as_ref().and_then(|pr| {
                pr.foreground_label()
                    .map(|(status, stale)| talaria_core::review::pr_label(&status, stale))
            }),
            pr_labels: self.review_pr.as_ref().map_or_else(BTreeMap::new, |pr| {
                pr.labels()
                    .into_iter()
                    .map(|(key, status)| (key, talaria_core::review::pr_label(&status, false)))
                    .collect()
            }),
            fleet_counts: self.status_counts,
            hivecode: self.hivecode.status_input(
                self.opened
                    .as_ref()
                    .map(|s| SessionId::new(s.session_id.clone()))
                    .as_ref(),
            ),
            workflow_run: self.opened.as_ref().and_then(|opened| {
                self.hivecode
                    .active_run(&SessionId::new(opened.session_id.clone()))
            }),
            workflow_agent: self
                .opened
                .as_ref()
                .and(self.workflow_agent.as_ref())
                .map(crate::workflow_agent::WorkflowAgentSession::note),
            interrupt_armed: turn_running(self) && self.interrupt_confirm.is_armed(self.now_ms),
            agent_back: self.workflow_agent.is_some()
                && opened
                    .zip(self.workflow_origin.as_ref())
                    .is_some_and(|(opened, origin)| origin.is_away_in(&opened.session_id)),
            authoring: self.opened.as_ref().and_then(|opened| {
                self.hivecode.authoring_view(
                    &SessionId::new(opened.session_id.clone()),
                    opened.state.cells(),
                    self.now_ms,
                )
            }),
        }
    }

    /// Arm the next countdown change, and say whether the section's text changed.
    ///
    /// The ticker compares the rows' text with the last call, so the app wakes only when a
    /// countdown changes what it shows.
    fn automation_wake(&mut self, now: Instant) -> bool {
        let wake = self
            .automation_ticker
            .on_wake(&self.automation, wall_clock_ms());
        self.automation_deadline = wake.next_in.map(|after| now + after);
        wake.redraw
    }

    /// A copy of the state a switch or a branch works on. The copy holds the handles the
    /// work needs and leaves the live state alone, so the loop keeps running while the
    /// network call is in flight.
    #[must_use]
    pub fn working_copy(&self) -> AppState {
        let mut copy = AppState::new(self.launch_id.clone());
        copy.gateway_version = self.gateway_version.clone();
        copy.compat_notice_shown = self.compat_notice_shown;
        copy.service = self.service.clone();
        copy.voice = self.voice.clone();
        copy.client_voice_config = self.client_voice_config.clone();
        copy.client_voice_worker = self.client_voice_worker.clone();
        copy.remote_gateway = self.remote_gateway;
        copy.remote_host = self.remote_host.clone();
        copy.connection_generation = self.connection_generation;
        copy.opened = self.opened.clone();
        copy.profile = self.profile.clone();
        copy.fleet_enabled = self.fleet_enabled;
        copy.model = self.model.clone();
        copy.effort = self.effort.clone();
        copy.effort_wire = self.effort_wire.clone();
        copy.hermes_home = self.hermes_home.clone();
        copy.launch_folder = self.launch_folder.clone();
        copy.launch = self.launch.clone();
        copy.sender = self.sender.clone();
        copy.link_epoch = Arc::clone(&self.link_epoch);
        copy.connectors = self.connectors.clone();
        copy.connectors_open = self.connectors_open;
        copy.connectors_consent = self.connectors_consent.clone();
        copy
    }

    /// Record that this launch is attached through a remote gateway dial.
    ///
    /// Call this before a session opens. Gateway voice records and plays on
    /// the gateway machine, so a remote attachment must refuse it.
    pub fn attached_to_remote_gateway(&mut self) {
        self.remote_gateway = true;
    }

    /// Select the configured backend for a newly started application.
    pub fn configure_voice(&mut self, config: crate::config::VoiceConfig) {
        self.voice
            .configure(config.read_aloud, config.speak_max_chars);
        self.client_voice_config = config;
    }

    pub(crate) fn voice_detaching(&mut self, session_id: &str) {
        if self.client_voice_config.backend == "client-bridge" {
            let _ = self
                .client_voice
                .update(talaria_core::voice::client::Input::Cutoff(
                    talaria_core::voice::client::Cutoff::TargetChanged,
                ));
            return;
        }
        if let Some(sender) = &self.sender {
            let _ = sender.send(AppMessage::Voice {
                session_id: session_id.to_owned(),
                generation: self.connection_generation,
                input: talaria_core::voice::VoiceInput::DetachBegins,
            });
        }
    }

    pub(crate) async fn close_client_voice(&self) {
        if let Some(worker) = &self.client_voice_worker {
            worker.close().await;
        }
    }

    async fn close_client_voice_jobs(&mut self) {
        if let Some(worker) = &self.client_voice_worker {
            self.client_voice_closing.push(tokio::spawn(worker.close()));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        for mut task in self.client_voice_closing.drain(..) {
            if tokio::time::timeout_at(deadline, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }

    pub(crate) fn uses_client_voice(&self) -> bool {
        self.client_voice_config.backend == "client-bridge"
    }

    pub(crate) fn voice_warning(&self, session_id: &str, warning: String) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(AppMessage::VoiceNotice {
                session_id: session_id.to_owned(),
                generation: self.connection_generation,
                text: warning.trim_end().to_owned(),
            });
        }
        self.voice.keep_warning(warning);
    }

    /// What a finished switch or a finished branch leaves behind: the session that is in
    /// front, the service it was opened on and the new link, when the profile changed.
    #[must_use]
    pub fn into_install(mut self) -> Option<SwitchInstall> {
        let opened = self.opened.take()?;
        let service = self.service.clone()?;
        let link = self.link.take();
        Some(SwitchInstall {
            opened,
            service,
            link,
        })
    }

    /// Install the foreground session's service and link without the start-up task: for
    /// callers that open the session themselves, such as tests of the fleet overlay.
    #[doc(hidden)]
    pub fn set_session_service(&mut self, service: SessionService, link: Link) {
        self.service = Some(service);
        self.link = Some(link);
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
    /// Super was held.
    pub super_key: bool,
    /// Hyper or meta was held.
    pub other: bool,
}

impl InputKey {
    /// A character that can be inserted into the transcript search editor.
    #[must_use]
    pub fn search_character(self) -> Option<char> {
        match self.code {
            InputCode::Char(character)
                if !self.ctrl && !self.alt && !self.super_key && !self.other =>
            {
                Some(character)
            }
            _ => None,
        }
    }
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
    /// Tab.
    Tab,
    /// Forward delete.
    Delete,
    /// Insert.
    Insert,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
    /// A function key.
    F(u8),
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
    Kanban {
        board: String,
        input: talaria_core::kanban::KanbanInput,
    },
    KanbanPrepared {
        board: String,
        result: Result<crate::kanban::KanbanService, String>,
    },
    AcpChanged {
        key: talaria_core::fleet::acp::AcpSessionKey,
        view: talaria_core::fleet::acp::AcpMapping,
    },
    AcpResult(crate::fleet::ExternalResult),
    VoiceFull {
        generation: u64,
        mode: Option<Option<String>>,
    },
    ClientVoice {
        session_id: String,
        generation: u64,
        input: talaria_core::voice::client::Input,
        notice: Option<String>,
    },
    ClientVoiceHealth {
        session_id: String,
        generation: u64,
        source: &'static str,
        ok: bool,
    },
    Voice {
        session_id: String,
        generation: u64,
        input: talaria_core::voice::VoiceInput,
    },
    VoiceNotice {
        session_id: String,
        generation: u64,
        text: String,
    },
    VoiceStatus {
        session_id: String,
        generation: u64,
        mode: Result<Option<String>, String>,
        requirements: Result<hermes_contract::schemas::VoiceToggleResult, (Option<i64>, String)>,
    },
    WorkflowDiscovered(talaria_workflow::runner::ProgressPicker),
    WorkflowOpened(Result<crate::workflow_progress::OpenedRun, String>),
    WorkflowScoped(PathBuf),
    WorkflowPolled {
        run: PathBuf,
        result: Result<crate::workflow_progress::PollBatch, String>,
    },
    /// A read of the journal of the run a workflow agent's session belongs to.
    WorkflowAgentRead {
        stored_id: String,
        result: Result<talaria_workflow::progress::agent_session::AgentSessionRead, String>,
    },
    WorkflowProgress {
        run: PathBuf,
        input: talaria_workflow::progress::live::ProgressInput,
    },
    HivecodeCatalog {
        request_id: u64,
        session: SessionId,
        profile: String,
        command: talaria_core::hivecode::HivecodeCommand,
        result: Result<talaria_core::commands::CommandCatalog, String>,
    },
    HivecodeDone {
        session: SessionId,
        original_text: String,
        restore_text: Option<String>,
        outcome: Result<Value, talaria_core::commands::ErrorView>,
    },
    HivecodeNotice {
        text: String,
    },
    /// The harmful Hermes settings found on a profile, possibly none.
    ConfigFindings {
        profile: String,
        findings: Vec<talaria_core::config_check::Finding>,
    },
    /// What a read of the local workflow setup found for a session.
    HivecodeSetup {
        session: SessionId,
        profile: String,
        facts: crate::hivecode::SetupFacts,
    },
    /// One read of the runs folder for the sessions that wait for a workflow run.
    HivecodeRuns {
        rows: Vec<talaria_workflow::runner::RunSummary>,
    },
    Configure {
        generation: u64,
        service: Option<Arc<crate::configure::Service>>,
        input: talaria_core::configure::ConfigureInput,
    },
    Capabilities {
        generation: u64,
        service: Option<Arc<crate::capabilities::CapabilitiesService>>,
        input: CapInput,
    },
    Memory {
        generation: u64,
        profile: String,
        service: Option<Arc<crate::memory::MemoryService>>,
        input: talaria_core::memory::MemoryInput,
    },
    Cron {
        tag: crate::cron::CronTag,
        input: CronInput,
    },
    CronPrepared {
        epoch: u64,
        profile: String,
        service: Arc<crate::cron::CronService>,
    },
    Connectors {
        generation: u64,
        profile: String,
        service: Option<Arc<crate::connectors::ConnectorsService>>,
        read: Option<(String, u64)>,
        input: talaria_core::connectors::ConnectorsInput,
    },
    Settings {
        generation: u64,
        profile: String,
        service: Option<Arc<crate::settings::SettingsService>>,
        input: talaria_core::settings::SettingsInput,
    },
    /// A completed doctor check, containing only scrubbed facts.
    DoctorFacts(DoctorFacts),
    /// A result or input for the independent gateway restart.
    DoctorRestart(RestartInput),
    /// A profiles transport reply or a scheduled profiles tick.
    Profiles(ProfilesInput),
    Providers {
        generation: u64,
        input: ProvidersInput,
    },
    ProviderReady {
        generation: u64,
        result: Result<Arc<crate::providers::ProviderService>, talaria_core::providers::Failure>,
    },
    /// A model roles reply, still owned by the page that asked.
    ModelRoles {
        generation: u64,
        profile: String,
        service: Option<Arc<crate::model_roles::ModelRolesService>>,
        input: talaria_core::model_roles::RolesInput,
    },
    Mcp {
        binding: u64,
        input: talaria_core::mcp::McpInput,
    },
    McpBound {
        binding: u64,
        result: Result<Arc<crate::mcp::McpService>, String>,
    },
    /// A background call's reply, still owned by the session that made it.
    BackgroundDone {
        session_id: SessionId,
        generation: u64,
        event: BackgroundEvent,
    },
    /// A replay-gap snapshot for the session that requested it.
    BackgroundRecovered {
        session_id: String,
        result: Result<OpenedSession, OpenError>,
    },
    Models(ModelsInput),
    /// A result for the fleet overlay.
    FleetView(talaria_core::fleet_view::FleetViewInput),
    /// The jump key named a row: open the fleet view on it.
    FleetOpenAt(RowKey),
    /// What the peek worker reports between results.
    Peek(crate::fleet_actions::PeekReport),
    /// A registry call the peek worker made, applied to the fleet service here.
    Registry(RegistryCall),
    /// A background launch's progress or result.
    Launch(crate::bg::LaunchNotice),
    /// One second passed for the launch the dialog shows.
    LaunchTick(u64),
    /// A review effect's result.
    Review(talaria_core::review::ReviewInput),
    /// A notice from a review action that has no state input, such as the browser failing.
    ReviewNotice(String),
    /// A review's folder was looked up.
    ReviewLocated {
        located: crate::review::Located,
        origin: ReviewOrigin,
        info: Option<Result<crate::review::SessionInfoReply, crate::review::SessionInfoError>>,
    },
    /// Pull-request labels changed.
    PrStatus(crate::review::PrStatusNotice),
    /// A fleet snapshot or attention. Stored; nothing is drawn for it.
    Fleet(crate::fleet::FleetNotice),
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
    /// An automation service reply. Issue #236.
    Automation(AutomationEvent),
    /// A slash command finished; `typed` is the line as it was run.
    CommandDone {
        typed: String,
        outcome: CommandOutcome,
    },
    /// A rewind input from the current session.
    Rewind(talaria_core::rewind::RewindInput),
    /// A rewind call's reply, bound to the session that issued it.
    RewindReply {
        owner: RowKey,
        input: talaria_core::rewind::RewindInput,
    },
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
    SideDone {
        key: RowKey,
        events: Vec<Event>,
    },
    /// A `/skin` command or a reconnect describe finished.
    AppearanceDone {
        /// The skin state it produced, when it changed one.
        skin: Option<AppearanceState>,
        /// Notice lines to show.
        lines: Vec<String>,
        /// The connection generation of a reconnect describe.
        generation: Option<u64>,
    },
    /// Work for the `/theme` or `/skin` selector finished.
    Choice(choice::ChoiceReply),
    /// A file read and whole-file validation completed off the application loop.
    ThemeFileDone {
        generation: u64,
        result: Result<(Box<appearance::ThemeFileSession>, Vec<String>), String>,
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
    /// One input for the session picker (#231).
    Picker(talaria_core::directory::state::PickerInput),
    /// A switch finished.
    Switched {
        outcome: SwitchOutcome,
        /// The new foreground session, when the switch opened one.
        install: Option<SwitchInstall>,
        /// The gateway's refusal reason, when it refused.
        reason: Option<String>,
    },
    /// A local session reset finished.
    SessionCleared(SessionCleared),
    /// A branch request came back; the loop settles it with the one ledger.
    BranchSent {
        request: MutationRequest,
        outcome: MutationOutcome,
    },
    /// A branch finished.
    Branched {
        outcome: BranchOutcome,
        /// The new foreground session, when the branch opened the child.
        install: Option<SwitchInstall>,
    },
    /// A mutation over the foreground link finished.
    Mutated {
        request: MutationRequest,
        outcome: MutationOutcome,
    },
    /// A rename over the side connection came back; the loop settles it with the one ledger.
    RowRenamed {
        key: RowKey,
        title: String,
        request: MutationRequest,
        /// The gateway's outcome, or the failure text when nothing could be sent.
        outcome: Result<MutationOutcome, String>,
    },
    /// An export finished.
    Exported {
        result: ExportResult,
        /// The format the export asked for.
        format: ExportFormat,
        /// Whether the export left the secrets in.
        redact: bool,
    },
    /// One of the subagent hub's own messages. Issue #242.
    Subagents(crate::subagents::SubagentMsg),
    /// A quiet-window or hold deadline has arrived.
    AttentionWake,
    /// Jump to the oldest background request.
    AttentionNext,
    /// A fleet-row pane open finished. Issue #246.
    PaneOpened(RowKey, PaneOutcome),
}

/// What a finished switch or branch brings back: the new session and, when the profile
/// changed, the new link and its service.
pub struct SwitchInstall {
    /// The session that is in front now.
    pub opened: OpenedSession,
    /// The service over the link the session was opened on.
    pub service: SessionService,
    /// The new link, when the switch crossed to another profile.
    pub link: Option<Link>,
}

/// Result of leaving the current session and opening a new one.
pub enum SessionCleared {
    DetachFailed {
        reason: String,
    },
    Started {
        install: Box<SwitchInstall>,
        resume_previous: bool,
    },
    OpenFailed {
        previous_id: String,
        reason: String,
        left: bool,
    },
    Skipped,
}

/// What the executor does after one update. Ranges index the open session's cells.
#[allow(clippy::large_enum_variant)]
pub enum AppEffect {
    PrepareKanban {
        board: String,
    },
    Kanban {
        board: String,
        effect: talaria_core::kanban::KanbanEffect,
    },
    Voice {
        session_id: String,
        effect: talaria_core::voice::VoiceEffect,
    },
    VoiceStatus {
        session_id: String,
    },
    ClientVoiceCapture {
        session_id: String,
        epoch: u64,
    },
    ClientVoiceFinish {
        epoch: u64,
    },
    ClientVoiceSpeak {
        session_id: String,
        text: String,
        audio_epoch: u64,
    },
    ClientVoiceStop {
        session_id: String,
        audio_epoch: u64,
    },
    ClientVoiceCancelCapture,
    ClientVoiceClose,
    WorkflowPrompt {
        run: PathBuf,
        key: String,
        attempt: u32,
        turn: u32,
        reference: talaria_workflow::progress::BlobRef,
    },
    WorkflowOpen(PathBuf),
    /// Read the journal of the run a workflow agent's session belongs to, off the update thread.
    WorkflowAgentRead {
        stored_id: String,
        request: crate::workflow_agent::JournalReadRequest,
    },
    WorkflowDiscover,
    WorkflowPoll {
        run: PathBuf,
        offset: u64,
        last_seq: u64,
    },
    WorkflowExecute {
        run: PathBuf,
        effects: Vec<talaria_workflow::progress::live::ProgressEffect>,
        keys: std::collections::HashMap<String, String>,
        at_ms: i64,
    },
    /// One line for `client.log` from the workflow view; it is never shown on screen.
    WorkflowLog(String),
    Configure {
        generation: u64,
        profile: String,
        home: PathBuf,
        client: Option<hermes_client::Client>,
        consent: crate::connectors::ConnectorsConsent,
        service: Option<Arc<crate::configure::Service>>,
        effects: Vec<talaria_core::configure::ConfigureEffect>,
    },
    Capabilities {
        generation: u64,
        home: PathBuf,
        service: Option<Arc<crate::capabilities::CapabilitiesService>>,
        socket: Option<hermes_client::Client>,
        effect: CapEffect,
    },
    Memory {
        generation: u64,
        profile: String,
        home: PathBuf,
        service: Option<Arc<crate::memory::MemoryService>>,
        effects: Vec<talaria_core::memory::MemoryEffect>,
    },
    CopyMemorySetup(String),
    /// Request a clipboard copy of a finished assistant reply.
    CopyReply {
        text: String,
    },
    Cron(CronEffect),
    CronNotice(String),
    CronFrame,
    PrepareCron {
        epoch: u64,
        generation: u64,
        profile: String,
        home: PathBuf,
        client: hermes_client::Client,
    },
    Connectors {
        generation: u64,
        profile: String,
        home: PathBuf,
        client: Option<hermes_client::Client>,
        service: Option<Arc<crate::connectors::ConnectorsService>>,
        consent: crate::connectors::ConnectorsConsent,
        effects: Vec<talaria_core::connectors::ConnectorsEffect>,
        reads: Vec<Option<(String, u64)>>,
    },
    ConnectorLocal(talaria_core::connectors::ConnectorsEffect),
    ConnectorWake {
        generation: u64,
        profile: String,
        at: Instant,
    },
    Settings {
        generation: u64,
        profile: String,
        home: PathBuf,
        service: Option<Arc<crate::settings::SettingsService>>,
        effect: talaria_core::settings::SettingsEffect,
    },
    /// Read a `/config` list that only slash completion on the session socket offers.
    SettingsChoices {
        generation: u64,
        profile: String,
        key: String,
        list: talaria_core::settings::RuntimeList,
        client: Option<hermes_client::Client>,
        session: Option<String>,
    },
    /// Read the profile's effective Hermes settings once and say which harmful ones are set.
    ConfigCheck {
        profile: String,
        home: PathBuf,
    },
    DoctorGather,
    DoctorActiveWork,
    DoctorPost {
        old_instance: String,
    },
    DoctorProbeAfter(Duration),
    /// Run one profiles call or manage its local timers and editor.
    Profiles(ProfilesEffect),
    Providers(ProvidersEffect),
    /// Model role calls, run together off the event loop.
    ModelRoles {
        generation: u64,
        profile: String,
        home: PathBuf,
        service: Option<Arc<crate::model_roles::ModelRolesService>>,
        effects: Vec<talaria_core::model_roles::RolesEffect>,
    },
    PrepareMcp,
    CloseMcp(Arc<crate::mcp::McpService>),
    Mcp(talaria_core::mcp::McpEffect),
    CommitSubagentBatch(talaria_core::subagents::FinishedBatch),
    /// A background call or log message for one session.
    Background {
        session_id: SessionId,
        effect: BackgroundEffect,
    },
    Models(ModelsEffect),
    /// One peek effect for the peek worker; its results come back as [`AppMessage::FleetView`].
    FleetPeek(talaria_core::fleet_view::FleetViewEffect),
    /// The foreground row's reply or stop, through the foreground's own paths.
    FleetForeground(ForegroundJob),
    /// An input for the overlay, known without a call.
    FleetInput(talaria_core::fleet_view::FleetViewInput),
    /// Answer the peek's card.
    FleetAnswer(crate::peek::CardAnswer),
    /// Answer a recorded row's card and return the outcome to its editor.
    FleetVoiceAnswer {
        key: RowKey,
        answer: crate::peek::CardAnswer,
    },
    /// Run one review effect on the review service.
    Review(talaria_core::review::ReviewEffect),
    /// Find a session's folder for a review, off the loop.
    ReviewLocate {
        row: RowKey,
        origin: ReviewOrigin,
    },
    /// Run one rewind call; its reply comes back as [`AppMessage::RewindReply`].
    Rewind {
        owner: RowKey,
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
    /// Save the line the slash palette ran: the composer did not record it, since Enter only
    /// told the palette to choose.
    RecordSubmit(String),
    /// `prompt.submit`.
    Send(Effect),
    /// `shell.exec` or `prompt.btw`, started at once: never queued behind a prompt or a turn.
    Side {
        key: RowKey,
        effect: Effect,
    },
    /// One queue call through the queue service. Issue #232.
    Queue(Effect),
    /// A prompt handed back by the queue, for an empty composer. Issue #232.
    OfferDraft(String),
    /// Clear the matching composer contents after a draft action is accepted.
    ClearSubmittedDraft(String),
    /// Feed this event once the reconcile wait has passed. Issue #232.
    ReconcileLater(Box<Event>),
    /// Deny open approvals, then `session.interrupt`.
    Interrupt(Effect),
    /// An approval or clarification answer, with the note to attach first.
    Respond {
        effect: Effect,
        note: Option<(PromptKey, String)>,
    },
    AnswerMasked {
        target: LiveMaskedTarget,
        value: Zeroizing<String>,
    },
    AnswerRecovered {
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
    /// Remove masked duplicates of the requests restored by a snapshot.
    InstallRecovered {
        ids: Vec<String>,
    },
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
    /// Load or save a theme choice away from the application reducer.
    ThemeFileCommand(String),
    /// Read the theme preferences for a profile just opened.
    RefreshThemeProfile,
    /// Start the next queued file command after adopting the preceding result.
    ContinueThemeFiles,
    /// Run `/skin`, with its name argument if any.
    Skin(Option<String>),
    /// Work for the `/theme` or `/skin` selector.
    Choice(choice::ChoiceWork),
    /// Describe again after a reconnect of this generation.
    Describe(u64),
    Quit,
    /// Read the workflow catalogue or run its skill command off the loop.
    Hivecode {
        action: crate::hivecode::Action,
        request_id: u64,
    },
    /// Read the local runs folder for runs that started at or after `since_ms`.
    HivecodeScan {
        since_ms: i64,
    },
    /// Merge changes into the per-session switch file; save number `generation`.
    HivecodeSave {
        generation: u64,
        changes: talaria_core::hivecode::SavedChanges,
    },
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
    /// Open the side connection and list the sessions.
    ListSessions {
        generation: u64,
    },
    /// Switch the foreground session to this row.
    Switch {
        key: RowKey,
    },
    /// Start a new session on another profile, then let go of the foreground session.
    StartInProfile {
        profile: String,
        cwd: PathBuf,
    },
    /// Open a new session after leaving the foreground session.
    ClearSession {
        skip_detach: bool,
        cwd: PathBuf,
    },
    /// Send a branch request the loop built with the one ledger.
    BranchSession {
        request: MutationRequest,
    },
    /// Open the child a committed branch made, then let go of the parent.
    OpenBranchChild {
        receipt: Value,
        /// The text typed after `/branch`, which Hermes took as the child's name.
        name: Option<String>,
    },
    /// Send a rename the loop built with the one ledger, over the side connection.
    RenameRow {
        key: RowKey,
        title: String,
        request: MutationRequest,
    },
    /// Export a row over the side connection.
    ExportSession {
        key: RowKey,
        format: ExportFormat,
        redact: bool,
        target: ExportTarget,
    },
    /// A mutation is sent over the foreground link.
    Mutate {
        request: MutationRequest,
        session_id: String,
    },
    /// Close a link a switch replaced.
    CloseLink(Link),
    /// Bind the services that hold a client to the link a switch installed.
    BindLinkServices,
    /// Close the side connection.
    CloseSide,
    /// Report the foreground pane state.
    ReportPane(talaria_core::attention::PaneReport),
    /// Report the foreground seat state.
    ReportSeat(SeatState),
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
    /// One automation reducer effect. Issue #236.
    Automation(AutomationEffect),
    /// Wake the attention hub at this instant.
    WakeAt(Instant),
    /// Open the fleet view on this row.
    OpenFleetViewAt(RowKey),
}

// Effect payloads can contain secrets; formatting exposes only the variant's discriminant.
impl std::fmt::Debug for AppEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppEffect")
            .field("kind", &std::mem::discriminant(self))
            .finish_non_exhaustive()
    }
}

pub(crate) fn notice_lines(text: &str) -> AppEffect {
    AppEffect::CommitLines(text.lines().map(|line| format!("\u{b7} {line}")).collect())
}

fn model_card_blocked(state: &AppState) -> bool {
    !state.masked.is_empty()
        || state.opened.as_ref().is_some_and(|s| {
            s.state
                .open_prompts()
                .any(|p| p.state == PromptState::Open && p.kind != PromptKind::ServerRequest)
        })
}

fn sync_models<S: Screen>(state: &mut AppState, screen: &mut S) {
    let (width, _) = screen.pane_size();
    state.profiles.set_key_labels(screen.profiles_key_labels());
    state.settings.set_reload_key(screen.settings_reload_key());
    let page_size = screen.profiles_page_size(&state.profiles.view());
    state.profiles.set_layout(width, page_size);
    if state.profiles.is_open() {
        screen.sync_providers(None);
    }
    screen.sync_profiles(
        state
            .profiles
            .is_open()
            .then(|| state.profiles.view())
            .as_ref(),
    );
    if state.profiles.is_open() {
        return;
    }
    state.models.set_list_height(screen.model_list_height());
    state
        .models
        .set_details_scroll_limit(screen.model_details_scroll_limit());
    screen.sync_models(
        state.models.view(),
        if model_card_blocked(state) {
            None
        } else {
            state.models.confirmation()
        },
    );
    screen.sync_providers(
        state
            .providers
            .is_open()
            .then(|| talaria_core::providers::view(&state.providers, Instant::now())),
    );
}

fn providers_input(state: &mut AppState, input: ProvidersInput) -> Vec<AppEffect> {
    if matches!(&input, ProvidersInput::Open { .. }) {
        state.providers_model_return = None;
    }
    if matches!(&input, ProvidersInput::Open { .. })
        && let Some(service) = &state.provider_service
    {
        service.open();
    }
    let mut effects: Vec<_> =
        talaria_core::providers::update(&mut state.providers, input, Instant::now())
            .into_iter()
            .map(AppEffect::Providers)
            .collect();
    if effects
        .iter()
        .any(|effect| matches!(effect, AppEffect::Providers(ProvidersEffect::Close)))
        && let Some(provider) = state.providers_model_return.take()
    {
        effects.extend(models_input(state, ModelsInput::ResumePicker { provider }));
    }
    effects.push(AppEffect::Redraw);
    effects
}

fn profiles_input(state: &mut AppState, input: ProfilesInput) -> Vec<AppEffect> {
    let is_tick = matches!(&input, ProfilesInput::Tick { .. });
    let was_open = state.profiles.is_open();
    if let ProfilesInput::Tick { token } = &input {
        state.profiles_ticks.remove(token);
    }
    state.profiles.set_connected_profile(&state.profile);
    state.profiles.set_remote_gateway(state.remote_gateway);
    let produced = state.profiles.update(input, state.now_ms);
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            ProfilesEffect::Notice(text) => effects.extend(reduce(
                state,
                Event::Notice {
                    kind: NoticeKind::Diagnostic,
                    text,
                },
            )),
            ProfilesEffect::OpenModelRoles { name } => {
                effects.extend(profiles_input(state, ProfilesInput::Closed));
                effects.extend(open_model_roles_for(state, name));
                state.model_roles_return_to_profiles = true;
            }
            ProfilesEffect::StartSession { name } => {
                effects.extend(profiles_input(state, ProfilesInput::Closed));
                effects.extend(begin_profile_start(state, name));
            }
            effect => effects.push(AppEffect::Profiles(effect)),
        }
    }
    if !is_tick && (was_open || state.profiles.is_open()) {
        effects.push(AppEffect::Redraw);
    }
    effects
}

fn cron_sync_profile(state: &mut AppState) {
    if state.cron.profile == state.profile {
        return;
    }
    let opening = state.cron.opening.take().or_else(|| {
        state
            .cron
            .state
            .as_ref()
            .filter(|cron| cron.is_open())
            .map(|_| CronOpen::List)
    });
    state.cron = CronBinding {
        profile: state.profile.clone(),
        generation: state.cron.generation.wrapping_add(1),
        epoch: state.cron.epoch.wrapping_add(1),
        opening,
        ..Default::default()
    };
}

fn cron_view(state: &AppState) -> Option<CronView> {
    if let Some(cron) = &state.cron.state {
        return cron
            .is_open()
            .then(|| cron::view(cron, state.wall_s as i64));
    }
    state.cron.opening.as_ref()?;
    Some(CronView {
        title: format!("Scheduled jobs · profile {}", plain(&state.profile)),
        banner: None,
        body: CronBody::Message(if state.link_state == LinkView::Connected {
            "Loading scheduled jobs…".into()
        } else {
            "Not connected to Hermes. Scheduled jobs need the gateway.".into()
        }),
        notice: None,
        question: None,
        hints: "esc close".into(),
    })
}

fn prepare_cron(state: &mut AppState, force: bool) -> Vec<AppEffect> {
    cron_sync_profile(state);
    if state.cron.opening.is_none() && state.cron.state.is_none() {
        return Vec::new();
    }
    if state.cron.initializing && !force {
        return Vec::new();
    }
    let Some(client) = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned()
    else {
        return Vec::new();
    };
    state.cron.initializing = true;
    state.cron.epoch = state.cron.epoch.wrapping_add(1);
    vec![AppEffect::PrepareCron {
        epoch: state.cron.epoch,
        generation: state.cron.generation,
        profile: state.profile.clone(),
        home: state.hermes_home.clone(),
        client,
    }]
}

fn cron_input(state: &mut AppState, input: CronInput) -> Vec<AppEffect> {
    cron_sync_profile(state);
    if matches!(input, CronInput::Close) {
        state.cron.deadline = None;
    }
    let retry_rest = matches!(
        input,
        CronInput::Key(CronKey::Action(cron::CronAction::Reload))
    ) && state.link_state == LinkView::Connected
        && state
            .cron
            .service
            .as_ref()
            .is_some_and(|service| service.rest_status().is_err());
    if let CronInput::Open(mode) = &input
        && state.cron.state.is_none()
    {
        state.cron.opening = Some(mode.clone());
        let mut effects = prepare_cron(state, false);
        effects.push(AppEffect::CronFrame);
        return effects;
    }
    let Some(cron) = &mut state.cron.state else {
        if matches!(input, CronInput::Close | CronInput::Key(CronKey::Esc)) {
            state.cron.opening = None;
        }
        return vec![AppEffect::CronFrame];
    };
    let mut effects = Vec::new();
    for effect in cron::update(cron, input, Instant::now(), state.wall_s as i64) {
        match effect {
            CronEffect::ScheduleTick { after } => {
                state.cron.deadline = Instant::now().checked_add(after);
            }
            CronEffect::CloseOverlay => state.cron.deadline = None,
            CronEffect::Notice(text) => effects.push(AppEffect::CronNotice(text)),
            CronEffect::OpenSession { session_id } => {
                effects.extend(cron_input(state, CronInput::Close));
                let switch = begin_switch(
                    state,
                    RowKey {
                        profile: state.profile.clone(),
                        session_id,
                    },
                );
                state.cron.switching_run =
                    switch.iter().any(|e| matches!(e, AppEffect::Switch { .. }));
                if state.cron.switching_run {
                    effects.extend(switch);
                } else if let Some(text) = last_line(&switch) {
                    effects.push(AppEffect::CronNotice(text));
                }
            }
            effect => effects.push(AppEffect::Cron(effect)),
        }
    }
    if retry_rest {
        effects.extend(prepare_cron(state, false));
    }
    effects.push(AppEffect::CronFrame);
    effects
}

fn open_cron(state: &mut AppState, mode: CronOpen) -> Vec<AppEffect> {
    cron_sync_profile(state);
    state.cron.first_frame_pending |= cron_view(state).is_none();
    cron_input(state, CronInput::Open(mode))
}

async fn make_cron_service(
    client: hermes_client::Client,
    home: PathBuf,
    profile: String,
    generation: u64,
) -> Arc<crate::cron::CronService> {
    use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..Default::default()
    };
    let rest = match hermes_client::discover_with(&home, &options).await {
        Ok(endpoint) => endpoint.resolve_profile(&profile).ok().and_then(|id| {
            let endpoint = Arc::new(endpoint);
            RestClient::new(
                endpoint.api_origin(),
                Arc::new(NativeHttpMinter::new(home, endpoint.clone(), id)),
                RestOptions::default(),
            )
            .ok()
        }),
        Err(_) => None,
    };
    Arc::new(crate::cron::CronService::new(
        client, rest, profile, generation,
    ))
}

fn cron_not_connected(effect: CronEffect) -> Option<CronInput> {
    let reason = "Not connected to Hermes.".to_owned();
    Some(match effect {
        CronEffect::List { seq } => CronInput::Listed {
            seq,
            result: Err(cron::CallFailure::NotSent(reason)),
        },
        CronEffect::Add(_) => CronInput::Added(Err(cron::CallFailure::NotSent(reason))),
        CronEffect::SetPaused { job_id, .. } => CronInput::PauseSet {
            job_id,
            result: Err(cron::CallFailure::NotSent(reason)),
        },
        CronEffect::Remove { job_id } => CronInput::Removed {
            job_id,
            result: Err(cron::CallFailure::NotSent(reason)),
        },
        CronEffect::Trigger { job_id, .. } => CronInput::Triggered {
            job_id,
            outcome: cron::TriggerOutcome::Failed(cron::RestFailure::NotSent(reason)),
        },
        CronEffect::Runs { job_id, seq } => CronInput::RunsLoaded {
            job_id,
            seq,
            result: Err(cron::RestFailure::NotSent(reason)),
        },
        CronEffect::DeliveryTargets => {
            CronInput::TargetsLoaded(Err(cron::RestFailure::NotSent(reason)))
        }
        _ => return None,
    })
}

pub(crate) fn models_input(state: &mut AppState, input: ModelsInput) -> Vec<AppEffect> {
    state.models.update(ModelsInput::Connection {
        connected: state.link_state == LinkView::Connected,
    });
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
            let level = value.clone().unwrap_or_else(|| "medium".into());
            // What Hermes sends on the route still holds while the level is the one it answered.
            if state.effort.as_deref() != Some(level.as_str()) {
                state.effort_wire = None;
            }
            state.effort = Some(level);
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
                state.effort_wire = payload
                    .get("reasoning_effort_wire")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
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
                state.effort_wire = receipt
                    .get("reasoning_effort_wire")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
            } else if receipt.get("effort").is_some() {
                state.effort = None;
                state.effort_wire = None;
            }
            if let Some(provider) = receipt.get("provider").and_then(Value::as_str)
                && let Some(opened) = &mut state.opened
            {
                opened.info.provider = Some(provider.into());
            }
        }
        _ => {}
    }
    let mut effects = Vec::new();
    for effect in state.models.update(input) {
        if let ModelsEffect::OpenProviders { slug } = effect {
            let provider = slug.clone();
            effects.extend(providers_input(
                state,
                ProvidersInput::Open {
                    target: talaria_core::providers::OpenTarget::Provider { slug },
                    profile_name: state.profile.clone(),
                    connected: state.link_state == LinkView::Connected,
                },
            ));
            state.providers_model_return = Some(provider);
        } else {
            effects.push(AppEffect::Models(effect));
        }
    }
    choice::refresh_effort(state);
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

fn bind_rewind_session(state: &mut AppState, owner: Option<RowKey>) {
    if state.rewind_owner != owner {
        state.rewind.change_session();
        state.rewind_owner = owner;
    }
}

/// Apply one rewind input and carry out what it asks for.
fn rewind_input(state: &mut AppState, input: RewindInput) -> Vec<AppEffect> {
    let owner = state.opened.as_ref().map(|opened| RowKey {
        profile: state.profile.clone(),
        session_id: opened.session_id.clone(),
    });
    bind_rewind_session(state, owner);
    let mut effects = Vec::new();
    for effect in rewind_update(&mut state.rewind, input, wall_ms()) {
        match effect {
            RewindEffect::Call { ticket, call } => {
                if let Some(owner) = state.rewind_owner.clone() {
                    effects.push(AppEffect::Rewind {
                        owner,
                        ticket,
                        call,
                    });
                }
            }
            RewindEffect::Rebuild { snapshot, notice } => effects.extend(rebuild_foreground(
                state,
                snapshot,
                RebuildMode::AfterRewind { notice },
            )),
            RewindEffect::ReturnPrompt { text, attachments } => {
                // A routed prompt comes back as typed; sending it routes it again.
                let text = talaria_core::hivecode::shown_text(&text).to_owned();
                effects.extend(return_prompt(state, text, attachments));
            }
            RewindEffect::Submit(text) => {
                let submission_id = state.next_submission_id();
                if let Some(owner) = state.rewind_owner.clone() {
                    state
                        .retry
                        .insert(submission_id.clone(), (owner, text.clone()));
                }
                effects.extend(reduce(
                    state,
                    Event::Submit {
                        display: talaria_core::hivecode::shown_text(&text).to_owned(),
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

fn open_doctor(state: &mut AppState) -> Vec<AppEffect> {
    if matches!(state.doctor_restart, RestartState::Done(_)) {
        state.doctor_restart = RestartState::Idle;
    }
    state.doctor = Some(DoctorView::new());
    vec![AppEffect::DoctorGather, AppEffect::Redraw]
}

fn doctor_key(state: &mut AppState, key: DoctorKey) -> Vec<AppEffect> {
    let Some(view) = state.doctor.as_mut() else {
        return Vec::new();
    };
    let previous = view.clone();
    let produced = doctor_view_update(view, &state.doctor_restart, key);
    let mut effects = Vec::new();
    if *view != previous {
        effects.push(AppEffect::Redraw);
    }
    for effect in produced {
        match effect {
            DoctorEffect::Gather => {
                if let Some(view) = state.doctor.as_mut() {
                    view.running = true;
                }
                if matches!(state.doctor_restart, RestartState::Done(_)) {
                    state.doctor_restart = RestartState::Idle;
                }
                effects.push(AppEffect::DoctorGather);
                effects.push(AppEffect::Redraw);
            }
            DoctorEffect::AskRestart => {
                if let Some(facts) = state.doctor.as_ref().and_then(|view| view.facts.as_ref()) {
                    let input = ask_input(facts);
                    effects.extend(doctor_restart_input(state, input));
                }
            }
            DoctorEffect::Restart(input) => effects.extend(doctor_restart_input(state, input)),
            DoctorEffect::Close => {
                state.doctor = None;
                effects.push(AppEffect::Redraw);
            }
        }
    }
    effects
}

fn doctor_restart_input(state: &mut AppState, input: RestartInput) -> Vec<AppEffect> {
    let previous = std::mem::replace(&mut state.doctor_restart, RestartState::Idle);
    let (next, produced) = restart_update(previous.clone(), input);
    let changed = match (&previous, &next) {
        (
            RestartState::Waiting {
                elapsed: before, ..
            },
            RestartState::Waiting { elapsed: after, .. },
        ) => before.as_secs() != after.as_secs(),
        _ => previous != next,
    };
    state.doctor_restart = next;
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            RestartEffect::QueryActiveWork => effects.push(AppEffect::DoctorActiveWork),
            RestartEffect::Post { old_instance } => {
                effects.push(AppEffect::DoctorPost { old_instance });
            }
            RestartEffect::ProbeAfter(delay) => effects.push(AppEffect::DoctorProbeAfter(delay)),
            RestartEffect::RerunChecks if state.doctor.is_some() => {
                if let Some(view) = state.doctor.as_mut() {
                    view.running = true;
                }
                effects.push(AppEffect::DoctorGather);
            }
            RestartEffect::Notice(text) if state.doctor.is_none() => {
                let kind = if matches!(
                    state.doctor_restart,
                    RestartState::Done(RestartOutcome::Restarted { .. })
                ) {
                    NoticeKind::System
                } else {
                    NoticeKind::Error
                };
                let text = format!("Doctor: {text}");
                if state.opened.is_some() {
                    effects.extend(reduce(state, Event::Notice { kind, text }));
                } else {
                    effects.push(notice_lines(&text));
                }
            }
            RestartEffect::RerunChecks | RestartEffect::Notice(_) => {}
        }
    }
    if changed && state.doctor.is_some() {
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// Route a global connectors event while the overlay is open. A closed overlay ignores it.
fn connectors_on_message(state: &mut AppState, message: &AppMessage) -> Vec<AppEffect> {
    if !state.connectors_open {
        return Vec::new();
    }
    let input = match message {
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(event),
            ..
        }) => match event.frame.type_name() {
            Some("connection.update") => {
                let payload = event.raw.get("payload");
                let owner = payload
                    .and_then(|value| value.get("owner"))
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str);
                let op_id = payload
                    .and_then(|value| value.get("op_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                Some(
                    talaria_core::connectors::ConnectorsInput::ConnectionUpdate {
                        op_id,
                        owner_is_account: owner == Some("account"),
                    },
                )
            }
            Some("platforms.changed") => {
                Some(talaria_core::connectors::ConnectorsInput::PlatformsChanged)
            }
            Some("pairing.changed") => {
                Some(talaria_core::connectors::ConnectorsInput::PairingChanged)
            }
            _ => None,
        },
        AppMessage::Link(LinkEvent::State(LinkState::Connected { generation }))
            if *generation >= 2 =>
        {
            state.connectors_service = None;
            Some(talaria_core::connectors::ConnectorsInput::Reconnected)
        }
        _ => None,
    };
    input.map_or_else(Vec::new, |input| connectors_input(state, input))
}

fn connectors_command(
    state: &mut AppState,
    open: talaria_core::connectors::ConnectorsOpen,
    argument: &str,
) -> Vec<AppEffect> {
    let name = match open {
        talaria_core::connectors::ConnectorsOpen::Tab(
            talaria_core::connectors::ConnectorsTab::Messaging,
        ) => "messaging",
        talaria_core::connectors::ConnectorsOpen::Tab(_) => "connectors",
    };
    if !argument.is_empty() {
        return vec![notice_lines(&format!("Usage: /{name}"))];
    }
    if state.connectors.is_busy() {
        return vec![notice_lines("Waiting for Hermes.")];
    }
    state.connectors_generation = state.connectors_generation.wrapping_add(1);
    state.connectors_profile = Some(state.profile.clone());
    state.connectors_open = true;
    state.connectors_service = None;
    state.connectors_reads.clear();
    let input = talaria_core::connectors::ConnectorsInput::Open {
        open,
        profile: state.profile.clone(),
        consented: state.connectors_consent.granted(&state.profile),
    };
    connectors_input(state, input)
}

fn connectors_read_key(effect: &talaria_core::connectors::ConnectorsEffect) -> Option<String> {
    use talaria_core::connectors::ConnectorsEffect as E;
    Some(match effect {
        E::LoadIdentity => "identity".into(),
        E::ListConnectors => "list".into(),
        E::LoadCatalog => "catalog".into(),
        E::LoadAccounts => "accounts".into(),
        E::LoadPolicy => "policy".into(),
        E::LoadPlatforms => "platforms".into(),
        E::LoadPairing => "pairing".into(),
        E::LoadTools { slug, .. } => format!("tools:{slug}"),
        E::TestPlatform { id } => format!("test:{id}"),
        _ => return None,
    })
}

fn connectors_input(
    state: &mut AppState,
    input: talaria_core::connectors::ConnectorsInput,
) -> Vec<AppEffect> {
    use talaria_core::connectors::{ConnectorsEffect as E, ConnectorsInput as I};
    if state.connectors_profile.as_ref() != Some(&state.profile)
        || (!state.connectors_open && matches!(input, I::Key(_) | I::Paste(_)))
    {
        return Vec::new();
    }
    let now = Instant::now();
    let before = talaria_core::connectors::view(&state.connectors, now);
    let mut result = Vec::new();
    let mut remote = Vec::new();
    for effect in talaria_core::connectors::update(&mut state.connectors, input, now) {
        match effect {
            E::GrantConsent => state.connectors_consent.grant(&state.profile),
            E::Notice(text) => result.push(notice_lines(&text)),
            E::Close => state.connectors_open = false,
            effect @ (E::EditSecret { .. }
            | E::ForgetSecrets
            | E::OpenUrl(_)
            | E::CopyText(_)
            | E::OpenDoctor) => result.push(AppEffect::ConnectorLocal(effect)),
            effect => remote.push(effect),
        }
    }
    if !remote.is_empty() {
        // Reserve read versions before the next channel message can arrive. An older
        // response must not satisfy a save's newly requested read-back.
        let reads = remote
            .iter()
            .map(|effect| {
                let key = connectors_read_key(effect)?;
                let version = state.connectors_reads.entry(key.clone()).or_default();
                *version = version.wrapping_add(1);
                Some((key, *version))
            })
            .collect();
        result.push(AppEffect::Connectors {
            generation: state.connectors_generation,
            profile: state.profile.clone(),
            home: state.hermes_home.clone(),
            client: state
                .service
                .as_ref()
                .and_then(|service| service.client())
                .cloned(),
            service: state.connectors_service.clone(),
            consent: state.connectors_consent.clone(),
            effects: remote,
            reads,
        });
    }
    let next = talaria_core::connectors::next_deadline(&state.connectors);
    if next != state.connectors_deadline {
        state.connectors_deadline = next;
        if let Some(at) = next {
            result.push(AppEffect::ConnectorWake {
                generation: state.connectors_generation,
                profile: state.profile.clone(),
                at,
            });
        }
    }
    if !state.connectors_open && !state.connectors.is_busy() {
        state.connectors_profile = None;
        state.connectors_service = None;
        state.connectors_deadline = None;
        state.connectors_reads.clear();
        state.connectors_generation = state.connectors_generation.wrapping_add(1);
        state.connectors = talaria_core::connectors::ConnectorsState::new();
        result.push(AppEffect::ConnectorLocal(E::ForgetSecrets));
    }
    if before != talaria_core::connectors::view(&state.connectors, now) || !state.connectors_open {
        result.push(AppEffect::Redraw);
    }
    result
}

/// Apply one message. No input or output: the effects say what to do.
///
/// A session or profile switch, whichever path starts it, first closes an open `/theme` or
/// `/skin` selector and waits for any skin write in flight.
pub fn update(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let cards = choice::cards(state);
    let effects = update_message(state, message);
    let effects = choice::yield_to_card(state, cards.as_deref(), effects);
    choice::before_switch(state, effects)
}

#[inline(never)]
fn update_message(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    let queue_now_ms = match &message {
        AppMessage::Tick { now_ms } => *now_ms,
        _ => state.now_ms,
    };
    let queue_age_changed = state
        .opened
        .as_mut()
        .is_some_and(|opened| opened.state.queue.observe_time(queue_now_ms));
    let previous_capabilities_foreground = state
        .capabilities
        .as_ref()
        .and_then(|_| capabilities_foreground(state));
    cron_sync_profile(state);
    let connectors_effects = connectors_on_message(state, &message);
    let kanban_before = state.kanban.as_ref().map(|s| s.frame());
    let background_version = state
        .opened
        .as_ref()
        .map(|opened| opened.state.background().version());
    let mut snapshot = attention_snapshot(state);
    let previous_session = state
        .opened
        .as_ref()
        .map(|o| (o.session_id.clone(), o.title.clone()));
    let restored = matches!(&message, AppMessage::Opened { .. });
    let settled = mutation_session(&message);
    let mut redraw = match &message {
        AppMessage::WorkflowScoped(_) | AppMessage::WorkflowProgress { .. } | AppMessage::WorkflowPolled { .. } | AppMessage::WorkflowAgentRead { .. } | AppMessage::WorkflowOpened(_) | AppMessage::User(UserAction::Workflow(_)) | AppMessage::ForegroundKey(UserAction::Workflow(_)) | AppMessage::Cron { .. } | AppMessage::CronPrepared { .. }
        | AppMessage::User(UserAction::Cron(_))
        | AppMessage::ForegroundKey(UserAction::Cron(_)) => false,
        AppMessage::PromptDone { events, notices, masked } => {
            !notices.is_empty() || masked.is_some() || events.iter().any(|event| {
                !matches!(event, Event::Session { frame, .. } if frame.type_name() == Some("todo.updated"))
            })
        }
        AppMessage::Link(LinkEvent::Inbound { item: Inbound::Event(event), .. }) => {
            !event.raw.get("session_id").and_then(Value::as_str).is_some_and(|id| !is_open_session(state, id))
                && !matches!(event.frame.type_name(), Some("todo.updated" | "agent.terminal.output"))
        }
        AppMessage::Connectors { .. }
        | AppMessage::Tick { .. }
        | AppMessage::HivecodeRuns { .. }
        | AppMessage::BackgroundDone { .. }
        | AppMessage::DoctorFacts(_)
        | AppMessage::DoctorRestart(_)
        | AppMessage::User(UserAction::Doctor(_))
        | AppMessage::ForegroundKey(UserAction::Doctor(_))
        | AppMessage::Profiles(_)
        | AppMessage::User(UserAction::Profiles(_))
        | AppMessage::ForegroundKey(UserAction::Profiles(_)) => false,
        AppMessage::Input(key) if !state.pager_open && tasks_key(state, key) => false,
        // A disabled fleet view only commits its notice; nothing on the live screen changes.
        AppMessage::User(UserAction::Submit(text))
            if !state.fleet_enabled && text.trim() == "/fleet" =>
        {
            false
        }
        _ => true,
    };
    let mut effects = match message {
        AppMessage::Configure {
            generation,
            service,
            input,
        } => {
            if generation != state.configure_generation {
                if let Some(service) = service {
                    tokio::spawn(async move {
                        service.close().await;
                    });
                }
                return Vec::new();
            }
            if let Some(service) = service {
                state.configure_service = Some(service);
            }
            configure_input(state, input, None)
        }
        AppMessage::Connectors {
            generation,
            profile,
            service,
            read,
            input,
        } => {
            if generation != state.connectors_generation
                || profile != state.profile
                || state.connectors_profile.as_ref() != Some(&profile)
                || read
                    .as_ref()
                    .is_some_and(|(key, version)| state.connectors_reads.get(key) != Some(version))
            {
                return Vec::new();
            }
            if let Some(service) = service {
                state.connectors_service = Some(service);
            }
            connectors_input(state, input)
        }
        AppMessage::Capabilities {
            generation,
            service,
            input,
        } => {
            if generation != state.capabilities_generation || state.capabilities.is_none() {
                return Vec::new();
            }
            if let Some(service) = service {
                state.capabilities_service = Some(service);
            }
            capabilities_input(state, input)
        }
        AppMessage::Memory {
            generation,
            profile,
            service,
            input,
        } => {
            if generation != state.memory_generation
                || profile != state.profile
                || state.memory_profile.as_ref() != Some(&profile)
            {
                return Vec::new();
            }
            if let Some(service) = service {
                state.memory_service = Some(service);
            }
            memory_input(state, input)
        }
        AppMessage::Cron { tag, input } => {
            if tag.generation != state.cron.generation || tag.profile != state.profile {
                if let (Some(name), CronInput::Triggered { outcome, .. }) = (tag.run_name, input) {
                    vec![AppEffect::CronNotice(cron::stale_run_notice(
                        &name,
                        &tag.profile,
                        tag.profile == state.profile,
                        &outcome,
                    ))]
                } else {
                    Vec::new()
                }
            } else {
                cron_input(state, input)
            }
        }
        AppMessage::CronPrepared {
            epoch,
            profile,
            service,
        } => {
            if epoch != state.cron.epoch || profile != state.profile {
                return Vec::new();
            }
            state.cron.initializing = false;
            let availability = service.rest_status();
            state.cron.service = Some(service);
            if let Some(cron) = &mut state.cron.state {
                cron.set_rest_status(availability);
            } else {
                state.cron.state = Some(CronState::new(profile, availability));
            }
            let mut effects = cron_input(
                state,
                CronInput::Connection {
                    up: state.link_state == LinkView::Connected,
                },
            );
            if let Some(mode) = state.cron.opening.take() {
                effects.extend(cron_input(state, CronInput::Open(mode)));
            }
            effects
        }
        AppMessage::Settings {
            generation,
            profile,
            service,
            input,
        } => {
            if generation != state.settings_generation || profile != state.profile {
                return Vec::new();
            }
            if let Some(service) = service {
                state.settings_service = Some(service);
            }
            settings_input(state, input, None)
        }
        AppMessage::DoctorFacts(facts) => {
            if let Some(view) = state.doctor.as_mut() {
                view.report = Some(diagnose(&facts));
                view.facts = Some(facts);
                view.running = false;
                view.selected = view.selected.min(view.last_row());
                vec![AppEffect::Redraw]
            } else {
                Vec::new()
            }
        }
        AppMessage::DoctorRestart(input) => doctor_restart_input(state, input),
        AppMessage::Providers { generation, input } => {
            if generation != state.provider_generation {
                return Vec::new();
            }
            providers_input(state, input)
        }
        AppMessage::ProviderReady { generation, result } => {
            if generation != state.provider_generation {
                return Vec::new();
            }
            state.provider_initializing = false;
            let pending = std::mem::take(&mut state.provider_pending_loads);
            match result {
                Ok(service) => {
                    if state.providers.is_open() {
                        service.open();
                    }
                    state.provider_service = Some(service);
                    pending.into_iter().map(AppEffect::Providers).collect()
                }
                Err(reason) => {
                    let mut effects = Vec::new();
                    for effect in pending {
                        if let Some(input) = provider_failed(effect, reason.clone()) {
                            effects.extend(providers_input(state, input));
                        }
                    }
                    effects
                }
            }
        }
        AppMessage::ModelRoles {
            generation,
            profile,
            service,
            input,
        } => {
            if generation != state.model_roles_generation || profile != state.model_roles_profile {
                return Vec::new();
            }
            if let Some(service) = service {
                state.model_roles_service = Some(service);
            }
            model_roles_input(state, input)
        }
        AppMessage::BackgroundRecovered { session_id, result } => {
            if is_open_session(state, &session_id) {
                recovered(state, result)
            } else {
                recover_retained(state, &session_id, result)
            }
        }
        AppMessage::BackgroundDone {
            session_id,
            generation,
            event,
        } => {
            if generation < state.connection_generation {
                return Vec::new();
            }
            background_event(state, session_id.as_str(), event)
        }
        AppMessage::Models(input) => models_input(state, input),
        AppMessage::Profiles(input) => profiles_input(state, input),
        AppMessage::Mcp { binding, input } => {
            if binding != state.mcp_binding {
                return Vec::new();
            }
            mcp_input(state, input)
        }
        AppMessage::McpBound { binding, result } => {
            if binding != state.mcp_binding {
                return Vec::new();
            }
            state.mcp_preparing = false;
            match result {
                Ok(service) => {
                    service.reopen();
                    state.mcp_service = Some(service);
                    let mut effects: Vec<_> = std::mem::take(&mut state.mcp_waiting)
                        .into_iter()
                        .map(AppEffect::Mcp)
                        .collect();
                    let connected = state.link_state == LinkView::Connected;
                    effects.extend(mcp_input(
                        state,
                        talaria_core::mcp::McpInput::Connection { connected },
                    ));
                    effects
                }
                Err(reason) => {
                    state.mcp_waiting.clear();
                    let mut effects = mcp_input(
                        state,
                        talaria_core::mcp::McpInput::ListLoaded(Err(
                            talaria_core::mcp::Failure::NotSent(reason.clone()),
                        )),
                    );
                    effects.extend(mcp_input(
                        state,
                        talaria_core::mcp::McpInput::CatalogLoaded(Err(
                            talaria_core::mcp::Failure::NotSent(reason),
                        )),
                    ));
                    effects
                }
            }
        }
        AppMessage::AcpChanged { key, view } => {
            redraw = false;
            let row = RowKey {
                profile: format!("acp:{}", key.agent_id),
                session_id: key.session_id,
            };
            let status = talaria_core::fleet_view::PeekStatus::Open {
                has_open_prompt: !view.needs.is_empty(),
                running: matches!(
                    view.state,
                    talaria_core::fleet::RowState::Working
                        | talaria_core::fleet::RowState::NeedsYou
                ),
            };
            state.acp_views.insert(row.clone(), view);
            if state
                .fleet
                .as_ref()
                .and_then(|fleet| fleet.peek())
                .is_some_and(|(peeked, _)| *peeked == row)
            {
                let mut effects = fleet_input(
                    state,
                    talaria_core::fleet_view::FleetViewInput::Peek { key: row, status },
                );
                effects.push(AppEffect::Redraw);
                effects
            } else {
                Vec::new()
            }
        }
        AppMessage::AcpResult(result) => {
            redraw = false;
            use crate::fleet::ExternalResult;
            use talaria_core::fleet_view::{FleetViewInput, ReplyOutcome, StopOutcome};
            match result {
                ExternalResult::Launch(result) => {
                    state.launch_ui.external_finished(result);
                    vec![AppEffect::Redraw]
                }
                ExternalResult::Prompt { key, result } => {
                    state.client_voice_inflight.remove(&key);
                    if result.is_ok()
                        && state
                            .fleet
                            .as_ref()
                            .is_none_or(|view| !view.has_draft(&key))
                    {
                        state.client_voice_drafts.remove(&key);
                    }
                    let notice = result
                        .as_ref()
                        .err()
                        .map(|reason| format!("Agent refused the reply ({reason})."));
                    let mut effects = fleet_input(
                        state,
                        FleetViewInput::ReplyDone {
                            key,
                            outcome: result.map_or_else(
                                |reason| ReplyOutcome::Refused { reason },
                                |_| ReplyOutcome::Sent,
                            ),
                        },
                    );
                    if let Some(notice) = notice {
                        effects.extend(fleet_input(state, FleetViewInput::Notice(notice)));
                    }
                    effects
                }
                ExternalResult::Cancel { key, result } => {
                    let notice = result.as_ref().map_or_else(
                        |reason| format!("Agent did not stop the turn ({reason})."),
                        |_| "Agent turn stopped.".into(),
                    );
                    let mut effects = fleet_input(
                        state,
                        FleetViewInput::StopDone {
                            key,
                            outcome: result.map_or_else(
                                |reason| StopOutcome::Refused { reason },
                                |_| StopOutcome::Stopping,
                            ),
                        },
                    );
                    effects.extend(fleet_input(state, FleetViewInput::Notice(notice)));
                    effects
                }
                ExternalResult::Answer { key, result } => {
                    state.client_voice_inflight.remove(&key);
                    if state.client_voice_drafts.contains_key(&key) {
                        let outcome = match result {
                            Ok(()) => {
                                if state
                                    .fleet
                                    .as_ref()
                                    .is_none_or(|view| !view.has_draft(&key))
                                {
                                    state.client_voice_drafts.remove(&key);
                                }
                                ReplyOutcome::Sent
                            }
                            Err(reason) => ReplyOutcome::Refused { reason },
                        };
                        return fleet_input(state, FleetViewInput::ReplyDone { key, outcome });
                    }
                    let mut effects = fleet_input(state, FleetViewInput::AnswerFocusEnded);
                    if let Err(reason) = result {
                        effects.extend(fleet_input(state, FleetViewInput::Notice(reason)));
                    }
                    effects
                }
            }
        }
        // Stored for the fleet view, fed to the overlay while it is open, and read
        // by the attention hub for the counters, the alerts and the quiet window.
        AppMessage::Fleet(notice) => {
            redraw = false;
            let mut effects = Vec::new();
            // Notices from the handle and the fleet task can cross; keep the newest snapshot.
            if let crate::fleet::FleetNotice::Changed(snapshot) = &notice
                && state
                    .fleet_snapshot
                    .as_ref()
                    .is_none_or(|held| snapshot.version >= held.version)
            {
                state.fleet_clock = snapshot.as_of - state.now_ms as f64 / 1000.0;
                state.fleet_snapshot = Some(Arc::clone(snapshot));
                if state.fleet.is_some() {
                    effects.extend(fleet_input(state, fleet_snapshot_input(snapshot)));
                }
            }
            // The hub reads the same notice for the counters, the alerts and the quiet
            // window, whether or not the overlay is open.
            effects.extend(fleet_notice(state, &notice));
            effects
        }
        AppMessage::FleetView(input) => {
            redraw = false;
            if let talaria_core::fleet_view::FleetViewInput::ReplyDone { key, .. } = &input {
                state.client_voice_inflight.remove(key);
            }
            if let talaria_core::fleet_view::FleetViewInput::ReplyDone {
                key,
                outcome:
                    talaria_core::fleet_view::ReplyOutcome::Sent
                    | talaria_core::fleet_view::ReplyOutcome::Queued
                    | talaria_core::fleet_view::ReplyOutcome::AlreadyRan,
            } = &input
                && state.fleet.as_ref().is_none_or(|view| !view.has_draft(key))
            {
                state.client_voice_drafts.remove(key);
            }
            fleet_input(state, input)
        }
        AppMessage::FleetOpenAt(key) => {
            redraw = false;
            fleet_open_at(state, key)
        }
        AppMessage::Peek(report) => {
            redraw = false;
            peek_report(state, report)
        }
        AppMessage::Registry(call) => {
            redraw = false;
            if let Some(fleet) = &state.fleet_service {
                match call {
                    RegistryCall::Subscribed(key, view) => fleet.observe_subscribed(key, view),
                    RegistryCall::Unsubscribed(key) => fleet.observe_unsubscribed(key),
                    RegistryCall::Acknowledge(key) => fleet.acknowledge(key),
                    RegistryCall::Refresh => fleet.refresh_now(),
                }
            }
            Vec::new()
        }
        AppMessage::Launch(notice) => {
            redraw = false;
            let out = state.launch_ui.notice(notice);
            launch_effects(state, out)
        }
        AppMessage::LaunchTick(launch) => {
            redraw = false;
            if state.launch_ui.tick(launch) {
                vec![AppEffect::Redraw]
            } else {
                Vec::new()
            }
        }
        AppMessage::Review(input) => {
            redraw = false;
            review_input(state, input)
        }
        AppMessage::ReviewNotice(text) => {
            redraw = false;
            review_notice(state, text)
        }
        AppMessage::ReviewLocated {
            located,
            origin,
            info,
        } => {
            redraw = false;
            review_located(state, located, origin, info)
        }
        AppMessage::PrStatus(_) => {
            redraw = false;
            review_pr_changed(state)
        }
        // The toggle asks for its own redraw: one when a panel is there, none otherwise.
        AppMessage::User(UserAction::ToggleTasks)
        | AppMessage::ForegroundKey(UserAction::ToggleTasks) => {
            redraw = false;
            user_action(state, UserAction::ToggleTasks)
        }
        AppMessage::User(action) => user_action(state, action),
        AppMessage::ClientVoice {
            session_id,
            generation,
            input,
            notice,
        } => {
            if generation != state.connection_generation
                || !is_open_session(state, &session_id)
                || (state.switching
                    && !matches!(
                        input,
                        talaria_core::voice::client::Input::Detach
                            | talaria_core::voice::client::Input::Cutoff(
                                talaria_core::voice::client::Cutoff::TargetChanged
                            )
                    ))
            {
                Vec::new()
            } else {
                if matches!(
                    input,
                    talaria_core::voice::client::Input::CaptureFinished { .. }
                ) {
                    state.client_helper_status = "started";
                }
                if matches!(input, talaria_core::voice::client::Input::Text { .. }) {
                    state.client_relay_status = "answered";
                }
                if let Some(message) = &notice {
                    if message.contains("helper") {
                        state.client_helper_status = "failed";
                    }
                    if message.contains("relay") {
                        state.client_relay_status = "failed";
                    }
                }
                let mut effects = client_voice_input(state, &session_id, input);
                if let Some(notice) = notice {
                    effects.extend(voice_notice(state, &session_id, notice));
                }
                effects
            }
        }
        AppMessage::ClientVoiceHealth {
            session_id,
            generation,
            source,
            ok,
        } => {
            if generation == state.connection_generation && is_open_session(state, &session_id) {
                let result = if ok { "ready" } else { "failed" };
                match source {
                    "capture" => state.client_helper_status = result,
                    "playback" => state.client_playback_status = result,
                    "relay" => state.client_relay_status = result,
                    _ => {}
                }
            }
            Vec::new()
        }
        AppMessage::VoiceFull { generation, mode } => {
            if generation != state.connection_generation {
                Vec::new()
            } else {
                state.voice_config_pending = false;
                if let Some(mode) = mode {
                    state.voice_submit_mode = Some(mode.clone());
                    let mut effects = Vec::new();
                    for session in state.voice.session_ids() {
                        effects.extend(voice_input(
                            state,
                            &session,
                            talaria_core::voice::VoiceInput::SubmitModeRead(mode.clone()),
                        ));
                    }
                    effects
                } else {
                    Vec::new()
                }
            }
        }
        AppMessage::Voice {
            session_id,
            generation,
            input,
        } => {
            if generation == state.connection_generation {
                voice_input(state, &session_id, input)
            } else {
                Vec::new()
            }
        }
        AppMessage::VoiceNotice {
            session_id,
            generation,
            text,
        } => {
            if generation == state.connection_generation {
                voice_notice(state, &session_id, text)
            } else {
                Vec::new()
            }
        }
        AppMessage::VoiceStatus {
            session_id,
            generation,
            mode,
            requirements,
        } => {
            if generation != state.connection_generation {
                Vec::new()
            } else {
                voice_status(state, &session_id, mode, requirements)
            }
        }
        AppMessage::Link(event) => {
            if matches!(&event, LinkEvent::Inbound { item: Inbound::Event(event), .. }
                if event.frame.type_name() == Some("cron.changed"))
            {
                return cron_input(state, CronInput::Changed);
            }
            let mut effects = voice_link(state, &event);
            effects.extend(workflow_link(state, &event));
            effects.extend(skin_changed(state, &event));
            if is_session_less_skin(&event) {
                // A skin equal to the current one draws nothing.
                if effects.is_empty() {
                    return effects;
                }
                effects.push(AppEffect::Redraw);
                return effects;
            }
            let completed = match &event {
                LinkEvent::Inbound {
                    item: Inbound::Event(inbound),
                    ..
                } if inbound.raw.get("type").and_then(Value::as_str)
                    == Some("message.complete")
                    && inbound
                        .raw
                        .pointer("/payload/outcome")
                        .and_then(Value::as_str)
                        .map_or_else(
                            || {
                                inbound
                                    .raw
                                    .pointer("/payload/status")
                                    .and_then(Value::as_str)
                                    == Some("complete")
                            },
                            |outcome| outcome == "completed",
                        ) =>
                {
                    inbound
                        .raw
                        .get("session_id")
                        .and_then(Value::as_str)
                        .filter(|id| {
                            state.opened.as_ref().is_some_and(|opened| {
                                let (epoch, seq) = event_position(&inbound.raw);
                                opened.session_id == *id && accept_event(&opened.cursor, epoch, seq)
                            })
                        })
                        .map(str::to_owned)
                }
                _ => None,
            };
            effects.extend(link_event(state, event));
            if let Some(session) = completed {
                let cells = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.state.cells())
                    .unwrap_or_default();
                // A turn starts at the person's prompt or at the notice of a turn Hermes wrote.
                let last_user = cells.iter().rposition(Cell::starts_turn);
                if let Some(index) = last_user {
                    let assistant_markdown: Vec<String> = cells[index + 1..]
                        .iter()
                        .filter_map(|cell| match cell {
                            Cell::Assistant(cell) => Some(cell.text.clone()),
                            _ => None,
                        })
                        .collect();
                    let speech_mode = state
                        .client_speech_modes
                        .get(&(state.profile.clone(), session.clone()))
                        .copied()
                        .unwrap_or_else(|| {
                            if state.client_voice_config.render_mode == "digest" {
                                talaria_core::voice::SpeechMode::Summary
                            } else {
                                talaria_core::voice::SpeechMode::Full
                            }
                        });
                    let markdown = assistant_markdown.join("\n\n");
                    let spoken = match speech_mode {
                        talaria_core::voice::SpeechMode::Full => {
                            talaria_core::voice::speakable::speakable_text(
                                &markdown,
                                state.client_voice_config.speak_max_chars,
                            )
                        }
                        talaria_core::voice::SpeechMode::Summary => {
                            talaria_core::voice::speakable::digest_text(
                                &markdown,
                                state.client_voice_config.speak_max_chars,
                            )
                        }
                        talaria_core::voice::SpeechMode::Alerts => None,
                    };
                    if state.client_voice_config.backend == "client-bridge"
                        && state.client_voice_enabled
                        && state.client_voice_config.read_aloud
                        && crate::voice::transport::remote_audio_allowed(
                            state.remote_gateway,
                            state.client_voice_config.remote_audio,
                        )
                        && let Some(text) = spoken
                    {
                        let previous = state.client_voice.audio_epoch();
                        let _ = state
                            .client_voice
                            .update(talaria_core::voice::client::Input::SpeechStarted);
                        let audio_epoch = state.client_voice.audio_epoch();
                        if audio_epoch != previous {
                            effects.push(AppEffect::ClientVoiceSpeak {
                                session_id: session.clone(),
                                text,
                                audio_epoch,
                            });
                            effects.push(AppEffect::Redraw);
                        }
                    }
                    effects.extend(voice_input(
                        state,
                        &session,
                        talaria_core::voice::VoiceInput::TurnCompleted {
                            assistant_markdown,
                            outcome: talaria_core::voice::TurnOutcome::Completed,
                            focused: true,
                        },
                    ));
                }
            }
            effects
        }
        AppMessage::Input(key) => input_key(state, key),
        AppMessage::Opened { opened, notice } => {
            let mut effects = opened_session(state, opened, notice);
            effects.extend(prepare_cron(state, true));
            effects
        }
        AppMessage::SubmitDone {
            submission_id,
            outcome,
        } => {
            let mut effects = Vec::new();
            if let Some((_, key)) = state.fleet_fg_reply.take_if(|(id, _)| *id == submission_id) {
                let outcome = crate::fleet_actions::reply_outcome(&outcome, None);
                effects.extend(fleet_input(
                    state,
                    talaria_core::fleet_view::FleetViewInput::ReplyDone { key, outcome },
                ));
            }
            effects.extend(submit_done(state, submission_id, outcome));
            effects
        }
        AppMessage::InterruptDone { outcome, notices } => {
            let mut effects = Vec::new();
            if let Some(key) = state.fleet_fg_stop.take() {
                let outcome = crate::fleet_actions::stop_outcome(&outcome);
                effects.extend(fleet_input(
                    state,
                    talaria_core::fleet_view::FleetViewInput::StopDone { key, outcome },
                ));
            }
            effects.extend(interrupt_done(state, outcome, notices));
            effects.extend(
                std::mem::take(&mut state.background_after_interrupt)
                    .into_iter()
                    .map(|(session_id, effect)| AppEffect::Background { session_id, effect }),
            );
            effects
        }
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
                if let Event::Submit {
                    submission_id,
                    text,
                    ..
                } = &event
                {
                    state
                        .remembered_notes
                        .insert(submission_id.clone(), text.clone());
                }
                effects.extend(reduce(state, event));
            }
            effects
        }
        AppMessage::Kanban { board, input } => {
            if state.kanban.as_ref().is_some_and(|s| s.board == board) {
                kanban_input(state, input)
            } else {
                vec![]
            }
        }
        AppMessage::KanbanPrepared { board, result } => {
            if !state.kanban.as_ref().is_some_and(|s| s.board == board) {
                vec![]
            } else {
                match result {
                    Ok(service) => {
                        state.kanban_service = Some(service);
                        vec![AppEffect::Kanban {
                            board,
                            effect: talaria_core::kanban::KanbanEffect::Fetch {
                                workflow: true,
                                detail: None,
                            },
                        }]
                    }
                    Err(error) => {
                        kanban_input(state, talaria_core::kanban::KanbanInput::Failed(error))
                    }
                }
            }
        }
        AppMessage::WorkflowScoped(run) => {
            state.workflow_views.scoped.insert(run);
            vec![]
        }
        AppMessage::WorkflowProgress { run, input } => workflow_input(state, run, input),
        AppMessage::WorkflowDiscovered(picker) => {
            state.workflow_views.picker = Some(picker);
            vec![AppEffect::Redraw]
        }
        AppMessage::WorkflowOpened(result) => workflow_opened(state, result),
        AppMessage::WorkflowPolled { run, result } => workflow_polled(state, run, result),
        AppMessage::WorkflowAgentRead { stored_id, result } => {
            let changed = state
                .workflow_agent
                .as_mut()
                .filter(|agent| agent.stored_id() == stored_id)
                .is_some_and(|agent| agent.finish_read(result));
            if changed {
                vec![AppEffect::Redraw]
            } else {
                Vec::new()
            }
        }
        AppMessage::Recovered(result) => recovered(state, result),
        AppMessage::Rewind(input) => rewind_input(state, input),
        AppMessage::RewindReply { owner, input } => {
            if state.rewind_owner.as_ref() == Some(&owner)
                && state.profile == owner.profile
                && is_open_session(state, &owner.session_id)
            {
                rewind_input(state, input)
            } else {
                redraw = false;
                Vec::new()
            }
        }
        AppMessage::Subagents(message) => {
            let now = subagent_clock(state);
            let command_output = match &message {
                crate::subagents::SubagentMsg::CommandControl {
                    key,
                    generation,
                    outcome,
                } if state
                    .subagents
                    .as_ref()
                    .is_some_and(|hub| hub.current(key, *generation)) =>
                {
                    Some(crate::subagents::outcome_text(outcome))
                }
                _ => None,
            };
            redraw = state
                .subagents
                .as_mut()
                .is_some_and(|hub| hub.on_message(message, now));
            let mut effects = subagent_batches(state);
            if let Some(text) = command_output {
                effects.push(notice_lines(&text));
            }
            effects
        }
        AppMessage::ForegroundKey(action) => foreground_key(state, action),
        AppMessage::Commands(CommandReply::Commands(input)) => commands_input(state, input),
        AppMessage::Commands(CommandReply::Palette(input)) => {
            let (effects, _) = palette(state, input);
            effects
        }
        AppMessage::HivecodeCatalog {
            request_id,
            session,
            profile,
            command,
            result,
        } => {
            if state.hivecode_requests.get(&session).copied() != Some(request_id) {
                return Vec::new();
            }
            let actions = state
                .hivecode
                .on_catalog_reply(&session, &profile, command, result);
            if is_foreground(state, session.as_str()) {
                if actions.iter().any(|action| matches!(action, crate::hivecode::Action::Notice(text) if *text == talaria_core::hivecode::notice::offer(&profile))) {
                    state.hivecode.record_shown(&session);
                }
                let turned_on = command != talaria_core::hivecode::HivecodeCommand::Status
                    && state.hivecode.status_input(Some(&session));
                let mut effects = hivecode_actions(state, actions);
                if turned_on && let Some(check) = state.hivecode.setup_check(&session) {
                    effects.extend(apply_hivecode(state, vec![check], false));
                }
                effects
            } else {
                Vec::new()
            }
        }
        AppMessage::HivecodeDone {
            session,
            original_text,
            restore_text,
            outcome,
        } => {
            if is_foreground(state, session.as_str()) {
                let mut actions =
                    state
                        .hivecode
                        .on_command_outcome(&session, &original_text, outcome);
                if let Some(expanded) = restore_text
                    && let Some(crate::hivecode::Action::RestoreComposer(draft)) =
                        actions.iter_mut().find(|action| {
                            matches!(action, crate::hivecode::Action::RestoreComposer(_))
                        })
                {
                    *draft = expanded;
                }
                if actions.iter().any(|action| matches!(action, crate::hivecode::Action::Notice(text) if *text == talaria_core::hivecode::notice::offer(&state.profile))) {
                    state.hivecode.record_shown(&session);
                }
                hivecode_actions(state, actions)
            } else {
                vec![notice_lines(
                    "The workflow reply belongs to another session; nothing was sent.",
                )]
            }
        }
        AppMessage::HivecodeNotice { text } => vec![notice_lines(&text)],
        AppMessage::ConfigFindings { profile, findings } => {
            if profile != state.profile {
                return Vec::new();
            }
            findings
                .iter()
                .map(|finding| notice_lines(&finding.notice(&profile)))
                .collect()
        }
        AppMessage::HivecodeSetup {
            session,
            profile,
            facts,
        } => {
            // A session in the background hears about its setup when it is next in front.
            if !is_foreground(state, session.as_str()) {
                return Vec::new();
            }
            let actions = state.hivecode.on_setup(&session, &profile, facts);
            apply_hivecode(state, actions, false)
        }
        AppMessage::HivecodeRuns { rows } => hivecode_runs(state, &rows),
        AppMessage::CommandDone { typed, outcome } => command_done(state, &typed, outcome),
        AppMessage::ContextDone { block } => context_done(state, block),
        AppMessage::CompactDone {
            request,
            outcome,
            typed,
        } => compact_done(state, request, outcome, typed),
        AppMessage::Tick { now_ms } => {
            state.now_ms = now_ms;
            if let Some(opened) = state.opened.as_mut() {
                let notice = opened.state.notices().most_recent().cloned();
                let warnings = opened.state.notices().warning_count();
                opened.state.expire_notices(now_ms);
                redraw |= notice.as_ref() != opened.state.notices().most_recent()
                    || warnings != opened.state.notices().warning_count();
            }
            refresh_fleet_warning_count(state);
            let delta = Duration::from_millis(now_ms.saturating_sub(state.voice_last_tick_ms));
            state.voice_last_tick_ms = now_ms;
            let mut effects = settled_interrupt(state);
            for session in state.voice.session_ids() {
                effects.extend(voice_input(
                    state,
                    &session,
                    talaria_core::voice::VoiceInput::Tick(delta),
                ));
            }
            effects.extend(review_tick(state));
            effects.extend(workflow_tick(state));
            if let Some(agent) = state.workflow_agent.as_mut()
                && let Some(request) = agent.tick(now_ms)
            {
                effects.push(AppEffect::WorkflowAgentRead {
                    stored_id: agent.stored_id().to_owned(),
                    request,
                });
            }
            effects.extend(hivecode_scan(state));
            effects.extend(kanban_input(
                state,
                talaria_core::kanban::KanbanInput::Tick {
                    now_ms: (state.wall_s * 1000.0).max(0.0) as u64,
                },
            ));
            if state
                .configure_tick_at
                .is_some_and(|at| Instant::now() >= at)
            {
                effects.extend(configure_input(
                    state,
                    talaria_core::configure::ConfigureInput::Tick,
                    None,
                ));
            }

            if state.fleet.is_some() && state.fleet_clock > 0.0 {
                let now = state.fleet_clock + now_ms as f64 / 1000.0;
                effects.extend(fleet_input(
                    state,
                    talaria_core::fleet_view::FleetViewInput::Tick { now },
                ));
            }
            let wall_s = state.wall_s;
            let pending = state
                .opened
                .as_ref()
                .is_some_and(|opened| !opened.state.side().pending.is_empty());
            if pending {
                redraw = true;
                effects.extend(reduce(state, Event::SideClock { now_ms }));
            }
            if let Some(session_id) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            {
                effects.extend(background_event(
                    state,
                    &session_id,
                    BackgroundEvent::Clock { now_ms, wall_s },
                ));
            }
            effects
        }
        AppMessage::SideDone { key, events } => {
            let mut effects = Vec::new();
            if foreground_row(state).as_ref() == Some(&key) {
                for event in events {
                    effects.extend(reduce(state, event));
                }
            } else if let Some(opened) = state.retained_sessions.get_mut(&key) {
                for event in events {
                    talaria_core::session::update(&mut opened.state, event);
                }
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
        AppMessage::Choice(reply) => choice::reply(state, reply),
        AppMessage::ThemeFileDone { generation, result } => {
            if !state.appearance.finish_file_request(generation) {
                return Vec::new();
            }
            let mut effects = match result {
                Err(notice) => vec![notice_lines(&notice)],
                Ok((files, lines)) => {
                    let change = state.appearance.adopt_file_result(generation, *files);
                    let mut effects = Vec::new();
                    if !lines.is_empty() {
                        effects.push(notice_lines(&lines.join("\n")));
                    }
                    if change == Some(AppearanceChange::Changed) {
                        effects.push(AppEffect::Appearance);
                    }
                    effects
                }
            };
            effects.push(AppEffect::ContinueThemeFiles);
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
            if text.is_empty() && !state.draft.is_empty() {
                state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
            }
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
        AppMessage::Automation(event) => automation_event(state, event),
        AppMessage::Picker(input) => picker_input(state, input),
        AppMessage::SessionCleared(result) => match result {
            SessionCleared::DetachFailed { reason } => vec![notice_lines(&format!(
                "Could not leave the current session: {reason}. You are still in it."
            ))],
            SessionCleared::Started {
                install,
                resume_previous,
            } => {
                let notice = if resume_previous {
                    let previous = state
                        .opened
                        .as_ref()
                        .map(|opened| opened.session_id.as_str())
                        .unwrap_or_default();
                    let hint = crate::launch::exit_hint(previous, &state.profile);
                    format!(
                        "Started a new session in this folder. The previous session was not ended; resume it with {}",
                        hint.lines().nth(1).unwrap_or_default().trim()
                    )
                } else {
                    "Started a new session in this folder.".to_owned()
                };
                opened_session(state, install.opened, Some(notice))
            }
            SessionCleared::OpenFailed {
                previous_id,
                reason,
                left,
            } => {
                state.opened = None;
                state.workflow_agent = None;
                state.workflow_origin = None;
                state.workflow_return = None;
                let notice = if left {
                    let hint = crate::launch::exit_hint(&previous_id, &state.profile);
                    format!(
                        "Left the previous session, but a new one could not be started: {reason}. Resume the previous session with {}",
                        hint.lines().nth(1).unwrap_or_default().trim()
                    )
                } else {
                    format!(
                        "The previous session had ended, and a new one could not be started: {reason}."
                    )
                };
                vec![notice_lines(&notice)]
            }
            SessionCleared::Skipped => Vec::new(),
        },
        AppMessage::Switched {
            outcome,
            install,
            reason,
        } => {
            if std::mem::take(&mut state.cron.switching_run)
                && let SwitchOutcome::Failed { text } = &outcome
            {
                state.switching = false;
                state.session_action = false;
                state.pending_switch = None;
                let mut effects = vec![AppEffect::CronNotice(text.clone())];
                for event in std::mem::take(&mut state.pending_switch_events) {
                    effects.extend(link_event(
                        state,
                        LinkEvent::Inbound {
                            item: Inbound::Event(event),
                            replayed: false,
                        },
                    ));
                }
                return effects;
            }
            switched(state, outcome, install, reason)
        }
        AppMessage::BranchSent { request, outcome } => branch_sent(state, request, outcome),
        AppMessage::Branched { outcome, install } => branched(state, outcome, install),
        AppMessage::Mutated { request, outcome } => mutated(state, request, outcome),
        AppMessage::RowRenamed {
            key,
            title,
            request,
            outcome,
        } => row_renamed(state, key, title, request, outcome),
        AppMessage::Exported {
            result,
            format,
            redact,
        } => exported(state, result, format, redact),
        AppMessage::AttentionWake => {
            // A wake only decides what was waiting; it draws nothing by itself.
            redraw = false;
            attention_wake(state)
        }
        AppMessage::AttentionNext => attention_next(state),
        AppMessage::PaneOpened(key, outcome) => {
            let title = state
                .pending_pane
                .take()
                .filter(|(pending, _)| pending == &key)
                .map(|(_, title)| title)
                .or_else(|| {
                    state
                        .fleet
                        .as_ref()
                        .and_then(|view| view.find_row(&key))
                        .map(talaria_core::fleet_view::title_or_untitled)
                })
                .unwrap_or_else(|| "(untitled)".to_owned());
            if matches!(outcome, PaneOutcome::Opened { .. }) {
                if let Some(sink) = &state.pane_sink {
                    sink.acknowledge(key);
                } else if let Some(fleet) = &state.fleet_service {
                    fleet.acknowledge(key);
                }
            }
            let text = crate::panes::outcome_text(&title, &outcome);
            pane_notice(state, text)
        }
    };
    if let Some(session_id) = settled {
        share_fences(state, &session_id);
    }
    if state.capabilities.is_some() {
        let foreground = capabilities_foreground(state);
        if foreground != previous_capabilities_foreground {
            effects.extend(capabilities_input(
                state,
                CapInput::SetForeground(foreground),
            ));
        }
    }
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
            if state.seat_reporting {
                effects.push(AppEffect::ReportSeat(state.attention.seat_state()));
            }
        }
    }
    let foreground = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone());
    if previous_session.as_ref().map(|(id, _)| id) != foreground.as_ref() {
        effects.extend(mcp_input(
            state,
            talaria_core::mcp::McpInput::Foreground(foreground),
        ));
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
        if state.seat_reporting
            && let Some(seat) = out.seat
        {
            effects.push(AppEffect::ReportSeat(seat));
        }
        if let Some(alert) = out.alert
            // A foreground alert shares the gate with the background ones, so two
            // sources never ring twice in a burst.
            && let Some(alert) = state.hub.on_foreground_alert(alert, state.attention_at)
        {
            state
                .attention
                .record_admitted(state.attention_at, alert.kind);
            effects.push(AppEffect::Alert(alert));
        }
    }

    // A first press of the interrupt key belongs to one running turn in one session.
    if !turn_running(state)
        || previous_session.as_ref().map(|(id, _)| id.as_str())
            != state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.as_str())
    {
        state.interrupt_confirm.reset();
    }
    redraw |= queue_age_changed;
    if let Some(opened) = state.opened.as_mut() {
        // Newly decoded snapshots first meet the application's clock here.
        redraw |= opened.state.queue.observe_time(state.now_ms);
    }
    // A prompt the queue handed back goes to an empty composer; the rest stay as notices.
    if let Some(text) = state
        .opened
        .as_mut()
        .and_then(|opened| opened.state.queue.take_returned().into_iter().next())
    {
        effects.push(AppEffect::OfferDraft(text));
    }
    // A prompt that carried the whole skill never started: the next routed prompt sends it.
    if let Some(opened) = state.opened.as_mut()
        && opened.state.queue.take_skill_dropped()
    {
        let session = SessionId::new(opened.session_id.clone());
        state.hivecode.rearm(&session);
    }
    // A prompt that named the session model never started: the next routed prompt names it.
    if let Some(opened) = state.opened.as_mut()
        && opened.state.queue.take_model_dropped()
    {
        let session = SessionId::new(opened.session_id.clone());
        state.hivecode.forget_model(&session);
    }
    effects.extend(connectors_effects);
    effects.extend(commit_ready(state));
    if !turn_running(state) {
        for block in state.context_blocks.release() {
            effects.push(AppEffect::CommitContext(block));
        }
    }
    effects.extend(checklist_effects(state));
    effects.extend(authoring_effects(state));
    redraw |= kanban_before != state.kanban.as_ref().map(|s| s.frame());
    redraw |= background_version
        != state
            .opened
            .as_ref()
            .map(|opened| opened.state.background().version());
    // The status line's counters changed: the line has to be drawn again.
    let foreground_open = foreground_open(state);
    let counts = status_counts(
        foreground_open,
        state
            .fleet_snapshot
            .as_ref()
            .map(|snapshot| snapshot.as_ref()),
    );
    if counts != state.status_counts || foreground_open != state.foreground_open {
        state.foreground_open = foreground_open;
        state.status_counts = counts;
        redraw = true;
    }
    if redraw
        && !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
    {
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// One fleet notice: the alerts it raises, the notice it shows, and the next wake.
fn fleet_notice(state: &mut AppState, notice: &crate::fleet::FleetNotice) -> Vec<AppEffect> {
    if let crate::fleet::FleetNotice::Changed(snapshot) = notice
        && let SideScope::ProfileOnly { profile, .. } = &snapshot.scope
    {
        // The profile whose rows need no prefix in a label.
        state.hub.set_foreground_profile(profile.clone());
    }
    let empty = crate::fleet_attention::empty_snapshot();
    let latest = state
        .fleet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.as_ref())
        .unwrap_or(&empty);
    let out = state.hub.on_notice(notice, latest, state.attention_at);
    hub_effects(state, out)
}

/// The quiet window or a hold deadline arrived.
fn attention_wake(state: &mut AppState) -> Vec<AppEffect> {
    let at = state.fleet_wake_at.take().unwrap_or(state.attention_at);
    let empty = crate::fleet_attention::empty_snapshot();
    let latest = state
        .fleet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.as_ref())
        .unwrap_or(&empty);
    let out = state.hub.on_wake(at, Some(latest));
    hub_effects(state, out)
}

/// The jump key: open the fleet view on the oldest background request, or say why not.
fn attention_next(state: &mut AppState) -> Vec<AppEffect> {
    let snapshot = state
        .fleet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.as_ref());
    match next_need(snapshot, state.fleet_enabled) {
        NextNeed::Peek(key) => vec![AppEffect::OpenFleetViewAt(key)],
        NextNeed::Notice(text) => vec![notice_lines(&text)],
    }
}

/// The effects of one hub output: the alerts, the notice, and the next wake.
fn hub_effects(state: &mut AppState, out: HubOutput) -> Vec<AppEffect> {
    let mut effects: Vec<AppEffect> = out.alerts.into_iter().map(AppEffect::Alert).collect();
    if let Some(text) = out.notice {
        effects.push(notice_lines(&text));
    }
    if out.wake_at != state.fleet_wake_at {
        state.fleet_wake_at = out.wake_at;
        if let Some(at) = out.wake_at {
            effects.push(AppEffect::WakeAt(at));
        }
    }
    effects
}

/// The foreground session's open requests.
fn foreground_open(state: &AppState) -> u32 {
    let open = state.opened.as_ref().map_or(0, |opened| {
        opened
            .state
            .open_prompts()
            .filter(|prompt| {
                prompt.kind != PromptKind::ServerRequest
                    || talaria_core::session::is_answerable(
                        prompt.command.as_deref().unwrap_or_default(),
                    )
            })
            .count()
            + usize::from(opened.state.connection_card().is_some())
    });
    u32::try_from(open).unwrap_or(u32::MAX)
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

/// Follow a routed turn that builds a workflow; when it ends, commit the one line it leaves.
fn authoring_effects(state: &mut AppState) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_ref() else {
        return Vec::new();
    };
    let session = SessionId::new(opened.session_id.clone());
    state
        .hivecode
        .observe_authoring(
            &session,
            opened.state.cells(),
            opened.state.turn(),
            state.now_ms,
        )
        .map(|line| vec![notice_lines(&line), AppEffect::Redraw])
        .unwrap_or_default()
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
        .filter(|command| {
            !matches!(
                command.action,
                LocalAction::Btw | LocalAction::Queue | LocalAction::Steer
            )
        })
        .map(|_| local::parse(trimmed))
}

/// The effect a local command asks for.
fn pane_notice(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if state.fleet.is_some() {
        fleet_input(
            state,
            talaria_core::fleet_view::FleetViewInput::Notice(text),
        )
    } else if let Some(hub) = state.subagents.as_mut()
        && hub.panel_key().is_some()
    {
        hub.set_flash(text);
        vec![AppEffect::Redraw]
    } else {
        vec![notice_lines(&text)]
    }
}

/// Open the model roles page for the connection's profile.
fn open_model_roles(state: &mut AppState) -> Vec<AppEffect> {
    let profile = state.profile.clone();
    open_model_roles_for(state, profile)
}

/// Open the model roles page for one served profile, which need not be the connected one.
fn open_model_roles_for(state: &mut AppState, profile: String) -> Vec<AppEffect> {
    let (mut page, effects) = talaria_core::model_roles::ModelRolesState::new(profile.clone());
    page.set_connection_profile(&state.profile);
    state.model_roles = Some(page);
    state.model_roles_profile = profile;
    state.model_roles_connection = state.profile.clone();
    state.model_roles_return_to_profiles = false;
    state.model_roles_service = None;
    state.model_roles_generation = state.model_roles_generation.wrapping_add(1);
    let mut result = model_roles_effects(state, effects);
    result.push(AppEffect::Redraw);
    result
}

/// Feed one input to the open page and queue the calls it asks for.
fn model_roles_input(
    state: &mut AppState,
    input: talaria_core::model_roles::RolesInput,
) -> Vec<AppEffect> {
    if state.model_roles_connection != state.profile {
        // The connection moved to another profile: the page was opened on the old one.
        close_model_roles(state);
        return vec![AppEffect::Redraw];
    }
    let Some(page) = state.model_roles.as_mut() else {
        return Vec::new();
    };
    let effects = page.update(input);
    let mut result = model_roles_effects(state, effects);
    result.push(AppEffect::Redraw);
    result
}

fn close_model_roles(state: &mut AppState) {
    state.model_roles = None;
    state.model_roles_return_to_profiles = false;
    state.model_roles_service = None;
    state.model_roles_generation = state.model_roles_generation.wrapping_add(1);
}

fn model_roles_effects(
    state: &mut AppState,
    effects: Vec<talaria_core::model_roles::RolesEffect>,
) -> Vec<AppEffect> {
    use talaria_core::model_roles::RolesEffect;
    if effects
        .iter()
        .any(|effect| matches!(effect, RolesEffect::Close))
    {
        let back_to_profiles = state.model_roles_return_to_profiles;
        let saved = state
            .model_roles
            .as_ref()
            .and_then(talaria_core::model_roles::ModelRolesState::saved_main_route);
        let name = state.model_roles_profile.clone();
        close_model_roles(state);
        let mut result = Vec::new();
        if let Some((provider, model)) = saved {
            // The list shows the saved model at once, before Hermes is read again.
            result.extend(profiles_input(
                state,
                ProfilesInput::ModelSaved {
                    name,
                    provider,
                    model,
                },
            ));
        }
        if back_to_profiles {
            result.extend(profiles_input(state, ProfilesInput::Open));
        }
        return result;
    }
    if effects.is_empty() {
        return Vec::new();
    }
    if effects
        .iter()
        .any(|effect| matches!(effect, RolesEffect::LoadAll))
    {
        // A reload builds the clients again, so a restarted gateway is found.
        state.model_roles_service = None;
    }
    vec![AppEffect::ModelRoles {
        generation: state.model_roles_generation,
        profile: state.model_roles_profile.clone(),
        home: state.hermes_home.clone(),
        service: state.model_roles_service.clone(),
        effects,
    }]
}

fn capabilities_foreground(state: &AppState) -> Option<CapSessionRef> {
    state.opened.as_ref().map(|opened| CapSessionRef {
        session_id: opened.session_id.clone(),
        profile: state.profile.clone(),
    })
}

fn capabilities_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    let tab = match argument.trim() {
        "" | "skills" => CapTab::Skills,
        "hub" => CapTab::Hub,
        "toolsets" => CapTab::Toolsets,
        "plugins" => CapTab::Plugins,
        _ => {
            return vec![notice_lines(
                "Use /capabilities skills, hub, toolsets or plugins.",
            )];
        }
    };
    if state.capabilities.is_none() {
        let mut capabilities =
            CapabilitiesState::new(state.profile.clone(), capabilities_foreground(state), tab);
        capabilities.set_display_paths(
            state.hermes_home.to_string_lossy().into_owned(),
            std::env::var("HOME").unwrap_or_default(),
        );
        state.capabilities = Some(capabilities);
        state.capabilities_service = None;
        state.capabilities_generation = state.capabilities_generation.wrapping_add(1);
        state.capabilities_started = Instant::now();
    }
    let foreground = capabilities_foreground(state);
    let mut effects = capabilities_input(state, CapInput::SetForeground(foreground));
    effects.extend(capabilities_input(state, CapInput::Open(tab)));
    effects
}

fn capabilities_input(state: &mut AppState, input: CapInput) -> Vec<AppEffect> {
    let Some(capabilities) = state.capabilities.as_mut() else {
        return Vec::new();
    };
    let was_open = capabilities.is_open();
    let now_ms = state
        .capabilities_started
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let produced = capabilities.update(input, now_ms);
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            CapEffect::Notice(text) => {
                let success = [
                    "Turned ",
                    "Installed ",
                    "Removed ",
                    "Plugin ",
                    "Installing ",
                    "Hermes is installing ",
                ]
                .iter()
                .any(|prefix| text.starts_with(prefix));
                if state.opened.is_some() {
                    effects.extend(reduce(
                        state,
                        Event::Notice {
                            kind: if success {
                                NoticeKind::Diagnostic
                            } else {
                                NoticeKind::Error
                            },
                            text,
                        },
                    ));
                } else if !success
                    || !state
                        .capabilities
                        .as_ref()
                        .is_some_and(CapabilitiesState::is_open)
                {
                    effects.push(notice_lines(&text));
                }
            }
            CapEffect::Dispose => {
                state.capabilities = None;
                state.capabilities_service = None;
                state.capabilities_generation = state.capabilities_generation.wrapping_add(1);
            }
            effect => effects.push(AppEffect::Capabilities {
                generation: state.capabilities_generation,
                home: state.hermes_home.clone(),
                service: state.capabilities_service.clone(),
                socket: state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned(),
                effect,
            }),
        }
    }
    if was_open
        || state
            .capabilities
            .as_ref()
            .is_some_and(CapabilitiesState::is_open)
    {
        effects.push(AppEffect::Redraw);
    }
    effects
}

async fn capabilities_service(
    home: PathBuf,
    socket: hermes_client::Client,
) -> Option<Arc<crate::capabilities::CapabilitiesService>> {
    use hermes_client::rest::{NativeHttpMinter, TicketMinter};
    use hermes_client::{DiscoverOptions, EnsurePolicy, discover_with};
    let endpoint = Arc::new(
        discover_with(
            &home,
            &DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        )
        .await
        .ok()?,
    );
    let origin = endpoint.api_origin().to_owned();
    let profiles = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    let hermes_home = home.to_string_lossy().into_owned();
    let factory = Arc::new(move |name: &str| -> Arc<dyn TicketMinter> {
        // The immutable served-profile list is checked before this factory is called.
        let profile = endpoint
            .resolve_profile(name)
            .expect("served profile was validated");
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile,
        ))
    });
    Some(Arc::new(crate::capabilities::CapabilitiesService::new(
        Some(origin),
        profiles,
        factory,
        socket,
        hermes_home,
        std::env::var("HOME").unwrap_or_default(),
    )))
}

fn capabilities_unavailable(effect: CapEffect) -> Vec<CapInput> {
    let error = CapError::NotConnected;
    let input = match effect {
        CapEffect::Fetch {
            tab, generation, ..
        } => CapInput::Loaded {
            tab,
            generation,
            result: Err(error),
        },
        CapEffect::HubBrowse { generation, .. } | CapEffect::HubSearch { generation, .. } => {
            CapInput::Loaded {
                tab: CapTab::Hub,
                generation,
                result: Err(error),
            }
        }
        CapEffect::ReadBack { op_id, .. } => CapInput::ReadBack {
            op_id,
            result: Err(error),
        },
        CapEffect::FetchDetail {
            tab, generation, ..
        } => CapInput::DetailLoaded {
            tab,
            generation,
            result: Err(error),
        },
        CapEffect::HubPreview { .. } => CapInput::PreviewDone(Err(error)),
        CapEffect::HubScan { .. } => CapInput::ScanDone(Err(error)),
        CapEffect::LookupPlugin { .. } => CapInput::LookupDone(Err(error)),
        CapEffect::ReloadSkills { .. } => CapInput::ReloadDone(Err(error)),
        CapEffect::FetchCatalog { generation, .. } => CapInput::CatalogLoaded {
            generation,
            result: Err(error),
        },
        CapEffect::ToggleSkill { op_id, .. }
        | CapEffect::ToggleToolset { op_id, .. }
        | CapEffect::TogglePlugin { op_id, .. }
        | CapEffect::InstallSkill { op_id, .. }
        | CapEffect::InstallPlugin { op_id, .. }
        | CapEffect::RemovePlugin { op_id, .. } => CapInput::WriteDone {
            op_id,
            result: Err(error),
        },
        _ => return Vec::new(),
    };
    vec![input]
}

#[cfg(test)]
mod capabilities_wiring_tests {
    use super::*;

    #[test]
    fn disposed_overlay_ignores_late_loads() {
        let mut state = AppState::new("capability-generation");
        capabilities_command(&mut state, "skills");
        let generation = state.capabilities_generation;
        capabilities_input(&mut state, CapInput::Close);
        assert!(state.capabilities.is_none());
        let effects = update(
            &mut state,
            AppMessage::Capabilities {
                generation,
                service: None,
                input: CapInput::Loaded {
                    tab: CapTab::Skills,
                    generation: 1,
                    result: Ok(talaria_core::capabilities::CapData::Skills(Vec::new())),
                },
            },
        );
        assert!(effects.is_empty());
        assert!(state.capabilities.is_none());
    }

    fn skill_change(state: &mut AppState) -> u64 {
        use talaria_core::capabilities::{CapData, KeyAction, Provenance, SkillRow};
        state.profile = "fixture".into();
        let effects = capabilities_command(state, "skills");
        let load_generation = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Capabilities {
                    effect:
                        CapEffect::Fetch {
                            generation,
                            tab: CapTab::Skills,
                            ..
                        },
                    ..
                } => Some(*generation),
                _ => None,
            })
            .expect("opening Skills schedules its load");
        capabilities_input(
            state,
            CapInput::Loaded {
                tab: CapTab::Skills,
                generation: load_generation,
                result: Ok(CapData::Skills(vec![SkillRow {
                    name: "fixture-skill".into(),
                    description: String::new(),
                    category: None,
                    enabled: true,
                    usage: 0,
                    provenance: Provenance::Bundled,
                }])),
            },
        );
        let effects = capabilities_input(state, CapInput::Key(KeyAction::Toggle));
        effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Capabilities {
                    effect: CapEffect::ToggleSkill { op_id, .. },
                    ..
                } => Some(*op_id),
                _ => None,
            })
            .expect("Space schedules the selected skill toggle")
    }

    #[test]
    fn refusal_remains_in_the_session_after_the_overlay_closes() {
        use talaria_core::capabilities::KeyAction;
        let snapshot =
            serde_json::from_str(include_str!("../tests/data/open_approval.json")).unwrap();
        let mut state = AppState::new("capability-refusal");
        state.opened = Some(OpenedSession::from_snapshot(snapshot));
        let op_id = skill_change(&mut state);
        capabilities_input(
            &mut state,
            CapInput::WriteDone {
                op_id,
                result: Err(CapError::Connect),
            },
        );
        let view = state.view().capabilities.unwrap();
        let message = view.message.join("\n");
        assert!(message.contains("could not reach the gateway"));
        capabilities_input(&mut state, CapInput::Key(KeyAction::Cancel));
        assert!(state.view().capabilities.is_none());
        assert!(
            state
                .view()
                .state
                .unwrap()
                .cells()
                .iter()
                .any(|cell| matches!(cell, Cell::Notice(notice) if notice.text == message))
        );
    }

    #[test]
    fn success_banner_is_recorded_in_the_session_while_the_overlay_is_open() {
        use talaria_core::capabilities::{CapData, KeyAction, Provenance, SkillRow};
        let snapshot =
            serde_json::from_str(include_str!("../tests/data/open_approval.json")).unwrap();
        let mut state = AppState::new("capability-success");
        state.opened = Some(OpenedSession::from_snapshot(snapshot));
        let op_id = skill_change(&mut state);
        capabilities_input(
            &mut state,
            CapInput::WriteDone {
                op_id,
                result: Ok(Default::default()),
            },
        );
        capabilities_input(
            &mut state,
            CapInput::ReadBack {
                op_id,
                result: Ok(CapData::Skills(vec![SkillRow {
                    name: "fixture-skill".into(),
                    description: String::new(),
                    category: None,
                    enabled: false,
                    usage: 0,
                    provenance: Provenance::Bundled,
                }])),
            },
        );
        let view = state.view();
        let message = view.capabilities.unwrap().message.join("\n");
        assert!(message.starts_with("Turned off"));
        assert!(view.state.unwrap().cells().iter().any(
            |cell| matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::Diagnostic && notice.text == message)
        ));
        capabilities_input(&mut state, CapInput::Key(KeyAction::Cancel));
        assert!(state.view().capabilities.is_none());
    }

    #[test]
    fn hidden_write_keeps_its_profile_and_reports_before_disposal() {
        let mut state = AppState::new("capability-hidden-write");
        let op_id = skill_change(&mut state);
        capabilities_input(&mut state, CapInput::Close);
        assert!(state.view().capabilities.is_none());
        let generation = state.capabilities_generation;
        state.profile = "other-profile".into();
        capabilities_command(&mut state, "skills");
        assert_eq!(state.capabilities_generation, generation);
        assert_eq!(state.view().capabilities.unwrap().profile, "fixture");
        assert!(state.capabilities.as_ref().unwrap().is_busy());
        capabilities_input(&mut state, CapInput::Close);
        let effects = capabilities_input(
            &mut state,
            CapInput::WriteDone {
                op_id,
                result: Err(CapError::Connect),
            },
        );
        assert!(effects.iter().any(|effect| matches!(effect, AppEffect::CommitLines(lines) if lines.iter().any(|line| line.contains("could not reach the gateway")))));
        assert!(state.capabilities.is_none());
    }
}

fn memory_input(state: &mut AppState, input: talaria_core::memory::MemoryInput) -> Vec<AppEffect> {
    use talaria_core::memory::{MemoryEffect, MemoryInput};
    if let MemoryInput::Open { profile } = &input {
        if state.memory.is_busy() {
            return vec![notice_lines("Waiting for Hermes."), AppEffect::Redraw];
        }
        state.memory_generation = state.memory_generation.wrapping_add(1);
        state.memory_visible = true;
        state.memory_profile = Some(profile.clone());
        state.memory_service = None;
    } else if state.memory_profile.as_ref() != Some(&state.profile)
        || (matches!(input, MemoryInput::Key(_)) && !state.memory_visible)
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut remote = Vec::new();
    for effect in state.memory.update(input, Instant::now()) {
        match effect {
            MemoryEffect::Notice(text) => result.push(notice_lines(&text)),
            MemoryEffect::CloseOverlay => {
                state.memory_visible = false;
            }
            MemoryEffect::OpenSettings { profile, filter } => result.extend(settings_input(
                state,
                talaria_core::settings::SettingsInput::Open {
                    profile,
                    filter: Some(filter.to_owned()),
                },
                None,
            )),
            MemoryEffect::CopyText(text) => result.push(AppEffect::CopyMemorySetup(text)),
            effect => remote.push(effect),
        }
    }
    if !remote.is_empty() {
        result.push(AppEffect::Memory {
            generation: state.memory_generation,
            profile: state.profile.clone(),
            home: state.hermes_home.clone(),
            service: state.memory_service.clone(),
            effects: remote,
        });
    }
    if !state.memory_visible && !state.memory.is_busy() {
        state.memory_profile = None;
        state.memory_service = None;
        state.memory_generation = state.memory_generation.wrapping_add(1);
        state.memory = talaria_core::memory::MemoryState::new();
    }
    result.push(AppEffect::Redraw);
    result
}

#[cfg(test)]
mod memory_wiring_tests {
    use super::*;
    use talaria_core::memory::{MemoryEffect, MemoryInput, MemoryKey};

    #[test]
    fn local_memory_command_opens_both_reads_before_replies() {
        let mut state = AppState::new("memory-open");
        let effects = local_effect(
            &mut state,
            LocalAction::MemorySettings,
            String::new(),
            "/memory-settings".into(),
        );
        assert!(state.view().memory.is_some());
        let reads = effects.iter().find_map(|effect| match effect {
            AppEffect::Memory { effects, .. } => Some(effects),
            _ => None,
        });
        assert!(matches!(
            reads.map(Vec::as_slice),
            Some([MemoryEffect::LoadStatus, MemoryEffect::LoadCurator])
        ));
    }

    #[test]
    fn closing_memory_drops_replies_from_the_previous_open() {
        let mut state = AppState::new("memory-close");
        let profile = state.profile.clone();
        memory_input(
            &mut state,
            MemoryInput::Open {
                profile: profile.clone(),
            },
        );
        let generation = state.memory_generation;
        memory_input(&mut state, MemoryInput::Key(MemoryKey::Close));
        assert!(state.view().memory.is_none());
        let effects = update(
            &mut state,
            AppMessage::Memory {
                generation,
                profile,
                service: None,
                input: MemoryInput::NotConnected,
            },
        );
        assert!(effects.is_empty());
        assert!(state.view().memory.is_none());
    }

    #[test]
    fn memory_settings_link_closes_before_opening_filtered_settings() {
        let mut state = AppState::new("memory-link");
        let profile = state.profile.clone();
        memory_input(&mut state, MemoryInput::Open { profile });
        let effects = memory_input(&mut state, MemoryInput::Key(MemoryKey::Settings));
        assert!(state.view().memory.is_none());
        assert!(state.view().settings.is_some());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Settings { .. }))
        );
    }

    #[test]
    fn hidden_memory_write_keeps_its_readback_and_notice() {
        use talaria_core::memory::{BuiltinFiles, CuratorView, StatusView, WriteOutcome};
        let mut state = AppState::new("memory-write");
        let profile = state.profile.clone();
        memory_input(&mut state, MemoryInput::Open { profile });
        memory_input(
            &mut state,
            MemoryInput::StatusLoaded(Ok(StatusView {
                active: String::new(),
                providers: Vec::new(),
                builtin_files: BuiltinFiles { memory: 0, user: 0 },
            })),
        );
        let mut curator = CuratorView {
            enabled: true,
            paused: false,
            interval_hours: None,
            last_run_at: None,
            min_idle_hours: None,
            stale_after_days: None,
            archive_after_days: None,
        };
        memory_input(&mut state, MemoryInput::CuratorLoaded(Ok(curator.clone())));
        for _ in 0..3 {
            memory_input(&mut state, MemoryInput::Key(MemoryKey::Down));
        }
        memory_input(&mut state, MemoryInput::Key(MemoryKey::Pause));
        let generation = state.memory_generation;
        memory_input(&mut state, MemoryInput::Key(MemoryKey::Close));
        assert!(state.view().memory.is_none());
        assert_eq!(generation, state.memory_generation);
        let effects = memory_input(
            &mut state,
            MemoryInput::PausedSet(WriteOutcome::Ok(serde_json::json!({"paused": true}))),
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Memory { effects, .. } if effects == &[MemoryEffect::LoadCurator]
        )));
        curator.paused = true;
        let effects = memory_input(&mut state, MemoryInput::CuratorLoaded(Ok(curator)));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::CommitLines(_)))
        );
        assert!(state.memory_profile.is_none());
    }
}

fn configure_input(
    state: &mut AppState,
    input: talaria_core::configure::ConfigureInput,
    submitted: Option<String>,
) -> Vec<AppEffect> {
    use talaria_core::configure::{ConfigureEffect, Phase};
    let Some(configure) = state.configure.as_mut() else {
        return Vec::new();
    };
    let now = Instant::now();
    let produced = configure.update(input, now);
    let phase = configure.view(now).phase;
    state.configure_tick_at =
        matches!(phase, Phase::Thinking | Phase::Applying).then(|| now + Duration::from_secs(1));
    let mut effects = Vec::new();
    let mut network = Vec::new();
    for effect in produced {
        match effect {
            ConfigureEffect::Notice(text) => effects.push(notice_lines(&text)),
            ConfigureEffect::ForgetHistoryLine(_) => {
                if let Some(text) = &submitted {
                    effects.push(AppEffect::UnrecordSubmit(text.clone()));
                }
            }
            ConfigureEffect::ClearEditor | ConfigureEffect::CloseOverlay => {}
            ConfigureEffect::Open(action) => {
                effects.extend(local_effect(state, action, String::new(), String::new()))
            }
            effect => network.push(effect),
        }
    }
    if !network.is_empty() {
        effects.push(AppEffect::Configure {
            generation: state.configure_generation,
            profile: state.profile.clone(),
            home: state.hermes_home.clone(),
            client: state
                .service
                .as_ref()
                .and_then(|service| service.client())
                .cloned(),
            consent: state.connectors_consent.clone(),
            service: state.configure_service.clone(),
            effects: network,
        });
    }
    effects.push(AppEffect::Redraw);
    effects
}

fn configure_command(state: &mut AppState, request: String, typed: String) -> Vec<AppEffect> {
    use talaria_core::configure::{ConfigureInput, ConfigureState};
    if state
        .configure
        .as_ref()
        .is_some_and(|configure| !configure.is_closed())
    {
        let mut effects = vec![
            notice_lines("A /configure is already open."),
            AppEffect::Redraw,
        ];
        if crate::configure::service::detect_secret(&request) {
            effects.push(AppEffect::UnrecordSubmit(typed));
        }
        return effects;
    }
    state.configure_generation = state.configure_generation.wrapping_add(1);
    state.configure_service = None;
    state.configure = Some(ConfigureState::new_with_prefix(
        crate::configure::service::detect_secret,
        format!(
            "{}-configure-{}",
            state.launch_id, state.configure_generation
        ),
    ));
    let model = state.configure_model.clone().or_else(|| {
        state
            .opened
            .as_ref()
            .and_then(|opened| opened.info.provider.clone().zip(opened.info.model.clone()))
    });
    configure_input(
        state,
        ConfigureInput::Open {
            request,
            profile: state.profile.clone(),
            connected: state.link_state == LinkView::Connected,
            model,
        },
        Some(typed),
    )
}

fn settings_input(
    state: &mut AppState,
    input: talaria_core::settings::SettingsInput,
    submitted: Option<String>,
) -> Vec<AppEffect> {
    use talaria_core::settings::SettingsEffect;
    if matches!(input, talaria_core::settings::SettingsInput::Key(_))
        && state.settings.view().profile != state.profile
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    for effect in state.settings.update(input) {
        match effect {
            SettingsEffect::Notice(text) => result.push(notice_lines(&text)),
            SettingsEffect::ForgetHistoryLine => {
                if let Some(text) = &submitted {
                    result.push(AppEffect::UnrecordSubmit(text.clone()));
                }
            }
            SettingsEffect::OpenLocal(action) => {
                result.extend(local_effect(state, action, String::new(), String::new()))
            }
            SettingsEffect::CloseOverlay => {}
            SettingsEffect::LoadChoices {
                key,
                list:
                    list @ (talaria_core::settings::RuntimeList::Skins
                    | talaria_core::settings::RuntimeList::Personalities),
                ..
            } => result.push(AppEffect::SettingsChoices {
                generation: state.settings_generation,
                profile: state.profile.clone(),
                key,
                list,
                client: state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned(),
                session: state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone()),
            }),
            effect => {
                if matches!(effect, SettingsEffect::Load { .. }) {
                    state.settings_generation = state.settings_generation.wrapping_add(1);
                    state.settings_service = None;
                }
                result.push(AppEffect::Settings {
                    generation: state.settings_generation,
                    profile: state.profile.clone(),
                    home: state.hermes_home.clone(),
                    service: state.settings_service.clone(),
                    effect,
                });
            }
        }
    }
    result.push(AppEffect::Redraw);
    result
}

#[cfg(test)]
mod connectors_wiring_tests {
    use super::*;
    use talaria_core::connectors::{
        ConnectorsEffect as E, ConnectorsInput as I, ConnectorsKey as K, ConnectorsOpen,
        ConnectorsTab, Screen as ConnectorScreen,
    };

    fn state() -> AppState {
        let mut state = AppState::new("connector-wiring");
        state.profile = "fixture".into();
        state
    }

    #[test]
    fn connector_consent_draws_before_any_request() {
        let mut state = state();
        let effects = connectors_command(
            &mut state,
            ConnectorsOpen::Tab(ConnectorsTab::Connectors),
            "",
        );
        assert!(matches!(
            state.view().connectors.unwrap().screen,
            ConnectorScreen::Consent { .. }
        ));
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Connectors { effects, .. } if effects == &[E::LoadIdentity])));
        assert!(!state.connectors_consent.granted("fixture"));
        let effects = connectors_input(&mut state, I::Key(K::Open));
        assert!(state.connectors_consent.granted("fixture"));
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Connectors { effects, .. } if effects == &[E::ListConnectors])));
        let copy = state.working_copy();
        assert!(copy.connectors_consent.granted("fixture"));
        assert!(!copy.connectors_consent.granted("default"));
    }

    #[test]
    fn messaging_opens_both_reads_and_has_no_idle_wake() {
        let mut state = state();
        let effects = connectors_command(
            &mut state,
            ConnectorsOpen::Tab(ConnectorsTab::Messaging),
            "",
        );
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Connectors { effects, .. } if effects == &[E::LoadPlatforms, E::LoadPairing])));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::ConnectorWake { .. }))
        );
        assert!(!state.connectors_consent.granted("fixture"));
        assert!(connectors_input(&mut state, I::Tick).is_empty());
    }

    #[test]
    fn a_closed_overlay_rejects_its_old_reply() {
        let mut state = state();
        connectors_command(
            &mut state,
            ConnectorsOpen::Tab(ConnectorsTab::Messaging),
            "",
        );
        let generation = state.connectors_generation;
        connectors_input(&mut state, I::Key(K::Close));
        assert!(state.view().connectors.is_none());
        let effects = update(
            &mut state,
            AppMessage::Connectors {
                generation,
                profile: "fixture".into(),
                service: None,
                read: None,
                input: I::PairingReply(Ok(Default::default())),
            },
        );
        assert!(effects.is_empty());
        assert!(state.view().connectors.is_none());
    }

    #[test]
    fn arguments_show_usage_without_opening_or_sending() {
        let mut state = state();
        let effects = connectors_command(
            &mut state,
            ConnectorsOpen::Tab(ConnectorsTab::Connectors),
            "extra",
        );
        assert!(state.view().connectors.is_none());
        assert!(
            matches!(&effects[..], [AppEffect::CommitLines(lines)] if lines == &["· Usage: /connectors"])
        );
    }

    #[test]
    fn old_platform_read_cannot_finish_a_new_save() {
        let mut state = state();
        connectors_command(
            &mut state,
            ConnectorsOpen::Tab(ConnectorsTab::Messaging),
            "",
        );
        let platform = || {
            serde_json::from_value(json!({
                "id": "signal", "name": "Signal", "enabled": false
            }))
            .unwrap()
        };
        connectors_input(&mut state, I::PlatformsReply(Ok(vec![platform()])));
        let old_read = state.connectors_reads["platforms"];
        connectors_input(&mut state, I::Key(K::Toggle));
        connectors_input(&mut state, I::Key(K::Confirm));
        assert!(state.connectors.is_busy());
        connectors_input(
            &mut state,
            I::UpdateReply {
                id: "signal".into(),
                reply: Ok(talaria_core::connectors::UpdateReply { hot_served: true }),
            },
        );
        let new_read = state.connectors_reads["platforms"];
        assert_ne!(old_read, new_read);
        let deliver = |state: &mut AppState, version| {
            update(
                state,
                AppMessage::Connectors {
                    generation: 1,
                    profile: "fixture".into(),
                    service: None,
                    read: Some(("platforms".into(), version)),
                    input: I::PlatformsReply(Ok(vec![platform()])),
                },
            )
        };
        assert!(deliver(&mut state, old_read).is_empty());
        assert!(state.connectors.is_busy());
        // The only accepted read is the one requested after the write completed.
        deliver(&mut state, new_read);
        assert!(!state.connectors.is_busy());
    }
}

#[cfg(test)]
mod configure_wiring_tests {
    use super::*;

    #[test]
    fn secret_request_forgets_exact_composer_line_without_network_work() {
        let mut state = AppState::new("configure-history");
        let token = "sk-or-v1-talaria-sentinel-0000000000000000";
        let line = format!("  /configure use {token}  ");
        let effects = submit(&mut state, line.clone());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::UnrecordSubmit(text) if text == &line))
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Configure { .. }))
        );
        assert!(!format!("{:?}", state.configure).contains(token));
        let home = tempfile::tempdir().unwrap();
        let (mut history, _) = crate::comforts::prompt_history::PromptHistory::open(
            home.path(),
            true,
            "configure-history",
        );
        assert!(crate::settings::append_history(&mut history, &line).is_none());
        assert!(history.composer_seed().is_empty());
    }

    #[test]
    fn a_second_command_focuses_the_existing_configuration_run() {
        let mut state = AppState::new("configure-focus");
        state.link_state = LinkView::Connected;
        let first = submit(&mut state, "/configure enable the bell".into());
        assert!(
            first
                .iter()
                .any(|effect| matches!(effect, AppEffect::Configure { .. }))
        );
        let generation = state.configure_generation;
        let second = submit(&mut state, "/configure make it compact".into());
        assert_eq!(state.configure_generation, generation);
        assert!(
            !second
                .iter()
                .any(|effect| matches!(effect, AppEffect::Configure { .. }))
        );
        assert!(state.view().configure.is_some());
    }

    #[test]
    fn sibling_screen_handoffs_open_their_real_overlays() {
        use talaria_core::{
            connectors::{ConnectorsOpen, ConnectorsTab},
            mcp::{McpOpen, McpTab},
            providers::{OpenTarget, ProvidersTab},
        };
        for (name, action) in [
            (
                "providers",
                LocalAction::Providers(OpenTarget::Tab(ProvidersTab::Keys)),
            ),
            ("mcp", LocalAction::Mcp(McpOpen::Tab(McpTab::Servers))),
            ("capabilities", LocalAction::Capabilities),
            ("cron", LocalAction::Cron),
            ("profiles", LocalAction::Profiles),
            (
                "connectors",
                LocalAction::Connectors(ConnectorsOpen::Tab(ConnectorsTab::Connectors)),
            ),
            ("memory", LocalAction::MemorySettings),
            ("models", LocalAction::ModelRoles),
        ] {
            let mut state = AppState::new("configure-handoff");
            state.profile = "fixture".into();
            let effects = local_effect(&mut state, action, String::new(), String::new());
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, AppEffect::ContextRead { .. }))
            );
            let opened = match name {
                "providers" => state.providers.is_open(),
                "mcp" => state.mcp.is_open(),
                "capabilities" => state.view().capabilities.is_some(),
                "cron" => cron_view(&state).is_some(),
                "profiles" => state.profiles.is_open(),
                "connectors" => state.view().connectors.is_some(),
                "memory" => state.view().memory.is_some(),
                "models" => state.model_roles.is_some(),
                _ => false,
            };
            assert!(opened, "{name} handoff must open its owner overlay");
        }
    }
}

#[cfg(test)]
mod settings_wiring_tests {
    use super::*;

    #[test]
    fn sensitive_submission_forgets_exact_line_without_loading() {
        let mut state = AppState::new("settings-history");
        let line = "  /config delegation.api_key=\"example-unclosed".to_owned();
        let effects = submit(&mut state, line.clone());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::UnrecordSubmit(text) if text == &line))
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Settings { .. }))
        );
        assert!(state.view().settings.is_none());
    }

    #[test]
    fn stale_settings_replies_are_discarded() {
        let mut state = AppState::new("settings-generation");
        let profile = state.profile.clone();
        let effects = settings_input(
            &mut state,
            talaria_core::settings::SettingsInput::Open {
                profile: profile.clone(),
                filter: None,
            },
            None,
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Settings { .. }))
        );
        let effects = update(
            &mut state,
            AppMessage::Settings {
                generation: 0,
                profile,
                service: None,
                input: talaria_core::settings::SettingsInput::LoadFailed(
                    talaria_core::settings::FailReason::NotConnected,
                ),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state.settings.view().screen,
            talaria_core::settings::Screen::Loading
        );
        let generation = state.settings_generation;
        let effects = update(
            &mut state,
            AppMessage::Settings {
                generation,
                profile: "other-profile".into(),
                service: None,
                input: talaria_core::settings::SettingsInput::LoadFailed(
                    talaria_core::settings::FailReason::NotConnected,
                ),
            },
        );
        assert!(effects.is_empty());
        assert_eq!(
            state.settings.view().screen,
            talaria_core::settings::Screen::Loading
        );
    }

    #[test]
    fn a_runtime_list_is_asked_for_on_the_right_channel() {
        use talaria_core::settings::{
            ConfigSchemaView, FieldKind, FieldView, RuntimeList, SettingsAction, SettingsEffect,
            SettingsInput, SettingsKey, SettingsSnapshot,
        };
        let mut state = AppState::new("settings-lists");
        let profile = state.profile.clone();
        settings_input(
            &mut state,
            SettingsInput::Open {
                profile: profile.clone(),
                filter: None,
            },
            None,
        );
        let field = |key: &str| FieldView {
            key: key.into(),
            kind: FieldKind::String,
            description: key.into(),
            category: "general".into(),
            options: None,
            searchable: false,
            clearable: false,
        };
        let values =
            serde_json::json!({"display": {"skin": "default"}, "terminal": {"backend": "local"}});
        let generation = state.settings_generation;
        update(
            &mut state,
            AppMessage::Settings {
                generation,
                profile: profile.clone(),
                service: None,
                input: SettingsInput::Loaded(SettingsSnapshot {
                    profile: profile.clone(),
                    schema: ConfigSchemaView {
                        fields: vec![field("display.skin"), field("terminal.backend")],
                        category_order: vec!["general".into()],
                    },
                    effective: values.clone(),
                    raw: serde_json::json!({}),
                    defaults: values,
                }),
            },
        );
        let key = |action| SettingsInput::Key(SettingsKey::Action(action));
        // Skins come from the session socket's slash completion.
        let effects = settings_input(&mut state, key(SettingsAction::Edit), None);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::SettingsChoices { key, list: RuntimeList::Skins, .. } if key == "display.skin"
        )));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Settings { .. }))
        );
        settings_input(
            &mut state,
            SettingsInput::Key(SettingsKey::Edit(talaria_core::settings::EditKey::Esc)),
            None,
        );
        settings_input(&mut state, key(SettingsAction::Down), None);
        // Terminal backends come from the profile's REST route.
        let effects = settings_input(&mut state, key(SettingsAction::Edit), None);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Settings {
                effect: SettingsEffect::LoadChoices {
                    list: RuntimeList::TerminalBackends,
                    ..
                },
                ..
            }
        )));
    }
}

fn local_effect(
    state: &mut AppState,
    action: LocalAction,
    argument: String,
    typed: String,
) -> Vec<AppEffect> {
    if action == LocalAction::Clear {
        let Some(opened) = state.opened.as_ref() else {
            return vec![notice_lines("No session is open.")];
        };
        if opened.state.turn() == &talaria_core::session::TurnState::Running
            || opened.state.open_prompts().next().is_some()
        {
            return vec![AppEffect::CommitLines(vec![
                "Finish or interrupt the current turn before /clear.".to_owned(),
            ])];
        }
        if !state.held.is_empty() {
            return vec![notice_lines(crate::switch::HELD_PROMPTS_TEXT)];
        }
        if state.link_failed {
            return vec![notice_lines(LINK_STOPPED_NOTICE)];
        }
        let cwd = state
            .folder
            .cwd
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| state.launch_folder.clone());
        return vec![AppEffect::ClearSession {
            skip_detach: state.session_gone,
            cwd,
        }];
    }
    if action == LocalAction::Copy {
        let raw = argument.trim();
        if raw == "link" {
            let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
                return vec![notice_lines("No session is open.")];
            };
            let Some((link, code)) = opened.state.notices().sign_in_copy() else {
                return vec![notice_lines("No sign-in link to copy.")];
            };
            let (text, notice) = match code {
                Some(code) => (
                    format!("{link}\n{code}"),
                    "Copy requested: sign-in link and code. Your terminal must allow clipboard access.",
                ),
                None => (
                    link,
                    "Copy requested: sign-in link. Your terminal must allow clipboard access.",
                ),
            };
            return vec![notice_lines(notice), AppEffect::CopyReply { text }];
        }
        let latest = raw.is_empty();
        let raw = if latest { "1" } else { raw };
        let digits = raw.strip_prefix('+').unwrap_or(raw);
        if digits.is_empty()
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
            || digits.bytes().all(|byte| byte == b'0')
        {
            return vec![notice_lines("Usage: /copy [N]")];
        }
        let Some(opened) = state.opened.as_ref().filter(|_| !state.session_gone) else {
            return vec![notice_lines("No session is open.")];
        };
        let Ok(n) = raw.parse::<u32>() else {
            return vec![notice_lines(&format!("No reply {raw} to copy."))];
        };
        let Ok(text) = talaria_core::session::assistant_reply(opened.state.cells(), n) else {
            if latest {
                return vec![notice_lines("No finished assistant reply to copy yet.")];
            }
            return vec![notice_lines(&format!("No reply {raw} to copy."))];
        };
        let count = text.chars().count();
        if count > 100_000 {
            return vec![notice_lines(&format!(
                "Reply {n} is too long to copy ({count} characters). Use /export."
            ))];
        }
        return vec![
            notice_lines(&format!(
                "Copy requested: reply {n}, {count} characters. Your terminal must allow clipboard access."
            )),
            AppEffect::CopyReply { text },
        ];
    }
    if action == LocalAction::Layout {
        return match talaria_core::layout::parse_layout_args(&argument) {
            Ok(talaria_core::layout::LayoutAction::Panel) => {
                user_action(state, UserAction::LayoutPanel)
            }
            Ok(talaria_core::layout::LayoutAction::Bar) => {
                user_action(state, UserAction::ToggleMenuBar)
            }
            Ok(talaria_core::layout::LayoutAction::Menu) => {
                user_action(state, UserAction::FocusMenu)
            }
            Ok(talaria_core::layout::LayoutAction::Mouse) => {
                state.layout.mouse = !state.layout.mouse;
                vec![
                    notice_lines(if state.layout.mouse {
                        "Mouse navigation on for questions, transcripts, workflows, Settings, model and session lists, and Kanban."
                    } else {
                        "Mouse navigation off."
                    }),
                    AppEffect::Redraw,
                ]
            }
            Err(notice) => vec![AppEffect::CommitLines(vec![notice])],
        };
    }
    if action == LocalAction::Thinking {
        use talaria_core::layout::ReasoningDisplay;
        return match talaria_core::layout::parse_thinking_args(&argument, state.layout.reasoning) {
            Ok(display) => {
                state.layout.reasoning = display;
                vec![
                    notice_lines(match display {
                        ReasoningDisplay::Full => THINKING_FULL_NOTICE,
                        ReasoningDisplay::Collapsed => THINKING_COLLAPSED_NOTICE,
                    }),
                    AppEffect::Redraw,
                ]
            }
            Err(notice) => vec![notice_lines(&notice)],
        };
    }
    if action == LocalAction::Kanban {
        let board = if argument.trim().is_empty() {
            "default"
        } else {
            argument.trim()
        };
        let (board_state, _) = talaria_core::kanban::KanbanBoardState::open(
            board,
            (state.wall_s * 1000.0).max(0.0) as u64,
        );
        state.kanban = Some(board_state);
        return vec![AppEffect::PrepareKanban {
            board: board.into(),
        }];
    }
    if action == LocalAction::Voice {
        if state.client_voice_config.backend == "client-bridge" {
            return client_voice_command(state, &argument);
        }
        if state.remote_gateway {
            return notice_effect(state, REMOTE_VOICE_NOTICE.into());
        }
        let Some(session) = state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.clone())
        else {
            return notice_effect(state, "No session is attached.".into());
        };
        return match talaria_core::voice::parse_voice_args(&argument) {
            Ok(talaria_core::voice::VoiceCommand::Status) => vec![AppEffect::VoiceStatus {
                session_id: session,
            }],
            Ok(command) => voice_input(
                state,
                &session,
                talaria_core::voice::VoiceInput::Command(command),
            ),
            Err(text) => notice_effect(state, text),
        };
    }
    if action == LocalAction::Workflows {
        return workflow_command(state, &argument);
    }
    if action == LocalAction::Hivecode {
        let session = state
            .opened
            .as_ref()
            .map(|s| SessionId::new(s.session_id.clone()));
        if let Some(session) = &session {
            let request = state.hivecode_requests.entry(session.clone()).or_default();
            *request = request.wrapping_add(1);
        }
        let profile = state.profile.clone();
        let foreground = session.as_ref().map(|session| crate::hivecode::Foreground {
            session,
            profile: &profile,
        });
        let actions = state.hivecode.handle_local_command(&argument, foreground);
        return hivecode_actions(state, actions);
    }

    if action == LocalAction::Configure {
        return configure_command(state, argument, typed);
    }
    if action == LocalAction::Capabilities {
        return capabilities_command(state, &argument);
    }
    if action == LocalAction::MemorySettings {
        return memory_input(
            state,
            talaria_core::memory::MemoryInput::Open {
                profile: state.profile.clone(),
            },
        );
    }
    if action == LocalAction::Cron {
        return match cron::parse_arguments(&argument) {
            Ok(mode) => open_cron(state, mode),
            Err(usage) => vec![notice_lines(&usage)],
        };
    }
    if action == LocalAction::Config {
        let input = talaria_core::settings::SettingsInput::Command {
            profile: state.profile.clone(),
            args: argument,
        };
        return settings_input(state, input, Some(typed));
    }
    if action == LocalAction::Doctor {
        return open_doctor(state);
    }
    if action == LocalAction::Profiles {
        return if argument.trim().is_empty() {
            profiles_input(state, ProfilesInput::Open)
        } else {
            vec![notice_lines("/profiles takes no arguments.")]
        };
    }
    if let LocalAction::Providers(target) = action {
        return providers_input(
            state,
            ProvidersInput::Open {
                target,
                profile_name: state.profile.clone(),
                connected: state.link_state == LinkView::Connected,
            },
        );
    }
    if matches!(action, LocalAction::Mcp(_)) {
        return match talaria_core::mcp::parse_mcp_args(&argument) {
            Ok(open) => open_mcp(state, open),
            Err(usage) => vec![notice_lines(usage)],
        };
    }
    if action == LocalAction::Fleet {
        return open_fleet_view(state);
    }
    if action == LocalAction::Agents {
        return agents_command(state, &argument);
    }
    if action == LocalAction::Review {
        return changes_command(state);
    }
    if action == LocalAction::ModelRoles {
        return open_model_roles(state);
    }
    if let LocalAction::Connectors(open) = action {
        return connectors_command(state, open, &argument);
    }
    if matches!(
        action,
        LocalAction::Processes | LocalAction::Bg | LocalAction::Stop
    ) {
        return background_command(state, action, &argument, typed);
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
        | LocalAction::Cd
        | LocalAction::Queue
        | LocalAction::Steer
        | LocalAction::Resume
        | LocalAction::Rename
        | LocalAction::Branch
        | LocalAction::Export => {
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
#[inline(never)]
fn reduce(state: &mut AppState, event: Event) -> Vec<AppEffect> {
    let observed = observe_context(state, &event);
    let mut effects = automation_from_session(state, &event);
    let Some(opened) = state.opened.as_mut() else {
        refresh_fleet_warning_count(state);
        return effects;
    };
    let produced = talaria_core::session::update(&mut opened.state, event);
    effects.extend(route(state, produced, None));
    effects.extend(observed);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    refresh_fleet_warning_count(state);
    effects
}

fn refresh_fleet_warning_count(state: &mut AppState) {
    if let Some(fleet) = state.fleet.as_mut() {
        let count = state
            .opened
            .as_ref()
            .map_or(0, |opened| opened.state.notices().warning_count());
        fleet.set_warning_count(u32::try_from(count).unwrap_or(u32::MAX));
    }
}

fn background_effects(
    session_id: SessionId,
    produced: Vec<BackgroundEffect>,
    foreground: bool,
) -> Vec<AppEffect> {
    produced
        .into_iter()
        .filter_map(|effect| match effect {
            BackgroundEffect::Redraw => foreground.then_some(AppEffect::Redraw),
            BackgroundEffect::Signal(_) => None,
            effect => Some(AppEffect::Background {
                session_id: session_id.clone(),
                effect,
            }),
        })
        .collect()
}

pub(crate) fn background_event(
    state: &mut AppState,
    session_id: &str,
    event: BackgroundEvent,
) -> Vec<AppEffect> {
    if is_open_session(state, session_id) {
        if state.session_gone {
            return Vec::new();
        }
        return reduce(state, Event::Background(event));
    }
    let key = retained_key(state, session_id);
    let Some(opened) = key.and_then(|key| state.retained_sessions.get_mut(&key)) else {
        return Vec::new();
    };
    let produced = talaria_core::session::update(&mut opened.state, Event::Background(event));
    let background = produced
        .into_iter()
        .filter_map(|effect| match effect {
            Effect::Background(effect) => Some(effect),
            _ => None,
        })
        .collect();
    background_effects(SessionId::new(session_id), background, false)
}

pub(crate) fn close_processes(state: &mut AppState) -> Vec<AppEffect> {
    if !state.processes_open {
        return Vec::new();
    }
    state.processes_open = false;
    let Some(session_id) = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
    else {
        return Vec::new();
    };
    background_event(state, &session_id, BackgroundEvent::OverlayClosed)
}

fn background_command(
    state: &mut AppState,
    action: LocalAction,
    argument: &str,
    typed: String,
) -> Vec<AppEffect> {
    let Some(session_id) = state
        .opened
        .as_ref()
        .filter(|_| !state.session_gone)
        .map(|opened| opened.session_id.clone())
    else {
        let name = match action {
            LocalAction::Processes => "processes",
            LocalAction::Bg => "bg",
            _ => "stop",
        };
        state.draft = typed.clone();
        return vec![
            notice_lines(&format!(
                "No session is attached, so /{name} cannot run. Start one with talaria, talaria -c or talaria --resume <id>."
            )),
            AppEffect::LoadDraft(typed),
        ];
    };
    if action == LocalAction::Processes {
        state.processes_open = true;
    }
    state.draft.clear();
    let Some(event) =
        crate::background::command_event(&action, argument, turn_running(state), state.now_ms)
    else {
        return Vec::new();
    };
    background_event(state, &session_id, event)
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
            | Effect::RespondConnection { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => effects.push(AppEffect::Respond {
                effect,
                note: note.take(),
            }),
            Effect::RunShell { .. } | Effect::AskSide { .. } => {
                if let Some(key) = foreground_row(state) {
                    effects.push(AppEffect::Side { key, effect });
                }
            }
            // An edited routed prompt keeps its building block.
            Effect::ReplaceQueued {
                ref admission_id,
                ref submission_id,
                ref text,
                ..
            } if talaria_core::hivecode::typed_text(text).is_some() => {
                if let Some(opened) = state.opened.as_ref()
                    && let Some(row) = opened
                        .state
                        .queue
                        .rows
                        .iter()
                        .find(|row| row.admission_id == *admission_id)
                {
                    let session = SessionId::new(opened.session_id.clone());
                    let edit = crate::hivecode::Retype {
                        input_id: &row.input_id.0,
                        submission_id: &submission_id.0,
                        old: talaria_core::hivecode::shown_text(&row.text),
                        new: talaria_core::hivecode::shown_text(text),
                    };
                    state.hivecode.retype_authoring(&session, &edit);
                }
                effects.push(AppEffect::Queue(effect));
            }
            // Issue 232. Queue and steer. The queue service will run these calls.
            Effect::CancelQueued { .. }
            | Effect::ReplaceQueued { .. }
            | Effect::SteerQueued { .. }
            | Effect::Steer { .. }
            | Effect::ResolveUnknown { .. } => effects.push(AppEffect::Queue(effect)),
            Effect::Background(effect) => {
                if let Some(session_id) = state
                    .opened
                    .as_ref()
                    .map(|opened| SessionId::new(&opened.session_id))
                {
                    if state.interrupting
                        && matches!(effect, BackgroundEffect::StopProcesses { .. })
                    {
                        if let BackgroundEffect::StopProcesses { request } = &effect {
                            state.background_stop_bindings.insert(
                                (session_id.as_str().to_owned(), request.0),
                                BackgroundBinding {
                                    service: state.background_service.clone(),
                                    generation: state.connection_generation,
                                },
                            );
                        }
                        state.background_after_interrupt.push((session_id, effect));
                    } else {
                        effects.extend(background_effects(session_id, vec![effect], true));
                    }
                }
            }
        }
    }
    effects
}

/// The session a mutation reply answers for, so its fences reach every ledger once it settles.
fn mutation_session(message: &AppMessage) -> Option<String> {
    let request = match message {
        AppMessage::Mutated { request, .. }
        | AppMessage::BranchSent { request, .. }
        | AppMessage::RowRenamed { request, .. }
        | AppMessage::CompactDone { request, .. }
        | AppMessage::Cd(CdInput::Outcome { request, .. })
        | AppMessage::Models(ModelsInput::MutationReturned { request, .. }) => request,
        _ => return None,
    };
    Some(request.session_id.clone())
}

/// The ledgers of session actions, folder moves and model picks fence the same session. What
/// one learns from a reply the others adopt, so a change sent right after another kind of
/// change presents the newer fences instead of being refused and sent again. Session events
/// already reach all three; a reply can arrive before its event.
fn share_fences(state: &mut AppState, session_id: &str) {
    let learned = [
        state.ledger.fences(session_id),
        state.cd_ledger.fences(session_id),
        state.models.fences(session_id),
    ];
    for fences in learned {
        state
            .ledger
            .fences_mut(session_id)
            .adopt(fences.revision, fences.execution_generation);
        state
            .cd_ledger
            .fences_mut(session_id)
            .adopt(fences.revision, fences.execution_generation);
        state.models.adopt_fences(session_id, fences);
    }
}

/// Feed the permissions reducer and turn its effects into executor effects. Before the first
/// session opens there is no permissions state, and inputs change nothing.
///
/// The session bypass is the value `/yolo` sets, so a reply that wrote or read it also reads
/// the standing yolo mode again, as a finished `/yolo` does. A call that was never sent changed
/// nothing, so it reads nothing.
pub(crate) fn permissions(state: &mut AppState, input: PermissionsInput) -> Vec<AppEffect> {
    let label = state.toggle_label.clone();
    let bypass = match &input {
        PermissionsInput::SessionOffSet {
            session_id, result, ..
        }
        | PermissionsInput::SessionOffRead {
            session_id, result, ..
        } => Some((
            session_id.clone(),
            !matches!(result, Err(PermissionsFailure::NotSent)),
        )),
        _ => None,
    };
    let Some(permissions) = state.permissions.as_mut() else {
        return Vec::new();
    };
    let mut effects: Vec<AppEffect> = permissions
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
        .collect();
    if let Some((session_id, sent)) = bypass {
        effects.extend(automation_event(
            state,
            AutomationEvent::CommandFinished {
                session_id,
                canonical: "yolo".to_string(),
                result: if sent {
                    CommandResult::Output
                } else {
                    CommandResult::NotRun
                },
            },
        ));
    }
    effects
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
        UserAction::VoiceRecord if state.client_voice_config.backend == "client-bridge" => {
            if state.switching {
                return Vec::new();
            }
            let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            else {
                return notice_effect(state, "No session is attached.".into());
            };
            if !state.client_voice_enabled {
                return notice_effect(
                    state,
                    "Voice is off. Type /voice on to use the microphone.".into(),
                );
            }
            if !crate::voice::transport::remote_audio_allowed(
                state.remote_gateway,
                state.client_voice_config.remote_audio,
            ) {
                return notice_effect(
                    state,
                    "Remote audio is off. Enable [voice] remote_audio to use this gateway's relay."
                        .into(),
                );
            }
            if !state.client_voice.active() || state.client_voice.speaking() {
                state.client_voice_target =
                    state.fleet.as_ref().and_then(|fleet| fleet.voice_target());
                if state.fleet.is_some() && state.client_voice_target.is_none() {
                    return notice_effect(state, "Select a fleet row before recording.".into());
                }
                if state
                    .client_voice_target
                    .as_ref()
                    .is_some_and(|target| state.client_voice_inflight.contains(&target.key))
                {
                    state.client_voice_target = None;
                    return notice_effect(
                        state,
                        "Wait for the previous reply before recording for this row.".into(),
                    );
                }
                if let Some(target) = state.client_voice_target.as_mut()
                    && target.backend == talaria_core::fleet::FleetBackend::Acp
                    && state.fleet.as_ref().is_some_and(|fleet| {
                        *fleet.focus() == talaria_core::fleet_view::Focus::Answer
                    })
                {
                    target.need = state
                        .acp_views
                        .get(&target.key)
                        .and_then(|view| view.needs.first())
                        .map(|need| talaria_core::fleet::NeedId {
                            key: target.key.clone(),
                            slot: talaria_core::fleet::NeedSlot::External(need.responder.clone()),
                        });
                    if target.need.is_none() {
                        return notice_effect(
                            state,
                            "The selected permission is no longer available.".into(),
                        );
                    }
                }
            }
            let input = talaria_core::voice::client::Input::Cutoff(
                talaria_core::voice::client::Cutoff::Record,
            );
            client_voice_input(state, &session, input)
        }
        UserAction::VoiceRecord if state.remote_gateway => {
            notice_effect(state, REMOTE_VOICE_NOTICE.into())
        }
        UserAction::VoiceRecord if state.switching => Vec::new(),
        UserAction::VoiceRecord => {
            let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            else {
                return notice_effect(state, "No session is attached.".into());
            };
            voice_input(state, &session, talaria_core::voice::VoiceInput::Press)
        }
        UserAction::Capabilities(input) => capabilities_input(state, *input),
        UserAction::Cron(input) => match *input {
            CronInput::Open(mode) => open_cron(state, mode),
            input => cron_input(state, input),
        },
        UserAction::Configure(input) => configure_input(state, *input, None),
        UserAction::Settings(input) => settings_input(state, input, None),
        UserAction::Doctor(key) => doctor_key(state, key),
        UserAction::Profiles(input) => profiles_input(state, input),
        UserAction::ProfilesLayout { width, page_size } => {
            state.profiles.set_layout(width, page_size);
            vec![AppEffect::Redraw]
        }
        UserAction::Providers(input) => providers_input(state, *input),
        UserAction::ModelRoles(input) => model_roles_input(state, input),
        UserAction::Mcp(input) => mcp_input(state, input),
        UserAction::Memory(input) => memory_input(state, input),
        UserAction::Connectors(input) => connectors_input(state, input),
        UserAction::Kanban(key) => kanban_input(state, talaria_core::kanban::KanbanInput::Key(key)),
        UserAction::Workflow(key) => {
            if state.workflow_views.picker.is_some() {
                return workflow_picker_input(state, key);
            }
            let Some(run) = state.workflow_views.active.clone() else {
                return vec![];
            };
            workflow_input(
                state,
                run,
                talaria_workflow::progress::live::ProgressInput::Key(key),
            )
        }
        UserAction::WorkflowAgentBack => workflow_agent_back(state),
        UserAction::WorkflowView => {
            // An agent's own session opens its run's view; the lead session opens the run it
            // started; any other session opens the run list.
            let agent_run = state
                .workflow_agent
                .as_ref()
                .filter(|_| state.opened.is_some())
                .map(|agent| agent.run_dir().to_string_lossy().into_owned());
            let run = agent_run.or_else(|| {
                state.opened.as_ref().and_then(|opened| {
                    state
                        .hivecode
                        .tracked(&SessionId::new(opened.session_id.clone()))
                        .map(|run| run.run_dir.clone())
                })
            });
            workflow_command(state, run.as_deref().unwrap_or_default())
        }
        UserAction::ToggleAgents => {
            let key = foreground_row(state);
            if let Some(hub) = state.subagents.as_mut() {
                if hub.panel_key().is_some() {
                    hub.close_panel();
                } else if let Some(key) = key {
                    hub.open_panel(key);
                }
            }
            vec![AppEffect::Redraw]
        }
        UserAction::Subagent(action) => {
            let now = subagent_clock(state);
            let key = state
                .subagents
                .as_ref()
                .and_then(|hub| hub.panel_key().cloned())
                .or_else(|| foreground_row(state));
            if let (Some(hub), Some(key)) = (state.subagents.as_mut(), key) {
                hub.act(&key, action, now);
            }
            vec![AppEffect::Redraw]
        }
        UserAction::SubagentPanel(key) => {
            if let Some(hub) = state.subagents.as_mut() {
                hub.open_panel(key);
            }
            vec![AppEffect::Redraw]
        }
        UserAction::SubagentNextSession => {
            if let Some(hub) = state.subagents.as_mut() {
                hub.next_panel_session();
            }
            vec![AppEffect::Redraw]
        }
        UserAction::SubagentClearFlash => {
            if let Some(hub) = state.subagents.as_mut() {
                hub.clear_flash();
            }
            Vec::new()
        }
        UserAction::Processes(intent) => {
            if matches!(intent, ProcessesIntent::Close) {
                state.processes_open = false;
            }
            let Some(session_id) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            else {
                return Vec::new();
            };
            crate::background::intent_event(&intent, state.now_ms).map_or_else(Vec::new, |event| {
                background_event(state, &session_id, event)
            })
        }
        UserAction::Rewind(key) => rewind_input(state, RewindInput::Key(key)),
        UserAction::RewindViewport(rows) => rewind_input(state, RewindInput::Viewport(rows)),
        UserAction::Escape { eligible } => {
            if state.uses_client_voice()
                && state.client_voice.active()
                && let Some(session) = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone())
            {
                return client_voice_input(
                    state,
                    &session,
                    talaria_core::voice::client::Input::Cutoff(
                        talaria_core::voice::client::Cutoff::Escape,
                    ),
                );
            }
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
        UserAction::ModelFilterPaste(text) => models_input(state, ModelsInput::PasteFilter(text)),
        UserAction::SelectModelRow(key) => models_input(state, ModelsInput::SelectRow(key)),
        UserAction::ModelConfirm(key) => models_input(state, ModelsInput::Confirm(key)),
        UserAction::Fleet(input) => fleet_input(state, input),
        UserAction::FleetOpen => open_fleet_view(state),
        UserAction::OpenInPane { key, title } => {
            let inputs = crate::fleet_actions::start_named_pane(state, key, title);
            let mut effects = Vec::new();
            for input in inputs {
                if let talaria_core::fleet_view::FleetViewInput::Notice(text) = input {
                    effects.extend(pane_notice(state, text));
                }
            }
            effects
        }
        UserAction::FleetAnswer(intent) => vec![AppEffect::FleetAnswer(intent)],
        UserAction::FleetAnswerEdited => vec![AppEffect::Redraw],
        UserAction::AcpSelect => vec![AppEffect::Redraw],
        UserAction::AcpAnswer {
            key,
            responder,
            tool_call_id,
            option_id,
        } => {
            if let Some(manager) = &state.acp_manager {
                manager.send(crate::fleet::ExternalCommand::Answer {
                    key,
                    responder,
                    tool_call_id,
                    option_id,
                });
            }
            Vec::new()
        }
        UserAction::Review(key) => review_input(state, talaria_core::review::ReviewInput::Key(key)),
        UserAction::ReviewPaste(text) => {
            review_input(state, talaria_core::review::ReviewInput::Paste(text))
        }
        UserAction::LaunchDialog(event) => {
            use crate::bg::dialog::DialogEvent;
            match event {
                DialogEvent::Opened | DialogEvent::Edited => vec![AppEffect::Redraw],
                DialogEvent::Closed => {
                    state.launch_ui.closed();
                    vec![AppEffect::Redraw]
                }
                DialogEvent::Submit(submit) => {
                    if let Some(name) = submit.profile.strip_prefix("acp:") {
                        let id = name.split_whitespace().next().unwrap_or_default();
                        let typed = submit.folder.trim();
                        let cwd = if typed == "~" || typed.starts_with("~/") {
                            state.home.as_ref().map(|home| {
                                home.join(typed.trim_start_matches('~').trim_start_matches('/'))
                            })
                        } else {
                            Some(PathBuf::from(typed))
                        };
                        if submit.worktree
                            || cwd
                                .as_ref()
                                .is_none_or(|path| !path.is_absolute() || !path.is_dir())
                        {
                            state.launch_ui.external_finished(Err(
                                "The working directory does not exist.".into(),
                            ));
                        } else if let (Some(manager), Some(cwd)) = (&state.acp_manager, cwd) {
                            state.launch_ui.external_started();
                            manager.send(crate::fleet::ExternalCommand::Launch {
                                agent_id: id.into(),
                                cwd,
                                prompt: submit.prompt,
                            });
                        }
                        return vec![AppEffect::Redraw];
                    }
                    let context = launch_context(state);
                    let out = state.launch_ui.submit(submit, &context);
                    launch_effects(state, out)
                }
            }
        }
        UserAction::Submit(text) => {
            if let Some(Ok((LocalAction::Layout, argument))) = local_line(&text) {
                return local_effect(state, LocalAction::Layout, argument, text);
            }
            if state.opened.is_none()
                && let Some(Ok((action, argument))) = local_line(&text)
                && matches!(
                    action,
                    LocalAction::Processes | LocalAction::Bg | LocalAction::Stop
                )
            {
                return background_command(state, action, &argument, text);
            }
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
            if state.uses_client_voice()
                && state.client_voice.active()
                && let Some(session) = state
                    .opened
                    .as_ref()
                    .map(|opened| opened.session_id.clone())
            {
                return client_voice_input(
                    state,
                    &session,
                    talaria_core::voice::client::Input::Cutoff(
                        talaria_core::voice::client::Cutoff::Escape,
                    ),
                );
            }
            if state.interrupting {
                return Vec::new();
            }
            reduce(state, Event::Interrupt)
        }
        UserAction::InterruptKey => {
            // The voice cutoff and an interrupt already in flight take the key as they always did.
            let voice_cutoff =
                state.uses_client_voice() && state.client_voice.active() && state.opened.is_some();
            let mut effects = Vec::new();
            if turn_running(state) && !state.interrupting && !voice_cutoff {
                // Only the settle tick interrupts: a second press waits to see whether the key
                // is held.
                let _: InterruptPress = state.interrupt_confirm.press(state.now_ms);
            } else {
                state.interrupt_confirm.reset();
                effects.extend(user_action(state, UserAction::Interrupt));
            }
            effects
        }
        UserAction::Quit => {
            state.quitting = true;
            vec![AppEffect::Quit]
        }
        UserAction::Connect {
            session_id,
            op_id,
            choice,
        } => {
            use hermes_contract::schemas::{
                ConnectionAnswer, ConnectionAnswerStatus, ConnectionAnswerTarget,
                ConnectionSettleReason,
            };
            let Some(opened) = state
                .opened
                .as_ref()
                .filter(|opened| opened.session_id == session_id)
            else {
                return Vec::new();
            };
            let Some(card) = opened
                .state
                .connection_card()
                .filter(|card| card.op_id == op_id)
            else {
                return Vec::new();
            };
            if choice == ConnectionChoice::Approve
                && card.targets.iter().any(|target| {
                    target
                        .required_env
                        .as_ref()
                        .is_some_and(|env| !env.is_empty())
                })
            {
                return Vec::new();
            }
            let result = if choice == ConnectionChoice::Continue {
                ConnectionAnswer {
                    settled_by: Some(ConnectionSettleReason::Continue),
                    ..ConnectionAnswer::default()
                }
            } else {
                ConnectionAnswer {
                    targets: card
                        .targets
                        .iter()
                        .map(|target| ConnectionAnswerTarget {
                            name: target.name.clone(),
                            status: if choice == ConnectionChoice::Approve {
                                ConnectionAnswerStatus::Approved
                            } else {
                                ConnectionAnswerStatus::Skipped
                            },
                            env: None,
                            detail: None,
                        })
                        .collect(),
                    ..ConnectionAnswer::default()
                }
            };
            route(
                state,
                vec![Effect::RespondConnection {
                    session_id: SessionId::new(session_id),
                    op_id,
                    result,
                }],
                None,
            )
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
        UserAction::Masked { target, value } => match target {
            MaskedTarget::Recovered { request_id } => {
                if state.opened.as_ref().is_some_and(|opened| {
                    opened.state.open_prompts().any(|prompt| {
                        prompt.kind == PromptKind::ServerRequest
                            && prompt.state == PromptState::Open
                            && prompt.key.prompt_id == request_id
                    })
                }) {
                    vec![PromptService::answer_recovered(&request_id, value)]
                } else {
                    Vec::new()
                }
            }
            MaskedTarget::Live(target) => vec![AppEffect::AnswerMasked { target, value }],
        },
        UserAction::Notice(text) => {
            let notice = notice_lines(&text);
            if let Some(board) = &mut state.kanban {
                talaria_core::kanban::update(
                    board,
                    talaria_core::kanban::KanbanInput::Notice(text),
                );
            }
            vec![notice]
        }
        UserAction::Queue(intent) => queue_intent(state, intent),
        UserAction::Palette(input) => palette_input(state, input),
        UserAction::Choice(key) => choice::key(state, key),
        UserAction::EffortScale(key) => choice::scale_key(state, key),
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
            let input = if recalled {
                PaletteInput::Recalled {
                    text: state.draft.clone(),
                }
            } else {
                PaletteInput::DraftChanged {
                    text: state.draft.clone(),
                    caret,
                }
            };
            effects.extend(palette_input(state, input));
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
            state
                .attachments
                .set_workspace(active_attachment_workspace(state));
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
            // The paste goes to the composer, so an open selector closes first and puts the
            // look back, as a typed `/` does; otherwise the next Enter would apply a theme or
            // skin instead of sending the pasted text.
            let mut effects = choice::close(state);
            effects.extend(paste(state, text, draft, caret));
            effects
        }
        UserAction::Input(key) => input_key(state, key),
        UserAction::LayoutPanel => {
            state
                .layout
                .toggle_panel(state.pane_size.0, state.pane_size.1);
            vec![AppEffect::Redraw]
        }
        UserAction::ToggleMenuBar => {
            state.layout.preferences.menu_bar = !state.layout.preferences.menu_bar;
            state.layout.menu_focused = state.layout.preferences.menu_bar;
            state.layout.menu_transient = state.layout.preferences.menu_bar
                && state.pane_size.1 < talaria_core::layout::MIN_ROWS;
            state.layout.menu_dropdown = false;
            vec![AppEffect::Redraw]
        }
        UserAction::FocusMenu => {
            state.layout.menu_focused = true;
            state.layout.menu_transient = !state.layout.preferences.menu_bar
                || state.pane_size.1 < talaria_core::layout::MIN_ROWS;
            state.layout.menu_dropdown = false;
            vec![AppEffect::Redraw]
        }
        UserAction::MenuFocus {
            focused,
            dropdown,
            transient,
        } => {
            state.layout.menu_focused = focused;
            state.layout.menu_dropdown = dropdown;
            state.layout.menu_transient = transient;
            vec![AppEffect::Redraw]
        }
        UserAction::MenuCommand(command) => submit(state, command),
        UserAction::DismissWorkPanel => {
            if state.layout.dismiss_panel() {
                vec![AppEffect::Redraw]
            } else {
                Vec::new()
            }
        }
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
        UserAction::Picker(input) => picker_input(state, input),
        UserAction::AttentionNext => attention_next(state),
    }
}

/// A paste into the composer: it lands at the caret, and a dropped file becomes an attachment.
fn paste(state: &mut AppState, text: String, draft: String, caret: usize) -> Vec<AppEffect> {
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

/// Run the interrupt key's timer at the application clock. When a second press has settled with
/// no further press, interrupt the turn as the Interrupt action does. The timer is reset when a
/// turn ends or the open session changes, so a press whose turn or session is gone is dropped
/// here and nothing else is interrupted.
fn settled_interrupt(state: &mut AppState) -> Vec<AppEffect> {
    // Keys that are waiting to be read decide first: one of them may be the press that shows
    // the key is held. The timer keeps its deadline, so the loop comes straight back.
    if state.keys_waiting {
        return Vec::new();
    }
    if state.interrupt_confirm.tick(state.now_ms) && turn_running(state) && !state.interrupting {
        user_action(state, UserAction::Interrupt)
    } else {
        Vec::new()
    }
}

/// Keys in the foreground: an open palette takes Enter and Esc before the composer and the
/// turn do, so Esc closes the palette and never interrupts a running turn.
fn foreground_key(state: &mut AppState, action: UserAction) -> Vec<AppEffect> {
    if !state.palette.open {
        return user_action(state, action);
    }
    match action {
        UserAction::InterruptKey => palette_input(state, PaletteInput::Escape),
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
    state.caret = caret;
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
                // Trimmed as a typed submit is, and recorded before the submit, so a later
                // UnrecordSubmit names the same text and can take it back.
                let line = line.trim_end().to_owned();
                effects.push(AppEffect::RecordSubmit(line.clone()));
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
    let accepted_catalog = matches!(&input, CommandsInput::CatalogArrived { request_id, .. }
        if state.commands.in_flight == Some(*request_id));
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
    if accepted_catalog {
        effects.extend(command_collision_notices(state));
    }
    effects
}

fn command_collision_notices(state: &mut AppState) -> Vec<AppEffect> {
    let (Some(opened), Some(catalog)) = (&state.opened, &state.commands.catalog) else {
        return Vec::new();
    };
    if state.session_gone
        || state.commands.bound_session.as_deref() != Some(&opened.session_id)
        || state.opened_profile.as_ref() != Some(&state.profile)
    {
        return Vec::new();
    }
    let session = RowKey {
        profile: state.profile.clone(),
        session_id: opened.stored_session_id.clone(),
    };
    let notices: Vec<_> = catalog
        .skill_collisions()
        .map(|(command, skill)| {
            format!(
                "Hermes: The built-in /{command} and the {skill} skill share a command name. \
                 Use /skill {skill} to run the skill."
            )
        })
        .filter(|text| {
            state
                .command_collisions
                .insert((session.clone(), text.clone()))
        })
        .collect();
    notices
        .into_iter()
        .flat_map(|text| {
            reduce(
                state,
                Event::Notice {
                    kind: NoticeKind::Warning,
                    text,
                },
            )
        })
        .collect()
}

/// The catalogue store follows the connection only once the session service exists.
fn commands_follow(state: &mut AppState, input: CommandsInput) -> Vec<AppEffect> {
    if state.service.is_none() {
        return Vec::new();
    }
    commands_input(state, input)
}

/// The conversation the open session holds: its stored id and how often Hermes compacted it.
fn skill_epoch(opened: &OpenedSession) -> talaria_core::hivecode::SkillEpoch {
    talaria_core::hivecode::SkillEpoch {
        stored: opened.stored_session_id.clone(),
        compressions: opened.state.usage().compressions.unwrap_or(0),
    }
}

/// Say which conversation the open session holds before a prompt is routed. A prompt that
/// carried the whole skill and still waits in the queue may yet be removed, so the next routed
/// prompt carries the skill as well; the same holds for a prompt that named the session model.
fn hivecode_note_conversation(state: &mut AppState, session: &SessionId) {
    let Some(opened) = state.opened.as_ref() else {
        return;
    };
    let epoch = skill_epoch(opened);
    let waiting = opened.state.queue.skill_waiting();
    let model_waiting = opened.state.queue.model_waiting();
    state.hivecode.note_epoch(session, &epoch);
    if waiting {
        state.hivecode.rearm(session);
    }
    if model_waiting {
        state.hivecode.forget_model(session);
    }
}

/// The one-time look at a profile's Hermes settings, asked the first time a session of the
/// profile opens in this run. Nothing is asked without a Hermes home to find the gateway from.
fn config_check_effect(state: &mut AppState) -> Option<AppEffect> {
    if state.hermes_home.as_os_str().is_empty()
        || !state.config_checked.insert(state.profile.clone())
    {
        return None;
    }
    Some(AppEffect::ConfigCheck {
        profile: state.profile.clone(),
        home: state.hermes_home.clone(),
    })
}

/// Apply workflow routing through the existing session events and effects.
fn hivecode_actions(state: &mut AppState, actions: Vec<crate::hivecode::Action>) -> Vec<AppEffect> {
    apply_hivecode(state, actions, true)
}

fn apply_hivecode(
    state: &mut AppState,
    actions: Vec<crate::hivecode::Action>,
    clear_composer: bool,
) -> Vec<AppEffect> {
    use crate::hivecode::Action;
    let mut effects = vec![];
    for action in actions {
        match action {
            Action::Notice(text) => effects.push(notice_lines(&text)),
            Action::SubmitCommand { text, display, .. } => {
                let submission_id = state.next_submission_id();
                // Every routed prompt, whole skill or reminder, watches for the run it may start.
                if let Some(opened) = state.opened.as_ref() {
                    let session = SessionId::new(opened.session_id.clone());
                    let epoch = skill_epoch(opened);
                    let since_ms = (state.wall_s * 1000.0) as i64;
                    let profile = state.profile.clone();
                    state
                        .hivecode
                        .arm(&session, &profile, since_ms, state.now_ms);
                    if talaria_core::hivecode::loads_skill(&text) {
                        state.hivecode.skill_sent(&session, epoch);
                    }
                    state.hivecode.model_sent(&session, &text);
                    state.hivecode.note_routed(&session);
                    let cells = opened.state.cells().len();
                    state.hivecode.begin_authoring(
                        &session,
                        &submission_id,
                        &display,
                        cells,
                        since_ms,
                    );
                }
                effects.extend(reduce(
                    state,
                    Event::SubmitCommand {
                        text,
                        display,
                        submission_id,
                    },
                ));
            }
            // A draft typed while the command was in flight is never replaced.
            Action::RestoreComposer(text) => {
                if state.draft.is_empty() {
                    effects.extend(load_draft(state, text));
                }
            }
            Action::Changed => effects.push(AppEffect::Redraw),
            action @ Action::RunCommand { .. } => {
                if clear_composer {
                    clear_submitted_draft(state);
                }
                if let Action::RunCommand { session, .. } = &action {
                    state.hivecode.note_routed(session);
                }
                effects.push(AppEffect::Hivecode {
                    action,
                    request_id: 0,
                });
            }
            action @ Action::FetchCatalog { .. } => {
                let request_id = match &action {
                    Action::FetchCatalog { session, .. } => state
                        .hivecode_requests
                        .get(session)
                        .copied()
                        .unwrap_or_default(),
                    _ => 0,
                };
                effects.push(AppEffect::Hivecode { action, request_id });
            }
            action @ (Action::InstallSkill { .. }
            | Action::ReinstallSkill { .. }
            | Action::CheckSetup { .. }) => {
                // The conversation holds the old instructions; the next routed prompt sends
                // the reinstalled skill whole.
                if let Action::ReinstallSkill { session, .. } = &action {
                    state.hivecode.rearm(session);
                }
                effects.push(AppEffect::Hivecode {
                    action,
                    request_id: 0,
                });
            }
        }
    }
    effects.extend(hivecode_save(state));
    effects
}

/// Save the per-session switch when a setting changed or a session that has one was opened.
fn hivecode_save(state: &mut AppState) -> Option<AppEffect> {
    let changes = state.hivecode.take_saved((state.wall_s * 1000.0) as i64)?;
    state.hivecode_saves += 1;
    Some(AppEffect::HivecodeSave {
        generation: state.hivecode_saves,
        changes,
    })
}

/// Apply a finished command's outcome: notices into the transcript, a directive's prompt
/// through the normal submission path with a fresh id, a refused bare name dimmed.
///
/// A presented command's reply becomes a cell instead of a notice, and the standing state
/// hears that the command finished.
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
    let catalog = state.commands.catalog.clone();
    let presented = automation_actions(typed, &outcome, catalog.as_ref());
    let actions = if presented.is_empty() {
        outcome_actions(outcome.clone(), typed)
    } else {
        presented
    };
    for action in actions {
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
    // The standing state hears about every presented command, whatever it did.
    if let Some(canonical) = canonical_name(typed, catalog.as_ref())
        && let Some(session_id) = state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.clone())
    {
        effects.extend(automation_event(
            state,
            AutomationEvent::CommandFinished {
                session_id,
                canonical,
                result: command_result(&outcome),
            },
        ));
    }
    effects
}

// ---------------------------------------------------------------------------------------------
// The standing automation state: the goal, the loop, the heartbeat and the session modes
// ---------------------------------------------------------------------------------------------

/// Fold one automation event into the standing state, turning its effects into app effects.
pub(crate) fn automation_event(state: &mut AppState, event: AutomationEvent) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for effect in talaria_core::automation::update(&mut state.automation, event) {
        match effect {
            AutomationEffect::ReadControl { .. } | AutomationEffect::ReadMode { .. } => {
                effects.push(AppEffect::Automation(effect));
            }
            AutomationEffect::Notice { text } => effects.extend(notice_effect(state, text)),
            AutomationEffect::Redraw => {}
        }
    }
    // The rows may have changed, so the next countdown is armed now.
    state.automation_wake(Instant::now());
    effects
}

/// One notice in the transcript, or as plain lines when no session is open.
fn notice_effect(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if state.opened.is_some() {
        reduce(
            state,
            Event::Notice {
                kind: NoticeKind::System,
                text,
            },
        )
    } else {
        vec![notice_lines(&text)]
    }
}

/// Route a session event to the standing state when it is one of its events.
///
/// The control update and the status note are ordinary session events, so they arrive on the
/// same path as every other frame. The state drops what belongs to another session.
fn automation_from_session(state: &mut AppState, event: &Event) -> Vec<AppEffect> {
    let Event::Session {
        session_id,
        frame,
        raw,
    } = event
    else {
        return Vec::new();
    };
    automation_from_frame(state, session_id.as_str(), frame, raw)
}

/// Route one session frame to the standing state.
fn automation_from_frame(
    state: &mut AppState,
    session_id: &str,
    frame: &EventFrame,
    raw: &Value,
) -> Vec<AppEffect> {
    let payload = &raw["payload"];
    match frame.type_name().unwrap_or("") {
        "session.control.update" => automation_event(
            state,
            AutomationEvent::ControlUpdated {
                session_id: session_id.to_owned(),
                raw: payload["control"].clone(),
            },
        ),
        "status.update" => automation_event(
            state,
            AutomationEvent::StatusNote {
                session_id: session_id.to_owned(),
                kind: payload["kind"].as_str().unwrap_or_default().to_owned(),
                text: payload["text"].as_str().unwrap_or_default().to_owned(),
            },
        ),
        _ => Vec::new(),
    }
}

/// What a finished command did, as the standing state reads it.
fn command_result(outcome: &CommandOutcome) -> CommandResult {
    match outcome {
        CommandOutcome::Output { .. } => CommandResult::Output,
        CommandOutcome::Submit { .. } => CommandResult::Submit,
        CommandOutcome::OutcomeUnknown { .. } => CommandResult::Unknown,
        CommandOutcome::Refused { .. }
        | CommandOutcome::Failed { .. }
        | CommandOutcome::Prefill { .. }
        | CommandOutcome::Rerun { .. }
        | CommandOutcome::Nothing => CommandResult::NotRun,
    }
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
            if name.is_none() {
                return vec![AppEffect::Choice(choice::ChoiceWork::ListSkins)];
            }
            return vec![AppEffect::Skin(name)];
        }
        LocalAction::Theme => return theme_command(state, argument.trim()),
        LocalAction::Model => parse_model_command(argument).map(ModelCommandOrEffort::Model),
        // Bare `/effort` opens the scale; without a session it answers as before.
        LocalAction::Effort if argument.trim().is_empty() && state.opened.is_some() => {
            return choice::open_effort(state);
        }
        LocalAction::Effort => parse_effort_command(argument).map(ModelCommandOrEffort::Effort),
        LocalAction::Rewind
        | LocalAction::Undo
        | LocalAction::Retry
        | LocalAction::Rollback
        | LocalAction::Diff => return rewind_command(state, action, argument),
        LocalAction::Permissions => return permissions_command(state, argument),
        LocalAction::Cd => return cd_input(state, CdInput::Command(parse_cd_command(argument))),
        LocalAction::Queue => return queue_line(state, queue_command(argument)),
        LocalAction::Steer => return queue_line(state, steer_command(argument)),
        LocalAction::Resume => return resume_command(state, argument),
        LocalAction::Rename => return rename_command(state, argument),
        LocalAction::Branch => return branch_command(state, argument),
        LocalAction::Export => return export_command(state, argument),
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

/// The refusal for a session action while another one runs.
fn session_busy(state: &AppState) -> Option<Vec<AppEffect>> {
    if state.switching {
        return Some(vec![AppEffect::CommitLines(vec![
            SWITCHING_NOTICE.to_owned(),
        ])]);
    }
    if state.session_action || !state.background_after_interrupt.is_empty() {
        return Some(vec![AppEffect::CommitLines(vec![
            SESSION_ACTION_NOTICE.to_owned(),
        ])]);
    }
    None
}

/// Whether the foreground session already is this one.
fn is_foreground(state: &AppState, session_id: &str) -> bool {
    state
        .opened
        .as_ref()
        .is_some_and(|opened| opened.session_id == session_id)
}

/// The foreground session's row.
fn foreground_row(state: &AppState) -> Option<RowKey> {
    state.opened.as_ref().map(|opened| RowKey {
        profile: state
            .opened_profile
            .clone()
            .unwrap_or_else(|| state.profile.clone()),
        session_id: opened.session_id.clone(),
    })
}

pub(crate) fn retained_key(state: &AppState, session_id: &str) -> Option<RowKey> {
    let current = RowKey {
        profile: state.profile.clone(),
        session_id: session_id.to_owned(),
    };
    if state.retained_sessions.contains_key(&current) {
        return Some(current);
    }
    let mut matches = state
        .retained_sessions
        .keys()
        .filter(|key| key.session_id == session_id);
    let key = matches.next()?.clone();
    matches.next().is_none().then_some(key)
}

fn subagent_clock(state: &AppState) -> talaria_core::subagents::Clock {
    talaria_core::subagents::Clock {
        mono_ms: state.now_ms,
        wall_s: wall_ms() as f64 / 1000.0,
    }
}

fn subagent_batches(state: &mut AppState) -> Vec<AppEffect> {
    state.subagents.as_mut().map_or_else(Vec::new, |hub| {
        hub.take_finished_batches()
            .into_iter()
            .map(AppEffect::CommitSubagentBatch)
            .collect()
    })
}

fn attach_subagent_foreground(state: &mut AppState) {
    let key = foreground_row(state);
    let client = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned();
    if let Some(hub) = state.subagents.as_mut() {
        hub.set_foreground(key.clone());
        if let (Some(key), Some(client)) = (key, client) {
            hub.attach(key, client);
        }
    }
}

fn agents_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    use talaria_core::subagents::{AgentsArg, parse_agents_arg};
    let argument = parse_agents_arg(argument);
    if let Some(text) = argument.unknown_text() {
        return vec![notice_lines(&text)];
    }
    let Some(key) = foreground_row(state) else {
        return vec![notice_lines("No session is open.")];
    };
    let now = subagent_clock(state);
    let Some(hub) = state.subagents.as_mut() else {
        return vec![notice_lines("Not connected to the gateway.")];
    };
    match argument {
        AgentsArg::Open => hub.open_panel(key),
        AgentsArg::Pause => {
            hub.open_panel(key.clone());
            if matches!(hub.spawning(), talaria_core::subagents::Spawning::Paused(_)) {
                hub.act(
                    &key,
                    crate::subagents::SubagentAction::ConfirmPause(false),
                    now,
                );
                // An already-paused command must not accidentally resume spawning.
                hub.show_paused();
            } else {
                hub.act(&key, crate::subagents::SubagentAction::RequestPause, now);
            }
        }
        AgentsArg::Resume => hub.resume_command(&key),
        AgentsArg::Unknown(_) => {}
    }
    vec![AppEffect::Redraw]
}

/// `/resume [id]`: the picker without an id, a switch with one.
fn resume_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    let argument = argument.trim();
    if argument.is_empty() {
        return open_picker(state);
    }
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    if is_foreground(state, argument) {
        return vec![AppEffect::CommitLines(vec![ALREADY_HERE_NOTICE.to_owned()])];
    }
    let key = RowKey {
        profile: state.profile.clone(),
        session_id: argument.to_owned(),
    };
    begin_switch(state, key)
}

/// Open the picker overlay and start the listing.
fn open_picker(state: &mut AppState) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    state.picker_generation += 1;
    let generation = state.picker_generation;
    let foreground = foreground_row(state);
    let profiles = state.picker_profiles.clone();
    let scope_note = state.picker_scope_note.clone();
    let mut picker = PickerState::new();
    picker.set_save_folder(talaria_core::workspace::display_path(
        &state.launch_folder,
        state.home.as_deref(),
    ));
    state.picker = Some(picker);
    picker_input(
        state,
        PickerInput::Opened {
            profiles,
            foreground,
            scope_note,
            cached: Vec::new(),
            generation,
        },
    )
}

/// Start a switch: the guards first, then the effect the executor runs.
fn begin_switch(state: &mut AppState, key: RowKey) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    state.switching = true;
    state.session_action = true;
    state.pending_switch = Some(key.clone());
    vec![AppEffect::Switch { key }]
}

/// Start a new session on another served profile in this folder: the guards of a switch
/// first, then the effect the executor runs. The old session keeps running in Hermes.
fn begin_profile_start(state: &mut AppState, profile: String) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    if !state.held.is_empty() {
        return vec![notice_lines(crate::switch::HELD_PROMPTS_TEXT)];
    }
    let cwd = state
        .folder
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.launch_folder.clone());
    state.switching = true;
    state.session_action = true;
    state.pending_switch = Some(RowKey {
        profile: profile.clone(),
        session_id: String::new(),
    });
    vec![AppEffect::StartInProfile { profile, cwd }]
}

/// `/rename [title]`: the foreground session, over the foreground link.
fn rename_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    let title = argument.trim();
    if title.is_empty() {
        let current = foreground_title(state);
        return vec![AppEffect::CommitLines(vec![
            format!("This session is called \"{current}\". Type /rename <new title> to change it."),
            "Give the session a new title:".to_owned(),
        ])];
    }
    let Some(opened) = state.opened.as_ref() else {
        return vec![AppEffect::CommitLines(vec![RENAME_FAILED.to_owned()])];
    };
    let session_id = opened.session_id.clone();
    let mut payload = serde_json::Map::new();
    payload.insert("title".to_owned(), Value::String(title.to_owned()));
    match state.ledger.build(&session_id, "rename", payload, false) {
        Ok(request) => {
            state.session_action = true;
            vec![AppEffect::Mutate {
                request,
                session_id,
            }]
        }
        Err(_) => vec![AppEffect::CommitLines(vec![RENAME_FAILED.to_owned()])],
    }
}

/// The foreground session's title, or `untitled session` when it has none.
fn foreground_title(state: &AppState) -> String {
    state
        .opened
        .as_ref()
        .and_then(|opened| opened.title.clone())
        .map(|title| plain(&title))
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "untitled session".to_owned())
}

/// `/branch [title]`: the foreground session, over the foreground link.
fn branch_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    if turn_running(state) {
        return vec![AppEffect::CommitLines(vec![BRANCH_TURN_RUNNING.to_owned()])];
    }
    let title = argument.trim();
    let title = if title.is_empty() {
        None
    } else {
        Some(title.to_owned())
    };
    start_branch(state, title)
}

/// Build the branch request with the one ledger, so a request id is never used twice, and
/// hand it to the executor.
fn start_branch(state: &mut AppState, title: Option<String>) -> Vec<AppEffect> {
    match crate::switch::branch_request(state, title) {
        Ok(request) => {
            state.switching = true;
            state.session_action = true;
            vec![AppEffect::BranchSession { request }]
        }
        Err(BranchOutcome::Refused { text } | BranchOutcome::Branched { notice: text }) => {
            vec![AppEffect::CommitLines(vec![text])]
        }
    }
}

/// `/export [--no-redact] [file]`: the foreground session, over the side connection.
fn export_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if let Some(refusal) = session_busy(state) {
        return refusal;
    }
    let arguments = match talaria_core::directory::export::parse_export_args(argument) {
        Ok(arguments) => arguments,
        Err(talaria_core::directory::export::ExportArgError::BadExtension) => {
            return vec![AppEffect::CommitLines(vec![
                EXPORT_BAD_EXTENSION.to_owned(),
            ])];
        }
        Err(_) => {
            return vec![AppEffect::CommitLines(vec![EXPORT_USAGE.to_owned()])];
        }
    };
    let Some(key) = foreground_row(state) else {
        return vec![AppEffect::CommitLines(vec![EXPORT_USAGE.to_owned()])];
    };
    let format = match arguments.path.as_deref() {
        Some(path) => std::path::Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(ExportFormat::from_extension)
            .unwrap_or(ExportFormat::Markdown),
        None => ExportFormat::Markdown,
    };
    let target = match arguments.path.as_deref() {
        Some(raw_path) => {
            let path = state.launch_folder.join(raw_path);
            if path.exists() {
                // The name as typed, so `./notes.md` reads back as `./notes.md`.
                return vec![AppEffect::CommitLines(vec![format!(
                    "{raw_path} already exists. Choose another file name."
                )])];
            }
            let Some(folder) = path.parent() else {
                return vec![AppEffect::CommitLines(vec![format!(
                    "The folder for {} does not exist.",
                    path.display()
                )])];
            };
            if !folder.is_dir() {
                return vec![AppEffect::CommitLines(vec![format!(
                    "The folder for {} does not exist.",
                    path.display()
                )])];
            }
            ExportTarget::Explicit { path }
        }
        None => ExportTarget::Auto {
            folder: state.launch_folder.clone(),
        },
    };
    state.session_action = true;
    vec![AppEffect::ExportSession {
        key,
        format,
        redact: arguments.redact,
        target,
    }]
}

/// Apply one picker input and turn its effects into executor effects.
fn picker_input(state: &mut AppState, input: PickerInput) -> Vec<AppEffect> {
    let Some(mut picker) = state.picker.take() else {
        return Vec::new();
    };
    if let PickerInput::Opened {
        profiles,
        scope_note,
        ..
    } = &input
    {
        // The scope the side connection reported is kept, so the next opening of the
        // picker knows which profiles it may reach.
        state.picker_profiles = profiles.clone();
        state.picker_scope_note = scope_note.clone();
    }
    let produced = talaria_core::directory::state::update(&mut picker, input, PICKER_PAGE);
    let mut effects = Vec::new();
    let mut closed = false;
    // Why a switch or branch was refused before anything was sent.
    let mut refused = None;
    for effect in produced {
        match effect {
            PickerEffect::List { generation } => {
                effects.push(AppEffect::ListSessions { generation });
            }
            PickerEffect::Open(key) => {
                if let Some(refusal) = session_busy(state) {
                    refused = last_line(&refusal);
                    effects.extend(refusal);
                    continue;
                }
                if is_foreground(state, &key.session_id) {
                    refused = Some(ALREADY_HERE_NOTICE.to_owned());
                    effects.push(AppEffect::CommitLines(vec![ALREADY_HERE_NOTICE.to_owned()]));
                    continue;
                }
                state.switching = true;
                state.session_action = true;
                state.pending_switch = Some(key.clone());
                effects.push(AppEffect::Switch { key });
            }
            PickerEffect::Rename { key, title } => {
                if let Some(refusal) = session_busy(state) {
                    effects.extend(refusal);
                    continue;
                }
                // A row the ledger has not seen is prepared against revision zero.
                talaria_core::directory::fences::seed_untracked(&mut state.ledger, &key.session_id);
                let mut payload = serde_json::Map::new();
                payload.insert("title".to_owned(), Value::String(title.clone()));
                match state
                    .ledger
                    .build(&key.session_id, "rename", payload, false)
                {
                    Ok(request) => {
                        state.session_action = true;
                        effects.push(AppEffect::RenameRow {
                            key,
                            title,
                            request,
                        });
                    }
                    Err(_) => effects.push(AppEffect::CommitLines(vec![RENAME_FAILED.to_owned()])),
                }
            }
            PickerEffect::Branch { title } => {
                if let Some(refusal) = session_busy(state) {
                    refused = last_line(&refusal);
                    effects.extend(refusal);
                    continue;
                }
                let started = start_branch(state, title);
                if !started
                    .iter()
                    .any(|effect| matches!(effect, AppEffect::BranchSession { .. }))
                {
                    refused = last_line(&started);
                }
                effects.extend(started);
            }
            PickerEffect::Export {
                key,
                format,
                redact,
            } => {
                if let Some(refusal) = session_busy(state) {
                    effects.extend(refusal);
                    continue;
                }
                let target = ExportTarget::Auto {
                    folder: state.launch_folder.clone(),
                };
                state.session_action = true;
                effects.push(AppEffect::ExportSession {
                    key,
                    format,
                    redact,
                    target,
                });
            }
            PickerEffect::Close => {
                closed = true;
                state.picker = None;
                if let Some(notice) = picker.closing_notice() {
                    effects.push(AppEffect::CommitLines(vec![notice.to_owned()]));
                }
                effects.push(AppEffect::CloseSide);
            }
        }
    }
    if !closed {
        // Nothing was sent, so no answer will end the wait.
        if let Some(text) = refused {
            picker.end_wait(text);
        }
        state.picker = Some(picker);
    }
    effects
}

/// The last line these effects commit.
fn last_line(effects: &[AppEffect]) -> Option<String> {
    effects.iter().rev().find_map(|effect| match effect {
        AppEffect::CommitLines(lines) => lines.last().cloned(),
        _ => None,
    })
}

/// Another session is in front: the picker's work is done, so it closes with its side
/// connection.
fn close_picker(state: &mut AppState) -> Vec<AppEffect> {
    if state.picker.take().is_some() {
        vec![AppEffect::CloseSide, AppEffect::Redraw]
    } else {
        Vec::new()
    }
}

/// The switch or branch the picker waited for left the old session in front: back to the
/// list, with why.
fn end_picker_wait(state: &mut AppState, text: &str) {
    if let Some(picker) = state.picker.as_mut() {
        picker.end_wait(text);
    }
}

/// A switch finished: the new session is in front, or the old one stayed.
fn switched(
    state: &mut AppState,
    outcome: SwitchOutcome,
    install: Option<SwitchInstall>,
    reason: Option<String>,
) -> Vec<AppEffect> {
    state.switching = false;
    state.session_action = false;
    let target = state.pending_switch.take();
    let pending_events = std::mem::take(&mut state.pending_switch_events);
    let switched_in = matches!(&outcome, SwitchOutcome::Switched { .. }) && install.is_some();
    let mut effects = match outcome {
        SwitchOutcome::AlreadyHere => {
            end_picker_wait(state, ALREADY_HERE_NOTICE);
            vec![AppEffect::CommitLines(vec![ALREADY_HERE_NOTICE.to_owned()])]
        }
        SwitchOutcome::Failed { text } => {
            end_picker_wait(state, &text);
            let mut lines = Vec::new();
            if state.workflow_return.is_some() {
                // One line, and the person stays in the agent's session.
                let because = reason
                    .as_deref()
                    .map_or_else(String::new, |reason| format!(" ({reason})"));
                lines.push(format!(
                    "Could not go back to the session you came from{because}. You are still in the agent's session."
                ));
            } else {
                if reason.as_deref() == Some("not_found")
                    && let Some(target) = target
                {
                    lines.push(format!(
                        "No session with the id {} in the profile {}. Type /resume to pick from a list.",
                        plain(&target.session_id),
                        plain(&target.profile)
                    ));
                }
                lines.push(text);
            }
            vec![AppEffect::CommitLines(lines)]
        }
        SwitchOutcome::Switched { notice } => {
            // The notice follows the new session's replayed history, so it is not buried.
            let mut effects = Vec::new();
            match install {
                Some(install) => {
                    if let Some(target) = target {
                        if state.appearance.set_file_profile(&target.profile) {
                            effects.push(AppEffect::RefreshThemeProfile);
                        }
                        if state.profile != target.profile {
                            forget_profile_views(state);
                            // Approval notices name the profile they are about.
                            state.permissions = Some(PermissionsState::new(target.profile.clone()));
                        }
                        state.profile = target.profile;
                    }
                    effects.extend(install_session(state, install, Some(notice)));
                }
                None => effects.push(notice_lines(&notice)),
            }
            effects.extend(close_picker(state));
            effects
        }
    };
    effects.extend(workflow_origin_settle(state, switched_in));
    for event in pending_events {
        effects.extend(link_event(
            state,
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                replayed: false,
            },
        ));
    }
    effects
}

/// A branch finished: the child is in front, or the parent stayed.
fn branched(
    state: &mut AppState,
    outcome: BranchOutcome,
    install: Option<SwitchInstall>,
) -> Vec<AppEffect> {
    state.switching = false;
    state.session_action = false;
    match outcome {
        BranchOutcome::Branched { notice } => {
            // The notice follows the child's replayed history, so the copied messages do not
            // bury it.
            let mut effects = Vec::new();
            let parent = state
                .opened
                .as_ref()
                .map(|opened| SessionId::new(opened.session_id.clone()));
            match install {
                Some(install) => {
                    let child = SessionId::new(install.opened.session_id.clone());
                    // Hivecode follows the branch, so a held prompt in the child is routed too.
                    let carried = parent
                        .as_ref()
                        .is_some_and(|parent| state.hivecode.carry(parent, &child));
                    if carried {
                        // The branch notice below says it; the open does not repeat it.
                        state.hivecode.mark_announced(&child);
                    }
                    effects.extend(install_session(state, install, Some(notice)));
                    if carried {
                        effects.push(notice_lines(talaria_core::hivecode::notice::BRANCH_CARRIED));
                    }
                }
                None => effects.push(notice_lines(&notice)),
            }
            effects.extend(close_picker(state));
            effects
        }
        BranchOutcome::Refused { text } => {
            end_picker_wait(state, &text);
            vec![AppEffect::CommitLines(vec![text])]
        }
    }
}

/// A branch request came back: settle it, try once more after a fence refusal when the turn
/// did not change, open the child after a commit, or say why Hermes refused.
fn branch_sent(
    state: &mut AppState,
    request: MutationRequest,
    outcome: MutationOutcome,
) -> Vec<AppEffect> {
    state.ledger.settle(&request, &outcome);
    if let MutationOutcome::FenceRefused { .. } = &outcome
        && talaria_core::directory::fences::branch_may_retry(
            request.expected_generation.unwrap_or_default(),
            &outcome,
        )
        && let Some(retry) = state.ledger.retry_after_fence(&request, &outcome)
    {
        return vec![AppEffect::BranchSession { request: retry }];
    }
    match outcome {
        MutationOutcome::Committed(receipt) => {
            let name = request
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned);
            vec![AppEffect::OpenBranchChild { receipt, name }]
        }
        outcome => branched(state, crate::switch::branch_refusal(&outcome), None),
    }
}

/// Put the new session in front: the link, the service, the fences and the history.
/// Forward one link's events to the loop until a later link replaces it.
///
/// `started_for` is the value [`AppState::link_epoch`] has while this link is the foreground
/// link. A replaced link still reports its own closing, and the loop cannot tell which link an
/// event came from, so without this check the status line said "disconnected" after every switch
/// to another profile. A server request still queued on a replaced link is released, not
/// refused, so other viewers of that session keep their prompt.
pub(crate) fn forward_link_events(
    epoch: Arc<AtomicU64>,
    started_for: u64,
    sender: mpsc::UnboundedSender<AppMessage>,
    mut inbound: mpsc::Receiver<LinkEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = inbound.recv().await {
            if epoch.load(Ordering::SeqCst) > started_for {
                if let LinkEvent::Inbound {
                    item: Inbound::ServerRequest(request),
                    ..
                } = event
                {
                    request.responder.ignore();
                }
                continue;
            }
            if sender.send(AppMessage::Link(event)).is_err() {
                break;
            }
        }
    })
}

#[cfg(test)]
mod link_forward_tests {
    use super::*;

    #[tokio::test]
    async fn a_replaced_link_stops_forwarding_and_the_new_link_keeps_forwarding() {
        let epoch = Arc::new(AtomicU64::new(0));
        let (sender, mut loop_inbox) = mpsc::unbounded_channel();
        let (old_link, old_events) = mpsc::channel(8);
        let old = forward_link_events(Arc::clone(&epoch), 0, sender.clone(), old_events);
        // The switch starts the new link's forwarder before the loop installs the link.
        let (new_link, new_events) = mpsc::channel(8);
        let new = forward_link_events(Arc::clone(&epoch), 1, sender, new_events);

        old_link
            .send(LinkEvent::State(LinkState::Connected { generation: 1 }))
            .await
            .unwrap();
        assert!(matches!(
            loop_inbox.recv().await,
            Some(AppMessage::Link(LinkEvent::State(LinkState::Connected {
                generation: 1
            })))
        ));

        // The loop installs the new link, then closes the old one.
        epoch.fetch_add(1, Ordering::SeqCst);
        old_link
            .send(LinkEvent::State(LinkState::Closed))
            .await
            .unwrap();
        drop(old_link);
        old.await.unwrap();
        assert!(
            loop_inbox.try_recv().is_err(),
            "the replaced link's closing must not reach the loop"
        );

        new_link
            .send(LinkEvent::State(LinkState::Connected { generation: 1 }))
            .await
            .unwrap();
        drop(new_link);
        new.await.unwrap();
        assert!(matches!(
            loop_inbox.try_recv(),
            Ok(AppMessage::Link(LinkEvent::State(LinkState::Connected {
                generation: 1
            })))
        ));
    }
}

fn install_session(
    state: &mut AppState,
    install: SwitchInstall,
    notice: Option<String>,
) -> Vec<AppEffect> {
    let SwitchInstall {
        opened,
        service,
        link,
    } = install;
    let mut effects = Vec::new();
    if link.is_some() {
        state.mcp_binding = state.mcp_binding.saturating_add(1);
        state.mcp_preparing = false;
        state.mcp_waiting.clear();
        state.mcp = talaria_core::mcp::McpState::default();
        if let Some(service) = state.mcp_service.take() {
            effects.push(AppEffect::CloseMcp(service));
        }
        // A new profile connection starts its own client generation at one.
        state.background_generation_base = state.connection_generation.saturating_add(1);
        state.connection_generation = state
            .background_generation_base
            .saturating_add(service.client().map_or(0, |client| client.generation().0));
    }
    state.background_service = service
        .client()
        .cloned()
        .map(crate::background::BackgroundService::new)
        .map(Arc::new);
    state.service = Some(service);
    if let Some(link) = link {
        // From here the replaced link's events, its own closing among them, are not this
        // connection's: its forwarder stops passing them on.
        state.link_epoch.fetch_add(1, Ordering::SeqCst);
        if let Some(old) = state.link.replace(link) {
            effects.push(AppEffect::CloseLink(old));
        }
        // Before the session opens, so its first approval read goes over the new link.
        effects.push(AppEffect::BindLinkServices);
        let counter = Arc::clone(&state.counter);
        let launch_id = state.launch_id.clone();
        state.prompts = Some(Arc::new(Mutex::new(PromptService::new(Box::new(
            move || submission_id(&launch_id, counter.fetch_add(1, Ordering::SeqCst) + 1),
        )))));
    }
    effects.extend(opened_session(state, opened, notice));
    effects.extend(prepare_cron(state, true));
    effects
}

/// A mutation over the foreground link finished.
fn mutated(
    state: &mut AppState,
    request: MutationRequest,
    outcome: MutationOutcome,
) -> Vec<AppEffect> {
    state.ledger.settle(&request, &outcome);
    match &outcome {
        MutationOutcome::Committed(result) => {
            state.session_action = false;
            let title = match result.get("title").and_then(Value::as_str) {
                Some(title) if !title.trim().is_empty() => title.to_owned(),
                _ => request
                    .payload
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            };
            if let Some(opened) = state.opened.as_mut()
                && opened.session_id == request.session_id
            {
                opened.title = Some(title.clone());
            }
            vec![AppEffect::CommitLines(vec![format!(
                "Renamed to \"{title}\"."
            )])]
        }
        MutationOutcome::FenceRefused { .. } => {
            // The session changed under the rename: one more try at the fences Hermes
            // named, and no more than that.
            if let Some(retry) = state.ledger.retry_after_fence(&request, &outcome) {
                return vec![AppEffect::Mutate {
                    request: retry,
                    session_id: request.session_id.clone(),
                }];
            }
            state.session_action = false;
            vec![AppEffect::CommitLines(vec![RENAME_FENCE.to_owned()])]
        }
        MutationOutcome::Refused { rpc_code, .. } if *rpc_code == -32603 => {
            state.session_action = false;
            vec![AppEffect::CommitLines(vec![RENAME_TAKEN.to_owned()])]
        }
        _ => {
            state.session_action = false;
            vec![AppEffect::CommitLines(vec![RENAME_FAILED.to_owned()])]
        }
    }
}

/// A rename over the side connection finished.
fn row_renamed(
    state: &mut AppState,
    key: RowKey,
    title: String,
    request: MutationRequest,
    outcome: Result<MutationOutcome, String>,
) -> Vec<AppEffect> {
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(text) => {
            state.session_action = false;
            return vec![AppEffect::CommitLines(vec![text])];
        }
    };
    state.ledger.settle(&request, &outcome);
    // The session changed under the rename: one more try at the fences Hermes named.
    if let MutationOutcome::FenceRefused { .. } = &outcome
        && let Some(retry) = state.ledger.retry_after_fence(&request, &outcome)
    {
        return vec![AppEffect::RenameRow {
            key,
            title,
            request: retry,
        }];
    }
    state.session_action = false;
    let title = match crate::directory::DirectoryService::rename_result(&outcome, &title) {
        crate::directory::RenameResult::Renamed { title, .. } => title,
        crate::directory::RenameResult::Failed { text } => {
            return vec![AppEffect::CommitLines(vec![text])];
        }
    };
    // Renaming the session in front from the picker renames it everywhere.
    if key.profile == state.profile
        && let Some(opened) = state.opened.as_mut()
        && opened.session_id == key.session_id
    {
        opened.title = Some(title.clone());
    }
    if let Some(picker) = state.picker.as_mut() {
        let _ = talaria_core::directory::state::update(
            picker,
            PickerInput::Renamed {
                key,
                title: title.clone(),
            },
            PICKER_PAGE,
        );
    }
    vec![AppEffect::CommitLines(vec![format!(
        "Renamed to \"{title}\"."
    )])]
}

/// An export finished.
fn exported(
    state: &mut AppState,
    result: ExportResult,
    format: ExportFormat,
    redact: bool,
) -> Vec<AppEffect> {
    state.session_action = false;
    match result {
        ExportResult::Written { path, bytes } => {
            if bytes == 0 {
                // Hermes answered without any content: the answer was not read, and
                // nothing is left behind.
                let _ = std::fs::remove_file(&path);
                return vec![AppEffect::CommitLines(vec![
                    "Hermes's export answer could not be read, so nothing was written.".to_owned(),
                ])];
            }
            let title = foreground_title(state);
            let secrets = if redact {
                "secrets redacted"
            } else {
                "secrets included"
            };
            // A file in the launch folder is named from it, as `./<name>`.
            let shown = match path.strip_prefix(&state.launch_folder) {
                Ok(relative) => {
                    let relative = relative.strip_prefix(".").unwrap_or(relative);
                    format!("./{}", relative.display())
                }
                Err(_) => path.display().to_string(),
            };
            vec![AppEffect::CommitLines(vec![format!(
                "Exported \"{title}\" to {shown} ({}, {}, {secrets}).",
                talaria_core::directory::export::format_size(bytes),
                format.label()
            )])]
        }
        ExportResult::Failed { text } => {
            vec![AppEffect::CommitLines(vec![export_failure_text(&text)])]
        }
    }
}

/// The export's failure text, by the reason the side connection reported.
fn export_failure_text(text: &str) -> String {
    let reason = text
        .strip_prefix("Hermes could not export the session: ")
        .unwrap_or(text)
        .trim();
    match reason {
        "not_found" => {
            "This session has nothing to export yet: Hermes has no saved messages for it."
                .to_owned()
        }
        "export_too_large" => {
            "This session is too large to export as JSON. Export it as Markdown instead."
                .to_owned()
        }
        "unknown_method" => {
            "This Hermes gateway has no session.export. Ask the host to upgrade its Hermes."
                .to_owned()
        }
        "the reply was too large" => {
            "Hermes's export was too large to hold. Export it as Markdown instead, or in smaller pieces."
                .to_owned()
        }
        "no answer within 10 s" | "connection closed" => {
            "No answer from Hermes within 60 seconds, or the connection dropped. Run /export again."
                .to_owned()
        }
        other => format!("Hermes could not export this session ({other})."),
    }
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
    super_key: bool,
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
            super_key: false,
        };
        for modifier in parts {
            match modifier.to_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                "super" => chord.super_key = true,
                _ => return None,
            }
        }
        chord.code = key_code(key, &mut chord.shift)?;
        Some(chord)
    }

    /// Whether this chord is the key.
    fn matches(&self, key: &InputKey) -> bool {
        if self.ctrl != key.ctrl
            || self.alt != key.alt
            || self.super_key != key.super_key
            || key.other
        {
            return false;
        }
        match (self.code, key.code) {
            (InputCode::Char(want), InputCode::Char(got)) => {
                let shifted = key.shift || got.is_uppercase();
                lower(got) == want && self.shift == shifted
            }
            (want, got) => want == got && self.shift == key.shift,
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
        "tab" => InputCode::Tab,
        "delete" => InputCode::Delete,
        "insert" => InputCode::Insert,
        "up" => InputCode::Up,
        "down" => InputCode::Down,
        "left" => InputCode::Left,
        "right" => InputCode::Right,
        "pageup" => InputCode::PageUp,
        "pagedown" => InputCode::PageDown,
        "home" => InputCode::Home,
        "end" => InputCode::End,
        "space" => InputCode::Char(' '),
        "plus" => InputCode::Char('+'),
        _ => {
            if let Some(number) = name
                .strip_prefix('f')
                .and_then(|number| number.parse::<u8>().ok())
                && (1..=24).contains(&number)
                && !name[1..].starts_with('0')
            {
                return Some(InputCode::F(number));
            }
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
    let interrupt = key.ctrl
        && !key.alt
        && !key.super_key
        && !key.other
        && matches!(key.code, InputCode::Char('c'));

    if !state.pager_open {
        if chords(state, "toggle_menu_bar", "alt+i")
            .iter()
            .any(|chord| chord.matches(&key))
        {
            return user_action(state, UserAction::ToggleMenuBar);
        }
        if state.layout.menu_focused {
            if key.code == InputCode::Esc && !key.ctrl && !key.alt && !key.super_key && !key.other {
                if state.layout.menu_dropdown {
                    state.layout.menu_dropdown = false;
                } else {
                    state.layout.menu_focused = false;
                    state.layout.menu_transient = false;
                }
            }
            return vec![AppEffect::Redraw];
        }
        if chords(state, "layout_panel", "alt+d")
            .iter()
            .any(|chord| chord.matches(&key))
        {
            return user_action(state, UserAction::LayoutPanel);
        }
        if key.code == InputCode::Esc
            && !key.ctrl
            && !key.alt
            && !key.super_key
            && !key.other
            && state.layout.panel == talaria_core::layout::PanelVisibility::OverlayOpen
        {
            return user_action(state, UserAction::DismissWorkPanel);
        }
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
            InputCode::Char(_)
                if key.search_character().is_some() && state.pager_typed_len < 256 =>
            {
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
    let bypassed = state
        .opened
        .as_ref()
        .filter(|_| !state.session_gone)
        .and_then(|opened| {
            let on = state
                .hivecode
                .status_input(Some(&SessionId::new(opened.session_id.clone())));
            talaria_core::hivecode::bypass(&text, on)
        });
    let mut effects = submit_line(state, text);
    // Only a command that went somewhere gets the notice; a refused one is back in the composer.
    if let Some(command) = bypassed
        && effects.iter().any(|effect| {
            !matches!(
                effect,
                AppEffect::CommitLines(_) | AppEffect::LoadDraft(_) | AppEffect::Redraw
            )
        })
    {
        effects.insert(
            0,
            notice_lines(&talaria_core::hivecode::notice::bypassed(&command)),
        );
    }
    effects
}

/// The model and provider the open session runs on, for a Hivecode-routed prompt; the model
/// the user switched to wins over the one the session started with.
fn session_model(state: &AppState) -> Option<(String, String)> {
    state
        .model
        .clone()
        .or_else(|| state.opened.as_ref().and_then(|o| o.info.model.clone()))
        .zip(state.opened.as_ref().and_then(|o| o.info.provider.clone()))
}

fn submit_line(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if text.trim().is_empty() {
        if let Some(session) = state
            .opened
            .as_ref()
            .filter(|_| !state.session_gone)
            .map(|opened| SessionId::new(opened.session_id.clone()))
            && let Some(actions) = state.hivecode.answer_install(&session, &text)
        {
            return hivecode_actions(state, actions);
        }
        return Vec::new();
    }
    if let Some(Ok((LocalAction::Configure, request))) = local_line(&text) {
        return configure_command(state, request, text);
    }
    if talaria_core::settings::is_secret_config_line(&text) {
        let args = text
            .trim_start()
            .strip_prefix("/config")
            .unwrap_or_default()
            .to_owned();
        let profile = state.profile.clone();
        return settings_input(
            state,
            talaria_core::settings::SettingsInput::Command { profile, args },
            Some(text),
        );
    }
    // A switch is running: the prompt stays in the composer and nothing is sent.
    if state.switching {
        return vec![
            AppEffect::CommitLines(vec![SWITCHING_NOTICE.to_owned()]),
            AppEffect::LoadDraft(text),
        ];
    }
    if let Some(rest) = text.trim().strip_prefix("/fleet ")
        && !rest.trim().is_empty()
    {
        return vec![notice_lines(FLEET_ARGUMENT), AppEffect::LoadDraft(text)];
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
            let argument = match state.attachments.expand_pastes(&argument) {
                Ok(argument) => argument,
                Err(notice) => return vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
            };
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
        Slash::Command(line) => {
            let command = match state.attachments.expand_pastes(&line) {
                Ok(command) => command.into_owned(),
                Err(notice) => return vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
            };
            return run_command(state, command);
        }
        Slash::Nothing => return Vec::new(),
        Slash::NotSlash => {}
    }
    if let Some(session) = state
        .opened
        .as_ref()
        .filter(|_| !state.session_gone)
        .map(|s| SessionId::new(s.session_id.clone()))
    {
        state.hivecode.bind(&session, &state.profile);
        if let Some(actions) = state.hivecode.answer_install(&session, &text) {
            return hivecode_actions(state, actions);
        }
        hivecode_note_conversation(state, &session);
        let session_model = session_model(state);
        let request = match state.attachments.expand_pastes(&text) {
            Ok(request) => request,
            Err(notice) => return vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
        };
        if let Some(actions) = state.hivecode.intercept_submit_with_model(
            &session,
            &text,
            &request,
            session_model
                .as_ref()
                .map(|(model, provider)| (model.as_str(), provider.as_str())),
        ) {
            // A prompt sent with the reminder goes straight to Hermes, like a plain one.
            if actions
                .iter()
                .any(|action| matches!(action, crate::hivecode::Action::SubmitCommand { .. }))
            {
                if state.link_failed {
                    return vec![
                        notice_lines(LINK_STOPPED_NOTICE),
                        AppEffect::LoadDraft(text),
                    ];
                }
                clear_submitted_draft(state);
            }
            let mut effects = hivecode_actions(state, actions);
            // A routed prompt checks the local setup on the side; the prompt does not wait.
            if state.hivecode.status_input(Some(&session))
                && let Some(check) = state.hivecode.setup_check(&session)
            {
                effects.extend(apply_hivecode(state, vec![check], false));
            }
            return effects;
        }
    }
    submit_prose(state, text)
}

/// Run what `/queue` or `/steer` asked for: an intent, a plain submit, or a notice (#232).
fn queue_line(state: &mut AppState, command: QueueCommand) -> Vec<AppEffect> {
    match command {
        QueueCommand::Intent(intent) => queue_intent(state, intent),
        QueueCommand::Submit(text) => submit_prose(state, text),
        QueueCommand::Notice(text) => match state.opened {
            Some(_) => reduce(
                state,
                Event::Notice {
                    kind: NoticeKind::System,
                    text: text.to_string(),
                },
            ),
            None => vec![notice_lines(text)],
        },
    }
}

/// Apply one queue intent to the open session; its calls go to the queue service (#232).
fn queue_intent(state: &mut AppState, mut intent: QueueIntent) -> Vec<AppEffect> {
    let draft = match &intent {
        QueueIntent::SendDraftNow(text) | QueueIntent::SteerText(text) => Some(text.clone()),
        _ => None,
    };
    let mut ids = QueueIds {
        launch_id: state.launch_id.clone(),
        counter: Arc::clone(&state.counter),
    };
    let Some(opened) = state.opened.as_mut() else {
        return Vec::new();
    };
    if !opened.state.queue.busy()
        && let QueueIntent::SendDraftNow(text)
        | QueueIntent::SteerText(text)
        | QueueIntent::SaveEdit(text) = &mut intent
    {
        match state.attachments.expand_pastes(text) {
            Ok(std::borrow::Cow::Owned(expanded)) => *text = expanded,
            Ok(std::borrow::Cow::Borrowed(_)) => {}
            Err(notice) => return vec![notice_lines(&notice)],
        }
    }
    let produced = update_queue(&mut opened.state, intent, &mut ids);
    let accepted = produced
        .iter()
        .any(|effect| matches!(effect, Effect::SendPrompt { .. } | Effect::Steer { .. }));
    let mut effects = route(state, produced, None);
    if accepted && let Some(text) = draft {
        effects.push(AppEffect::ClearSubmittedDraft(text));
    }
    effects
}

/// The reconcile event a lost queue reply schedules, if `event` is one.
fn reconcile_due(event: &Event) -> Option<Event> {
    match event {
        Event::QueueCallUnknown {
            action,
            admission_id,
            submission_id,
        } => Some(Event::QueueReconcileDue {
            action: *action,
            admission_id: admission_id.clone(),
            submission_id: submission_id.clone(),
        }),
        _ => None,
    }
}

/// Fresh submission ids for queue calls, from the same per-launch counter as prompts.
struct QueueIds {
    launch_id: String,
    counter: Arc<AtomicU64>,
}

impl IdSource for QueueIds {
    fn submission_id(&mut self) -> SubmissionId {
        SubmissionId(submission_id(
            &self.launch_id,
            self.counter.fetch_add(1, Ordering::SeqCst) + 1,
        ))
    }
}

/// Text for the model: held before the session exists, refused when the link is gone.
fn submit_prose(state: &mut AppState, text: String) -> Vec<AppEffect> {
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
            Ok(()) => {
                state
                    .attachments
                    .set_workspace(active_attachment_workspace(state));
                match state.attachments.on_submit(&text) {
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
                }
            }
            Err(notice) => vec![notice_lines(&notice), AppEffect::LoadDraft(text)],
        };
    }
    // A workflow agent's session is not an ordinary conversation: say so once, keep the text.
    if let Some(agent) = state.workflow_agent.as_mut()
        && let Some(warning) = agent.first_send_warning(&mut state.workflow_agent_warned)
    {
        return vec![notice_lines(&warning), AppEffect::LoadDraft(text)];
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

fn active_attachment_workspace(state: &AppState) -> PathBuf {
    state
        .folder
        .cwd
        .as_deref()
        .or_else(|| {
            state
                .opened
                .as_ref()
                .and_then(|opened| opened.info.cwd.as_deref())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| state.attachment_folder.clone())
}

fn send_text(state: &mut AppState, text: String) -> Vec<AppEffect> {
    state
        .attachments
        .set_workspace(active_attachment_workspace(state));
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

/// `/theme` with its argument: the report and selector, or a theme preference command.
fn theme_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if argument.is_empty() {
        return vec![AppEffect::Choice(choice::ChoiceWork::ListThemes)];
    }
    if argument == "report" {
        return vec![AppEffect::ThemeReport];
    }
    let background = matches!(argument, "light" | "dark" | "auto");
    if matches!(argument.split_whitespace().next(), Some("use" | "reload"))
        || (background && (state.appearance.theme_files().is_some() || state.home.is_some()))
    {
        return vec![AppEffect::ThemeFileCommand(argument.to_owned())];
    }
    let (lines, change) = state.appearance.theme_command(argument);
    let mut effects = vec![notice_lines(&lines.join("\n"))];
    if background {
        effects.push(notice_lines(
            "The background choice could not be saved because the home directory is unknown.",
        ));
    }
    if change != AppearanceChange::Unchanged {
        effects.push(AppEffect::Appearance);
    }
    effects
}

/// The foreground owns requests for its opened and retained sessions, as well as requests
/// without a session id.
fn foreground_owns_request(state: &AppState, session_id: Option<&str>) -> bool {
    match session_id.map(str::trim).filter(|id| !id.is_empty()) {
        None => true,
        Some(id) => {
            state
                .opened
                .as_ref()
                .is_some_and(|opened| opened.session_id == id)
                || retained_key(state, id).is_some()
        }
    }
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

fn enqueue_server_request(state: &AppState, executor: &Executor, request: ServerRequest) {
    if !foreground_owns_request(state, request.session_id.as_deref()) {
        request.responder.ignore();
        return;
    }
    let Some(prompts) = &state.prompts else {
        return;
    };
    executor.prompt_task(prompts, move |prompts| async move {
        let notices = prompts
            .lock()
            .await
            .on_server_request_for_session(request, None)
            .await;
        (Vec::new(), notice_texts(notices))
    });
}

fn enqueue_cancel_request(
    state: &AppState,
    executor: &Executor,
    id: String,
    method: String,
    reason: String,
) {
    let recovered = state.opened.as_ref().is_some_and(|opened| {
        opened.state.open_prompts().any(|prompt| {
            prompt.kind == PromptKind::ServerRequest
                && prompt.state == PromptState::Open
                && prompt.key.prompt_id == id
        })
    });
    if recovered {
        let id = id.clone();
        let reason = reason.clone();
        executor.send_later(async move {
            AppMessage::PromptDone {
                events: vec![Event::ServerRequestCancelled { id }],
                notices: vec![crate::prompts::cancel_reason_notice(&reason)],
                masked: None,
            }
        });
    }
    let Some(prompts) = &state.prompts else {
        return;
    };
    executor.prompt_task(prompts, move |prompts| async move {
        let notices = prompts
            .lock()
            .await
            .on_request_cancel(&id, &method, &reason);
        (Vec::new(), notice_texts(notices))
    });
}

fn finish_request_cancel(
    mut effects: Vec<AppEffect>,
    cancel: &mut Option<AppEffect>,
) -> Vec<AppEffect> {
    if let Some(cancel) = cancel.take()
        && !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::CancelRequest { .. }))
    {
        effects.insert(0, cancel);
    }
    effects
}

fn request_cancel_effect(frame: &hermes_contract::events::EventFrame) -> Option<AppEffect> {
    match &frame.notification {
        hermes_contract::events::Notification::RequestCancel(payload) => {
            Some(AppEffect::CancelRequest {
                id: payload.id.clone(),
                method: payload.method.clone(),
                reason: payload.reason.clone(),
            })
        }
        _ => None,
    }
}

fn is_open_session(state: &AppState, session_id: &str) -> bool {
    state
        .opened
        .as_ref()
        .is_some_and(|opened| opened.session_id == session_id)
}

fn link_event(state: &mut AppState, event: LinkEvent) -> Vec<AppEffect> {
    let now_ms = state.now_ms;
    match event {
        LinkEvent::State(LinkState::Connected { generation }) => {
            state.connection_generation = state
                .connection_generation
                .max(state.background_generation_base.saturating_add(generation));
            state.link_state = LinkView::Connected;
            let mut effects = vec![AppEffect::ConnectionOpened(generation)];
            if let Some(text) = talaria_core::doctor::connect_notice(
                state.gateway_version.as_deref(),
                &hermes_contract::compat::CURRENT,
                &mut state.compat_notice_shown,
            ) {
                effects.extend(reduce(
                    state,
                    Event::Notice {
                        kind: talaria_core::session::NoticeKind::Warning,
                        text,
                    },
                ));
            }
            if state.uses_client_voice() {
                let _ = state
                    .client_voice
                    .update(talaria_core::voice::client::Input::Reopen);
            }
            let new_connection = state.voice_connection_generation != state.connection_generation;
            state.voice_connection_generation = state.connection_generation;
            if new_connection && !state.voice.session_ids().is_empty() {
                state.voice_submit_mode = None;
                state.voice_config_pending = false;
                for session in state.voice.session_ids() {
                    effects.extend(voice_input(
                        state,
                        &session,
                        talaria_core::voice::VoiceInput::ConnectionOpened,
                    ));
                    effects.push(AppEffect::Voice {
                        session_id: session,
                        effect: talaria_core::voice::VoiceEffect::ReadSubmitMode,
                    });
                }
            }
            effects.extend(prepare_cron(state, true));
            // A reconnect describes again: the skin may have changed while the link was down.
            if generation >= 2 {
                effects.push(AppEffect::Describe(generation));
                let foreground = foreground_row(state);
                if let (Some(hub), Some(key)) = (state.subagents.as_mut(), foreground) {
                    hub.on_reconnected(&key);
                }
            }
            if let Some(opened) = state.opened.as_mut() {
                let produced = note_connection(&mut opened.state, generation);
                effects.extend(route(state, produced, None));
            }
            effects.extend(commands_follow(state, CommandsInput::Connected));
            effects.extend(permissions(state, PermissionsInput::ConnectionOpened));
            effects.extend(models_input(
                state,
                ModelsInput::Connection { connected: true },
            ));
            if state.providers.is_open() {
                effects.extend(providers_input(
                    state,
                    ProvidersInput::Connection { connected: true },
                ));
            }
            let connected = state.mcp_service.is_some();
            effects.extend(mcp_input(
                state,
                talaria_core::mcp::McpInput::Connection { connected },
            ));
            if state.mcp.is_open() && state.mcp_service.is_none() && !state.mcp_preparing {
                effects.push(AppEffect::PrepareMcp);
            }
            effects
        }
        LinkEvent::State(LinkState::Reconnecting { .. }) => {
            state.link_state = LinkView::Reconnecting;
            let mut effects =
                providers_input(state, ProvidersInput::Connection { connected: false });
            client_voice_link_lost(state, &mut effects);
            effects.extend(models_input(
                state,
                ModelsInput::Connection { connected: false },
            ));
            effects.extend(mcp_input(
                state,
                talaria_core::mcp::McpInput::Connection { connected: false },
            ));
            effects.extend(cron_input(state, CronInput::Connection { up: false }));
            effects.extend(configure_input(
                state,
                talaria_core::configure::ConfigureInput::LinkLost,
                None,
            ));
            effects
        }
        LinkEvent::State(LinkState::Closed) => {
            state.link_state = LinkView::Closed;
            let mut effects =
                providers_input(state, ProvidersInput::Connection { connected: false });
            client_voice_link_lost(state, &mut effects);
            effects.extend(models_input(
                state,
                ModelsInput::Connection { connected: false },
            ));
            effects.extend(mcp_input(
                state,
                talaria_core::mcp::McpInput::Connection { connected: false },
            ));
            effects.extend(cron_input(state, CronInput::Connection { up: false }));
            effects.extend(configure_input(
                state,
                talaria_core::configure::ConfigureInput::LinkLost,
                None,
            ));
            effects
        }
        event @ LinkEvent::State(LinkState::Failed { .. }) => {
            state.link_state = LinkView::Failed;
            state.link_failed = true;
            abandon_foreground_turn(state);
            let mut effects =
                providers_input(state, ProvidersInput::Connection { connected: false });
            client_voice_link_lost(state, &mut effects);
            effects.extend(models_input(
                state,
                ModelsInput::Connection { connected: false },
            ));
            effects.extend(mcp_input(
                state,
                talaria_core::mcp::McpInput::Connection { connected: false },
            ));
            effects.extend(cron_input(state, CronInput::Connection { up: false }));
            effects.extend(configure_input(
                state,
                talaria_core::configure::ConfigureInput::LinkLost,
                None,
            ));
            if let Some(text) = link_notice(&event) {
                effects.push(notice_lines(&text));
            }
            effects
        }
        LinkEvent::Inbound { item, .. } => match item {
            // A skill root moved: refetch the catalogue. It is not a session event.
            Inbound::Event(inbound)
                if inbound.raw.get("type").and_then(Value::as_str) == Some("skills.changed") =>
            {
                let mut effects = commands_follow(state, CommandsInput::SkillsChanged);
                effects.extend(capabilities_input(state, CapInput::SkillsChanged));
                effects
            }
            Inbound::Event(mut inbound) => {
                bind_approval_cancelled_session(&mut inbound.raw);
                let mut request_cancel = request_cancel_effect(&inbound.frame);
                if state.pending_switch.as_ref().is_some_and(|target| {
                    target.profile == state.profile
                        && inbound.raw.get("session_id").and_then(Value::as_str)
                            == Some(target.session_id.as_str())
                        && !is_open_session(state, &target.session_id)
                }) {
                    state.pending_switch_events.push(inbound);
                    return finish_request_cancel(Vec::new(), &mut request_cancel);
                }
                if inbound
                    .raw
                    .get("session_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| {
                        state
                            .configure
                            .as_ref()
                            .and_then(|state| state.hidden_session_id())
                            == Some(id)
                    })
                {
                    let session_id = inbound
                        .raw
                        .get("session_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into();
                    let event = Event::Session {
                        session_id,
                        frame: inbound.frame,
                        raw: inbound.raw,
                    };
                    if let Some(service) = &state.configure_service {
                        service.observe(&event);
                    }
                    return finish_request_cancel(
                        configure_input(
                            state,
                            talaria_core::configure::ConfigureInput::Session(event),
                            None,
                        ),
                        &mut request_cancel,
                    );
                }
                if let Some(opened) = state.opened.as_ref()
                    && inbound.raw.get("session_id").and_then(Value::as_str)
                        == Some(opened.session_id.as_str())
                    && !accept_event(
                        &opened.cursor,
                        inbound.raw.get("replay_epoch").and_then(Value::as_str),
                        inbound.raw.get("seq").and_then(Value::as_u64),
                    )
                {
                    return finish_request_cancel(Vec::new(), &mut request_cancel);
                }
                // Child events remain independent of the parent's interrupt latch.
                let key = inbound
                    .raw
                    .get("session_id")
                    .and_then(Value::as_str)
                    .and_then(|session_id| {
                        state.subagents.as_ref().and_then(|hub| {
                            hub.attached_keys()
                                .into_iter()
                                .find(|key| key.session_id == session_id)
                        })
                    });
                let now = subagent_clock(state);
                let hub_changed = if let (Some(hub), Some(key)) = (state.subagents.as_mut(), key) {
                    hub.on_notification(&key, &inbound.frame.notification, now)
                } else {
                    false
                };
                let mut hub_effects = subagent_batches(state);
                if hub_changed {
                    hub_effects.push(AppEffect::Redraw);
                }
                let session = inbound.raw.get("session_id").and_then(Value::as_str);
                let (epoch, seq) = event_position(&inbound.raw);
                if let Some(session_id) = session
                    .filter(|id| !is_open_session(state, id))
                    .map(str::to_owned)
                {
                    let key = retained_key(state, &session_id);
                    let Some(opened) = key.and_then(|key| state.retained_sessions.get_mut(&key))
                    else {
                        return finish_request_cancel(hub_effects, &mut request_cancel);
                    };
                    if !accept_event(&opened.cursor, epoch, seq) {
                        return finish_request_cancel(hub_effects, &mut request_cancel);
                    }
                    if let Some(sequence) = seq {
                        opened.cursor.last_sequence = opened.cursor.last_sequence.max(sequence);
                    }
                    opened.state.background_mut().observe_time(state.now_ms);
                    opened.state.queue.observe_time(state.now_ms);
                    // A compaction while the session is in the background moves it too.
                    let rotated = (inbound.raw.get("type").and_then(Value::as_str)
                        == Some("session.info"))
                    .then(|| {
                        inbound
                            .raw
                            .get("payload")
                            .and_then(|payload| payload.get("stored_session_id"))
                            .and_then(Value::as_str)
                            .filter(|stored| !stored.is_empty())
                            .map(str::to_owned)
                    })
                    .flatten();
                    let produced =
                        apply_inbound(&mut opened.state, &session_id, inbound.frame, inbound.raw);
                    if let Some(stored) = rotated {
                        state
                            .hivecode
                            .rotate(&SessionId::new(session_id.clone()), &stored);
                        hub_effects.extend(hivecode_save(state));
                    }
                    let background = produced
                        .into_iter()
                        .filter_map(|effect| match effect {
                            Effect::Background(effect) => Some(effect),
                            _ => None,
                        })
                        .collect();
                    hub_effects.extend(background_effects(
                        SessionId::new(session_id),
                        background,
                        false,
                    ));
                    return finish_request_cancel(hub_effects, &mut request_cancel);
                }
                let Some(opened) = state.opened.as_mut() else {
                    return finish_request_cancel(hub_effects, &mut request_cancel);
                };
                if session != Some(opened.session_id.as_str())
                    || !accept_event(&opened.cursor, epoch, seq)
                {
                    return finish_request_cancel(hub_effects, &mut request_cancel);
                }
                if let hermes_contract::events::Notification::SessionUsage(payload) =
                    &inbound.frame.notification
                {
                    state.subagents_usage_active = payload
                        .usage
                        .as_ref()
                        .and_then(|usage| usage.active_subagents)
                        .or(payload.active_subagents)
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0);
                }
                let session_id = opened.session_id.clone();
                let mut effects = Vec::new();
                effects.append(&mut hub_effects);
                if let hermes_contract::events::Notification::SessionTitle(payload) =
                    &inbound.frame.notification
                    && payload.session_id == opened.session_id.as_str()
                {
                    opened.title = Some(payload.title.clone());
                }
                effects.extend(automation_from_frame(
                    state,
                    &session_id,
                    &inbound.frame,
                    &inbound.raw,
                ));
                let type_name = inbound
                    .raw
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let payload = inbound.raw.get("payload").cloned().unwrap_or(Value::Null);
                let completed = type_name == "message.complete";
                let Some(opened) = state.opened.as_mut() else {
                    return finish_request_cancel(effects, &mut request_cancel);
                };
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
                opened.state.background_mut().observe_time(state.now_ms);
                let produced =
                    apply_inbound(&mut opened.state, &session_id, inbound.frame, inbound.raw);
                effects.extend(route(state, produced, None));
                // The move's fences and the folder come from this session's own events (#237).
                state
                    .cd_ledger
                    .fences_mut(&session_id)
                    .observe(&session_id, &type_name, &payload);
                // So do the fences of a rename or a branch: a finished turn moves the
                // generation they must present.
                state
                    .ledger
                    .fences_mut(&session_id)
                    .observe(&session_id, &type_name, &payload);
                state
                    .context
                    .observe(&session_id, &type_name, &payload, state.now_ms);
                if type_name == "session.info"
                    && let Some(cwd) = payload.get("cwd").and_then(Value::as_str)
                {
                    state.folder.apply_info_cwd(cwd);
                }
                // A compaction moves the session to a new stored id; the switch follows it.
                if type_name == "session.info"
                    && let Some(stored) = payload
                        .get("stored_session_id")
                        .and_then(Value::as_str)
                        .filter(|stored| !stored.is_empty())
                {
                    state
                        .hivecode
                        .rotate(&SessionId::new(session_id.clone()), stored);
                    effects.extend(hivecode_save(state));
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
                finish_request_cancel(effects, &mut request_cancel)
            }
            Inbound::ServerRequest(request) => {
                if request.session_id.as_deref().is_some_and(|id| {
                    state
                        .configure
                        .as_ref()
                        .and_then(|state| state.hidden_session_id())
                        == Some(id)
                }) {
                    let request_id = request.id.clone();
                    if let Some(service) = &state.configure_service {
                        service.hold_request(request);
                    }
                    return configure_input(
                        state,
                        talaria_core::configure::ConfigureInput::ServerRequest {
                            request_id,
                            method: "server request".into(),
                        },
                        None,
                    );
                }
                // A same-profile workflow reuses this socket. Answering its request, including
                // with `-32601`, settles the prompt for every viewer.
                if !foreground_owns_request(state, request.session_id.as_deref()) {
                    request.responder.ignore();
                    return Vec::new();
                }
                vec![AppEffect::ServerRequest(request)]
            }
            Inbound::Malformed(_) | Inbound::Closed(_) => Vec::new(),
        },
        LinkEvent::Gap(gap) => {
            if is_open_session(state, &gap.session_id)
                || retained_key(state, &gap.session_id).is_some()
            {
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
            } else if let Some(opened) = retained_key(state, &session_id)
                .and_then(|key| state.retained_sessions.get_mut(&key))
            {
                opened.subscription_id = subscription_id;
            }
            background_event(state, &session_id, BackgroundEvent::Attached { now_ms })
        }
        event @ LinkEvent::SessionGone { .. } => {
            let LinkEvent::SessionGone { session_id } = &event else {
                return Vec::new();
            };
            if !is_open_session(state, session_id) {
                if let Some(key) = retained_key(state, session_id) {
                    state.retained_sessions.remove(&key);
                }
                return Vec::new();
            }
            let mut closing = close_processes(state);
            state.session_gone = true;
            abandon_foreground_turn(state);
            let mut effects = link_notice(&event)
                .map(|text| vec![notice_lines(&text)])
                .unwrap_or_default();
            effects.extend(permissions(state, PermissionsInput::SessionClosed));
            effects.extend(automation_event(state, AutomationEvent::Detached));
            effects.append(&mut closing);
            effects
        }
    }
}

/// The foreground session's open turn can no longer finish, so every surface that waits on a
/// running turn (the working row, the status bar's working count, the interrupt key) lets go.
fn abandon_foreground_turn(state: &mut AppState) {
    if let Some(opened) = state.opened.as_mut() {
        opened.state.abandon_turn();
    }
}

#[cfg(test)]
mod link_event_prompt_tests {
    use super::*;
    use hermes_client::{Generation, Responder};
    use hermes_contract::{schemas::SudoRequestParams, server_requests};

    fn state_with_retained_session() -> AppState {
        let open = |id: &str| {
            let mut snapshot: hermes_contract::schemas::CanonicalSnapshot =
                serde_json::from_str(include_str!("../tests/data/running_turn.json"))
                    .expect("snapshot");
            snapshot.session_id = id.into();
            snapshot.stored_session_id = id.into();
            OpenedSession::from_snapshot(snapshot)
        };
        let mut state = AppState::new("prompt-owner");
        state.opened = Some(open("s2"));
        state.retained_sessions.insert(
            RowKey {
                profile: state.profile.clone(),
                session_id: "s1".into(),
            },
            open("s1"),
        );
        state
    }

    fn sudo_request(session_id: &str) -> LinkEvent {
        LinkEvent::Inbound {
            item: Inbound::ServerRequest(ServerRequest {
                generation: Generation(0),
                id: "srq-1".into(),
                session_id: Some(session_id.into()),
                request: server_requests::ServerRequest::Sudo(SudoRequestParams {
                    command: "true".into(),
                    session_id: session_id.into(),
                }),
                responder: Responder::detached(Arc::default()),
            }),
            replayed: false,
        }
    }

    #[test]
    fn retained_session_request_reaches_the_prompt_service() {
        let effects = link_event(&mut state_with_retained_session(), sudo_request("s1"));
        assert!(
            matches!(effects.as_slice(), [AppEffect::ServerRequest(request)] if request.session_id.as_deref() == Some("s1"))
        );
    }

    #[test]
    fn request_for_an_unknown_session_is_still_ignored() {
        let effects = link_event(&mut state_with_retained_session(), sudo_request("s9"));
        assert!(effects.is_empty());
    }

    fn foreground_running(state: &AppState) -> bool {
        state
            .opened
            .as_ref()
            .is_some_and(|opened| *opened.state.turn() == TurnState::Running)
    }

    #[test]
    fn a_session_the_gateway_lost_has_no_running_turn() {
        let mut state = state_with_retained_session();
        assert!(foreground_running(&state), "the fixture's turn runs");
        link_event(
            &mut state,
            LinkEvent::SessionGone {
                session_id: "s2".into(),
            },
        );
        assert!(state.session_gone);
        assert!(
            !foreground_running(&state),
            "the turn of a session that no longer exists still runs: {:?}",
            state.opened.as_ref().map(|opened| opened.state.turn())
        );
    }

    #[test]
    fn a_failed_link_leaves_no_running_turn() {
        let mut state = state_with_retained_session();
        link_event(
            &mut state,
            LinkEvent::State(LinkState::Failed {
                reason: "the gateway closed the connection".into(),
                next_step: "Try again.".into(),
            }),
        );
        assert!(
            !foreground_running(&state),
            "the turn still runs after the link gave up: {:?}",
            state.opened.as_ref().map(|opened| opened.state.turn())
        );
    }

    #[test]
    fn a_reconnecting_link_keeps_the_running_turn() {
        let mut state = state_with_retained_session();
        link_event(
            &mut state,
            LinkEvent::State(LinkState::Reconnecting {
                attempt: 1,
                retry_in: std::time::Duration::from_millis(250),
                reason: "connection closed".into(),
            }),
        );
        assert!(
            foreground_running(&state),
            "the gateway may still be running the turn while the link redials"
        );
    }
}

fn opened_session(
    state: &mut AppState,
    mut opened: OpenedSession,
    notice: Option<String>,
) -> Vec<AppEffect> {
    workflow_origin_follow(state, &opened);
    let rewind_owner = RowKey {
        profile: state.profile.clone(),
        session_id: opened.session_id.clone(),
    };
    bind_rewind_session(state, Some(rewind_owner.clone()));
    state.subagents_usage_active = 0;
    let now_ms = state.now_ms;
    let mut background = close_processes(state);
    let old_profile = state
        .opened_profile
        .take()
        .unwrap_or_else(|| state.profile.clone());
    let same_session = old_profile == state.profile && is_open_session(state, &opened.session_id);
    // A reconnect may reopen the same stored session under a new runtime id.
    if let Some(old) = state.opened.as_ref().filter(|old| {
        old_profile == state.profile
            && old.session_id != opened.session_id
            && old.stored_session_id == opened.stored_session_id
    }) {
        let old = SessionId::new(old.session_id.clone());
        state
            .hivecode
            .carry(&old, &SessionId::new(opened.session_id.clone()));
    }
    // The switch belongs to the stored session, so a return from another session, a restart or
    // a compaction finds it again.
    let hivecode_session = SessionId::new(opened.session_id.clone());
    let announce_hivecode = state
        .hivecode
        .adopt(&hivecode_session, &opened.stored_session_id)
        && !same_session;
    if !same_session && let Some(old_id) = state.opened.as_ref().map(|old| old.session_id.clone()) {
        background.extend(background_event(
            state,
            &old_id,
            BackgroundEvent::ForegroundChanged {
                foreground: false,
                now_ms,
            },
        ));
    }
    if let Some(mut old) = state.opened.take() {
        if same_session {
            carry_over(&mut old.state, &mut opened.state);
            opened.state.queue.carry_waiting_from(&old.state.queue);
            opened.carry_background_from(&old);
        } else {
            let key = RowKey {
                profile: old_profile,
                session_id: old.session_id.clone(),
            };
            state.retained_sessions.insert(key, old);
        }
    }
    let key = RowKey {
        profile: state.profile.clone(),
        session_id: opened.session_id.clone(),
    };
    if let Some(mut old) = state.retained_sessions.remove(&key) {
        carry_over(&mut old.state, &mut opened.state);
        opened.state.queue.carry_waiting_from(&old.state.queue);
        opened.carry_background_from(&old);
    }
    state.opened_profile = Some(state.profile.clone());
    state.model = opened.info.model.clone();
    state.effort = opened.effort.clone();
    state.effort_wire = None;
    // The snapshot's fences are the ones a later mutation must present.
    state
        .ledger
        .fences_mut(&opened.session_id)
        .adopt(Some(opened.revision), Some(opened.execution_generation));
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
    let (mut header, range) = resume_header(opened.state.cells(), &title);
    if header.is_empty()
        && matches!(
            state.launch.as_ref().map(|launch| &launch.mode),
            Some(StartMode::Resume(Some(_)))
        )
    {
        header.push(format!("── Resumed: {title} ──"));
    }
    if !header.is_empty() {
        effects.push(AppEffect::CommitLines(header));
        if !range.is_empty() {
            effects.push(AppEffect::CommitCells(range.clone()));
        }
    }
    opened.state.publish_commit_cursor(range.end);
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
    state.workflow_agent = crate::workflow_agent::WorkflowAgentSession::identify(
        &opened,
        &state.workflow_views,
        &state.workflow_runs_dir,
        state.now_ms,
    );
    if let Some(agent) = state.workflow_agent.as_mut()
        && let Some(request) = agent.first_read()
    {
        effects.push(AppEffect::WorkflowAgentRead {
            stored_id: agent.stored_id().to_owned(),
            request,
        });
    }
    state.opened = Some(opened);
    if state.client_voice_config.backend == "client-bridge" {
        if !same_session {
            let _ = state
                .client_voice
                .update(talaria_core::voice::client::Input::Detach);
        }
        let _ = state
            .client_voice
            .update(talaria_core::voice::client::Input::Reopen);
    }
    if state.draft.is_empty() {
        let returned = state
            .retry_returns
            .get_mut(&rewind_owner)
            .and_then(VecDeque::pop_front);
        if let Some(text) = returned {
            effects.extend(return_prompt(state, text, false));
        }
        if state
            .retry_returns
            .get(&rewind_owner)
            .is_some_and(VecDeque::is_empty)
        {
            state.retry_returns.remove(&rewind_owner);
        }
    }
    if state.voice.open(&session_id) {
        effects.push(AppEffect::Voice {
            session_id: session_id.clone(),
            effect: talaria_core::voice::VoiceEffect::ReadMode,
        });
    } else if !same_session {
        effects.extend(voice_input(
            state,
            &session_id,
            talaria_core::voice::VoiceInput::SessionReopened,
        ));
    }
    if let Some(mode) = state.voice_submit_mode.clone() {
        effects.extend(voice_input(
            state,
            &session_id,
            talaria_core::voice::VoiceInput::SubmitModeRead(mode),
        ));
    } else {
        effects.push(AppEffect::Voice {
            session_id: session_id.clone(),
            effect: talaria_core::voice::VoiceEffect::ReadSubmitMode,
        });
    }
    attach_subagent_foreground(state);
    state.session_gone = false;
    background.extend(background_event(
        state,
        &session_id,
        BackgroundEvent::Attached { now_ms },
    ));
    if !same_session {
        background.extend(background_event(
            state,
            &session_id,
            BackgroundEvent::ForegroundChanged {
                foreground: true,
                now_ms,
            },
        ));
    }
    effects.extend(background);
    state.link_state = LinkView::Connected;
    // An attached session reads the standing state once.
    effects.extend(automation_event(
        state,
        AutomationEvent::Attached {
            session_id: session_id.clone(),
        },
    ));
    effects.extend(commands_follow(
        state,
        CommandsInput::ForegroundSession(Some(session_id.clone())),
    ));
    effects.extend(config_check_effect(state));
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
    if announce_hivecode {
        state.hivecode.bind(&hivecode_session, &state.profile);
        effects.push(notice_lines(state.hivecode.opened_notice()));
        if let Some(check) = state.hivecode.setup_check(&hivecode_session) {
            effects.extend(apply_hivecode(state, vec![check], false));
        }
    }
    effects.extend(hivecode_save(state));
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
    if let Some(session) = state
        .opened
        .as_ref()
        .map(|opened| SessionId::new(opened.session_id.clone()))
        && let Some(text) = state.hivecode.show_kept(&session)
    {
        effects.push(notice_lines(&text));
    }
    while let Some(text) = state.held.pop_front() {
        if let Some(session) = state
            .opened
            .as_ref()
            .map(|opened| SessionId::new(opened.session_id.clone()))
        {
            state.hivecode.bind(&session, &state.profile);
            if let Some(actions) = state.hivecode.answer_install(&session, &text) {
                effects.extend(apply_hivecode(state, actions, false));
                continue;
            }
            hivecode_note_conversation(state, &session);
            let session_model = session_model(state);
            let held_payload = state
                .held_attachments
                .front()
                .filter(|payload| payload.expanded.display == text);
            let request =
                held_payload.map_or(text.as_str(), |payload| payload.expanded.text.as_str());
            if let Some(actions) = state.hivecode.intercept_submit_with_model(
                &session,
                &text,
                request,
                session_model
                    .as_ref()
                    .map(|(model, provider)| (model.as_str(), provider.as_str())),
            ) {
                if held_payload.is_some() {
                    state.held_attachments.pop_front();
                }
                effects.extend(apply_hivecode(state, actions, false));
                continue;
            }
        }
        // A prompt typed before the session finished opening gets the same first-send warning.
        if let Some(agent) = state.workflow_agent.as_mut()
            && let Some(warning) = agent.first_send_warning(&mut state.workflow_agent_warned)
        {
            effects.push(notice_lines(&warning));
            effects.extend(return_held_prompt(state, text));
            continue;
        }
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

/// Put a held prompt back in the composer, with its attachments, ahead of anything typed
/// since.
fn return_held_prompt(state: &mut AppState, text: String) -> Vec<AppEffect> {
    let display = if state
        .held_attachments
        .front()
        .is_some_and(|payload| payload.expanded.display == text)
    {
        let payload = state
            .held_attachments
            .pop_front()
            .expect("checked held payload");
        state.attachments.chips.restore_payloads(payload.media);
        state
            .attachments
            .chips
            .restore_pastes(&payload.expanded.display);
        payload.expanded.display
    } else {
        text
    };
    state.draft = if state.draft.is_empty() {
        display
    } else {
        format!("{display}\n{}", state.draft)
    };
    state.caret = state.draft.len();
    state.attachment_epoch = state.attachment_epoch.wrapping_add(1);
    vec![AppEffect::LoadDraft(state.draft.clone())]
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
    let retried = state.retry.remove(&id);
    if let Some((owner, text)) = retried.as_ref()
        && (state.profile != owner.profile || !is_open_session(state, &owner.session_id))
    {
        return match outcome {
            ExecuteOutcome::Refused { reason } => {
                state
                    .retry_returns
                    .entry(owner.clone())
                    .or_default()
                    .push_back(text.clone());
                vec![
                    notice_lines(&rewind_text::resend_failed(&reason)),
                    notice_lines("The retry is kept for its original session."),
                ]
            }
            ExecuteOutcome::OutcomeUnknown => {
                state
                    .retry_returns
                    .entry(owner.clone())
                    .or_default()
                    .push_back(text.clone());
                vec![notice_lines(
                    "The retry's outcome is unknown; its text is kept for its original session.",
                )]
            }
            ExecuteOutcome::Sent | ExecuteOutcome::VoiceStopped => Vec::new(),
        };
    }
    let mut effects = submit_outcome(state, id, outcome.clone());
    // `/retry` already removed the prompt, so a send that did not go through returns it.
    if let Some((_, text)) = retried {
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
    let remembered = match &outcome {
        ExecuteOutcome::Sent => {
            state.remembered_notes.remove(&id);
            None
        }
        ExecuteOutcome::Refused { .. } => state.remembered_notes.remove(&id),
        ExecuteOutcome::OutcomeUnknown | ExecuteOutcome::VoiceStopped => None,
    };
    let voice_session = state.voice_submissions.remove(&id);
    let voice_stopped = matches!(outcome, ExecuteOutcome::VoiceStopped);
    let voice_effects = if voice_stopped {
        voice_session.map_or_else(Vec::new, |session| {
            voice_input(
                state,
                &session,
                talaria_core::voice::VoiceInput::VoiceStopped,
            )
        })
    } else {
        Vec::new()
    };
    let mut attachment_notices = Vec::new();
    if let Some((owner, payload)) = state.submitted_attachments.remove(&id) {
        if state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.as_str())
            != Some(owner.as_str())
        {
            return voice_effects;
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
    let (reason, notice) = match outcome {
        ExecuteOutcome::Sent => return attachment_notices,
        ExecuteOutcome::Refused { reason } => ("refused", failure::prompt_refused_notice(&reason)),
        ExecuteOutcome::OutcomeUnknown => {
            let mut effects = lost_submit(state, id);
            effects.extend(attachment_notices);
            effects.extend(restore_failed_attachments(state));
            return effects;
        }
        ExecuteOutcome::VoiceStopped => ("voice stop", failure::voice_stop_notice()),
    };
    // A refused retry of a lost submit hands its text back (#232); the reducer forgets it.
    let returned = state.opened.as_ref().and_then(|opened| {
        opened
            .state
            .queue
            .ledger
            .get(&SubmissionId(id.clone()))
            .filter(|entry| entry.retried)
            .map(|entry| talaria_core::hivecode::shown_text(&entry.text).to_owned())
    });
    // A routed prompt that was not sent builds no workflow.
    state.hivecode.submission_failed(&id);
    let mut effects = reduce(
        state,
        Event::SubmitFailed {
            submission_id: id,
            reason: reason.to_string(),
        },
    );
    effects.push(notice_lines(&notice));
    if let Some(text) = &remembered {
        effects.push(notice_lines(&format!("note not sent: {text}")));
    }
    effects.extend(voice_effects);
    effects.extend(attachment_notices);
    effects.extend(restore_failed_attachments(state));
    if voice_stopped {
        effects.retain(|effect| !matches!(effect, AppEffect::LoadDraft(_)));
    }
    effects.extend(remembered.or(returned).map(AppEffect::OfferDraft));
    effects
}

/// A lost submit reply is not a failure yet: the composer shows `Sending…` until the next queue
/// frame or the reconcile wait, and the reducer then retries once with the same id (#232).
fn lost_submit(state: &mut AppState, id: String) -> Vec<AppEffect> {
    let submission_id = Some(SubmissionId(id));
    let mut effects = reduce(
        state,
        Event::QueueCallUnknown {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: submission_id.clone(),
        },
    );
    effects.push(AppEffect::ReconcileLater(Box::new(
        Event::QueueReconcileDue {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id,
        },
    )));
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
        ExecuteOutcome::Sent => {
            // An accidental interrupt does not end workflow mode; say so.
            if let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| SessionId::new(opened.session_id.clone()))
                && let Some(action) = state.hivecode.after_interrupt(&session)
            {
                effects.extend(apply_hivecode(state, vec![action], false));
            }
            return effects;
        }
        ExecuteOutcome::Refused { reason } => reason,
        ExecuteOutcome::OutcomeUnknown | ExecuteOutcome::VoiceStopped => {
            "connection_dropped".to_string()
        }
    };
    effects.extend(reduce(state, Event::InterruptFailed));
    effects.push(notice_lines(&failure::interrupt_refused_notice(&reason)));
    effects
}

/// Commit cells that became final, and tell the prompt service when the open prompts changed.
fn commit_ready(state: &mut AppState) -> Vec<AppEffect> {
    let Some(opened) = state.opened.as_mut() else {
        return Vec::new();
    };
    let mut effects = Vec::new();
    let ready = opened.state.final_prefix_len();
    // Compaction moves this cursor with the cells, so a newly final cell that
    // slid behind the old position is still committed.
    let baseline = opened.state.commit_cursor();
    if ready > baseline {
        effects.push(AppEffect::CommitCells(baseline..ready));
    }
    let committed = ready.min(opened.state.cells().len());
    state.committed_cells = committed;
    opened.state.publish_commit_cursor(committed);
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
        Cell::HermesCommand(command) => command.typed.clone(),
        Cell::Background(cell) => match &cell.prompt {
            Some(prompt) => format!("bg \u{b7} {prompt}"),
            None => format!("bg \u{b7} {}", cell.task_id),
        },
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
            if command.action == LocalAction::ModelRoles {
                return Slash::Notice("Usage: /models".to_owned());
            }
            return Slash::Notice(format!("/{} does not take an argument.", line.name));
        }
        return match command.action {
            LocalAction::Quit => Slash::Quit,
            LocalAction::Btw => Slash::Btw(line.arg.clone()),
            LocalAction::Providers(_) => {
                match talaria_core::providers::parse_providers_args(&line.arg) {
                    Ok(target) => Slash::Local(LocalAction::Providers(target), line.arg),
                    Err(usage) => Slash::Notice(usage.into()),
                }
            }
            _ => Slash::Local(command.action.clone(), line.arg),
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
    quit_with_voice(state, &mut Vec::new()).await
}

async fn quit_with_voice(
    state: &mut AppState,
    err: &mut (dyn std::io::Write + Send),
) -> (DetachOutcome, u8, String) {
    state.quitting = true;
    if state.uses_client_voice() {
        let _ = state
            .client_voice
            .update(talaria_core::voice::client::Input::Cutoff(
                talaria_core::voice::client::Cutoff::Quit,
            ));
    }
    state.close_client_voice_jobs().await;
    let broken = state.session_gone || state.link_failed;
    let mut outcome = DetachOutcome::Skipped;
    if !broken
        && let (Some(opened), Some(service)) = (state.opened.as_ref(), state.service.as_ref())
    {
        if !state.uses_client_voice()
            && let Some(client) = service.client()
        {
            state
                .voice
                .stop_before_detach(client, &opened.session_id, err)
                .await;
        }
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

    #[test]
    fn queue_draft_clear_is_emitted_only_after_acceptance() {
        use talaria_core::queue::{MovePlan, QueueIntent};
        for steer in [false, true] {
            for busy in [false, true] {
                let mut state = opened();
                if busy {
                    state.opened.as_mut().unwrap().state.queue.plan = Some(MovePlan {
                        steps: Vec::new(),
                        next: 0,
                        target: Vec::new(),
                        then_interrupt: None,
                    });
                }
                let text = " draft text ".to_owned();
                let intent = if steer {
                    QueueIntent::SteerText(text.clone())
                } else {
                    QueueIntent::SendDraftNow(text.clone())
                };
                let effects = update(
                    &mut state,
                    AppMessage::ForegroundKey(UserAction::Queue(intent)),
                );
                let clears = effects.iter().filter(|effect| matches!(effect, AppEffect::ClearSubmittedDraft(draft) if draft == &text)).count();
                assert_eq!(clears, usize::from(!busy));
                if busy {
                    assert!(effects.iter().any(
                        |effect| matches!(effect, AppEffect::OfferDraft(draft) if draft == &text)
                    ));
                    assert!(
                        !effects.iter().any(|effect| matches!(
                            effect,
                            AppEffect::Send(_) | AppEffect::Queue(_)
                        ))
                    );
                } else {
                    assert!(
                        effects.iter().any(|effect| matches!(
                            effect,
                            AppEffect::Send(_) | AppEffect::Queue(_)
                        ))
                    );
                }
            }
        }
    }

    #[test]
    fn clearing_a_synchronized_draft_invalidates_pending_attachment_results() {
        let mut state = opened();
        state.draft = "draft text".into();
        state.attachment_epoch = 2;
        let _ = update(
            &mut state,
            AppMessage::DraftSynced {
                text: String::new(),
                caret: 0,
            },
        );
        assert_eq!(state.attachment_epoch, 3);
        let effects = update(
            &mut state,
            AppMessage::AttachmentResult {
                epoch: 2,
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
    }

    #[test]
    fn cold_start_routes_held_workflows_without_clearing_the_next_draft() {
        let mut state = AppState::new("cold-start");
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        let initial = submit(&mut state, "review the parser".into());
        assert!(
            !initial
                .iter()
                .any(|effect| matches!(effect, AppEffect::Send(_)))
        );
        assert_eq!(state.held.len(), 1);
        state.draft = "next draft".into();
        state.caret = state.draft.len();
        let session = opened().opened.take().unwrap();
        let effects = opened_session(&mut state, session, None);
        assert!(effects.iter().any(|effect|matches!(effect,AppEffect::Hivecode{action:crate::hivecode::Action::RunCommand{command_line,..},..} if command_line=="/talaria-workflows review the parser")));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Send(Effect::SendPrompt { .. })))
        );
        assert_eq!(state.draft, "next draft");
        assert_eq!(state.caret, 10);
        assert!(state.held.is_empty());
        assert!(state.held_attachments.is_empty());
    }

    #[test]
    fn held_workflow_prompt_names_the_model_the_opened_session_runs_on() {
        let mut state = AppState::new("cold-start");
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        let _ = submit(&mut state, "review the parser".into());
        assert_eq!(state.held.len(), 1);
        let mut session = opened().opened.take().unwrap();
        session.info.model = Some("session-model".into());
        session.info.provider = Some("provider-a".into());
        let effects = opened_session(&mut state, session, None);
        let lines: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::RunCommand { command_line, .. },
                    ..
                } => Some(command_line.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines,
            ["/talaria-workflows review the parser\nSession model: session-model on provider-a"]
        );
    }

    #[test]
    fn routed_workflow_prompt_names_the_model_the_session_runs_on() {
        let mut state = opened();
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        let line = |effects: &[AppEffect]| {
            effects.iter().find_map(|effect| match effect {
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::RunCommand { command_line, .. },
                    ..
                } => Some(command_line.clone()),
                _ => None,
            })
        };
        let effects = submit(&mut state, "review the parser".into());
        assert_eq!(
            line(&effects).as_deref(),
            Some("/talaria-workflows review the parser")
        );
        if let Some(opened) = state.opened.as_mut() {
            opened.info.model = Some("profile-model".into());
            opened.info.provider = Some("provider-a".into());
        }
        // The model the user switched to wins over the one the session started with.
        state.model = Some("picked-model".into());
        let effects = submit(&mut state, "review the parser".into());
        assert_eq!(
            line(&effects).as_deref(),
            Some("/talaria-workflows review the parser\nSession model: picked-model on provider-a")
        );
    }

    #[test]
    fn a_held_workflow_routes_the_paste_after_its_composer_is_cleared() {
        let mut state = AppState::new("cold-start");
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        let paste = "error α\n\tfirst detail\nsecond detail\nthird detail\nlast detail";
        let label = state.attachments.chips.collapse_paste(paste);
        let draft = format!("Review this: {label}");
        let _ = submit(&mut state, draft.clone());
        assert_eq!(
            state.attachments.chips.expand_pastes(&draft).unwrap(),
            draft
        );

        let session = opened().opened.take().unwrap();
        let effects = opened_session(&mut state, session, None);
        let (session, original_text, restore_text, command_line) = effects
            .into_iter()
            .find_map(|effect| match effect {
                AppEffect::Hivecode {
                    action:
                        crate::hivecode::Action::RunCommand {
                            session,
                            original_text,
                            restore_text,
                            command_line,
                        },
                    ..
                } => Some((session, original_text, restore_text, command_line)),
                _ => None,
            })
            .expect("workflow command");
        assert_eq!(original_text, draft);
        assert_eq!(
            command_line,
            format!("/talaria-workflows Review this: {paste}")
        );

        let effects = update(
            &mut state,
            AppMessage::HivecodeDone {
                session,
                original_text,
                restore_text,
                outcome: Err(talaria_core::commands::ErrorView::NotConnected),
            },
        );
        let restored = format!("Review this: {paste}");
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::LoadDraft(draft) if draft == &restored
        )));
    }

    #[test]
    fn a_workflow_followup_expands_the_paste_in_the_model_text() {
        let mut state = opened();
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        let session = SessionId::new("session");
        let epoch = skill_epoch(state.opened.as_ref().unwrap());
        state.hivecode.skill_sent(&session, epoch);
        let paste = "error α\n\tfirst detail\nsecond detail\nthird detail\nlast detail";
        let label = state.attachments.chips.collapse_paste(paste);
        let draft = format!("Review this: {label}");

        let effects = submit(&mut state, draft);
        assert!(effects.iter().any(|effect| matches!(effect,
            AppEffect::Send(Effect::SendPrompt { text, .. }) if text.contains(paste) && !text.contains(&label)
        )));
    }

    #[test]
    fn queue_shortcuts_expand_paste_before_sending_and_clear_the_visible_draft() {
        for steer in [false, true] {
            let mut state = opened();
            let paste = "error α\n\tfirst detail\nsecond detail\nthird detail\nlast detail";
            let draft = state.attachments.chips.collapse_paste(paste);
            let intent = if steer {
                QueueIntent::SteerText(draft.clone())
            } else {
                QueueIntent::SendDraftNow(draft.clone())
            };
            let effects = queue_intent(&mut state, intent);
            assert!(effects.iter().any(|effect| matches!(effect,
                AppEffect::Send(Effect::SendPrompt { text, .. }) if text == paste
            )));
            assert!(effects.iter().any(|effect| matches!(effect,
                AppEffect::ClearSubmittedDraft(text) if text == &draft
            )));
        }
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
        (OpenError::MismatchedSession { .. }, _) => {
            failure::link_failure("the gateway returned a snapshot for a different session")
        }
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
        // Nothing picked a session for us, so the newest one in the folder opens. The
        // command line resolves its own pick before the loop starts.
        SessionRequest::Pick => (
            SessionRequest::Latest {
                workspace: workspace.clone(),
            },
            false,
        ),
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
    /// Fleet timings when the interactive launch runs the fleet service; `None` runs none.
    pub fleet: Option<crate::fleet::FleetConfig>,
    /// Optional live terminal-agent discovery.
    pub herdr: Option<crate::herdr::HerdrConfig>,
    /// Whether the launch uses a remote gateway.
    pub remote_gateway: bool,
    /// Host of the remote gateway. Prompt cards include it when it is not local.
    pub remote_host: Option<String>,
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

/// A unit of work that waits for the next frame.
type Deferred = Box<dyn FnOnce() + Send + 'static>;

/// The executor side of the loop: everything it needs to start a call.
struct Executor {
    sender: mpsc::UnboundedSender<AppMessage>,
    /// Set when the restart request returns, before its result reaches the reducer.
    doctor_posted_at: Arc<Mutex<Option<Instant>>>,
    /// Time zero of the app clock that side work is timed by.
    started: Instant,
    /// Prompts go through one worker, one call at a time, so they reach the gateway in the
    /// order they were submitted.
    prompts: mpsc::UnboundedSender<(SessionService, Effect)>,
    model_files: mpsc::UnboundedSender<ModelFileJob>,
    /// The side connection: the listing, the picker renames and the exports.
    side: Arc<Mutex<DirectoryService>>,
    /// Whether the side connection is open.
    side_open: Arc<AtomicBool>,
    workflow_connections: Arc<crate::workflow_progress::WorkflowConnections>,
    /// Work that waits for the next frame, so the overlay is on screen first.
    deferred: std::sync::Mutex<Vec<Deferred>>,
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
                let (outcome, receipt) = service.execute_with_receipt(effect).await;
                // The queue ledger matches this prompt's cell by its admission (#232).
                if let Some(result) = receipt {
                    let event = Event::QueueCallResult {
                        action: QueueAction::Submit,
                        admission_id: None,
                        submission_id: Some(SubmissionId(submission_id.clone())),
                        result,
                    };
                    let _ = results.send(AppMessage::PromptDone {
                        events: vec![event],
                        notices: Vec::new(),
                        masked: None,
                    });
                }
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
            doctor_posted_at: Arc::new(Mutex::new(None)),
            prompts,
            model_files,
            started: Instant::now(),
            side: Arc::new(Mutex::new(DirectoryService::new())),
            side_open: Arc::new(AtomicBool::new(false)),
            workflow_connections: Arc::default(),
            deferred: std::sync::Mutex::new(Vec::new()),
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

    /// One queue call on a task; its events come back as one message. A lost reply also
    /// arms the reconcile wait, whose end comes back as `QueueReconcileDue` (#232).
    fn queue_call(&self, client: hermes_client::Client, session_id: SessionId, effect: Effect) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let events = QueueService::new(client, session_id).run(effect).await;
            let lost: Vec<Event> = events.iter().filter_map(reconcile_due).collect();
            let _ = sender.send(AppMessage::PromptDone {
                events,
                notices: Vec::new(),
                masked: None,
            });
            if !lost.is_empty() {
                tokio::time::sleep(RECONCILE_WAIT).await;
                let _ = sender.send(AppMessage::PromptDone {
                    events: lost,
                    notices: Vec::new(),
                    masked: None,
                });
            }
        });
    }

    fn send_later(&self, task: impl Future<Output = AppMessage> + Send + 'static) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.send(task.await);
        });
    }

    /// Run a task whose result the loop does not read.
    fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        tokio::spawn(task);
    }

    /// Hold work until the next frame is on screen.
    fn defer(&self, work: impl FnOnce() + Send + 'static) {
        if let Ok(mut deferred) = self.deferred.lock() {
            deferred.push(Box::new(work));
        }
    }

    /// Run the work that waited for the frame.
    fn run_deferred(&self) {
        let work = match self.deferred.lock() {
            Ok(mut deferred) => deferred.drain(..).collect::<Vec<Deferred>>(),
            Err(_) => Vec::new(),
        };
        for work in work {
            work();
        }
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
                    command: command_of(action.clone()).to_owned(),
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
                    command: command_of(action.clone()).to_owned(),
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

fn background_not_sent(effect: BackgroundEffect, now_ms: u64) -> Option<BackgroundEvent> {
    use talaria_core::background::{CallFailure, KillOutcome, StopOutcome};
    Some(match effect {
        BackgroundEffect::ListProcesses { request } => BackgroundEvent::ListFailed {
            request,
            failure: CallFailure::NotSent,
            now_ms,
        },
        BackgroundEffect::KillProcess { request, id } => BackgroundEvent::KillReplied {
            request,
            process_id: id,
            outcome: KillOutcome::Failed(CallFailure::NotSent),
            now_ms,
        },
        BackgroundEffect::StopProcesses { request } => BackgroundEvent::StopReplied {
            request,
            outcome: StopOutcome::Failed(CallFailure::NotSent),
            now_ms,
        },
        BackgroundEffect::StartBackground { request, .. } => BackgroundEvent::BgFailed {
            request,
            failure: CallFailure::NotSent,
            now_ms,
        },
        _ => return None,
    })
}

fn background_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    let opened = state.opened.as_ref()?;
    if state.session_gone {
        return None;
    }
    let due = background::next_deadline(opened.state.background(), executor.now_ms())?;
    executor.started.checked_add(Duration::from_millis(due))
}

fn notice_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    let opened = state.opened.as_ref()?;
    if state.session_gone {
        return None;
    }
    let due = opened.state.next_notice_deadline()?;
    executor.started.checked_add(Duration::from_millis(due))
}

/// When the interrupt key's timer next changes: a pending press settles into an interrupt, the
/// "press again to interrupt" hint ends, or a held key is over. The loop wakes then with no key
/// pressed.
fn interrupt_hint_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    let due = state.interrupt_confirm.next_deadline()?;
    executor.started.checked_add(Duration::from_millis(due))
}

fn queue_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    if state.session_gone {
        return None;
    }
    let due = state
        .opened
        .as_ref()?
        .state
        .queue
        .next_waiting_tick(executor.now_ms())?;
    executor.started.checked_add(Duration::from_millis(due))
}

/// An open workflow keeps consuming journal and control changes even without input.
fn workflow_wake(state: &AppState, executor: &Executor) -> Option<Instant> {
    if state.kanban.is_some() {
        return Some(Instant::now() + Duration::from_millis(250));
    }
    [
        state.workflow_views.next_tick_ms(),
        state.hivecode.next_scan_ms(),
        state
            .workflow_agent
            .as_ref()
            .and_then(crate::workflow_agent::WorkflowAgentSession::next_refresh_ms),
    ]
    .into_iter()
    .flatten()
    .min()
    .and_then(|ms| executor.started.checked_add(Duration::from_millis(ms)))
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

/// The side connection, opened when none is open yet. The guard is held for the call.
async fn side_service<'a>(
    side: &'a Arc<Mutex<DirectoryService>>,
    side_open: &AtomicBool,
    home: &Path,
    profile: &str,
) -> Result<MutexGuard<'a, DirectoryService>, String> {
    let mut service = side.lock().await;
    if !side_open.load(Ordering::SeqCst) {
        service
            .open(home, profile)
            .await
            .map_err(|error| error.phrase().to_owned())?;
        side_open.store(true, Ordering::SeqCst);
    }
    Ok(service)
}

/// The profiles the side connection may reach and the note its scope carries.
fn scope_parts(scope: &SideScope) -> (Vec<String>, Option<String>) {
    match scope {
        SideScope::Host { profiles } => (profiles.clone(), None),
        SideScope::ProfileOnly { profile, why } => (
            vec![profile.clone()],
            Some(format!(
                "Showing only the profile {profile}: Hermes did not allow a connection across profiles ({why})."
            )),
        ),
    }
}

/// Tell the screen what the picker shows, so it can open or close the overlay.
fn sync_picker<S: Screen>(state: &AppState, screen: &mut S) {
    if state.profiles.is_open() {
        return;
    }
    screen.sync_picker(state.picker.as_ref());
    screen.sync_fleet(&state.view());
    let mcp = state
        .mcp
        .is_open()
        .then(|| talaria_core::mcp::view(&state.mcp, Instant::now()));
    screen.sync_mcp(mcp.as_ref());
}

fn mcp_input(state: &mut AppState, input: talaria_core::mcp::McpInput) -> Vec<AppEffect> {
    let prepare = state.mcp.is_open()
        && state.mcp_service.is_none()
        && !state.mcp_preparing
        && state.link_state == LinkView::Connected
        && matches!(
            &input,
            talaria_core::mcp::McpInput::Key(talaria_core::mcp::McpKey::Action(
                talaria_core::mcp::McpAction::Reload
            ))
        );
    let mut effects: Vec<_> = talaria_core::mcp::update(&mut state.mcp, input, state.attention_at)
        .into_iter()
        .map(AppEffect::Mcp)
        .collect();
    if prepare {
        effects.insert(0, AppEffect::PrepareMcp);
    }
    effects
}

fn open_mcp(state: &mut AppState, open: talaria_core::mcp::McpOpen) -> Vec<AppEffect> {
    if let Some(service) = &state.mcp_service {
        service.reopen();
    }
    let input = talaria_core::mcp::McpInput::Open {
        open,
        profile_name: state.profile.clone(),
        connected: state.mcp_service.is_some() && state.link_state == LinkView::Connected,
        over_ssh: talaria_core::mcp::over_ssh(|key| std::env::var(key).ok()),
        foreground: state
            .opened
            .as_ref()
            .map(|opened| opened.session_id.clone()),
    };
    let mut effects = mcp_input(state, input);
    if state.mcp_service.is_none() && !state.mcp_preparing {
        effects.insert(0, AppEffect::PrepareMcp);
    }
    effects
}

async fn prepare_mcp(
    client: hermes_client::Client,
    home: PathBuf,
    profile: String,
) -> Result<Arc<crate::mcp::McpService>, String> {
    use hermes_client::{
        discovery::{DiscoverOptions, EnsurePolicy},
        rest::{NativeHttpMinter, RestClient, RestOptions},
    };
    let endpoint = hermes_client::discover_with(
        &home,
        &DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..DiscoverOptions::default()
        },
    )
    .await
    .map(Arc::new)
    .map_err(|_| "Could not find the connected Hermes web interface.".to_owned())?;
    let resolved = endpoint
        .resolve_profile(&profile)
        .map_err(|_| "Hermes no longer serves this profile.".to_owned())?;
    let rest = RestClient::new(
        endpoint.api_origin(),
        Arc::new(NativeHttpMinter::new(home, endpoint.clone(), resolved)),
        RestOptions::default(),
    )
    .map_err(|_| "Could not use the connected Hermes web interface.".to_owned())?;
    Ok(Arc::new(crate::mcp::McpService::new(client, rest, profile)))
}

fn start_mcp_prepare(state: &mut AppState, executor: &Executor) {
    if state.mcp_service.is_some() || state.mcp_preparing {
        return;
    }
    state.mcp_preparing = true;
    let binding = state.mcp_binding;
    let client = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned();
    let home = state.hermes_home.clone();
    let profile = state.profile.clone();
    executor.send_later(async move {
        let result = match client {
            Some(client) => prepare_mcp(client, home, profile).await,
            None => Err("Not connected to Hermes.".to_owned()),
        };
        AppMessage::McpBound { binding, result }
    });
}

#[cfg(test)]
mod mcp_integration_tests {
    use super::*;
    use talaria_core::mcp::{McpAction, McpEffect, McpInput, McpKey};

    #[test]
    fn local_apply_survives_asynchronous_service_preparation() {
        let mut state = AppState::new("mcp-test");
        state.profile = "fixture".into();
        let effects = submit(&mut state, "/mcp apply".into());
        assert!(state.mcp.is_open());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::PrepareMcp))
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Mcp(McpEffect::Reload { .. })))
        );
        let effects = mcp_input(&mut state, McpInput::Connection { connected: true });
        assert!(effects.iter().any(|effect| matches!(
            effect,
            AppEffect::Mcp(McpEffect::Reload { confirm: false, .. })
        )));
    }

    #[test]
    fn refresh_can_retry_failed_web_client_preparation() {
        let mut state = AppState::new("mcp-test");
        state.link_state = LinkView::Connected;
        submit(&mut state, "/mcp".into());
        let effects = mcp_input(&mut state, McpInput::Key(McpKey::Action(McpAction::Reload)));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::PrepareMcp))
        );
    }

    #[test]
    fn closing_the_overlay_never_interrupts_the_foreground() {
        let mut state = AppState::new("mcp-test");
        submit(&mut state, "/mcp apply".into());
        let effects = user_action(&mut state, UserAction::Mcp(McpInput::Close));
        assert!(!state.mcp.is_open());
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Mcp(McpEffect::Close)))
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Send(Effect::Interrupt { .. })))
        );
        let effects = mcp_input(&mut state, McpInput::Connection { connected: true });
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Mcp(McpEffect::Reload { .. })))
        );
    }
}

async fn profiles_without_client(
    effect: ProfilesEffect,
    home: PathBuf,
    profile: String,
) -> Option<ProfilesInput> {
    use hermes_client::discovery::{DiscoverOptions, EnsurePolicy, discover_with};
    use talaria_core::profiles::{CallFailure, DiscoveryFailure, RescanFailure};
    Some(match effect {
        ProfilesEffect::Discover { generation } => {
            let options = DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            };
            let result = discover_with(&home, &options)
                .await
                .map(|endpoint| crate::profiles::gateway_view(&endpoint, &home))
                .map_err(|error| DiscoveryFailure {
                    message: error.to_string(),
                    next_step: error.next_step().to_owned(),
                });
            ProfilesInput::Discovered(generation, result)
        }
        ProfilesEffect::List { generation } => {
            ProfilesInput::Listed(generation, Err("not connected".to_string()))
        }
        ProfilesEffect::Topology { generation } => ProfilesInput::Topology(
            generation,
            crate::profiles::load_topology(&home, &profile).await,
        ),
        ProfilesEffect::Describe { generation, name } => {
            ProfilesInput::Described(generation, name, Err(CallFailure::NotSent))
        }
        ProfilesEffect::Create(_) => ProfilesInput::CreateAnswered(Err(CallFailure::NotSent)),
        // Rename and delete ride a ticket of their own, not the session socket.
        ProfilesEffect::Delete { name } => {
            let result = crate::profiles::delete_profile(&home, &name).await;
            ProfilesInput::DeleteAnswered(name, result)
        }
        ProfilesEffect::Rename { name, new_name } => {
            let result = crate::profiles::rename_profile(&home, &name, &new_name).await;
            ProfilesInput::RenameAnswered(name, result)
        }
        ProfilesEffect::Configure { name, section, .. } => {
            ProfilesInput::ConfigureAnswered(name, section, Err(CallFailure::NotSent))
        }
        ProfilesEffect::Rescan => {
            ProfilesInput::RescanAnswered(Err(RescanFailure::Other("not connected".to_string())))
        }
        _ => return None,
    })
}

/// Append one line to `home/.talaria/logs/client.log`.
///
/// On Unix the home directory is the anchor. `.talaria`, `logs`, and `client.log`
/// are opened with `NOFOLLOW`. Each handle must be an owned directory or regular
/// file before its mode changes. A log that other users can read is replaced by
/// a new `0600` inode under an exclusive lock before the line is written.
fn append_background_client_log(home: &Path, text: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        append_background_client_log_unix(home, text)
    }
    #[cfg(not(unix))]
    {
        let logs = home.join(".talaria").join("logs");
        std::fs::create_dir_all(&logs)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("client.log"))?;
        use std::io::Write;
        writeln!(file, "{text}")?;
        Ok(())
    }
}

#[cfg(unix)]
fn append_background_client_log_unix(home: &Path, text: &str) -> std::io::Result<()> {
    let home = open_owned_directory(home)?;
    let talaria = ensure_owned_directory(&home, ".talaria")?;
    let logs = ensure_owned_directory(&talaria, "logs")?;
    rustix::fs::flock(&logs, rustix::fs::FlockOperation::LockExclusive)?;
    let _lock = LogDirLock(&logs);
    let mut file = open_client_log(&logs)?;
    use std::io::Write;
    writeln!(file, "{text}")?;
    Ok(())
}

#[cfg(unix)]
struct LogDirLock<'a>(&'a rustix::fd::OwnedFd);

#[cfg(unix)]
impl Drop for LogDirLock<'_> {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(self.0, rustix::fs::FlockOperation::Unlock);
    }
}

#[cfg(unix)]
fn directory_flags() -> rustix::fs::OFlags {
    rustix::fs::OFlags::RDONLY
        | rustix::fs::OFlags::DIRECTORY
        | rustix::fs::OFlags::NOFOLLOW
        | rustix::fs::OFlags::CLOEXEC
}

#[cfg(unix)]
fn open_owned_directory(path: &Path) -> std::io::Result<rustix::fd::OwnedFd> {
    let fd = rustix::fs::open(path, directory_flags(), rustix::fs::Mode::empty())?;
    require_owned_directory(&fd)?;
    Ok(fd)
}

#[cfg(unix)]
fn ensure_owned_directory(
    parent: &rustix::fd::OwnedFd,
    name: &str,
) -> std::io::Result<rustix::fd::OwnedFd> {
    match open_directory_at(parent, name) {
        Ok(fd) => return tighten_directory(fd),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match rustix::fs::mkdirat(parent, name, rustix::fs::Mode::RWXU) {
        Ok(()) => {}
        Err(rustix::io::Errno::EXIST) => {}
        Err(errno) => return Err(errno.into()),
    }
    tighten_directory(open_directory_at(parent, name)?)
}

#[cfg(unix)]
fn open_directory_at(
    parent: &rustix::fd::OwnedFd,
    name: &str,
) -> std::io::Result<rustix::fd::OwnedFd> {
    Ok(rustix::fs::openat(
        parent,
        name,
        directory_flags(),
        rustix::fs::Mode::empty(),
    )?)
}

#[cfg(unix)]
fn tighten_directory(fd: rustix::fd::OwnedFd) -> std::io::Result<rustix::fd::OwnedFd> {
    require_owned_directory(&fd)?;
    rustix::fs::fchmod(&fd, rustix::fs::Mode::RWXU)?;
    Ok(fd)
}

#[cfg(unix)]
fn require_owned_directory(fd: &rustix::fd::OwnedFd) -> std::io::Result<()> {
    let stat = rustix::fs::fstat(fd)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::Directory
        || stat.st_uid != rustix::process::getuid().as_raw()
    {
        return Err(refused_log_path());
    }
    Ok(())
}

#[cfg(unix)]
fn require_owned_file(fd: &rustix::fd::OwnedFd) -> std::io::Result<rustix::fs::Stat> {
    let stat = rustix::fs::fstat(fd)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile
        || stat.st_uid != rustix::process::getuid().as_raw()
    {
        return Err(refused_log_path());
    }
    Ok(stat)
}

#[cfg(unix)]
fn exposes_group_or_other(stat: &rustix::fs::Stat) -> bool {
    rustix::fs::Mode::from_raw_mode(stat.st_mode).as_raw_mode() & 0o077 != 0
}

#[cfg(unix)]
fn refused_log_path() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "background log path is not an owned directory or regular file",
    )
}

#[cfg(unix)]
fn open_client_log(logs: &rustix::fd::OwnedFd) -> std::io::Result<std::fs::File> {
    const NAME: &str = "client.log";
    match rustix::fs::openat(
        logs,
        NAME,
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::APPEND
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(fd) => {
            let stat = require_owned_file(&fd)?;
            if exposes_group_or_other(&stat) {
                replace_readable_log(logs, fd)
            } else {
                rustix::fs::fchmod(&fd, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR)?;
                Ok(std::fs::File::from(fd))
            }
        }
        Err(rustix::io::Errno::NOENT) => create_client_log(logs),
        Err(errno) => Err(errno.into()),
    }
}

#[cfg(unix)]
fn create_client_log(logs: &rustix::fd::OwnedFd) -> std::io::Result<std::fs::File> {
    let fd = create_private_file(logs, "client.log")?;
    Ok(std::fs::File::from(fd))
}

#[cfg(unix)]
fn create_private_file(
    logs: &rustix::fd::OwnedFd,
    name: &str,
) -> std::io::Result<rustix::fd::OwnedFd> {
    let fd = rustix::fs::openat(
        logs,
        name,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    require_owned_file(&fd)?;
    rustix::fs::fchmod(&fd, rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR)?;
    Ok(fd)
}

#[cfg(unix)]
fn replace_readable_log(
    logs: &rustix::fd::OwnedFd,
    old: rustix::fd::OwnedFd,
) -> std::io::Result<std::fs::File> {
    let mut bytes = Vec::new();
    {
        let mut old = std::fs::File::from(old);
        use std::io::Read;
        old.read_to_end(&mut bytes)?;
    }
    let mut created = None;
    for n in 0..32 {
        let name = format!(".client.log.{}.{}", std::process::id(), n);
        match create_private_file(logs, &name) {
            Ok(fd) => {
                created = Some((name, fd));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some((name, fd)) = created else {
        return Err(std::io::Error::other(
            "could not create a private replacement for the background log",
        ));
    };
    let copied = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::from(fd);
        use std::io::Write;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = copied {
        let _ = rustix::fs::unlinkat(logs, &name, rustix::fs::AtFlags::empty());
        return Err(error);
    }
    if let Err(errno) = rustix::fs::renameat(logs, &name, logs, "client.log") {
        let _ = rustix::fs::unlinkat(logs, &name, rustix::fs::AtFlags::empty());
        return Err(errno.into());
    }
    let fd = rustix::fs::openat(
        logs,
        "client.log",
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::APPEND
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let stat = require_owned_file(&fd)?;
    if exposes_group_or_other(&stat) {
        return Err(refused_log_path());
    }
    Ok(std::fs::File::from(fd))
}

/// Apply one message and run its effects. Returns whether the screen needs a draw.
#[inline(never)]
fn apply<S: Screen>(
    state: &mut AppState,
    message: AppMessage,
    screen: &mut S,
    executor: &Executor,
) -> bool {
    state.attention_at = Instant::now();
    let mut redraw = false;
    state.now_ms = executor.now_ms();
    state.wall_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    if let Some(client) = state.service.as_ref().and_then(SessionService::client) {
        state.connection_generation = state
            .background_generation_base
            .saturating_add(client.generation().0);
        if state.background_service.is_none() {
            state.background_service = Some(Arc::new(crate::background::BackgroundService::new(
                client.clone(),
            )));
        }
    }
    if matches!(&message, AppMessage::AttachmentResult { epoch, .. } if *epoch != state.attachment_epoch)
    {
        return false;
    }
    if matches!(
        &message,
        AppMessage::Attachment(_)
            | AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(_),
                ..
            })
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
    let effects = update(state, message);
    screen.sync_workspace_transcript(state.source_epoch, state.committed_cells);
    for effect in effects {
        match effect {
            AppEffect::Configure {
                generation,
                profile,
                home,
                client,
                consent,
                service,
                effects,
            } => {
                let sender = executor.sender.clone();
                executor.defer(move || {
                    tokio::spawn(async move {
                        let service = match (service, client) {
                            (Some(service), _) => Some(service),
                            (None, Some(client)) => {
                                crate::configure::discover_service(home, client, profile, consent)
                                    .await
                            }
                            _ => None,
                        };
                        if let Some(service) = service {
                            for effect in effects {
                                for input in service.execute(effect).await {
                                    let _ = sender.send(AppMessage::Configure {
                                        generation,
                                        service: Some(service.clone()),
                                        input,
                                    });
                                }
                            }
                        } else {
                            let _ = sender.send(AppMessage::Configure {
                                generation,
                                service: None,
                                input: talaria_core::configure::ConfigureInput::GatherFailed(
                                    "Not connected to Hermes. /configure needs the gateway.".into(),
                                ),
                            });
                        }
                    });
                });
            }
            AppEffect::Connectors {
                generation,
                profile,
                home,
                client,
                service,
                consent,
                effects,
                reads,
            } => {
                let requests: Vec<_> = effects
                    .into_iter()
                    .enumerate()
                    .map(|(index, effect)| {
                        let secrets = screen.connector_secrets(&effect);
                        (effect, secrets, reads.get(index).cloned().flatten())
                    })
                    .collect();
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let service = match (service, client) {
                        (Some(service), _) => Some(service),
                        (None, Some(client)) => {
                            crate::connectors::discover_service(home, &profile, client, consent)
                                .await
                        }
                        (None, None) => None,
                    };
                    for (effect, secrets, read) in requests {
                        let sender = sender.clone();
                        let service = service.clone();
                        let profile = profile.clone();
                        tokio::spawn(async move {
                            let inputs = if let Some(service) = &service {
                                service.execute(effect, secrets).await
                            } else {
                                crate::connectors::unavailable(effect)
                            };
                            for input in inputs {
                                let _ = sender.send(AppMessage::Connectors {
                                    generation,
                                    profile: profile.clone(),
                                    service: service.clone(),
                                    read: read.clone(),
                                    input,
                                });
                            }
                        });
                    }
                });
            }
            AppEffect::ConnectorWake {
                generation,
                profile,
                at,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
                    let _ = sender.send(AppMessage::Connectors {
                        generation,
                        profile,
                        service: None,
                        read: None,
                        input: talaria_core::connectors::ConnectorsInput::Tick,
                    });
                });
            }
            AppEffect::ConnectorLocal(effect) => {
                use talaria_core::connectors::ConnectorsEffect as E;
                match effect {
                    E::EditSecret { key } => screen.edit_connector_secret(&key),
                    E::ForgetSecrets => screen.forget_connector_secrets(),
                    E::CopyText(text) => screen.copy_provider_text(&text),
                    E::OpenDoctor => {
                        let _ = executor
                            .sender
                            .send(AppMessage::User(UserAction::Submit("/doctor".into())));
                    }
                    E::OpenUrl(url) => {
                        let sender = executor.sender.clone();
                        tokio::spawn(async move {
                            let opened = tokio::task::spawn_blocking(move || {
                                use std::process::{Command, Stdio};
                                let argv = crate::providers::opener_argv(&url)?;
                                Command::new(&argv[0])
                                    .args(&argv[1..])
                                    .stdin(Stdio::null())
                                    .stdout(Stdio::null())
                                    .stderr(Stdio::null())
                                    .spawn()
                                    .and_then(|mut child| child.wait())
                                    .ok()
                                    .filter(std::process::ExitStatus::success)
                            })
                            .await;
                            if !matches!(opened, Ok(Some(_))) {
                                let _ = sender.send(AppMessage::User(UserAction::Notice(
                                    "Could not open the link. Copy it and open it on a device where you can sign in.".into(),
                                )));
                            }
                        });
                    }
                    _ => {}
                }
            }
            AppEffect::Capabilities {
                generation,
                home,
                service,
                socket,
                effect,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    if let CapEffect::ScheduleTick { after_ms } = effect {
                        tokio::time::sleep(Duration::from_millis(after_ms)).await;
                        let _ = sender.send(AppMessage::Capabilities {
                            generation,
                            service: None,
                            input: CapInput::Tick,
                        });
                        return;
                    }
                    let service = match (service, socket) {
                        (Some(service), _) => Some(service),
                        (None, Some(socket)) => capabilities_service(home, socket).await,
                        (None, None) => None,
                    };
                    let inputs = if let Some(service) = &service {
                        service.execute(effect).await
                    } else {
                        capabilities_unavailable(effect)
                    };
                    for input in inputs {
                        let _ = sender.send(AppMessage::Capabilities {
                            generation,
                            service: service.clone(),
                            input,
                        });
                    }
                });
            }
            AppEffect::Memory {
                generation,
                profile,
                home,
                service,
                effects,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let service = match service {
                        Some(service) => Some(service),
                        None => crate::memory::discover_service(home, &profile).await,
                    };
                    let Some(service) = service else {
                        let _ = sender.send(AppMessage::Memory {
                            generation,
                            profile,
                            service: None,
                            input: talaria_core::memory::MemoryInput::NotConnected,
                        });
                        return;
                    };
                    for effect in effects {
                        let sender = sender.clone();
                        let service = service.clone();
                        let profile = profile.clone();
                        tokio::spawn(async move {
                            for input in service.execute(effect).await {
                                let _ = sender.send(AppMessage::Memory {
                                    generation,
                                    profile: profile.clone(),
                                    service: Some(service.clone()),
                                    input,
                                });
                            }
                        });
                    }
                });
            }
            AppEffect::CopyMemorySetup(text) => screen.copy_provider_text(&text),
            AppEffect::CopyReply { text } => screen.copy_mcp_text(&text),
            AppEffect::CronNotice(text) => {
                state.model_notice_counter = state.model_notice_counter.saturating_add(1);
                screen.commit_cells(&[Cell::Notice(talaria_core::session::NoticeCell {
                    id: talaria_core::session::CellId::new(u64::MAX - state.model_notice_counter),
                    kind: talaria_core::session::NoticeKind::Diagnostic,
                    text: format!("Scheduled jobs: {text}"),
                    detail: None,
                    body: None,
                })]);
            }
            AppEffect::CronFrame => {
                let focused = state
                    .cron
                    .state
                    .as_ref()
                    .is_some_and(CronState::history_focused);
                redraw |= screen.sync_cron(cron_view(state), focused);
            }
            AppEffect::PrepareCron {
                epoch,
                generation,
                profile,
                home,
                client,
            } => {
                executor.send_later(async move {
                    let service =
                        make_cron_service(client, home, profile.clone(), generation).await;
                    AppMessage::CronPrepared {
                        epoch,
                        profile,
                        service,
                    }
                });
            }
            AppEffect::Cron(effect) => {
                let service = state.cron.service.clone();
                let tag = service.as_ref().map_or_else(
                    || crate::cron::CronTag {
                        generation: state.cron.generation,
                        profile: state.profile.clone(),
                        run_name: match &effect {
                            CronEffect::Trigger { name, .. } => Some(name.clone()),
                            _ => None,
                        },
                    },
                    |service| service.tag_for(&effect),
                );
                let sender = executor.sender.clone();
                let task = async move {
                    let inputs = match service {
                        Some(service) => service.execute(effect).await,
                        None => cron_not_connected(effect).into_iter().collect(),
                    };
                    for input in inputs {
                        let _ = sender.send(AppMessage::Cron {
                            tag: tag.clone(),
                            input,
                        });
                    }
                };
                if state.cron.first_frame_pending {
                    executor.defer(move || {
                        tokio::spawn(task);
                    });
                } else {
                    executor.spawn(task);
                }
            }
            AppEffect::Settings {
                generation,
                profile,
                home,
                service,
                effect,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    use talaria_core::settings::{FailReason, SettingsEffect, SettingsInput};
                    if let SettingsEffect::ScheduleReveal { id, after } = effect {
                        tokio::time::sleep(after).await;
                        let _ = sender.send(AppMessage::Settings {
                            generation,
                            profile,
                            service: None,
                            input: SettingsInput::RevealTimeout(id),
                        });
                        return;
                    }
                    let service = if matches!(effect, SettingsEffect::Load { .. }) {
                        crate::settings::discover_service(home).await
                    } else {
                        service
                    };
                    let inputs = if let Some(service) = &service {
                        service.execute(effect).await
                    } else {
                        vec![SettingsInput::LoadFailed(FailReason::NotConnected)]
                    };
                    for input in inputs {
                        let _ = sender.send(AppMessage::Settings {
                            generation,
                            profile: profile.clone(),
                            service: service.clone(),
                            input,
                        });
                    }
                });
            }
            AppEffect::SettingsChoices {
                generation,
                profile,
                key,
                list,
                client,
                session,
            } => executor.send_later(async move {
                use talaria_core::settings::{FailReason, SettingsInput};
                let answer = match &client {
                    Some(client) => {
                        crate::settings::slash_choices(client, session.as_deref(), list).await
                    }
                    None => Err(FailReason::NotConnected.text()),
                };
                let input = match answer {
                    Ok(items) => SettingsInput::ChoicesLoaded {
                        key,
                        profile: profile.clone(),
                        items,
                    },
                    Err(reason) => SettingsInput::ChoicesFailed {
                        key,
                        profile: profile.clone(),
                        reason,
                    },
                };
                AppMessage::Settings {
                    generation,
                    profile,
                    service: None,
                    input,
                }
            }),
            AppEffect::ConfigCheck { profile, home } => {
                executor.send_later(async move {
                    use talaria_core::config_check::ConfigFacts;
                    // A configuration that cannot be read adds no notice.
                    let findings = match crate::settings::discover_service(home).await {
                        Some(service) => {
                            match crate::settings::config_facts(&service, &profile).await {
                                ConfigFacts::Read(findings) => findings,
                                ConfigFacts::NotTried | ConfigFacts::Unknown { .. } => Vec::new(),
                            }
                        }
                        None => Vec::new(),
                    };
                    AppMessage::ConfigFindings { profile, findings }
                });
            }
            AppEffect::DoctorGather => {
                let service = crate::doctor::DoctorService::new(state.hermes_home.clone());
                let profile = state.profile.clone();
                executor.send_later(async move {
                    AppMessage::DoctorFacts(service.gather(&profile).await)
                });
            }
            AppEffect::DoctorActiveWork => {
                let service = crate::doctor::DoctorService::new(state.hermes_home.clone());
                executor.send_later(async move {
                    AppMessage::DoctorRestart(RestartInput::ActiveWork(service.active_work().await))
                });
            }
            AppEffect::DoctorPost { old_instance } => {
                let service = crate::doctor::DoctorService::new(state.hermes_home.clone());
                let profile = state.profile.clone();
                let posted_at = Arc::clone(&executor.doctor_posted_at);
                executor.send_later(async move {
                    let result = service.post_restart(&old_instance, &profile).await;
                    *posted_at.lock().await = Some(Instant::now());
                    AppMessage::DoctorRestart(RestartInput::Posted(result))
                });
            }
            AppEffect::DoctorProbeAfter(delay) => {
                let service = crate::doctor::DoctorService::new(state.hermes_home.clone());
                let posted_at = Arc::clone(&executor.doctor_posted_at);
                let sender = executor.sender.clone();
                executor.spawn(async move {
                    let Some(posted_at) = *posted_at.lock().await else {
                        return;
                    };
                    tokio::time::sleep(delay).await;
                    let result = service.probe().await;
                    let _ = sender.send(AppMessage::DoctorRestart(RestartInput::Tick(
                        posted_at.elapsed(),
                    )));
                    let _ = sender.send(AppMessage::DoctorRestart(RestartInput::Probe(result)));
                });
            }
            AppEffect::Providers(ProvidersEffect::ForgetSecret) => screen.forget_provider_secret(),
            AppEffect::Providers(ProvidersEffect::CopyText(text)) => {
                screen.copy_provider_text(&text)
            }
            AppEffect::Providers(ProvidersEffect::OpenUrl(url)) => {
                let sender = executor.sender.clone();
                let generation = state.provider_generation;
                executor.spawn(async move {
                    let opened = tokio::task::spawn_blocking(move || {
                        use std::process::{Command, Stdio};
                        let argv = crate::providers::opener_argv(&url)?;
                        Command::new(&argv[0])
                            .args(&argv[1..])
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                            .and_then(|mut child| child.wait())
                            .ok()
                            .filter(std::process::ExitStatus::success)
                    })
                    .await;
                    if !matches!(opened, Ok(Some(_))) {
                        let _ = sender.send(AppMessage::Providers {
                            generation,
                            input: ProvidersInput::OpenFailed,
                        });
                    }
                });
            }
            AppEffect::ModelRoles {
                generation,
                profile,
                home,
                service,
                effects,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    use talaria_core::model_roles::RolesInput;
                    let service = match service {
                        Some(service) => Some(service),
                        None => crate::model_roles::open_service(home, profile.clone()).await,
                    };
                    for effect in effects {
                        let sender = sender.clone();
                        let profile = profile.clone();
                        let service = service.clone();
                        tokio::spawn(async move {
                            let inputs = match &service {
                                Some(service) => service.execute(effect).await,
                                None => vec![RolesInput::NotConnected],
                            };
                            for input in inputs {
                                let _ = sender.send(AppMessage::ModelRoles {
                                    generation,
                                    profile: profile.clone(),
                                    service: service.clone(),
                                    input,
                                });
                            }
                        });
                    }
                });
            }
            AppEffect::Providers(ProvidersEffect::Close) => {
                if let Some(service) = state.provider_service.clone() {
                    let closing = service.close();
                    executor.spawn(closing);
                }
            }
            AppEffect::Providers(effect) => {
                if matches!(
                    effect,
                    ProvidersEffect::LoadEnv
                        | ProvidersEffect::LoadOptions
                        | ProvidersEffect::LoadOauth
                        | ProvidersEffect::LoadEndpoints
                ) {
                    if state.provider_service.is_none() && !state.provider_initializing {
                        start_provider_service(state, executor);
                    }
                    if state.provider_initializing {
                        state.provider_pending_loads.push(effect);
                        continue;
                    }
                }
                let secret = screen.provider_secret(&effect);
                let service = state.provider_service.clone();
                let sender = executor.sender.clone();
                let generation = state.provider_generation;
                executor.defer(move || {
                    tokio::spawn(async move {
                        let inputs = match service {
                            Some(service) => service.execute(effect, secret).await,
                            None => provider_failed(
                                effect,
                                talaria_core::providers::Failure::NotSent(
                                    "the profile configuration service is not available".into(),
                                ),
                            )
                            .into_iter()
                            .collect(),
                        };
                        for input in inputs {
                            let _ = sender.send(AppMessage::Providers { generation, input });
                        }
                    });
                });
            }
            AppEffect::WorkflowPrompt {
                run,
                key,
                attempt,
                turn,
                reference,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let path = run.clone();
                    let text = tokio::task::spawn_blocking(move || {
                        talaria_workflow::journal::RunDir::open(&path)
                            .ok()
                            .and_then(|dir| {
                                talaria_workflow::progress::load_text(&dir, &reference).ok()
                            })
                            .map(|s| s.lines().next().unwrap_or_default().to_string())
                            .unwrap_or_else(|| "(prompt unavailable — journal blob missing)".into())
                    })
                    .await
                    .unwrap_or_else(|_| "(prompt unavailable — journal blob missing)".into());
                    let _ = sender.send(AppMessage::WorkflowProgress {
                        run,
                        input: talaria_workflow::progress::ProgressInput::PromptLoadedFor {
                            key,
                            attempt,
                            turn,
                            text,
                        },
                    });
                });
            }
            AppEffect::PrepareKanban { board } => {
                let home = state.hermes_home.clone();
                let profile = state.profile.clone();
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let result = async {
                        let endpoint = Arc::new(
                            hermes_client::discover_with(
                                &home,
                                &hermes_client::DiscoverOptions {
                                    ensure: hermes_client::EnsurePolicy::Never,
                                    ..Default::default()
                                },
                            )
                            .await
                            .map_err(|e| e.to_string())?,
                        );
                        let id = endpoint
                            .resolve_profile(&profile)
                            .map_err(|e| e.to_string())?;
                        let minter = Arc::new(hermes_client::NativeHttpMinter::new(
                            home,
                            endpoint.clone(),
                            id,
                        ));
                        Ok(crate::kanban::KanbanService::new(
                            endpoint.api_origin().into(),
                            minter,
                        ))
                    }
                    .await;
                    let _ = sender.send(AppMessage::KanbanPrepared { board, result });
                });
            }
            AppEffect::Kanban { board, effect } => {
                if let Some(service) = state.kanban_service.as_mut() {
                    service.execute(&board, effect, executor.sender.clone());
                }
            }
            AppEffect::HivecodeScan { since_ms } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let rows = tokio::task::spawn_blocking(move || {
                        talaria_workflow::runner::discover_runs_since(
                            &talaria_workflow::runner::runs_dir(None),
                            since_ms,
                        )
                    })
                    .await
                    .unwrap_or_default();
                    let _ = sender.send(AppMessage::HivecodeRuns { rows });
                });
            }
            AppEffect::WorkflowDiscover => {
                let sender = executor.sender.clone();
                let read_local_cache = !state.remote_gateway;
                tokio::spawn(async move {
                    if let Ok(picker) = tokio::task::spawn_blocking(move || {
                        crate::workflow_progress::load_picker(read_local_cache)
                    })
                    .await
                    {
                        let _ = sender.send(AppMessage::WorkflowDiscovered(picker));
                    }
                });
            }
            AppEffect::WorkflowOpen(path) => {
                let sender = executor.sender.clone();
                let read_local_cache = !state.remote_gateway;
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        crate::workflow_progress::open_run(path, read_local_cache)
                    })
                    .await
                    .unwrap_or_else(|_| Err("workflow view task failed".into()));
                    let _ = sender.send(AppMessage::WorkflowOpened(result));
                });
            }
            AppEffect::WorkflowAgentRead { stored_id, request } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        talaria_workflow::progress::agent_session::read_agent_journal(
                            &request.run_dir,
                            request.offset,
                        )
                    })
                    .await
                    .unwrap_or_else(|_| Err("workflow agent read task failed".into()));
                    let _ = sender.send(AppMessage::WorkflowAgentRead { stored_id, result });
                });
            }
            AppEffect::WorkflowPoll {
                run,
                offset,
                last_seq,
            } => {
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    let path = run.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        crate::workflow_progress::poll_run(&path, offset, last_seq)
                    })
                    .await
                    .unwrap_or_else(|_| Err("workflow view task failed".into()));
                    let _ = sender.send(AppMessage::WorkflowPolled { run, result });
                });
            }
            AppEffect::WorkflowExecute {
                run,
                effects,
                keys,
                at_ms,
            } => {
                let client = state.service.as_ref().and_then(|s| s.client()).cloned();
                crate::workflow_progress::spawn_effects(
                    run,
                    crate::workflow_progress::WorkflowConnection {
                        home: state.hermes_home.clone(),
                        fallback: client,
                        cache: executor.workflow_connections.clone(),
                    },
                    executor.sender.clone(),
                    effects,
                    keys,
                    at_ms,
                );
            }
            AppEffect::CommitSubagentBatch(batch) => screen.commit_subagent_batch(&batch),
            AppEffect::Background {
                session_id: _,
                effect: BackgroundEffect::Log { text },
            }
            | AppEffect::WorkflowLog(text) => {
                if let Some(home) = state.home.clone() {
                    tokio::task::spawn_blocking(move || {
                        let text: String = talaria_core::side::clean_output(&text).chars().filter(|ch| !ch.is_control() && !matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')).collect();
                        let _ = append_background_client_log(&home, &text);
                    });
                }
            }
            AppEffect::Background { session_id, effect } => {
                let current_generation = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .map_or(state.connection_generation, |client| {
                        state
                            .background_generation_base
                            .saturating_add(client.generation().0)
                    });
                let binding = match &effect {
                    BackgroundEffect::StopProcesses { request } => state
                        .background_stop_bindings
                        .remove(&(session_id.as_str().to_owned(), request.0)),
                    _ => None,
                };
                let BackgroundBinding {
                    service,
                    generation,
                } = binding.unwrap_or_else(|| BackgroundBinding {
                    service: state.background_service.clone(),
                    generation: current_generation,
                });
                let sender = executor.sender.clone();
                let started = executor.started;
                executor.spawn(async move {
                    let event = match service.filter(|_| generation >= current_generation) {
                        Some(service) => {
                            service
                                .execute(&session_id, effect, move || clock_ms(started))
                                .await
                        }
                        None => background_not_sent(effect, clock_ms(started)),
                    };
                    if let Some(event) = event {
                        let _ = sender.send(AppMessage::BackgroundDone {
                            session_id,
                            generation,
                            event,
                        });
                    }
                });
            }
            AppEffect::Rewind {
                owner,
                ticket,
                call,
            } => {
                let current = state.profile == owner.profile
                    && state
                        .opened
                        .as_ref()
                        .is_some_and(|opened| opened.session_id == owner.session_id);
                let client = current
                    .then(|| {
                        state
                            .service
                            .as_ref()
                            .and_then(SessionService::client)
                            .cloned()
                    })
                    .flatten();
                let session_id = current.then(|| owner.session_id.clone());
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
                    AppMessage::RewindReply { owner, input }
                });
            }
            AppEffect::Profiles(effect) => match effect {
                ProfilesEffect::Redraw => redraw = true,
                ProfilesEffect::ScheduleTick { after_ms, token } => {
                    if let Some(previous) = state.profiles_ticks.remove(&token) {
                        previous.abort();
                    }
                    let sender = executor.sender.clone();
                    let task = tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(after_ms)).await;
                        let _ = sender.send(AppMessage::Profiles(ProfilesInput::Tick { token }));
                    });
                    state.profiles_ticks.insert(token, task);
                }
                ProfilesEffect::CancelTicks | ProfilesEffect::Close => {
                    for (_, task) in std::mem::take(&mut state.profiles_ticks) {
                        task.abort();
                    }
                }
                ProfilesEffect::OpenEditor(text) => {
                    if let Some(result) = screen.edit_profile_soul(text) {
                        let _ = executor
                            .sender
                            .send(AppMessage::Profiles(ProfilesInput::EditorReturned(result)));
                    }
                }
                effect => {
                    let Some(client) = state
                        .service
                        .as_ref()
                        .and_then(SessionService::client)
                        .cloned()
                    else {
                        let home = state.hermes_home.clone();
                        let profile = state.profile.clone();
                        let sender = executor.sender.clone();
                        tokio::spawn(async move {
                            if let Some(input) =
                                profiles_without_client(effect, home, profile).await
                            {
                                let _ = sender.send(AppMessage::Profiles(input));
                            }
                        });
                        continue;
                    };
                    let service = crate::profiles::ProfilesService::new(
                        client,
                        state.hermes_home.clone(),
                        state.profile.clone(),
                    );
                    let sender = executor.sender.clone();
                    tokio::spawn(async move {
                        if let Some(input) = service.execute(effect).await {
                            let _ = sender.send(AppMessage::Profiles(input));
                        }
                    });
                }
            },
            AppEffect::CloseMcp(service) => {
                screen.forget_mcp_secrets();
                screen.sync_mcp(None);
                executor.spawn(service.closing());
            }
            AppEffect::PrepareMcp => {
                start_mcp_prepare(state, executor);
            }
            AppEffect::Mcp(effect) => {
                use talaria_core::mcp::{McpEffect, McpInput};
                match effect {
                    McpEffect::ForgetSecrets => screen.forget_mcp_secrets(),
                    McpEffect::OpenUrl(url) => {
                        if !screen.open_mcp_url(&url) {
                            let _ = executor.sender.send(AppMessage::Mcp {
                                binding: state.mcp_binding,
                                input: McpInput::OpenFailed,
                            });
                        }
                    }
                    McpEffect::CopyText(text) => screen.copy_mcp_text(&text),
                    McpEffect::Close => {
                        screen.forget_mcp_secrets();
                        screen.sync_mcp(None);
                        if state.mcp.pending_changes() {
                            screen.commit_lines(vec![
                                "· MCP changes are saved. /mcp apply uses them in this session."
                                    .to_owned(),
                            ]);
                        }
                        if let Some(service) = state.mcp_service.clone() {
                            executor.spawn(service.closing());
                        }
                    }
                    effect => {
                        if let Some(service) = state.mcp_service.clone() {
                            let secret = screen.take_mcp_secret(&effect);
                            let binding = state.mcp_binding;
                            let sender = executor.sender.clone();
                            executor.spawn(async move {
                                for input in service.execute(effect, secret).await {
                                    let _ = sender.send(AppMessage::Mcp { binding, input });
                                }
                            });
                        } else {
                            state.mcp_waiting.push(effect);
                            start_mcp_prepare(state, executor);
                        }
                    }
                }
                redraw |= state.mcp.is_open();
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
                        body: None,
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
            AppEffect::RecordSubmit(text) => {
                if let Some(notice) = screen.record_submit(&text, state.opened.is_some())
                    && let AppEffect::CommitLines(lines) = notice_lines(&notice)
                {
                    screen.commit_lines(lines);
                }
            }
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
                if let Effect::SendPrompt {
                    session_id,
                    submission_id,
                    ..
                } = &effect
                {
                    state
                        .voice_submissions
                        .insert(submission_id.clone(), session_id.as_str().to_owned());
                }
                let Some(service) = state.service.clone() else {
                    continue;
                };
                let _ = executor.prompts.send((service, effect));
            }
            AppEffect::Side { key, effect } => {
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
                    AppMessage::SideDone { key, events }
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
            AppEffect::OfferDraft(text) => screen.offer_draft(&text),
            AppEffect::ClearSubmittedDraft(text) => {
                if let Some((text, caret)) = screen.clear_submitted_draft(&text) {
                    redraw |= apply(
                        state,
                        AppMessage::DraftSynced { text, caret },
                        screen,
                        executor,
                    );
                }
            }
            AppEffect::ReconcileLater(event) => executor.send_later(async move {
                tokio::time::sleep(RECONCILE_WAIT).await;
                AppMessage::PromptDone {
                    events: vec![*event],
                    notices: Vec::new(),
                    masked: None,
                }
            }),
            AppEffect::Queue(effect) => {
                let (Some(client), Some(opened)) = (
                    state
                        .service
                        .as_ref()
                        .and_then(SessionService::client)
                        .cloned(),
                    &state.opened,
                ) else {
                    continue;
                };
                executor.queue_call(client, SessionId::new(&opened.session_id), effect);
            }
            AppEffect::AnswerMasked { target, value } => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    let result = prompts.lock().await.answer_masked(&target, value).await;
                    let notices = match result {
                        Ok(()) => Vec::new(),
                        Err(_) => vec![
                            "The answer could not be sent; Hermes may have moved on.".to_string(),
                        ],
                    };
                    (Vec::new(), notices)
                });
            }
            AppEffect::AnswerRecovered { request_id, value } => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(SessionService::client)
                    .cloned();
                executor.send_later(async move {
                    let result =
                        match client {
                            Some(client) => client
                                .call::<hermes_contract::methods::legacy::request_answer::Marker>(
                                    hermes_contract::schemas::RequestAnswerParams {
                                        id: request_id.clone(),
                                        profile: None,
                                        result: serde_json::Map::from_iter([(
                                            "value".to_owned(),
                                            json!(value.as_str()),
                                        )]),
                                    },
                                )
                                .await,
                            None => Err(hermes_client::CallError::NotConnected),
                        };
                    let fallback = "The answer could not be sent; Hermes may have moved on.";
                    let (events, notices) = match result {
                        Ok(reply)
                            if reply.status == hermes_contract::schemas::ClarifyLockStatus::Ok =>
                        {
                            (
                                vec![Event::ServerRequestAnswered { id: request_id }],
                                Vec::new(),
                            )
                        }
                        Err(hermes_client::CallError::Rpc(error))
                            if !error.message.trim().is_empty() =>
                        {
                            (Vec::new(), vec![error.message])
                        }
                        _ => (Vec::new(), vec![fallback.to_owned()]),
                    };
                    AppMessage::PromptDone {
                        events,
                        notices,
                        masked: None,
                    }
                });
            }
            AppEffect::ServerRequest(request) => {
                enqueue_server_request(state, executor, request);
            }
            AppEffect::CancelRequest { id, method, reason } => {
                enqueue_cancel_request(state, executor, id, method, reason);
            }
            AppEffect::Voice { session_id, effect } => {
                run_voice_effect(state, executor, session_id, effect)
            }
            AppEffect::VoiceStatus { session_id } => run_voice_status(state, executor, session_id),
            AppEffect::ClientVoiceCapture { session_id, epoch } => {
                if crate::voice::transport::remote_audio_allowed(
                    state.remote_gateway,
                    state.client_voice_config.remote_audio,
                ) && let Some(worker) = &state.client_voice_worker
                {
                    worker.capture(
                        state.client_voice_config.capture_command.clone(),
                        state.profile.clone(),
                        session_id,
                        state.connection_generation,
                        epoch,
                    );
                }
            }
            AppEffect::ClientVoiceFinish { epoch: _ } => {
                if let Some(worker) = &state.client_voice_worker {
                    worker.finish_capture();
                }
            }
            AppEffect::ClientVoiceSpeak {
                session_id,
                text,
                audio_epoch,
            } => {
                if crate::voice::transport::remote_audio_allowed(
                    state.remote_gateway,
                    state.client_voice_config.remote_audio,
                ) && let Some(worker) = &state.client_voice_worker
                {
                    let sentences =
                        talaria_core::voice::speakable::sentence_chunks(&text, audio_epoch)
                            .into_iter()
                            .map(|chunk| chunk.text)
                            .collect();
                    worker.speak_sentences(crate::voice::relay::SpeechRequest {
                        command: state.client_voice_config.playback_command.clone(),
                        profile: state.profile.clone(),
                        session_id,
                        generation: state.connection_generation,
                        audio_epoch: Some(audio_epoch),
                        sentences,
                    });
                }
            }
            AppEffect::ClientVoiceCancelCapture => {
                if let Some(worker) = &state.client_voice_worker {
                    worker.cancel_capture();
                }
            }
            AppEffect::ClientVoiceStop {
                session_id,
                audio_epoch,
            } => {
                let closing = state
                    .client_voice_worker
                    .as_ref()
                    .map(|worker| worker.stop_playback());
                let sender = executor.sender.clone();
                let generation = state.connection_generation;
                tokio::spawn(async move {
                    let quiet = match closing {
                        Some(closing) => closing.await,
                        None => false,
                    };
                    let input = if quiet {
                        talaria_core::voice::client::Input::Quiet { audio_epoch }
                    } else {
                        talaria_core::voice::client::Input::QuietTimeout { audio_epoch }
                    };
                    let _ = sender.send(AppMessage::ClientVoice {
                        session_id,
                        generation,
                        input,
                        notice: None,
                    });
                });
            }
            AppEffect::ClientVoiceClose => {
                if let Some(worker) = &state.client_voice_worker {
                    let closing = worker.close();
                    state
                        .client_voice_closing
                        .retain(|task| !task.is_finished());
                    state.client_voice_closing.push(tokio::spawn(closing));
                }
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
            AppEffect::InstallRecovered { ids } => {
                let Some(prompts) = &state.prompts else {
                    continue;
                };
                executor.prompt_task(prompts, move |prompts| async move {
                    prompts.lock().await.install_recovered(&ids);
                    (Vec::new(), Vec::new())
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
                executor.send_later(async move {
                    let session_id = gap.session_id.clone();
                    AppMessage::BackgroundRecovered {
                        session_id,
                        result: service.recover(gap).await,
                    }
                });
            }
            AppEffect::Quit => state.quitting = true,
            AppEffect::Hivecode { action, request_id } => match action {
                crate::hivecode::Action::FetchCatalog {
                    session,
                    profile,
                    command,
                } => {
                    let client = state.service.as_ref().and_then(|s| s.client()).cloned();
                    executor.send_later(async move {
                        let result = match client {
                            Some(client) => {
                                crate::commands::fresh_workflow_catalog(&client, session.as_str())
                                    .await
                            }
                            None => Err("not connected".into()),
                        };
                        AppMessage::HivecodeCatalog {
                            request_id,
                            session,
                            profile,
                            command,
                            result,
                        }
                    });
                }
                crate::hivecode::Action::RunCommand {
                    session,
                    original_text,
                    restore_text,
                    command_line,
                } => {
                    let runner = state.command_service.as_ref().map(|s| s.runner(None));
                    executor.send_later(async move {
                        let outcome = match runner {
                            Some(runner) => {
                                runner.workflow_raw(session.as_str(), &command_line).await
                            }
                            None => Err(talaria_core::commands::ErrorView::NotConnected),
                        };
                        AppMessage::HivecodeDone {
                            session,
                            original_text,
                            restore_text,
                            outcome,
                        }
                    });
                }
                crate::hivecode::Action::CheckSetup { session, profile } => {
                    let Some(talaria_home) = talaria_workflow::staffing::talaria_home() else {
                        continue;
                    };
                    let hermes_home = hermes_client::default_home();
                    executor.send_later(async move {
                        let read_profile = profile.clone();
                        let facts = tokio::task::spawn_blocking(move || {
                            crate::hivecode_files::read_setup(
                                &read_profile,
                                &hermes_home,
                                &talaria_home,
                            )
                        })
                        .await
                        .unwrap_or(crate::hivecode::SetupFacts {
                            skill: crate::hivecode::SkillCheck::Unknown,
                            tiers: crate::hivecode::TierCheck::Present,
                            missing_builtins: Vec::new(),
                        });
                        AppMessage::HivecodeSetup {
                            session,
                            profile,
                            facts,
                        }
                    });
                }
                action @ (crate::hivecode::Action::InstallSkill { .. }
                | crate::hivecode::Action::ReinstallSkill { .. }) => {
                    // A reinstall replaces a changed copy after backing it up.
                    let (profile, force) = match action {
                        crate::hivecode::Action::ReinstallSkill { profile, .. } => (profile, true),
                        crate::hivecode::Action::InstallSkill { profile, .. } => (profile, false),
                        _ => continue,
                    };
                    executor.send_later(async move {
                        let result = async {
                            let (target, warnings) =
                                talaria_workflow::skill::resolve_target(&profile).await?;
                            let home =
                                talaria_workflow::staffing::talaria_home().ok_or_else(|| {
                                    talaria_workflow::skill::SkillError::Template(
                                        "no home folder for Talaria".into(),
                                    )
                                })?;
                            let builtins_home = home.clone();
                            let options = talaria_workflow::skill::InstallOptions {
                                command: crate::hivecode_files::INSTALL_COMMAND.into(),
                                force,
                                dry_run: false,
                                talaria_home: home,
                                now: chrono::Utc::now(),
                                lock_wait: Duration::from_secs(10),
                            };
                            let owned_target = target.clone();
                            let mut report = tokio::task::spawn_blocking(move || {
                                talaria_workflow::skill::install(&owned_target, &options)
                            })
                            .await
                            .map_err(|error| {
                                talaria_workflow::skill::SkillError::Template(error.to_string())
                            })??;
                            report.warnings.splice(0..0, warnings);
                            // The built-in workflows go in with the skill, so the agent has
                            // something to start from. A copy already there is kept.
                            let builtins = tokio::task::spawn_blocking(move || {
                                crate::hivecode_files::install_missing_builtins(&builtins_home)
                            })
                            .await
                            .map_err(|error| error.to_string())
                            .and_then(|result| result);
                            let builtins_note = match builtins {
                                Ok(done) => talaria_core::hivecode::notice::builtins_outcome(
                                    &done.created,
                                    &done.unreadable,
                                ),
                                Err(reason) => {
                                    Some(talaria_core::hivecode::notice::builtins_failed(&reason))
                                }
                            };
                            // `/hivecode reinstall` confirms in one line.
                            let text = if force {
                                talaria_core::hivecode::notice::reinstalled(
                                    &target.profile,
                                    report
                                        .backup
                                        .as_ref()
                                        .map(|backup| backup.display().to_string())
                                        .as_deref(),
                                )
                            } else {
                                talaria_workflow::skill::report_text(&target, &report, false, false)
                            };
                            Ok::<_, talaria_workflow::skill::SkillError>(match builtins_note {
                                Some(note) => format!("{text}\n{note}"),
                                None => text,
                            })
                        }
                        .await;
                        let text = match result {
                            Ok(text) => text,
                            Err(error) if force => {
                                talaria_core::hivecode::notice::reinstall_failed(&error.to_string())
                            }
                            Err(error) => error.to_string(),
                        };
                        AppMessage::HivecodeNotice { text }
                    });
                }
                _ => {}
            },
            AppEffect::HivecodeSave {
                generation,
                changes,
            } => {
                if let Some(file) = state.hivecode_file.clone() {
                    executor.spawn(async move {
                        let _ =
                            tokio::task::spawn_blocking(move || file.write(generation, &changes))
                                .await;
                    });
                }
            }
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
            AppEffect::ListSessions { generation } => {
                let side = Arc::clone(&executor.side);
                let sender = executor.sender.clone();
                let side_open = Arc::clone(&executor.side_open);
                let home = state.hermes_home.clone();
                let profile = state.profile.clone();
                let foreground = foreground_row(state);
                // The overlay has to be on screen before the first list frame reaches
                // the gateway, so the listing waits for the next frame.
                executor.defer(move || {
                    tokio::spawn(async move {
                        let mut service = side.lock().await;
                        if !side_open.load(Ordering::SeqCst) {
                            // The first listing opens the side connection; its scope tells the
                            // picker which profiles it may reach.
                            match service.open(&home, &profile).await {
                                Ok(scope) => {
                                    side_open.store(true, Ordering::SeqCst);
                                    let (profiles, scope_note) = scope_parts(&scope);
                                    let _ = sender.send(AppMessage::Picker(PickerInput::Opened {
                                        profiles,
                                        foreground,
                                        scope_note,
                                        cached: Vec::new(),
                                        generation,
                                    }));
                                }
                                Err(error) => {
                                    let _ =
                                        sender.send(AppMessage::Picker(PickerInput::SideFailed {
                                            reason: error.phrase().to_owned(),
                                        }));
                                }
                            }
                            return;
                        }
                        // The listing runs in its own tasks, so a slow profile never holds
                        // the loop.
                        let (tx, mut rx) = mpsc::channel(64);
                        let forward = sender.clone();
                        tokio::spawn(async move {
                            while let Some(message) = rx.recv().await {
                                if forward.send(message).is_err() {
                                    break;
                                }
                            }
                        });
                        service.start_list(tx);
                    });
                });
            }
            AppEffect::Switch { key } => {
                let mut copy = state.working_copy();
                executor.send_later(async move {
                    let outcome = crate::switch::switch_foreground(&mut copy, key).await;
                    let reason = copy.switch_reason.take();
                    let install = copy.into_install();
                    AppMessage::Switched {
                        outcome,
                        install,
                        reason,
                    }
                });
            }
            AppEffect::StartInProfile { profile, cwd } => {
                let mut copy = state.working_copy();
                executor.send_later(async move {
                    let outcome = crate::switch::start_in_profile(&mut copy, profile, cwd).await;
                    let install = copy.into_install();
                    AppMessage::Switched {
                        outcome,
                        install,
                        reason: None,
                    }
                });
            }
            AppEffect::ClearSession { skip_detach, cwd } => {
                let mut copy = state.working_copy();
                executor.send_later(async move {
                    let Some(old) = copy.opened.clone() else {
                        return AppMessage::SessionCleared(SessionCleared::Skipped);
                    };
                    let Some(service) = copy.service.clone() else {
                        if skip_detach {
                            return AppMessage::SessionCleared(SessionCleared::OpenFailed {
                                previous_id: old.session_id,
                                reason: "Talaria is not connected to Hermes".into(),
                                left: false,
                            });
                        }
                        return AppMessage::SessionCleared(SessionCleared::DetachFailed {
                            reason: "Talaria is not connected to Hermes".into(),
                        });
                    };
                    if !skip_detach {
                        crate::switch::stop_voice(&mut copy, &service, &old.session_id).await;
                        match service.detach(&old).await {
                            DetachOutcome::Detached | DetachOutcome::StaleSubscription => {}
                            DetachOutcome::Failed { reason } => {
                                return AppMessage::SessionCleared(SessionCleared::DetachFailed {
                                    reason,
                                });
                            }
                            DetachOutcome::Skipped => {
                                return AppMessage::SessionCleared(SessionCleared::Skipped);
                            }
                        }
                    }
                    let request = crate::launch::NewSession {
                        request_id: new_request_id(),
                        source: crate::launch::SESSION_SOURCE.into(),
                        cwd,
                        model: None,
                        provider: None,
                        reasoning: None,
                    };
                    match service
                        .open(crate::launch::SessionRequest::New(request))
                        .await
                    {
                        Ok(opened) => {
                            copy.opened = Some(opened);
                            match copy.into_install() {
                                Some(install) => {
                                    AppMessage::SessionCleared(SessionCleared::Started {
                                        install: Box::new(install),
                                        resume_previous: !skip_detach,
                                    })
                                }
                                None => AppMessage::SessionCleared(SessionCleared::Skipped),
                            }
                        }
                        Err(error) => AppMessage::SessionCleared(SessionCleared::OpenFailed {
                            previous_id: old.session_id,
                            reason: crate::switch::refusal_reason(&error),
                            left: !skip_detach,
                        }),
                    }
                });
            }
            AppEffect::BranchSession { request } => {
                let client = state
                    .service
                    .as_ref()
                    .and_then(|service| service.client().cloned());
                executor.send_later(async move {
                    match client {
                        Some(client) => {
                            let outcome = MutationClient::new(client).send(&request).await;
                            AppMessage::BranchSent { request, outcome }
                        }
                        None => AppMessage::Branched {
                            outcome: BranchOutcome::Refused {
                                text: "Talaria is not connected to Hermes.".to_owned(),
                            },
                            install: None,
                        },
                    }
                });
            }
            AppEffect::OpenBranchChild { receipt, name } => {
                let mut copy = state.working_copy();
                executor.send_later(async move {
                    let outcome =
                        crate::switch::open_branch_child(&mut copy, &receipt, name.as_deref())
                            .await;
                    let install = copy.into_install();
                    AppMessage::Branched { outcome, install }
                });
            }
            AppEffect::RenameRow {
                key,
                title,
                request,
            } => {
                let side = Arc::clone(&executor.side);
                let side_open = Arc::clone(&executor.side_open);
                let home = state.hermes_home.clone();
                let profile = state.profile.clone();
                executor.send_later(async move {
                    let outcome = match side_service(&side, &side_open, &home, &profile).await {
                        Ok(mut service) => service.send_rename(&key, &request).await,
                        Err(text) => Err(text),
                    };
                    AppMessage::RowRenamed {
                        key,
                        title,
                        request,
                        outcome,
                    }
                });
            }
            AppEffect::ExportSession {
                key,
                format,
                redact,
                target,
            } => {
                let side = Arc::clone(&executor.side);
                let side_open = Arc::clone(&executor.side_open);
                let home = state.hermes_home.clone();
                let profile = state.profile.clone();
                executor.send_later(async move {
                    let result = match side_service(&side, &side_open, &home, &profile).await {
                        Ok(mut service) => service.export(&key, format, redact, target).await,
                        Err(text) => ExportResult::Failed { text },
                    };
                    AppMessage::Exported {
                        result,
                        format,
                        redact,
                    }
                });
            }
            AppEffect::Mutate { request, .. } => {
                let Some(service) = state.service.clone() else {
                    continue;
                };
                let Some(client) = service.client().cloned() else {
                    continue;
                };
                executor.send_later(async move {
                    let outcome = MutationClient::new(client).send(&request).await;
                    AppMessage::Mutated { request, outcome }
                });
            }
            AppEffect::CloseLink(link) => {
                executor.spawn(async move { link.close().await });
            }
            AppEffect::BindLinkServices => bind_link_services(state, executor),
            AppEffect::FleetPeek(effect) => {
                use talaria_core::fleet_view::FleetViewEffect;
                let closing = matches!(effect, FleetViewEffect::Close);
                if matches!(effect, FleetViewEffect::Close | FleetViewEffect::ClosePeek)
                    && state.peek_worker.is_none()
                {
                    continue;
                }
                let result = peek_worker(state).run(effect);
                let sender = executor.sender.clone();
                executor.spawn(async move {
                    if let Ok(inputs) = result.await {
                        for input in inputs {
                            let _ = sender.send(AppMessage::FleetView(input));
                        }
                    }
                });
                if closing {
                    drop_peek_worker(state);
                }
            }
            AppEffect::Review(effect) => {
                if let Some(service) = &state.review_service {
                    service.execute(effect, executor.sender.clone());
                }
            }
            AppEffect::ReviewLocate { row, origin } => {
                let job = review_locate_job(state, row, origin);
                let sender = executor.sender.clone();
                executor.spawn(async move {
                    let message = job.run().await;
                    let _ = sender.send(message);
                });
            }
            AppEffect::FleetAnswer(intent) => {
                let Some(worker) = &state.peek_worker else {
                    continue;
                };
                let result = worker.answer(intent);
                let sender = executor.sender.clone();
                executor.spawn(async move {
                    if let Ok(notices) = result.await
                        && !notices.is_empty()
                    {
                        let _ = sender.send(AppMessage::Peek(
                            crate::fleet_actions::PeekReport::Notices(notices),
                        ));
                    }
                });
            }
            AppEffect::FleetVoiceAnswer { key, answer } => {
                use talaria_core::fleet_view::{FleetViewInput, ReplyOutcome};
                let Some(worker) = &state.peek_worker else {
                    let _ =
                        executor
                            .sender
                            .send(AppMessage::FleetView(FleetViewInput::ReplyDone {
                                key,
                                outcome: ReplyOutcome::NotAttached {
                                    reason: "the answer card is unavailable".into(),
                                },
                            }));
                    continue;
                };
                let result = worker.answer(answer);
                executor.send_later(async move {
                    let outcome = match result.await {
                        Ok(notices) if notices.is_empty() => ReplyOutcome::Sent,
                        Ok(notices) => ReplyOutcome::Refused {
                            reason: notices
                                .into_iter()
                                .map(|notice| notice.text)
                                .collect::<Vec<_>>()
                                .join(" "),
                        },
                        Err(_) => ReplyOutcome::Unknown {
                            why: "the answer service stopped".into(),
                        },
                    };
                    AppMessage::FleetView(FleetViewInput::ReplyDone { key, outcome })
                });
            }
            AppEffect::FleetInput(input) => {
                let _ = executor.sender.send(AppMessage::FleetView(input));
            }
            // The overlay's input path turns these into ordinary effects before they get here.
            AppEffect::FleetForeground(_) => {}
            AppEffect::CloseSide => {
                let side = Arc::clone(&executor.side);
                executor.side_open.store(false, Ordering::SeqCst);
                executor.spawn(async move {
                    let mut service = side.lock().await;
                    service.close().await;
                });
            }
            AppEffect::ReportPane(pane) => screen.report_pane(
                &pane,
                state
                    .opened
                    .as_ref()
                    .map(|o| (o.session_id.as_str(), state.profile.as_str())),
            ),
            AppEffect::ReportSeat(seat) => screen.report_seat(seat),
            AppEffect::Alert(alert) => {
                screen.alert(&alert);
                if alert.speech
                    && state.uses_client_voice()
                    && state.client_voice_enabled
                    && crate::voice::transport::remote_audio_allowed(
                        state.remote_gateway,
                        state.client_voice_config.remote_audio,
                    )
                    && !state.client_voice.active()
                    && let (Some(opened), Some(worker)) =
                        (&state.opened, &state.client_voice_worker)
                {
                    let _ = state
                        .client_voice
                        .update(talaria_core::voice::client::Input::SpeechStarted);
                    let audio_epoch = state.client_voice.audio_epoch();
                    let sentences =
                        talaria_core::voice::speakable::sentence_chunks(&alert.text, audio_epoch)
                            .into_iter()
                            .map(|chunk| chunk.text)
                            .collect();
                    worker.speak_sentences(crate::voice::relay::SpeechRequest {
                        command: state.client_voice_config.playback_command.clone(),
                        profile: state.profile.clone(),
                        session_id: opened.session_id.clone(),
                        generation: state.connection_generation,
                        audio_epoch: Some(audio_epoch),
                        sentences,
                    });
                    redraw = true;
                }
            }
            AppEffect::Appearance => {
                screen.appearance_changed(&state.appearance.view());
                redraw = true;
            }
            AppEffect::ThemeReport => {
                if let Some(report) = screen.theme_report() {
                    let mut lines = theme_lines(&report);
                    lines.push(format!(
                        "  Built-in themes: {}",
                        crate::theme_file::BUILTIN_THEMES.join(", ")
                    ));
                    if let Some(file) = screen.theme_file_report() {
                        lines.extend(appearance::theme_file_lines(&file));
                    }
                    if let AppEffect::CommitLines(lines) = notice_lines(&lines.join("\n")) {
                        screen.commit_lines(lines);
                    }
                }
            }
            AppEffect::ThemeFileCommand(argument) => {
                run_theme_file_command(state, executor, argument)
            }
            AppEffect::RefreshThemeProfile => {
                refresh_theme_profile(state, executor);
            }
            AppEffect::ContinueThemeFiles => start_next_theme_file_command(state, executor),
            AppEffect::Choice(work) => {
                let ignored = screen.theme_report().and_then(|report| report.not_applied);
                choice::run(state, executor, work, ignored);
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
            AppEffect::Automation(effect) => {
                let Some(client) = state.service.as_ref().and_then(SessionService::client) else {
                    continue;
                };
                let service = AutomationService::new(client.clone());
                let sender = executor.sender.clone();
                tokio::spawn(async move {
                    if let Some(event) = service.execute(effect).await {
                        let _ = sender.send(AppMessage::Automation(event));
                    }
                });
            }
            AppEffect::WakeAt(at) => {
                if let Some(old) = state.fleet_wake_task.take() {
                    old.abort();
                }
                let sender = executor.sender.clone();
                state.fleet_wake_task = Some(crate::fleet_attention::wake_task(at, move || {
                    let _ = sender.send(AppMessage::AttentionWake);
                }));
            }
            // The fleet view opens the row the jump key named. The overlay is the
            // fleet view unit's own; this is the path to it from here.
            AppEffect::OpenFleetViewAt(key) => {
                redraw |= apply(state, AppMessage::FleetOpenAt(key), screen, executor);
            }
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

async fn make_provider_service(
    home: PathBuf,
    profile_name: String,
    client: hermes_client::Client,
) -> Result<Arc<crate::providers::ProviderService>, talaria_core::providers::Failure> {
    use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
    use talaria_core::providers::Failure;
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..Default::default()
    };
    let endpoint = hermes_client::discover_with(&home, &options)
        .await
        .map_err(|_| {
            Failure::NotSent("could not discover the profile configuration service".into())
        })?;
    let profile = endpoint
        .resolve_profile(&profile_name)
        .map_err(|_| Failure::NotSent("the connected profile is not served".into()))?;
    let endpoint = Arc::new(endpoint);
    let minter = NativeHttpMinter::new(home, Arc::clone(&endpoint), profile);
    let rest = RestClient::new(
        endpoint.api_origin(),
        Arc::new(minter),
        RestOptions::default(),
    )
    .map_err(|_| Failure::NotSent("could not prepare the profile configuration service".into()))?;
    Ok(Arc::new(crate::providers::ProviderService::new(
        client,
        rest,
        profile_name,
    )))
}

fn provider_failed(
    effect: ProvidersEffect,
    failure: talaria_core::providers::Failure,
) -> Option<ProvidersInput> {
    Some(match effect {
        ProvidersEffect::LoadEnv => ProvidersInput::EnvLoaded(Err(failure)),
        ProvidersEffect::LoadOptions => ProvidersInput::OptionsLoaded(Err(failure)),
        ProvidersEffect::LoadOauth => ProvidersInput::OauthLoaded(Err(failure)),
        ProvidersEffect::LoadEndpoints => ProvidersInput::EndpointsLoaded(Err(failure)),
        ProvidersEffect::CheckKey { .. } => ProvidersInput::KeyChecked(Err(failure)),
        ProvidersEffect::SaveKey { .. } => ProvidersInput::KeySaved(Err(failure)),
        ProvidersEffect::Disconnect { .. } => ProvidersInput::Disconnected(Err(failure)),
        ProvidersEffect::StartSignIn { attempt, .. } => ProvidersInput::SignInStarted {
            attempt,
            result: Err(failure),
        },
        ProvidersEffect::Poll { session_id, .. } => ProvidersInput::SignInPolled {
            session_id,
            result: Err(failure),
        },
        ProvidersEffect::SignOut { .. } => ProvidersInput::SignedOut(Err(failure)),
        ProvidersEffect::CheckEndpoint(_) => ProvidersInput::EndpointChecked(Err(failure)),
        ProvidersEffect::SaveEndpoint(_) => ProvidersInput::EndpointSaved(Err(failure)),
        ProvidersEffect::DeleteEndpoint { .. } => ProvidersInput::EndpointDeleted(Err(failure)),
        _ => return None,
    })
}

/// Take the opened session's handles into the state and start forwarding link events.
fn install(state: &mut AppState, ready: Ready, executor: &Executor) -> AppMessage {
    let Ready {
        connection,
        service,
        opened,
        notice,
    } = ready;
    state.gateway_version = connection.gateway_version.clone();
    let OpenConnection {
        link,
        inbound,
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
    if state.profile != profile_name {
        forget_profile_views(state);
    }
    state.profile = profile_name;
    if state.appearance.set_file_profile(&state.profile) {
        refresh_theme_profile(state, executor);
    }
    state.link = Some(link);
    state.background_service = service
        .client()
        .cloned()
        .map(crate::background::BackgroundService::new)
        .map(Arc::new);
    state.service = Some(service);
    bind_link_services(state, executor);
    let counter = Arc::clone(&state.counter);
    let launch_id = state.launch_id.clone();
    state.prompts = Some(Arc::new(Mutex::new(PromptService::new(Box::new(
        move || submission_id(&launch_id, counter.fetch_add(1, Ordering::SeqCst) + 1),
    )))));
    forward_link_events(
        Arc::clone(&state.link_epoch),
        state.link_epoch.load(Ordering::SeqCst),
        executor.sender.clone(),
        inbound,
    );
    AppMessage::Opened { opened, notice }
}

/// Drop what the memory and connectors views hold about the profile Talaria is leaving.
fn forget_profile_views(state: &mut AppState) {
    state.memory_visible = false;
    state.memory_profile = None;
    state.memory_service = None;
    state.memory = talaria_core::memory::MemoryState::new();
    state.memory_generation = state.memory_generation.wrapping_add(1);
    state.connectors_open = false;
    state.connectors_profile = None;
    state.connectors_service = None;
    state.connectors_deadline = None;
    state.connectors_reads.clear();
    state.connectors_generation = state.connectors_generation.wrapping_add(1);
    state.connectors = talaria_core::connectors::ConnectorsState::new();
}

/// Bind the services that hold a client to the foreground link's client: Hermes commands,
/// the approval settings, workspace changes, the provider list and the model picker.
///
/// Start-up does this once. A switch to a session on another profile opens a new link, so it
/// has to do it again; before it did, these services kept calling the closed link, and the
/// approval setting read as unknown with "Talaria is not connected to Hermes".
fn bind_link_services(state: &mut AppState, executor: &Executor) {
    let Some(client) = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned()
    else {
        return;
    };
    let mut commands = CommandService::new(client.clone());
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
    state.permissions_service = Some(Arc::new(PermissionsService::new(Arc::new(client.clone()))));
    state.workspace_service = Some(Arc::new(WorkspaceService::new(MutationClient::new(
        client.clone(),
    ))));
    state.provider_generation = state.provider_generation.wrapping_add(1);
    state.provider_pending_loads.clear();
    if let Some(service) = state.provider_service.take() {
        executor.spawn(service.close());
    }
    start_provider_service(state, executor);
    state.model_service = Some(Arc::new(crate::models::ModelService::new(
        client,
        crate::config::models_path().unwrap_or_default(),
    )));
}

fn run_theme_file_command(state: &mut AppState, executor: &Executor, argument: String) {
    if state.appearance.theme_files().is_none() {
        let Some(home) = &state.home else {
            let _ = executor.sender.send(AppMessage::ThemeFileDone {
                generation: state.appearance.file_generation(),
                result: Err(
                    "Theme files are unavailable because the home directory is unknown.".into(),
                ),
            });
            return;
        };
        state
            .appearance
            .set_theme_files(appearance::ThemeFileSession::new(
                crate::config::user_config_path(home),
                crate::config::Config::default().theme,
                &state.profile,
            ));
    }
    state
        .appearance
        .queue_file_work(ThemeFileWork::Command(argument));
    start_next_theme_file_command(state, executor);
}

fn refresh_theme_profile(state: &mut AppState, executor: &Executor) {
    state
        .appearance
        .queue_file_work(ThemeFileWork::ProfileChanged);
    start_next_theme_file_command(state, executor);
}

fn start_next_theme_file_command(state: &mut AppState, executor: &Executor) {
    let Some((generation, mut files, work)) = state.appearance.begin_file_request() else {
        return;
    };
    let startup = state.appearance.startup_polarity();
    executor.send_later(async move {
        let result = tokio::task::spawn_blocking(move || {
            let lines = files.run_work(work, startup);
            (Box::new(files), lines)
        })
        .await
        .map_err(|_| {
            "The theme choice could not be applied; the last good theme is unchanged.".to_owned()
        });
        AppMessage::ThemeFileDone { generation, result }
    });
}

/// Finish choices already confirmed before the terminal closed, including queued writes.
async fn flush_theme_file_commands(
    state: &mut AppState,
    messages: &mut mpsc::UnboundedReceiver<AppMessage>,
    executor: &Executor,
    stderr: &mut String,
) {
    let finished = tokio::time::timeout(QUIT_WAIT, async {
        while state.appearance.file_loading() {
            let Some(message) = messages.recv().await else {
                break;
            };
            if matches!(message, AppMessage::ThemeFileDone { .. }) {
                for effect in update(state, message) {
                    match effect {
                        AppEffect::CommitLines(lines) => {
                            for line in lines {
                                stderr.push_str(&line);
                                stderr.push('\n');
                            }
                        }
                        AppEffect::ContinueThemeFiles => {
                            start_next_theme_file_command(state, executor);
                        }
                        _ => {}
                    }
                }
            }
        }
    })
    .await;
    if finished.is_err() {
        stderr.push_str("Some theme choices could not be confirmed saved before exit.\n");
    }
}

fn start_provider_service(state: &mut AppState, executor: &Executor) {
    state.provider_initializing = true;
    let provider_generation = state.provider_generation;
    let provider_home = state.hermes_home.clone();
    let provider_profile = state.profile.clone();
    let provider_client = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned();
    executor.send_later(async move {
        let result = match provider_client {
            Some(client) => make_provider_service(provider_home, provider_profile, client).await,
            None => Err(talaria_core::providers::Failure::NotSent(
                "no gateway connection is open".into(),
            )),
        };
        AppMessage::ProviderReady {
            generation: provider_generation,
            result,
        }
    });
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

/// Report the foreground session to the fleet service: the row on a switch, and the view only
/// when it changed since the last report.
fn sync_fleet(state: &mut AppState) {
    let Some(fleet) = &state.fleet_service else {
        return;
    };
    let current = state.opened.as_ref().map(|opened| {
        let key = RowKey {
            profile: state.profile.clone(),
            session_id: opened.session_id.clone(),
        };
        let masked: Vec<(String, talaria_core::attention::NeedSummary)> = state
            .masked
            .iter()
            .filter(|request| {
                request
                    .session_id
                    .as_deref()
                    .is_none_or(|id| id == opened.session_id)
            })
            .map(|request| (request.id.clone(), masked_summary(request.kind)))
            .collect();
        let mut view = talaria_core::fleet::SubscribedView::from_state(&opened.state, &masked);
        if view.title.is_none() {
            view.title.clone_from(&opened.title);
        }
        (key, view)
    });
    for report in crate::fleet::foreground_reports(state.fleet_reported.as_ref(), current.as_ref())
    {
        fleet.report(report);
    }
    state.fleet_reported = current;
}

fn masked_summary(kind: crate::prompts::MaskedKind) -> talaria_core::attention::NeedSummary {
    use crate::prompts::MaskedKind;
    use talaria_core::attention::NeedSummary;
    match kind {
        MaskedKind::Sudo => NeedSummary::Sudo,
        MaskedKind::Secret => NeedSummary::Secret,
        MaskedKind::VaultCode => NeedSummary::VaultCode,
        MaskedKind::VaultUnlockPrompt => NeedSummary::VaultUnlock,
    }
}

/// The foreground row's reply or stop, run through the foreground's own paths.
pub enum ForegroundJob {
    Reply { key: RowKey, text: String },
    Stop { key: RowKey },
}

/// One registry call of the peek, carried to the loop that owns the fleet service.
pub enum RegistryCall {
    Subscribed(RowKey, talaria_core::fleet::SubscribedView),
    Unsubscribed(RowKey),
    Acknowledge(RowKey),
    Refresh,
}

/// Forwards the peek's registry calls to the loop.
struct LoopSink(mpsc::UnboundedSender<AppMessage>);

impl crate::peek::FleetSink for LoopSink {
    fn observe_subscribed(&self, key: RowKey, view: talaria_core::fleet::SubscribedView) {
        let _ = self
            .0
            .send(AppMessage::Registry(RegistryCall::Subscribed(key, view)));
    }

    fn observe_unsubscribed(&self, key: RowKey) {
        let _ = self
            .0
            .send(AppMessage::Registry(RegistryCall::Unsubscribed(key)));
    }

    fn acknowledge(&self, key: RowKey) {
        let _ = self
            .0
            .send(AppMessage::Registry(RegistryCall::Acknowledge(key)));
    }
}

impl crate::bg::LaunchFleet for LoopSink {
    fn observe_subscribed(&self, key: RowKey, view: talaria_core::fleet::SubscribedView) {
        crate::peek::FleetSink::observe_subscribed(self, key, view);
    }

    fn observe_unsubscribed(&self, key: RowKey) {
        crate::peek::FleetSink::observe_unsubscribed(self, key);
    }

    fn refresh_now(&self) {
        let _ = self.0.send(AppMessage::Registry(RegistryCall::Refresh));
    }
}

/// What a fleet launch needs from the app's state.
fn launch_context(state: &AppState) -> crate::bg::dialog::LaunchContext {
    let fleet = match (&state.sender, state.fleet_enabled) {
        (Some(sender), true) => {
            Some(Arc::new(LoopSink(sender.clone())) as Arc<dyn crate::bg::LaunchFleet>)
        }
        _ => None,
    };
    crate::bg::dialog::LaunchContext {
        home: state.hermes_home.clone(),
        foreground_profile: state.profile.clone(),
        fleet,
        sender: state.sender.clone(),
        home_dir: std::env::var_os("HOME").map(PathBuf::from),
    }
}

/// The dialog's redraw and notices as effects.
fn launch_effects(state: &mut AppState, out: crate::bg::dialog::LaunchUiOut) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for (kind, text) in out.notices {
        if state.opened.is_some() {
            effects.extend(reduce(state, Event::Notice { kind, text }));
        } else {
            effects.push(notice_lines(&text));
        }
    }
    if out.redraw {
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// The foreground session's row.
fn fleet_foreground_key(state: &AppState) -> Option<RowKey> {
    state.opened.as_ref().map(|opened| RowKey {
        profile: state.profile.clone(),
        session_id: opened.session_id.clone(),
    })
}

/// The panel status of the foreground row.
fn foreground_status(state: &AppState) -> talaria_core::fleet_view::PeekStatus {
    match state.opened.as_ref() {
        Some(opened) => {
            crate::fleet_actions::foreground_status(&opened.state, !state.masked.is_empty())
        }
        None => talaria_core::fleet_view::PeekStatus::Failed {
            text: "Could not open this session: no session is open.".to_owned(),
        },
    }
}

/// A registry snapshot as an overlay input.
fn fleet_snapshot_input(
    snapshot: &crate::fleet::FleetSnapshot,
) -> talaria_core::fleet_view::FleetViewInput {
    use crate::fleet::FleetLinkState;
    let scope_note = match &snapshot.scope {
        crate::side_connection::SideScope::ProfileOnly { profile, why } => {
            Some(format!("Showing profile {profile} only: {why}."))
        }
        crate::side_connection::SideScope::Host { .. } => None,
    };
    let link_note = match &snapshot.link {
        FleetLinkState::Connecting => Some("connecting\u{2026}".to_owned()),
        FleetLinkState::Connected => None,
        FleetLinkState::Reconnecting { reason } => Some(format!("reconnecting: {reason}")),
        FleetLinkState::Failed { reason } => Some(format!("fleet connection failed: {reason}")),
    };
    talaria_core::fleet_view::FleetViewInput::Snapshot {
        version: snapshot.version,
        rows: snapshot.rows.clone(),
        queue: snapshot.queue.clone(),
        profiles: snapshot.profiles.clone(),
        scope_note,
        link_note,
    }
}

/// `/fleet`: open the overlay on the latest snapshot, before any call.
pub(crate) fn open_fleet_view(state: &mut AppState) -> Vec<AppEffect> {
    if !state.fleet_enabled {
        return vec![notice_lines(FLEET_DISABLED)];
    }
    if let Some(pr) = review_pr(state) {
        pr.want(crate::review::PrDemand::Fleet(true));
        for root in &state.review_repos {
            pr.know(root);
        }
    }
    if state.fleet.is_none() {
        let now = state
            .fleet_snapshot
            .as_ref()
            .map_or(0.0, |snapshot| snapshot.as_of);
        state.fleet = Some(FleetViewState::new(fleet_foreground_key(state), now));
    }
    let mut effects = match state.fleet_snapshot.clone() {
        Some(snapshot) => fleet_input(state, fleet_snapshot_input(&snapshot)),
        None => Vec::new(),
    };
    if let Some(view) = state.fleet.as_mut() {
        view.take_frame();
    }
    refresh_fleet_warning_count(state);
    effects.push(AppEffect::Redraw);
    effects
}

/// Open the fleet view on the row the jump key named.
///
/// The overlay and the peek belong to the fleet view unit. The cursor goes on the
/// row and the peek opens through the same path the overlay's own Space key uses, so
/// the peek's work runs in the worker and its answer comes back as a message.
fn fleet_open_at(state: &mut AppState, key: RowKey) -> Vec<AppEffect> {
    let mut effects = open_fleet_view(state);
    let produced = state
        .fleet
        .as_mut()
        .map(|view| view.open_at(key.clone()))
        .unwrap_or_default();
    for effect in produced {
        for out in fleet_effect(state, effect) {
            match out {
                AppEffect::FleetInput(input) => effects.extend(fleet_input(state, input)),
                AppEffect::FleetForeground(job) => effects.extend(foreground_job(state, job)),
                other => effects.push(other),
            }
        }
    }
    if let Some(view) = state.fleet.as_mut()
        && view.needs_redraw()
    {
        view.take_frame();
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// Apply one overlay input and turn its effects into executor effects.
pub(crate) fn fleet_input(
    state: &mut AppState,
    input: talaria_core::fleet_view::FleetViewInput,
) -> Vec<AppEffect> {
    let Some(view) = state.fleet.as_mut() else {
        return Vec::new();
    };
    let produced = talaria_core::fleet_view::update(view, input, FLEET_PAGE);
    let mut effects = Vec::new();
    for effect in produced {
        for out in fleet_effect(state, effect) {
            match out {
                AppEffect::FleetInput(input) => effects.extend(fleet_input(state, input)),
                AppEffect::FleetForeground(job) => effects.extend(foreground_job(state, job)),
                other => effects.push(other),
            }
        }
    }
    if let Some(view) = state.fleet.as_mut()
        && view.needs_redraw()
    {
        view.take_frame();
        effects.push(AppEffect::Redraw);
    }
    effects
}

/// One overlay effect: the overlay's own work here, peek work for the worker.
pub(crate) fn fleet_effect(
    state: &mut AppState,
    effect: talaria_core::fleet_view::FleetViewEffect,
) -> Vec<AppEffect> {
    use talaria_core::fleet_view::{FleetViewEffect, FleetViewInput, ReplyOutcome};
    if let FleetViewEffect::SendReply { key, text } = &effect
        && let Some(target) = state.client_voice_drafts.get(key).cloned()
    {
        let refuse = |reason: &str| {
            vec![AppEffect::FleetInput(FleetViewInput::ReplyDone {
                key: key.clone(),
                outcome: ReplyOutcome::Refused {
                    reason: reason.into(),
                },
            })]
        };
        let row = state
            .fleet_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.rows.iter().find(|row| row.key == *key));
        if row.is_none_or(|row| row.backend() != target.backend) {
            return refuse("the recorded target is no longer available");
        }
        if target.backend == talaria_core::fleet::FleetBackend::Hermes
            && let Some(need) = &target.need
        {
            if state
                .fleet
                .as_ref()
                .and_then(|view| view.peek())
                .is_none_or(|(peeked, _)| peeked != key)
            {
                return refuse("the recorded answer card is no longer open");
            }
            let Some(item) = state
                .fleet_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.queue.iter().find(|item| &item.id == need))
            else {
                return refuse("the recorded request is no longer available");
            };
            let talaria_core::fleet::NeedSlot::Prompt(prompt_id) = &need.slot else {
                return refuse("use the answer card for this request");
            };
            let Some(execution_generation) = item.execution_generation else {
                return refuse("the recorded request identity is unavailable");
            };
            let prompt = talaria_core::session::PromptKey {
                session_id: SessionId::new(&key.session_id),
                prompt_id: prompt_id.clone(),
                execution_generation,
            };
            let answer = match &item.detail {
                talaria_core::fleet::NeedDetail::Known(
                    talaria_core::attention::NeedSummary::Question(_),
                ) => crate::peek::CardAnswer::Clarify {
                    key: prompt,
                    answer: text.clone(),
                },
                talaria_core::fleet::NeedDetail::Known(
                    talaria_core::attention::NeedSummary::Approval { .. },
                ) => {
                    let normalize = |s: &str| {
                        s.split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .to_lowercase()
                    };
                    let spoken = normalize(text);
                    let mut options = item
                        .choices
                        .iter()
                        .filter(|choice| normalize(choice) == spoken);
                    let (Some(choice), None) = (options.next(), options.next()) else {
                        return refuse("speak one exact approval option");
                    };
                    crate::peek::CardAnswer::Approve {
                        key: prompt,
                        choice: choice.clone(),
                        note: None,
                    }
                }
                _ => return refuse("use the answer card for this request"),
            };
            state.client_voice_inflight.insert(key.clone());
            return vec![AppEffect::FleetVoiceAnswer {
                key: key.clone(),
                answer,
            }];
        }
        if target.backend == talaria_core::fleet::FleetBackend::Acp {
            if let Some(need) = &target.need {
                let talaria_core::fleet::NeedSlot::External(responder) = &need.slot else {
                    return refuse("the recorded permission is no longer available");
                };
                let request = state
                    .acp_views
                    .get(key)
                    .and_then(|view| view.needs.iter().find(|item| &item.responder == responder));
                let Some(request) = request else {
                    return refuse("the recorded permission is no longer available");
                };
                let normalize = |s: &str| {
                    s.split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                };
                let spoken = normalize(text);
                let mut options = request
                    .options
                    .iter()
                    .filter(|option| normalize(&option.name) == spoken);
                let (Some(option), None) = (options.next(), options.next()) else {
                    return refuse("speak one exact permission option");
                };
                if let Some(manager) = &state.acp_manager {
                    let sent = manager.send(crate::fleet::ExternalCommand::Answer {
                        key: key.clone(),
                        responder: request.responder.clone(),
                        tool_call_id: request.tool_call_id.clone(),
                        option_id: option.id.clone(),
                    });
                    if sent {
                        state.client_voice_inflight.insert(key.clone());
                        return Vec::new();
                    }
                    return refuse("the agent is unavailable");
                }
                return refuse("the agent is unavailable");
            }
            if row.is_some_and(|row| {
                matches!(
                    row.state,
                    talaria_core::fleet::RowState::Working
                        | talaria_core::fleet::RowState::NeedsYou
                )
            }) {
                return refuse("the agent is already working");
            }
            state.client_voice_inflight.insert(key.clone());
        }
    }
    let foreground = fleet_foreground_key(state);
    let is_foreground = |key: &RowKey| foreground.as_ref() == Some(key);
    let peeking_foreground = state
        .fleet
        .as_ref()
        .and_then(|view| view.peek())
        .is_some_and(|(key, _)| is_foreground(key));
    match effect {
        FleetViewEffect::OpenInPane(key) if is_external_row(state, &key) => {
            vec![AppEffect::FleetInput(FleetViewInput::Notice(
                "External agents cannot be opened in a Hermes pane.".into(),
            ))]
        }
        FleetViewEffect::OpenPeek(key) if is_external_row(state, &key) => {
            drop_peek_worker(state);
            state.peek_frame = None;
            let status = state.acp_views.get(&key).map_or_else(
                || talaria_core::fleet_view::PeekStatus::Failed {
                    text: "Agent session is unavailable.".into(),
                },
                |view| talaria_core::fleet_view::PeekStatus::Open {
                    has_open_prompt: !view.needs.is_empty(),
                    running: matches!(
                        view.state,
                        talaria_core::fleet::RowState::Working
                            | talaria_core::fleet::RowState::NeedsYou
                    ),
                },
            );
            vec![AppEffect::FleetInput(FleetViewInput::Peek { key, status })]
        }
        FleetViewEffect::SendReply { key, text } if is_external_row(state, &key) => {
            if let Some(manager) = &state.acp_manager {
                manager.send(crate::fleet::ExternalCommand::Prompt { key, text });
                Vec::new()
            } else {
                vec![AppEffect::FleetInput(FleetViewInput::ReplyDone {
                    key,
                    outcome: ReplyOutcome::NotAttached {
                        reason: "the agent is unavailable".into(),
                    },
                })]
            }
        }
        FleetViewEffect::Stop(key) if is_external_row(state, &key) => {
            if let Some(manager) = &state.acp_manager {
                manager.send(crate::fleet::ExternalCommand::Cancel { key });
                Vec::new()
            } else {
                vec![AppEffect::FleetInput(FleetViewInput::StopDone {
                    key,
                    outcome: talaria_core::fleet_view::StopOutcome::NotAttached {
                        reason: "the agent is unavailable".into(),
                    },
                })]
            }
        }
        FleetViewEffect::ResendReply(key) if is_external_row(state, &key) => {
            vec![AppEffect::FleetInput(FleetViewInput::ReplyDone {
                key,
                outcome: ReplyOutcome::Refused {
                    reason: "check the agent session before sending it again".into(),
                },
            })]
        }
        FleetViewEffect::Open(key) if is_external_row(state, &key) => {
            let produced = state
                .fleet
                .as_mut()
                .map(|view| view.open_at(key))
                .unwrap_or_default();
            produced
                .into_iter()
                .flat_map(|effect| fleet_effect(state, effect))
                .collect()
        }
        FleetViewEffect::OpenInPane(key) => crate::fleet_actions::start_pane(state, key)
            .into_iter()
            .map(AppEffect::FleetInput)
            .collect(),
        FleetViewEffect::OpenPeek(key) if is_foreground(&key) => {
            let status = foreground_status(state);
            vec![AppEffect::FleetInput(FleetViewInput::Peek { key, status })]
        }
        FleetViewEffect::ClosePeek if peeking_foreground || state.peek_worker.is_none() => {
            vec![AppEffect::FleetInput(FleetViewInput::PeekClosed)]
        }
        FleetViewEffect::SendReply { key, text } if is_foreground(&key) => {
            vec![AppEffect::FleetForeground(ForegroundJob::Reply {
                key,
                text,
            })]
        }
        FleetViewEffect::Stop(key) if is_foreground(&key) => {
            vec![AppEffect::FleetForeground(ForegroundJob::Stop { key })]
        }
        FleetViewEffect::ResendReply(key) if is_foreground(&key) => {
            vec![AppEffect::FleetInput(FleetViewInput::ReplyDone {
                key,
                outcome: ReplyOutcome::Refused {
                    reason: "check the session itself before sending it again".to_owned(),
                },
            })]
        }
        FleetViewEffect::Acknowledge(key) => {
            if is_external_row(state, &key)
                && let Some(manager) = &state.acp_manager
            {
                manager.send(crate::fleet::ExternalCommand::Acknowledge { key: key.clone() });
            }
            if let Some(fleet) = &state.fleet_service {
                fleet.acknowledge(key);
            }
            Vec::new()
        }
        FleetViewEffect::Open(key) => {
            let mut effects = close_overlay(state);
            effects.push(AppEffect::FleetPeek(FleetViewEffect::Close));
            effects.push(AppEffect::FleetInput(FleetViewInput::PeekClosed));
            if is_foreground(&key) {
                effects.push(AppEffect::CommitLines(vec![ALREADY_HERE_NOTICE.to_owned()]));
            } else if let Some(refusal) = session_busy(state) {
                effects.extend(refusal);
            } else {
                state.switching = true;
                state.session_action = true;
                state.pending_switch = Some(key.clone());
                effects.push(AppEffect::Switch { key });
            }
            effects
        }
        FleetViewEffect::Close => {
            let mut effects = close_overlay(state);
            effects.push(AppEffect::FleetPeek(FleetViewEffect::Close));
            effects.push(AppEffect::FleetInput(FleetViewInput::PeekClosed));
            effects
        }
        FleetViewEffect::FocusAnswer => Vec::new(),
        FleetViewEffect::Review(row) => vec![AppEffect::ReviewLocate {
            row,
            origin: ReviewOrigin::Fleet,
        }],
        other => vec![AppEffect::FleetPeek(other)],
    }
}

fn is_external_row(state: &AppState, key: &RowKey) -> bool {
    state.acp_views.contains_key(key)
        || state.fleet_snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.rows.iter().any(|row| {
                row.key == *key && row.backend() == talaria_core::fleet::FleetBackend::Acp
            })
        })
}

/// Close the overlay; each reply still unsettled leaves one scrollback notice.
fn close_overlay(state: &mut AppState) -> Vec<AppEffect> {
    let Some(view) = state.fleet.take() else {
        return Vec::new();
    };
    state.client_voice_drafts.clear();
    if let Some(pr) = state.review_pr.as_ref() {
        pr.want(crate::review::PrDemand::Fleet(false));
    }
    state.peek_frame = None;
    let mut effects: Vec<AppEffect> = crate::fleet_actions::unsettled_notices(&view)
        .iter()
        .map(|text| notice_lines(text))
        .collect();
    effects.push(AppEffect::Redraw);
    effects
}

/// The foreground row's reply or stop, through the loop's own executor.
fn foreground_job(state: &mut AppState, job: ForegroundJob) -> Vec<AppEffect> {
    use talaria_core::fleet_view::{FleetViewInput, ReplyOutcome, StopOutcome};
    match job {
        ForegroundJob::Reply { key, text } => {
            let effects = submit_prose(state, text);
            let sent = effects.iter().find_map(|effect| match effect {
                AppEffect::Send(Effect::SendPrompt { submission_id, .. }) => {
                    Some(submission_id.clone())
                }
                _ => None,
            });
            let mut effects = effects;
            match sent {
                Some(id) => state.fleet_fg_reply = Some((id, key)),
                None => effects.extend(fleet_input(
                    state,
                    FleetViewInput::ReplyDone {
                        key,
                        outcome: ReplyOutcome::NotAttached {
                            reason: "the session is not ready".to_owned(),
                        },
                    },
                )),
            }
            effects
        }
        ForegroundJob::Stop { key } => {
            if state.interrupting {
                return fleet_input(
                    state,
                    FleetViewInput::StopDone {
                        key,
                        outcome: StopOutcome::Stopping,
                    },
                );
            }
            let mut effects = reduce(state, Event::Interrupt);
            if effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Interrupt(_)))
            {
                state.fleet_fg_stop = Some(key);
            } else {
                effects.extend(fleet_input(
                    state,
                    FleetViewInput::StopDone {
                        key,
                        outcome: StopOutcome::NothingRunning,
                    },
                ));
            }
            effects
        }
    }
}

/// The foreground row's reply or stop, waited for. Callers outside the loop use this.
pub(crate) async fn run_foreground(
    state: &mut AppState,
    job: ForegroundJob,
) -> Vec<talaria_core::fleet_view::FleetViewInput> {
    use talaria_core::fleet_view::{FleetViewInput, ReplyOutcome};
    match job {
        ForegroundJob::Reply { key, text } => {
            let submission_id = state.next_submission_id();
            let service = state.service.clone();
            let Some(opened) = state.opened.as_mut() else {
                return vec![FleetViewInput::ReplyDone {
                    key,
                    outcome: ReplyOutcome::NotAttached {
                        reason: "no session is open".to_owned(),
                    },
                }];
            };
            let effect = talaria_core::session::update(
                &mut opened.state,
                Event::Submit {
                    display: text.clone(),
                    text,
                    submission_id,
                    attachments: Vec::new(),
                },
            )
            .into_iter()
            .find(|effect| matches!(effect, Effect::SendPrompt { .. }));
            let (Some(effect), Some(service)) = (effect, service) else {
                return vec![FleetViewInput::ReplyDone {
                    key,
                    outcome: ReplyOutcome::NotAttached {
                        reason: "the session is not ready".to_owned(),
                    },
                }];
            };
            let (outcome, receipt) = service.execute_with_receipt(effect).await;
            let status = receipt.and_then(|receipt| {
                serde_json::to_value(receipt.status)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
            });
            vec![FleetViewInput::ReplyDone {
                key,
                outcome: crate::fleet_actions::reply_outcome(&outcome, status.as_deref()),
            }]
        }
        ForegroundJob::Stop { key } => {
            let outcome = interrupt(state).await;
            vec![FleetViewInput::StopDone {
                key,
                outcome: crate::fleet_actions::stop_outcome(&outcome),
            }]
        }
    }
}

/// The peek worker, started the first time the overlay needs it.
pub(crate) fn peek_worker(state: &mut AppState) -> &crate::fleet_actions::PeekWorker {
    if state.peek_worker.is_none() {
        let launch = state.launch.clone().unwrap_or_else(|| Launch {
            mode: crate::launch::StartMode::New,
            model: None,
            provider: None,
            effort: None,
            profile: state.profile.clone(),
            folder: state.launch_folder.clone(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        });
        let launch_id = state.launch_id.clone();
        let counter = Arc::clone(&state.counter);
        let (fleet, reports): (
            Arc<dyn crate::peek::FleetSink>,
            Option<mpsc::UnboundedSender<crate::fleet_actions::PeekReport>>,
        ) = match &state.sender {
            Some(sender) => {
                let (reports, mut received) = mpsc::unbounded_channel();
                let forward = sender.clone();
                tokio::spawn(async move {
                    while let Some(report) = received.recv().await {
                        if forward.send(AppMessage::Peek(report)).is_err() {
                            break;
                        }
                    }
                });
                (Arc::new(LoopSink(sender.clone())), Some(reports))
            }
            None => (Arc::new(crate::peek::NoFleet), None),
        };
        state.peek_worker = Some(crate::fleet_actions::PeekWorker::start(
            crate::fleet_actions::PeekSetup {
                launch,
                home: state.hermes_home.clone(),
                fleet,
                next_submission_id: Box::new(move || {
                    submission_id(&launch_id, counter.fetch_add(1, Ordering::SeqCst) + 1)
                }),
                timeouts: state.peek_timeouts,
                reports,
            },
        ));
    }
    match &state.peek_worker {
        Some(worker) => worker,
        None => unreachable!("the worker was just started"),
    }
}

/// Detach the sub-agent tracker opened for the fleet peek, keeping the foreground row's tracker.
fn detach_peek_subagent(state: &mut AppState) {
    let Some(old) = state.peek_subagent_key.take() else {
        return;
    };
    if foreground_row(state).as_ref() != Some(&old)
        && let Some(hub) = state.subagents.as_mut()
    {
        hub.detach(&old);
    }
}

/// Close the peek worker: its queued work runs, then it closes the peek and ends.
pub(crate) fn drop_peek_worker(state: &mut AppState) {
    state.peek_worker = None;
    detach_peek_subagent(state);
}

/// What the peek worker reported between results.
fn peek_report(state: &mut AppState, report: crate::fleet_actions::PeekReport) -> Vec<AppEffect> {
    use crate::fleet_actions::PeekReport;
    use talaria_core::fleet_view::{FleetViewInput, Focus};
    match report {
        PeekReport::Attached { key, client } => {
            if state.fleet.is_none() {
                return Vec::new();
            }
            if state.peek_subagent_key.as_ref() != Some(&key) {
                detach_peek_subagent(state);
                if let Some(hub) = state.subagents.as_mut() {
                    hub.attach(key.clone(), client);
                    state.peek_subagent_key = Some(key);
                }
            }
            Vec::new()
        }
        PeekReport::Frame(frame) => {
            if state.fleet.is_none() {
                return Vec::new();
            }
            if state
                .fleet
                .as_ref()
                .and_then(|fleet| fleet.peek())
                .is_some_and(|(key, _)| is_external_row(state, key) && *key != frame.key)
            {
                return Vec::new();
            }
            let card = frame
                .state
                .open_prompts()
                .any(|prompt| prompt.kind != PromptKind::ServerRequest)
                || !frame.masked.is_empty();
            state.peek_frame = Some(*frame);
            let mut effects = vec![AppEffect::Redraw];
            if !card
                && state
                    .fleet
                    .as_ref()
                    .is_some_and(|view| *view.focus() == Focus::Answer)
            {
                effects.extend(fleet_input(state, FleetViewInput::AnswerFocusEnded));
            }
            effects
        }
        PeekReport::Gone => {
            state.peek_frame = None;
            detach_peek_subagent(state);
            vec![AppEffect::Redraw]
        }
        PeekReport::Input(input) => fleet_input(state, input),
        PeekReport::Notices(notices) => {
            let mut effects = Vec::new();
            for notice in notices {
                effects.extend(fleet_input(state, FleetViewInput::Notice(notice.text)));
            }
            effects
        }
    }
}

/// The earliest of any number of deadlines.
fn earliest_of(deadlines: impl IntoIterator<Item = Option<Instant>>) -> Option<Instant> {
    deadlines.into_iter().flatten().min()
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
#[inline(never)]
fn draw_current_view<S: Screen>(state: &AppState, screen: &mut S) {
    screen.draw(&state.view());
}

#[inline(never)]
fn route_screen_input<S: Screen>(
    state: &AppState,
    screen: &mut S,
    input: S::Input,
    now: Instant,
) -> Vec<UserAction> {
    screen.input(input, &state.view(), now)
}

pub async fn run<S: Screen>(
    launch: Launch,
    setup: RunSetup,
    screen: &mut S,
    inputs: mpsc::UnboundedReceiver<S::Input>,
) -> Outcome {
    Box::pin(run_inner(launch, setup, screen, inputs, false, None)).await
}

pub async fn run_with_hivecode<S: Screen>(
    launch: Launch,
    setup: RunSetup,
    screen: &mut S,
    inputs: mpsc::UnboundedReceiver<S::Input>,
    default_on: bool,
) -> Outcome {
    Box::pin(run_inner(launch, setup, screen, inputs, default_on, None)).await
}

/// Run with the authenticated remote audio context from the connection handoff.
pub async fn run_with_remote_audio<S: Screen>(
    launch: Launch,
    setup: RunSetup,
    screen: &mut S,
    inputs: mpsc::UnboundedReceiver<S::Input>,
    default_on: bool,
    remote_audio: Option<hermes_client::audio_api::AudioRelay>,
) -> Outcome {
    Box::pin(run_inner(
        launch,
        setup,
        screen,
        inputs,
        default_on,
        remote_audio,
    ))
    .await
}

async fn run_inner<S: Screen>(
    launch: Launch,
    mut setup: RunSetup,
    screen: &mut S,
    mut inputs: mpsc::UnboundedReceiver<S::Input>,
    default_hivecode: bool,
    remote_audio: Option<hermes_client::audio_api::AudioRelay>,
) -> Outcome {
    let mut state = AppState::new_boxed(setup.launch_id.clone());
    state.seat_reporting = screen.seat_reporting();
    if setup.remote_gateway {
        state.attached_to_remote_gateway();
    }
    state.remote_host = setup
        .remote_host
        .as_deref()
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_owned);
    state.hivecode = crate::hivecode::HivecodeService::new(default_hivecode);
    if setup.remote_gateway {
        state.hivecode.set_remote(true);
    }
    // The per-session switch saved by earlier runs; an unreadable file starts empty and is
    // replaced by the next change.
    if let Some(home) = talaria_workflow::staffing::talaria_home() {
        let path = home.join(crate::hivecode_files::MODE_FILE);
        if let Ok(mut saved) = crate::hivecode_files::ModeFile::read(&path) {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as i64);
            saved.prune(now_ms);
            state.hivecode.restore(saved);
        }
        state.hivecode_file = Some(crate::hivecode_files::ModeFile::new(path));
    }
    let voice_config = screen.voice_config();
    state.configure_voice(voice_config);
    state.models = ModelsState::new(
        &format!("{}-model", setup.launch_id),
        crate::config::load_models(),
    );
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
    if let Some(files) = screen.startup_theme_files() {
        let startup = state.appearance.polarity();
        let selected = files.file().is_some() || files.polarity(startup) != startup;
        state.appearance.set_theme_files(files);
        if selected {
            screen.appearance_changed(&state.appearance.view());
        }
    }
    state.attachments = AttachmentsState::new(screen.attachment_rule(), 16 * 1024 * 1024);
    state.attachment_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    state.attachments.set_home(state.attachment_home.clone());
    state.attachment_folder = launch.folder.clone();
    state
        .attachments
        .set_workspace(state.attachment_folder.clone());
    state.key_bindings = setup.keys.clone();
    state.toggle_label = screen.toggle_label();
    state.interrupt_label = screen.interrupt_label();
    state.workflow_key_label = screen.workflow_key_label();
    state.home = std::env::var_os("HOME").map(PathBuf::from);
    state.profile = launch.profile.trim().to_lowercase();
    state.hub = AttentionHub::new(
        screen.fleet_attention_config(),
        state.profile.clone(),
        screen.attention_key_label(),
    );
    state.list_title = setup.list_title;
    state.model = launch.model.clone();
    state.effort = launch.effort.clone();
    state.effort_wire = None;
    state.hermes_home = setup.home.clone();
    state.launch_folder = launch.folder.clone();
    state.launch = Some(launch.clone());
    state.pane_settings = screen.pane_settings();
    state.configure_model = screen.configure_model();
    state.pane_size = screen.pane_size();
    state.layout = talaria_core::layout::LayoutState::new(
        talaria_core::layout::LayoutPreferences {
            work_panel: screen.work_panel(),
            menu_bar: screen.menu_bar(),
        },
        state.pane_size.0,
        state.pane_size.1,
    );
    state.layout.reasoning = screen.reasoning_display();
    state.layout.mouse = screen.mouse_navigation();
    sync_models(&mut state, screen);
    sync_picker(&state, screen);
    draw_current_view(&state, screen);
    screen.report_pane(&state.attention.pane(), None);
    if !setup.notices.is_empty() {
        screen.commit_lines(
            setup
                .notices
                .iter()
                .map(|notice| format!("\u{b7} {notice}"))
                .collect(),
        );
        draw_current_view(&state, screen);
    }

    let (sender, mut messages) = mpsc::unbounded_channel();
    let executor = Executor::new(sender);
    let effort_source = if setup.remote_gateway {
        match remote_audio.as_ref() {
            Some(hermes_client::audio_api::AudioRelay::Remote { config, store }) => {
                Some(crate::subagents::EffortSource::Remote {
                    config: config.clone(),
                    store: store.clone(),
                })
            }
            _ => None,
        }
    } else {
        Some(crate::subagents::EffortSource::Local(setup.home.clone()))
    };
    state.client_voice_worker = Some(Arc::new(crate::voice::relay::ClientAudio::new(
        setup.home.clone(),
        remote_audio,
        setup.remote_gateway,
        state.client_voice_config.remote_audio,
        executor.sender.clone(),
    )));
    state.sender = Some(executor.sender.clone());
    state.subagents = Some(
        crate::subagents::SubagentHub::new(screen.subagents_config(), executor.sender.clone())
            .with_effort_source(effort_source),
    );
    // The first frame is drawn; the fleet link starts in the background.
    state.fleet_enabled = setup.fleet.is_some();
    if let Some(cfg) = setup.fleet.take() {
        let fleet_sender = executor.sender.clone();
        let fleet = crate::fleet::FleetService::start(
            setup.home.clone(),
            state.profile.clone(),
            cfg,
            move |notice| {
                let _ = fleet_sender.send(AppMessage::Fleet(notice));
            },
        );
        let acp_config = screen.acp_config();
        state.acp_agents = acp_config
            .agents
            .iter()
            .map(|agent| (agent.id.clone(), agent.display_name.clone()))
            .collect();
        state.acp_manager = Some(crate::acp::manager::start(
            acp_config,
            fleet.reporter(),
            executor.sender.clone(),
        ));
        state.fleet_service = Some(fleet);
    }
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
        let background_due = background_wake(&state, &executor);
        let notice_due = notice_wake(&state, &executor);
        let workflow_due = workflow_wake(&state, &executor);
        let queue_due = queue_wake(&state, &executor);
        let deadline = earliest_of([
            screen.next_tick(),
            screen.next_wake(now),
            side_due,
            background_due,
        ]);
        // A compacting line counts its seconds, so the screen wakes once a second
        // while one is shown.
        let deadline = if state
            .opened
            .as_ref()
            .is_some_and(|opened| state.context.compacting_line(&opened.session_id).is_some())
        {
            let wait = 1_000 - state.now_ms % 1_000;
            earliest_of([deadline, Some(now + Duration::from_millis(wait))])
        } else {
            deadline
        };
        let voice_due = state
            .voice
            .session_ids()
            .iter()
            .any(|id| {
                state.voice.get(id).is_some_and(|voice| {
                    matches!(
                        voice.phase(),
                        talaria_core::voice::VoicePhase::Recording
                            | talaria_core::voice::VoicePhase::Transcribing
                    )
                })
            })
            .then(|| {
                executor.started
                    + Duration::from_millis(state.voice_last_tick_ms.saturating_add(1_000))
            });
        let deadline = earliest_of([deadline, voice_due]);
        let deadline = earliest_of([deadline, state.attachments.next_deadline()]);
        let deadline = earliest_of([deadline, state.automation_deadline]);
        let interrupt_due = interrupt_hint_wake(&state, &executor);
        let deadline = earliest_of([deadline, interrupt_due]);
        let deadline = earliest_of([deadline, state.configure_tick_at]);
        let provider_deadline = talaria_core::providers::next_deadline(&state.providers);
        let deadline = earliest_of([deadline, provider_deadline]);
        let deadline = earliest_of([deadline, talaria_core::mcp::next_deadline(&state.mcp)]);
        let non_cron_deadline = deadline;
        let cron_deadline = state.cron.deadline;
        let deadline = earliest_of([deadline, cron_deadline]);
        let other_deadline = deadline;
        let deadline = earliest_of([deadline, workflow_due]);
        let deadline = earliest_of([deadline, queue_due]);
        let deadline = earliest_of([deadline, notice_due]);
        let mut redraw = false;
        let mut woke_on_message = false;
        tokio::select! {
            input = inputs.recv() => {
                let Some(input) = input else { break Ending::InputClosed };
                sync_models(&mut state, screen);
                sync_picker(&state, screen);
                state.now_ms = executor.now_ms();
                let actions = route_screen_input(&state, screen, input, Instant::now());
                state.pane_size = screen.pane_size();
                let previous_layout = state.layout;
                state.layout.resize(state.pane_size.0, state.pane_size.1);
                redraw |= previous_layout != state.layout;
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
                state.keys_waiting = !inputs.is_empty();
                if voice_due.is_some_and(|due| due <= now) || interrupt_due.is_some_and(|due| due <= now) || side_due.is_some_and(|due| due <= now) || background_due.is_some_and(|due| due <= now) || notice_due.is_some_and(|due| due <= now) || workflow_due.is_some_and(|due| due <= now) || queue_due.is_some_and(|due| due <= now) || state.configure_tick_at.is_some_and(|due| due <= now) {
                    let now_ms = executor.now_ms();
                    redraw |= apply(&mut state, AppMessage::Tick { now_ms }, screen, &executor);
                }
                state.keys_waiting = false;
                if let Some((text, caret)) = screen.draft_snapshot()
                    && (text != state.draft || caret != state.caret) {
                        let _ = apply(&mut state, AppMessage::User(UserAction::DraftEdited {
                            text, caret, typed: true, recalled: false, now,
                        }), screen, &executor);
                }
                let _ = apply(&mut state, AppMessage::AttachmentTick(now), screen, &executor);
                if provider_deadline.is_some_and(|due| due <= now) {
                    let generation = state.provider_generation;
                    let _ = apply(&mut state, AppMessage::Providers { generation, input: ProvidersInput::Tick }, screen, &executor);
                }
                if talaria_core::mcp::next_deadline(&state.mcp).is_some_and(|due| due <= now) {
                    let binding = state.mcp_binding;
                    let _ = apply(&mut state, AppMessage::Mcp {
                        binding,
                        input: talaria_core::mcp::McpInput::Tick,
                    }, screen, &executor);
                }
                // A countdown that changed its text arms the next one.
                state.automation_wake(now);
                let cron_due = cron_deadline.is_some_and(|due| due <= now);
                redraw |= other_deadline.is_some_and(|due| due <= now)
                    && (!cron_due || non_cron_deadline.is_some_and(|due| due <= now));
                if cron_due {
                    state.cron.deadline = None;
                    redraw |= apply(&mut state, AppMessage::User(UserAction::Cron(Box::new(CronInput::Tick))), screen, &executor);
                }
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
                sync_picker(&state, screen);
                state.now_ms = executor.now_ms();
                let actions = route_screen_input(&state, screen, input, Instant::now());
                state.pane_size = screen.pane_size();
                let previous_layout = state.layout;
                state.layout.resize(state.pane_size.0, state.pane_size.1);
                redraw |= previous_layout != state.layout;
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
        sync_fleet(&mut state);
        if redraw {
            sync_models(&mut state, screen);
            sync_picker(&state, screen);
            draw_current_view(&state, screen);
            state.cron.first_frame_pending = false;
            screen.on_tick(Instant::now());
            // The frame is on screen; the work that waited for it may go.
            executor.run_deferred();
        }
    };

    for (_, task) in std::mem::take(&mut state.profiles_ticks) {
        task.abort();
    }
    state.close_client_voice_jobs().await;
    choice::restore_on_exit(&mut state).await;
    let provider_closing = state.provider_service.as_ref().map(|service| {
        let closing = service.close();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(1), closing).await;
        })
    });
    screen.forget_provider_secret();
    let mut outcome = Outcome::default();
    if state
        .client_voice_worker
        .as_ref()
        .is_some_and(|worker| worker.capture_blocked())
    {
        outcome
            .stderr
            .push_str("The capture helper could not be confirmed stopped before exit.\n");
    }
    if state
        .client_voice_worker
        .as_ref()
        .is_some_and(|worker| worker.playback_blocked())
    {
        outcome
            .stderr
            .push_str("The playback helper could not be confirmed stopped before exit.\n");
    }
    screen.forget_mcp_secrets();
    if let Some(service) = state.mcp_service.take() {
        let _ = tokio::time::timeout(Duration::from_secs(1), service.close()).await;
    }
    match ending {
        Ending::Quit | Ending::InputClosed => {
            let (detached, code) = if pending.is_some() {
                state.pending = pending.take();
                let (detached, code, _) = quit_while_opening(&mut state).await;
                (detached, code)
            } else {
                let mut voice_error = Vec::new();
                let (detached, code, hint) = quit_with_voice(&mut state, &mut voice_error).await;
                outcome
                    .stderr
                    .push_str(&String::from_utf8_lossy(&voice_error));
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
            if let (Some(opened), Some(client)) = (
                state.opened.as_ref(),
                state.service.as_ref().and_then(SessionService::client),
            ) {
                let mut warning = Vec::new();
                state
                    .voice
                    .stop_before_detach(client, &opened.session_id, &mut warning)
                    .await;
                outcome.stderr.push_str(&String::from_utf8_lossy(&warning));
            }
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
    if let Some(closing) = provider_closing {
        let _ = closing.await;
    }
    let unfinished = state.launch_ui.shutdown().await;
    if let Some(manager) = state.acp_manager.take() {
        manager.close().await;
    }
    if let Some(fleet) = state.fleet_service.take() {
        fleet.close().await;
    }
    if executor.flush_model_files().await {
        outcome
            .stderr
            .push_str("Some model choices could not be saved before exit.\n");
    }
    flush_theme_file_commands(&mut state, &mut messages, &executor, &mut outcome.stderr).await;
    drive_status(screen, hermes_client::is_suspicious_key).await;
    screen.restore();
    for warning in state.voice.take_warnings() {
        outcome.stderr.push_str(&warning);
    }
    outcome.stderr.push_str(&held_text(&state.held));
    for line in unfinished {
        outcome.stderr.push_str(line);
        outcome.stderr.push('\n');
    }
    outcome
}

/// Where a review was opened from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOrigin {
    /// `v` on a fleet row: the review replaces the fleet list, and Esc returns to it.
    Fleet,
    /// `/changes`: the review opens over the screen, and Esc closes it.
    Changes,
}

/// What `v` shows for a row with no folder, and what `/changes` shows instead.
const FLEET_NO_FOLDER: &str = "Talaria cannot find this session's folder.";
const CHANGES_NO_FOLDER: &str = "Hermes did not report a folder for this session.";
const CHANGES_NO_SESSION: &str = "No session in the foreground, so there is no folder to review.";

/// The pull-request status service, made on first use.
fn review_pr(state: &mut AppState) -> Option<Arc<crate::review::PrStatusService>> {
    if state.review_pr.is_none() {
        let sender = state.sender.clone()?;
        let notify = move |notice| {
            let _ = sender.send(AppMessage::PrStatus(notice));
        };
        state.review_pr = Some(Arc::new(crate::review::PrStatusService::new(
            crate::review::GhRunner::from_env(),
            crate::worktree::GitRunner::from_env(),
            Arc::new(Instant::now),
            notify,
        )));
    }
    state.review_pr.clone()
}

/// The review clock, in seconds.
fn review_now(state: &AppState) -> f64 {
    state.now_ms as f64 / 1000.0
}

/// The review's effects as the app's: `Close` and `Redraw` belong here.
fn review_effects(
    state: &mut AppState,
    effects: Vec<talaria_core::review::ReviewEffect>,
) -> Vec<AppEffect> {
    use talaria_core::review::ReviewEffect;
    let mut out = Vec::new();
    for effect in effects {
        match effect {
            ReviewEffect::Close => out.extend(close_review(state)),
            ReviewEffect::Redraw => out.push(AppEffect::Redraw),
            other => out.push(AppEffect::Review(other)),
        }
    }
    out
}

fn review_input(state: &mut AppState, input: talaria_core::review::ReviewInput) -> Vec<AppEffect> {
    let Some(service) = state.review_service.as_mut() else {
        return Vec::new();
    };
    let effects = service.update(input);
    review_effects(state, effects)
}

/// Close the review. The fleet list it replaced shows again with its cursor unchanged.
fn close_review(state: &mut AppState) -> Vec<AppEffect> {
    let Some(service) = state.review_service.as_mut() else {
        return Vec::new();
    };
    let root = service
        .state()
        .map(|review| review.target().main_root.clone());
    service.close();
    state.review_origin = None;
    if let (Some(pr), Some(root)) = (state.review_pr.as_ref(), root) {
        pr.want(crate::review::PrDemand::Review(root, false));
    }
    vec![AppEffect::Redraw]
}

/// A review notice: on the fleet view's notice line while it is open, else the main one.
fn review_notice(state: &mut AppState, text: String) -> Vec<AppEffect> {
    if state.fleet.is_some() {
        let mut effects = fleet_input(
            state,
            talaria_core::fleet_view::FleetViewInput::Notice(text),
        );
        effects.push(AppEffect::Redraw);
        return effects;
    }
    notice_effect(state, text)
}

/// `/changes`: review the foreground session's folder.
fn changes_command(state: &mut AppState) -> Vec<AppEffect> {
    match fleet_foreground_key(state) {
        Some(row) => vec![AppEffect::ReviewLocate {
            row,
            origin: ReviewOrigin::Changes,
        }],
        None => notice_effect(state, CHANGES_NO_SESSION.to_owned()),
    }
}

/// Once a second: due pull-request refreshes, and the review's own clock.
fn review_tick(state: &mut AppState) -> Vec<AppEffect> {
    if let Some(pr) = state.review_pr.as_ref() {
        pr.tick();
    }
    if state
        .review_service
        .as_ref()
        .is_some_and(crate::review::ReviewService::is_open)
    {
        let now = review_now(state);
        return review_input(state, talaria_core::review::ReviewInput::Tick { now });
    }
    Vec::new()
}

/// Labels changed: the review's own label, the fleet column and the status line.
fn review_pr_changed(state: &mut AppState) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    if let (Some(pr), Some((root, branch))) = (state.review_pr.clone(), review_branch(state)) {
        let status = pr.for_branch(&root, &branch).map(|(status, _)| status);
        effects.extend(review_input(
            state,
            talaria_core::review::ReviewInput::PrStatus(status),
        ));
    }
    effects.push(AppEffect::Redraw);
    effects
}

/// The open review's repository and branch, when it is on one.
fn review_branch(state: &AppState) -> Option<(PathBuf, String)> {
    let review = state.review_service.as_ref()?.state()?;
    let target = review.target();
    let branch = review
        .listing()
        .and_then(|listing| listing.head.head.clone())
        .or_else(|| target.record.as_ref().map(|record| record.branch.clone()))?;
    Some((target.main_root.clone(), branch))
}

/// Whether the reviewed session has a turn running: its fleet row, or the foreground's turn.
fn review_working(state: &AppState, row: Option<&RowKey>) -> bool {
    let Some(row) = row else {
        return false;
    };
    let from_fleet = state.fleet_snapshot.as_ref().and_then(|snapshot| {
        snapshot
            .rows
            .iter()
            .find(|item| &item.key == row)
            .map(|item| {
                matches!(
                    item.state,
                    talaria_core::fleet::RowState::Working
                        | talaria_core::fleet::RowState::NeedsYou
                )
            })
    });
    from_fleet.unwrap_or_else(|| {
        fleet_foreground_key(state).as_ref() == Some(row) && state.view().turn_running()
    })
}

/// A folder was found, or not: open the review, or say why.
fn review_located(
    state: &mut AppState,
    located: crate::review::Located,
    origin: ReviewOrigin,
    info: Option<Result<crate::review::SessionInfoReply, crate::review::SessionInfoError>>,
) -> Vec<AppEffect> {
    use talaria_core::review::ReviewInput;
    let target = match located {
        crate::review::Located::Found(target) => target,
        crate::review::Located::NotFound(text) => return review_notice(state, text),
    };
    let Some(pr) = review_pr(state) else {
        return Vec::new();
    };
    let root = target.main_root.clone();
    // A checkout Git discovered is not enrolled. review_repos holds only folders
    // the operator selected.
    pr.know(&root);
    pr.want(crate::review::PrDemand::Review(root.clone(), true));
    if let Some(record) = &target.record {
        pr.note_branch(&root, &record.branch, target.session.clone());
    }
    let working = review_working(state, target.session.as_ref());
    let has_session = target.session.is_some();
    let now = review_now(state);
    if state.review_service.is_none() {
        state.review_service = Some(
            crate::review::ReviewService::new(
                crate::worktree::GitRunner::from_env(),
                crate::review::GhRunner::from_env(),
                Arc::clone(&pr),
            )
            .with_link(state.hermes_home.clone(), state.profile.clone()),
        );
    }
    let Some(service) = state.review_service.as_mut() else {
        return Vec::new();
    };
    let mut effects = service.open(target, now);
    let (model, can_draft) = match info {
        Some(Ok(reply)) => {
            let model = reply.model.filter(|model| !model.is_empty());
            let can_draft = (!reply.lazy || reply.cwd.is_some()) && model.is_some();
            (model, can_draft)
        }
        _ => (None, false),
    };
    if has_session {
        effects.extend(service.update(ReviewInput::SessionInfo { model, can_draft }));
    }
    effects.extend(service.update(ReviewInput::SessionWorking(working)));
    state.review_origin = Some(origin);
    let mut out = review_effects(state, effects);
    out.extend(review_pr_changed(state));
    out
}

/// Everything a folder lookup needs, taken from the app state.
struct ReviewLocateJob {
    row: RowKey,
    origin: ReviewOrigin,
    roots: Vec<PathBuf>,
    folders: Vec<PathBuf>,
    foreground: Option<(RowKey, Option<String>)>,
    home: PathBuf,
    profile: String,
}

fn review_locate_job(state: &AppState, row: RowKey, origin: ReviewOrigin) -> ReviewLocateJob {
    let foreground_cwd = state
        .opened
        .as_ref()
        .and_then(|opened| opened.info.cwd.clone());
    // Roots come from folders the operator already holds. The gateway cwd is a
    // candidate to check against those roots, never a root of its own.
    let folders = vec![state.launch_folder.clone()];
    ReviewLocateJob {
        row,
        origin,
        roots: state.review_repos.clone(),
        folders,
        foreground: fleet_foreground_key(state).map(|key| (key, foreground_cwd)),
        home: state.hermes_home.clone(),
        profile: state.profile.clone(),
    }
}

impl ReviewLocateJob {
    /// Find the folder: the worktree record, then the foreground's own folder, then
    /// `session.info`, which also decides whether the session can draft.
    async fn run(self) -> AppMessage {
        use crate::review::{
            KnownRepos, LinkInfoSource, LocateContext, Located, SessionInfoSource,
        };
        let git = crate::worktree::GitRunner::from_env();
        let mut known = KnownRepos::new();
        for root in self.roots {
            known.insert_explicit(root);
        }
        for folder in &self.folders {
            if folder.as_os_str().is_empty() {
                continue;
            }
            known.insert_explicit(folder.clone());
        }
        let source = LinkInfoSource::new(self.home, self.profile);
        let ctx = LocateContext {
            git: &git,
            known_roots: &known,
            foreground: self
                .foreground
                .as_ref()
                .map(|(key, cwd)| (key, cwd.as_deref())),
            info: &source,
        };
        let mut located = crate::review::locate(&self.row, &ctx).await;
        if self.origin == ReviewOrigin::Changes
            && matches!(&located, Located::NotFound(text) if text == FLEET_NO_FOLDER)
        {
            located = Located::NotFound(CHANGES_NO_FOLDER.to_owned());
        }
        let info = match &located {
            Located::Found(target) => match &target.session {
                Some(row) => match source.last() {
                    Some(reply) => Some(reply),
                    None => Some(source.info(row).await),
                },
                None => None,
            },
            Located::NotFound(_) => None,
        };
        AppMessage::ReviewLocated {
            located,
            origin: self.origin,
            info,
        }
    }
}

#[cfg(test)]
mod review_cwd_trust_tests {
    use super::*;

    fn opened_with_cwd(cwd: &str) -> AppState {
        let mut state = AppState::new("review-trust");
        state.profile = "fixture".into();
        state.launch_folder = PathBuf::from("/operator/launch");
        state
            .review_repos
            .push(PathBuf::from("/operator/known-root"));
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
                cwd: Some(cwd.to_owned()),
                branch: None,
            },
            cursor: Cursor {
                replay_epoch: "epoch".into(),
                last_sequence: 0,
            },
        });
        state
    }

    #[test]
    fn review_locate_job_never_seeds_gateway_cwd() {
        let state = opened_with_cwd("/hostile/repo");
        let job = review_locate_job(
            &state,
            RowKey {
                profile: "fixture".into(),
                session_id: "session".into(),
            },
            ReviewOrigin::Changes,
        );
        assert!(
            job.folders
                .iter()
                .all(|folder| folder != Path::new("/hostile/repo")),
            "gateway cwd must not be seeded as a trusted folder: {:?}",
            job.folders
        );
        assert!(
            job.roots
                .iter()
                .all(|root| root != Path::new("/hostile/repo")),
            "gateway cwd must not be seeded as a known root: {:?}",
            job.roots
        );
        assert!(
            job.folders
                .iter()
                .any(|folder| folder == Path::new("/operator/launch")),
            "the launch folder stays trusted: {:?}",
            job.folders
        );
        assert!(
            job.roots
                .iter()
                .any(|root| root == Path::new("/operator/known-root")),
            "review repos stay trusted: {:?}",
            job.roots
        );
        let (_, cwd) = job.foreground.expect("the foreground candidate");
        assert_eq!(cwd.as_deref(), Some("/hostile/repo"));
    }

    #[test]
    fn review_located_does_not_enroll_a_discovered_main_root() {
        let mut state = opened_with_cwd("/operator/launch/linked");
        let (sender, _received) = mpsc::unbounded_channel();
        state.sender = Some(sender);
        let before = state.review_repos.clone();
        let target = talaria_core::review::ReviewTarget {
            folder: PathBuf::from("/operator/launch/linked"),
            main_root: PathBuf::from("/outside/main"),
            session: None,
            record: None,
            title: "linked".to_owned(),
        };
        let _ = review_located(
            &mut state,
            crate::review::Located::Found(target),
            ReviewOrigin::Changes,
            None,
        );
        assert_eq!(
            state.review_repos, before,
            "a discovered main checkout must not join the trust set"
        );
        assert!(
            state
                .review_repos
                .iter()
                .all(|root| root != Path::new("/outside/main"))
        );
    }
}

#[cfg(test)]
mod doctor_loop_tests {
    use super::*;
    use talaria_core::doctor::{
        AttachFacts, CheckId, GatewayFacts, IdentityFacts, Pin, PostResult, ProbeResult,
        StatusFacts,
    };

    fn facts() -> DoctorFacts {
        DoctorFacts {
            talaria_version: "1.0.0".into(),
            pin: Pin {
                tag: hermes_contract::pinned::TAG.into(),
                commit: hermes_contract::pinned::COMMIT.into(),
            },
            profile: "fixture".into(),
            gateway: GatewayFacts::Ready {
                instance_id: "instance-full-identity".into(),
                epoch: 1,
                launch: None,
            },
            identity: Some(IdentityFacts {
                code_version: None,
                code_sha: None,
                capabilities: Vec::new(),
                served: vec!["default".into(), "fixture".into()],
                parked: Vec::new(),
            }),
            attach: AttachFacts::NotTried,
            status: StatusFacts::NotTried,
            config: talaria_core::config_check::ConfigFacts::NotTried,
        }
    }

    #[test]
    fn rerun_marks_the_view_busy_before_another_key() {
        let mut state = AppState::new("doctor-test");
        open_doctor(&mut state);
        update(&mut state, AppMessage::DoctorFacts(facts()));
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Doctor(DoctorKey::Rerun)),
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorGather))
        );
        assert!(state.doctor.as_ref().unwrap().running);
        let repeated = update(
            &mut state,
            AppMessage::User(UserAction::Doctor(DoctorKey::Rerun)),
        );
        assert!(
            !repeated
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorGather))
        );
        let restart = update(
            &mut state,
            AppMessage::User(UserAction::Doctor(DoctorKey::Restart)),
        );
        assert!(
            !restart
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorActiveWork))
        );
    }

    #[test]
    fn opening_gathers_and_facts_preserve_the_row_state_and_full_restart_identity() {
        let mut state = AppState::new("doctor-test");
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Submit("/doctor".into())),
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorGather))
        );
        let view = state.doctor.as_mut().unwrap();
        assert_eq!(view.selected, 0);
        assert!(view.running);
        assert!(view.report.is_none());
        view.selected = 10;
        view.expanded.insert(CheckId::HermesVersion);

        update(&mut state, AppMessage::DoctorFacts(facts()));
        let view = state.doctor.as_ref().unwrap();
        assert_eq!(view.selected, 6);
        assert!(view.expanded.contains(&CheckId::HermesVersion));
        assert!(!view.running);
        assert_eq!(view.report.as_ref().unwrap().checks.len(), 7);

        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Doctor(DoctorKey::Restart)),
        );
        assert!(matches!(
            &state.doctor_restart,
            RestartState::Asking { old_instance, profiles }
                if old_instance == "instance-full-identity" && profiles == &["default", "fixture"]
        ));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorActiveWork))
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorPost { .. }))
        );
    }

    #[test]
    fn a_closed_restart_finishes_as_a_notice_and_reopening_clears_the_outcome() {
        let snapshot =
            serde_json::from_str(include_str!("../tests/data/open_approval.json")).unwrap();
        let mut state = AppState::new("doctor-test");
        state.opened = Some(OpenedSession::from_snapshot(snapshot));
        state.doctor = Some(DoctorView::new());
        state.doctor_restart = RestartState::Requesting {
            old_instance: "old-instance".into(),
        };
        update(
            &mut state,
            AppMessage::User(UserAction::Doctor(DoctorKey::Close)),
        );
        assert!(state.doctor.is_none());
        assert!(matches!(
            state.doctor_restart,
            RestartState::Requesting { .. }
        ));
        update(
            &mut state,
            AppMessage::DoctorRestart(RestartInput::Posted(PostResult::Changed)),
        );
        assert!(state.opened.as_ref().unwrap().state.cells().iter().any(|cell| {
            matches!(cell, Cell::Notice(notice)
                if notice.kind == NoticeKind::Error && notice.text.starts_with("Doctor: The gateway changed"))
        }));
        open_doctor(&mut state);
        assert!(matches!(state.doctor_restart, RestartState::Idle));
    }

    #[test]
    fn waiting_survives_reopening_and_redraws_only_when_the_progress_text_changes() {
        let mut state = AppState::new("doctor-test");
        state.doctor_restart = RestartState::Waiting {
            old_instance: "old-instance".into(),
            elapsed: Duration::from_millis(1100),
            unknown_request: false,
        };
        open_doctor(&mut state);
        assert!(matches!(state.doctor_restart, RestartState::Waiting { .. }));
        let same_second = update(
            &mut state,
            AppMessage::DoctorRestart(RestartInput::Tick(Duration::from_millis(1900))),
        );
        assert!(
            !same_second
                .iter()
                .any(|effect| matches!(effect, AppEffect::Redraw))
        );
        let next_second = update(
            &mut state,
            AppMessage::DoctorRestart(RestartInput::Tick(Duration::from_secs(2))),
        );
        assert!(
            next_second
                .iter()
                .any(|effect| matches!(effect, AppEffect::Redraw))
        );
        let same_instance = update(
            &mut state,
            AppMessage::DoctorRestart(RestartInput::Probe(ProbeResult::Ready {
                instance_id: "old-instance".into(),
            })),
        );
        assert!(
            same_instance
                .iter()
                .any(|effect| matches!(effect, AppEffect::DoctorProbeAfter(_)))
        );
        assert!(
            !same_instance
                .iter()
                .any(|effect| matches!(effect, AppEffect::Redraw))
        );
    }
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
    fn subagent_usage_fallback_reads_the_nested_value_and_resets_on_switch() {
        let mut state = state();
        let usage = inbound(
            &state,
            "session.usage",
            json!({
                "active_subagents": 99, "usage": {"active_subagents": 7}
            }),
        );
        update(&mut state, usage);
        assert_eq!(state.view().subagents_usage_active, 7);
        let snapshot =
            serde_json::from_str(include_str!("../tests/data/open_approval.json")).unwrap();
        update(
            &mut state,
            AppMessage::Opened {
                opened: OpenedSession::from_snapshot(snapshot),
                notice: None,
            },
        );
        assert_eq!(state.view().subagents_usage_active, 0);
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

    #[test]
    fn retained_request_cancel_removes_the_masked_card_and_cancels_the_responder() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let mut state = state();
            let foreground = state.opened.as_ref().unwrap().session_id.clone();
            let background = "retained-session-1".to_owned();
            let snapshot: hermes_contract::schemas::CanonicalSnapshot =
                serde_json::from_value(json!({
                    "session_id": background,
                    "stored_session_id": "stored-retained",
                    "messages": [],
                    "message_count": 0,
                    "running": false,
                    "authority_epoch": 1,
                    "replay_epoch": "epoch-1",
                    "last_sequence": 0,
                    "subscription_id": "sub-retained",
                    "revision": 1,
                    "execution_generation": 1,
                    "pending": [],
                    "prompts": [],
                    "info": {"desktop_protocol": "1"},
                }))
                .expect("snapshot");
            state.retained_sessions.insert(
                RowKey {
                    profile: state.profile.clone(),
                    session_id: background.clone(),
                },
                OpenedSession::from_snapshot(snapshot),
            );
            let prompts = Arc::new(Mutex::new(PromptService::new(Box::new(|| {
                "talaria-retained-1".to_owned()
            }))));
            state.prompts = Some(Arc::clone(&prompts));
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let executor = Executor::new(sender);
            let effects = link_event(
                &mut state,
                LinkEvent::Inbound {
                    item: Inbound::ServerRequest(sudo_request("srq-retained", &background)),
                    replayed: false,
                },
            );
            let Ok([AppEffect::ServerRequest(request)]) = <[_; 1]>::try_from(effects) else {
                panic!("a retained session's request must reach the prompt service");
            };
            enqueue_server_request(&state, &executor, request);
            let AppMessage::PromptDone {
                masked: Some(cards),
                ..
            } = prompt_done(&mut receiver).await
            else {
                panic!("the prompt service must publish masked cards");
            };
            let [card] = cards.as_slice() else {
                panic!("the retained session's request must open one masked card");
            };
            assert_eq!(card.session_id.as_deref(), Some(background.as_str()));
            let target = card.target();
            let raw = json!({
                "session_id": background,
                "replay_epoch": "epoch-1",
                "seq": 1,
                "type": "request.cancel",
                "payload": {"id": "srq-retained", "method": "sudo", "reason": "timeout"},
            });
            let effects = update(
                &mut state,
                AppMessage::Link(LinkEvent::Inbound {
                    item: Inbound::Event(hermes_client::channel::InboundEvent {
                        generation: hermes_client::Generation(1),
                        frame: EventFrame::from_params(&raw).unwrap(),
                        raw,
                    }),
                    replayed: false,
                }),
            );
            let (id, method, reason) = effects
                .iter()
                .find_map(|effect| match effect {
                    AppEffect::CancelRequest { id, method, reason } => {
                        Some((id.clone(), method.clone(), reason.clone()))
                    }
                    _ => None,
                })
                .expect("request.cancel for a retained session must reach the prompt service");
            assert_ne!(background, foreground);
            let mut prompts = prompts.lock().await;
            let notices = prompts.on_request_cancel(&id, &method, &reason);
            assert!(
                prompts.masked_requests().is_empty(),
                "the retained session's masked card is still open"
            );
            assert_eq!(
                notices.len(),
                1,
                "cancelling the responder must record the withdrawal"
            );
            let error = prompts
                .answer_masked(&target, Zeroizing::new("secret".into()))
                .await
                .expect_err("a cancelled responder must refuse a late answer");
            assert_eq!(error, hermes_client::RespondError::Cancelled);
        });
    }

    fn cancel_message(session: &str, epoch: &str, seq: u64, id: &str) -> AppMessage {
        let raw = json!({
            "session_id": session,
            "replay_epoch": epoch,
            "seq": seq,
            "type": "request.cancel",
            "payload": {"id": id, "method": "sudo", "reason": "timeout"},
        });
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(hermes_client::channel::InboundEvent {
                generation: hermes_client::Generation(1),
                frame: EventFrame::from_params(&raw).expect("cancel frame"),
                raw,
            }),
            replayed: false,
        })
    }

    fn sudo_request(id: &str, session: &str) -> ServerRequest {
        ServerRequest {
            generation: hermes_client::Generation(1),
            id: id.into(),
            session_id: Some(session.into()),
            request: hermes_contract::server_requests::ServerRequest::Sudo(
                hermes_contract::schemas::SudoRequestParams {
                    command: "sudo true".into(),
                    session_id: session.into(),
                },
            ),
            responder: hermes_client::Responder::detached(Arc::new(AtomicU64::new(0))),
        }
    }

    fn configure_context() -> talaria_core::configure::Context {
        use talaria_core::configure::Section;
        talaria_core::configure::Context {
            profile: "default".into(),
            hermes_version: None,
            schema: talaria_core::settings::ConfigSchemaView {
                fields: vec![],
                category_order: vec![],
            },
            effective: json!({}),
            raw: json!({}),
            defaults: json!({}),
            inventory: talaria_core::configure::Inventory {
                mcp_servers: Section::Unavailable("not gathered".into()),
                mcp_catalogue: Section::Unavailable("not gathered".into()),
                skills: Section::Unavailable("not gathered".into()),
                toolsets: Section::Unavailable("not gathered".into()),
                plugins: Section::Unavailable("not gathered".into()),
                cron_jobs: Section::Unavailable("not gathered".into()),
                providers: Section::Unavailable("not gathered".into()),
                main_model: Section::Unavailable("not gathered".into()),
                routes: Section::Unavailable("not gathered".into()),
                profiles: Section::Unavailable("not gathered".into()),
                connectors: Section::Unavailable("not gathered".into()),
            },
        }
    }

    async fn prompt_done(rx: &mut mpsc::UnboundedReceiver<AppMessage>) -> AppMessage {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("the prompt task should finish")
            .expect("the prompt channel stays open")
    }

    #[test]
    fn request_cancel_reaches_the_executor_on_cursor_configure_and_retained_paths() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let mut state = state();
            let foreground = state.opened.as_ref().unwrap().session_id.clone();
            let epoch = state.opened.as_ref().unwrap().cursor.replay_epoch.clone();
            let stale = state.opened.as_ref().unwrap().cursor.last_sequence;
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let executor = Executor::new(sender);
            let prompts = Arc::new(Mutex::new(PromptService::new(Box::new(|| {
                "talaria-cancel-1".into()
            }))));
            state.prompts = Some(Arc::clone(&prompts));

            enqueue_server_request(&state, &executor, sudo_request("srq-cursor", &foreground));
            prompt_done(&mut receiver).await;
            assert_eq!(prompts.lock().await.masked_requests().len(), 1);
            let effects = update(
                &mut state,
                cancel_message(&foreground, &epoch, stale, "srq-cursor"),
            );
            let cancel = effects.iter().find_map(|effect| match effect {
                AppEffect::CancelRequest { id, method, reason } if id == "srq-cursor" => {
                    Some((id.clone(), method.clone(), reason.clone()))
                }
                _ => None,
            });
            let (id, method, reason) =
                cancel.expect("a cursor-rejected cancel still reaches the prompt service");
            enqueue_cancel_request(&state, &executor, id, method, reason);
            prompt_done(&mut receiver).await;
            assert!(prompts.lock().await.masked_requests().is_empty());

            let background = "retained-session-1".to_owned();
            let snapshot: hermes_contract::schemas::CanonicalSnapshot =
                serde_json::from_value(json!({
                    "session_id": background,
                    "stored_session_id": "stored-retained",
                    "messages": [],
                    "message_count": 0,
                    "running": false,
                    "authority_epoch": 1,
                    "replay_epoch": "epoch-1",
                    "last_sequence": 4,
                    "subscription_id": "sub-retained",
                    "revision": 1,
                    "execution_generation": 1,
                    "pending": [],
                    "prompts": [],
                    "info": {"desktop_protocol": "1"},
                }))
                .expect("snapshot");
            state.retained_sessions.insert(
                RowKey {
                    profile: state.profile.clone(),
                    session_id: background.clone(),
                },
                OpenedSession::from_snapshot(snapshot),
            );
            enqueue_server_request(&state, &executor, sudo_request("srq-retained", &background));
            prompt_done(&mut receiver).await;
            let effects = update(
                &mut state,
                cancel_message(&background, "epoch-1", 4, "srq-retained"),
            );
            let (id, method, reason) = effects
                .iter()
                .find_map(|effect| match effect {
                    AppEffect::CancelRequest { id, method, reason } if id == "srq-retained" => {
                        Some((id.clone(), method.clone(), reason.clone()))
                    }
                    _ => None,
                })
                .expect("a cursor-rejected retained cancel still reaches the prompt service");
            enqueue_cancel_request(&state, &executor, id, method, reason);
            prompt_done(&mut receiver).await;
            assert!(prompts.lock().await.masked_requests().is_empty());

            state.link_state = LinkView::Connected;
            configure_command(
                &mut state,
                "turn on the bell".into(),
                "turn on the bell".into(),
            );
            configure_input(
                &mut state,
                talaria_core::configure::ConfigureInput::Gathered(Box::new(configure_context())),
                None,
            );
            let created: hermes_contract::schemas::CanonicalSnapshot =
                serde_json::from_value(json!({
                    "session_id": "configure-session",
                    "stored_session_id": "stored-configure",
                    "messages": [],
                    "message_count": 0,
                    "running": false,
                    "authority_epoch": 1,
                    "replay_epoch": "epoch-1",
                    "last_sequence": 0,
                    "subscription_id": "sub-configure",
                    "revision": 1,
                    "execution_generation": 1,
                    "pending": [],
                    "prompts": [],
                    "info": {"desktop_protocol": "1"},
                }))
                .expect("configure snapshot");
            configure_input(
                &mut state,
                talaria_core::configure::ConfigureInput::Created(Box::new(created)),
                None,
            );
            let hidden = state
                .configure
                .as_ref()
                .and_then(|configure| configure.hidden_session_id())
                .expect("the configure session id")
                .to_owned();
            prompts
                .lock()
                .await
                .on_server_request_for_session(
                    sudo_request("srq-configure", &hidden),
                    Some(&hidden),
                )
                .await;
            assert_eq!(prompts.lock().await.masked_requests().len(), 1);
            let effects = update(
                &mut state,
                cancel_message(&hidden, "epoch-1", 1, "srq-configure"),
            );
            assert!(
                effects.iter().any(|effect| {
                    matches!(effect, AppEffect::CancelRequest { id, .. } if id == "srq-configure")
                }),
                "configure routing must not swallow request.cancel"
            );
            enqueue_cancel_request(
                &state,
                &executor,
                "srq-configure".into(),
                "sudo".into(),
                "timeout".into(),
            );
            prompt_done(&mut receiver).await;
            assert!(prompts.lock().await.masked_requests().is_empty());

            enqueue_cancel_request(
                &state,
                &executor,
                "srq-race".into(),
                "sudo".into(),
                "timeout".into(),
            );
            prompt_done(&mut receiver).await;
            enqueue_server_request(&state, &executor, sudo_request("srq-race", &foreground));
            prompt_done(&mut receiver).await;
            assert!(
                prompts.lock().await.masked_requests().is_empty(),
                "a register that loses the race to cancel must not restore the card"
            );
        });
    }

    #[test]
    fn approval_cancelled_omitted_request_ids_withdraws_only_the_named_session() {
        let mut state = state();
        let other: hermes_contract::schemas::CanonicalSnapshot = serde_json::from_value(json!({
            "session_id": "other-session",
            "stored_session_id": "stored-other",
            "messages": [],
            "message_count": 0,
            "running": false,
            "authority_epoch": 1,
            "replay_epoch": "epoch-1",
            "last_sequence": 1,
            "subscription_id": "sub-other",
            "revision": 1,
            "execution_generation": 1,
            "pending": [],
            "prompts": [{
                "kind": "approval",
                "prompt_id": "prompt-other",
                "execution_generation": 1,
                "choices": ["once", "deny"],
                "command": "ls"
            }],
            "info": {"desktop_protocol": "1"},
        }))
        .expect("snapshot");
        state.retained_sessions.insert(
            RowKey {
                profile: state.profile.clone(),
                session_id: "other-session".into(),
            },
            OpenedSession::from_snapshot(other),
        );
        let params = json!({
            "session_id": "",
            "replay_epoch": "epoch-1",
            "seq": 5,
            "type": "approval.cancelled",
            "payload": {
                "session_id": "local-session-1",
                "stored_session_id": "stored-1",
                "reason": "reap",
                "cancelled_count": 1
            }
        });
        let bytes = json!({"jsonrpc":"2.0","method":"event","params": params}).to_string();
        let decoded = hermes_client::decode_frame(
            hermes_client::Generation(1),
            bytes.as_bytes(),
            &mut |_| panic!("not a server request"),
        );
        let hermes_client::Decoded::Inbound(hermes_client::Inbound::Event(event)) = decoded else {
            panic!("an omitted request_ids list was rejected");
        };
        update(
            &mut state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(event),
                replayed: false,
            }),
        );
        assert!(
            !state
                .opened
                .as_ref()
                .unwrap()
                .state
                .open_prompts()
                .any(|prompt| prompt.key.prompt_id == "prompt-1")
        );
        let other = state
            .retained_sessions
            .get(&RowKey {
                profile: state.profile.clone(),
                session_id: "other-session".into(),
            })
            .unwrap();
        assert!(
            other
                .state
                .open_prompts()
                .any(|prompt| prompt.key.prompt_id == "prompt-other"),
            "the unnamed session was withdrawn"
        );
    }
}

#[cfg(test)]
mod providers_command_tests {
    use super::*;
    use talaria_core::providers::{OpenTarget, ProvidersTab};

    fn model_setup_catalogue() -> hermes_contract::schemas::ModelOptionsResult {
        serde_json::from_value(serde_json::json!({"provider":"example-current","model":"example-model","providers":[
            {"slug":"example-current","name":"Example current","authenticated":true,"models":["example-model"]},
            {"slug":"example-setup","name":"Example setup","authenticated":false,"models":[]}
        ]})).unwrap()
    }

    #[test]
    fn model_stub_returns_only_when_the_provider_dialog_closes() {
        use talaria_core::models::{ModelsEffect, ModelsInput, PickerKey};
        use talaria_core::providers::{EndpointList, EnvRow, ProvidersInput, ProvidersKey};
        let recording = include_str!("../../talaria-core/tests/data/resume_snapshot.jsonl");
        let mut snapshot = recording
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|line| line.get("frame").and_then(|f| f.get("result")).cloned())
            .unwrap();
        snapshot["prompts"] = serde_json::json!([]);
        snapshot["running"] = serde_json::json!(false);
        let opened = OpenedSession::from_snapshot(serde_json::from_value(snapshot).unwrap());
        let sid = opened.session_id.clone();
        let mut state = AppState::new("provider-return");
        state.profile = "fixture".into();
        state.link_state = LinkView::Connected;
        update(
            &mut state,
            AppMessage::Opened {
                opened,
                notice: None,
            },
        );
        models_input(
            &mut state,
            ModelsInput::OpenPicker {
                session_id: Some(sid.clone()),
                profile: "fixture".into(),
            },
        );
        models_input(
            &mut state,
            ModelsInput::OptionsLoaded {
                session_id: sid.clone(),
                result: model_setup_catalogue(),
            },
        );
        models_input(
            &mut state,
            ModelsInput::EffortRead {
                session_id: sid.clone(),
                value: Some("high".into()),
            },
        );
        for ch in "Example setup".chars() {
            models_input(&mut state, ModelsInput::Picker(PickerKey::Char(ch)));
        }
        models_input(&mut state, ModelsInput::Picker(PickerKey::Enter));
        assert!(state.providers.is_open());
        assert!(state.models.view().is_none());
        providers_input(
            &mut state,
            ProvidersInput::EnvLoaded(Ok(vec![EnvRow {
                env_var: "EXAMPLE_API_KEY".into(),
                slug: "example-setup".into(),
                label: "Example setup".into(),
                primary: true,
                is_password: true,
                ..Default::default()
            }])),
        );
        providers_input(
            &mut state,
            ProvidersInput::OptionsLoaded(Ok(model_setup_catalogue())),
        );
        providers_input(&mut state, ProvidersInput::OauthLoaded(Ok(vec![])));
        providers_input(
            &mut state,
            ProvidersInput::EndpointsLoaded(Ok(EndpointList::default())),
        );
        if talaria_core::providers::view(&state.providers, Instant::now())
            .card
            .is_none()
        {
            providers_input(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
        }
        assert!(
            talaria_core::providers::view(&state.providers, Instant::now())
                .card
                .is_some()
        );
        providers_input(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
        assert!(state.providers.is_open());
        assert!(
            state.models.view().is_none(),
            "closing a setup card must not return early"
        );
        let effects = providers_input(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
        assert!(!state.providers.is_open());
        assert_eq!(
            state
                .models
                .view()
                .expect("dialog close resumes model picker")
                .filter,
            "Example setup"
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            AppEffect::Models(ModelsEffect::LoadOptions { refresh: true, .. })
        )));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, AppEffect::Models(ModelsEffect::Mutate(_))))
        );
    }

    #[test]
    fn bare_providers_close_does_not_open_a_model_picker() {
        let mut state = AppState::new("provider-command");
        update(
            &mut state,
            AppMessage::User(UserAction::Submit("/providers".into())),
        );
        assert!(state.providers.is_open());
        providers_input(
            &mut state,
            talaria_core::providers::ProvidersInput::Key(
                talaria_core::providers::ProvidersKey::Esc,
            ),
        );
        assert!(!state.providers.is_open());
        assert!(state.models.view().is_none());
    }

    #[test]
    fn provider_tabs_and_invalid_arguments_reach_the_application() {
        for (argument, tab) in [
            ("", ProvidersTab::All),
            (" KeYs ", ProvidersTab::Keys),
            (" AcCoUnTs ", ProvidersTab::Accounts),
            (" EnDpOiNtS ", ProvidersTab::Endpoints),
        ] {
            let typed = format!("/providers {argument}");
            let Slash::Local(action, argument) = slash_line(&typed) else {
                panic!("expected a local provider command");
            };
            assert_eq!(action, LocalAction::Providers(OpenTarget::Tab(tab)));
            let mut state = AppState::new("provider-command");
            state.profile = "fixture".into();
            local_effect(&mut state, action, argument, typed);
            let view = talaria_core::providers::view(&state.providers, Instant::now());
            assert!(state.providers.is_open());
            assert_eq!(view.tab, tab);
            assert_eq!(view.title, "Providers · profile fixture");
        }
        assert_eq!(
            slash_line("/providers other"),
            Slash::Notice("Usage: /providers [keys|accounts|endpoints]".into())
        );
    }
}

#[cfg(test)]
mod checklist_command_tests {
    use super::*;
    use talaria_core::session::TodoState;

    #[test]
    fn tasks_alias_opens_agents_and_the_checklist_key_still_toggles() {
        let command = LocalCommandTable::builtin()
            .find("tasks")
            .cloned()
            .expect("local tasks");
        assert_eq!(command.action, LocalAction::Agents);
        assert_eq!(
            slash_line("/tasks"),
            Slash::Local(LocalAction::Agents, String::new())
        );
        assert_eq!(
            slash_line("/tasks pause"),
            Slash::Local(LocalAction::Agents, "pause".into())
        );
        let mut state = AppState::new("0123456789abcdef");
        state.checklist.attach(
            Some(&TodoState {
                revision: 1,
                todos: vec![json!({"id":"1", "content":"A step", "status":"pending"})],
            }),
            true,
        );
        let expanded = state.checklist.expanded();
        let effects = update(&mut state, AppMessage::User(UserAction::ToggleTasks));
        assert_eq!(state.checklist.expanded(), !expanded);
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], AppEffect::Redraw));
    }
}

#[cfg(test)]
mod profiles_action_tests {
    use super::*;
    use talaria_core::model_roles::{RolesInput, RolesKey};
    use talaria_core::profiles::{GatewayView, HostTopology, KeyAction};

    /// Connected to `fixture` with a session open, and /profiles loaded with `default` selected.
    fn loaded() -> AppState {
        let recording = include_str!("../../talaria-core/tests/data/resume_snapshot.jsonl");
        let mut snapshot = recording
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|line| line.get("frame").and_then(|f| f.get("result")).cloned())
            .unwrap();
        snapshot["prompts"] = serde_json::json!([]);
        snapshot["running"] = serde_json::json!(false);
        snapshot["info"]["cwd"] = serde_json::json!("/work");
        let mut state = AppState::new("profiles-actions");
        state.profile = "fixture".into();
        state.link_state = LinkView::Connected;
        update(
            &mut state,
            AppMessage::Opened {
                opened: OpenedSession::from_snapshot(serde_json::from_value(snapshot).unwrap()),
                notice: None,
            },
        );
        profiles_input(&mut state, ProfilesInput::Open);
        let served = ["default", "fixture"].map(String::from).to_vec();
        profiles_input(
            &mut state,
            ProfilesInput::Discovered(
                0,
                Ok(GatewayView {
                    served,
                    parked: vec![("fixture-b".into(), "unusable".into())],
                }),
            ),
        );
        profiles_input(&mut state, ProfilesInput::Listed(0, Err("no".into())));
        profiles_input(
            &mut state,
            ProfilesInput::Topology(
                0,
                Ok(HostTopology {
                    stopped: vec![],
                    own_gateway: Some(vec![]),
                    gateway_mode: Some("multiplex".into()),
                    single_reason: None,
                }),
            ),
        );
        // The connected profile is selected first; `default` sorts above it.
        profiles_input(&mut state, ProfilesInput::Key(KeyAction::Up));
        state
    }

    fn key(state: &mut AppState, key: KeyAction) -> Vec<AppEffect> {
        update(
            state,
            AppMessage::User(UserAction::Profiles(ProfilesInput::Key(key))),
        )
    }

    fn lines(effects: &[AppEffect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::CommitLines(lines) => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn roles_title(state: &AppState) -> Option<String> {
        state.view().model_roles.map(|view| view.title)
    }

    #[test]
    fn models_page_opened_for_another_profile_stays_open_and_returns_to_profiles() {
        for opener in [KeyAction::Models, KeyAction::Open] {
            let mut state = loaded();
            let effects = key(&mut state, opener.clone());
            assert!(!state.profiles.is_open(), "{opener:?} leaves the list");
            assert!(
                effects.iter().any(|effect| matches!(
                    effect,
                    AppEffect::ModelRoles { profile, .. } if profile == "default"
                )),
                "{opener:?} loads the page for default, not the fixture connection"
            );
            let title = roles_title(&state).expect("the page is drawn");
            assert!(title.ends_with("default"), "{title}");

            // A reply for the page keeps it open: it belongs to default, not the connection.
            update(
                &mut state,
                AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Down))),
            );
            assert!(roles_title(&state).is_some(), "{opener:?} keeps the page");

            // Closing the page lands back in /profiles.
            update(
                &mut state,
                AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Escape))),
            );
            assert!(roles_title(&state).is_none());
            assert!(state.profiles.is_open(), "{opener:?} returns to /profiles");
        }

        // /models still opens the connected profile and does not return to /profiles.
        let mut state = loaded();
        profiles_input(&mut state, ProfilesInput::Closed);
        open_model_roles(&mut state);
        assert!(roles_title(&state).is_some_and(|title| title.ends_with("fixture")));
        update(
            &mut state,
            AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Escape))),
        );
        assert!(!state.profiles.is_open());
    }

    /// A main model saved on the page shows in the list the moment the page closes.
    #[test]
    fn a_main_model_saved_on_the_models_page_shows_in_the_list_at_once() {
        use talaria_core::model_roles::{ApplyReply, ApplyStep, AssignOutcome};
        let model_cell = |state: &AppState| {
            state
                .profiles
                .view()
                .rows
                .into_iter()
                .find(|row| row.name_cell == "default")
                .map(|row| row.model_cell)
        };
        let mut state = loaded();
        key(&mut state, KeyAction::Models);
        update(
            &mut state,
            AppMessage::User(UserAction::ModelRoles(RolesInput::Applied {
                step: ApplyStep::MainModel,
                reply: ApplyReply::Assigned(AssignOutcome::Main {
                    provider: "fixture-provider".to_owned(),
                    model: "fixture-model".to_owned(),
                    stale: Vec::new(),
                }),
            })),
        );
        update(
            &mut state,
            AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Escape))),
        );
        assert!(state.profiles.is_open());
        assert_eq!(
            model_cell(&state).as_deref(),
            Some("fixture-model · fixture-provider")
        );

        // Closing without a save leaves the list as Hermes last reported it.
        let mut state = loaded();
        let before = model_cell(&state);
        key(&mut state, KeyAction::Models);
        update(
            &mut state,
            AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Escape))),
        );
        assert_eq!(model_cell(&state), before);
    }

    /// Over a remote gateway the registry routes need this machine's control socket, so delete
    /// and rename are refused before anything is typed or sent.
    #[test]
    fn delete_and_rename_are_refused_over_a_remote_gateway() {
        for (action, words) in [
            (
                KeyAction::Delete,
                "Talaria deletes profiles only through a gateway on this machine. On the gateway's machine, run: hermes profile delete default",
            ),
            (
                KeyAction::Rename,
                "Talaria renames profiles only through a gateway on this machine. On the gateway's machine, run: hermes profile rename default NEW-NAME",
            ),
        ] {
            let mut state = loaded();
            state.attached_to_remote_gateway();
            let effects = key(&mut state, action);
            assert!(
                !effects.iter().any(|effect| matches!(
                    effect,
                    AppEffect::Profiles(
                        ProfilesEffect::Delete { .. } | ProfilesEffect::Rename { .. }
                    )
                )),
                "{words}"
            );
            assert_eq!(state.profiles.view().notice.as_deref(), Some(words));
        }
    }

    #[test]
    fn models_page_for_another_profile_closes_when_the_connection_moves() {
        let mut state = loaded();
        key(&mut state, KeyAction::Models);
        state.profile = "elsewhere".into();
        update(
            &mut state,
            AppMessage::User(UserAction::ModelRoles(RolesInput::Key(RolesKey::Down))),
        );
        assert!(roles_title(&state).is_none());
        assert!(
            !state.profiles.is_open(),
            "a moved connection does not reopen the list"
        );
    }

    #[test]
    fn connect_puts_the_previewed_skin_back_before_it_starts_on_another_profile() {
        use super::choice::tests::{answered, open, previewing, switches, writes};
        let mut state = loaded();
        open(&mut state);
        previewing(&mut state);
        let _ = answered(&mut state, "ares");
        key(&mut state, KeyAction::Connect);
        let effects = key(&mut state, KeyAction::Char('y'));
        assert!(
            state.choice.is_none(),
            "the selector closes before the start"
        );
        assert_eq!(writes(&effects), ["default"], "the restore goes to fixture");
        assert_eq!(switches(&effects), 0, "the start waits for the restore");
        let effects = answered(&mut state, "default");
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                AppEffect::StartInProfile { profile, .. } if profile == "default"
            )),
            "the start on default runs once fixture has its skin back"
        );
    }

    #[test]
    fn connect_starts_a_session_on_the_profile_in_this_folder() {
        let mut state = loaded();
        key(&mut state, KeyAction::Connect);
        let effects = key(&mut state, KeyAction::Char('y'));
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                AppEffect::StartInProfile { profile, cwd }
                    if profile == "default" && cwd == Path::new("/work")
            )),
            "the start runs on default in the session's folder"
        );
        assert!(!state.profiles.is_open());
        assert!(state.switching);
    }

    #[test]
    fn start_is_refused_while_prompts_are_held_or_a_switch_runs() {
        let mut state = loaded();
        state.held.push_back("unsent words".into());
        key(&mut state, KeyAction::Connect);
        let effects = key(&mut state, KeyAction::Char('y'));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::StartInProfile { .. }))
        );
        assert!(
            lines(&effects)
                .iter()
                .any(|line| line.contains(crate::switch::HELD_PROMPTS_TEXT)),
            "{:?}",
            lines(&effects)
        );

        let mut state = loaded();
        state.switching = true;
        key(&mut state, KeyAction::Connect);
        let effects = key(&mut state, KeyAction::Char('y'));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::StartInProfile { .. }))
        );
        assert!(lines(&effects).contains(&SWITCHING_NOTICE.to_owned()));
    }

    /// A terminal whose keymap binds the models action to F5 and leaves connect unbound.
    #[derive(Default)]
    struct Rebound {
        drawn: Option<ProfilesView>,
    }
    impl ComposerDeadline for Rebound {
        fn tick(&mut self, _: Instant) {}
        fn next_tick(&self) -> Option<Instant> {
            None
        }
    }
    impl StatusHost for Rebound {
        fn on_tick(&mut self, _: Instant) {}
        fn next_wake(&self, _: Instant) -> Option<Instant> {
            None
        }
        fn shutdown(&mut self) -> impl Future<Output = ()> + Send {
            std::future::ready(())
        }
    }
    impl Screen for Rebound {
        type Input = ();
        fn input(&mut self, _: (), _: &View<'_>, _: Instant) -> Vec<UserAction> {
            Vec::new()
        }
        fn draw(&mut self, _: &View<'_>) {}
        fn commit_lines(&mut self, _: Vec<String>) {}
        fn commit_context(&mut self, _block: &ContextBlock) {}
        fn commit_cells(&mut self, _: &[Cell]) {}
        fn load_draft(&mut self, _: &str) {}
        fn restore(&mut self) {}
        fn sync_profiles(&mut self, view: Option<&ProfilesView>) {
            self.drawn = view.cloned();
        }
        fn profiles_key_labels(&self) -> talaria_core::profiles::KeyLabels {
            talaria_core::profiles::KeyLabels {
                models: Some("F5".into()),
                connect: None,
                ..Default::default()
            }
        }
    }

    #[test]
    fn profiles_hints_use_the_terminal_key_labels() {
        let mut state = loaded();
        let mut screen = Rebound::default();
        sync_models(&mut state, &mut screen);
        let drawn = screen.drawn.expect("the profiles screen is drawn");
        // Creating a profile leads the hints; the rebound models key follows it.
        assert_eq!(
            drawn.hints.get(..2),
            Some(&["n new profile".to_owned(), "F5 models".to_owned()][..])
        );
        assert!(
            !drawn.hints.iter().any(|hint| hint.ends_with("connect")),
            "{:?}",
            drawn.hints
        );
    }
}

#[cfg(test)]
mod settings_reload_key_tests {
    use super::*;
    use talaria_core::settings::{
        ConfigSchemaView, FieldKind, FieldView, SettingsAction, SettingsInput, SettingsKey,
        SettingsSnapshot,
    };

    /// A terminal whose keymap binds reloading settings to F5.
    struct ReloadOnF5;
    impl ComposerDeadline for ReloadOnF5 {
        fn tick(&mut self, _: Instant) {}
        fn next_tick(&self) -> Option<Instant> {
            None
        }
    }
    impl StatusHost for ReloadOnF5 {
        fn on_tick(&mut self, _: Instant) {}
        fn next_wake(&self, _: Instant) -> Option<Instant> {
            None
        }
        fn shutdown(&mut self) -> impl Future<Output = ()> + Send {
            std::future::ready(())
        }
    }
    impl Screen for ReloadOnF5 {
        type Input = ();
        fn input(&mut self, _: (), _: &View<'_>, _: Instant) -> Vec<UserAction> {
            Vec::new()
        }
        fn draw(&mut self, _: &View<'_>) {}
        fn commit_lines(&mut self, _: Vec<String>) {}
        fn commit_context(&mut self, _block: &ContextBlock) {}
        fn commit_cells(&mut self, _: &[Cell]) {}
        fn load_draft(&mut self, _: &str) {}
        fn restore(&mut self) {}
        fn settings_reload_key(&self) -> Option<String> {
            Some("F5".into())
        }
    }

    #[test]
    fn the_chooser_hint_names_the_terminal_reload_key() {
        let mut state = AppState::new("settings-test");
        state.settings.update(SettingsInput::Open {
            profile: "fixture".into(),
            filter: None,
        });
        state
            .settings
            .update(SettingsInput::Loaded(SettingsSnapshot {
                profile: "fixture".into(),
                schema: ConfigSchemaView {
                    fields: vec![FieldView {
                        key: "terminal.backend".into(),
                        kind: FieldKind::String,
                        description: String::new(),
                        category: "general".into(),
                        options: None,
                        searchable: false,
                        clearable: false,
                    }],
                    category_order: vec!["general".into()],
                },
                effective: serde_json::json!({"terminal": {"backend": "local"}}),
                raw: serde_json::json!({}),
                defaults: serde_json::json!({}),
            }));
        state
            .settings
            .update(SettingsInput::Key(SettingsKey::Action(
                SettingsAction::Edit,
            )));
        assert!(
            state
                .settings
                .view()
                .footer
                .ends_with("· Ctrl+R reload list · Esc cancel")
        );
        sync_models(&mut state, &mut ReloadOnF5);
        let footer = state.settings.view().footer;
        assert!(
            footer.ends_with("· F5 reload list · Esc cancel"),
            "{footer}"
        );
    }
}

#[cfg(test)]
mod cron_binding_tests {
    use super::*;

    fn state() -> AppState {
        let mut state = AppState::new("cron-test");
        state.profile = "fixture".into();
        cron_sync_profile(&mut state);
        state.cron.state = Some(CronState::new("fixture".into(), Ok(())));
        state
    }

    #[test]
    fn same_profile_keeps_state_and_returning_to_profile_advances_generation() {
        let mut state = state();
        open_cron(&mut state, CronOpen::NewJob);
        let generation = state.cron.generation;
        cron_sync_profile(&mut state);
        assert_eq!(state.cron.generation, generation);
        assert!(matches!(cron_view(&state).unwrap().body, CronBody::Form(_)));
        state.profile = "other".into();
        cron_sync_profile(&mut state);
        state.profile = "fixture".into();
        cron_sync_profile(&mut state);
        assert_eq!(state.cron.generation, generation + 2);
        assert!(state.cron.state.is_none());
    }

    #[test]
    fn stale_read_is_dropped_but_trigger_keeps_original_profile_in_notice() {
        let mut state = state();
        let generation = state.cron.generation;
        state.profile = "other".into();
        let old = crate::cron::CronTag {
            generation,
            profile: "fixture".into(),
            run_name: None,
        };
        let effects = update(
            &mut state,
            AppMessage::Cron {
                tag: old.clone(),
                input: CronInput::Listed {
                    seq: 1,
                    result: Ok(cron::ListReply {
                        jobs: vec![],
                        warning: None,
                    }),
                },
            },
        );
        assert!(effects.is_empty());
        let effects = update(
            &mut state,
            AppMessage::Cron {
                tag: crate::cron::CronTag {
                    run_name: Some("digest".into()),
                    ..old
                },
                input: CronInput::Triggered {
                    job_id: "job".into(),
                    outcome: cron::TriggerOutcome::Accepted,
                },
            },
        );
        assert!(
            matches!(&effects[..], [AppEffect::CronNotice(text)] if text.contains("digest") && text.contains("fixture"))
        );
    }

    #[test]
    fn escape_cancels_opening_before_a_service_is_prepared() {
        let mut state = AppState::new("cron-opening");
        open_cron(&mut state, CronOpen::List);
        assert!(cron_view(&state).is_some());
        cron_input(&mut state, CronInput::Key(CronKey::Esc));
        assert!(cron_view(&state).is_none());
        assert!(state.cron.deadline.is_none());
    }

    #[test]
    fn failed_run_switch_uses_a_notice_and_clears_switch_state() {
        let mut state = state();
        state.cron.switching_run = true;
        state.switching = true;
        state.session_action = true;
        let effects = update(
            &mut state,
            AppMessage::Switched {
                outcome: SwitchOutcome::Failed {
                    text: "session missing".into(),
                },
                install: None,
                reason: None,
            },
        );
        assert!(matches!(&effects[..], [AppEffect::CronNotice(text)] if text == "session missing"));
        assert!(!state.switching && !state.session_action && !state.cron.switching_run);
    }
}

fn workflow_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    if argument.trim().is_empty() {
        state.workflow_views.active = None;
        return vec![AppEffect::WorkflowDiscover];
    }
    state.workflow_views.picker = None;
    let run = PathBuf::from(argument.trim());
    if state.workflow_views.states.contains_key(&run) {
        state.workflow_views.active = Some(run.clone());
        state.workflow_views.last_poll_ms = state.now_ms;
        return workflow_input(
            state,
            run,
            talaria_workflow::progress::ProgressInput::Reopen {
                connected: state.link_state == LinkView::Connected,
            },
        );
    }
    vec![AppEffect::WorkflowOpen(run)]
}
fn workflow_picker_input(
    state: &mut AppState,
    key: talaria_workflow::progress::live::ProgressKey,
) -> Vec<AppEffect> {
    use talaria_workflow::progress::live::ProgressKey;
    let Some(picker) = state.workflow_views.picker.as_mut() else {
        return vec![];
    };
    match key {
        ProgressKey::Up | ProgressKey::Down => {
            picker.move_selection(key == ProgressKey::Down);
        }
        ProgressKey::OpenAgent | ProgressKey::Enter => {
            let Some(row) = picker.selected() else {
                return vec![];
            };
            let run = row.run_dir.clone();
            return workflow_command(state, &run);
        }
        ProgressKey::CloseView => state.workflow_views.picker = None,
        _ => return vec![],
    }
    vec![AppEffect::Redraw]
}
fn workflow_opened(
    state: &mut AppState,
    result: Result<crate::workflow_progress::OpenedRun, String>,
) -> Vec<AppEffect> {
    match result {
        Ok(opened) => {
            let run = opened.path;
            state
                .workflow_views
                .profiles
                .insert(run.clone(), opened.profile);
            let mut view = opened.state;
            view.now_ms = (state.wall_s * 1000.0) as i64;
            view.opened_at_ms = view.now_ms;
            view.last_journal_seen_ms = view.now_ms;
            state
                .workflow_views
                .offsets
                .insert(run.clone(), opened.offset);
            state.workflow_views.states.insert(run.clone(), view);
            if let Some((pending, selection)) = state.workflow_views.pending_selection.take()
                && pending == run
                && let Some(view) = state.workflow_views.states.get_mut(&run)
            {
                view.selection = selection;
            }
            state.workflow_views.active = Some(run.clone());
            state.workflow_views.last_poll_ms = state.now_ms;
            let mut effects = workflow_input(
                state,
                run,
                talaria_workflow::progress::live::ProgressInput::Connected(
                    state.link_state == LinkView::Connected,
                ),
            );
            effects.push(AppEffect::Redraw);
            effects
        }
        Err(reason) => vec![notice_lines(&reason)],
    }
}
fn workflow_polled(
    state: &mut AppState,
    run: PathBuf,
    result: Result<crate::workflow_progress::PollBatch, String>,
) -> Vec<AppEffect> {
    state.workflow_views.polling.remove(&run);
    match result {
        Ok(batch) => {
            state
                .workflow_views
                .offsets
                .insert(run.clone(), batch.offset);
            let mut effects = workflow_input(
                state,
                run.clone(),
                talaria_workflow::progress::live::ProgressInput::Journal(batch.lines),
            );
            if let Some(verdict) = batch.verdict {
                effects.extend(workflow_input(
                    state,
                    run.clone(),
                    talaria_workflow::progress::live::ProgressInput::Verdict(verdict),
                ));
            }
            effects.extend(workflow_input(
                state,
                run.clone(),
                talaria_workflow::progress::live::ProgressInput::Controls(batch.controls),
            ));
            if let Some(alive) = batch.runner_alive {
                effects.extend(workflow_input(
                    state,
                    run,
                    talaria_workflow::progress::live::ProgressInput::Runner { alive },
                ));
            }
            effects
        }
        Err(reason) => workflow_input(
            state,
            run,
            talaria_workflow::progress::live::ProgressInput::Error(reason),
        ),
    }
}
/// Read the runs folder when a session waits for a workflow run and a read is due.
fn hivecode_scan(state: &mut AppState) -> Vec<AppEffect> {
    state
        .hivecode
        .scan_due(state.now_ms)
        .map(|since_ms| AppEffect::HivecodeScan { since_ms })
        .into_iter()
        .collect()
}
/// Announce the foreground session's run starting and ending; other sessions only update.
fn hivecode_runs(
    state: &mut AppState,
    rows: &[talaria_workflow::runner::RunSummary],
) -> Vec<AppEffect> {
    let key = state.workflow_key_label.clone();
    let actions = state.hivecode.on_runs(rows, state.now_ms, key.as_deref());
    let actions = actions
        .into_iter()
        .filter(|(session, action)| {
            matches!(action, crate::hivecode::Action::Changed)
                || is_foreground(state, session.as_str())
        })
        .map(|(_, action)| action)
        .collect();
    apply_hivecode(state, actions, false)
}
fn workflow_tick(state: &mut AppState) -> Vec<AppEffect> {
    state.workflow_views.last_tick_ms = state.now_ms;
    let Some(run) = state.workflow_views.active.clone() else {
        return vec![];
    };
    if state
        .workflow_views
        .states
        .get(&run)
        .is_some_and(|view| view.error.is_some())
    {
        return vec![];
    }
    let mut effects = workflow_input(
        state,
        run.clone(),
        talaria_workflow::progress::live::ProgressInput::Clock {
            now_ms: (state.wall_s * 1000.0) as i64,
        },
    );
    state.workflow_views.last_poll_ms = state.now_ms;
    if state.workflow_views.polling.insert(run.clone()) {
        effects.push(AppEffect::WorkflowPoll {
            offset: state.workflow_views.offsets.get(&run).copied().unwrap_or(0),
            last_seq: state
                .workflow_views
                .states
                .get(&run)
                .map_or(0, |view| view.progress.last_seq),
            run,
        });
    }
    effects
}
fn workflow_input(
    state: &mut AppState,
    run: PathBuf,
    input: talaria_workflow::progress::live::ProgressInput,
) -> Vec<AppEffect> {
    let poll_after = matches!(
        &input,
        talaria_workflow::progress::live::ProgressInput::GatewayResponse { .. }
            | talaria_workflow::progress::live::ProgressInput::StopReceipt { .. }
    );
    let Some(view) = state.workflow_views.states.get_mut(&run) else {
        return vec![];
    };
    let previous = view.revision;
    let mut effects = vec![];
    if matches!(&input, talaria_workflow::progress::ProgressInput::Key(_)) {
        effects.extend(talaria_workflow::progress::update(
            view,
            talaria_workflow::progress::ProgressInput::Clock {
                now_ms: (state.wall_s * 1000.0) as i64,
            },
        ));
    }
    effects.extend(talaria_workflow::progress::live::update(view, input));
    let keys = view
        .progress
        .agents
        .iter()
        .flat_map(|a| {
            [a.session_id.clone(), a.stored_session_id.clone()]
                .into_iter()
                .flatten()
                .map(|s| (s, a.key.clone()))
        })
        .collect();
    let mut result = vec![];
    if !effects.is_empty()
        || (!view.connected
            && view.error.is_none()
            && !view.closed
            && state
                .workflow_views
                .profiles
                .get(&run)
                .is_some_and(|p| !p.is_empty() && p != &state.profile))
    {
        result.push(AppEffect::WorkflowExecute {
            run: run.clone(),
            effects,
            keys,
            at_ms: view.now_ms,
        });
    }
    if let Some(key) = view.selected_agent_key().map(str::to_owned) {
        if let Some(a) = view.progress.agent(&key) {
            let identity = (key.clone(), a.attempt, a.turn);
            if view.prompt_identity.as_ref() != Some(&identity) {
                view.prompt_first_line = None;
                if let Some(reference) = a.prompt_ref.clone() {
                    result.push(AppEffect::WorkflowPrompt {
                        run: run.clone(),
                        key,
                        attempt: a.attempt,
                        turn: a.turn,
                        reference,
                    });
                }
                view.prompt_identity = Some(identity);
            }
        }
    } else {
        view.prompt_identity = None;
        view.prompt_first_line = None;
    }
    let run_name = run.file_name().map_or_else(
        || run.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    result.extend(
        view.take_diagnostics()
            .into_iter()
            .map(|line| AppEffect::WorkflowLog(format!("{line} (run {run_name})"))),
    );
    let opened = view.opened_session.take();
    let selection = view.selection.clone();
    let closed = view.closed;
    if view.revision != previous {
        result.push(AppEffect::Redraw);
    }
    if poll_after && state.workflow_views.polling.insert(run.clone()) {
        result.push(AppEffect::WorkflowPoll {
            run: run.clone(),
            offset: state.workflow_views.offsets.get(&run).copied().unwrap_or(0),
            last_seq: view.progress.last_seq,
        });
    }
    if closed && state.workflow_views.active.as_ref() == Some(&run) {
        state.workflow_views.active = None;
    }
    if let Some(session) = opened {
        let profile = state
            .workflow_views
            .profiles
            .get(&run)
            .filter(|p| !p.is_empty())
            .cloned()
            .unwrap_or_else(|| state.profile.clone());
        let target = RowKey {
            profile: profile.clone(),
            session_id: session.clone(),
        };
        let started = if profile == state.profile {
            resume_command(state, &session)
        } else {
            begin_switch(state, target.clone())
        };
        if started
            .iter()
            .any(|effect| matches!(effect, AppEffect::Switch { .. }))
        {
            workflow_origin_enter(state, run.clone(), selection, target);
        }
        result.extend(started);
    }
    result
}

/// The run view's Enter has started a switch to an agent's session: remember where the person
/// is. An agent's session is never the place to return to, so an Enter made from one keeps the
/// place the first Enter remembered, and shows the run and row of the newest Enter.
fn workflow_origin_enter(
    state: &mut AppState,
    run: PathBuf,
    selection: talaria_workflow::progress::live::NodeKey,
    target: RowKey,
) {
    use crate::workflow_origin::{WorkflowOrigin, origin_row};
    let here = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone());
    let kept = state
        .workflow_origin
        .as_mut()
        .filter(|origin| here.as_deref().is_some_and(|id| origin.is_away_in(id)));
    if let Some(origin) = kept {
        origin.retarget(run, selection, target);
        return;
    }
    state.workflow_origin = if state.workflow_agent.is_some() {
        None
    } else {
        let profile = state
            .opened_profile
            .clone()
            .unwrap_or_else(|| state.profile.clone());
        state
            .opened
            .as_ref()
            .and_then(|opened| origin_row(opened, &profile))
            .map(|session| WorkflowOrigin::leaving(session, run, selection, target))
    };
}

/// A switch finished. Settle the trip it belonged to: confirm the agent's session, drop the
/// memory when the switch landed elsewhere, or open the run view when the trip back landed.
fn workflow_origin_settle(state: &mut AppState, switched_in: bool) -> Vec<AppEffect> {
    if let Some(plan) = state.workflow_return.take() {
        let landed = switched_in
            && state
                .opened
                .as_ref()
                .is_some_and(|opened| plan.matches(opened, &state.profile));
        return if landed {
            reopen_workflow_view(state, plan.run, plan.selection)
        } else {
            Vec::new()
        };
    }
    let Some(origin) = state.workflow_origin.as_mut() else {
        return Vec::new();
    };
    if !origin.is_leaving() {
        return Vec::new();
    }
    let kept = if switched_in {
        state
            .opened
            .as_ref()
            .is_some_and(|opened| origin.landed(opened, &state.profile))
    } else {
        origin.failed()
    };
    if !kept {
        state.workflow_origin = None;
    }
    Vec::new()
}

/// A session opened by some route other than the trip: the memory follows the same stored
/// session across a reconnect and ends for any other.
fn workflow_origin_follow(state: &mut AppState, opened: &OpenedSession) {
    let Some(origin) = state.workflow_origin.as_mut() else {
        return;
    };
    if origin.is_leaving() {
        return;
    }
    let same_profile = state
        .opened_profile
        .as_deref()
        .is_none_or(|profile| profile == state.profile);
    let kept = state
        .opened
        .as_ref()
        .is_some_and(|old| origin.follow_reopen(old, opened, same_profile));
    if !kept {
        state.workflow_origin = None;
    }
}

/// The key in an agent's session: go back to the session the person came from. Nothing is sent
/// to the agent; the session is left as any switch leaves it.
fn workflow_agent_back(state: &mut AppState) -> Vec<AppEffect> {
    let Some(here) = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
    else {
        return Vec::new();
    };
    let Some(target) = state
        .workflow_origin
        .as_ref()
        .filter(|origin| origin.is_away_in(&here))
        .map(|origin| origin.session().clone())
    else {
        return Vec::new();
    };
    if state.workflow_agent.is_none()
        || !state.draft.is_empty()
        || state.palette.open
        || state.picker.is_some()
    {
        return Vec::new();
    }
    let effects = begin_switch(state, target);
    if effects
        .iter()
        .any(|effect| matches!(effect, AppEffect::Switch { .. }))
        && let Some(origin) = state.workflow_origin.take()
    {
        state.workflow_return = Some(origin.into_return());
    }
    effects
}

/// Open the run's view again with the row selected that was selected when Enter left it.
fn reopen_workflow_view(
    state: &mut AppState,
    run: PathBuf,
    selection: talaria_workflow::progress::live::NodeKey,
) -> Vec<AppEffect> {
    state.workflow_views.picker = None;
    if !state.workflow_views.states.contains_key(&run) {
        state.workflow_views.pending_selection = Some((run.clone(), selection));
        return vec![AppEffect::WorkflowOpen(run)];
    }
    if let Some(view) = state.workflow_views.states.get_mut(&run) {
        view.selection = selection;
    }
    state.workflow_views.active = Some(run.clone());
    state.workflow_views.last_poll_ms = state.now_ms;
    let connected = state.link_state == LinkView::Connected;
    let mut effects = workflow_input(
        state,
        run,
        talaria_workflow::progress::live::ProgressInput::Reopen { connected },
    );
    effects.push(AppEffect::Redraw);
    effects
}

fn workflow_link(state: &mut AppState, event: &LinkEvent) -> Vec<AppEffect> {
    use talaria_workflow::progress::live::ProgressInput as I;
    let runs: Vec<_> = state.workflow_views.states.keys().cloned().collect();
    let mut effects = vec![];
    for run in runs {
        if state.workflow_views.scoped.contains(&run) {
            continue;
        }
        match event {
            LinkEvent::State(status) => effects.extend(workflow_input(
                state,
                run,
                I::Connected(matches!(status, LinkState::Connected { .. })),
            )),
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            } => {
                if let Some(session) = &event.frame.session_id
                    && state
                        .workflow_views
                        .states
                        .get_mut(&run)
                        .is_some_and(|v| v.progress.for_session_mut(session).is_some())
                {
                    if let Some(generation) = event.frame.execution_generation {
                        effects.extend(workflow_input(
                            state,
                            run.clone(),
                            I::Generation {
                                session_id: session.clone(),
                                generation,
                            },
                        ));
                    }
                    let input = if let Some(subagent) =
                        talaria_core::subagents::SubagentEvent::from_notification(
                            &event.frame.notification,
                        ) {
                        I::Subagent {
                            session_id: session.clone(),
                            event: subagent,
                        }
                    } else {
                        I::Gateway {
                            session_id: session.clone(),
                            event: event.frame.notification.clone(),
                        }
                    };
                    effects.extend(workflow_input(state, run, input));
                }
            }
            LinkEvent::Gap(gap) => effects.extend(workflow_input(
                state,
                run,
                I::ReplayGap {
                    session_id: gap.session_id.clone(),
                },
            )),
            _ => {}
        }
    }
    effects
}

#[cfg(test)]
mod workflow_wake_tests {
    use super::*;

    #[tokio::test]
    async fn idle_workflow_has_a_deadline_and_opening_or_reopening_rearms_it() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let executor = Executor::new(tx);
        let mut state = AppState::new("workflow-wake");
        assert!(workflow_wake(&state, &executor).is_none());
        let run = PathBuf::from("idle-run");
        let manifest = serde_json::from_value(serde_json::json!({
            "v":1,"run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C", "created_at_ms":1000,
            "workflow":{"name":"review-branch","script_sha256":"a".repeat(64)},
            "args":null,"started_at_ms":1,"seed":4242,"profile":"fixture",
            "on_unknown":"wait","talaria_version":env!("CARGO_PKG_VERSION"), "hermes":{},
            "roster":{},"approval_policy":{}
        }))
        .unwrap();
        let opened = crate::workflow_progress::OpenedRun {
            path: run.clone(),
            profile: "fixture".into(),
            state: talaria_workflow::progress::live::ProgressState::blank(&manifest),
            offset: 0,
        };
        state.now_ms = 700;
        workflow_opened(&mut state, Ok(opened));
        assert_eq!(state.workflow_views.last_poll_ms, 700);
        assert_eq!(
            workflow_wake(&state, &executor),
            Some(executor.started + Duration::from_millis(950))
        );
        state.now_ms = 950;
        assert!(
            workflow_tick(&mut state)
                .iter()
                .any(|e| matches!(e, AppEffect::WorkflowPoll { .. }))
        );
        assert_eq!(
            workflow_wake(&state, &executor),
            Some(executor.started + Duration::from_millis(1200))
        );
        // An in-flight disk poll must not leave an overdue timer spinning the loop.
        state.now_ms = 1200;
        workflow_tick(&mut state);
        assert_eq!(
            workflow_wake(&state, &executor),
            Some(executor.started + Duration::from_millis(1450))
        );
        state.workflow_views.active = None;
        assert!(workflow_wake(&state, &executor).is_none());
        state.now_ms = 2000;
        workflow_command(&mut state, "idle-run");
        assert_eq!(state.workflow_views.last_poll_ms, 2000);
        let opened = workflow_input(
            &mut state,
            run.clone(),
            talaria_workflow::progress::live::ProgressInput::Opened("fixture-session".into()),
        );
        assert!(opened.iter().any(|e| matches!(e, AppEffect::Switch {key} if key.profile == "fixture" && key.session_id == "fixture-session")));
        state.workflow_views.states.get_mut(&run).unwrap().connected = true;
        state.workflow_views.scoped.insert(run.clone());
        workflow_link(&mut state, &LinkEvent::State(LinkState::Closed));
        assert!(
            state.workflow_views.states[&run].connected,
            "foreground disconnect must not disable a scoped workflow link"
        );
        state.workflow_views.scoped.clear();
        workflow_link(&mut state, &LinkEvent::State(LinkState::Closed));
        assert!(
            !state.workflow_views.states[&run].connected,
            "fallback views still follow the foreground link"
        );
        let failed = workflow_input(
            &mut state,
            run.clone(),
            talaria_workflow::progress::live::ProgressInput::Error("corrupt".into()),
        );
        assert!(
            !failed
                .iter()
                .any(|e| matches!(e, AppEffect::WorkflowExecute { .. })),
            "connection failure must not start another connection loop"
        );
        assert!(workflow_wake(&state, &executor).is_none());
    }

    #[test]
    fn closing_and_reopening_a_folded_view_lists_its_agents_and_logs_unknown_events() {
        use talaria_workflow::progress::live::{NodeKey, ProgressInput, ProgressKey};
        let mut state = AppState::new("workflow-reopen");
        let run = PathBuf::from("reopen-run");
        let manifest = serde_json::from_value(serde_json::json!({
            "v":1,"run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C", "created_at_ms":1000,
            "workflow":{"name":"review-branch","script_sha256":"a".repeat(64)},
            "args":null,"started_at_ms":1,"seed":4242,"profile":"fixture",
            "on_unknown":"wait","talaria_version":env!("CARGO_PKG_VERSION"), "hermes":{},
            "roster":{},"approval_policy":{}
        }))
        .unwrap();
        let key = "8".repeat(32);
        let lines: Vec<talaria_workflow::journal::RecordLine> = [
            serde_json::json!({"t":"run_started","run_id":"01JB2X9Q4M8Z7K3T5V6W8Y0A1C",
                "script_sha256":"a".repeat(64)}),
            serde_json::json!({"t":"call_intent","key":key,"attempt":0,
                "call_id":{"digest":key,"occurrence":0},"label":"fixture-row","phase":"Verify",
                "workflow_path":[],"launch":{"profile":"fixture","model":"fixture-model",
                "provider":null,"effort":null,"toolsets":[],"skills":[],"max_turns":null,
                "isolation":null,"cwd":null},"request_id":"wf:fixture"}),
            serde_json::json!({"t":"session_bound","key":key,"attempt":0,
                "session_id":"fixture-session","stored_session_id":"fixture-session"}),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, mut record)| {
            record["v"] = 1.into();
            record["seq"] = (i as u64 + 1).into();
            record["at_ms"] = (1000 + i as i64).into();
            serde_json::from_value(record).unwrap()
        })
        .collect();
        let opened = crate::workflow_progress::OpenedRun {
            path: run.clone(),
            profile: "fixture".into(),
            state: talaria_workflow::progress::live::ProgressState::new(
                talaria_workflow::progress::RunProgress::from_records(&manifest, &lines),
            ),
            offset: 0,
        };
        workflow_opened(&mut state, Ok(opened));
        let listed = |state: &AppState| {
            state.workflow_views.states[&run]
                .visible_nodes()
                .iter()
                .any(|n| n.key == NodeKey::Agent(key.clone()))
        };
        assert!(listed(&state));
        for _ in 0..4 {
            workflow_input(&mut state, run.clone(), ProgressInput::Key(ProgressKey::Up));
            workflow_input(
                &mut state,
                run.clone(),
                ProgressInput::Key(ProgressKey::Left),
            );
        }
        workflow_input(
            &mut state,
            run.clone(),
            ProgressInput::Key(ProgressKey::CloseView),
        );
        workflow_command(&mut state, "reopen-run");
        assert!(listed(&state), "reopening must list the agents again");

        // A broadcast never reaches the footer; an unknown type for an owned session is logged once.
        let broadcast = hermes_contract::events::EventFrame::from_params(&serde_json::json!({
            "type":"platforms.changed","session_id":"","payload":{}
        }))
        .unwrap();
        workflow_input(
            &mut state,
            run.clone(),
            ProgressInput::Gateway {
                session_id: String::new(),
                event: broadcast.notification,
            },
        );
        assert_eq!(state.workflow_views.states[&run].footer, None);
        let unknown = || ProgressInput::Gateway {
            session_id: "fixture-session".into(),
            event: hermes_contract::events::EventFrame::from_params(&serde_json::json!({
                "type":"groups.changed","session_id":"fixture-session","payload":{}
            }))
            .unwrap()
            .notification,
        };
        let logged: Vec<_> = workflow_input(&mut state, run.clone(), unknown())
            .into_iter()
            .chain(workflow_input(&mut state, run.clone(), unknown()))
            .filter_map(|e| match e {
                AppEffect::WorkflowLog(text) => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(logged.len(), 1, "{logged:?}");
        assert!(
            logged[0].contains("groups.changed") && logged[0].contains("reopen-run"),
            "{logged:?}"
        );
        assert_eq!(state.workflow_views.states[&run].footer, None);
    }
}

#[cfg(test)]
mod hivecode_visibility_tests {
    use super::*;
    use talaria_core::hivecode::{HivecodeCommand, notice};

    fn session(runtime: &str, stored: &str) -> OpenedSession {
        OpenedSession {
            state: SessionState::empty(SessionId::new(runtime)),
            session_id: runtime.into(),
            stored_session_id: stored.into(),
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
        }
    }

    fn opened(runtime: &str) -> AppState {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        state.opened = Some(session(runtime, &format!("stored-{runtime}")));
        state
    }

    /// Turn Hivecode on the way `/hivecode on` does once the catalogue lists the skill.
    fn turn_on(state: &mut AppState, runtime: &str) {
        let result: talaria_core::commands::CatalogResult = serde_json::from_value(json!({
            "skills": {"/talaria-workflows": {"usage": 0, "origin": "local"}},
            "pairs": []
        }))
        .unwrap();
        let catalog = talaria_core::commands::CommandCatalog::from_result(&result);
        let _ = state.hivecode.on_catalog_reply(
            &SessionId::new(runtime),
            "fixture",
            HivecodeCommand::On,
            Ok(catalog),
        );
        assert!(state.hivecode.status_input(Some(&SessionId::new(runtime))));
    }

    fn lines(effects: &[AppEffect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::CommitLines(lines) => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn summary(
        status: talaria_workflow::runner::RunStatus,
        finished: u32,
    ) -> talaria_workflow::runner::RunSummary {
        talaria_workflow::runner::RunSummary {
            run_id: "run-0001".into(),
            name: "review-branch".into(),
            profile: "fixture".into(),
            started_at_ms: 2_000_000,
            status,
            agents_finished: finished,
            agents_total_known: 10,
            parked: 0,
            tokens_spent: None,
            cache_read_tokens: None,
            cache_unknown_turns: 0,
            money_spent_usd: None,
            run_dir: "fixture-runs/run-0001".into(),
            warning: None,
            attention: Default::default(),
        }
    }

    #[test]
    fn goal_with_hivecode_on_runs_command_with_notice() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let effects = submit(&mut state, "/goal build a fixture tool".into());
        let expected = vec![format!("\u{b7} {}", notice::bypassed("goal"))];
        assert!(
            matches!(effects.first(), Some(AppEffect::CommitLines(first)) if *first == expected),
            "the notice comes first: {effects:?}"
        );
        assert!(
            effects.iter().any(|effect| matches!(effect,
                AppEffect::RunCommand { typed, .. } if typed == "/goal build a fixture tool")),
            "the goal still goes to Hermes: {effects:?}"
        );
        let status = submit(&mut state, "/goal status".into());
        assert!(lines(&status).is_empty(), "{status:?}");
        let mut off = opened("session");
        let effects = submit(&mut off, "/goal build a fixture tool".into());
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn routed_prompt_prints_no_interim_line() {
        // The building block above the composer says the prompt is handled as a workflow.
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let effects = submit(&mut state, "fix the parser".into());
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    /// A frame from Hermes for the open session, numbered after the last one.
    fn frame(state: &mut AppState, kind: &str, payload: Value) -> Vec<AppEffect> {
        let opened = state.opened.as_mut().unwrap();
        let raw = json!({"session_id": opened.session_id, "replay_epoch": opened.cursor.replay_epoch,
            "seq": opened.cursor.last_sequence + 1, "type": kind, "payload": payload});
        update(
            state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(hermes_client::channel::InboundEvent {
                    generation: hermes_client::channel::Generation(1),
                    frame: EventFrame::from_params(&raw).unwrap(),
                    raw,
                }),
                replayed: false,
            }),
        )
    }

    /// What Hermes expands a routed prompt into.
    fn expanded(typed: &str) -> String {
        format!(
            "[IMPORTANT: The user has invoked the \"talaria-workflows\" skill, indicating they want you to follow its instructions. The full skill content is loaded below.]\n\n# Fixture workflow guide\n\nThe user has provided the following instruction alongside the skill invocation: {typed}"
        )
    }

    fn loads_skill(effects: &[AppEffect]) -> bool {
        effects.iter().any(|effect| {
            matches!(
                effect,
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::RunCommand { .. },
                    ..
                }
            )
        })
    }

    fn sent(effects: &[AppEffect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::Send(Effect::SendPrompt { text, .. }) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn user_cells(state: &AppState) -> Vec<String> {
        state
            .opened
            .as_ref()
            .unwrap()
            .state
            .cells()
            .iter()
            .filter_map(|cell| match cell {
                Cell::User(user) => Some(user.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Type `typed` into the composer and press Enter.
    fn enter(state: &mut AppState, typed: &str) -> Vec<AppEffect> {
        state.draft = typed.into();
        state.caret = state.draft.len();
        submit(state, typed.into())
    }

    /// Hermes answers the skill dispatch of a routed prompt.
    fn dispatched(state: &mut AppState, typed: &str) -> Vec<AppEffect> {
        let reply = json!({"type": "skill", "name": "talaria-workflows",
            "message": expanded(typed), "display": format!("/talaria-workflows {typed}")});
        update(
            state,
            AppMessage::HivecodeDone {
                session: SessionId::new("session"),
                original_text: typed.into(),
                restore_text: None,
                outcome: Ok(reply),
            },
        )
    }

    fn turn(state: &mut AppState, status: &str) -> Vec<AppEffect> {
        let mut effects = frame(state, "message.start", json!({}));
        effects.extend(frame(
            state,
            "message.complete",
            json!({"text": "Here is the proposed workflow.", "status": status}),
        ));
        effects
    }

    const DRAFT: &str = "/home/fixture/.talaria/drafts/fix-parser.workflow";

    fn tool_start(state: &mut AppState, id: &str, name: &str, args: Value) -> Vec<AppEffect> {
        frame(
            state,
            "tool.start",
            json!({"tool_id": id, "name": name, "args": args, "context": name}),
        )
    }

    fn stage(state: &AppState) -> Option<talaria_core::hivecode::authoring::AuthoringStage> {
        state.view().authoring.map(|block| block.progress.stage)
    }

    #[test]
    fn a_routed_turn_shows_the_building_block_until_it_ends() {
        use talaria_core::hivecode::authoring::AuthoringStage;
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        // Nothing shows until the turn starts.
        assert_eq!(stage(&state), None);
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(stage(&state), Some(AuthoringStage::Planning));
        state.now_ms = 66_000;
        let _ = tool_start(
            &mut state,
            "t1",
            "write_file",
            json!({"path": DRAFT, "content": "export const meta = {name:'fix-parser'};"}),
        );
        let block = state
            .view()
            .authoring
            .expect("the block shows while writing");
        assert_eq!(block.progress.stage, AuthoringStage::Writing);
        assert_eq!(
            block.progress.script.as_deref(),
            Some("fix-parser.workflow")
        );
        assert_eq!(block.elapsed_ms, 65_000);
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Here is the proposed workflow. Start it?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode wrote the workflow script fix-parser.workflow in 1m 05s and is waiting for your approval to start it."
            ]
        );
        assert_eq!(stage(&state), None, "the block leaves with the turn");
        // The line is committed once.
        let effects = update(&mut state, AppMessage::Tick { now_ms: 90_000 });
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn an_interrupted_build_says_it_stopped() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        state.now_ms = 13_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "", "status": "interrupted"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode stopped: interrupted while planning the workflow after 12s. No workflow started. Hivecode is still on."
            ]
        );
        assert_eq!(stage(&state), None);
    }

    #[test]
    fn an_interrupted_build_leaves_one_line_after_the_interrupted_notice() {
        let expected = [
            "\u{b7} Hivecode stopped: interrupted while planning the workflow after 12s. No workflow started. Hivecode is still on.",
        ];
        // Hermes's interrupt reply comes before the turn's end, and after it.
        for reply_first in [true, false] {
            let mut state = opened("session");
            turn_on(&mut state, "session");
            state.now_ms = 1_000;
            let _ = enter(&mut state, "fix the parser");
            let _ = dispatched(&mut state, "fix the parser");
            let _ = frame(&mut state, "message.start", json!({}));
            state.now_ms = 13_000;
            let mut all = vec![];
            if reply_first {
                all.extend(lines(&interrupt_done(
                    &mut state,
                    ExecuteOutcome::Sent,
                    Vec::new(),
                )));
            }
            let ended = frame(
                &mut state,
                "message.complete",
                json!({"text": "", "status": "interrupted"}),
            );
            all.extend(lines(&ended));
            // Hermes's own line goes into the scrollback first.
            let cells = state.opened.as_ref().unwrap().state.cells();
            let committed = ended.iter().position(|effect| {
                matches!(effect, AppEffect::CommitCells(range) if cells[range.clone()].iter().any(
                    |cell| matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::Interrupted)))
            });
            let line = ended
                .iter()
                .position(|effect| matches!(effect, AppEffect::CommitLines(_)));
            assert!(committed.is_some() && committed < line, "{ended:?}");
            if !reply_first {
                all.extend(lines(&interrupt_done(
                    &mut state,
                    ExecuteOutcome::Sent,
                    Vec::new(),
                )));
            }
            assert_eq!(all, expected, "reply first: {reply_first}");
            let interrupted = state.opened.as_ref().unwrap().state.cells().iter().any(
                |cell| matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::Interrupted),
            );
            assert!(interrupted, "Hermes's own line stays");
        }
    }

    #[test]
    fn a_tracked_run_takes_the_slot_and_the_last_line_counts_its_agents() {
        use talaria_core::hivecode::authoring::AuthoringStage;
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.wall_s = 2_000.0;
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = tool_start(
            &mut state,
            "t1",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}"), "background": true}),
        );
        assert_eq!(stage(&state), Some(AuthoringStage::Running));
        let mut run = summary(talaria_workflow::runner::RunStatus::Running, 0);
        run.started_at_ms = 2_000_500;
        let _ = update(&mut state, AppMessage::HivecodeRuns { rows: vec![run] });
        assert_eq!(stage(&state), None, "the run dock takes over");
        assert!(state.view().workflow_run.is_some());
        state.now_ms = 201_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Started.", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode built the workflow in 3m 20s: review-branch is running (10 agents so far)."
            ]
        );
    }

    #[test]
    fn a_build_stopped_after_starting_the_run_says_the_run_may_go_on() {
        // On a remote gateway, or before the runs folder is read again, Talaria has not found
        // the run the turn started: the line must not say no workflow started.
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = tool_start(
            &mut state,
            "t1",
            "write_file",
            json!({"path": DRAFT, "content": "export const meta = {name:'fix-parser'};"}),
        );
        let _ = tool_start(
            &mut state,
            "t2",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}"), "background": true}),
        );
        state.now_ms = 131_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "", "status": "interrupted"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode stopped: interrupted after starting fix-parser.workflow, 2m 10s in. The workflow may still be running. Hivecode is still on."
            ]
        );
    }

    #[test]
    fn a_routed_prompt_queued_behind_a_building_turn_waits_for_its_own_turn() {
        use talaria_core::hivecode::authoring::AuthoringStage;
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = tool_start(
            &mut state,
            "t1",
            "terminal",
            json!({"command": "talaria workflow roster --profile fixture --json"}),
        );
        state.now_ms = 5_000;
        // The skill is loaded, so the second prompt goes with the reminder and waits in the
        // queue behind the running turn.
        let effects = enter(&mut state, "also cover the lexer");
        assert!(!loads_skill(&effects), "{effects:?}");
        let block = state
            .view()
            .authoring
            .expect("the first turn keeps its block");
        assert_eq!(block.progress.stage, AuthoringStage::Planning);
        assert_eq!(block.elapsed_ms, 4_000);
        state.now_ms = 11_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Which tests matter?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            ["\u{b7} Hivecode finished in 10s without writing a workflow script."]
        );
        // The queued prompt's turn starts and gets its own block and its own line.
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("also cover the lexer")
        );
        assert_eq!(stage(&state), Some(AuthoringStage::Planning));
        state.now_ms = 14_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "", "status": "interrupted"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode stopped: interrupted while planning the workflow after 3s. No workflow started. Hivecode is still on."
            ]
        );
        assert_eq!(stage(&state), None);
    }

    #[test]
    fn an_edited_queued_routed_prompt_keeps_its_building_block() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = frame(&mut state, "message.start", json!({}));
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        let input = sent_id(&effects);
        state.opened.as_mut().unwrap().state.queue.rows = vec![talaria_core::queue::QueuedRow {
            admission_id: talaria_core::queue::AdmissionId("admission-1".into()),
            input_id: talaria_core::queue::SubmissionId(input),
            text: expanded("fix the parser"),
            status: talaria_core::queue::RowStatus::Queued,
            outcome: None,
            controls: talaria_core::queue::Controls {
                steer: true,
                replace: true,
            },
            steering: false,
            sequence: 1,
            execution_generation: Some(1),
        }];
        // Edit the queued prompt before it starts.
        let _ = queue_intent(&mut state, QueueIntent::EnterSelection);
        let _ = queue_intent(&mut state, QueueIntent::BeginEdit);
        let effects = queue_intent(
            &mut state,
            QueueIntent::SaveEdit("fix the parser and the lexer".into()),
        );
        let replaced = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Queue(Effect::ReplaceQueued { submission_id, .. }) => {
                    Some(submission_id.clone())
                }
                _ => None,
            })
            .expect("the edit replaces the row");
        let receipt = serde_json::from_value(json!({"admission_id": "admission-2",
            "authority_epoch": 1, "execution_generation": null, "outcome": null,
            "ref": {"profile_id": "fixture", "session_id": "session"}, "sequence": 2,
            "status": "queued"}))
        .unwrap();
        let _ = update(
            &mut state,
            AppMessage::PromptDone {
                events: vec![Event::QueueCallResult {
                    action: QueueAction::Edit,
                    admission_id: Some(talaria_core::queue::AdmissionId("admission-1".into())),
                    submission_id: Some(replaced),
                    result: receipt,
                }],
                notices: Vec::new(),
                masked: None,
            },
        );
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        // The edited row starts.
        let raw = {
            let opened = state.opened.as_ref().unwrap();
            json!({"session_id": opened.session_id, "replay_epoch": opened.cursor.replay_epoch,
                "seq": opened.cursor.last_sequence + 1, "type": "message.start",
                "admission_id": "admission-2", "payload": {}})
        };
        let _ = update(
            &mut state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(hermes_client::channel::InboundEvent {
                    generation: hermes_client::channel::Generation(1),
                    frame: EventFrame::from_params(&raw).unwrap(),
                    raw,
                }),
                replayed: false,
            }),
        );
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("fix the parser and the lexer")
        );
        assert!(
            stage(&state).is_some(),
            "the edited prompt's turn shows the block"
        );
        state.now_ms = 11_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Which tests matter?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            ["\u{b7} Hivecode finished in 10s without writing a workflow script."]
        );
    }

    #[test]
    fn editing_the_second_of_two_identical_queued_routed_prompts_keeps_both_blocks() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = frame(&mut state, "message.start", json!({}));
        state.now_ms = 1_000;
        let mut inputs = vec![];
        for _ in 0..2 {
            let _ = enter(&mut state, "fix the parser");
            inputs.push(sent_id(&dispatched(&mut state, "fix the parser")));
        }
        let row = |admission: &str, input: &str, sequence: i64| talaria_core::queue::QueuedRow {
            admission_id: talaria_core::queue::AdmissionId(admission.into()),
            input_id: talaria_core::queue::SubmissionId(input.into()),
            text: expanded("fix the parser"),
            status: talaria_core::queue::RowStatus::Queued,
            outcome: None,
            controls: talaria_core::queue::Controls {
                steer: true,
                replace: true,
            },
            steering: false,
            sequence,
            execution_generation: Some(1),
        };
        state.opened.as_mut().unwrap().state.queue.rows = vec![
            row("admission-1", &inputs[0], 1),
            row("admission-2", &inputs[1], 2),
        ];
        // Edit the newest queued copy.
        let _ = queue_intent(&mut state, QueueIntent::EnterSelection);
        let _ = queue_intent(&mut state, QueueIntent::BeginEdit);
        let effects = queue_intent(
            &mut state,
            QueueIntent::SaveEdit("fix the parser and the lexer".into()),
        );
        let (replaced, old) = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Queue(Effect::ReplaceQueued {
                    submission_id,
                    admission_id,
                    ..
                }) => Some((submission_id.clone(), admission_id.0.clone())),
                _ => None,
            })
            .expect("the edit replaces the row");
        assert_eq!(old, "admission-2");
        let receipt = serde_json::from_value(json!({"admission_id": "admission-3",
            "authority_epoch": 1, "execution_generation": null, "outcome": null,
            "ref": {"profile_id": "fixture", "session_id": "session"}, "sequence": 3,
            "status": "queued"}))
        .unwrap();
        let _ = update(
            &mut state,
            AppMessage::PromptDone {
                events: vec![Event::QueueCallResult {
                    action: QueueAction::Edit,
                    admission_id: Some(talaria_core::queue::AdmissionId("admission-2".into())),
                    submission_id: Some(replaced),
                    result: receipt,
                }],
                notices: Vec::new(),
                masked: None,
            },
        );
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        let start = |state: &mut AppState, admission: &str| {
            let raw = {
                let opened = state.opened.as_ref().unwrap();
                json!({"session_id": opened.session_id, "replay_epoch": opened.cursor.replay_epoch,
                    "seq": opened.cursor.last_sequence + 1, "type": "message.start",
                    "admission_id": admission, "payload": {}})
            };
            let _ = update(
                state,
                AppMessage::Link(LinkEvent::Inbound {
                    item: Inbound::Event(hermes_client::channel::InboundEvent {
                        generation: hermes_client::channel::Generation(1),
                        frame: EventFrame::from_params(&raw).unwrap(),
                        raw,
                    }),
                    replayed: false,
                }),
            );
        };
        // The unedited copy runs first and shows its block and its line.
        start(&mut state, "admission-1");
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("fix the parser")
        );
        assert!(stage(&state).is_some(), "the first copy shows the block");
        state.now_ms = 11_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Which tests matter?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            ["\u{b7} Hivecode finished in 10s without writing a workflow script."]
        );
        // Then the edited prompt runs and keeps its own block and line.
        start(&mut state, "admission-3");
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("fix the parser and the lexer")
        );
        assert!(stage(&state).is_some(), "the edited prompt shows the block");
        state.now_ms = 14_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Which tests matter?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            ["\u{b7} Hivecode finished in 3s without writing a workflow script."]
        );
    }

    #[test]
    fn a_refused_routed_prompt_leaves_no_block_for_a_later_turn() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        let _ = update(
            &mut state,
            AppMessage::SubmitDone {
                submission_id: sent_id(&effects),
                outcome: ExecuteOutcome::Refused {
                    reason: "invalid_params".into(),
                },
            },
        );
        // A turn Hermes starts on its own draws no prompt cell, so the refused prompt's cell is
        // still the newest one; it must not take that turn.
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(stage(&state), None);
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Done.", "status": "complete"}),
        );
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    /// The submission id of the prompt a routed prompt's dispatch sent.
    fn sent_id(effects: &[AppEffect]) -> String {
        effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Send(Effect::SendPrompt { submission_id, .. }) => {
                    Some(submission_id.clone())
                }
                _ => None,
            })
            .expect("the prompt is sent")
    }

    #[test]
    fn a_refused_routed_prompt_does_not_take_a_plain_turn_after_hivecode_is_off() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        let _ = update(
            &mut state,
            AppMessage::SubmitDone {
                submission_id: sent_id(&effects),
                outcome: ExecuteOutcome::Refused {
                    reason: "invalid_params".into(),
                },
            },
        );
        // The refused prompt's cell stays; Hivecode is turned off and a plain prompt sent.
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("fix the parser")
        );
        let _ = submit(&mut state, "/hivecode off".into());
        let _ = enter(&mut state, "hello");
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(user_cells(&state).last().map(String::as_str), Some("hello"));
        assert_eq!(stage(&state), None);
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Hi.", "status": "complete"}),
        );
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn the_answer_to_an_approval_question_goes_on_from_the_written_script() {
        use talaria_core::hivecode::authoring::AuthoringStage;
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.now_ms = 1_000;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = tool_start(
            &mut state,
            "t1",
            "write_file",
            json!({"path": DRAFT, "content": "export const meta = {name:'fix-parser'};"}),
        );
        state.now_ms = 91_000;
        // The agent asks in its reply, which ends the turn.
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Here is the proposed workflow. Start it?", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            [
                "\u{b7} Hivecode wrote the workflow script fix-parser.workflow in 1m 30s and is waiting for your approval to start it."
            ]
        );
        // The answer's turn starts at the answer, with the script named and the time going on.
        state.now_ms = 300_000;
        let _ = enter(&mut state, "yes");
        let _ = frame(&mut state, "message.start", json!({}));
        let block = state
            .view()
            .authoring
            .expect("the answer's turn shows the block");
        assert_eq!(block.progress.stage, AuthoringStage::Answered);
        assert_eq!(block.progress.label(), "reading your answer");
        assert_eq!(
            block.progress.script.as_deref(),
            Some("fix-parser.workflow")
        );
        assert_eq!(block.elapsed_ms, 90_000);
        let _ = tool_start(
            &mut state,
            "t2",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}"), "background": true}),
        );
        assert_eq!(stage(&state), Some(AuthoringStage::Running));
        state.now_ms = 308_000;
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Started.", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            ["\u{b7} Hivecode built the workflow in 1m 38s and started fix-parser.workflow."]
        );
        // Nothing is carried past a turn that started the run.
        let _ = enter(&mut state, "and the lexer");
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(stage(&state), Some(AuthoringStage::Planning));
    }

    #[test]
    fn a_plain_turn_shows_no_building_block() {
        let mut state = opened("session");
        let _ = enter(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        assert_eq!(stage(&state), None);
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Done.", "status": "complete"}),
        );
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn workflow_prompts_show_what_was_typed_and_load_the_skill_once() {
        let reminder = talaria_core::hivecode::follow_up_text;
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let mut screen = vec![];

        // The first prompt goes to Hermes for the whole skill.
        let effects = enter(&mut state, "fix the parser");
        assert!(loads_skill(&effects), "{effects:?}");
        assert!(lines(&effects).is_empty(), "{effects:?}");
        assert!(state.draft.is_empty());
        screen.extend(lines(&effects));
        let effects = dispatched(&mut state, "fix the parser");
        assert_eq!(sent(&effects), [expanded("fix the parser")]);
        assert_eq!(user_cells(&state), ["fix the parser"]);
        screen.extend(lines(&effects));
        screen.extend(lines(&turn(&mut state, "complete")));

        // A short reply and a new task carry the reminder, not the skill.
        for typed in ["go ahead", "now add a test for the lexer"] {
            let effects = enter(&mut state, typed);
            assert!(!loads_skill(&effects), "{effects:?}");
            assert_eq!(sent(&effects), [reminder(typed)]);
            assert!(lines(&effects).is_empty(), "{effects:?}");
            assert!(state.draft.is_empty(), "the composer clears");
            assert_eq!(user_cells(&state).last().map(String::as_str), Some(typed));
            screen.extend(lines(&effects));
            screen.extend(lines(&turn(&mut state, "complete")));
        }

        // An interrupted routed turn leaves the skill in the conversation.
        let effects = enter(&mut state, "and the docs");
        assert_eq!(sent(&effects), [reminder("and the docs")]);
        screen.extend(lines(&turn(&mut state, "interrupted")));
        let effects = enter(&mut state, "keep going");
        assert_eq!(sent(&effects), [reminder("keep going")]);
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("keep going")
        );
        screen.extend(lines(&effects));

        // A prompt sent while that turn runs waits, then shows what was typed.
        screen.extend(lines(&frame(&mut state, "message.start", json!({}))));
        let effects = enter(&mut state, "then tidy the docs");
        assert_eq!(sent(&effects), [reminder("then tidy the docs")]);
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("keep going")
        );
        screen.extend(lines(&effects));
        screen.extend(lines(&frame(
            &mut state,
            "message.complete",
            json!({"text": "ok"}),
        )));
        screen.extend(lines(&turn(&mut state, "complete")));
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("then tidy the docs")
        );

        // After a compaction the next routed prompt loads the whole skill again.
        screen.extend(lines(&frame(
            &mut state,
            "session.usage",
            json!({"usage": {"compressions": 1}}),
        )));
        let effects = enter(&mut state, "go on");
        assert!(loads_skill(&effects), "{effects:?}");
        assert!(sent(&effects).is_empty());

        let mut shown = screen;
        shown.extend(user_cells(&state));
        for line in &shown {
            for hidden in [
                "/talaria-workflows",
                "[IMPORTANT",
                "Fixture workflow guide",
                "[Talaria workflow mode",
            ] {
                assert!(
                    !line.contains(hidden),
                    "{hidden:?} reached the screen: {line:?}"
                );
            }
        }
    }

    /// The routed prompt's skill dispatch, answered by Hermes with the request it was sent.
    fn dispatched_line(state: &mut AppState, typed: &str, request: &str) -> Vec<AppEffect> {
        let reply = json!({"type": "skill", "name": "talaria-workflows",
            "message": expanded(request), "display": format!("/talaria-workflows {request}")});
        update(
            state,
            AppMessage::HivecodeDone {
                session: SessionId::new("session"),
                original_text: typed.into(),
                restore_text: None,
                outcome: Ok(reply),
            },
        )
    }

    fn command_line(effects: &[AppEffect]) -> Option<String> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Hivecode {
                action: crate::hivecode::Action::RunCommand { command_line, .. },
                ..
            } => Some(command_line.clone()),
            _ => None,
        })
    }

    #[test]
    fn a_switched_model_goes_with_the_next_routed_prompt() {
        let reminder = talaria_core::hivecode::follow_up_text;
        let mut state = opened("session");
        {
            let opened = state.opened.as_mut().unwrap();
            opened.info.model = Some("model-a".into());
            opened.info.provider = Some("provider-a".into());
        }
        turn_on(&mut state, "session");
        let effects = enter(&mut state, "fix the parser");
        let first = command_line(&effects).expect("the first prompt carries the skill");
        assert!(
            first.ends_with("\nSession model: model-a on provider-a"),
            "{first:?}"
        );
        let request = first.trim_start_matches("/talaria-workflows ").to_owned();
        let _ = dispatched_line(&mut state, "fix the parser", &request);
        let _ = turn(&mut state, "complete");
        // The same pair is not repeated.
        let effects = enter(&mut state, "go ahead");
        assert_eq!(sent(&effects), [reminder("go ahead")]);
        let _ = turn(&mut state, "complete");
        // The user switches the session's model, then sends the next task.
        state.model = Some("model-b".into());
        let effects = enter(&mut state, "now add a test for the lexer");
        assert_eq!(
            sent(&effects),
            [format!(
                "{}\nSession model: model-b on provider-a",
                reminder("now add a test for the lexer")
            )]
        );
        assert_eq!(
            user_cells(&state).last().map(String::as_str),
            Some("now add a test for the lexer"),
            "the line never shows on screen"
        );
        let _ = turn(&mut state, "complete");
        let effects = enter(&mut state, "and the docs");
        assert_eq!(sent(&effects), [reminder("and the docs")]);
    }

    #[test]
    fn a_switched_model_whose_prompt_never_ran_goes_with_the_next_routed_prompt() {
        let reminder = talaria_core::hivecode::follow_up_text;
        let with_b =
            |typed: &str| format!("{}\nSession model: model-b on provider-a", reminder(typed));
        let mut state = opened("session");
        {
            let opened = state.opened.as_mut().unwrap();
            opened.info.model = Some("model-a".into());
            opened.info.provider = Some("provider-a".into());
        }
        turn_on(&mut state, "session");
        let effects = enter(&mut state, "fix the parser");
        let first = command_line(&effects).expect("the first prompt carries the skill");
        let request = first.trim_start_matches("/talaria-workflows ").to_owned();
        let _ = dispatched_line(&mut state, "fix the parser", &request);
        let _ = turn(&mut state, "complete");
        // The user switches model; the prompt that names it is refused by Hermes.
        state.model = Some("model-b".into());
        let effects = enter(&mut state, "add a lexer test");
        assert_eq!(sent(&effects), [with_b("add a lexer test")]);
        let _ = update(
            &mut state,
            AppMessage::SubmitDone {
                submission_id: sent_id(&effects),
                outcome: ExecuteOutcome::Refused {
                    reason: "invalid_params".into(),
                },
            },
        );
        let effects = enter(&mut state, "add a parser test");
        assert_eq!(sent(&effects), [with_b("add a parser test")]);
        // The next one waits in the queue behind a running turn, and is removed from it.
        let _ = turn(&mut state, "complete");
        state.model = Some("model-c".into());
        let _ = frame(&mut state, "message.start", json!({}));
        let effects = enter(&mut state, "add a docs test");
        let line =
            |typed: &str| format!("{}\nSession model: model-c on provider-a", reminder(typed));
        assert_eq!(sent(&effects), [line("add a docs test")]);
        state.opened.as_mut().unwrap().state.queue.rows = vec![talaria_core::queue::QueuedRow {
            admission_id: talaria_core::queue::AdmissionId("admission-1".into()),
            input_id: talaria_core::queue::SubmissionId(sent_id(&effects)),
            text: line("add a docs test"),
            status: talaria_core::queue::RowStatus::Queued,
            outcome: None,
            controls: talaria_core::queue::Controls {
                steer: true,
                replace: true,
            },
            steering: false,
            sequence: 1,
            execution_generation: Some(1),
        }];
        // While it waits, a routed prompt names the model again: the first may yet be removed.
        let effects = enter(&mut state, "add a config test");
        assert_eq!(sent(&effects), [line("add a config test")]);
        let receipt = serde_json::from_value(json!({"admission_id": "admission-1",
            "authority_epoch": 1, "execution_generation": null, "outcome": "cancelled",
            "ref": {"profile_id": "fixture", "session_id": "session"}, "sequence": 1,
            "status": "terminal"}))
        .unwrap();
        let _ = update(
            &mut state,
            AppMessage::PromptDone {
                events: vec![Event::QueueCallResult {
                    action: QueueAction::Remove,
                    admission_id: Some(talaria_core::queue::AdmissionId("admission-1".into())),
                    submission_id: None,
                    result: receipt,
                }],
                notices: Vec::new(),
                masked: None,
            },
        );
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        let effects = enter(&mut state, "add a cli test");
        assert_eq!(sent(&effects), [line("add a cli test")]);
    }

    #[test]
    fn a_provider_learned_later_goes_with_the_next_routed_prompt() {
        let reminder = talaria_core::hivecode::follow_up_text;
        let mut state = opened("session");
        state.opened.as_mut().unwrap().info.model = Some("model-a".into());
        turn_on(&mut state, "session");
        let effects = enter(&mut state, "fix the parser");
        let first = command_line(&effects).expect("the first prompt carries the skill");
        assert!(!first.contains("Session model"), "{first:?}");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = turn(&mut state, "complete");
        state.opened.as_mut().unwrap().info.provider = Some("provider-a".into());
        let effects = enter(&mut state, "go ahead");
        assert_eq!(
            sent(&effects),
            [format!(
                "{}\nSession model: model-a on provider-a",
                reminder("go ahead")
            )]
        );
    }

    fn scans(effects: &[AppEffect]) -> Vec<i64> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::HivecodeScan { since_ms } => Some(*since_ms),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_reply_after_the_first_hour_still_watches_for_the_run_it_starts() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.wall_s = 1_000.0;
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = turn(&mut state, "complete");
        // An hour on, the watch ends without seeing a run.
        let late = 61 * 60 * 1_000;
        let effects = update(&mut state, AppMessage::Tick { now_ms: late });
        assert_eq!(scans(&effects), [1_000_000]);
        let _ = update(&mut state, AppMessage::HivecodeRuns { rows: vec![] });
        let effects = update(
            &mut state,
            AppMessage::Tick {
                now_ms: late + 5_000,
            },
        );
        assert!(scans(&effects).is_empty(), "the watch ended: {effects:?}");
        // The answer goes with the reminder, and watches for the run it starts.
        state.wall_s = 5_000.0;
        let effects = enter(&mut state, "go ahead");
        assert_eq!(
            sent(&effects),
            [talaria_core::hivecode::follow_up_text("go ahead")]
        );
        let effects = update(
            &mut state,
            AppMessage::Tick {
                now_ms: late + 6_000,
            },
        );
        assert_eq!(scans(&effects), [5_000_000]);
        let mut run = summary(talaria_workflow::runner::RunStatus::Running, 0);
        run.started_at_ms = 5_100_000;
        let effects = update(&mut state, AppMessage::HivecodeRuns { rows: vec![run] });
        assert_eq!(
            lines(&effects),
            [format!(
                "\u{b7} {}",
                notice::run_started("review-branch", "run-0001", None)
            )]
        );
    }

    #[test]
    fn a_first_prompt_that_never_starts_leaves_the_skill_to_the_next_one() {
        // Queued behind a running turn, then removed from the queue.
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        assert_eq!(sent(&effects), [expanded("fix the parser")]);
        // The first one still waits, so the second carries the skill too.
        let second = enter(&mut state, "fix the tokenizer");
        assert!(loads_skill(&second), "{second:?}");
        state.opened.as_mut().unwrap().state.queue.rows = vec![talaria_core::queue::QueuedRow {
            admission_id: talaria_core::queue::AdmissionId("admission-1".into()),
            input_id: talaria_core::queue::SubmissionId("input-1".into()),
            text: expanded("fix the parser"),
            status: talaria_core::queue::RowStatus::Queued,
            outcome: None,
            controls: talaria_core::queue::Controls {
                steer: true,
                replace: true,
            },
            steering: false,
            sequence: 1,
            execution_generation: Some(1),
        }];
        let cancelled = serde_json::from_value(json!({"admission_id": "admission-1",
            "authority_epoch": 1, "execution_generation": null, "outcome": "cancelled",
            "ref": {"profile_id": "fixture", "session_id": "session"}, "sequence": 1,
            "status": "terminal"}))
        .unwrap();
        let effects = update(
            &mut state,
            AppMessage::PromptDone {
                events: vec![Event::QueueCallResult {
                    action: QueueAction::Remove,
                    admission_id: Some(talaria_core::queue::AdmissionId("admission-1".into())),
                    submission_id: None,
                    result: cancelled,
                }],
                notices: Vec::new(),
                masked: None,
            },
        );
        assert!(
            effects.iter().any(
                |effect| matches!(effect, AppEffect::OfferDraft(text) if text == "fix the parser")
            ),
            "{effects:?}"
        );
        let effects = enter(&mut state, "fix the lexer");
        assert!(loads_skill(&effects), "{effects:?}");

        // Refused by Hermes.
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        let id = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Send(Effect::SendPrompt { submission_id, .. }) => {
                    Some(submission_id.clone())
                }
                _ => None,
            })
            .expect("the whole skill is sent");
        let _ = update(
            &mut state,
            AppMessage::SubmitDone {
                submission_id: id,
                outcome: ExecuteOutcome::Refused {
                    reason: "invalid_params".into(),
                },
            },
        );
        let effects = enter(&mut state, "fix the lexer");
        assert!(loads_skill(&effects), "{effects:?}");
    }

    #[test]
    fn a_routed_prompt_behind_a_waiting_skill_prompt_carries_the_skill_too() {
        // The prompt that carries the skill waits in the queue, so a removal could still take
        // the skill out of the conversation: the next routed prompt carries it as well.
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = enter(&mut state, "fix the parser");
        let effects = dispatched(&mut state, "fix the parser");
        assert_eq!(sent(&effects), [expanded("fix the parser")]);
        let effects = enter(&mut state, "fix the tokenizer");
        assert!(loads_skill(&effects), "{effects:?}");
        let effects = dispatched(&mut state, "fix the tokenizer");
        assert_eq!(sent(&effects), [expanded("fix the tokenizer")]);
        // Once the first one starts, the skill is in the conversation.
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "ok", "status": "complete"}),
        );
        let effects = enter(&mut state, "fix the lexer");
        assert_eq!(
            sent(&effects),
            [talaria_core::hivecode::follow_up_text("fix the lexer")]
        );
    }

    #[test]
    fn branch_child_inherits_hivecode() {
        let mut state = opened("parent");
        turn_on(&mut state, "parent");
        let install = SwitchInstall {
            opened: session("child", "stored-child"),
            service: SessionService::new("launch"),
            link: None,
        };
        let effects = branched(
            &mut state,
            BranchOutcome::Branched {
                notice: "Branched into the fixture.".into(),
            },
            Some(install),
        );
        assert!(state.hivecode.status_input(Some(&SessionId::new("child"))));
        let shown = lines(&effects);
        let branch = shown
            .iter()
            .position(|line| line.contains("Branched into the fixture."))
            .expect("branch notice");
        let carried = shown
            .iter()
            .position(|line| line == &format!("\u{b7} {}", notice::BRANCH_CARRIED))
            .unwrap_or_else(|| panic!("carried notice: {shown:?}"));
        assert!(carried > branch, "{shown:?}");

        let mut off = opened("parent");
        let install = SwitchInstall {
            opened: session("child", "stored-child"),
            service: SessionService::new("launch"),
            link: None,
        };
        let effects = branched(
            &mut off,
            BranchOutcome::Branched {
                notice: "Branched into the fixture.".into(),
            },
            Some(install),
        );
        assert!(!off.hivecode.status_input(Some(&SessionId::new("child"))));
        assert!(!lines(&effects).iter().any(|line| line.contains("Hivecode")));
    }

    #[test]
    fn reopen_with_same_stored_id_keeps_hivecode() {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        state.opened = Some(session("runtime-1", "stored-1"));
        turn_on(&mut state, "runtime-1");
        let _ = opened_session(&mut state, session("runtime-2", "stored-1"), None);
        assert!(
            state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-2")))
        );
        let _ = opened_session(&mut state, session("runtime-3", "stored-other"), None);
        assert!(
            !state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-3"))),
            "another stored session keeps its own setting"
        );
    }

    /// `/hivecode on` typed in the composer, answered by a catalogue that lists the skill.
    fn type_hivecode_on(state: &mut AppState) -> Vec<AppEffect> {
        let mut effects = submit(state, "/hivecode on".into());
        let (request_id, session) = effects
            .iter()
            .find_map(|effect| match effect {
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::FetchCatalog { session, .. },
                    request_id,
                } => Some((*request_id, session.clone())),
                _ => None,
            })
            .expect("the catalogue is read");
        let result: talaria_core::commands::CatalogResult = serde_json::from_value(json!({
            "skills": {"/talaria-workflows": {"usage": 0, "origin": "local"}},
            "pairs": []
        }))
        .unwrap();
        effects.extend(update(
            state,
            AppMessage::HivecodeCatalog {
                request_id,
                session,
                profile: "fixture".into(),
                command: HivecodeCommand::On,
                result: Ok(talaria_core::commands::CommandCatalog::from_result(&result)),
            },
        ));
        effects
    }

    /// The saved switch file after the saves `effects` asked for, starting from an empty file.
    fn saved(effects: &[AppEffect]) -> Option<talaria_core::hivecode::SavedModes> {
        let mut file = None;
        for effect in effects {
            if let AppEffect::HivecodeSave { changes, .. } = effect {
                file.get_or_insert_with(talaria_core::hivecode::SavedModes::default)
                    .merge(changes);
            }
        }
        file
    }

    fn checks_setup(effects: &[AppEffect]) -> bool {
        effects.iter().any(|effect| {
            matches!(
                effect,
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::CheckSetup { .. },
                    ..
                }
            )
        })
    }

    fn opened_line() -> String {
        format!("\u{b7} {}", notice::OPENED_ON)
    }

    #[test]
    fn hivecode_survives_return_from_another_session() {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        let _ = opened_session(&mut state, session("runtime-a1", "stored-s"), None);
        let _ = type_hivecode_on(&mut state);
        // A workflow agent's session opened from the workflow view.
        let effects = opened_session(&mut state, session("runtime-w1", "stored-w"), None);
        assert!(
            !state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-w1")))
        );
        assert!(!lines(&effects).contains(&opened_line()), "{effects:?}");
        // Back to the first session, which Hermes reopens under a new runtime id.
        let effects = opened_session(&mut state, session("runtime-b2", "stored-s"), None);
        assert!(
            state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-b2")))
        );
        assert_eq!(
            lines(&effects)
                .iter()
                .filter(|line| **line == opened_line())
                .count(),
            1,
            "{effects:?}"
        );
        assert!(checks_setup(&effects), "a reopen checks the setup");
        let effects = enter(&mut state, "fix the parser");
        assert!(
            effects.iter().any(|effect| matches!(effect,
                AppEffect::Hivecode { action: crate::hivecode::Action::RunCommand { session, .. }, .. }
                    if session.as_str() == "runtime-b2")),
            "{effects:?}"
        );
        assert!(checks_setup(&effects), "a routed prompt checks the setup");
        // A plain reconnect of the same runtime session repeats nothing.
        let effects = opened_session(&mut state, session("runtime-b2", "stored-s"), None);
        assert!(!lines(&effects).contains(&opened_line()), "{effects:?}");
    }

    #[test]
    fn hivecode_is_restored_after_a_restart() {
        let mut first = AppState::new("launch");
        first.profile = "fixture".into();
        let _ = opened_session(&mut first, session("runtime-a1", "stored-s"), None);
        let effects = type_hivecode_on(&mut first);
        assert!(checks_setup(&effects), "turning on checks the setup");
        let text = saved(&effects).expect("turning on saves the switch");

        // `talaria --resume` in a new process opens the stored session under a new runtime id.
        let mut second = AppState::new("launch");
        second.profile = "fixture".into();
        second.hivecode.restore(text);
        let effects = opened_session(&mut second, session("runtime-r2", "stored-s"), None);
        assert!(lines(&effects).contains(&opened_line()), "{effects:?}");
        let effects = enter(&mut second, "lets continue the workflow");
        assert!(loads_skill(&effects), "{effects:?}");

        // Off is remembered the same way.
        let effects = submit(&mut second, "/hivecode off".into());
        let text = saved(&effects).expect("turning off saves the switch");
        let mut third = AppState::new("launch");
        third.profile = "fixture".into();
        third.hivecode.restore(text);
        let effects = opened_session(&mut third, session("runtime-r3", "stored-s"), None);
        assert!(
            !third
                .hivecode
                .status_input(Some(&SessionId::new("runtime-r3")))
        );
        assert!(!lines(&effects).contains(&opened_line()), "{effects:?}");
    }

    #[test]
    fn the_launch_flag_turns_on_the_session_it_opens_even_when_saved_off() {
        let text = talaria_core::hivecode::SavedModes::from_json(
            r#"{"version":1,"sessions":{"stored-s":{"on":false,"seen_ms":0},"stored-t":{"on":false,"seen_ms":0}}}"#,
        )
        .unwrap();
        // `talaria --hivecode --resume stored-s`.
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        state.hivecode.restore(text);
        let effects = opened_session(&mut state, session("runtime-1", "stored-s"), None);
        assert!(
            state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-1")))
        );
        assert!(lines(&effects).contains(&opened_line()), "{effects:?}");
        let file = saved(&effects).expect("the launch's choice is saved");
        assert!(
            file.to_json().contains("\"on\": true"),
            "{}",
            file.to_json()
        );
        // Only the session the launch opened: another session saved as off keeps its setting.
        let _ = opened_session(&mut state, session("runtime-2", "stored-t"), None);
        assert!(
            !state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-2")))
        );
    }

    #[test]
    fn a_session_compacted_while_away_keeps_hivecode() {
        // Back under the same runtime id, after Hermes compacted it with no frame seen.
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        let _ = opened_session(&mut state, session("runtime-a1", "stored-s"), None);
        let _ = type_hivecode_on(&mut state);
        let _ = opened_session(&mut state, session("runtime-w1", "stored-w"), None);
        let effects = opened_session(&mut state, session("runtime-a1", "stored-tip"), None);
        assert!(
            state
                .hivecode
                .status_input(Some(&SessionId::new("runtime-a1")))
        );
        assert!(lines(&effects).contains(&opened_line()), "{effects:?}");
        assert!(saved(&effects).is_some_and(|file| file.to_json().contains("stored-tip")));

        // Back under a new runtime id: a resume by id answers with the id Talaria asked for,
        // here the stored id saved before the restart, and names the compaction's tip.
        let mut restarted = AppState::new("launch");
        restarted.profile = "fixture".into();
        restarted.hivecode.restore(
            talaria_core::hivecode::SavedModes::from_json(
                r#"{"version":1,"sessions":{"stored-s":{"on":true,"seen_ms":0}}}"#,
            )
            .unwrap(),
        );
        let effects = opened_session(&mut restarted, session("stored-s", "stored-tip"), None);
        assert!(
            restarted
                .hivecode
                .status_input(Some(&SessionId::new("stored-s")))
        );
        assert!(lines(&effects).contains(&opened_line()), "{effects:?}");
    }

    #[test]
    fn a_compaction_in_the_background_moves_hivecode() {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        let _ = opened_session(&mut state, session("runtime-a1", "stored-s"), None);
        let _ = type_hivecode_on(&mut state);
        let _ = opened_session(&mut state, session("runtime-w1", "stored-w"), None);
        // A frame for the retained first session, while the agent's session is in front.
        let raw = json!({"session_id": "runtime-a1", "replay_epoch": "epoch", "seq": 1,
            "type": "session.info", "payload": {"stored_session_id": "stored-tip"}});
        let effects = update(
            &mut state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(hermes_client::channel::InboundEvent {
                    generation: hermes_client::channel::Generation(1),
                    frame: EventFrame::from_params(&raw).unwrap(),
                    raw,
                }),
                replayed: false,
            }),
        );
        let file = saved(&effects).expect("the move is saved");
        let mut restarted = AppState::new("launch");
        restarted.profile = "fixture".into();
        restarted.hivecode.restore(file);
        let _ = opened_session(&mut restarted, session("runtime-2", "stored-tip"), None);
        assert!(
            restarted
                .hivecode
                .status_input(Some(&SessionId::new("runtime-2")))
        );
    }

    #[test]
    fn compaction_moves_hivecode_to_the_new_stored_id() {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        let _ = opened_session(&mut state, session("session", "stored-before"), None);
        let _ = type_hivecode_on(&mut state);
        let effects = frame(
            &mut state,
            "session.info",
            json!({"stored_session_id": "stored-after"}),
        );
        let text = saved(&effects).expect("the new stored id is saved");
        let mut restarted = AppState::new("launch");
        restarted.profile = "fixture".into();
        restarted.hivecode.restore(text);
        let _ = opened_session(&mut restarted, session("runtime-2", "stored-after"), None);
        assert!(
            restarted
                .hivecode
                .status_input(Some(&SessionId::new("runtime-2")))
        );
    }

    #[test]
    fn a_branch_says_hivecode_once() {
        let mut state = opened("parent");
        turn_on(&mut state, "parent");
        let install = SwitchInstall {
            opened: session("child", "stored-child"),
            service: SessionService::new("launch"),
            link: None,
        };
        let effects = branched(
            &mut state,
            BranchOutcome::Branched {
                notice: "Branched into the fixture.".into(),
            },
            Some(install),
        );
        let shown = lines(&effects);
        assert_eq!(
            shown
                .iter()
                .filter(|line| line.contains("Hivecode"))
                .count(),
            1,
            "{shown:?}"
        );
        assert!(saved(&effects).is_some_and(|file| file.to_json().contains("stored-child")));
    }

    #[test]
    fn interrupting_a_routed_turn_says_hivecode_is_still_on() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let effects = interrupt_done(&mut state, ExecuteOutcome::Sent, Vec::new());
        assert_eq!(
            lines(&effects),
            [format!("\u{b7} {}", notice::STILL_ON)],
            "{effects:?}"
        );
        let mut off = opened("session");
        let effects = interrupt_done(&mut off, ExecuteOutcome::Sent, Vec::new());
        assert!(lines(&effects).is_empty(), "{effects:?}");
    }

    #[test]
    fn a_remote_gateway_skips_the_setup_check() {
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        state.hivecode.set_remote(true);
        let _ = opened_session(&mut state, session("runtime-a1", "stored-s"), None);
        let effects = type_hivecode_on(&mut state);
        assert!(!checks_setup(&effects), "{effects:?}");
        let effects = enter(&mut state, "fix the parser");
        assert!(!checks_setup(&effects), "{effects:?}");
    }

    #[test]
    fn a_remote_session_opened_with_hivecode_on_says_runs_are_elsewhere() {
        // `talaria --remote <host> --hivecode --resume stored-s`.
        let mut state = AppState::new("launch");
        state.profile = "fixture".into();
        state.hivecode = crate::hivecode::HivecodeService::new(true);
        state.hivecode.set_remote(true);
        let shown = lines(&opened_session(
            &mut state,
            session("runtime-1", "stored-s"),
            None,
        ));
        assert_eq!(shown, [format!("\u{b7} {}", notice::OPENED_ON_REMOTE)]);
        assert!(shown[0].contains("gateway's machine"), "{shown:#?}");
        // The line a routed turn leaves once it started a run says so again.
        let _ = enter(&mut state, "fix the parser");
        let reply = json!({"type": "skill", "name": "talaria-workflows",
            "message": expanded("fix the parser"), "display": "/talaria-workflows fix the parser"});
        let _ = update(
            &mut state,
            AppMessage::HivecodeDone {
                session: SessionId::new("runtime-1"),
                original_text: "fix the parser".into(),
                restore_text: None,
                outcome: Ok(reply),
            },
        );
        let _ = frame(&mut state, "message.start", json!({}));
        let _ = tool_start(
            &mut state,
            "t1",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}")}),
        );
        let effects = frame(
            &mut state,
            "message.complete",
            json!({"text": "Started.", "status": "complete"}),
        );
        assert_eq!(
            lines(&effects),
            [format!(
                "\u{b7} Hivecode built the workflow in 0s and started the run. {}",
                notice::REMOTE_RUN_TAIL
            )]
        );
    }

    #[test]
    fn a_profiles_harmful_settings_are_looked_up_once_per_run() {
        let mut state = opened("session");
        state.profile = "fixture".into();
        state.config_checked.clear();
        state.hermes_home = PathBuf::new();
        assert!(
            config_check_effect(&mut state).is_none(),
            "no home, no lookup"
        );
        state.hermes_home = PathBuf::from("hermes-home");
        assert!(matches!(
            config_check_effect(&mut state),
            Some(AppEffect::ConfigCheck { profile, .. }) if profile == "fixture"
        ));
        assert!(
            config_check_effect(&mut state).is_none(),
            "the same profile is not asked twice"
        );
        state.profile = "other".into();
        assert!(
            config_check_effect(&mut state).is_some(),
            "another profile is asked once"
        );
    }

    #[test]
    fn each_harmful_setting_shows_one_short_notice() {
        use talaria_core::config_check::Finding;
        let mut state = opened("session");
        state.profile = "fixture".into();
        let findings = vec![
            Finding::CompressionNotInPlace,
            Finding::StepCap { turns: 60 },
        ];
        let shown = lines(&update(
            &mut state,
            AppMessage::ConfigFindings {
                profile: "fixture".into(),
                findings: findings.clone(),
            },
        ));
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert!(shown[0].contains("compression.in_place") && shown[0].contains("context overflow"));
        assert!(shown[1].contains("agent.max_turns") && shown[1].contains("60 steps"));
        let none = update(
            &mut state,
            AppMessage::ConfigFindings {
                profile: "fixture".into(),
                findings: Vec::new(),
            },
        );
        assert!(lines(&none).is_empty());
        let stale = update(
            &mut state,
            AppMessage::ConfigFindings {
                profile: "elsewhere".into(),
                findings,
            },
        );
        assert!(
            lines(&stale).is_empty(),
            "a reply for another profile is dropped"
        );
    }

    fn setup(state: &mut AppState, skill: crate::hivecode::SkillCheck) -> Vec<AppEffect> {
        update(
            state,
            AppMessage::HivecodeSetup {
                session: SessionId::new("session"),
                profile: "fixture".into(),
                facts: crate::hivecode::SetupFacts {
                    skill,
                    tiers: crate::hivecode::TierCheck::Present,
                    missing_builtins: Vec::new(),
                },
            },
        )
    }

    fn reinstalls(effects: &[AppEffect]) -> bool {
        effects.iter().any(|effect| {
            matches!(
                effect,
                AppEffect::Hivecode {
                    action: crate::hivecode::Action::ReinstallSkill { .. },
                    ..
                }
            )
        })
    }

    #[test]
    fn a_changed_skill_never_takes_the_answer_meant_for_the_agent() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = frame(&mut state, "message.start", json!({}));
        // The setup read the routed prompt started comes back while the turn runs.
        let shown = lines(&setup(
            &mut state,
            crate::hivecode::SkillCheck::Modified {
                changes: vec!["changed:SKILL.md".into()],
                digest: "digest-1".into(),
            },
        ));
        assert!(
            shown
                .iter()
                .any(|line| line.contains("/hivecode reinstall")),
            "{shown:#?}"
        );
        assert!(
            !shown.iter().any(|line| line.contains("[y/N]")),
            "{shown:#?}"
        );
        let _ = frame(
            &mut state,
            "message.complete",
            json!({"text": "Here is the proposed workflow. Start it?", "status": "complete"}),
        );
        // The yes goes to the agent.
        let effects = enter(&mut state, "yes");
        assert_eq!(
            sent(&effects),
            [talaria_core::hivecode::follow_up_text("yes")]
        );
        assert!(!reinstalls(&effects), "{effects:?}");
        // So does an empty line or a no: nothing waits for an answer.
        assert!(!reinstalls(&enter(&mut state, "n")));
    }

    #[test]
    fn hivecode_reinstall_reinstalls_the_skill_and_sends_it_whole_next_time() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        let _ = enter(&mut state, "fix the parser");
        let _ = dispatched(&mut state, "fix the parser");
        let _ = turn(&mut state, "complete");
        let effects = submit(&mut state, "/hivecode reinstall".into());
        assert!(reinstalls(&effects), "{effects:?}");
        let effects = enter(&mut state, "go on");
        assert!(loads_skill(&effects), "{effects:?}");
        // A remote gateway's skill lives on its own machine.
        let mut remote = opened("session");
        remote.hivecode.set_remote(true);
        let effects = submit(&mut remote, "/hivecode reinstall".into());
        assert!(!reinstalls(&effects), "{effects:?}");
        assert!(
            lines(&effects)
                .iter()
                .any(|line| line.contains("talaria workflow skill install --force")),
            "{effects:?}"
        );
    }

    #[tokio::test]
    async fn workflow_tick_schedules_hivecode_scan_only_when_armed() {
        let mut state = opened("session");
        turn_on(&mut state, "session");
        state.wall_s = 2_000.0;
        let idle = update(&mut state, AppMessage::Tick { now_ms: 10 });
        assert!(
            !idle
                .iter()
                .any(|effect| matches!(effect, AppEffect::HivecodeScan { .. }))
        );
        let reply = json!({"type": "skill", "name": "talaria-workflows",
            "message": "fixture skill body", "display": "/talaria-workflows fix it"});
        let _ = update(
            &mut state,
            AppMessage::HivecodeDone {
                session: SessionId::new("session"),
                original_text: "fix it".into(),
                restore_text: None,
                outcome: Ok(reply),
            },
        );
        let armed = update(&mut state, AppMessage::Tick { now_ms: 20 });
        assert!(
            armed
                .iter()
                .any(|effect| matches!(effect, AppEffect::HivecodeScan { .. })),
            "{armed:?}"
        );
        let (tx, _rx) = mpsc::unbounded_channel();
        let executor = Executor::new(tx);
        assert!(
            workflow_wake(&state, &executor).is_none(),
            "a scan is in flight"
        );
        state.workflow_key_label = Some("Alt+W".into());
        let effects = update(
            &mut state,
            AppMessage::HivecodeRuns {
                rows: vec![summary(talaria_workflow::runner::RunStatus::Running, 3)],
            },
        );
        assert_eq!(
            lines(&effects),
            [format!(
                "\u{b7} {}",
                notice::run_started("review-branch", "run-0001", Some("Alt+W"))
            )]
        );
        assert!(workflow_wake(&state, &executor).is_some(), "the next scan");
        assert_eq!(
            state.view().workflow_run.map(|run| run.agents_finished),
            Some(3)
        );
    }

    #[test]
    fn alt_w_opens_tracked_run_or_picker() {
        let mut state = opened("session");
        let effects = update(&mut state, AppMessage::User(UserAction::WorkflowView));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::WorkflowDiscover)),
            "{effects:?}"
        );
        turn_on(&mut state, "session");
        state.wall_s = 1_000.0;
        state
            .hivecode
            .arm(&SessionId::new("session"), "fixture", 1_000_000, 0);
        let _ = state.hivecode.scan_due(0);
        let _ = state.hivecode.on_runs(
            &[summary(talaria_workflow::runner::RunStatus::Running, 1)],
            0,
            None,
        );
        let effects = update(&mut state, AppMessage::User(UserAction::WorkflowView));
        assert!(
            effects.iter().any(|effect| matches!(effect,
                AppEffect::WorkflowOpen(path) if path == Path::new("fixture-runs/run-0001"))),
            "{effects:?}"
        );
    }
}

fn kanban_input(state: &mut AppState, input: talaria_core::kanban::KanbanInput) -> Vec<AppEffect> {
    use talaria_core::kanban::KanbanEffect;
    let Some(board_state) = state.kanban.as_mut() else {
        return vec![];
    };
    let effects = talaria_core::kanban::update(board_state, input);
    let board = board_state.board.clone();
    let close = effects.contains(&KanbanEffect::Close);
    let mapped = effects
        .into_iter()
        .map(|effect| AppEffect::Kanban {
            board: board.clone(),
            effect,
        })
        .collect();
    if close {
        state.kanban = None;
    }
    mapped
}

#[cfg(test)]
mod external_fleet_route_tests {
    use super::{AppEffect, AppState, UserAction, fleet_effect, user_action};
    use talaria_core::fleet::RowKey;
    use talaria_core::fleet::acp::AcpMapping;
    use talaria_core::fleet_view::FleetViewEffect;

    #[test]
    fn external_actions_never_fall_through_to_the_hermes_peek_worker() {
        let key = RowKey {
            profile: "acp:example".into(),
            session_id: "s1".into(),
        };
        let mut state = AppState::new("launch");
        state.acp_views.insert(key.clone(), AcpMapping::default());
        for action in [
            FleetViewEffect::OpenPeek(key.clone()),
            FleetViewEffect::SendReply {
                key: key.clone(),
                text: "hello".into(),
            },
            FleetViewEffect::Stop(key.clone()),
            FleetViewEffect::OpenInPane(key.clone()),
        ] {
            let effects = fleet_effect(&mut state, action);
            assert!(!effects.iter().any(|effect| matches!(
                effect,
                AppEffect::FleetPeek(_) | AppEffect::Switch { .. }
            )));
        }
        let effects = user_action(
            &mut state,
            UserAction::AcpAnswer {
                key,
                responder: "1:7".into(),
                tool_call_id: "tool".into(),
                option_id: "reject".into(),
            },
        );
        assert!(
            !effects.iter().any(|effect| matches!(
                effect,
                AppEffect::FleetAnswer(_) | AppEffect::FleetPeek(_)
            ))
        );
    }
}

const REMOTE_VOICE_NOTICE: &str = "Voice needs a local Hermes gateway for now, because a remote gateway would record and play on its own machine.";

fn client_voice_link_lost(state: &mut AppState, effects: &mut Vec<AppEffect>) {
    if state.uses_client_voice() {
        let _ = state
            .client_voice
            .update(talaria_core::voice::client::Input::Detach);
        effects.push(AppEffect::ClientVoiceClose);
    }
}

fn client_voice_command(state: &mut AppState, argument: &str) -> Vec<AppEffect> {
    use talaria_core::voice::client::{Cutoff, Input};
    use talaria_core::voice::{SpeechMode, VoiceCommand, parse_voice_args};
    match parse_voice_args(argument) {
        Ok(VoiceCommand::On) => {
            state.client_voice_enabled = true;
            let _ = state
                .client_voice
                .update(talaria_core::voice::client::Input::Reopen);
            notice_effect(state, "Client voice on.".into())
        }
        Ok(VoiceCommand::Off) => {
            state.client_voice_enabled = false;
            let mut effects = notice_effect(state, "Client voice off.".into());
            if let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            {
                effects.extend(client_voice_input(
                    state,
                    &session,
                    Input::Cutoff(Cutoff::Stop),
                ));
            }
            effects
        }
        Ok(VoiceCommand::Read(value)) => {
            state.client_voice_config.read_aloud =
                value.unwrap_or(!state.client_voice_config.read_aloud);
            if state.client_voice_config.read_aloud {
                notice_effect(state, "Read-aloud on.".into())
            } else if let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            {
                client_voice_input(state, &session, Input::Cutoff(Cutoff::ReadOff))
            } else {
                notice_effect(state, "Read-aloud stopped.".into())
            }
        }
        Ok(VoiceCommand::Stop) => {
            if let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            {
                client_voice_input(state, &session, Input::Cutoff(Cutoff::Stop))
            } else {
                notice_effect(state, "No session is attached.".into())
            }
        }
        Ok(VoiceCommand::Mode(mode)) => {
            let Some(session) = state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.clone())
            else {
                return notice_effect(state, "No session is attached.".into());
            };
            state
                .client_speech_modes
                .insert((state.profile.clone(), session.clone()), mode);
            let mut effects = if mode == SpeechMode::Alerts && state.client_voice.speaking() {
                client_voice_input(state, &session, Input::Cutoff(Cutoff::Stop))
            } else {
                Vec::new()
            };
            effects.extend(notice_effect(
                state,
                format!(
                    "Voice mode: {}.",
                    match mode {
                        SpeechMode::Full => "full",
                        SpeechMode::Summary => "summary",
                        SpeechMode::Alerts => "alerts",
                    }
                ),
            ));
            effects
        }
        Ok(VoiceCommand::Status) => notice_effect(
            state,
            format!(
                "Voice backend: client-bridge. Capture helper: {}. Playback helper: {}. Relay: {}. Remote audio opt-in: {}.",
                state.client_helper_status,
                state.client_playback_status,
                state.client_relay_status,
                if state.client_voice_config.remote_audio {
                    "on"
                } else {
                    "off"
                }
            ),
        ),
        Err(text) => notice_effect(state, text),
    }
}

fn client_voice_input(
    state: &mut AppState,
    session: &str,
    input: talaria_core::voice::client::Input,
) -> Vec<AppEffect> {
    use talaria_core::voice::client::Effect;
    let mut effects = Vec::new();
    for effect in state.client_voice.update(input) {
        match effect {
            Effect::Capture { epoch } => effects.push(AppEffect::ClientVoiceCapture {
                session_id: session.to_owned(),
                epoch,
            }),
            Effect::FinishCapture { epoch } => effects.push(AppEffect::ClientVoiceFinish { epoch }),
            Effect::Transcribe { .. } => {}
            Effect::Draft(text) if state.client_voice_target.is_some() => {
                let target = state.client_voice_target.take().expect("target checked");
                if state.client_voice_inflight.contains(&target.key) {
                    effects.extend(voice_notice(
                        state,
                        session,
                        format!(
                            "Voice transcript for {} was not sent: {}",
                            target.key.session_id, text
                        ),
                    ));
                } else if let Some(fleet) = state.fleet.as_mut() {
                    fleet.stage_voice(&target.key, &text);
                    state.client_voice_drafts.insert(target.key.clone(), target);
                } else {
                    effects.extend(voice_notice(
                        state,
                        session,
                        format!(
                            "Voice transcript for {} was not sent: {}",
                            target.key.session_id, text
                        ),
                    ));
                }
            }
            Effect::Draft(text) if is_open_session(state, session) => {
                if state.client_voice_config.client_submit_mode == "direct"
                    && state.draft.is_empty()
                {
                    let submission_id = state.next_submission_id();
                    effects.extend(reduce(
                        state,
                        Event::Submit {
                            display: text.clone(),
                            text,
                            submission_id,
                            attachments: Vec::new(),
                        },
                    ));
                } else {
                    let trimmed = state.draft.trim_end().len();
                    state.draft.truncate(trimmed);
                    if !state.draft.is_empty() {
                        state.draft.push(' ');
                    }
                    state.draft.push_str(&text);
                    let draft = state.draft.clone();
                    effects.extend(load_draft(state, draft));
                }
            }
            Effect::CancelCapture { .. } => effects.push(AppEffect::ClientVoiceCancelCapture),
            Effect::StopPlayback { audio_epoch } => effects.push(AppEffect::ClientVoiceStop {
                session_id: session.to_owned(),
                audio_epoch,
            }),
            Effect::KillPlayback { .. } => effects.push(AppEffect::ClientVoiceClose),
            Effect::Notice(text) => effects.extend(voice_notice(state, session, text)),
            Effect::Draft(_) => {}
        }
    }
    effects.push(AppEffect::Redraw);
    effects
}

fn voice_input(
    state: &mut AppState,
    session: &str,
    input: talaria_core::voice::VoiceInput,
) -> Vec<AppEffect> {
    if state.client_voice_config.backend == "client-bridge" {
        return Vec::new();
    }
    use talaria_core::voice::VoiceEffect as V;
    let is_tick = matches!(input, talaria_core::voice::VoiceInput::Tick(_));
    if matches!(input, talaria_core::voice::VoiceInput::Press) {
        state.voice_last_tick_ms = state.now_ms;
    }
    let was_timed = state.voice.get(session).is_some_and(|voice| {
        matches!(
            voice.phase(),
            talaria_core::voice::VoicePhase::Recording
                | talaria_core::voice::VoicePhase::Transcribing
        )
    });
    let produced = state.voice.update(session, input, state.draft.is_empty());
    let now_timed = state.voice.get(session).is_some_and(|voice| {
        matches!(
            voice.phase(),
            talaria_core::voice::VoicePhase::Recording
                | talaria_core::voice::VoicePhase::Transcribing
        )
    });
    if !was_timed && now_timed {
        state.voice_last_tick_ms = state.now_ms;
    }
    let mut effects = Vec::new();
    for effect in produced {
        match effect {
            V::Submit(text) if is_open_session(state, session) => {
                let submission_id = state.next_submission_id();
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
            V::AppendToDraft(text) if is_open_session(state, session) => {
                let trimmed = state.draft.trim_end().len();
                state.draft.truncate(trimmed);
                if !state.draft.is_empty() {
                    state.draft.push(' ');
                }
                state.draft.push_str(&text);
                let draft = state.draft.clone();
                effects.extend(load_draft(state, draft));
            }
            V::Speak(_) if state.remote_gateway => {
                if state
                    .remote_voice_noticed
                    .insert((state.profile.clone(), session.to_owned()))
                {
                    effects.extend(voice_notice(state, session, REMOTE_VOICE_NOTICE.into()));
                }
            }
            V::Notice(text) => effects.extend(voice_notice(state, session, text)),
            V::NoticeIn { session_id, text } => {
                effects.extend(voice_notice(state, &session_id, text))
            }
            V::Submit(_) | V::AppendToDraft(_) | V::StopBeforeDetach => {}
            other => effects.push(AppEffect::Voice {
                session_id: session.to_owned(),
                effect: other,
            }),
        }
    }
    if is_open_session(state, session) && (!is_tick || was_timed || now_timed) {
        effects.push(AppEffect::Redraw);
    }
    effects
}

fn voice_notice(state: &mut AppState, session: &str, text: String) -> Vec<AppEffect> {
    if is_open_session(state, session) {
        return notice_effect(state, text);
    }
    if let Some(opened) =
        retained_key(state, session).and_then(|key| state.retained_sessions.get_mut(&key))
    {
        let _ = talaria_core::session::update(
            &mut opened.state,
            Event::Notice {
                kind: NoticeKind::System,
                text,
            },
        );
    }
    Vec::new()
}

fn voice_link(state: &mut AppState, event: &LinkEvent) -> Vec<AppEffect> {
    let LinkEvent::Inbound {
        item: Inbound::Event(inbound),
        ..
    } = event
    else {
        return Vec::new();
    };
    let Some(id) = inbound.raw.get("session_id").and_then(Value::as_str) else {
        return Vec::new();
    };
    let opened = state
        .opened
        .as_ref()
        .filter(|opened| opened.session_id == id)
        .or_else(|| retained_key(state, id).and_then(|key| state.retained_sessions.get(&key)));
    let Some(opened) = opened else {
        return Vec::new();
    };
    let (epoch, seq) = event_position(&inbound.raw);
    if !accept_event(&opened.cursor, epoch, seq) {
        return Vec::new();
    }
    let focused = is_open_session(state, id);
    let Some((session, voice)) = crate::voice::voice_event(&inbound.raw, focused) else {
        return Vec::new();
    };
    voice_input(
        state,
        &session,
        talaria_core::voice::VoiceInput::Event(voice),
    )
}

fn voice_status(
    state: &mut AppState,
    session: &str,
    mode: Result<Option<String>, String>,
    requirements: Result<hermes_contract::schemas::VoiceToggleResult, (Option<i64>, String)>,
) -> Vec<AppEffect> {
    use talaria_core::voice::{VoiceInput, clean_message};
    let mut effects = Vec::new();
    let mode_text = match mode {
        Ok(mode) => {
            effects.extend(voice_input(
                state,
                session,
                VoiceInput::ModeRead(mode.clone()),
            ));
            mode.unwrap_or_else(|| "off".into())
                .chars()
                .filter(|ch| !ch.is_control())
                .take(24)
                .collect::<String>()
        }
        Err(message) => format!("unknown ({})", clean_message(&message)),
    };
    let chords = state
        .key_bindings
        .get("voice_record")
        .cloned()
        .unwrap_or_else(|| vec!["alt+r".into()]);
    let chords = if chords.is_empty() {
        "not bound".into()
    } else {
        chords
            .iter()
            .map(|chord| {
                chord
                    .split('+')
                    .map(|part| {
                        let mut chars = part.chars();
                        chars.next().map_or_else(String::new, |first| {
                            first.to_uppercase().collect::<String>() + chars.as_str()
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    if matches!(&requirements, Err((Some(-32601), _))) {
        effects.extend(voice_input(state, session, VoiceInput::Unavailable));
    }
    let read = state
        .voice
        .get(session)
        .is_some_and(talaria_core::voice::VoiceState::read_aloud);
    let mut lines = vec![
        format!("Mode: {mode_text}"),
        format!("Push-to-talk: {chords}"),
        format!("Read aloud: {}", if read { "on" } else { "off" }),
    ];
    match requirements {
        Ok(result)
            if result.available.is_none()
                && result.audio_available.is_none()
                && result.stt_available.is_none() =>
        {
            lines
                .push("Requirements: unknown (gateway probe returned no availability data).".into())
        }
        Ok(result) => {
            fn yes(value: Option<bool>) -> &'static str {
                match value {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "unknown",
                }
            }
            lines.push(format!("Audio: {}", yes(result.audio_available)));
            lines.push(format!("Speech-to-text: {}", yes(result.stt_available)));
            lines.push(format!("Ready: {}", yes(result.available)));
            if let Some(details) = result.details {
                lines.extend(
                    details
                        .lines()
                        .map(talaria_core::voice::clean_transcript)
                        .filter(|line| !line.is_empty())
                        .map(|line| format!("  {line}")),
                );
            }
        }
        Err((_code, message)) => {
            lines.push(format!(
                "Requirements: unknown ({}).",
                clean_message(&message)
            ));
        }
    }
    effects.extend(voice_notice(state, session, lines.join("\n")));
    effects
}

fn voice_error(error: &hermes_client::rpc::CallError) -> (Option<i64>, String, bool) {
    match error {
        hermes_client::rpc::CallError::Rpc(rpc) => (Some(rpc.code), rpc.message.clone(), false),
        _ => (
            None,
            error.to_string(),
            error.outcome_unknown() || matches!(error, hermes_client::rpc::CallError::NotConnected),
        ),
    }
}

fn run_voice_status(state: &AppState, executor: &Executor, session_id: String) {
    if state.remote_gateway || state.client_voice_config.backend == "client-bridge" {
        return;
    }
    let Some(client) = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned()
    else {
        return;
    };
    let generation = state.connection_generation;
    executor.send_later(async move {
        let (mode, requirements) = tokio::join!(
            hermes_client::voice_api::voice_mode(&client, &session_id),
            hermes_client::voice_api::voice_requirements(&client)
        );
        AppMessage::VoiceStatus {
            session_id,
            generation,
            mode: mode.map_err(|e| voice_error(&e).1),
            requirements: requirements.map_err(|e| {
                let (code, message, _) = voice_error(&e);
                (code, message)
            }),
        }
    });
}

fn run_voice_effect(
    state: &mut AppState,
    executor: &Executor,
    session_id: String,
    effect: talaria_core::voice::VoiceEffect,
) {
    use talaria_core::voice::{RecordStatus, VoiceEffect as V, VoiceInput as I};
    if state.remote_gateway || state.client_voice_config.backend == "client-bridge" {
        return;
    }
    let Some(client) = state
        .service
        .as_ref()
        .and_then(SessionService::client)
        .cloned()
    else {
        return;
    };
    let generation = state.connection_generation;
    let sender = executor.sender.clone();
    if matches!(effect, V::ReadSubmitMode) {
        if state.voice_config_pending {
            return;
        }
        state.voice_config_pending = true;
    }
    let pending = matches!(effect, V::Record { .. }).then(|| state.voice.track_record(&session_id));
    tokio::spawn(async move {
        let send = |input| {
            let _ = sender.send(AppMessage::Voice {
                session_id: session_id.clone(),
                generation,
                input,
            });
        };
        let notice = |text| {
            let _ = sender.send(AppMessage::VoiceNotice {
                session_id: session_id.clone(),
                generation,
                text,
            });
        };
        match effect {
            V::ReadMode => {
                if let Ok(mode) = hermes_client::voice_api::voice_mode(&client, &session_id).await {
                    send(I::ModeRead(mode));
                }
            }
            V::ReadSubmitMode => {
                let mode = hermes_client::voice_api::voice_submit_mode(&client)
                    .await
                    .ok();
                let _ = sender.send(AppMessage::VoiceFull { generation, mode });
            }
            V::SetMode { on } => {
                match hermes_client::voice_api::set_voice_mode(&client, &session_id, on).await {
                    Ok(result) => {
                        if let Some(output) = result.output.filter(|text| !text.is_empty()) {
                            notice(output);
                        }
                        if let Some(warning) = result.warning.filter(|text| !text.is_empty()) {
                            notice(warning);
                        }
                        if let Ok(mode) =
                            hermes_client::voice_api::voice_mode(&client, &session_id).await
                        {
                            send(I::ModeRead(mode));
                        }
                        if on {
                            let mode = hermes_client::voice_api::voice_submit_mode(&client)
                                .await
                                .ok();
                            let _ = sender.send(AppMessage::VoiceFull { generation, mode });
                        }
                    }
                    Err(error) => {
                        notice(format!(
                            "Could not change voice mode: {}.",
                            talaria_core::voice::clean_message(&voice_error(&error).1)
                        ));
                        if let Ok(mode) =
                            hermes_client::voice_api::voice_mode(&client, &session_id).await
                        {
                            send(I::ModeRead(mode));
                        }
                    }
                }
            }
            V::Record { start } => {
                match hermes_client::voice_api::voice_record(&client, &session_id, start).await {
                    Ok(reply) => {
                        use hermes_contract::schemas::VoiceRecordStatus as R;
                        let status = match reply.status {
                            R::Recording => RecordStatus::Recording,
                            R::Stopped => RecordStatus::Stopped,
                            R::Busy => RecordStatus::Busy,
                        };
                        if let Some(pending) = pending {
                            use crate::voice::RecordSettlement as S;
                            let settlement = if !start {
                                S::StopConfirmed
                            } else if status == RecordStatus::Recording {
                                S::Started
                            } else {
                                S::Rejected
                            };
                            let _ = pending.send(Some(settlement));
                        }
                        send(I::RecordReply {
                            start,
                            status,
                            reason: reply.reason,
                        });
                    }
                    Err(error) => {
                        if let Some(pending) = pending {
                            use crate::voice::RecordSettlement as S;
                            let settlement =
                                if matches!(error, hermes_client::rpc::CallError::Rpc(_)) {
                                    if start { S::Rejected } else { S::StopConfirmed }
                                } else {
                                    S::Unknown
                                };
                            let _ = pending.send(Some(settlement));
                        }
                        let (code, message, dropped) = voice_error(&error);
                        send(I::RecordFailed {
                            start,
                            code,
                            message,
                            dropped,
                        });
                    }
                }
            }
            V::Speak(text) => {
                if let Err(error) = hermes_client::voice_api::voice_speak(&client, &text).await {
                    let (code, message, dropped) = voice_error(&error);
                    send(I::SpeakFailed {
                        code,
                        message,
                        dropped,
                    });
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod voice_ownership_tests {
    use super::*;

    #[test]
    fn retained_voice_notices_respect_profile_and_ambiguity() {
        let snapshot: hermes_contract::schemas::CanonicalSnapshot =
            serde_json::from_str(include_str!("../tests/data/running_turn.json"))
                .expect("snapshot");
        let session = snapshot.session_id.clone();
        let first = RowKey {
            profile: "first".into(),
            session_id: session.clone(),
        };
        let second = RowKey {
            profile: "second".into(),
            session_id: session.clone(),
        };
        let mut state = AppState::new("voice-owner");
        state.profile = first.profile.clone();
        state.retained_sessions.insert(
            first.clone(),
            OpenedSession::from_snapshot(snapshot.clone()),
        );
        state
            .retained_sessions
            .insert(second.clone(), OpenedSession::from_snapshot(snapshot));

        voice_notice(&mut state, &session, "first only".into());
        let has_notice = |key: &RowKey, text: &str, state: &AppState| {
            state.retained_sessions[key]
                .state
                .cells()
                .iter()
                .any(|cell| matches!(cell, Cell::Notice(notice) if notice.text == text))
        };
        assert!(has_notice(&first, "first only", &state));
        assert!(!has_notice(&second, "first only", &state));

        state.profile = "different".into();
        voice_notice(&mut state, &session, "ambiguous".into());
        assert!(!has_notice(&first, "ambiguous", &state));
        assert!(!has_notice(&second, "ambiguous", &state));

        state.retained_sessions.remove(&second);
        voice_notice(&mut state, &session, "unique".into());
        assert!(has_notice(&first, "unique", &state));
    }

    #[test]
    fn typed_stop_reply_reads_the_origin_after_focus_changes() {
        let snapshot: hermes_contract::schemas::CanonicalSnapshot =
            serde_json::from_str(include_str!("../tests/data/running_turn.json"))
                .expect("snapshot");
        let mut state = AppState::new("voice-owner");
        state.opened = Some(OpenedSession::from_snapshot(snapshot));
        state.voice.open("earlier-session");
        state.voice.update(
            "earlier-session",
            talaria_core::voice::VoiceInput::ModeRead(Some("voice_only".into())),
            true,
        );
        state
            .voice_submissions
            .insert("prompt-1".into(), "earlier-session".into());
        state.submitted_attachments.insert(
            "prompt-1".into(),
            (
                "earlier-session".into(),
                SubmittedAttachments {
                    expanded: Expanded {
                        text: "stop".into(),
                        display: "stop".into(),
                        attachments: Vec::new(),
                        reference_count: 0,
                    },
                    media: Vec::new(),
                },
            ),
        );
        let effects = submit_outcome(&mut state, "prompt-1".into(), ExecuteOutcome::VoiceStopped);
        assert!(effects.iter().any(|effect| matches!(effect, AppEffect::Voice { session_id, effect: talaria_core::voice::VoiceEffect::ReadMode } if session_id == "earlier-session")));
        assert_eq!(
            state
                .voice
                .get("earlier-session")
                .map(talaria_core::voice::VoiceState::phase),
            Some(talaria_core::voice::VoicePhase::Off)
        );
    }

    #[test]
    fn read_only_card_is_absent_from_foreground_open() {
        let make_state = |method: &str| {
            let snapshot: hermes_contract::schemas::CanonicalSnapshot =
                serde_json::from_value(serde_json::json!({
                    "session_id": "sess-1",
                    "stored_session_id": "stored-1",
                    "messages": [],
                    "message_count": 0,
                    "running": false,
                    "authority_epoch": 1,
                    "replay_epoch": "epoch-1",
                    "last_sequence": 0,
                    "subscription_id": "sub-1",
                    "revision": 1,
                    "execution_generation": 1,
                    "pending": [],
                    "prompts": [],
                    "open_requests": [
                        {"id": "req-1", "method": method, "params": {}}
                    ],
                    "info": {"desktop_protocol": "1"},
                }))
                .expect("snapshot");
            let mut state = AppState::new("fixture");
            state.opened = Some(OpenedSession::from_snapshot(snapshot));
            state
        };

        let read_only = make_state("vault.save_login");
        assert_eq!(foreground_open(&read_only), 0);

        let sudo = make_state("sudo");
        assert_eq!(foreground_open(&sudo), 1);
    }
}

#[cfg(all(test, unix))]
mod background_client_log_tests {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;

    use super::append_background_client_log;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode()
    }

    #[test]
    fn background_client_log_is_owner_only() {
        let home = tempfile::tempdir().unwrap();
        append_background_client_log(home.path(), "status").unwrap();
        let talaria = home.path().join(".talaria");
        let logs = talaria.join("logs");
        let file = logs.join("client.log");
        assert_eq!(mode(&talaria) & 0o777, 0o700, "log parent directory");
        assert_eq!(mode(&logs) & 0o777, 0o700, "log directory");
        assert_eq!(mode(&file) & 0o077, 0, "new client log");

        std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        append_background_client_log(home.path(), "again").unwrap();
        assert_eq!(mode(&logs) & 0o777, 0o700, "existing log directory");
        assert_eq!(mode(&file) & 0o777, 0o600, "existing client log");
    }

    #[test]
    fn background_client_log_refuses_symlinked_talaria() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let marker = outside.path().join("marker");
        std::fs::write(&marker, b"keep").unwrap();
        let before = mode(outside.path());
        std::os::unix::fs::symlink(outside.path(), home.path().join(".talaria")).unwrap();
        assert!(append_background_client_log(home.path(), "nope").is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), b"keep");
        assert_eq!(mode(outside.path()), before);
        assert!(!outside.path().join("logs").exists());
        assert!(
            home.path()
                .join(".talaria")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn background_client_log_refuses_symlinked_logs() {
        let home = tempfile::tempdir().unwrap();
        let talaria = home.path().join(".talaria");
        std::fs::create_dir(&talaria).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("client.log"), b"keep").unwrap();
        let before = mode(outside.path());
        std::os::unix::fs::symlink(outside.path(), talaria.join("logs")).unwrap();
        assert!(append_background_client_log(home.path(), "nope").is_err());
        assert_eq!(
            std::fs::read(outside.path().join("client.log")).unwrap(),
            b"keep"
        );
        assert_eq!(mode(outside.path()), before);
        assert!(
            talaria
                .join("logs")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn background_client_log_refuses_symlinked_client_log() {
        let home = tempfile::tempdir().unwrap();
        let logs = home.path().join(".talaria/logs");
        std::fs::create_dir_all(&logs).unwrap();
        let target = logs.join("elsewhere.log");
        std::fs::write(&target, b"keep").unwrap();
        let before = mode(&target);
        std::os::unix::fs::symlink(&target, logs.join("client.log")).unwrap();
        assert!(append_background_client_log(home.path(), "nope").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        assert_eq!(mode(&target), before);
        assert!(
            logs.join("client.log")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn background_client_log_migration_hides_new_bytes_from_old_reader() {
        let home = tempfile::tempdir().unwrap();
        let logs = home.path().join(".talaria/logs");
        std::fs::create_dir_all(&logs).unwrap();
        let path = logs.join("client.log");
        std::fs::write(&path, b"legacy\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let mut old = std::fs::File::open(&path).unwrap();
        let old_ino = old.metadata().unwrap().ino();
        let mut before = String::new();
        old.read_to_string(&mut before).unwrap();
        assert_eq!(before, "legacy\n");

        append_background_client_log(home.path(), "secret").unwrap();

        let mut extra = String::new();
        old.read_to_string(&mut extra).unwrap();
        assert_eq!(extra, "");
        let mut disk = std::fs::File::open(&path).unwrap();
        let mut seen = String::new();
        disk.read_to_string(&mut seen).unwrap();
        assert_eq!(seen, "legacy\nsecret\n");
        assert_ne!(disk.metadata().unwrap().ino(), old_ino);
        assert_eq!(mode(&path) & 0o777, 0o600);
    }
}

#[cfg(test)]
mod workflow_agent_session_tests {
    use super::*;
    use talaria_workflow::progress::agent_session::{AgentPhase, Tracking};

    const RUN: &str = "01JB2X9Q4M8Z7K3T5V6W8Y0A1C";

    fn digest() -> String {
        "a".repeat(64)
    }

    /// A run folder under `runs` whose one agent, `review:gameplay`, is bound to
    /// `stored-bound` and has failed.
    fn run_folder(runs: &Path) -> PathBuf {
        let dir = runs.join(RUN);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = json!({
            "v":1,"run_id":RUN,"created_at_ms":1000,
            "workflow":{"name":"review-branch","script_sha256":"a".repeat(64)},
            "args":null,"started_at_ms":1,"seed":4242,"profile":"fixture",
            "on_unknown":"wait","talaria_version":env!("CARGO_PKG_VERSION"), "hermes":{},
            "roster":{},"approval_policy":{}
        });
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        std::fs::write(dir.join("run.lock"), "").unwrap();
        let key = "8".repeat(32);
        let records = [
            json!({"t":"run_started","run_id":RUN,"script_sha256":"a".repeat(64)}),
            json!({"t":"call_intent","key":key,"attempt":0,
                "call_id":{"digest":digest(),"occurrence":0},"label":"review:gameplay",
                "phase":"Verify","workflow_path":[],
                "launch":{"profile":"fixture","model":"fixture-model","provider":null,
                "effort":null,"toolsets":[],"skills":[],"max_turns":null,"isolation":null,
                "cwd":null},"request_id":"wf:fixture"}),
            json!({"t":"session_bound","key":key,"attempt":0,
                "session_id":"runtime-bound","stored_session_id":"stored-bound"}),
            json!({"t":"call_finished","key":key,"attempt":0,
                "outcome":{"kind":"failed","reason":"context too long"},"turns":1}),
        ];
        let mut text = String::new();
        for (i, mut record) in records.into_iter().enumerate() {
            record["v"] = 1.into();
            record["seq"] = (i as u64 + 1).into();
            record["at_ms"] = (1000 + i as i64).into();
            text.push_str(&format!("{record}\n"));
        }
        std::fs::write(dir.join("journal.jsonl"), text).unwrap();
        dir
    }

    fn session(runtime: &str, stored: &str, cwd: Option<String>) -> OpenedSession {
        OpenedSession {
            state: SessionState::empty(SessionId::new(runtime)),
            session_id: runtime.into(),
            stored_session_id: stored.into(),
            subscription_id: "subscription".into(),
            title: None,
            revision: 0,
            execution_generation: 0,
            launch_notices: Vec::new(),
            effort: None,
            info: SessionInfoSummary {
                model: None,
                provider: None,
                cwd,
                branch: None,
            },
            cursor: Cursor {
                replay_epoch: "epoch".into(),
                last_sequence: 0,
            },
        }
    }

    /// Do what the executor does with the journal read an opening asks for, then hand the
    /// answer back as the message the executor would send.
    fn answer_reads(state: &mut AppState, effects: Vec<AppEffect>) {
        for effect in effects {
            if let AppEffect::WorkflowAgentRead { stored_id, request } = effect {
                let result = talaria_workflow::progress::agent_session::read_agent_journal(
                    &request.run_dir,
                    request.offset,
                );
                let _ = update(state, AppMessage::WorkflowAgentRead { stored_id, result });
            }
        }
    }

    fn open_with(
        state: &mut AppState,
        runs: &Path,
        runtime: &str,
        stored: &str,
        in_worktree: bool,
    ) -> Vec<AppEffect> {
        state.profile = "fixture".into();
        state.workflow_runs_dir = runs.to_owned();
        let cwd = in_worktree.then(|| {
            runs.join(RUN)
                .join("worktrees")
                .join(format!("{}-0", digest()))
                .to_string_lossy()
                .into_owned()
        });
        opened_session(state, session(runtime, stored, cwd), None)
    }

    fn open(state: &mut AppState, runs: &Path, runtime: &str, stored: &str, in_worktree: bool) {
        let effects = open_with(state, runs, runtime, stored, in_worktree);
        answer_reads(state, effects);
    }

    fn sent(effects: &[AppEffect]) -> bool {
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Send(Effect::SendPrompt { .. })))
    }

    #[test]
    fn a_follow_on_session_in_a_run_worktree_is_labelled_untracked() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        let note = state.view().workflow_agent.cloned().expect("labelled");
        assert_eq!(note.label.as_deref(), Some("review:gameplay"));
        assert_eq!(note.phase, Some(AgentPhase::Failed));
        assert_eq!(note.tracking, Tracking::NotFollowed);
    }

    #[test]
    fn the_bound_session_is_followed_even_without_a_worktree_folder() {
        let temp = tempfile::tempdir().unwrap();
        let run = run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        // The folder is not a worktree, so only the loaded run can say whose session this is.
        let manifest = talaria_workflow::journal::RunDir::open(&run).unwrap();
        let (lines, offset) = talaria_workflow::journal::JournalReader::read_from(&run, 0).unwrap();
        let opened_run = crate::workflow_progress::OpenedRun {
            path: run.clone(),
            profile: "fixture".into(),
            state: talaria_workflow::progress::live::ProgressState::new(
                talaria_workflow::progress::RunProgress::from_records(manifest.manifest(), &lines),
            ),
            offset,
        };
        workflow_opened(&mut state, Ok(opened_run));
        open(
            &mut state,
            temp.path(),
            "runtime-bound",
            "stored-bound",
            false,
        );
        let note = state.view().workflow_agent.cloned().expect("labelled");
        assert_eq!(note.tracking, Tracking::Followed);
        assert_eq!(note.phase, Some(AgentPhase::Failed));
    }

    #[test]
    fn an_ordinary_session_shows_no_banner_and_sends_at_once() {
        let mut state = AppState::new("agent-banner");
        state.profile = "fixture".into();
        let _ = opened_session(&mut state, session("runtime-1", "stored-1", None), None);
        assert!(state.view().workflow_agent.is_none());
        assert!(sent(&submit(&mut state, "hello".into())));
    }

    #[test]
    fn the_first_prompt_into_an_agent_session_warns_and_keeps_the_text_then_the_second_sends() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        let first = submit(&mut state, "yes, commit it".into());
        assert!(!sent(&first), "{first:?}");
        assert!(
            first.iter().any(
                |effect| matches!(effect, AppEffect::LoadDraft(text) if text == "yes, commit it")
            ),
            "the typed text must come back: {first:?}"
        );
        let warning: Vec<String> = first
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::CommitLines(lines) => Some(lines.join("\n")),
                _ => None,
            })
            .collect();
        assert_eq!(warning.len(), 1, "{first:?}");
        assert!(warning[0].contains("review:gameplay"), "{warning:?}");
        assert!(
            warning[0].contains("does not track this conversation"),
            "{warning:?}"
        );
        assert!(warning[0].contains("Submit again to send."), "{warning:?}");
        let second = submit(&mut state, "yes, commit it".into());
        assert!(sent(&second), "{second:?}");
        let third = submit(&mut state, "and push".into());
        assert!(sent(&third), "once per session: {third:?}");
    }

    #[test]
    fn leaving_and_returning_to_the_session_does_not_warn_again() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert!(!sent(&submit(&mut state, "one".into())));
        let _ = opened_session(&mut state, session("runtime-1", "stored-1", None), None);
        assert!(state.view().workflow_agent.is_none());
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert!(state.view().workflow_agent.is_some());
        assert!(sent(&submit(&mut state, "two".into())));
    }

    #[test]
    fn the_workflow_key_in_an_agent_session_opens_that_runs_view() {
        let temp = tempfile::tempdir().unwrap();
        let run = run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        let effects = update(&mut state, AppMessage::User(UserAction::WorkflowView));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::WorkflowOpen(path) if *path == run)),
            "{effects:?}"
        );
    }

    #[test]
    fn the_banner_follows_the_journal_while_the_agent_runs() {
        let temp = tempfile::tempdir().unwrap();
        let run = run_folder(temp.path());
        // A live runner, and the failure taken back out: the agent is bound and unfinished.
        let runner = std::fs::File::open(run.join("run.lock")).unwrap();
        runner.try_lock().unwrap();
        let journal = std::fs::read_to_string(run.join("journal.jsonl")).unwrap();
        let unfinished: Vec<&str> = journal.lines().take(3).collect();
        std::fs::write(run.join("journal.jsonl"), unfinished.join("\n") + "\n").unwrap();
        let mut state = AppState::new("agent-banner");
        state.now_ms = 1_000;
        let opening = open_with(
            &mut state,
            temp.path(),
            "runtime-bound",
            "stored-bound",
            true,
        );
        // Until the read answers, the banner says it is still reading.
        let note = state.view().workflow_agent.cloned().unwrap();
        assert!(note.pending, "{note:?}");
        answer_reads(&mut state, opening);
        let note = state.view().workflow_agent.cloned().unwrap();
        assert_eq!(note.phase, Some(AgentPhase::Queued));
        std::fs::write(run.join("journal.jsonl"), journal).unwrap();
        let effects = update(&mut state, AppMessage::Tick { now_ms: 4_000 });
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::WorkflowAgentRead { .. })),
            "the tick asks for a read instead of doing it: {effects:?}"
        );
        answer_reads(&mut state, effects);
        let note = state.view().workflow_agent.cloned().unwrap();
        assert_eq!(note.phase, Some(AgentPhase::Failed));
        // The agent has stopped: no further reads are asked for.
        let later = update(&mut state, AppMessage::Tick { now_ms: 9_000 });
        assert!(
            !later
                .iter()
                .any(|e| matches!(e, AppEffect::WorkflowAgentRead { .. })),
            "{later:?}"
        );
        drop(runner);
    }

    #[test]
    fn one_read_runs_at_a_time_and_a_dead_runner_is_not_polled() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        state.now_ms = 1_000;
        let opening = open_with(
            &mut state,
            temp.path(),
            "runtime-bound",
            "stored-bound",
            true,
        );
        assert_eq!(
            opening
                .iter()
                .filter(|e| matches!(e, AppEffect::WorkflowAgentRead { .. }))
                .count(),
            1
        );
        // The first read has not answered: a tick must not start a second one.
        let tick = update(&mut state, AppMessage::Tick { now_ms: 9_000 });
        assert!(
            !tick
                .iter()
                .any(|e| matches!(e, AppEffect::WorkflowAgentRead { .. }))
        );
    }

    #[test]
    fn a_failed_open_clears_the_agent_session_and_its_polling() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert!(state.workflow_agent.is_some());
        let _ = update(
            &mut state,
            AppMessage::SessionCleared(SessionCleared::OpenFailed {
                previous_id: "runtime-new".into(),
                reason: "gone".into(),
                left: true,
            }),
        );
        assert!(state.opened.is_none());
        assert!(state.workflow_agent.is_none());
    }

    #[test]
    fn a_prompt_held_while_the_session_opened_gets_the_first_send_warning() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        // The person typed before the session finished opening.
        state.held.push_back("yes, commit it".into());
        let effects = open_with(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert!(!sent(&effects), "{effects:?}");
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::LoadDraft(text) if text == "yes, commit it")),
            "the text comes back to the composer: {effects:?}"
        );
        assert_eq!(state.draft, "yes, commit it");
        assert!(state.held.is_empty());
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppEffect::CommitLines(lines)
            if lines.iter().any(|l| l.contains("Workflow agent session"))))
        );
        // Submitting it again goes through: the warning was given once.
        assert!(sent(&submit(&mut state, "yes, commit it".into())));
    }

    #[test]
    fn a_held_prompt_returns_ahead_of_text_typed_since() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-banner");
        state.held.push_back("first".into());
        state.draft = "second".into();
        state.caret = state.draft.len();
        let _ = open_with(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert_eq!(state.draft, "first\nsecond");
    }

    // --- Going back from an agent's session to the session the person came from. ---

    const AGENT_KEY: &str = "88888888888888888888888888888888";

    /// A person in an ordinary session with the run's view loaded.
    fn from_origin(profile: &str) -> (tempfile::TempDir, AppState, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let run = run_folder(temp.path());
        let mut state = AppState::new("agent-back");
        state.profile = profile.into();
        state.workflow_runs_dir = temp.path().to_owned();
        let manifest = talaria_workflow::journal::RunDir::open(&run).unwrap();
        let (lines, offset) = talaria_workflow::journal::JournalReader::read_from(&run, 0).unwrap();
        workflow_opened(
            &mut state,
            Ok(crate::workflow_progress::OpenedRun {
                path: run.clone(),
                profile: profile.into(),
                state: talaria_workflow::progress::live::ProgressState::new(
                    talaria_workflow::progress::RunProgress::from_records(
                        manifest.manifest(),
                        &lines,
                    ),
                ),
                offset,
            }),
        );
        let _ = opened_session(
            &mut state,
            session("runtime-origin", "stored-origin", None),
            None,
        );
        (temp, state, run)
    }

    /// Enter on the agent row: the run view's key.
    fn press_enter(state: &mut AppState, run: &Path, session: &str) -> Vec<AppEffect> {
        let view = state.workflow_views.states.get_mut(run).unwrap();
        view.selection = talaria_workflow::progress::live::NodeKey::Agent(AGENT_KEY.into());
        workflow_input(
            state,
            run.to_owned(),
            talaria_workflow::progress::live::ProgressInput::Opened(session.into()),
        )
    }

    fn switch_target(effects: &[AppEffect]) -> Option<(String, String)> {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Switch { key } => Some((key.profile.clone(), key.session_id.clone())),
            _ => None,
        })
    }

    /// The executor opened `opened` and the switch finished.
    fn land(state: &mut AppState, opened: OpenedSession) -> Vec<AppEffect> {
        let effects = update(
            state,
            AppMessage::Switched {
                outcome: SwitchOutcome::Switched {
                    notice: "Switched.".into(),
                },
                install: Some(SwitchInstall {
                    opened,
                    service: SessionService::new("launch"),
                    link: None,
                }),
                reason: None,
            },
        );
        let reads: Vec<AppEffect> = effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::WorkflowAgentRead { stored_id, request } => {
                    Some(AppEffect::WorkflowAgentRead {
                        stored_id: stored_id.clone(),
                        request: request.clone(),
                    })
                }
                _ => None,
            })
            .collect();
        answer_reads(state, reads);
        effects
    }

    fn agent_session(runs: &Path) -> OpenedSession {
        let _ = runs;
        session("runtime-bound", "stored-bound", None)
    }

    /// The key did nothing: at most the screen is drawn again.
    fn only_redraws(effects: &[AppEffect]) -> bool {
        effects
            .iter()
            .all(|effect| matches!(effect, AppEffect::Redraw))
    }

    fn back(state: &mut AppState) -> Vec<AppEffect> {
        update(state, AppMessage::User(UserAction::WorkflowAgentBack))
    }

    #[test]
    fn back_returns_to_the_origin_and_reopens_the_view_with_the_agent_selected() {
        let (temp, mut state, run) = from_origin("fixture");
        let entered = press_enter(&mut state, &run, "runtime-bound");
        assert_eq!(
            switch_target(&entered),
            Some(("fixture".into(), "runtime-bound".into()))
        );
        assert!(!state.view().agent_back, "not confirmed until it lands");
        let _ = land(&mut state, agent_session(temp.path()));
        assert!(state.view().workflow_agent.is_some());
        assert!(state.view().agent_back);
        assert_eq!(state.workflow_views.active, None, "Enter closed the view");
        // The person moves the selection elsewhere while away; the view comes back as left.
        state.workflow_views.states.get_mut(&run).unwrap().selection =
            talaria_workflow::progress::live::NodeKey::Run;

        let sent = back(&mut state);
        assert_eq!(
            switch_target(&sent),
            Some(("fixture".into(), "stored-origin".into())),
            "{sent:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|effect| matches!(effect, AppEffect::Interrupt(_) | AppEffect::Send(_))),
            "nothing is sent to the agent: {sent:?}"
        );
        // The origin is dropped as the trip starts, so a second press does nothing.
        assert!(state.workflow_origin.is_none());
        assert!(!state.view().agent_back);

        let _ = land(
            &mut state,
            session("runtime-origin-2", "stored-origin", None),
        );
        assert_eq!(state.workflow_views.active.as_deref(), Some(run.as_path()));
        let view = &state.workflow_views.states[&run];
        assert!(!view.closed);
        assert_eq!(
            view.selection,
            talaria_workflow::progress::live::NodeKey::Agent(AGENT_KEY.into())
        );
        assert!(state.workflow_return.is_none());
        assert!(state.workflow_agent.is_none());
        assert!(!state.view().agent_back);
    }

    #[test]
    fn back_does_nothing_with_a_draft_or_a_palette_and_keeps_the_memory() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        state.draft = "half a thought".into();
        assert!(only_redraws(&back(&mut state)));
        state.draft.clear();
        state.palette.open = true;
        assert!(only_redraws(&back(&mut state)));
        state.palette.open = false;
        assert!(state.view().agent_back, "the memory is still there");
        assert!(switch_target(&back(&mut state)).is_some());
    }

    #[test]
    fn back_without_a_remembered_origin_does_nothing() {
        let temp = tempfile::tempdir().unwrap();
        run_folder(temp.path());
        let mut state = AppState::new("agent-back");
        // An agent's session reached by /resume or a picker, with no Enter behind it.
        open(&mut state, temp.path(), "runtime-new", "stored-new", true);
        assert!(state.view().workflow_agent.is_some());
        assert!(!state.view().agent_back);
        assert!(only_redraws(&back(&mut state)));
        // An ordinary session has nowhere to go back to either.
        let _ = opened_session(&mut state, session("runtime-1", "stored-1", None), None);
        assert!(only_redraws(&back(&mut state)));
    }

    #[test]
    fn any_other_route_to_a_different_session_drops_the_memory() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        assert!(state.view().agent_back);
        // A picker, /resume or a new session opens another session.
        let _ = opened_session(&mut state, session("runtime-9", "stored-9", None), None);
        assert!(state.workflow_origin.is_none());
        // Coming back to the agent's session by that route does not bring the memory back.
        let _ = opened_session(&mut state, agent_session(temp.path()), None);
        assert!(!state.view().agent_back);
        assert!(only_redraws(&back(&mut state)));
    }

    #[test]
    fn a_reconnect_of_the_agents_session_keeps_the_memory() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        // The same stored session reopens under a new runtime id.
        let _ = opened_session(
            &mut state,
            session("runtime-bound-2", "stored-bound", None),
            None,
        );
        assert!(state.view().agent_back);
        let sent = back(&mut state);
        assert_eq!(
            switch_target(&sent),
            Some(("fixture".into(), "stored-origin".into()))
        );
    }

    #[test]
    fn entering_another_agent_keeps_the_original_session_as_the_origin() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        // The person opens the run view from the agent's session and enters another agent,
        // whose session sits in the run's worktrees.
        let second = press_enter(&mut state, &run, "runtime-agent-2");
        assert_eq!(
            switch_target(&second),
            Some(("fixture".into(), "runtime-agent-2".into()))
        );
        let worktree = temp
            .path()
            .join(RUN)
            .join("worktrees")
            .join(format!("{}-0", digest()))
            .to_string_lossy()
            .into_owned();
        let _ = land(
            &mut state,
            session("runtime-agent-2", "stored-agent-2", Some(worktree)),
        );
        assert!(state.view().workflow_agent.is_some());
        assert!(state.view().agent_back);
        let sent = back(&mut state);
        assert_eq!(
            switch_target(&sent),
            Some(("fixture".into(), "stored-origin".into())),
            "never the first agent's session: {sent:?}"
        );
    }

    #[test]
    fn a_second_enter_that_fails_leaves_the_first_trip_standing() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        let _ = press_enter(&mut state, &run, "runtime-agent-2");
        let _ = update(
            &mut state,
            AppMessage::Switched {
                outcome: SwitchOutcome::Failed {
                    text: "could not open".into(),
                },
                install: None,
                reason: None,
            },
        );
        assert!(
            state.view().agent_back,
            "still in the first agent's session"
        );
        assert!(switch_target(&back(&mut state)).is_some());
    }

    #[test]
    fn a_first_enter_that_fails_remembers_nothing() {
        let (_temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = update(
            &mut state,
            AppMessage::Switched {
                outcome: SwitchOutcome::Failed {
                    text: "could not open".into(),
                },
                install: None,
                reason: None,
            },
        );
        assert!(state.workflow_origin.is_none());
    }

    #[test]
    fn back_works_when_the_agent_runs_on_another_profile() {
        let (temp, mut state, run) = from_origin("fixture");
        // The run was started on another profile: Enter takes the begin_switch path.
        state
            .workflow_views
            .profiles
            .insert(run.clone(), "other".into());
        let entered = press_enter(&mut state, &run, "runtime-bound");
        assert_eq!(
            switch_target(&entered),
            Some(("other".into(), "runtime-bound".into()))
        );
        let _ = land(&mut state, agent_session(temp.path()));
        assert_eq!(state.profile, "other");
        assert!(state.view().agent_back);

        // The key switches back to the profile and stored session the person left.
        let sent = back(&mut state);
        assert_eq!(
            switch_target(&sent),
            Some(("fixture".into(), "stored-origin".into())),
            "{sent:?}"
        );
        let _ = land(
            &mut state,
            session("runtime-origin-3", "stored-origin", None),
        );
        assert_eq!(state.profile, "fixture");
        assert_eq!(state.workflow_views.active.as_deref(), Some(run.as_path()));
        assert_eq!(
            state.workflow_views.states[&run].selection,
            talaria_workflow::progress::live::NodeKey::Agent(AGENT_KEY.into())
        );
    }

    #[test]
    fn an_origin_that_cannot_be_opened_says_so_once_and_the_person_stays() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        let _ = back(&mut state);
        let effects = update(
            &mut state,
            AppMessage::Switched {
                outcome: SwitchOutcome::Failed {
                    text: "Could not open \"stored-origin\": Hermes refused the resume (not_found). Still in the old session.".into(),
                },
                install: None,
                reason: Some("not_found".into()),
            },
        );
        let lines: Vec<String> = effects
            .iter()
            .filter_map(|effect| match effect {
                AppEffect::CommitLines(lines) => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("Could not go back"), "{lines:?}");
        assert_eq!(
            state
                .opened
                .as_ref()
                .map(|opened| opened.session_id.as_str()),
            Some("runtime-bound")
        );
        assert!(state.workflow_origin.is_none() && state.workflow_return.is_none());
        assert!(!state.view().agent_back);
        assert!(state.workflow_views.active.is_none(), "no view opened");
    }

    #[test]
    fn an_origin_whose_runtime_and_stored_ids_are_equal_is_remembered() {
        let (_temp, mut state, run) = from_origin("fixture");
        // The shape the Hermes session authority sends: one id in both fields, for the
        // person's session and for the agent's.
        let _ = opened_session(
            &mut state,
            session("local-origin", "local-origin", None),
            None,
        );
        let entered = press_enter(&mut state, &run, "runtime-bound");
        assert_eq!(
            switch_target(&entered),
            Some(("fixture".into(), "runtime-bound".into()))
        );
        let _ = land(&mut state, session("runtime-bound", "runtime-bound", None));
        assert!(state.view().workflow_agent.is_some());
        assert!(state.view().agent_back, "the key is offered");

        let sent = back(&mut state);
        assert_eq!(
            switch_target(&sent),
            Some(("fixture".into(), "local-origin".into())),
            "{sent:?}"
        );
        let _ = land(&mut state, session("local-origin", "local-origin", None));
        assert_eq!(state.workflow_views.active.as_deref(), Some(run.as_path()));
        assert!(state.workflow_return.is_none());
        assert!(!state.view().agent_back);
    }

    #[test]
    fn an_origin_with_no_id_at_all_is_not_offered() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = opened_session(&mut state, session("runtime-new-origin", "", None), None);
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let _ = land(&mut state, agent_session(temp.path()));
        assert!(state.workflow_origin.is_none());
        assert!(!state.view().agent_back);
        assert!(only_redraws(&back(&mut state)));
    }

    #[test]
    fn going_back_leaves_the_agents_turn_alone() {
        let (temp, mut state, run) = from_origin("fixture");
        let _ = press_enter(&mut state, &run, "runtime-bound");
        let mut running = agent_session(temp.path());
        running.state = SessionState::from_snapshot(
            serde_json::from_value(json!({
                "session_id": "runtime-bound", "stored_session_id": "stored-bound",
                "messages": [], "display_messages": [], "message_count": 0, "running": true,
                "authority_epoch": 1, "replay_epoch": "epoch-1", "last_sequence": 1,
                "subscription_id": "sub-1", "revision": 1, "execution_generation": 2,
                "pending": [], "prompts": [], "info": {"desktop_protocol": "1", "cwd": "/work/proj"}
            }))
            .unwrap(),
        );
        let _ = land(&mut state, running);
        assert!(turn_running(&state));
        let sent = back(&mut state);
        assert!(switch_target(&sent).is_some());
        assert!(
            !sent.iter().any(|effect| matches!(
                effect,
                AppEffect::Interrupt(_) | AppEffect::Send(_) | AppEffect::Queue(_)
            )),
            "{sent:?}"
        );
    }
}
