//! Build the services for one launch and run until the process should exit.
//!
//! Order: the launch folder, the `--record` path and `~/.talaria/config.toml` are checked first
//! (local reads only), then the terminal; the `--resume` list, when asked for, connects and asks
//! on the normal screen; then the terminal is set up and `talaria_app::app::run` draws the first
//! frame before anything touches the network.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, IsTerminal, Stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use talaria_app::app::{
    self, AttachmentView, ComposerDeadline, InputCode, InputKey, LinkView, Outcome, RunSetup,
    Screen, StatusHost, ToggleContext, TranscriptAction, UserAction, View,
};
use talaria_app::appearance::AppearanceView;
use talaria_app::attachments::{Action as AttachmentAction, LabelStyle};
use talaria_app::bg::dialog::{DialogEvent, DialogStatus, LaunchSubmit};
use talaria_app::comforts::{editor, herdr::HerdrReporter, prompt_history::PromptHistory, suspend};
use talaria_app::config::{self, LoadedConfig};
use talaria_app::connection::OpenConnection;
use talaria_app::context::ContextBlock;
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::launch::{Launch, StartMode, new_launch_id};
use talaria_app::peek::CardAnswer;
use talaria_app::prompts::{MaskedKind, MaskedRequestInfo};
use talaria_app::subagents::SubagentAction;
use talaria_core::appearance::ThemeReport;
use talaria_core::attachments::CollapseRule;
use talaria_core::attention::{Alert, AttentionConfig, PaneReport, PaneState};
use talaria_core::checklist::ChecklistCommit;
use talaria_core::commands::PaletteInput;
use talaria_core::configure::{ConfigureInput, ConfigureKey, Phase as ConfigurePhase};
use talaria_core::directory::state::{PickerAction, PickerInput, PickerMode, PickerState};
use talaria_core::fleet::ProfileStatus;
use talaria_core::fleet_attention::FleetAttentionConfig;
use talaria_core::fleet_view::{FleetAction, FleetViewInput, FleetViewState, Focus as FleetFocus};
use talaria_core::models::{ConfirmKey, PickerKey, PickerRow, PickerView};
use talaria_core::mutation::Confirmation;
use talaria_core::permissions::{ApprovalSegmentText, ApprovalTone};
use talaria_core::profiles::{
    KeyAction as ProfilesKey, Panel as ProfilesPanel, ProfilesInput, ProfilesView, RawEdit,
};
use talaria_core::providers::{ProvidersEffect, ProvidersView};
use talaria_core::queue::{AdmissionId, QueueIntent, QueueView};
use talaria_core::review::{ReviewFocus, ReviewKey, ReviewView};
use talaria_core::session::{
    AssistantCell, Cell, CellId, PromptCell, ReasoningCell, TurnState as SessionTurn,
};
use talaria_core::subagents::{Clock as SubagentClock, FinishedBatch};
use talaria_tui::automation::{AutomationSection, AutomationTheme};
use talaria_tui::background::{BackgroundSection, BackgroundTheme, ProcessesView};
use talaria_tui::checklist::{
    ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS, SHORT_TERMINAL_ROWS, committed_block_lines,
};
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome, VimMode, clean_text};
use talaria_tui::context_view::{
    CompactingSection, ContextStyle, compact_block, context_block, error_block, notice_block,
    prebuild_block, usage_block,
};
use talaria_tui::fleet_view::{ExternalPeekPanel, FleetTheme, FleetView, PeekPanel};
use talaria_tui::history::{CellLines, PlainCellLines};
use talaria_tui::history_search::{HistorySearch, SearchOutcome};
use talaria_tui::keys::{
    Action, CompletionAction, Key, KeyCode, KeyContext, KeyKind, KeyMap, ModelAction,
};
use talaria_tui::launch_dialog::{DialogAction, DialogDefaults, DialogMode, LaunchDialog};
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::markdown::cells::MarkdownCellLines;
use talaria_tui::markdown::section::MarkdownTailSection;
use talaria_tui::markdown::{MarkdownOptions, MarkdownTheme};
use talaria_tui::mention::MentionSection;
use talaria_tui::model_picker::{ModelConfirmSection, draw_model_picker};
use talaria_tui::pager::{
    ConnectionBanner, Details, Pager, PagerAction, PagerCellLines, PagerContext, PagerTheme,
    SearchIndex, SearchInput,
};
use talaria_tui::palette::PaletteSection;
use talaria_tui::profiles::{
    ProfilesRenderer, cursor_position as profiles_cursor, key_action as profiles_key,
};
use talaria_tui::prompts::{KeyOutcome, MaskedView, PromptIntent, PromptStack};
use talaria_tui::queue::{composer_title, handle_selection_key, render_preview};
use talaria_tui::review_view::draw_review;
use talaria_tui::rewind::{RewindTheme, draw_rewind, key_to_rewind, visible_rows};
use talaria_tui::session_picker::{PickerTheme, SessionPickerView};
use talaria_tui::side::{SideTasksSection, SideTheme, shell_mode_title};
use talaria_tui::status::{
    ApprovalsSegment, CommandLimits, CommandResult, CommandSpec, Connection, ExternalDisplay,
    SessionRef, StatusCommandService, StatusDocument, StatusInputs, StatusPalette, StatusSection,
    Tone, TurnState, interval_from_config, parse_command,
};
use talaria_tui::subagents::{
    AgentsPanel, PanelHeader, PanelMode, SubagentDock, SubagentStyle, TailView, batch_block_lines,
};
use talaria_tui::terminal::{
    InlineTerminal, InputThread, TerminalError, TerminalGuard, probe_startup, spawn_input,
    terminal_size,
};
use talaria_tui::theme::cells::{ThemedCells, themed_cell_id};
use talaria_tui::theme::detect::{self, EnvView};
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors, Theme,
    ThemeInputs, ThemeMode,
};
use talaria_tui::tool_cell::{
    Detail, DraftingLine, ToolCellLines, ToolClock, ToolRenderOptions, ToolTheme,
};
use tokio::sync::mpsc;

/// How long the start-up probe waits for the terminal's answers.
const PROBE_DEADLINE: Duration = Duration::from_millis(200);
/// The live region's first height, before the first draw sets it.
const LIVE_HEIGHT: u16 = 4;
/// The composer's tallest box, in rows.
const COMPOSER_ROWS: u16 = 10;
/// A long open cell is drawn from at least this many of its last lines.
const PREVIEW_MIN_LINES: usize = 80;
/// At most this many tail rows are shown in the live region while a turn streams.
const MAX_TAIL_ROWS: usize = 15;
/// Shown under the composer until the session is open.
const CONNECTING: &str = "Connecting to Hermes\u{2026}";

/// The plain renderer, for the cell kinds the transcript does not know.
static PLAIN_CELLS: PlainCellLines = PlainCellLines;

/// One raw input for the screen.
pub enum ScreenInput {
    /// The raw soul editor returned; terminal ownership is restored before dispatch.
    ProfileEdited(RawEdit),
    Terminal(crossterm::event::Event),
    Status(CommandResult),
    Edited(editor::EditResult),
    Resumed(suspend::SuspendResult),
    Continued,
    ComfortNotice(String),
}

/// The overlay the terminal shows, if any. The model picker and the session picker share
/// the one overlay the terminal has, so only one of them is open at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    WorkflowProgress,
    Kanban,
    Cron,
    /// The live screen, with no overlay.
    Live,
    /// The profile roster and configuration panels.
    Profiles,
    /// The model picker.
    Models,
    /// The session picker.
    Sessions,
    /// The fleet overlay.
    Fleet,
    /// The subagent tree, details and transcript.
    Agents,
    /// The foreground session's background processes.
    Processes,
    /// A review opened with `/changes`.
    Review,
    /// Profile settings.
    Settings,
    /// The gateway diagnostics and confirmed restart card.
    Doctor,
    Providers,
    /// The profile's model roles.
    ModelRoles,
    Mcp,
    /// Profile skills, toolsets and plugins.
    Capabilities,
    /// Memory providers, files and curator.
    Memory,
    Connectors,
    /// The configuration assistant.
    Configure,
}

fn background_theme(color: bool) -> BackgroundTheme {
    let mut theme = BackgroundTheme::default();
    if !color {
        theme.warning = ratatui::style::Style::default();
        theme.error = ratatui::style::Style::default();
        theme.ok = ratatui::style::Style::default();
    }
    theme
}

fn memory_footer_key(label: &str) -> Option<talaria_core::memory::MemoryKey> {
    use talaria_core::memory::MemoryKey;
    Some(match label {
        "up" => MemoryKey::Up,
        "down" => MemoryKey::Down,
        "enter" => MemoryKey::Open,
        "u" => MemoryKey::Use,
        "d" => MemoryKey::Delete,
        "D" => MemoryKey::DeleteBoth,
        "p" => MemoryKey::Pause,
        "R" => MemoryKey::Run,
        "s" => MemoryKey::Settings,
        "k" => MemoryKey::CuratorSettings,
        "ctrl+r" => MemoryKey::Reload,
        "c" => MemoryKey::Copy,
        "ctrl+s" => MemoryKey::Save,
        "esc" => MemoryKey::Close,
        _ => return None,
    })
}

/// The terminal side of the loop: the inline terminal, the composer, the prompt cards and the
/// status line.
pub struct TuiScreen {
    providers: crate::providers_wiring::ProvidersWiring,
    providers_view: Option<ProvidersView>,
    cron: Option<talaria_core::cron::CronView>,
    cron_gate: talaria_tui::cron::CronFrameGate,
    cron_history_focused: bool,
    cron_dirty: bool,
    guard: TerminalGuard,
    input: Option<InputThread>,
    history: PromptHistory,
    search: Option<HistorySearch>,
    config: config::Config,
    folder: std::path::PathBuf,
    reporter: Option<HerdrReporter>,
    pane: PaneReport,
    report_session: Option<(String, String)>,
    expected: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    continued: Arc<AtomicBool>,
    compact_started: bool,
    terminal: InlineTerminal<Stdout>,
    composer: Composer,
    prompts: PromptStack,
    configure_prompts: PromptStack,
    configure_cancel_at: Option<Instant>,
    status: Option<StatusCommandService>,
    external: ExternalDisplay,
    inputs: StatusInputs,
    palette: StatusPalette,
    /// Markdown cells, with tool cells drawn as tool rows and diffs.
    cells: ToolCellLines,
    /// When each running tool cell was first seen, for its seconds counter.
    clock: ToolClock,
    tool_theme: ToolTheme,
    /// The settings and start-up decisions the theme is resolved from.
    look: Look,
    /// The theme in force for the composer, the status line and cells that start now.
    theme: Arc<Theme>,
    /// The theme each assistant or reasoning cell started under, until it is committed.
    captured: HashMap<CellId, Arc<Theme>>,
    /// The checklist's styles: the same choice of colour the Markdown renderer makes.
    checklist_style: ChecklistStyle,
    color: bool,
    sender: mpsc::UnboundedSender<ScreenInput>,
    connecting: bool,
    /// The key map, for the composer, the completion list and the model picker.
    keymap: KeyMap,
    reported: (String, usize),
    attachments: AttachmentView,
    attachment_rule: CollapseRule,
    /// Terminal rows, for the palette's height cap.
    rows: u16,
    picker: Option<ModelOverlay>,
    profiles: ProfilesRenderer,
    overlay: Overlay,
    mcp_view: Option<talaria_core::mcp::McpView>,
    mcp_wiring: crate::mcp_wiring::McpWiring,
    connectors_wiring: crate::connectors_wiring::ConnectorsWiring,
    connectors_ssh: bool,
    /// The card of the peeked background session, while the fleet overlay shows it.
    peek_prompts: PromptStack,
    acp_option: usize,
    acp_responder: Option<String>,
    agents: Option<AgentsPanel>,
    agents_flash: Option<String>,
    agents_pause_pending: bool,
    processes: ProcessesView,
    /// The launch dialog over the fleet overlay; it owns every key while it is open.
    launch_dialog: Option<LaunchDialog>,
    /// The prompt kept from the last launch dialog closed with Esc.
    launch_draft: Option<String>,
    /// The version of the app's dialog status already shown.
    launch_seen: u64,
    confirmation: Option<Confirmation>,
    confirmation_focused: Option<Instant>,
    /// The rewind overlay holds the alternate screen. Issue #233.
    rewind_open: bool,
    /// The list rows last reported to the loop, so paging moves by what is visible.
    rewind_rows: u16,
    rewind_theme: RewindTheme,
    /// The transcript screen, while it is open.
    transcript: Option<Transcript>,
    /// What the application asked of the transcript screen.
    transcript_actions: Vec<TranscriptAction>,
    /// The queued row whose edit the composer holds, once its text is loaded (#232).
    editing: Option<AdmissionId>,
    /// The person saved or cancelled the edit; clear the composer when it closes.
    edit_closing: bool,
}

struct ModelOverlay {
    title: String,
    filter: String,
    effort: String,
    hints: String,
    refreshing: bool,
    rows: Vec<PickerRow>,
    selected: Option<usize>,
    banner: Option<String>,
}

impl ModelOverlay {
    fn from_view(view: PickerView<'_>) -> Self {
        Self {
            title: view.title.into(),
            filter: view.filter.into(),
            effort: view.effort.into(),
            hints: view.hints.into(),
            refreshing: view.refreshing,
            rows: view.rows.to_vec(),
            selected: view.selected,
            banner: view.banner.map(String::from),
        }
    }

    fn view(&self) -> PickerView<'_> {
        PickerView {
            title: &self.title,
            filter: &self.filter,
            effort: &self.effort,
            hints: &self.hints,
            refreshing: self.refreshing,
            rows: &self.rows,
            selected: self.selected,
            banner: self.banner.as_deref(),
        }
    }
}

fn cron_key(
    keymap: &KeyMap,
    key: Key,
    history_focused: bool,
    question: bool,
) -> Option<talaria_core::cron::CronKey> {
    use talaria_core::cron::{CronAction, CronKey};
    if key.kind == KeyKind::Release {
        return None;
    }
    let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
    if plain {
        let fixed = match key.code {
            KeyCode::Tab if key.mods.shift => Some(CronKey::BackTab),
            KeyCode::Tab => Some(CronKey::Tab),
            KeyCode::Esc => Some(CronKey::Esc),
            KeyCode::Enter => Some(CronKey::Enter),
            KeyCode::Backspace => Some(CronKey::Backspace),
            KeyCode::Delete => Some(CronKey::Delete),
            KeyCode::Left => Some(CronKey::Left),
            KeyCode::Right => Some(CronKey::Right),
            KeyCode::Up => Some(CronKey::Up),
            KeyCode::Down => Some(CronKey::Down),
            KeyCode::Home => Some(CronKey::Home),
            KeyCode::End => Some(CronKey::End),
            KeyCode::PageUp => Some(CronKey::PageUp),
            KeyCode::PageDown => Some(CronKey::PageDown),
            KeyCode::Char(c @ ('y' | 'n')) if question => Some(CronKey::Char(c)),
            _ => None,
        };
        if fixed.is_some() {
            return fixed;
        }
    }
    if let Some(action) = keymap.resolve_cron(KeyContext::Cron, &key)
        && (action != CronAction::OpenRun || history_focused)
    {
        return Some(CronKey::Action(action));
    }
    match key.code {
        KeyCode::Char(c) if plain && !c.is_control() => Some(CronKey::Char(c)),
        _ => None,
    }
}

fn picker_key(keymap: &KeyMap, key: Key) -> Option<PickerKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    if let Some(action) = keymap.resolve_model(KeyContext::ModelPicker, &key) {
        return match action {
            ModelAction::EffortDown => Some(PickerKey::Left),
            ModelAction::EffortUp => Some(PickerKey::Right),
            ModelAction::ModelScope => Some(PickerKey::ToggleScope),
            ModelAction::ModelFavourite => Some(PickerKey::ToggleFavourite),
            ModelAction::ModelRefresh => Some(PickerKey::Refresh),
            ModelAction::ModelPicker => None,
        };
    }
    let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
    if plain {
        return match key.code {
            KeyCode::Char(c) if !c.is_control() => Some(PickerKey::Char(c)),
            KeyCode::Enter if !key.mods.shift => Some(PickerKey::Enter),
            KeyCode::Esc if !key.mods.shift => Some(PickerKey::Escape),
            KeyCode::Backspace if !key.mods.shift => Some(PickerKey::Backspace),
            KeyCode::Up if !key.mods.shift => Some(PickerKey::Up),
            KeyCode::Down if !key.mods.shift => Some(PickerKey::Down),
            KeyCode::PageUp if !key.mods.shift => Some(PickerKey::PageUp),
            KeyCode::PageDown if !key.mods.shift => Some(PickerKey::PageDown),
            KeyCode::Home if !key.mods.shift => Some(PickerKey::Home),
            KeyCode::End if !key.mods.shift => Some(PickerKey::End),
            _ => None,
        };
    }
    if key.mods.ctrl && !key.mods.alt && !key.mods.shift && !key.mods.super_key && !key.mods.other {
        return match key.code {
            KeyCode::Char('c') => Some(PickerKey::Close),
            KeyCode::Char('u') => Some(PickerKey::ClearFilter),
            _ => None,
        };
    }
    None
}

/// Route only keys that the current immutable attachment view can consume.
fn attachment_key(
    keymap: &KeyMap,
    key: &Key,
    view: &AttachmentView,
    draft: &str,
    caret: usize,
) -> Option<AttachmentAction> {
    if key.kind == KeyKind::Release {
        return None;
    }
    let bound = |action| keymap.chords(action).iter().any(|chord| chord.matches(key));
    if !draft.starts_with('/')
        && let Some(mention) = &view.mention
    {
        for (key_action, action) in [
            (Action::CompletionNext, AttachmentAction::CompletionNext),
            (Action::CompletionPrev, AttachmentAction::CompletionPrev),
            (Action::CompletionAccept, AttachmentAction::CompletionAccept),
            (Action::CompletionClose, AttachmentAction::CompletionClose),
        ] {
            if bound(key_action) {
                return Some(action);
            }
        }
        if !mention.rows.is_empty() && bound(Action::Submit) {
            return Some(AttachmentAction::Submit);
        }
    }
    if bound(Action::AttachClipboardImage) {
        return Some(AttachmentAction::AttachClipboardImage);
    }
    for (key_action, action) in [
        (Action::DeleteBackward, AttachmentAction::DeleteBackward),
        (Action::DeleteForward, AttachmentAction::DeleteForward),
    ] {
        if bound(key_action)
            && view.highlights.iter().any(|(range, _)| {
                draft.get(range.clone()).is_some()
                    && if key_action == Action::DeleteBackward {
                        range.end == caret
                    } else {
                        range.start == caret
                    }
            })
        {
            return Some(action);
        }
    }
    None
}

/// The picker's action for a key map action, when the two name the same thing.
///
/// The picker's own actions carry the `Picker` prefix in the key map; the state machine
/// names them without it.
fn picker_action(action: Action) -> Option<PickerAction> {
    Some(match action {
        Action::PickerUp => PickerAction::Up,
        Action::PickerDown => PickerAction::Down,
        Action::PickerPageUp => PickerAction::PageUp,
        Action::PickerPageDown => PickerAction::PageDown,
        Action::PickerTop => PickerAction::Top,
        Action::PickerBottom => PickerAction::Bottom,
        Action::PickerOpen => PickerAction::Open,
        Action::PickerRename => PickerAction::Rename,
        Action::PickerBranch => PickerAction::Branch,
        Action::PickerExport => PickerAction::Export,
        Action::PickerSearch => PickerAction::Search,
        Action::PickerProfileNext => PickerAction::ProfileNext,
        Action::PickerProfilePrev => PickerAction::ProfilePrev,
        Action::PickerRefresh => PickerAction::Refresh,
        Action::PickerClose => PickerAction::Close,
        Action::PickerFormatMarkdown => PickerAction::FormatMarkdown,
        Action::PickerFormatJson => PickerAction::FormatJson,
        Action::PickerFormatHtml => PickerAction::FormatHtml,
        Action::PickerToggleRedact => PickerAction::ToggleRedact,
        _ => return None,
    })
}

/// The character a key types, when it types one: a printable key without Ctrl or Alt.
fn typed_character(key: &Key) -> Option<char> {
    if key.mods.ctrl || key.mods.alt {
        return None;
    }
    match key.code {
        KeyCode::Char(character) if !character.is_control() => Some(character),
        _ => None,
    }
}

/// The editing key a key is, when it is one: the keys the search line and the rename
/// editor keep for themselves.
fn editing_key(key: &Key) -> Option<PickerInput> {
    Some(match key.code {
        KeyCode::Backspace => PickerInput::Backspace,
        KeyCode::Delete => PickerInput::Delete,
        KeyCode::Left => PickerInput::Left,
        KeyCode::Right => PickerInput::Right,
        KeyCode::Home => PickerInput::Home,
        KeyCode::End => PickerInput::End,
        _ => return None,
    })
}

/// Whether the key is the escape that closes the overlay.
fn is_escape(key: &Key) -> bool {
    key.code == KeyCode::Esc && !key.mods.ctrl && !key.mods.alt
}

/// Whether the key is the Ctrl+C that closes the overlay.
fn is_ctrl_c(key: &Key) -> bool {
    key.mods.ctrl
        && !key.mods.alt
        && !key.mods.super_key
        && !key.mods.other
        && matches!(key.code, KeyCode::Char('c' | 'C'))
}

/// The review's key for a key map action.
fn review_action(action: Action) -> Option<ReviewKey> {
    Some(match action {
        Action::ReviewUp => ReviewKey::Up,
        Action::ReviewDown => ReviewKey::Down,
        Action::ReviewTop => ReviewKey::Top,
        Action::ReviewBottom => ReviewKey::Bottom,
        Action::ReviewShowDiff => ReviewKey::ShowDiff,
        Action::ReviewDiffPageUp => ReviewKey::PageUp,
        Action::ReviewDiffPageDown => ReviewKey::PageDown,
        Action::ReviewScope => ReviewKey::Scope,
        Action::ReviewStage => ReviewKey::StageToggle,
        Action::ReviewStageAll => ReviewKey::StageAll,
        Action::ReviewUnstageAll => ReviewKey::UnstageAll,
        Action::ReviewDiscard => ReviewKey::Discard,
        Action::ReviewCommit => ReviewKey::Commit,
        Action::ReviewPush => ReviewKey::Push,
        Action::ReviewPullRequest => ReviewKey::PullRequest,
        Action::ReviewOpenPr => ReviewKey::OpenPr,
        Action::ReviewRefresh => ReviewKey::Refresh,
        Action::CommitDraft => ReviewKey::Draft,
        Action::CommitSubmit => ReviewKey::Submit,
        _ => return None,
    })
}

/// The fleet overlay's action for a key map action.
fn fleet_action(action: Action) -> Option<FleetAction> {
    Some(match action {
        Action::FleetUp => FleetAction::Up,
        Action::FleetDown => FleetAction::Down,
        Action::FleetPageUp => FleetAction::PageUp,
        Action::FleetPageDown => FleetAction::PageDown,
        Action::FleetTop => FleetAction::Top,
        Action::FleetBottom => FleetAction::Bottom,
        Action::FleetOpen => FleetAction::Open,
        Action::FleetOpenInPane => FleetAction::OpenInPane,
        Action::FleetPeek => FleetAction::Peek,
        Action::FleetReply => FleetAction::Reply,
        Action::FleetStop => FleetAction::Stop,
        Action::FleetAnswer => FleetAction::Answer,
        Action::FleetFilter => FleetAction::Filter,
        Action::FleetProfileNext => FleetAction::ProfileNext,
        Action::FleetProfilePrev => FleetAction::ProfilePrev,
        Action::FleetNew => FleetAction::New,
        Action::FleetNewWorktree => FleetAction::NewInWorktree,
        Action::FleetReview => FleetAction::Review,
        _ => return None,
    })
}

/// A card intent in the peek's terms.
fn card_answer(intent: PromptIntent) -> CardAnswer {
    match intent {
        PromptIntent::Approve { key, choice, note } => CardAnswer::Approve { key, choice, note },
        PromptIntent::Clarify { key, answer } => CardAnswer::Clarify { key, answer },
        PromptIntent::Masked { request_id, value } => CardAnswer::Masked {
            request_id,
            value: value.into_inner().to_string(),
        },
    }
}

fn fleet_peek_is_foreground(
    peeked: &talaria_core::directory::rows::RowKey,
    profile: &str,
    session_id: Option<&str>,
) -> bool {
    peeked.profile == profile && Some(peeked.session_id.as_str()) == session_id
}

fn fleet_paste(
    focus: &FleetFocus,
    foreground: bool,
    prompts: &mut PromptStack,
    peek_prompts: &mut PromptStack,
    text: String,
    now: Instant,
) -> Vec<UserAction> {
    match focus {
        FleetFocus::Reply | FleetFocus::Filter => {
            vec![UserAction::Fleet(FleetViewInput::Paste(text))]
        }
        FleetFocus::Answer => {
            let stack = if foreground { prompts } else { peek_prompts };
            fleet_card_actions(stack.handle_paste(&text, now))
        }
        _ => Vec::new(),
    }
}

fn fleet_card_actions(outcome: KeyOutcome) -> Vec<UserAction> {
    let KeyOutcome::Consumed {
        intent,
        interrupt_requested,
        notice,
    } = outcome
    else {
        return Vec::new();
    };
    let mut actions = vec![UserAction::FleetAnswerEdited];
    if let Some(intent) = intent {
        actions.push(UserAction::FleetAnswer(card_answer(intent)));
    }
    if interrupt_requested {
        actions.push(UserAction::Fleet(FleetViewInput::CtrlC));
    }
    if let Some(notice) = notice {
        actions.push(UserAction::Notice(notice));
    }
    actions
}

/// The picker's input for one key, in the mode the picker is in.
///
/// The mode decides the rules. The list reads the picker's own key context; the export
/// dialog reads the export context and nothing else; the search line and the rename
/// editor take a printable key as text and keep their editing keys. Esc and Ctrl+C are
/// fixed in every mode: they close the overlay and never reach the agent.
fn picker_input(keymap: &KeyMap, key: Key, mode: &PickerMode) -> Option<PickerInput> {
    if key.kind == KeyKind::Release {
        return None;
    }
    if is_escape(&key) {
        return Some(PickerInput::Escape);
    }
    if is_ctrl_c(&key) {
        return Some(PickerInput::CtrlC);
    }
    match mode {
        PickerMode::Waiting(_) => None,
        PickerMode::List => keymap
            .resolve_in(&key, KeyContext::SessionPicker)
            .and_then(picker_action)
            .map(PickerInput::Action)
            .or_else(|| editing_key(&key)),
        PickerMode::Search => {
            if let Some(character) = typed_character(&key) {
                return Some(PickerInput::Text(character));
            }
            // Enter keeps the query and returns to the list, so it never opens a row.
            if key.code == KeyCode::Enter {
                return Some(PickerInput::Enter);
            }
            keymap
                .resolve_in(&key, KeyContext::SessionPicker)
                .and_then(picker_action)
                .map(PickerInput::Action)
                .or_else(|| editing_key(&key))
        }
        PickerMode::Rename(_) => {
            if let Some(character) = typed_character(&key) {
                return Some(PickerInput::Text(character));
            }
            if key.code == KeyCode::Enter {
                return Some(PickerInput::Enter);
            }
            editing_key(&key)
        }
        PickerMode::Export(_) => {
            if key.code == KeyCode::Enter {
                return Some(PickerInput::Enter);
            }
            keymap
                .resolve_in(&key, KeyContext::SessionPickerExport)
                .and_then(picker_action)
                .map(PickerInput::Action)
        }
    }
}

/// The picker's input for a paste, in the mode the picker is in.
///
/// Only the search line and the rename editor take a paste, and only its first line:
/// a pasted title is one line.
fn picker_paste(mode: &PickerMode, text: &str) -> Option<PickerInput> {
    match mode {
        PickerMode::Search | PickerMode::Rename(_) => {
            let first = text.lines().next().unwrap_or_default();
            Some(PickerInput::Paste(first.to_owned()))
        }
        _ => None,
    }
}

/// The current time as a Unix timestamp in seconds, for the picker's age column.
fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

fn confirmation_key(key: Key, focused: Option<Instant>, now: Instant) -> Option<ConfirmKey> {
    if key.kind == KeyKind::Release
        || key.mods.ctrl
        || key.mods.alt
        || key.mods.super_key
        || key.mods.other
        || key.mods.shift
    {
        return None;
    }
    if !focused.is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(250)) {
        return None;
    }
    match key.code {
        KeyCode::Enter | KeyCode::Char('y') => Some(ConfirmKey::Accept),
        KeyCode::Esc | KeyCode::Char('n') => Some(ConfirmKey::Decline),
        _ => None,
    }
}

/// What the theme is resolved from besides the skin: decided once at start-up.
#[derive(Clone, Copy, Debug)]
struct Look {
    follow_hermes_skin: bool,
    depth: (ColorDepth, DepthSource),
    motion: (Motion, MotionSource),
    /// The start-up probe colours. The re-probe after an editor or a resume does not change them.
    terminal: TerminalColors,
    polarity: (Polarity, PolaritySource),
}

impl Look {
    fn resolve(&self, view: &AppearanceView<'_>) -> Theme {
        talaria_tui::theme::resolve(&ThemeInputs {
            skin: view.skin,
            skipped: view.skipped,
            follow_hermes_skin: self.follow_hermes_skin,
            polarity: view.polarity,
            depth: self.depth,
            motion: self.motion,
            terminal: self.terminal,
        })
    }
}

/// The tool cell renderer for a theme: tool rows in the theme's tool styles, other cells plain.
/// A cut preview names the key that opens the transcript, when one is bound.
fn tool_cells(theme: &Theme, keymap: &KeyMap) -> ToolCellLines {
    ToolCellLines::new(
        Box::new(MarkdownCellLines::new(true, MarkdownTheme::default())),
        ToolRenderOptions {
            width: 0,
            theme: theme.tool.clone(),
            color: theme.color,
            detail: Detail::Compact,
            expand_hint: keymap
                .label(Action::TranscriptOpen)
                .map(|label| format!("{label} to view")),
            elapsed: None,
        },
    )
}

/// Committed cells drawn with the theme each one started under.
struct CapturedCells<'a> {
    current: &'a Theme,
    captured: &'a HashMap<CellId, Arc<Theme>>,
    /// Draws tool cells; every other cell goes through the theme.
    tools: &'a ToolCellLines,
}

impl CellLines for CapturedCells<'_> {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        if matches!(cell, Cell::Tool(_)) {
            return self.tools.lines(cell, width);
        }
        if let Cell::Background(cell) = cell {
            return talaria_tui::background::background_cell_lines(
                cell,
                width,
                &background_theme(self.current.color),
            );
        }
        let theme = themed_cell_id(cell)
            .and_then(|id| self.captured.get(&id))
            .map_or(self.current, |theme| theme.as_ref());
        ThemedCells { theme }.lines(cell, width)
    }
}

/// Load a history entry into the composer and report it as a recall, so the paste placeholders
/// it holds get their text back.
fn recall_draft(
    composer: &mut Composer,
    reported: &mut (String, usize),
    text: String,
    now: Instant,
) -> UserAction {
    composer.load(text);
    *reported = (composer.text().to_owned(), composer.caret());
    UserAction::DraftEdited {
        text: reported.0.clone(),
        caret: reported.1,
        typed: false,
        recalled: true,
        now,
    }
}

/// The draft once no burst is held back. The caller passes it to the loop, so it also becomes
/// the reported draft and the next key does not report it again.
fn reported_snapshot(
    composer: &Composer,
    reported: &mut (String, usize),
) -> Option<(String, usize)> {
    let snapshot = (!composer.is_collecting_burst())
        .then(|| (composer.text().to_string(), composer.caret()))?;
    reported.clone_from(&snapshot);
    Some(snapshot)
}

fn flush_draft_actions(composer: &mut Composer, now: Instant) -> Vec<UserAction> {
    let before = (composer.text().to_string(), composer.caret());
    composer.flush_pending();
    if before == (composer.text().to_string(), composer.caret()) {
        return Vec::new();
    }
    vec![UserAction::DraftEdited {
        text: composer.text().to_string(),
        caret: composer.caret(),
        typed: true,
        recalled: false,
        now,
    }]
}

fn paste_actions(composer: &mut Composer, text: &str, now: Instant) -> Vec<UserAction> {
    let mut actions = flush_draft_actions(composer, now);
    actions.push(UserAction::Paste {
        text: clean_text(text),
        draft: composer.text().to_string(),
        caret: composer.caret(),
    });
    actions
}

fn composer_key_actions(
    composer: &mut Composer,
    key: Key,
    ctx: ComposerContext,
    now: Instant,
) -> Vec<UserAction> {
    let before = (composer.text().to_string(), composer.caret());
    let outcome = composer.handle_key(key, ctx, now);
    if let ComposerOutcome::Submit(text) = outcome {
        return vec![UserAction::Submit(text)];
    }
    let eligible = outcome == ComposerOutcome::Unchanged
        && composer.is_empty()
        && !ctx.turn_running
        && !composer.vim_enabled();
    let mut actions = match outcome {
        ComposerOutcome::Interrupt => vec![UserAction::Interrupt],
        ComposerOutcome::ExitRequested => vec![UserAction::Quit],
        _ => Vec::new(),
    };
    if key.kind != KeyKind::Release && key.code == KeyCode::Esc {
        actions.push(UserAction::Escape { eligible });
        // Two quick Esc bytes can arrive as one Esc with Alt held: count it twice.
        if eligible && key.mods.alt {
            actions.push(UserAction::Escape { eligible });
        }
    }
    if !composer.is_collecting_burst()
        && (before != (composer.text().to_string(), composer.caret())
            || composer.last_history_recall())
    {
        actions.push(UserAction::DraftEdited {
            text: composer.text().to_string(),
            caret: composer.caret(),
            typed: true,
            recalled: composer.last_history_recall(),
            now,
        });
    }
    actions
}

/// The transcript screen and the search text of the final cells.
struct Transcript {
    pager: Pager,
    index: SearchIndex,
    dirty: bool,
}

/// The key of one terminal event, in the form the application routes.
fn input_key(event: &crossterm::event::KeyEvent) -> Option<InputKey> {
    let key = Key::from(*event);
    if key.kind != KeyKind::Press {
        return None;
    }
    let code = match key.code {
        KeyCode::Char(character) => InputCode::Char(character),
        KeyCode::Enter => InputCode::Enter,
        KeyCode::Esc => InputCode::Esc,
        KeyCode::Backspace => InputCode::Backspace,
        KeyCode::Tab => InputCode::Tab,
        KeyCode::Delete => InputCode::Delete,
        KeyCode::Insert => InputCode::Insert,
        KeyCode::Up => InputCode::Up,
        KeyCode::Down => InputCode::Down,
        KeyCode::Left => InputCode::Left,
        KeyCode::Right => InputCode::Right,
        KeyCode::PageUp => InputCode::PageUp,
        KeyCode::PageDown => InputCode::PageDown,
        KeyCode::Home => InputCode::Home,
        KeyCode::End => InputCode::End,
        KeyCode::F(number) => InputCode::F(number),
        _ => return None,
    };
    Some(InputKey {
        code,
        ctrl: key.mods.ctrl,
        alt: key.mods.alt,
        shift: key.mods.shift,
        super_key: key.mods.super_key,
        other: key.mods.other,
    })
}

fn masked_method(kind: MaskedKind) -> &'static str {
    match kind {
        MaskedKind::Sudo => "sudo",
        MaskedKind::Secret => "secret",
        MaskedKind::VaultCode => "vault.code",
        MaskedKind::VaultUnlockPrompt => "vault.unlock_prompt",
    }
}

fn masked_views(masked: &[MaskedRequestInfo]) -> Vec<MaskedView> {
    masked
        .iter()
        .map(|info| MaskedView {
            request_id: info.id.clone(),
            session_id: info.session_id.clone(),
            method: masked_method(info.kind).to_string(),
            fields: info
                .fields
                .iter()
                .map(|(name, value)| ((*name).to_string(), value.clone()))
                .collect(),
        })
        .collect()
}

/// The last lines of a long open assistant or reasoning cell, for the live tail only.
///
/// The tail section shows the bottom rows alone, so rendering the whole text on every frame
/// only makes a fast stream fall behind. The cut starts at a blank line when the kept lines
/// hold one, and is skipped when the text holds a code fence or the kept lines a table row,
/// whose rows depend on lines above the cut. Committed cells are always rendered whole.
fn preview(cell: &Cell, limit: usize) -> Option<Cell> {
    let text = match cell {
        Cell::Assistant(inner) => &inner.text,
        Cell::Reasoning(inner) => &inner.text,
        _ => return None,
    };
    let tail = preview_text(text, limit)?;
    Some(match cell {
        Cell::Assistant(inner) => Cell::Assistant(AssistantCell {
            text: tail,
            ..inner.clone()
        }),
        Cell::Reasoning(inner) => Cell::Reasoning(ReasoningCell {
            text: tail,
            ..inner.clone()
        }),
        _ => return None,
    })
}

/// The text `preview` draws: the last `limit` lines, from a blank line when they hold one.
fn preview_text(text: &str, limit: usize) -> Option<String> {
    if text.contains("```") || text.contains("~~~") {
        return None;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() <= limit {
        return None;
    }
    let mut kept = &lines[lines.len() - limit..];
    if let Some(blank) = kept.iter().position(|line| line.trim().is_empty())
        && kept[blank + 1..].iter().any(|line| !line.trim().is_empty())
    {
        kept = &kept[blank + 1..];
    }
    if kept.iter().any(|line| line.trim_start().starts_with('|')) {
        return None;
    }
    Some(kept.join("\n"))
}

/// What the queue section draws for the open session (#232).
fn queue_view(view: &View<'_>) -> Option<QueueView> {
    view.state
        .map(|state| state.queue.view(view.turn_running()))
}

fn queue_draft_intent(composer: &Composer, bound: Option<Action>) -> Option<QueueIntent> {
    let draft = composer.text().to_owned();
    let intent = match bound {
        Some(Action::ComposerSendNow) if !draft.trim().is_empty() => {
            QueueIntent::SendDraftNow(draft)
        }
        Some(Action::ComposerSteer) if !draft.trim().is_empty() => QueueIntent::SteerText(draft),
        _ => return None,
    };
    Some(intent)
}

fn clear_queue_draft(
    composer: &mut Composer,
    reported: &mut (String, usize),
    text: &str,
) -> Option<(String, usize)> {
    if composer.text() != text {
        return None;
    }
    composer.clear();
    reported_snapshot(composer, reported)
}

fn connectors_editor_key(key: Key) -> Option<talaria_core::connectors::ConnectorsKey> {
    use talaria_core::connectors::ConnectorsKey as K;
    let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
    match key.code {
        KeyCode::Char('u')
            if key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other =>
        {
            Some(K::ClearField)
        }
        KeyCode::Char(ch) if plain => Some(K::Char(ch)),
        KeyCode::Backspace if plain => Some(K::Backspace),
        KeyCode::Enter if plain => Some(K::Open),
        KeyCode::Esc if plain => Some(K::Close),
        _ => None,
    }
}

fn status_inputs(view: &View<'_>, home_dir: Option<String>) -> StatusInputs {
    let open = view.state.map_or(0, |state| state.open_prompts().count());
    let context_percent = if view.context_hidden {
        None
    } else {
        view.state
            .and_then(|state| state.usage().context_percent)
            .map(|percent| percent as i64)
    };
    let open = u32::try_from(open).unwrap_or(u32::MAX);
    let turn = match view.state.map(|state| state.turn()) {
        Some(SessionTurn::Running) if open > 0 => TurnState::Waiting,
        Some(SessionTurn::Running) => TurnState::Streaming,
        _ => TurnState::Idle,
    };
    let counts = view
        .subagents
        .and_then(|hub| hub.foreground_key().and_then(|key| hub.known_counts(key)));
    StatusInputs {
        ultracode: view.ultracode,
        connection: match view.link {
            LinkView::Connecting => Connection::Connecting,
            LinkView::Connected => Connection::Connected,
            LinkView::Reconnecting => Connection::Reconnecting,
            LinkView::Failed | LinkView::Closed => Connection::Disconnected,
        },
        model: view.model.map(str::to_string),
        reasoning_effort: view.effort.map(str::to_string),
        reasoning_effort_wire: view.effort.map(str::to_string),
        // One folder for the foreground session, kept by the app and moved by `/cd` (#237).
        working_dir: view.folder.and_then(|folder| folder.cwd.clone()),
        home_dir,
        branch: view.folder.and_then(|folder| folder.branch.clone()),
        needs_you: view.fleet_counts.needs_you,
        working: view
            .fleet_counts
            .working
            .saturating_add(u32::from(turn != TurnState::Idle)),
        session: view.session_id.map(|id| SessionRef {
            id: id.to_string(),
            title: view.title.map(str::to_string),
        }),
        turn,
        pending_prompts: open,
        subagents_active: counts.map_or(view.subagents_usage_active, |counts| counts.live),
        subagents_terminal: counts.map_or(0, |counts| counts.terminal),
        failed: view.fleet_counts.failed,
        done: view.fleet_counts.done,
        context_percent,
        approvals: view.approvals.as_ref().map(approvals_segment),
        pull_request: view.pull_request.clone(),
        ..StatusInputs::default()
    }
}

/// The permissions state's segment in the status line's own types (#237).
fn approvals_segment(segment: &ApprovalSegmentText) -> ApprovalsSegment {
    ApprovalsSegment {
        text: segment.text.clone(),
        tone: match segment.tone {
            ApprovalTone::Muted => Tone::Muted,
            ApprovalTone::Warning => Tone::Warning,
        },
    }
}

impl TuiScreen {
    fn connectors_key(
        &mut self,
        key: Key,
        view: &talaria_core::connectors::ConnectorsView,
        filtering: bool,
    ) -> Vec<UserAction> {
        use talaria_core::connectors::{ConnectorsInput, ConnectorsKey as K, Screen as S};
        if key.kind == KeyKind::Release {
            return Vec::new();
        }
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
        if matches!(view.screen, S::SecretEditor { .. }) {
            let input = match key.code {
                KeyCode::Enter if plain => self
                    .connectors_wiring
                    .accept_secret_edit()
                    .then_some(ConnectorsInput::Key(K::Open)),
                KeyCode::Esc if plain => {
                    self.connectors_wiring.cancel_secret_edit();
                    Some(ConnectorsInput::Key(K::Close))
                }
                _ => self.connectors_wiring.route_secret_key(&key),
            };
            return input.map(UserAction::Connectors).into_iter().collect();
        }
        let confirming = matches!(view.screen, S::Confirm { .. } | S::Review { .. });
        let editing = matches!(view.screen, S::Prompt { .. }) || filtering;
        let action = if confirming {
            if plain && key.code == KeyCode::Esc {
                Some(K::Close)
            } else {
                self.keymap
                    .resolve_connectors(KeyContext::Connectors, &key)
                    .filter(|action| {
                        matches!(action, K::Confirm | K::Cancel | K::Close)
                            || (matches!(view.screen, S::Review { .. })
                                && matches!(action, K::Up | K::Down))
                    })
            }
        } else if editing {
            connectors_editor_key(key)
        } else {
            self.keymap
                .resolve_connectors(KeyContext::Connectors, &key)
                .filter(|action| {
                    !matches!(view.screen, S::Consent { .. })
                        || matches!(
                            action,
                            K::Open | K::Close | K::Cancel | K::NextTab | K::PrevTab
                        )
                })
        };
        if let Some(action) = action
            && let Some(offset) = talaria_tui::connectors::review_scroll_target(
                view,
                action,
                self.terminal.screen_area(),
            )
        {
            return vec![UserAction::Connectors(ConnectorsInput::ReviewScroll(
                offset,
            ))];
        }
        action
            .map(|key| UserAction::Connectors(self.connectors_wiring.route_key(key)))
            .into_iter()
            .collect()
    }
    fn confirmation_takes_keys(&self) -> bool {
        self.overlay == Overlay::Live
            && self.confirmation.is_some()
            && self.prompts.section().is_none()
            && self.composer.is_empty()
    }

    fn sync_confirmation_focus(&mut self, now: Instant) {
        if self.confirmation_takes_keys() {
            self.confirmation_focused.get_or_insert(now);
        } else {
            self.confirmation_focused = None;
        }
    }

    /// A key while the rewind overlay is open: it takes every key. The list height is reported
    /// first whenever it changed, so paging moves by the rows on screen.
    fn rewind_key(
        &mut self,
        raw: &crossterm::event::KeyEvent,
        screen: talaria_core::rewind::RewindScreen,
    ) -> Vec<UserAction> {
        let mut actions = Vec::new();
        let width = self.terminal.viewport().width;
        let rows = visible_rows(ratatui::layout::Rect::new(0, 0, width, self.rows));
        if rows != self.rewind_rows {
            self.rewind_rows = rows;
            actions.push(UserAction::RewindViewport(rows));
        }
        if raw.kind != crossterm::event::KeyEventKind::Release
            && let Some(key) = key_to_rewind(raw, screen)
        {
            actions.push(UserAction::Rewind(key));
        }
        actions
    }

    /// One key for the fleet overlay, by its focus.
    fn fleet_key(
        &mut self,
        key: Key,
        fleet: &FleetViewState,
        view: &View<'_>,
        now: Instant,
    ) -> Vec<UserAction> {
        if key.kind == KeyKind::Release {
            return Vec::new();
        }
        let input = |input: FleetViewInput| vec![UserAction::Fleet(input)];
        match fleet.focus() {
            FleetFocus::Answer => {
                // Ctrl+C goes back to the list; every other key, Esc included, is the card's.
                if is_ctrl_c(&key) {
                    return input(FleetViewInput::CtrlC);
                }
                if let Some(mapping) = view.acp_peek {
                    if is_escape(&key) {
                        return input(FleetViewInput::CtrlC);
                    }
                    let Some(need) = mapping.needs.first() else {
                        return input(FleetViewInput::AnswerFocusEnded);
                    };
                    match key.code {
                        KeyCode::Up => {
                            self.acp_option = self.acp_option.saturating_sub(1);
                            return vec![UserAction::AcpSelect];
                        }
                        KeyCode::Down => {
                            self.acp_option = self
                                .acp_option
                                .saturating_add(1)
                                .min(need.options.len().saturating_sub(1));
                            return vec![UserAction::AcpSelect];
                        }
                        KeyCode::Char(ch) if ch.is_ascii_digit() && ch != '0' => {
                            let selected = ch as usize - '1' as usize;
                            if selected >= need.options.len() {
                                return Vec::new();
                            }
                            self.acp_option = selected;
                        }
                        KeyCode::Enter => {}
                        _ => return Vec::new(),
                    }
                    let Some(option) = need.options.get(self.acp_option) else {
                        return Vec::new();
                    };
                    let Some((key, _)) = fleet.peek() else {
                        return Vec::new();
                    };
                    return vec![UserAction::AcpAnswer {
                        key: key.clone(),
                        responder: need.responder.clone(),
                        tool_call_id: need.tool_call_id.clone(),
                        option_id: option.id.clone(),
                    }];
                }
                let foreground = fleet.peek().is_some_and(|(peeked, _)| {
                    fleet_peek_is_foreground(peeked, view.profile, view.session_id)
                });
                let outcome = if foreground {
                    self.prompts.handle_key(key, now)
                } else {
                    self.peek_prompts.handle_key(key, now)
                };
                fleet_card_actions(outcome)
            }
            FleetFocus::Reply | FleetFocus::Filter => {
                if is_escape(&key) {
                    return input(FleetViewInput::Escape);
                }
                if is_ctrl_c(&key) {
                    return input(FleetViewInput::CtrlC);
                }
                if let Some(character) = typed_character(&key) {
                    return input(FleetViewInput::Text(character));
                }
                match key.code {
                    KeyCode::Enter => input(FleetViewInput::Enter),
                    KeyCode::Backspace => input(FleetViewInput::Backspace),
                    KeyCode::Left => input(FleetViewInput::Left),
                    KeyCode::Right => input(FleetViewInput::Right),
                    KeyCode::Home => input(FleetViewInput::Home),
                    KeyCode::End => input(FleetViewInput::End),
                    _ => Vec::new(),
                }
            }
            FleetFocus::List | FleetFocus::ConfirmStop => {
                if is_escape(&key) {
                    return input(FleetViewInput::Escape);
                }
                if is_ctrl_c(&key) {
                    return input(FleetViewInput::CtrlC);
                }
                match self
                    .keymap
                    .resolve_in(&key, KeyContext::Fleet)
                    .and_then(fleet_action)
                {
                    // The dialog opens over the list; the overlay itself does nothing.
                    Some(FleetAction::New) if fleet.focus() == &FleetFocus::List => {
                        self.open_launch_dialog(fleet, view, DialogMode::Plain)
                    }
                    Some(FleetAction::NewInWorktree) if fleet.focus() == &FleetFocus::List => {
                        self.open_launch_dialog(fleet, view, DialogMode::Worktree)
                    }
                    Some(action) => input(FleetViewInput::Action(action)),
                    None => input(FleetViewInput::AnyOtherKey),
                }
            }
        }
    }

    /// One key for the open review, by its focus: the commit editor takes text, every other
    /// focus goes through the key map's review context. Esc and Ctrl+C are fixed.
    fn review_key(&self, key: Key, review: &ReviewView<'_>) -> Vec<UserAction> {
        if key.kind == KeyKind::Release {
            return Vec::new();
        }
        let send = |key: ReviewKey| vec![UserAction::Review(key)];
        if is_escape(&key) {
            return send(ReviewKey::Esc);
        }
        if is_ctrl_c(&key) {
            return send(ReviewKey::CtrlC);
        }
        if *review.focus == ReviewFocus::Commit {
            match self.keymap.resolve_in(&key, KeyContext::CommitMessage) {
                Some(Action::CommitSubmit) => return send(ReviewKey::Submit),
                Some(Action::CommitDraft) => return send(ReviewKey::Draft),
                _ => {}
            }
            if let Some(character) = typed_character(&key) {
                return send(ReviewKey::Char(character));
            }
            return send(match key.code {
                KeyCode::Enter => ReviewKey::Newline,
                KeyCode::Backspace => ReviewKey::Backspace,
                KeyCode::Up => ReviewKey::Up,
                KeyCode::Down => ReviewKey::Down,
                _ => ReviewKey::Other,
            });
        }
        if *review.focus == ReviewFocus::ConfirmPr && key.code == KeyCode::Enter {
            return send(ReviewKey::Enter);
        }
        let mapped = self
            .keymap
            .resolve_in(&key, KeyContext::Review)
            .and_then(review_action);
        send(mapped.unwrap_or(ReviewKey::Other))
    }

    /// `n` or `w` in the fleet list: the dialog starts from the foreground session's profile
    /// and folder, the profiles the fleet lists, and the draft kept from the last Esc.
    fn open_launch_dialog(
        &mut self,
        fleet: &FleetViewState,
        view: &View<'_>,
        mode: DialogMode,
    ) -> Vec<UserAction> {
        let profile_only = fleet.scope_note().is_some();
        let mut profiles: Vec<String> = if profile_only {
            Vec::new()
        } else {
            fleet
                .profiles()
                .iter()
                .filter(|(_, status)| {
                    !matches!(
                        status,
                        ProfileStatus::Parked { .. } | ProfileStatus::OutOfScope
                    )
                })
                .map(|(name, _)| name.clone())
                .collect()
        };
        if !profiles.iter().any(|name| name == view.profile) {
            profiles.insert(0, view.profile.to_owned());
        }
        if mode == DialogMode::Plain {
            profiles.extend(
                view.acp_agents
                    .iter()
                    .map(|(id, name)| format!("acp:{id} · {}", talaria_core::text::sanitize(name))),
            );
        }
        let folder = view
            .folder
            .and_then(|folder| folder.cwd.as_deref())
            .map_or_else(|| view.launch_folder.to_path_buf(), PathBuf::from);
        let defaults = DialogDefaults {
            profiles,
            profile: view.profile.to_owned(),
            profile_note: profile_only
                .then(|| "only this profile: the gateway refused a host-scoped ticket".to_owned()),
            folder,
            draft: self.launch_draft.clone(),
            model_placeholder: "profile default".to_owned(),
            slug: talaria_app::bg::slugify,
        };
        let mut dialog = LaunchDialog::open(mode, defaults);
        dialog.set_agent_launch_directory(view.launch_folder.to_path_buf());
        self.launch_dialog = Some(dialog);
        // A status left from an earlier launch is not this dialog's.
        self.launch_seen = view.launch_status.map_or(0, |(version, _)| version);
        vec![UserAction::LaunchDialog(DialogEvent::Opened)]
    }

    /// One key for the open launch dialog.
    fn launch_key(&mut self, key: Key) -> Vec<UserAction> {
        let Some(dialog) = self.launch_dialog.as_mut() else {
            return Vec::new();
        };
        match dialog.handle_pressed(key) {
            DialogAction::None => vec![UserAction::LaunchDialog(DialogEvent::Edited)],
            DialogAction::Close { draft } => {
                self.launch_draft = (!draft.is_empty()).then_some(draft);
                self.launch_dialog = None;
                vec![UserAction::LaunchDialog(DialogEvent::Closed)]
            }
            DialogAction::Launch(submit) => {
                vec![UserAction::LaunchDialog(DialogEvent::Submit(
                    LaunchSubmit {
                        prompt: submit.prompt,
                        profile: submit.profile,
                        folder: submit.folder,
                        model: submit.model,
                        worktree: submit.worktree,
                        worktree_name: submit.worktree_name,
                    },
                ))]
            }
        }
    }

    /// Show the app's latest status for the dialog's launch.
    fn sync_launch_dialog(&mut self, view: &View<'_>) {
        if view.fleet.is_none() {
            self.launch_dialog = None;
            return;
        }
        let Some((version, status)) = view.launch_status else {
            return;
        };
        if version <= self.launch_seen {
            return;
        }
        self.launch_seen = version;
        let Some(dialog) = self.launch_dialog.as_mut() else {
            return;
        };
        match status {
            DialogStatus::Invalid(text) => dialog.show_invalid(text),
            DialogStatus::Progress(text) => dialog.show_progress(text),
            DialogStatus::Failed { what, next_step } => dialog.show_error(what, next_step),
            DialogStatus::Close => {
                self.launch_dialog = None;
                self.launch_draft = None;
            }
        }
    }

    fn draw_picker(&mut self) {
        if let Some(picker) = &self.picker {
            let view = picker.view();
            let _ = self
                .terminal
                .draw_overlay(|area, buf| draw_model_picker(&view, area, buf));
        }
    }

    /// Show the terminal overlay for one picker, closing the other one.
    fn open_overlay(&mut self, kind: Overlay) {
        if self.overlay == Overlay::Profiles && kind != Overlay::Profiles {
            return;
        }
        if self.overlay == kind {
            return;
        }
        if self.overlay != Overlay::Live {
            let _ = self.terminal.close_overlay();
        }
        let _ = self.terminal.open_overlay();
        self.overlay = kind;
    }

    /// Close the terminal overlay, when it shows the given picker.
    fn close_overlay(&mut self, kind: Overlay) {
        if self.overlay != kind {
            return;
        }
        let _ = self.terminal.close_overlay();
        self.overlay = Overlay::Live;
    }

    /// The session picker's overlay: open it and draw it, or close it.
    fn sync_session_picker(&mut self, picker: Option<&PickerState>) {
        match picker {
            Some(state) => {
                self.open_overlay(Overlay::Sessions);
                self.draw_session_picker(state);
            }
            None => self.close_overlay(Overlay::Sessions),
        }
    }

    /// The picker's own frame, over the whole screen.
    fn draw_session_picker(&mut self, state: &PickerState) {
        let theme = PickerTheme::default();
        let now = unix_now();
        let _ = self
            .terminal
            .draw_overlay(|area, buf| SessionPickerView::draw(state, area, buf, now, &theme));
    }

    fn sync_agents(&mut self, view: &View<'_>) {
        if view.profiles.is_some() {
            return;
        }
        if view.processes_open {
            return;
        }
        let Some(hub) = view.subagents else {
            self.agents = None;
            self.agents_flash = None;
            self.agents_pause_pending = false;
            self.close_overlay(Overlay::Agents);
            return;
        };
        let Some(key) = hub.panel_key() else {
            self.agents = None;
            self.agents_flash = None;
            self.agents_pause_pending = false;
            self.close_overlay(Overlay::Agents);
            return;
        };
        self.open_overlay(Overlay::Agents);
        let panel = self
            .agents
            .get_or_insert_with(|| AgentsPanel::new(key.clone()));
        if panel.key() != key {
            panel.set_key(key.clone());
            self.agents_flash = None;
            self.agents_pause_pending = false;
        }
        panel.set_keymap(&self.keymap);
        panel.set_style(if self.color {
            SubagentStyle::default()
        } else {
            SubagentStyle::plain()
        });
        panel.set_spawning(hub.spawning(), hub.gateway_active());
        panel.tick(view.now_ms);
        if hub.pause_confirmation_pending() && !self.agents_pause_pending {
            panel.confirm_pause();
        }
        self.agents_pause_pending = hub.pause_confirmation_pending();
        if hub.flash() != self.agents_flash.as_deref() {
            if let Some(flash) = hub.flash() {
                panel.set_flash(flash.to_owned());
            }
            self.agents_flash = hub.flash().map(str::to_owned);
        }
    }

    fn draw_agents(&mut self, view: &View<'_>) {
        let Some(hub) = view.subagents else {
            return;
        };
        let Some(panel) = &self.agents else {
            return;
        };
        let Some(tree) = hub.tree(panel.key()) else {
            return;
        };
        let title = view
            .fleet_snapshot
            .and_then(|snapshot| {
                snapshot
                    .rows
                    .iter()
                    .find(|row| &row.key == panel.key())
                    .map(|row| row.title.as_str())
            })
            .filter(|title| !title.is_empty())
            .unwrap_or(&panel.key().session_id);
        let header = PanelHeader {
            title: title.to_owned(),
            spawning: hub.spawning(),
            key_label: None,
        };
        let tail = hub.tail(panel.key()).map(|tail| TailView {
            text: tail.text.clone(),
            truncated: tail.truncated,
            ever_failed: tail.ever_failed,
        });
        let now = SubagentClock {
            mono_ms: view.now_ms,
            wall_s: unix_now(),
        };
        let _ = self.terminal.draw_overlay(|area, buf| {
            for (offset, line) in panel
                .render(tree, &header, tail.as_ref(), area.width, area.height, now)
                .iter()
                .enumerate()
            {
                buf.set_line(area.x, area.y + offset as u16, line, area.width);
            }
        });
    }

    fn agents_key(&mut self, key: Key, view: &View<'_>) -> Vec<UserAction> {
        let Some(hub) = view.subagents else {
            return Vec::new();
        };
        let Some(panel) = &mut self.agents else {
            return Vec::new();
        };
        let Some(tree) = hub.tree(panel.key()) else {
            return Vec::new();
        };
        if key.kind == KeyKind::Release {
            return Vec::new();
        }
        panel.clear_flash();
        panel.tick(view.now_ms);
        let before = panel.mode();
        let mut actions = vec![UserAction::SubagentClearFlash];
        let mode_action = self.keymap.resolve_in(&key, KeyContext::Agents);
        if matches!(before, PanelMode::Steer | PanelMode::ConfirmPause) {
            match key.code {
                KeyCode::Backspace if before == PanelMode::Steer => panel.backspace(),
                KeyCode::Char(c) if !key.mods.ctrl && !key.mods.alt => panel.type_char(c),
                _ => {
                    if let Some(action) = mode_action
                        && let Some(control) = panel.handle(action, tree)
                    {
                        actions.push(UserAction::Subagent(control));
                    }
                }
            }
        } else if mode_action == Some(Action::AgentsNextSession) {
            if before == PanelMode::Tail {
                actions.push(UserAction::Subagent(SubagentAction::CloseTail));
            }
            actions.push(UserAction::SubagentNextSession);
        } else if mode_action == Some(Action::AgentsOpenInPane) {
            if let Some((key, title)) = panel.pane_target(tree) {
                actions.push(UserAction::OpenInPane { key, title });
            } else {
                panel.set_flash("This subagent has no session id yet.".to_owned());
            }
        } else if let Some(action) = mode_action
            && let Some(control) = panel.handle(action, tree)
        {
            actions.push(UserAction::Subagent(control));
        }
        let mut answered_pause = false;
        if let Some(control) = panel.take_pending_action() {
            answered_pause = matches!(control, SubagentAction::ConfirmPause(_));
            actions.push(UserAction::Subagent(control));
        }
        if before == PanelMode::ConfirmPause && panel.mode() != before && !answered_pause {
            actions.push(UserAction::Subagent(SubagentAction::ConfirmPause(false)));
        }
        if before != PanelMode::Tail
            && panel.mode() == PanelMode::Tail
            && let Some(id) = panel.selected_id(tree)
        {
            actions.push(UserAction::Subagent(SubagentAction::OpenTail(id)));
        }
        if before == PanelMode::Tail
            && !matches!(
                panel.mode(),
                PanelMode::Tail | PanelMode::Steer | PanelMode::ConfirmPause
            )
        {
            actions.push(UserAction::Subagent(SubagentAction::CloseTail));
        }
        if panel.take_close_request() {
            actions.push(UserAction::ToggleAgents);
        }
        actions
    }

    /// Take a newly resolved theme for the composer, the status line and later cells.
    fn set_theme(&mut self, theme: Theme) {
        if (self.color != theme.color || self.tool_theme != theme.tool)
            && let Some(transcript) = self.transcript.as_mut()
        {
            transcript.pager.invalidate_rendering();
        }
        self.color = theme.color;
        self.checklist_style = if theme.color {
            ChecklistStyle::default()
        } else {
            ChecklistStyle::plain()
        };
        self.composer.set_style(theme.composer);
        self.palette = theme.status;
        self.cells = tool_cells(&theme, &self.keymap);
        self.tool_theme = theme.tool.clone();
        self.theme = Arc::new(theme);
    }

    fn sync_prompts(&mut self, view: &View<'_>, now: Instant) {
        let open: Vec<&PromptCell> = view
            .state
            .map(|state| state.open_prompts().collect())
            .unwrap_or_default();
        self.prompts.sync(
            view.session_id,
            &open,
            &masked_views(view.masked),
            self.composer.is_empty(),
            now,
        );
    }

    fn sync_configure_prompts(&mut self, view: &View<'_>, now: Instant) {
        let open: Vec<&PromptCell> = view
            .configure_session
            .map(|session| session.open_prompts().collect())
            .unwrap_or_default();
        let session = open.first().map(|prompt| prompt.key.session_id.as_str());
        self.configure_prompts.sync(session, &open, &[], true, now);
    }

    fn configure_prompt_actions(outcome: KeyOutcome) -> Vec<UserAction> {
        let KeyOutcome::Consumed {
            intent,
            interrupt_requested,
            notice,
        } = outcome
        else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if let Some(PromptIntent::Clarify { key, answer }) = intent {
            actions.push(UserAction::Configure(Box::new(
                ConfigureInput::ClarifyAnswered { key, answer },
            )));
        }
        if interrupt_requested {
            actions.push(UserAction::Configure(Box::new(ConfigureInput::Key(
                ConfigureKey::Esc,
            ))));
        }
        if let Some(notice) = notice {
            actions.push(UserAction::Notice(notice));
        }
        actions
    }

    fn prompt_actions(outcome: KeyOutcome) -> Option<Vec<UserAction>> {
        let KeyOutcome::Consumed {
            intent,
            interrupt_requested,
            notice,
        } = outcome
        else {
            return None;
        };
        let mut actions = Vec::new();
        match intent {
            Some(PromptIntent::Approve { key, choice, note }) => {
                actions.push(UserAction::Approve { key, choice, note });
            }
            Some(PromptIntent::Clarify { key, answer }) => {
                actions.push(UserAction::Clarify { key, answer });
            }
            Some(PromptIntent::Masked { request_id, value }) => actions.push(UserAction::Masked {
                request_id,
                value: value.into_inner(),
            }),
            None => {}
        }
        if interrupt_requested {
            actions.push(UserAction::Interrupt);
        }
        if let Some(notice) = notice {
            actions.push(UserAction::Notice(notice));
        }
        Some(actions)
    }

    /// Tell the loop when the draft or the caret moved, so the slash palette follows it. A
    /// submitted draft is not reported: the loop knows the composer took it.
    fn report_draft(&mut self, actions: &mut Vec<UserAction>) {
        if self.composer.is_collecting_burst() {
            return;
        }
        if !actions
            .iter()
            .any(|action| matches!(action, UserAction::Submit(_)))
            && actions
                .iter()
                .any(|action| matches!(action, UserAction::DraftEdited { .. }))
        {
            self.reported = (self.composer.text().to_owned(), self.composer.caret());
            return;
        }
        let previous_len = actions.len();
        sync_reported_draft(
            self.composer.text(),
            self.composer.caret(),
            &mut self.reported,
            actions,
        );
        if actions.len() > previous_len
            && let Some(UserAction::Palette(PaletteInput::DraftChanged { text, caret })) =
                actions.first()
        {
            actions[0] = UserAction::DraftEdited {
                text: text.clone(),
                caret: *caret,
                typed: false,
                recalled: false,
                now: Instant::now(),
            };
        }
    }

    /// The shell-mode title follows the draft: set while it starts with `!`, cleared otherwise.
    fn sync_title(&mut self) {
        if self.editing.is_some() {
            return;
        }
        let title = shell_mode_title(self.composer.text()).map(str::to_string);
        self.composer.set_title(title);
    }

    fn composer_action(&mut self, outcome: ComposerOutcome, has_session: bool) -> Vec<UserAction> {
        match outcome {
            ComposerOutcome::Submit(text) => {
                let mut actions = vec![UserAction::Submit(text.clone())];
                if should_record_submission(&text, has_session)
                    && let Some(notice) =
                        talaria_app::settings::append_history(&mut self.history, &text)
                {
                    actions.push(UserAction::Notice(notice));
                }
                actions
            }
            ComposerOutcome::Interrupt => vec![UserAction::Interrupt],
            ComposerOutcome::ExitRequested => vec![UserAction::Quit],
            ComposerOutcome::Unchanged | ComposerOutcome::Changed => Vec::new(),
        }
    }

    fn close_palette(actions: &mut Vec<UserAction>) {
        actions.insert(0, UserAction::Palette(PaletteInput::Close));
    }

    /// Queue selection and edit mode take their keys after a focused card and before the
    /// palette and the composer (#232). `None` passes the key on.
    fn queue_keys(
        &mut self,
        raw: crossterm::event::KeyEvent,
        key: Key,
        view: &View<'_>,
        prefix: &mut Vec<UserAction>,
    ) -> Option<Vec<UserAction>> {
        if key.kind == KeyKind::Release {
            return None;
        }
        let queue = queue_view(view)?;
        let bound = self.keymap.resolve(&key, KeyContext::Composer);
        if queue.editing.is_some() {
            if bound == Some(Action::Submit) {
                // An edit saves before the blank-draft rule, slash dispatch and `!`.
                self.edit_closing = true;
                let mut actions = flush_draft_actions(&mut self.composer, Instant::now());
                let text = self.composer.text().to_string();
                actions.push(UserAction::Queue(QueueIntent::SaveEdit(text)));
                return Some(actions);
            }
            if let Some(intent) = handle_selection_key(raw, &queue, &self.keymap) {
                self.edit_closing = true;
                return Some(vec![UserAction::Queue(intent)]);
            }
            // Arrows move the caret only: no history while an edit is open.
            let plain_up = key.code == KeyCode::Up && key.mods == Default::default();
            return (plain_up && self.composer.cursor_on_first_row()).then(Vec::new);
        }
        if queue.selected.is_some() {
            return match handle_selection_key(raw, &queue, &self.keymap) {
                Some(intent) => Some(vec![UserAction::Queue(intent)]),
                None => {
                    prefix.push(UserAction::Queue(QueueIntent::Leave));
                    None
                }
            };
        }
        if self.composer.is_empty()
            && self.composer.cursor_on_first_row()
            && let Some(intent) = handle_selection_key(raw, &queue, &self.keymap)
        {
            return Some(vec![UserAction::Queue(intent)]);
        }
        if matches!(bound, Some(Action::ComposerSendNow | Action::ComposerSteer)) {
            prefix.extend(flush_draft_actions(&mut self.composer, Instant::now()));
        }
        let intent = queue_draft_intent(&self.composer, bound)?;
        Some(vec![UserAction::Queue(intent)])
    }

    fn composer_key(&mut self, key: Key, view: &View<'_>, now: Instant) -> Vec<UserAction> {
        let ctx = ComposerContext {
            turn_running: view.turn_running(),
        };
        let mut actions = Vec::new();
        for action in composer_key_actions(&mut self.composer, key, ctx, now) {
            match action {
                UserAction::Submit(text) => actions.extend(
                    self.composer_action(ComposerOutcome::Submit(text), view.state.is_some()),
                ),
                UserAction::Escape { eligible } => actions.push(UserAction::Escape {
                    eligible: eligible && !view.palette.open,
                }),
                action => actions.push(action),
            }
        }
        actions
    }

    /// Load an opened edit's text, clear a closed one, and title the composer (#232).
    fn sync_queue(&mut self, view: &View<'_>) {
        let state = view.state.map(|state| &state.queue);
        let open = state.and_then(|queue| queue.edit.as_ref());
        match (open, &self.editing) {
            (Some(edit), current) if current.as_ref() != Some(&edit.admission_id) => {
                self.composer.load(edit.original_text.clone());
                self.editing = Some(edit.admission_id.clone());
                self.edit_closing = false;
            }
            (None, Some(_)) => {
                // Saved or cancelled: the draft goes. A row that started keeps it as a draft.
                if self.edit_closing {
                    self.composer.clear();
                }
                self.editing = None;
                self.edit_closing = false;
            }
            _ => {}
        }
        let title = queue_view(view)
            .and_then(|queue| composer_title(&queue))
            .or_else(|| shell_mode_title(self.composer.text()).map(str::to_owned));
        self.composer.set_title(title);
    }

    fn terminal_input(
        &mut self,
        event: crossterm::event::Event,
        view: &View<'_>,
        now: Instant,
    ) -> Vec<UserAction> {
        let mut prefix = Vec::new();
        let mut actions = self.terminal_input_actions(event, view, now, &mut prefix);
        prefix.append(&mut actions);
        prefix
    }

    fn terminal_input_actions(
        &mut self,
        event: crossterm::event::Event,
        view: &View<'_>,
        now: Instant,
        prefix: &mut Vec<UserAction>,
    ) -> Vec<UserAction> {
        use crossterm::event::Event;
        if let Some(board) = view.kanban
            && !self.terminal.is_released()
        {
            use talaria_core::kanban::{BoardKey, BoardMode};
            match &event {
                Event::Key(raw) => {
                    let key = Key::from(*raw);
                    if key.kind == KeyKind::Release
                        || key.mods.ctrl
                        || key.mods.alt
                        || key.mods.super_key
                        || key.mods.other
                    {
                        return vec![];
                    }
                    let text_mode = matches!(
                        board.mode,
                        BoardMode::Comment { .. } | BoardMode::Evidence { .. } | BoardMode::Filter
                    );
                    let key = match key.code {
                        KeyCode::Char(c) => Some(BoardKey::Char(c)),
                        KeyCode::Left if !text_mode => Some(BoardKey::Char('h')),
                        KeyCode::Down if !text_mode => Some(BoardKey::Char('j')),
                        KeyCode::Up if !text_mode => Some(BoardKey::Char('k')),
                        KeyCode::Right if !text_mode => Some(BoardKey::Char('l')),
                        KeyCode::Enter => Some(BoardKey::Enter),
                        KeyCode::Esc => Some(BoardKey::Esc),
                        KeyCode::Backspace => Some(BoardKey::Backspace),
                        _ => None,
                    };
                    return key.map(UserAction::Kanban).into_iter().collect();
                }
                Event::Paste(text) => {
                    return if matches!(
                        board.mode,
                        BoardMode::Comment { .. } | BoardMode::Evidence { .. } | BoardMode::Filter
                    ) {
                        text.chars()
                            .filter(|c| !c.is_control())
                            .map(|c| UserAction::Kanban(BoardKey::Char(c)))
                            .collect()
                    } else {
                        vec![]
                    };
                }
                _ => {}
            }
        }
        if self.cron.is_some()
            && !self.terminal.is_released()
            && self.mcp_view.is_none()
            && view.profiles.is_none()
            && view.doctor.is_none()
            && view.memory.is_none()
            && view.capabilities.is_none()
            && view.connectors.is_none()
            && view.configure.is_none()
        {
            match &event {
                Event::Key(raw) => {
                    let key = Key::from(*raw);
                    if key.kind == KeyKind::Release {
                        return Vec::new();
                    }
                    if is_ctrl_c(&key) {
                        return vec![UserAction::Interrupt];
                    }
                    if key.mods.ctrl && key.code == KeyCode::Char('z') {
                        return self.start_suspend();
                    }
                    let question = self
                        .cron
                        .as_ref()
                        .is_some_and(|view| view.question.is_some());
                    return cron_key(&self.keymap, key, self.cron_history_focused, question)
                        .map(|key| {
                            UserAction::Cron(Box::new(talaria_core::cron::CronInput::Key(key)))
                        })
                        .into_iter()
                        .collect();
                }
                Event::Paste(text) => {
                    return vec![UserAction::Cron(Box::new(
                        talaria_core::cron::CronInput::Key(talaria_core::cron::CronKey::Paste(
                            text.clone(),
                        )),
                    ))];
                }
                Event::Resize(_, _) => self.cron_dirty = true,
                _ => {}
            }
        }
        if view.profiles.is_none()
            && view.connectors.is_none()
            && view.configure.is_none()
            && view.settings.is_none()
            && view.doctor.is_none()
            && !self.terminal.is_released()
            && let Some(providers) = self.providers_view.as_ref()
        {
            match event {
                Event::Key(raw) => {
                    return crate::providers_wiring::key_for(
                        providers,
                        &self.keymap,
                        Key::from(raw),
                    )
                    .map(|key| {
                        UserAction::Providers(Box::new(self.providers.route_key(providers, key)))
                    })
                    .into_iter()
                    .collect();
                }
                Event::Paste(mut text) => {
                    let input = self.providers.route_paste(providers, &text);
                    zeroize::Zeroize::zeroize(&mut text);
                    return input
                        .map(|input| UserAction::Providers(Box::new(input)))
                        .into_iter()
                        .collect();
                }
                _ => {}
            }
        }
        match event {
            Event::Key(raw) => {
                if self.terminal.is_released() {
                    return Vec::new();
                }
                let key = Key::from(raw);
                if key.kind == KeyKind::Release {
                    return Vec::new();
                }
                if is_ctrl_c(&key)
                    && (view.configure.is_some() || self.configure_cancel_at.is_some())
                {
                    if key.kind == KeyKind::Repeat {
                        return Vec::new();
                    }
                    if self.configure_cancel_at.is_some_and(|pressed| {
                        now.saturating_duration_since(pressed) < Duration::from_secs(2)
                    }) {
                        self.configure_cancel_at = None;
                        return vec![
                            UserAction::Configure(Box::new(ConfigureInput::Quit)),
                            UserAction::Quit,
                        ];
                    }
                    if view.configure.is_some() {
                        self.configure_cancel_at = Some(now);
                        return vec![UserAction::Configure(Box::new(ConfigureInput::Quit))];
                    }
                }
                self.configure_cancel_at = None;
                if self.mcp_view.is_none()
                    && view.profiles.is_none()
                    && view.doctor.is_none()
                    && let Some(configure) = &view.configure
                {
                    if (view.configure_refining && is_escape(&key))
                        || (configure.phase == ConfigurePhase::Question
                            && self.keymap.resolve_configure(&key) == Some(ConfigureKey::Esc))
                    {
                        return vec![UserAction::Configure(Box::new(ConfigureInput::Key(
                            ConfigureKey::Esc,
                        )))];
                    }
                    if configure.phase == ConfigurePhase::Question
                        && self.configure_prompts.section().is_some()
                    {
                        let actions = Self::configure_prompt_actions(
                            self.configure_prompts.handle_key(key, now),
                        );
                        if actions.iter().any(|action| {
                            matches!(action, UserAction::Configure(input)
                            if matches!(input.as_ref(), ConfigureInput::ClarifyAnswered { .. }))
                        }) {
                            self.configure_prompts = PromptStack::new();
                        }
                        return actions;
                    }
                    let input = if view.configure_refining {
                        (!key.mods.super_key && !key.mods.other)
                            .then(|| typed_character(&key))
                            .flatten()
                            .map(ConfigureKey::Char)
                            .or(match key.code {
                                KeyCode::Enter => Some(ConfigureKey::Enter),
                                KeyCode::Backspace => Some(ConfigureKey::Backspace),
                                KeyCode::Esc => Some(ConfigureKey::Esc),
                                _ => None,
                            })
                    } else {
                        talaria_tui::configure::route_configure(&self.keymap, &key)
                    };
                    if let Some(key) = &input
                        && let Some(offset) = talaria_tui::configure::detail_scroll_target(
                            configure,
                            key,
                            self.terminal.screen_area(),
                        )
                    {
                        return vec![UserAction::Configure(Box::new(
                            ConfigureInput::DetailScroll(offset),
                        ))];
                    }
                    return input
                        .map(|key| UserAction::Configure(Box::new(ConfigureInput::Key(key))))
                        .into_iter()
                        .collect();
                }
                if self.mcp_view.is_none()
                    && view.profiles.is_none()
                    && view.doctor.is_none()
                    && let Some(connectors) = &view.connectors
                {
                    return self.connectors_key(
                        Key::from(raw),
                        connectors,
                        view.connectors_filtering,
                    );
                }
                if let Some(memory) = &view.memory {
                    let key = Key::from(raw);
                    if key.kind == KeyKind::Release {
                        return Vec::new();
                    }
                    return talaria_tui::memory::route_memory(memory, &self.keymap, &key)
                        .map(talaria_core::memory::MemoryInput::Key)
                        .map(UserAction::Memory)
                        .into_iter()
                        .collect();
                }
                if let Some(capabilities) = &view.capabilities {
                    let input = talaria_tui::capabilities::route_capabilities(
                        capabilities,
                        &self.keymap,
                        &Key::from(raw),
                    )
                    .unwrap_or(talaria_core::capabilities::CapInput::DismissMessage);
                    return vec![UserAction::Capabilities(Box::new(input))];
                }
                if let Some(mcp) = &self.mcp_view {
                    return self
                        .mcp_wiring
                        .route_key_with_map(mcp, raw, &self.keymap)
                        .map(UserAction::Mcp)
                        .into_iter()
                        .collect();
                }
                if let Some(profiles) = &view.profiles {
                    let key = Key::from(raw);
                    if key.kind == KeyKind::Release {
                        return Vec::new();
                    }
                    let action = if profiles.confirm.is_some() {
                        Some(
                            if !key.mods.ctrl
                                && !key.mods.alt
                                && !key.mods.super_key
                                && key.code == KeyCode::Char('y')
                            {
                                ProfilesKey::Char('y')
                            } else {
                                ProfilesKey::Back
                            },
                        )
                    } else if matches!(profiles.panel, ProfilesPanel::Review(_))
                        && key.code == KeyCode::Enter
                        && !key.mods.ctrl
                        && !key.mods.alt
                    {
                        Some(ProfilesKey::Enter)
                    } else {
                        profiles_key(
                            &self.keymap,
                            &key,
                            view.profiles_text_focus
                                || matches!(profiles.panel, ProfilesPanel::Form(_)),
                        )
                    };
                    return vec![UserAction::Profiles(
                        action.map_or(ProfilesInput::UnboundKey, ProfilesInput::Key),
                    )];
                }
                if let Some(settings) = &view.settings {
                    let key = Key::from(raw);
                    if key.kind == KeyKind::Release {
                        return Vec::new();
                    }
                    if key.mods.ctrl && key.code == KeyCode::Char('z') {
                        let mut actions = vec![UserAction::Settings(
                            talaria_core::settings::SettingsInput::HideReveal,
                        )];
                        actions.extend(self.start_suspend());
                        return actions;
                    }
                    if is_ctrl_c(&key) {
                        return vec![
                            UserAction::Settings(talaria_core::settings::SettingsInput::HideReveal),
                            UserAction::Interrupt,
                        ];
                    }
                    let input = talaria_tui::settings::route_settings(settings, &self.keymap, &key)
                        .map_or(talaria_core::settings::SettingsInput::HideReveal, |key| {
                            talaria_tui::settings::review_scroll_target(
                                settings,
                                &key,
                                self.terminal.screen_area(),
                            )
                            .map_or(
                                talaria_core::settings::SettingsInput::Key(key),
                                talaria_core::settings::SettingsInput::ReviewScroll,
                            )
                        });
                    return vec![UserAction::Settings(input)];
                }
                if view.doctor.is_some() {
                    return self
                        .keymap
                        .resolve_in(&Key::from(raw), KeyContext::Doctor)
                        .and_then(talaria_tui::keys::doctor_key)
                        .map(UserAction::Doctor)
                        .into_iter()
                        .collect();
                }
                if let Some(roles) = &view.model_roles {
                    let key = Key::from(raw);
                    if key.kind == KeyKind::Release {
                        return Vec::new();
                    }
                    if key.mods.ctrl && key.code == KeyCode::Char('z') {
                        return self.start_suspend();
                    }
                    if is_ctrl_c(&key) {
                        return vec![UserAction::Interrupt];
                    }
                    return talaria_tui::model_roles::route_model_roles(roles, &self.keymap, &key)
                        .map(|key| {
                            vec![UserAction::ModelRoles(
                                talaria_core::model_roles::RolesInput::Key(key),
                            )]
                        })
                        .unwrap_or_default();
                }
                if view.processes_open {
                    return view.state.map_or_else(Vec::new, |state| {
                        vec![UserAction::Processes(self.processes.handle_key(
                            Key::from(raw),
                            state.background(),
                            view.now_ms,
                        ))]
                    });
                }
                // The launch dialog owns every key while it is open, then the fleet overlay;
                // in the overlay's answer focus the peeked session's card takes them.
                // An open review owns every key: its list, its confirmations, its editor.
                if let Some(review) = &view.review {
                    return self.review_key(Key::from(raw), review);
                }
                if view.fleet.is_some() && self.launch_dialog.is_some() {
                    return self.launch_key(Key::from(raw));
                }
                if let Some(fleet) = view.fleet {
                    return self.fleet_key(Key::from(raw), fleet, view, now);
                }
                if view.workflow_picker.is_some() {
                    return talaria_tui::workflow_view::picker_key_input(
                        &self.keymap,
                        Key::from(raw),
                    )
                    .map(UserAction::Workflow)
                    .into_iter()
                    .collect();
                }
                if let Some(progress) = view.workflow_progress {
                    return talaria_tui::workflow_view::key_input(
                        &self.keymap,
                        Key::from(raw),
                        progress,
                    )
                    .map(UserAction::Workflow)
                    .into_iter()
                    .collect();
                }
                if self.overlay == Overlay::Agents {
                    return self.agents_key(Key::from(raw), view);
                }
                // The session picker's overlay owns every key while it is open: nothing
                // reaches the composer, the prompt cards or the running turn.
                if self.overlay == Overlay::Sessions {
                    let Some(picker) = view.picker else {
                        return Vec::new();
                    };
                    return picker_input(&self.keymap, Key::from(raw), picker.mode())
                        .map(UserAction::Picker)
                        .into_iter()
                        .collect();
                }
                let input = input_key(&raw);
                if view.pager_open {
                    return input
                        .map(|key| vec![UserAction::Input(key)])
                        .unwrap_or_default();
                }
                let key = Key::from(raw);
                let action = self.keymap.resolve(&key, KeyContext::Composer);
                if action == Some(Action::FleetViewOpen) {
                    return vec![UserAction::FleetOpen];
                }
                if action == Some(Action::AgentsPanel) && !view.pager_open && view.rewind.is_none()
                {
                    return vec![UserAction::ToggleAgents];
                }
                let focus = if self.search.is_some() {
                    talaria_tui::comfort_route::Focus::Search
                } else if self.composer.is_empty() && self.prompts.section().is_some() {
                    talaria_tui::comfort_route::Focus::Card
                } else {
                    talaria_tui::comfort_route::Focus::Composer
                };
                if config::vim_redo_claim(
                    action == Some(Action::HistorySearch),
                    focus == talaria_tui::comfort_route::Focus::Composer
                        && self.overlay == Overlay::Live
                        && view.rewind.is_none()
                        && !self.confirmation_takes_keys()
                        && !view.palette.open
                        && queue_view(view).is_none_or(|queue| {
                            queue.selected.is_none() && queue.editing.is_none()
                        }),
                    matches!(
                        self.composer.vim_mode(),
                        Some(VimMode::Normal | VimMode::Visual | VimMode::VisualLine)
                    ),
                ) {
                    return self.composer_key(key, view, now);
                }
                let route = talaria_tui::comfort_route::route(action, focus);
                if route == talaria_tui::comfort_route::Route::Suspend {
                    let _ = self.composer.handle_key(
                        key,
                        ComposerContext {
                            turn_running: view.turn_running(),
                        },
                        now,
                    );
                    let mut actions = self.start_suspend();
                    Self::close_palette(&mut actions);
                    return actions;
                }
                if let Some(rewind) = &view.rewind {
                    return self.rewind_key(&raw, rewind.screen);
                }
                let key = Key::from(raw);
                if self.overlay == Overlay::Models {
                    return picker_key(&self.keymap, key)
                        .map(UserAction::ModelPicker)
                        .into_iter()
                        .collect();
                }
                if let Some(search) = &mut self.search {
                    let mut actions = Vec::new();
                    Self::close_palette(&mut actions);
                    match search.handle_key(key, std::time::SystemTime::now()) {
                        SearchOutcome::Accept(text) => {
                            self.search = None;
                            let recalled =
                                recall_draft(&mut self.composer, &mut self.reported, text, now);
                            actions.insert(0, recalled);
                        }
                        SearchOutcome::Cancel => self.search = None,
                        _ => {}
                    }
                    return actions;
                }
                if self.opens_transcript(&key) {
                    let mut actions = flush_draft_actions(&mut self.composer, now);
                    actions.extend(input.map(UserAction::Input));
                    return actions;
                }
                // The checklist key is looked up whatever has focus, so a card keeps
                // its focus and the key never reaches the composer.
                if self.keymap.resolve(&key, KeyContext::Composer) == Some(Action::ToggleTasks) {
                    return vec![UserAction::ToggleTasks];
                }
                if let Some(mut actions) = Self::prompt_actions(self.prompts.handle_key(key, now)) {
                    Self::close_palette(&mut actions);
                    return actions;
                }
                self.sync_confirmation_focus(now);
                let control_c = key.mods.ctrl
                    && !key.mods.alt
                    && !key.mods.super_key
                    && matches!(key.code, KeyCode::Char('c' | 'C'));
                if self.confirmation_takes_keys() && !control_c {
                    return confirmation_key(key, self.confirmation_focused, now)
                        .map(UserAction::ModelConfirm)
                        .into_iter()
                        .collect();
                }
                if let Some(actions) = self.queue_keys(raw, key, view, prefix) {
                    return actions;
                }
                if self
                    .composer
                    .vim_mode()
                    .is_some_and(|mode| mode != VimMode::Insert)
                    && focus == talaria_tui::comfort_route::Focus::Composer
                    && !view.palette.open
                    && route != talaria_tui::comfort_route::Route::OpenEditor
                    && route != talaria_tui::comfort_route::Route::AttentionNext
                {
                    return self.composer_key(key, view, now);
                }
                // An open completion list takes its four keys before the composer does.
                if view.palette.open
                    && focus == talaria_tui::comfort_route::Focus::Composer
                    && key.kind != KeyKind::Release
                    && let Some(action) = self.keymap.completion(&key)
                {
                    let input = match action {
                        CompletionAction::Next => PaletteInput::Next,
                        CompletionAction::Prev => PaletteInput::Prev,
                        CompletionAction::Accept => PaletteInput::Accept,
                        CompletionAction::Close => PaletteInput::Close,
                    };
                    let mut actions = flush_draft_actions(&mut self.composer, now);
                    actions.push(UserAction::Palette(input));
                    return actions;
                }
                if !view.palette.open
                    && self.keymap.resolve_model(KeyContext::Composer, &key)
                        == Some(ModelAction::ModelPicker)
                {
                    let mut actions = flush_draft_actions(&mut self.composer, now);
                    actions.push(UserAction::OpenModels);
                    return actions;
                }
                let mut actions = Vec::new();
                if attachment_key(
                    &self.keymap,
                    &key,
                    &self.attachments,
                    self.composer.text(),
                    self.composer.caret(),
                )
                .is_some()
                {
                    actions = flush_draft_actions(&mut self.composer, now);
                    if let Some(action) = attachment_key(
                        &self.keymap,
                        &key,
                        &self.attachments,
                        self.composer.text(),
                        self.composer.caret(),
                    ) {
                        actions.push(UserAction::AttachmentKey {
                            action,
                            text: self.composer.text().to_string(),
                            caret: self.composer.caret(),
                            now,
                        });
                        return actions;
                    }
                }
                if route == talaria_tui::comfort_route::Route::AttentionNext {
                    // The jump key belongs to the composer: it never edits the draft.
                    let mut actions = Vec::new();
                    Self::close_palette(&mut actions);
                    actions.push(UserAction::AttentionNext);
                    return actions;
                }
                if route == talaria_tui::comfort_route::Route::OpenEditor {
                    let _ = self.composer.handle_key(
                        key,
                        ComposerContext {
                            turn_running: view.turn_running(),
                        },
                        now,
                    );
                    let mut actions = self.start_editor();
                    Self::close_palette(&mut actions);
                    return actions;
                }
                if route == talaria_tui::comfort_route::Route::HistorySearch {
                    let _ = self.composer.handle_key(
                        key,
                        ComposerContext {
                            turn_running: view.turn_running(),
                        },
                        now,
                    );
                    self.search = Some(HistorySearch::open(
                        self.history.snapshot(),
                        self.composer.text().into(),
                        self.keymap.chords(Action::HistorySearch).to_vec(),
                    ));
                    let mut actions = Vec::new();
                    Self::close_palette(&mut actions);
                    return actions;
                }
                // A focused card took the key above. The completion list keeps it; the
                // composer hands it to the approvals toggle (#237).
                if key.kind != KeyKind::Release
                    && self.keymap.resolve(&key, KeyContext::Composer)
                        == Some(Action::ApprovalsToggle)
                {
                    let context = if view.palette.open {
                        ToggleContext::Completion
                    } else {
                        ToggleContext::Composer
                    };
                    return vec![UserAction::ApprovalsToggle { context }];
                }
                actions.extend(self.composer_key(key, view, now));
                actions
            }
            Event::Paste(mut text) => {
                self.configure_cancel_at = None;
                if self.mcp_view.is_none()
                    && view.profiles.is_none()
                    && view.doctor.is_none()
                    && let Some(configure) = &view.configure
                {
                    if self.terminal.is_released() {
                        return Vec::new();
                    }
                    if configure.phase == ConfigurePhase::Question
                        && self.configure_prompts.section().is_some()
                    {
                        return Self::configure_prompt_actions(
                            self.configure_prompts.handle_paste(&text, now),
                        );
                    }
                    if view.configure_refining {
                        return vec![UserAction::Configure(Box::new(ConfigureInput::Key(
                            ConfigureKey::Paste(text),
                        )))];
                    }
                    return Vec::new();
                }
                if self.mcp_view.is_none()
                    && view.profiles.is_none()
                    && view.doctor.is_none()
                    && let Some(connectors) = &view.connectors
                {
                    if self.terminal.is_released() {
                        return Vec::new();
                    }
                    let input = if matches!(
                        connectors.screen,
                        talaria_core::connectors::Screen::SecretEditor { .. }
                    ) {
                        let input = self.connectors_wiring.route_secret_paste(&text);
                        text.zeroize();
                        input
                    } else {
                        Some(self.connectors_wiring.route_paste(text))
                    };
                    return input.map(UserAction::Connectors).into_iter().collect();
                }
                if view.memory.is_some() && !self.terminal.is_released() {
                    return vec![UserAction::Memory(talaria_core::memory::MemoryInput::Key(
                        talaria_core::memory::MemoryKey::Paste(text),
                    ))];
                }
                if let Some(capabilities) = &view.capabilities {
                    if self.terminal.is_released() || !capabilities.text_focus {
                        return Vec::new();
                    }
                    return text
                        .chars()
                        .filter(|character| !character.is_control())
                        .map(|character| {
                            UserAction::Capabilities(Box::new(
                                talaria_core::capabilities::CapInput::Char(character),
                            ))
                        })
                        .collect();
                }
                if let Some(mcp) = &self.mcp_view {
                    return self
                        .mcp_wiring
                        .route_paste(mcp, &text)
                        .map(UserAction::Mcp)
                        .into_iter()
                        .collect();
                }
                if view.profiles.is_some() {
                    return if self.terminal.is_released() {
                        Vec::new()
                    } else {
                        vec![UserAction::Profiles(ProfilesInput::Paste(text))]
                    };
                }
                if view.settings.is_some() && !self.terminal.is_released() {
                    return vec![UserAction::Settings(
                        talaria_core::settings::SettingsInput::Key(
                            talaria_core::settings::SettingsKey::Paste(text),
                        ),
                    )];
                }
                if view.doctor.is_some() {
                    return Vec::new();
                }
                if view.model_roles.is_some() {
                    return Vec::new();
                }
                if view.processes_open {
                    return Vec::new();
                }
                if self.terminal.is_released() {
                    return Vec::new();
                }
                if view.review.is_some() {
                    return vec![UserAction::ReviewPaste(text)];
                }
                if view.fleet.is_some()
                    && let Some(dialog) = self.launch_dialog.as_mut()
                {
                    dialog.handle_paste(&text);
                    return vec![UserAction::LaunchDialog(DialogEvent::Edited)];
                }
                if let Some(fleet) = view.fleet {
                    let foreground = fleet.peek().is_some_and(|(peeked, _)| {
                        fleet_peek_is_foreground(peeked, view.profile, view.session_id)
                    });
                    return fleet_paste(
                        fleet.focus(),
                        foreground,
                        &mut self.prompts,
                        &mut self.peek_prompts,
                        text,
                        now,
                    );
                }
                if let Some(progress) = view.workflow_progress {
                    if progress.steer_input.is_some() {
                        return text
                            .chars()
                            .filter(|c| !c.is_control())
                            .map(|c| {
                                UserAction::Workflow(talaria_tui::workflow_view::ProgressKey::Char(
                                    c,
                                ))
                            })
                            .collect();
                    }
                    return vec![];
                }
                if self.overlay == Overlay::Agents {
                    if let Some(panel) = &mut self.agents
                        && panel.mode() == PanelMode::Steer
                    {
                        for c in text.chars() {
                            panel.type_char(c);
                        }
                    }
                    return vec![UserAction::SubagentClearFlash];
                }
                if self.overlay == Overlay::Sessions {
                    let Some(picker) = view.picker else {
                        return Vec::new();
                    };
                    return picker_paste(picker.mode(), &text)
                        .map(UserAction::Picker)
                        .into_iter()
                        .collect();
                }
                if view.pager_typing {
                    let first = text.split('\n').next().unwrap_or("");
                    return first
                        .chars()
                        .map(|character| {
                            UserAction::Input(InputKey {
                                code: InputCode::Char(character),
                                ctrl: false,
                                alt: false,
                                shift: false,
                                super_key: false,
                                other: false,
                            })
                        })
                        .collect();
                }
                if view.pager_open || view.rewind.is_some() || self.overlay == Overlay::Models {
                    return Vec::new();
                }
                if let Some(search) = &mut self.search {
                    search.handle_paste(&text);
                    return Vec::new();
                }
                if let Some(mut actions) =
                    Self::prompt_actions(self.prompts.handle_paste(&text, now))
                {
                    Self::close_palette(&mut actions);
                    return actions;
                }
                if self.confirmation_takes_keys() {
                    return Vec::new();
                }
                paste_actions(&mut self.composer, &text, now)
            }
            Event::Resize(cols, rows) => {
                self.rows = rows;
                let _ = self.terminal.resize(cols, rows);
                view.profiles
                    .as_ref()
                    .map(|profiles| UserAction::ProfilesLayout {
                        width: cols,
                        page_size: self.profiles_page_size(profiles),
                    })
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// The facts the transcript screen draws.
fn transcript_context<'a>(view: &'a View<'a>) -> PagerContext<'a> {
    let cells = view.state.map_or(&[][..], |state| state.cells());
    let title = view.title.unwrap_or("");
    let running = view.turn_running();
    let needs_answer = !view.masked.is_empty()
        || view
            .state
            .is_some_and(|state| state.open_prompts().next().is_some());
    let connection = match view.link {
        LinkView::Connected => ConnectionBanner::Connected,
        LinkView::Connecting | LinkView::Reconnecting => ConnectionBanner::Reconnecting,
        LinkView::Failed | LinkView::Closed => ConnectionBanner::Disconnected,
    };
    let history_truncated = view.state.is_some_and(|state| state.history_truncated());
    PagerContext {
        cells,
        source_epoch: view.source_epoch,
        title,
        running,
        needs_answer,
        connection,
        history_truncated,
    }
}

/// The terminal key of an application key.
fn tui_key(key: InputKey) -> Key {
    let code = match key.code {
        InputCode::Char(character) => KeyCode::Char(character),
        InputCode::Enter => KeyCode::Enter,
        InputCode::Esc => KeyCode::Esc,
        InputCode::Backspace => KeyCode::Backspace,
        InputCode::Tab => KeyCode::Tab,
        InputCode::Delete => KeyCode::Delete,
        InputCode::Insert => KeyCode::Insert,
        InputCode::Up => KeyCode::Up,
        InputCode::Down => KeyCode::Down,
        InputCode::Left => KeyCode::Left,
        InputCode::Right => KeyCode::Right,
        InputCode::PageUp => KeyCode::PageUp,
        InputCode::PageDown => KeyCode::PageDown,
        InputCode::Home => KeyCode::Home,
        InputCode::End => KeyCode::End,
        InputCode::F(number) => KeyCode::F(number),
    };
    Key {
        code,
        mods: talaria_tui::keys::Mods {
            ctrl: key.ctrl,
            alt: key.alt,
            shift: key.shift,
            super_key: key.super_key,
            other: key.other,
        },
        kind: KeyKind::Press,
    }
}

/// Translate a transcript search key into a pager edit.
fn pager_search_input(key: InputKey) -> Option<SearchInput> {
    match key.code {
        InputCode::Enter => Some(SearchInput::Enter),
        InputCode::Esc => Some(SearchInput::Escape),
        InputCode::Backspace => Some(SearchInput::Backspace),
        InputCode::Char(_) => key.search_character().map(SearchInput::Char),
        _ => None,
    }
}

/// The transcript action of a key-map action.
fn pager_action(action: Action) -> Option<PagerAction> {
    Some(match action {
        Action::PagerLineUp => PagerAction::LineUp,
        Action::PagerLineDown => PagerAction::LineDown,
        Action::PagerPageUp => PagerAction::PageUp,
        Action::PagerPageDown => PagerAction::PageDown,
        Action::PagerHalfUp => PagerAction::HalfPageUp,
        Action::PagerHalfDown => PagerAction::HalfPageDown,
        Action::PagerTop => PagerAction::Top,
        Action::PagerBottom => PagerAction::Bottom,
        Action::PagerPrevPrompt => PagerAction::PrevPrompt,
        Action::PagerNextPrompt => PagerAction::NextPrompt,
        Action::PagerSearch => PagerAction::StartSearch,
        Action::PagerNextMatch => PagerAction::NextMatch,
        Action::PagerPrevMatch => PagerAction::PrevMatch,
        Action::PagerDetails => PagerAction::ToggleDetails,
        Action::PagerClose => PagerAction::Close,
        _ => return None,
    })
}

fn transcript_tool_renderer(
    details: Details,
    width: u16,
    color: bool,
    theme: &ToolTheme,
) -> ToolCellLines {
    ToolCellLines::new(
        Box::new(PagerCellLines {
            details,
            markdown: MarkdownOptions {
                width: width.saturating_sub(2).max(8),
                color,
                ..MarkdownOptions::default()
            },
            theme: PagerTheme::default(),
            other: &PLAIN_CELLS,
        }),
        ToolRenderOptions {
            width,
            theme: theme.clone(),
            color,
            detail: match details {
                Details::Expanded => Detail::Full,
                Details::Compact => Detail::Compact,
            },
            expand_hint: None,
            elapsed: None,
        },
    )
}

impl TuiScreen {
    /// Whether the key opens the transcript.
    fn opens_transcript(&self, key: &Key) -> bool {
        self.keymap
            .chords(Action::TranscriptOpen)
            .iter()
            .any(|chord| chord.matches(key))
    }

    /// Apply what the application asked of the transcript screen.
    fn apply_transcript(&mut self, view: &View<'_>) {
        if self.transcript_actions.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut self.transcript_actions);
        let width = self.terminal.viewport().width;
        let height = self.terminal.viewport().height;
        for action in actions {
            match action {
                TranscriptAction::Open => {
                    let ctx = transcript_context(view);
                    let mut index = SearchIndex::new();
                    let pager = Pager::open(&ctx, width, height, &mut index);
                    self.transcript = Some(Transcript {
                        pager,
                        index,
                        dirty: false,
                    });
                }
                TranscriptAction::Close => self.transcript = None,
                TranscriptAction::Changed => {
                    if let Some(transcript) = self.transcript.as_mut() {
                        transcript.dirty = true;
                    }
                }
                TranscriptAction::Key(key) => {
                    // Escape is reserved in the pager context, so the key map does not bind it.
                    // A bare Escape still has to reach the pager, where it clears a confirmed
                    // search before a later Escape closes the screen.
                    let action = if key.code == InputCode::Esc
                        && !key.ctrl
                        && !key.alt
                        && !key.shift
                        && !key.super_key
                        && !key.other
                    {
                        Some(PagerAction::Escape)
                    } else {
                        self.keymap
                            .resolve(&tui_key(key), KeyContext::Pager)
                            .and_then(pager_action)
                    };
                    let Some(transcript) = self.transcript.as_mut() else {
                        continue;
                    };
                    let ctx = transcript_context(view);
                    if transcript.pager.is_typing_search() {
                        let input = pager_search_input(key);
                        if let Some(input) = input {
                            transcript.pager.handle_search_input(
                                input,
                                &ctx,
                                &mut transcript.index,
                            );
                        }
                    } else if let Some(action) = action {
                        let renderer = transcript_tool_renderer(
                            transcript.pager.details(),
                            width,
                            self.color,
                            &self.tool_theme,
                        );
                        transcript.pager.handle_action_with_renderer(
                            action,
                            &ctx,
                            &mut transcript.index,
                            &renderer,
                        );
                    }
                }
            }
        }
    }

    /// Draw the transcript over the live view, or close it.
    fn draw_transcript(&mut self, view: &View<'_>) {
        let Some(transcript) = self.transcript.as_mut() else {
            let _ = self.terminal.close_overlay();
            return;
        };
        let ctx = transcript_context(view);
        if transcript.dirty {
            transcript
                .pager
                .content_changed(&ctx, &mut transcript.index);
            transcript.dirty = false;
        }
        let _ = self.terminal.open_overlay();
        let width = self.terminal.viewport().width;
        let height = self.terminal.viewport().height;
        transcript.pager.resize(width, height);
        let details = transcript.pager.details();
        let renderer = transcript_tool_renderer(details, width, self.color, &self.tool_theme);
        let _ = self
            .terminal
            .draw_overlay(|area, buf| transcript.pager.draw(area, buf, &ctx, &renderer));
    }
}

impl ComposerDeadline for TuiScreen {
    fn tick(&mut self, now: Instant) {
        if let Some(dialog) = self.launch_dialog.as_mut() {
            dialog.tick(now);
        }
        let _ = self.composer.tick(now);
        // The loop reads the draft a flushed burst left right after this tick.
        let _ = reported_snapshot(&self.composer, &mut self.reported);
        self.sync_title();
    }

    fn next_tick(&self) -> Option<Instant> {
        let dialog = self
            .launch_dialog
            .as_ref()
            .and_then(LaunchDialog::next_tick);
        match (self.composer.next_tick(), dialog) {
            (Some(composer), Some(dialog)) => Some(composer.min(dialog)),
            (composer, dialog) => composer.or(dialog),
        }
    }
}

impl StatusHost for TuiScreen {
    fn on_tick(&mut self, now: Instant) {
        let (Some(service), Some(document)) = (
            self.status.as_mut(),
            StatusDocument::from_inputs(&self.inputs),
        ) else {
            return;
        };
        let sender = self.sender.clone();
        let _ = service.on_tick(now, &document, move |result| {
            let _ = sender.send(ScreenInput::Status(result));
        });
    }

    /// The status command's next run, or the next whole second of a running tool call. Nothing
    /// is scheduled while no tool runs, so an idle screen draws nothing.
    fn next_wake(&self, now: Instant) -> Option<Instant> {
        let status = self
            .status
            .as_ref()
            .and_then(|service| service.next_wake(now));
        match (status, self.clock.next_tick(now)) {
            (Some(status), Some(tick)) => Some(status.min(tick)),
            (status, tick) => status.or(tick),
        }
    }

    fn shutdown(&mut self) -> impl std::future::Future<Output = ()> + Send {
        let status = self.status.as_mut();
        async move {
            if let Some(service) = status {
                service.shutdown().await;
            }
        }
    }
}

impl Screen for TuiScreen {
    fn acp_config(&self) -> talaria_app::acp::AcpConfig {
        self.config.acp.clone()
    }
    fn sync_providers(&mut self, view: Option<ProvidersView>) {
        if view.is_some() {
            self.open_overlay(Overlay::Providers);
        } else {
            self.close_overlay(Overlay::Providers);
        }
        self.providers_view = view;
    }

    fn provider_secret(&self, effect: &ProvidersEffect) -> Option<zeroize::Zeroizing<String>> {
        self.providers.secret_for(effect)
    }

    fn forget_provider_secret(&mut self) {
        self.providers.forget();
    }

    fn copy_provider_text(&mut self, text: &str) {
        let _ = self.providers.copy(&mut self.terminal, text);
    }
    fn sync_mcp(&mut self, view: Option<&talaria_core::mcp::McpView>) {
        self.mcp_view = view.cloned();
        if self.mcp_view.is_some() {
            self.open_overlay(Overlay::Mcp);
        } else {
            self.mcp_wiring.forget();
            self.close_overlay(Overlay::Mcp);
        }
    }

    fn take_mcp_secret(
        &mut self,
        effect: &talaria_core::mcp::McpEffect,
    ) -> Option<zeroize::Zeroizing<String>> {
        let secret = self.mcp_wiring.secret_for(effect);
        if let Some(slot) = effect.secret_slot() {
            self.mcp_wiring.forget_slot(slot);
        }
        secret
    }

    fn forget_mcp_secrets(&mut self) {
        self.mcp_wiring.forget();
    }

    fn open_mcp_url(&mut self, url: &str) -> bool {
        let Some(argv) = talaria_app::providers::opener_argv(url) else {
            return false;
        };
        let Some((program, args)) = argv.split_first() else {
            return false;
        };
        match std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                tokio::task::spawn_blocking(move || {
                    let _ = child.wait();
                });
                true
            }
            Err(_) => false,
        }
    }

    fn copy_mcp_text(&mut self, text: &str) {
        let _ = self.terminal.copy_to_clipboard(text);
    }

    fn sync_cron(
        &mut self,
        mut view: Option<talaria_core::cron::CronView>,
        history_focused: bool,
    ) -> bool {
        use talaria_core::cron::CronAction;
        if let Some(view) = &mut view {
            view.hints = view
                .hints
                .split(" · ")
                .map(|hint| {
                    let Some((key, description)) = hint.split_once(' ') else {
                        return hint.to_owned();
                    };
                    let action = match key {
                        "ctrl+n" => CronAction::New,
                        "ctrl+t" => CronAction::RunNow,
                        "ctrl+e" => CronAction::TogglePause,
                        "ctrl+d" => CronAction::Remove,
                        "ctrl+r" => CronAction::Reload,
                        "ctrl+s" => CronAction::Save,
                        "o" => CronAction::OpenRun,
                        _ => return hint.to_owned(),
                    };
                    let label = self.keymap.cron_labels(action);
                    let label = if label.is_empty() {
                        "unbound".into()
                    } else {
                        label
                    };
                    format!("{label} {description}")
                })
                .collect::<Vec<_>>()
                .join(" · ");
        }
        self.cron_history_focused = history_focused;
        let changed = match &view {
            Some(view) => self.cron_gate.needs_draw(view),
            None => {
                self.cron_gate = talaria_tui::cron::CronFrameGate::new();
                self.close_overlay(Overlay::Cron);
                self.cron.is_some()
            }
        };
        self.cron = view;
        self.cron_dirty |= changed;
        changed
    }
    fn edit_connector_secret(&mut self, key: &str) {
        self.connectors_wiring.edit_secret(key.to_owned());
    }
    fn forget_connector_secrets(&mut self) {
        self.connectors_wiring.forget();
    }
    fn connector_secrets(
        &self,
        effect: &talaria_core::connectors::ConnectorsEffect,
    ) -> Option<BTreeMap<String, zeroize::Zeroizing<String>>> {
        self.connectors_wiring.secrets_for(effect)
    }
    fn configure_model(&self) -> Option<(String, String)> {
        self.config.configure.clone()
    }

    fn pane_settings(&self) -> talaria_app::panes::PaneSettings {
        self.config.panes.clone()
    }

    fn pane_size(&self) -> (u16, u16) {
        (self.terminal.viewport().width, self.rows)
    }

    type Input = ScreenInput;

    fn subagents_config(&self) -> talaria_app::subagents::SubagentsConfig {
        self.config.subagents
    }

    fn model_list_height(&self) -> usize {
        terminal_size().map_or(10, |(_, rows)| usize::from(rows.saturating_sub(3)).max(3))
    }

    fn sync_profiles(&mut self, view: Option<&ProfilesView>) {
        let Some(view) = view else {
            self.close_overlay(Overlay::Profiles);
            return;
        };
        self.open_overlay(Overlay::Profiles);
        let renderer = &mut self.profiles;
        let _ = self.terminal.draw_overlay_with_cursor(|area, buffer| {
            renderer.draw(view, area, buffer);
            profiles_cursor(view, area)
        });
    }

    fn profiles_page_size(&self, view: &ProfilesView) -> usize {
        let (width, height) = self.pane_size();
        talaria_tui::profiles::list_page_size(view, ratatui::layout::Rect::new(0, 0, width, height))
    }

    fn edit_profile_soul(&mut self, text: String) -> Option<RawEdit> {
        let argv = match editor::resolve_editor(
            Some(&self.config.editor.command),
            &|key| std::env::var(key).ok(),
            &executable_on_path,
        ) {
            Ok(argv) => argv,
            Err(editor::EditorError::Parse(reason)) => {
                return Some(RawEdit::Failed(editor::parse_notice(&reason)));
            }
        };
        if let Err(error) = self.release_terminal() {
            self.take_terminal();
            return Some(RawEdit::Failed(format!(
                "Could not release the terminal ({:?}).",
                error.kind()
            )));
        }
        let sender = self.sender.clone();
        let edited = tokio::task::spawn_blocking(move || {
            match tempfile::Builder::new().prefix("talaria-soul-").tempdir() {
                Ok(directory) => talaria_app::profiles_editor::edit_text_raw(
                    &argv,
                    &text,
                    directory.path(),
                    directory.path(),
                    talaria_app::profiles_editor::MAX_SOUL_BYTES,
                ),
                Err(_) => RawEdit::Failed(
                    "Could not create a private temporary directory; nothing was sent.".to_string(),
                ),
            }
        });
        tokio::spawn(async move {
            let result = edited.await.unwrap_or_else(|_| {
                RawEdit::Failed("The editor stopped unexpectedly; nothing was sent.".to_string())
            });
            let _ = sender.send(ScreenInput::ProfileEdited(result));
        });
        None
    }

    fn sync_models(&mut self, picker: Option<PickerView<'_>>, confirmation: Option<&Confirmation>) {
        if self.overlay == Overlay::Profiles {
            return;
        }
        match picker {
            Some(view) => {
                self.open_overlay(Overlay::Models);
                self.picker = Some(ModelOverlay::from_view(view));
                self.draw_picker();
            }
            None => {
                self.picker = None;
                self.close_overlay(Overlay::Models);
            }
        }
        if self.confirmation.as_ref().map(|card| &card.token)
            != confirmation.map(|card| &card.token)
        {
            self.confirmation_focused = None;
        }
        self.confirmation = confirmation.cloned();
        self.sync_confirmation_focus(Instant::now());
    }

    fn sync_picker(&mut self, picker: Option<&PickerState>) {
        if self.overlay == Overlay::Profiles {
            return;
        }
        self.sync_session_picker(picker);
    }

    fn sync_fleet(&mut self, view: &View<'_>) {
        if view.profiles.is_some() {
            return;
        }
        if view.processes_open {
            return;
        }
        let Some(fleet) = view.fleet else {
            self.launch_dialog = None;
            self.close_overlay(Overlay::Fleet);
            return;
        };
        self.open_overlay(Overlay::Fleet);
        self.sync_launch_dialog(view);
        let now = Instant::now();
        let peeked = fleet.peek().map(|(key, _)| key.clone());
        if let Some(frame) = view
            .peek
            .filter(|frame| peeked.as_ref() == Some(&frame.key))
        {
            let open: Vec<&PromptCell> = frame.state.open_prompts().collect();
            self.peek_prompts.sync(
                Some(frame.key.session_id.as_str()),
                &open,
                &masked_views(&frame.masked),
                true,
                now,
            );
        }
        let foreground = peeked
            .as_ref()
            .is_some_and(|key| fleet_peek_is_foreground(key, view.profile, view.session_id));
        let clock = unix_now();
        let panel = if foreground {
            view.state.map(|session| PeekPanel {
                session,
                stack: Some(&self.prompts),
                now: clock,
            })
        } else {
            view.peek
                .filter(|frame| peeked.as_ref() == Some(&frame.key))
                .map(|frame| PeekPanel {
                    session: &frame.state,
                    stack: Some(&self.peek_prompts),
                    now: clock,
                })
        };
        // A review opened from the fleet list draws in its place; Esc returns to the list.
        if let Some(review) = &view.review {
            let theme = &self.tool_theme;
            let _ = self
                .terminal
                .draw_overlay(|area, buf| draw_review(review, theme, area, buf));
            return;
        }
        let theme = FleetTheme::with_key_map(&self.keymap);
        let dialog = self.launch_dialog.as_ref();
        let labels = &view.pr_labels;
        let responder = view
            .acp_peek
            .and_then(|mapping| mapping.needs.first())
            .map(|need| need.responder.clone());
        if responder != self.acp_responder {
            self.acp_responder = responder;
            self.acp_option = 0;
        }
        let external = view.acp_peek.map(|mapping| ExternalPeekPanel {
            mapping,
            selected_option: self.acp_option,
            now: clock,
        });
        let _ = self.terminal.draw_overlay(|area, buf| {
            FleetView::draw_with_sources(fleet, panel, external, area, buf, &theme, labels);
            if let Some(dialog) = dialog {
                dialog.draw(dialog.area(area), buf);
            }
        });
    }

    fn input(&mut self, input: ScreenInput, view: &View<'_>, now: Instant) -> Vec<UserAction> {
        self.sync_agents(view);
        self.sync_prompts(view, now);
        self.sync_configure_prompts(view, now);
        self.sync_confirmation_focus(now);
        let mut actions = match input {
            ScreenInput::ProfileEdited(result) => {
                self.take_terminal();
                self.draw(view);
                vec![UserAction::Profiles(ProfilesInput::EditorReturned(result))]
            }
            ScreenInput::Terminal(event) => self.terminal_input(event, view, now),
            ScreenInput::Status(result) => {
                self.external.apply(&result);
                Vec::new()
            }
            ScreenInput::Edited(result) => {
                self.take_terminal();
                match result {
                    editor::EditResult::Loaded(text) => {
                        self.composer.load(text);
                        Vec::new()
                    }
                    editor::EditResult::Cleared => {
                        self.composer.load(String::new());
                        vec![UserAction::Notice(editor::cleared_notice())]
                    }
                    editor::EditResult::Unchanged { hint: true } => {
                        vec![UserAction::Notice(editor::unchanged_hint_notice())]
                    }
                    editor::EditResult::Failed(notice) => vec![UserAction::Notice(notice)],
                    _ => Vec::new(),
                }
            }
            ScreenInput::Resumed(result) => {
                self.take_terminal();
                self.expected.store(false, Ordering::SeqCst);
                if result == suspend::SuspendResult::NotStopped {
                    vec![UserAction::Notice(
                        "Suspend did not take effect: the stop signal is ignored in this terminal."
                            .into(),
                    )]
                } else {
                    Vec::new()
                }
            }
            ScreenInput::Continued => {
                let _ = self.guard.enter_raw();
                let _ = self.terminal.reassert_modes();
                self.cron_dirty = true;
                Vec::new()
            }
            ScreenInput::ComfortNotice(text) => vec![UserAction::Notice(text)],
        };
        self.sync_title();
        self.report_draft(&mut actions);
        self.draw(view);
        actions
    }

    fn draw(&mut self, view: &View<'_>) {
        if let Some(board) = view.kanban {
            self.open_overlay(Overlay::Kanban);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::kanban_view::KanbanView::draw(board, area, buf)
            });
            return;
        }
        self.close_overlay(Overlay::Kanban);
        if let Some(picker) = view.workflow_picker {
            self.open_overlay(Overlay::WorkflowProgress);
            let theme = talaria_tui::workflow_view::WorkflowTheme::default();
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::workflow_view::WorkflowView::render_picker(area, buf, picker, &theme)
            });
            return;
        }
        if let Some(progress) = view.workflow_progress {
            self.open_overlay(Overlay::WorkflowProgress);
            let theme = talaria_tui::workflow_view::WorkflowTheme::default();
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::workflow_view::WorkflowView::render(area, buf, progress, &theme)
            });
            return;
        }
        self.close_overlay(Overlay::WorkflowProgress);

        if let Some(mcp) = &self.mcp_view {
            let wiring = &self.mcp_wiring;
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::mcp::draw_mcp(mcp, &|slot| wiring.secret_len(slot), area, buf);
            });
            return;
        }
        if let Some(profiles) = &view.profiles {
            self.sync_profiles(Some(profiles));
            return;
        }
        self.close_overlay(Overlay::Profiles);
        if let Some(doctor) = view.doctor {
            self.open_overlay(Overlay::Doctor);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::doctor_view::draw_doctor(doctor, view.doctor_restart, area, buf);
            });
            return;
        }
        self.close_overlay(Overlay::Doctor);
        self.sync_agents(view);
        self.apply_transcript(view);
        self.sync_configure_prompts(view, Instant::now());
        if let Some(configure) = &view.configure {
            self.open_overlay(Overlay::Configure);
            let section = self.configure_prompts.section();
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::configure::draw_configure(configure, area, buf);
                if configure.phase == ConfigurePhase::Question
                    && let Some(section) = section
                {
                    let room = area.height.saturating_sub(2);
                    let height = section.height(area.width).min(room);
                    let card = ratatui::layout::Rect::new(
                        area.x,
                        area.y.saturating_add(1),
                        area.width,
                        height,
                    );
                    ratatui::widgets::Widget::render(ratatui::widgets::Clear, card, buf);
                    section.render(card, buf);
                }
            });
            return;
        }
        self.close_overlay(Overlay::Configure);
        if let Some(connectors) = &view.connectors {
            let mut connectors = connectors.clone();
            if let talaria_core::connectors::Screen::LinkCard { ssh, .. } = &mut connectors.screen {
                *ssh = self.connectors_ssh;
            }
            if !matches!(
                connectors.screen,
                talaria_core::connectors::Screen::Prompt { .. }
                    | talaria_core::connectors::Screen::SecretEditor { .. }
                    | talaria_core::connectors::Screen::Confirm { .. }
                    | talaria_core::connectors::Screen::Review { .. }
            ) {
                for (key, _) in &mut connectors.footer {
                    if let Some(label) = self.keymap.connectors_labels_for_default(key) {
                        *key = label;
                    }
                }
            }
            let lens = self.connectors_wiring.secret_lens();
            let palette = self.palette;
            self.open_overlay(Overlay::Connectors);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::connectors::draw_connectors_with_palette(
                    &connectors,
                    &lens,
                    &palette,
                    area,
                    buf,
                );
            });
            return;
        }
        if self.overlay == Overlay::Connectors {
            self.connectors_wiring.forget();
        }
        self.close_overlay(Overlay::Connectors);
        if let Some(memory) = &view.memory {
            let mut memory = memory.clone();
            if !memory.editing && memory.confirm.is_none() {
                for (key, _) in &mut memory.footer {
                    if let Some(action) = memory_footer_key(key) {
                        let labels = self.keymap.memory_chords(&action);
                        *key = if labels.is_empty() {
                            "(unbound)".into()
                        } else {
                            labels.join("/")
                        };
                    }
                }
            }
            self.open_overlay(Overlay::Memory);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::memory::draw_memory(&memory, area, buf);
            });
            return;
        }
        self.close_overlay(Overlay::Memory);
        if let Some(capabilities) = &view.capabilities {
            self.open_overlay(Overlay::Capabilities);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::capabilities::draw_capabilities(capabilities, area, buf);
            });
            return;
        }
        self.close_overlay(Overlay::Capabilities);
        if let Some(cron) = self.cron.clone() {
            if self.cron_dirty || self.overlay != Overlay::Cron {
                self.open_overlay(Overlay::Cron);
                let _ = self.terminal.draw_overlay(|area, buf| {
                    talaria_tui::cron::draw_cron(&cron, area, buf);
                });
                self.cron_dirty = false;
            }
            return;
        }
        if let Some(settings) = &view.settings {
            let mut settings = settings.clone();
            settings.help = self.keymap.settings_help();
            self.open_overlay(Overlay::Settings);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::settings::draw_settings(&settings, area, buf);
            });
            return;
        }
        self.close_overlay(Overlay::Settings);
        if let Some(providers) = &self.providers_view {
            let wiring = &self.providers;
            let _ = self
                .terminal
                .draw_overlay(|area, buffer| wiring.draw(providers, area, buffer));
            return;
        }
        if let Some(roles) = &view.model_roles {
            self.open_overlay(Overlay::ModelRoles);
            let _ = self.terminal.draw_overlay(|area, buf| {
                talaria_tui::model_roles::draw_model_roles(roles, area, buf);
            });
            return;
        }
        self.close_overlay(Overlay::ModelRoles);
        if view.processes_open {
            if self.overlay != Overlay::Processes {
                if let Some(state) = view.state {
                    self.processes.reopen(state.background());
                }
                self.open_overlay(Overlay::Processes);
            }
            if let Some(state) = view.state {
                let theme = background_theme(self.color);
                let processes = &self.processes;
                let _ = self.terminal.draw_overlay(|area, buf| {
                    processes.render(
                        state.background(),
                        view.now_ms,
                        state.background().wall_s(),
                        area,
                        buf,
                        &theme,
                    );
                });
            }
            return;
        }
        self.close_overlay(Overlay::Processes);
        // A review opened with `/changes` has the overlay to itself.
        match (&view.review, view.fleet) {
            (Some(review), None) => {
                self.open_overlay(Overlay::Review);
                let theme = &self.tool_theme;
                let _ = self
                    .terminal
                    .draw_overlay(|area, buf| draw_review(review, theme, area, buf));
                return;
            }
            _ => self.close_overlay(Overlay::Review),
        }
        let now = Instant::now();
        if let Some(cwd) = view.folder.and_then(|folder| folder.cwd.as_deref()) {
            self.folder = std::path::PathBuf::from(cwd);
        }
        self.connecting = view.state.is_none() && view.link == LinkView::Connecting;
        self.inputs = status_inputs(view, self.inputs.home_dir.clone());
        self.sync_prompts(view, now);
        self.sync_queue(view);
        self.clock.observe(
            view.state.map(|state| state.cells()).unwrap_or_default(),
            now,
        );
        self.cells.set_clock(&self.clock, now);
        self.sync_confirmation_focus(now);
        if let Some(rewind) = &view.rewind {
            if !self.rewind_open {
                let _ = self.terminal.open_overlay();
                self.rewind_open = true;
            }
            let theme = &self.rewind_theme;
            let _ = self
                .terminal
                .draw_overlay(|area, buf| draw_rewind(rewind, area, buf, theme));
            return;
        }
        if self.rewind_open {
            let _ = self.terminal.close_overlay();
            self.rewind_open = false;
        }
        if self.overlay == Overlay::Agents {
            self.draw_agents(view);
            return;
        }
        if self.overlay == Overlay::Models && self.transcript.is_none() {
            self.draw_picker();
            return;
        }
        if self.overlay == Overlay::Sessions {
            if let Some(state) = view.picker {
                self.draw_session_picker(state);
            }
            return;
        }
        if self.overlay == Overlay::Fleet {
            self.sync_fleet(view);
            return;
        }
        let width = self.terminal.viewport().width;
        let mut tail: Vec<Line<'static>> = view
            .state
            .map(|state| {
                let limit = usize::from(self.terminal.viewport().height)
                    .saturating_mul(2)
                    .max(PREVIEW_MIN_LINES);
                let current = &self.theme;
                let captured = &mut self.captured;
                let tools = &self.cells;
                state
                    .live_tail()
                    .iter()
                    .flat_map(|cell| {
                        if matches!(cell, Cell::Tool(_)) {
                            return match preview(cell, limit) {
                                Some(trimmed) => tools.lines(&trimmed, width),
                                None => tools.lines(cell, width),
                            };
                        }
                        let theme = match themed_cell_id(cell) {
                            Some(id) => Arc::clone(
                                captured.entry(id).or_insert_with(|| Arc::clone(current)),
                            ),
                            None => Arc::clone(current),
                        };
                        let cells = ThemedCells { theme: &theme };
                        match preview(cell, limit) {
                            Some(trimmed) => cells.lines(&trimmed, width),
                            None => cells.lines(cell, width),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        if tail.len() > MAX_TAIL_ROWS {
            tail.drain(..tail.len() - MAX_TAIL_ROWS);
        }
        let tail = MarkdownTailSection::new(tail);
        let drafting = DraftingLine {
            drafted: view.state.and_then(|state| state.drafting_tool()),
            theme: &self.tool_theme,
        };
        let connecting = PlainTextSection::new(CONNECTING);
        let status = StatusSection::new(&self.inputs, self.external.segment(), &self.palette);
        let prompts = self.prompts.section();
        let palette = PaletteSection::new(view.palette, self.rows);
        let confirmation = if prompts.is_none() {
            self.confirmation.as_ref().map(|card| {
                let section =
                    ModelConfirmSection::new(card, view.model.unwrap_or("the current model"));
                if self.composer.is_empty() {
                    section
                } else {
                    section.waiting_on_draft()
                }
            })
        } else {
            None
        };
        let composer_is_empty = self.composer.is_empty();
        let side_theme = SideTheme::default();
        let side = view.state.map(|state| SideTasksSection {
            tasks: state.side(),
            now_ms: view.now_ms,
            theme: &side_theme,
        });
        let background_theme = background_theme(self.color);
        let background = view.state.map(|state| BackgroundSection {
            work: state.background(),
            width_hint: width,
            theme: &background_theme,
        });
        let mention = self
            .attachments
            .mention
            .as_ref()
            .filter(|_| !self.composer.text().starts_with('/'))
            .map(MentionSection::new);
        let queue = MarkdownTailSection::new(
            queue_view(view)
                .map(|queue| render_preview(&queue, width, self.composer.is_empty()))
                .unwrap_or_default(),
        );
        let composer = self.composer.section(width, COMPOSER_ROWS);
        let compacting = CompactingSection {
            line: view.compacting_line,
            now_ms: view.now_ms,
        };
        // The task checklist sits above the composer, and is one row on a short screen.
        let max_rows = if self.rows < SHORT_TERMINAL_ROWS {
            1
        } else {
            PANEL_MAX_ROWS
        };
        let checklist = view.checklist.map(|list| {
            ChecklistSection::new(
                list,
                view.checklist_expanded,
                max_rows,
                self.keymap.label(Action::ToggleTasks),
                &self.checklist_style,
            )
        });
        let subagent_style = if self.color {
            SubagentStyle::default()
        } else {
            SubagentStyle::plain()
        };
        let subagent_hint = self.keymap.label(Action::AgentsPanel);
        let subagents = view.subagents.and_then(|hub| {
            let key = hub.foreground_key()?;
            let tree = hub.tree(key)?;
            Some(SubagentDock::new(
                tree,
                tree.counts(),
                None,
                hub.spawning(),
                subagent_hint.as_deref(),
                &subagent_style,
                hub.cfg().dock_rows,
                self.rows,
            ))
        });
        let automation_theme = AutomationTheme::default();
        let automation = AutomationSection {
            state: view.automation,
            now_ms: view.automation_now_ms,
            theme: &automation_theme,
        };
        let mut sections: Vec<&dyn LiveSection> = vec![&tail, &drafting];
        if let Some(prompts) = &prompts {
            sections.push(prompts);
        }
        if let Some(confirmation) = &confirmation {
            sections.push(confirmation);
        }
        if self.connecting {
            sections.push(&connecting);
        }
        if view.compacting_line.is_some() {
            sections.push(&compacting);
        }
        let search = self
            .search
            .as_ref()
            .map(|s| s.section(std::time::SystemTime::now()));
        if let Some(search) = &search {
            if let Some(side) = &side {
                sections.push(side);
            }
            if let Some(background) = &background {
                sections.push(background);
            }
            sections.push(search);
        } else {
            sections.push(&queue);
            if let Some(mention) = &mention {
                sections.push(mention);
            }
            if let Some(checklist) = &checklist {
                sections.push(checklist);
            }
            if let Some(subagents) = &subagents {
                sections.push(subagents);
            }
            if let Some(side) = &side {
                sections.push(side);
            }
            if let Some(background) = &background {
                sections.push(background);
            }
            sections.push(&composer);
            if !composer_is_empty || prompts.is_none() {
                sections.push(&palette);
            }
        }
        sections.push(&automation);
        sections.push(&status);
        let _ = self.terminal.draw_live(&LiveView { sections });
        if !self.compact_started {
            self.compact_started = true;
            let history = self.history.clone();
            let sender = self.sender.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(error) = history.compact() {
                    let _ = sender.send(ScreenInput::ComfortNotice(
                        talaria_app::comforts::prompt_history::compact_failed_notice(
                            &editor::io_reason(&error),
                        ),
                    ));
                }
            });
        }
        drop(search);
        self.draw_transcript(view);
    }

    fn transcript(&mut self, action: TranscriptAction) {
        self.transcript_actions.push(action);
    }

    fn commit_lines(&mut self, lines: Vec<String>) {
        let _ = self
            .terminal
            .commit_lines(lines.into_iter().map(Line::from).collect());
    }

    fn commit_context(&mut self, block: &ContextBlock) {
        let width = self.terminal.viewport().width;
        let style = ContextStyle::standard();
        let lines = match block {
            ContextBlock::Breakdown { typed, breakdown } => {
                context_block(typed, breakdown, width, &style)
            }
            ContextBlock::Prebuild { typed, last } => {
                prebuild_block(typed, last.as_ref(), width, &style)
            }
            ContextBlock::Usage { typed, report } => usage_block(typed, report, width, &style),
            ContextBlock::Compact { typed, result } => compact_block(typed, result, width, &style),
            ContextBlock::Error {
                typed,
                command,
                method,
                sid,
                error,
            } => error_block(typed, command, method, sid, error, width, &style),
            ContextBlock::Notice(notice) => notice_block(notice, width, &style),
        };
        let _ = self.terminal.commit_lines(lines);
    }

    fn commit_checklist(&mut self, commit: &ChecklistCommit) {
        let width = self.terminal.viewport().width;
        let lines = committed_block_lines(commit, width, &self.checklist_style);
        let _ = self.terminal.commit_lines(lines);
    }

    fn commit_subagent_batch(&mut self, batch: &FinishedBatch) {
        let style = if self.color {
            SubagentStyle::default()
        } else {
            SubagentStyle::plain()
        };
        let lines = batch_block_lines(batch, self.terminal.viewport().width, &style);
        let _ = self.terminal.commit_lines(lines);
    }

    fn commit_cells(&mut self, cells: &[Cell]) {
        let render = CapturedCells {
            current: &self.theme,
            captured: &self.captured,
            tools: &self.cells,
        };
        let _ = self.terminal.commit(cells, &render);
        for id in cells.iter().filter_map(themed_cell_id) {
            self.captured.remove(&id);
        }
    }

    fn unrecord_submit(&mut self, text: &str) {
        self.composer.forget_newest_history(text);
    }

    fn offer_draft(&mut self, text: &str) {
        if self.composer.is_empty() && self.editing.is_none() {
            self.composer.load(text.to_string());
        }
    }

    fn clear_submitted_draft(&mut self, text: &str) -> Option<(String, usize)> {
        let snapshot = clear_queue_draft(&mut self.composer, &mut self.reported, text)?;
        self.sync_title();
        Some(snapshot)
    }

    fn load_draft(&mut self, text: &str) {
        self.composer.load(text.to_string());
        self.reported = (self.composer.text().to_owned(), self.composer.caret());
        self.sync_title();
    }

    fn edit_draft(&mut self, range: std::ops::Range<usize>, text: &str) -> Option<(String, usize)> {
        self.composer.replace_range(range, text);
        reported_snapshot(&self.composer, &mut self.reported)
    }

    fn draft_snapshot(&self) -> Option<(String, usize)> {
        (!self.composer.is_collecting_burst())
            .then(|| (self.composer.text().to_string(), self.composer.caret()))
    }

    fn flush_draft(&mut self) -> Option<(String, usize)> {
        self.composer.flush_pending();
        reported_snapshot(&self.composer, &mut self.reported)
    }

    fn sync_attachments(&mut self, view: AttachmentView) {
        self.composer.set_highlights(
            view.highlights
                .iter()
                .map(|(range, style)| {
                    (
                        range.clone(),
                        match style {
                            LabelStyle::Accent => Style::default().fg(Color::Cyan),
                            LabelStyle::Dimmed => Style::default().add_modifier(Modifier::DIM),
                        },
                    )
                })
                .collect(),
        );
        self.attachments = view;
    }

    fn attachment_rule(&self) -> CollapseRule {
        self.attachment_rule
    }

    fn restore(&mut self) {
        let _ = self.terminal.restore();
        let _ = self.guard.leave_raw();
        if let Some(reporter) = self.reporter.take() {
            reporter.release(Duration::from_millis(500));
        }
    }

    fn toggle_label(&self) -> Option<String> {
        self.keymap.label(Action::ApprovalsToggle)
    }

    fn interrupt_label(&self) -> Option<String> {
        self.keymap.label(Action::Interrupt)
    }
    fn startup_appearance(&self) -> Option<(TerminalColors, (Polarity, PolaritySource))> {
        Some((self.look.terminal, self.look.polarity))
    }

    fn appearance_changed(&mut self, view: &AppearanceView<'_>) {
        let theme = self.look.resolve(view);
        self.set_theme(theme);
    }

    fn theme_report(&self) -> Option<ThemeReport> {
        Some(self.theme.report.clone())
    }

    fn attention_config(&self) -> AttentionConfig {
        AttentionConfig {
            bell: self.config.notifications.bell.clone(),
            desktop: self.config.notifications.desktop.clone(),
            done_after: Duration::from_secs(self.config.notifications.done_after_seconds),
        }
    }

    fn fleet_attention_config(&self) -> FleetAttentionConfig {
        FleetAttentionConfig {
            bell: self.config.notifications.bell.clone(),
            desktop: self.config.notifications.desktop.clone(),
            done_after: Duration::from_secs(self.config.notifications.done_after_seconds),
            needs_you: self.config.notifications.fleet_needs_you,
            finished: self.config.notifications.fleet_finished,
        }
    }

    fn attention_key_label(&self) -> Option<String> {
        // The start-up notice names the key the way a person presses it.
        self.keymap
            .label(Action::AttentionNext)
            .map(|label| label.to_ascii_lowercase())
    }

    fn alert(&mut self, alert: &Alert) {
        let tmux = std::env::var_os("TMUX").is_some_and(|s| !s.is_empty());
        let bytes =
            talaria_tui::terminal::alert_bytes(&alert.text, alert.bell, alert.desktop, tmux);
        let _ = self
            .terminal
            .alert(bytes, talaria_core::attention::urgency(alert.kind));
    }

    fn report_pane(&mut self, pane: &PaneReport, session: Option<(&str, &str)>) {
        self.pane = pane.clone();
        self.report_session = session.map(|(id, profile)| (id.into(), profile.into()));
        if let Some(reporter) = &self.reporter {
            reporter.report(pane, session);
            for notice in reporter.take_notices() {
                let _ = self.sender.send(ScreenInput::ComfortNotice(notice));
            }
        }
    }
}

impl TuiScreen {
    fn release_terminal(&mut self) -> io::Result<()> {
        if let Some(input) = &self.input {
            input.pause()?;
        }
        self.terminal.release()?;
        self.guard.leave_raw()?;
        self.released.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn take_terminal(&mut self) {
        let _ = self.guard.enter_raw();
        if let Ok((cols, rows)) = terminal_size() {
            let _ = self.terminal.resize(cols, rows);
        }
        let _ = self
            .terminal
            .reacquire(&mut || talaria_tui::terminal::probe_reacquire(PROBE_DEADLINE));
        self.released.store(false, Ordering::SeqCst);
        if let Some(input) = &self.input {
            input.resume();
        }
        if let Some(reporter) = &self.reporter {
            reporter.report(
                &self.pane,
                self.report_session
                    .as_ref()
                    .map(|(id, profile)| (id.as_str(), profile.as_str())),
            );
        }
    }

    fn start_editor(&mut self) -> Vec<UserAction> {
        let argv = match editor::resolve_editor(
            Some(&self.config.editor.command),
            &|key| std::env::var(key).ok(),
            &executable_on_path,
        ) {
            Ok(argv) => argv,
            Err(editor::EditorError::Parse(reason)) => {
                return vec![UserAction::Notice(editor::parse_notice(&reason))];
            }
        };
        if let Err(error) = self.release_terminal() {
            self.take_terminal();
            return vec![UserAction::Notice(format!(
                "Could not release the terminal ({:?}).",
                error.kind()
            ))];
        }
        let draft = self.composer.text().to_string();
        let folder = self.folder.clone();
        let sender = self.sender.clone();
        tokio::task::spawn_blocking(move || {
            let result = editor::edit_draft(&argv, &draft, &folder, &std::env::temp_dir());
            let _ = sender.send(ScreenInput::Edited(result));
        });
        Vec::new()
    }

    fn start_suspend(&mut self) -> Vec<UserAction> {
        if !suspend::job_control_available() {
            return vec![UserAction::Notice("Ctrl+Z cannot suspend here: Talaria was not started from a shell with job control. Use /quit; the session keeps running in Hermes.".into())];
        }
        self.continued.store(false, Ordering::SeqCst);
        self.expected.store(true, Ordering::SeqCst);
        if self.release_terminal().is_err() {
            self.take_terminal();
            self.expected.store(false, Ordering::SeqCst);
            return Vec::new();
        }
        let flag = Arc::clone(&self.continued);
        let sender = self.sender.clone();
        tokio::task::spawn_blocking(move || {
            let result = suspend::suspend_process(&flag);
            let _ = sender.send(ScreenInput::Resumed(result));
        });
        Vec::new()
    }
}

fn executable_on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |p: &std::path::Path| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        return executable(std::path::Path::new(name));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| executable(&dir.join(name))))
}

fn fail(failure: &LaunchFailure) -> ExitCode {
    eprint!("{}", failure.render());
    ExitCode::from(failure.exit_code)
}

fn user_config() -> LoadedConfig {
    match std::env::var_os("HOME") {
        Some(home) => config::load_user_config(&PathBuf::from(home)),
        None => config::parse(""),
    }
}

fn status_service(
    loaded: &LoadedConfig,
    folder: &std::path::Path,
    notices: &mut Vec<String>,
) -> Option<StatusCommandService> {
    let command = loaded.config.status.command.as_deref()?;
    match parse_command(command) {
        Ok(argv) => Some(StatusCommandService::new(
            CommandSpec {
                argv,
                launch_dir: folder.to_path_buf(),
                env_allowlist: loaded.config.environment.allowlist.clone(),
            },
            interval_from_config(loaded.config.status.interval_seconds),
            std::env::vars().collect(),
            hermes_client::is_suspicious_key,
            CommandLimits::default(),
        )),
        Err(reason) => {
            notices.push(reason);
            None
        }
    }
}

/// The `--resume` list, on the normal screen: connect, list, ask. `Ok(None)` is a cancel or an
/// empty list.
fn pick_session(
    runtime: &tokio::runtime::Runtime,
    launch: &Launch,
    home: &std::path::Path,
    launch_id: &str,
    preopened: Option<OpenConnection>,
) -> Result<Option<(OpenConnection, String, Option<String>)>, LaunchFailure> {
    let (connection, rows) = runtime.block_on(async {
        let connection = match preopened {
            Some(connection) => connection,
            None => app::connect(launch, home).await?,
        };
        match app::list_sessions(&connection, launch_id).await {
            Ok(rows) => Ok((connection, rows)),
            Err(failure) => {
                connection.link.close().await;
                Err(failure)
            }
        }
    })?;
    let picked = crate::picker::pick(
        &rows,
        &mut io::stdin().lock(),
        &mut io::stdout(),
        &launch.profile,
    );
    match picked {
        Ok(Some(id)) => {
            let title = rows
                .iter()
                .find(|row| row.session_id == id)
                .and_then(|row| row.title.clone());
            Ok(Some((connection, id, title)))
        }
        Ok(None) | Err(_) => {
            runtime.block_on(connection.link.close());
            Ok(None)
        }
    }
}

fn print_outcome(outcome: &Outcome) {
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);
}

pub fn run(launch: Launch) -> ExitCode {
    run_with_ultracode(launch, false)
}

pub fn run_with_ultracode(launch: Launch, ultracode: bool) -> ExitCode {
    run_selected(launch, ultracode, None)
}

pub fn run_remote(
    launch: Launch,
    ultracode: bool,
    cfg: hermes_client::remote::RemoteConfig,
    store: hermes_client::remote::TokenStore,
) -> ExitCode {
    run_selected(launch, ultracode, Some((cfg, store)))
}

fn run_selected(
    mut launch: Launch,
    ultracode: bool,
    remote: Option<(
        hermes_client::remote::RemoteConfig,
        hermes_client::remote::TokenStore,
    )>,
) -> ExitCode {
    match launch.folder.canonicalize() {
        Ok(folder) if folder.is_dir() => launch.folder = folder,
        _ => return fail(&failure::folder_unreadable()),
    }
    if let Some(record) = launch.record.take() {
        let record = if record.is_absolute() {
            record
        } else {
            launch.folder.join(record)
        };
        if let Err(failure) = app::check_record_path(&record) {
            return fail(&failure);
        }
        launch.record = Some(record);
    }
    let loaded = user_config();
    let (keymap, key_notices) = KeyMap::from_entries(&loaded.config.keys);
    let mut notices = loaded.notices.clone();
    notices.extend(key_notices);
    // The environment is read once, here.
    let env = EnvView::from_process();
    notices.extend(detect::env_notices(&env));
    let settings = ThemeSettings {
        ultracode,
        mode: loaded.config.theme.mode,
        look: Look {
            follow_hermes_skin: loaded.config.theme.follow_hermes_skin,
            depth: detect::depth(&env, loaded.config.theme.colors),
            motion: detect::motion(&env, loaded.config.ui.reduced_motion.then_some(true)),
            terminal: TerminalColors::default(),
            polarity: (Polarity::Dark, PolaritySource::Default),
        },
        env,
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return fail(&failure::no_terminal());
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return fail(&failure::link_failure("the async runtime could not start")),
    };
    let home = hermes_client::default_home();
    let launch_id = new_launch_id();
    let mut setup = RunSetup {
        herdr: loaded
            .config
            .herdr
            .enabled
            .then(|| loaded.config.herdr.fleet_config()),

        home: home.clone(),
        launch_id: launch_id.clone(),
        preopened: None,
        list_title: None,
        notices,
        keys: loaded.config.keys.clone(),
        fleet: talaria_app::fleet::attaches_after_first_frame(
            false,
            loaded.config.fleet.enabled && remote.is_none(),
        )
        .then(|| loaded.config.fleet.clone()),
    };
    if let Some((cfg, store)) = remote {
        let dial = hermes_client::remote::RemoteDial::new(cfg, store, launch.profile.clone());
        let dial = match launch.record.as_deref() {
            Some(path) => dial.with_recording(path.to_path_buf()),
            None => dial,
        };
        match runtime.block_on(talaria_app::connection::ConnectionService::open_with_dial(
            &launch, dial,
        )) {
            Ok(connection) => setup.preopened = Some(connection),
            Err(failure) => {
                eprintln!(
                    "talaria: {}\nNext step: {}",
                    failure.what,
                    failure.next_step.trim_end_matches('.')
                );
                return ExitCode::from(failure.exit_code);
            }
        }
    }
    if launch.mode == StartMode::Resume(None) {
        match pick_session(&runtime, &launch, &home, &launch_id, setup.preopened.take()) {
            Ok(Some((connection, id, title))) => {
                launch.mode = StartMode::Resume(Some(id));
                setup.preopened = Some(connection);
                setup.list_title = title;
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(failure) => return fail(&failure),
        }
    }
    let status = status_service(&loaded, &launch.folder, &mut setup.notices);
    let outcome = match session(
        &runtime,
        launch,
        setup,
        keymap,
        status,
        loaded.config,
        settings,
    ) {
        Ok(outcome) => outcome,
        Err(TerminalError::NotATerminal) => return fail(&failure::no_terminal()),
        Err(TerminalError::Io(_)) => {
            return fail(&failure::link_failure("the terminal could not be set up"));
        }
    };
    print_outcome(&outcome);
    runtime.shutdown_timeout(Duration::from_secs(1));
    ExitCode::from(outcome.code)
}

/// The theme settings read before the terminal is set up.
struct ThemeSettings {
    ultracode: bool,
    env: EnvView,
    mode: ThemeMode,
    look: Look,
}

/// Acquire the terminal, run the loop, restore. The guard drops before the caller prints.
fn session(
    runtime: &tokio::runtime::Runtime,
    launch: Launch,
    setup: RunSetup,
    keymap: KeyMap,
    status: Option<StatusCommandService>,
    config: config::Config,
    settings: ThemeSettings,
) -> Result<Outcome, TerminalError> {
    let attachment_rule = CollapseRule {
        lines: config.composer.paste_collapse_lines,
        chars: config.composer.paste_collapse_chars,
    };
    let guard = TerminalGuard::acquire()?;
    let probe = probe_startup(PROBE_DEADLINE)?;
    // Light or dark is decided once, from the start-up probe; `/theme` overrides it per session.
    let look = Look {
        terminal: probe.colors,
        polarity: detect::polarity(None, &settings.env, settings.mode, &probe.colors),
        ..settings.look
    };
    let theme = look.resolve(&AppearanceView {
        skin: None,
        skipped: &[],
        polarity: look.polarity,
    });
    let size = terminal_size().map_err(TerminalError::Io)?;
    let terminal = InlineTerminal::new(io::stdout(), size, LIVE_HEIGHT, probe, guard.link())
        .map_err(TerminalError::Io)?;
    let (sender, receiver) = mpsc::unbounded_channel();
    let color = theme.color;
    let history_dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".talaria");
    let workspace = talaria_app::comforts::prompt_history::history_workspace(&launch.folder);
    let (history, history_notice) = PromptHistory::open(
        &history_dir,
        config.history.save,
        &workspace.to_string_lossy(),
    );
    let mut setup = setup;
    setup.notices.extend(history_notice);
    let mut composer = Composer::from_startup(
        keymap.clone(),
        history.composer_seed(),
        config::composer_starts_with_vim(&config),
    );
    composer.set_keyboard_enhancement(probe.caps.keyboard_enhancement);
    composer.set_style(theme.composer);
    let mut screen = TuiScreen {
        providers: crate::providers_wiring::ProvidersWiring::new(),
        providers_view: None,
        guard,
        input: None,
        history,
        search: None,
        folder: launch.folder.clone(),
        reporter: HerdrReporter::from_env_with_notices(
            config.herdr.report_state,
            &|key| std::env::var(key).ok(),
            unix_millis,
            {
                let sender = sender.clone();
                move |notice| {
                    let _ = sender.send(ScreenInput::ComfortNotice(notice));
                }
            },
        ),
        config,
        pane: PaneReport {
            state: PaneState::Idle,
            message: None,
        },
        report_session: None,
        expected: Arc::new(AtomicBool::new(false)),
        released: Arc::new(AtomicBool::new(false)),
        continued: Arc::new(AtomicBool::new(false)),
        compact_started: false,
        terminal,
        composer,
        prompts: PromptStack::new(),
        configure_prompts: PromptStack::new(),
        configure_cancel_at: None,
        status,
        external: ExternalDisplay::default(),
        inputs: StatusInputs {
            home_dir: std::env::var("HOME").ok(),
            ..StatusInputs::default()
        },
        palette: theme.status,
        cells: tool_cells(&theme, &keymap),
        clock: ToolClock::new(),
        tool_theme: theme.tool.clone(),
        look,
        theme: Arc::new(theme),
        captured: HashMap::new(),
        checklist_style: if color {
            ChecklistStyle::default()
        } else {
            ChecklistStyle::plain()
        },
        color,
        sender: sender.clone(),
        connecting: true,
        keymap,
        reported: (String::new(), 0),
        rows: size.1,
        picker: None,
        profiles: ProfilesRenderer::default(),
        overlay: Overlay::Live,
        mcp_view: None,
        mcp_wiring: crate::mcp_wiring::McpWiring::default(),
        cron: None,
        cron_gate: talaria_tui::cron::CronFrameGate::new(),
        cron_history_focused: false,
        cron_dirty: false,
        connectors_wiring: crate::connectors_wiring::ConnectorsWiring::default(),
        connectors_ssh: std::env::var_os("SSH_CONNECTION").is_some()
            || std::env::var_os("SSH_TTY").is_some(),
        peek_prompts: PromptStack::new(),
        acp_option: 0,
        acp_responder: None,
        agents: None,
        agents_flash: None,
        agents_pause_pending: false,
        processes: ProcessesView::new(),
        launch_dialog: None,
        launch_draft: None,
        launch_seen: 0,
        confirmation: None,
        confirmation_focused: None,
        rewind_open: false,
        rewind_rows: 0,
        rewind_theme: rewind_theme(),
        attachments: AttachmentView::default(),
        attachment_rule,
        transcript: None,
        transcript_actions: Vec::new(),
        editing: None,
        edit_closing: false,
    };
    let notify = sender.clone();
    suspend::spawn_sigcont_watch(
        screen.expected.clone(),
        screen.released.clone(),
        screen.continued.clone(),
        move || {
            let _ = notify.send(ScreenInput::Continued);
        },
    )
    .map_err(TerminalError::Io)?;
    suspend::catch_interrupts_while_released().map_err(TerminalError::Io)?;
    screen.input = spawn_input(move |event| sender.send(ScreenInput::Terminal(event)).is_ok());
    let outcome = runtime.block_on(app::run_with_ultracode(
        launch,
        setup,
        &mut screen,
        receiver,
        settings.ultracode,
    ));
    screen.restore();
    drop(screen.input.take());
    drop(screen);
    Ok(outcome)
}

/// Rewind overlay colours. Without colour the warning is bold and the accent plain.
fn rewind_theme() -> RewindTheme {
    use ratatui::style::{Color, Modifier, Style};
    let color = std::env::var_os("NO_COLOR").is_none();
    RewindTheme {
        selected: Style::default().add_modifier(Modifier::REVERSED),
        dim: Style::default().add_modifier(Modifier::DIM),
        warning: if color {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        },
        accent: if color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        },
    }
}

fn sync_reported_draft(
    text: &str,
    caret: usize,
    reported: &mut (String, usize),
    actions: &mut Vec<UserAction>,
) {
    if actions
        .iter()
        .any(|action| matches!(action, UserAction::Submit(_)))
    {
        *reported = (String::new(), 0);
        return;
    }
    if (reported.0.as_str(), reported.1) == (text, caret) {
        return;
    }
    *reported = (text.to_owned(), caret);
    actions.insert(
        0,
        UserAction::Palette(PaletteInput::DraftChanged {
            text: text.to_owned(),
            caret,
        }),
    );
}

fn should_record_submission(text: &str, has_session: bool) -> bool {
    use talaria_core::side::{Submission, btw_command, classify};

    match classify(text) {
        Submission::ShellEmpty => false,
        Submission::Shell(_) => has_session,
        Submission::Prompt => match app::slash_line(text) {
            app::Slash::Btw(argument) => {
                has_session
                    && matches!(
                        btw_command(&argument, 0),
                        talaria_core::session::Event::SideAsked { .. }
                    )
            }
            _ => true,
        },
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::{
        CellLines, TuiScreen, attachment_key, composer_key_actions, confirmation_key,
        paste_actions, picker_key, preview_text, recall_draft, reported_snapshot,
        should_record_submission, status_inputs, sync_reported_draft, tool_cells,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    use talaria_app::app::AttachmentView;
    use talaria_app::app::{AppMessage, AppState, UserAction, update};
    use talaria_app::attachments::{Action as AttachmentAction, LabelStyle};
    use talaria_core::attachments::MentionView;
    use talaria_core::models::{ConfirmKey, PickerKey};
    use talaria_core::session::{Cell, CellId, ToolCell, ToolStatus};
    use talaria_tui::composer::{Composer, ComposerContext};
    use talaria_tui::keys::{Key, KeyMap};
    use talaria_tui::markdown::{MarkdownOptions, render_markdown};
    use talaria_tui::theme::{
        ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
        ThemeInputs,
    };

    #[test]
    fn busy_queue_shortcuts_keep_text_and_caret() {
        use talaria_core::queue::QueueIntent;
        use talaria_tui::keys::Action;
        for action in [Action::ComposerSendNow, Action::ComposerSteer] {
            let mut composer = Composer::new(KeyMap::defaults());
            composer.load("draft text".into());
            let left = Key::from(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
            let _ =
                composer.handle_key(left, ComposerContext { turn_running: true }, Instant::now());
            let caret = composer.caret();
            assert!(caret < composer.text().len());
            let intent = super::queue_draft_intent(&composer, Some(action)).unwrap();
            assert!(
                matches!(intent, QueueIntent::SendDraftNow(ref text) | QueueIntent::SteerText(ref text) if text == "draft text")
            );
            assert_eq!((composer.text(), composer.caret()), ("draft text", caret));
            let mut reported = ("draft text".to_owned(), caret);
            assert!(
                super::clear_queue_draft(&mut composer, &mut reported, "older draft").is_none()
            );
            assert_eq!((composer.text(), composer.caret()), ("draft text", caret));
            assert_eq!(
                super::clear_queue_draft(&mut composer, &mut reported, "draft text"),
                Some((String::new(), 0))
            );
            assert!(composer.text().is_empty());
        }
    }

    #[test]
    fn configured_pager_named_keys_survive_the_terminal_and_application_path() {
        use talaria_app::app::{AppEffect, TranscriptAction};
        use talaria_tui::keys::{Action, KeyContext};
        for (binding, code, modifiers) in [
            ("left", KeyCode::Left, KeyModifiers::NONE),
            ("right", KeyCode::Right, KeyModifiers::NONE),
            ("tab", KeyCode::Tab, KeyModifiers::NONE),
            ("delete", KeyCode::Delete, KeyModifiers::NONE),
            ("insert", KeyCode::Insert, KeyModifiers::NONE),
            ("f7", KeyCode::F(7), KeyModifiers::NONE),
            ("super+left", KeyCode::Left, KeyModifiers::SUPER),
            ("shift+tab", KeyCode::BackTab, KeyModifiers::NONE),
        ] {
            let entries = BTreeMap::from([("pager_line_up".into(), vec![binding.into()])]);
            let (map, notices) = KeyMap::from_entries(&entries);
            assert!(notices.is_empty(), "{binding}: {notices:?}");
            let raw = KeyEvent::new(code, modifiers);
            let input = super::input_key(&raw).unwrap_or_else(|| panic!("{binding} was dropped"));
            let mut state = AppState::new("pager-keys");
            state.pager_open = true;
            let effects = update(&mut state, AppMessage::Input(input));
            let routed = effects
                .iter()
                .find_map(|effect| match effect {
                    AppEffect::Transcript(TranscriptAction::Key(key)) => Some(*key),
                    _ => None,
                })
                .expect("key reaches the transcript");
            assert_eq!(
                map.resolve(&super::tui_key(routed), KeyContext::Pager),
                Some(Action::PagerLineUp),
                "{binding}"
            );
        }
    }

    #[test]
    fn control_u_routes_to_the_active_connector_filter() {
        use talaria_core::connectors::{
            CatalogRow, ConnectorsInput, ConnectorsKey, ConnectorsOpen, ConnectorsState,
            ConnectorsTab, Screen, update, view,
        };
        let now = Instant::now();
        let mut state = ConnectorsState::new();
        let _ = update(
            &mut state,
            ConnectorsInput::Open {
                open: ConnectorsOpen::Tab(ConnectorsTab::Connectors),
                profile: "fixture".into(),
                consented: true,
            },
            now,
        );
        let rows = ["github", "linear"]
            .into_iter()
            .map(|name| CatalogRow {
                slug: name.into(),
                name: name.into(),
                description: None,
                category: None,
            })
            .collect();
        let _ = update(&mut state, ConnectorsInput::CatalogReply(Ok(rows)), now);
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Filter), now);
        let _ = update(&mut state, ConnectorsInput::Paste("gi".into()), now);
        let key = Key::from(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        let routed = super::connectors_editor_key(key).expect("ctrl+u binding");
        let _ = update(&mut state, ConnectorsInput::Key(routed), now);
        let Screen::ConnectorList {
            rows,
            filter,
            selected,
        } = view(&state, now).screen
        else {
            panic!("connector list")
        };
        assert_eq!(filter.as_deref(), Some(""));
        assert_eq!((rows.len(), selected), (2, 0));
    }

    #[test]
    fn named_bindings_open_and_close_the_pager_through_the_application() {
        let mut state = AppState::new("pager-open-keys");
        state
            .key_bindings
            .insert("transcript_open".into(), vec!["left".into()]);
        state
            .key_bindings
            .insert("pager_close".into(), vec!["f7".into()]);
        let left = super::input_key(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)).unwrap();
        update(&mut state, AppMessage::Input(left));
        assert!(state.pager_open);
        let f7 = super::input_key(&KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE)).unwrap();
        update(&mut state, AppMessage::Input(f7));
        assert!(!state.pager_open);

        state.pager_open = true;
        state
            .key_bindings
            .insert("pager_search".into(), vec!["plus".into()]);
        let plus = super::input_key(&KeyEvent::new(
            KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE).code,
            KeyModifiers::NONE,
        ))
        .unwrap();
        update(&mut state, AppMessage::Input(plus));
        assert!(state.pager_typing);
    }

    #[test]
    fn cron_open_run_binding_only_consumes_text_in_history() {
        use talaria_core::cron::{CronAction, CronKey};
        let map = KeyMap::defaults();
        let key = Key::from(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(
            super::cron_key(&map, key, false, false),
            Some(CronKey::Char('o'))
        );
        assert_eq!(
            super::cron_key(&map, key, true, false),
            Some(CronKey::Action(CronAction::OpenRun))
        );
        let key = Key::from(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE));
        assert_eq!(
            super::cron_key(&map, key, false, false),
            Some(CronKey::BackTab)
        );
        let entries = BTreeMap::from([("cron_open_run".into(), vec!["x".into()])]);
        let (map, notices) = KeyMap::from_entries(&entries);
        assert!(notices.is_empty());
        let key = Key::from(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(
            super::cron_key(&map, key, false, false),
            Some(CronKey::Char('x'))
        );
        assert_eq!(
            super::cron_key(&map, key, true, false),
            Some(CronKey::Action(CronAction::OpenRun))
        );
        let entries = BTreeMap::from([("cron_reload".into(), vec!["esc".into()])]);
        let (_, notices) = KeyMap::from_entries(&entries);
        assert!(!notices.is_empty());
    }

    #[test]
    fn status_keeps_the_usage_count_until_a_session_tree_is_known() {
        let mut state = AppState::new("subagent-status");
        state.subagents_usage_active = 9;
        let inputs = status_inputs(&state.view(), None);
        assert_eq!(inputs.subagents_active, 9);
        assert_eq!(inputs.subagents_terminal, 0);
    }

    #[test]
    fn rejected_side_submissions_stay_out_of_persistent_history() {
        for (text, has_session) in [
            ("! ", true),
            ("/btw", true),
            ("/btw ", true),
            ("!echo hello", false),
            ("/btw hello", false),
        ] {
            assert!(!should_record_submission(text, has_session));
        }
        for (text, has_session) in [
            ("!echo hello", true),
            ("/btw hello", true),
            ("ordinary prompt", false),
        ] {
            assert!(should_record_submission(text, has_session));
        }
    }

    /// The bottom `rows` rows of a rendering, as plain text.
    fn bottom(text: &str, rows: usize) -> Vec<String> {
        let lines = render_markdown(text, &MarkdownOptions::default());
        let skip = lines.len().saturating_sub(rows);
        lines.iter().skip(skip).map(ToString::to_string).collect()
    }

    #[test]
    fn loaded_draft_is_reported_before_closing_the_palette() {
        for was_open in [false, true] {
            let mut reported = (String::new(), 0);
            let mut actions = Vec::new();
            TuiScreen::close_palette(&mut actions);
            sync_reported_draft("/help", 5, &mut reported, &mut actions);
            let mut state = AppState::new("draft-sync");
            state.palette.open = was_open;
            for action in actions {
                let _ = update(&mut state, AppMessage::ForegroundKey(action));
            }
            assert_eq!(state.draft, "/help");
            assert!(!state.palette.open);
            assert_eq!(reported, ("/help".into(), 5));
        }
    }

    #[test]
    fn submission_does_not_report_the_composers_cleared_draft() {
        let mut reported = ("/help".into(), 5);
        let mut actions = vec![UserAction::Submit("/help".into())];
        sync_reported_draft("", 0, &mut reported, &mut actions);
        assert_eq!(reported, (String::new(), 0));
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], UserAction::Submit(text) if text == "/help"));
    }

    #[test]
    fn preview_keeps_the_bottom_rows_of_a_long_answer() {
        let count: String = (1..=2000).map(|n| format!("{n}\n")).collect();
        let lists: String = (1..=300)
            .map(|n| format!("- item {n} with a little more text\n"))
            .collect();
        let mixed = format!("{lists}\nA closing paragraph.\n\n{count}");
        for text in [count.as_str(), lists.as_str(), mixed.as_str()] {
            let limit = 96;
            let trimmed = preview_text(text, limit).expect("a long text is cut");
            assert!(trimmed.lines().count() <= limit);
            assert_eq!(bottom(&trimmed, 48), bottom(text, 48));
        }
    }

    #[test]
    fn preview_leaves_short_fenced_and_table_text_whole() {
        assert_eq!(preview_text("one\ntwo\n", 80), None);
        let fenced = format!("```\n{}```\n", "x\n".repeat(200));
        assert_eq!(preview_text(&fenced, 80), None);
        let table = format!("| a | b |\n|---|---|\n{}", "| 1 | 2 |\n".repeat(200));
        assert_eq!(preview_text(&table, 80), None);
    }

    #[test]
    fn picker_keys_use_the_overlay_configuration_and_keep_typed_text() {
        let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "model_scope".to_string(),
            vec!["ctrl+r".to_string()],
        )]));
        assert_eq!(notices.len(), 1);
        let key = |code, mods| Key::from(KeyEvent::new(code, mods));
        assert_eq!(
            picker_key(&map, key(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            Some(PickerKey::ToggleScope)
        );
        assert_eq!(
            picker_key(&map, key(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            Some(PickerKey::Char('G'))
        );
        assert_eq!(
            picker_key(&map, key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(PickerKey::Close)
        );
        assert_eq!(
            picker_key(&map, key(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(PickerKey::ClearFilter)
        );
        assert_eq!(
            picker_key(&map, key(KeyCode::Char('p'), KeyModifiers::ALT)),
            None
        );
    }

    #[test]
    fn confirmation_requires_the_focus_guard_and_only_its_fixed_keys() {
        let now = Instant::now();
        let enter = Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(confirmation_key(enter, Some(now), now), None);
        let ready = now + Duration::from_millis(250);
        assert_eq!(
            confirmation_key(enter, Some(now), ready),
            Some(ConfirmKey::Accept)
        );
        let escape = Key::from(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            confirmation_key(escape, Some(now), ready),
            Some(ConfirmKey::Decline)
        );
        let ctrl_c = Key::from(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(confirmation_key(ctrl_c, Some(now), ready), None);
        let shifted = Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        assert_eq!(confirmation_key(shifted, Some(now), ready), None);
    }

    #[test]
    fn attachment_keys_follow_open_list_and_live_label_boundaries() {
        let map = KeyMap::defaults();
        let key = |code, mods| Key::from(KeyEvent::new(code, mods));
        let plain = |code| key(code, KeyModifiers::NONE);
        let mut view = AttachmentView::default();
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Down), &view, "@a", 2),
            None
        );
        assert_eq!(
            attachment_key(
                &map,
                &key(KeyCode::Char('v'), KeyModifiers::CONTROL),
                &view,
                "",
                0
            ),
            Some(AttachmentAction::AttachClipboardImage)
        );
        view.mention = Some(MentionView {
            rows: Vec::new(),
            selected: None,
            status: None,
            footer: "",
        });
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Down), &view, "@a", 2),
            Some(AttachmentAction::CompletionNext)
        );
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Enter), &view, "@a", 2),
            None
        );
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Esc), &view, "@a", 2),
            Some(AttachmentAction::CompletionClose)
        );
        view.mention
            .as_mut()
            .expect("open")
            .rows
            .push(("a".into(), String::new()));
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Enter), &view, "@a", 2),
            Some(AttachmentAction::Submit)
        );
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Tab), &view, "/help", 5),
            None
        );
        view.highlights.push((0..10, LabelStyle::Accent));
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Backspace), &view, "[Image #1]", 10),
            Some(AttachmentAction::DeleteBackward)
        );
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Backspace), &view, "[Image #1]", 9),
            None
        );
        assert_eq!(
            attachment_key(&map, &plain(KeyCode::Delete), &view, "[Image #1]", 0),
            Some(AttachmentAction::DeleteForward)
        );
    }

    #[test]
    fn composer_escape_preserves_rewind_eligibility() {
        use talaria_app::app::UserAction;
        for (text, running, modifiers, count) in [
            ("", false, KeyModifiers::NONE, 1),
            ("", false, KeyModifiers::ALT, 2),
            ("draft", false, KeyModifiers::NONE, 0),
            ("", true, KeyModifiers::NONE, 0),
        ] {
            let mut composer = Composer::new(KeyMap::defaults());
            composer.load(text.to_string());
            let actions = composer_key_actions(
                &mut composer,
                Key::from(KeyEvent::new(KeyCode::Esc, modifiers)),
                ComposerContext {
                    turn_running: running,
                },
                Instant::now(),
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, UserAction::Escape { eligible: true }))
                    .count(),
                count,
            );
        }
    }

    #[test]
    fn collected_raw_keys_never_report_a_false_clear() {
        let mut composer = Composer::new(KeyMap::defaults());
        let now = std::time::Instant::now();
        for character in ['a', 'b', 'c'] {
            let actions = composer_key_actions(
                &mut composer,
                Key::from(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
                ComposerContext {
                    turn_running: false,
                },
                now,
            );
            assert!(!actions.iter().any(|action| matches!(action,
                talaria_app::app::UserAction::DraftEdited { text, .. } if text.is_empty()
            )));
            if character == 'c' {
                assert!(actions.is_empty());
                assert!(composer.is_collecting_burst());
            }
        }
    }

    #[test]
    fn bracketed_paste_follows_buffered_raw_keys() {
        use talaria_app::app::UserAction;
        let mut composer = Composer::new(KeyMap::defaults());
        let now = std::time::Instant::now();
        for character in ['a', 'b', 'c'] {
            composer_key_actions(
                &mut composer,
                Key::from(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
                ComposerContext {
                    turn_running: false,
                },
                now,
            );
        }
        let actions = paste_actions(&mut composer, "P", now);
        assert_eq!(actions.len(), 2);
        assert!(matches!(&actions[0], UserAction::DraftEdited {
            text, caret: 3, typed: true, recalled: false, ..
        } if text == "abc"));
        let UserAction::Paste { text, draft, caret } = &actions[1] else {
            panic!("paste follows the canonical draft update");
        };
        assert_eq!(draft, "abc");
        assert_eq!(*caret, 3);
        composer.replace_range(*caret..*caret, text);
        assert_eq!(composer.text(), "abcP");
    }
    /// A finished shell call with ten output lines, so its compact preview is cut.
    fn ten_line_tool() -> Cell {
        Cell::Tool(ToolCell {
            id: CellId::new(1),
            tool_id: "call-1".to_string(),
            name: "terminal".to_string(),
            context: None,
            args: Some(json!({"command": "seq 1 10"})),
            labels: Vec::new(),
            status: ToolStatus::Succeeded,
            progress: None,
            summary: None,
            result: Some(
                json!({"output": "1\n2\n3\n4\n5\n6\n7\n8\n9\n10", "exit_code": 0, "error": null})
                    .to_string(),
            ),
            diff: None,
            duration_s: Some(0.1),
        })
    }

    #[test]
    fn scrollback_tool_cells_name_the_transcript_key() {
        let theme = talaria_tui::theme::resolve(&ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: false,
            polarity: (Polarity::Dark, PolaritySource::Config),
            depth: (ColorDepth::None, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            terminal: TerminalColors::default(),
        });
        let last_row = |keymap: &KeyMap| {
            let lines = tool_cells(&theme, keymap).lines(&ten_line_tool(), 100);
            lines.last().map(ToString::to_string).unwrap_or_default()
        };
        assert_eq!(
            last_row(&KeyMap::defaults()),
            "     … +7 lines (Ctrl+O to view)"
        );
        let entries = BTreeMap::from([("transcript_open".to_string(), vec!["ctrl+t".to_string()])]);
        let (rebound, _) = KeyMap::from_entries(&entries);
        assert_eq!(last_row(&rebound), "     … +7 lines (Ctrl+T to view)");
    }

    #[test]
    fn accepted_search_entry_restores_its_paste_placeholder() {
        let mut state = AppState::new("search-recall");
        let label = state
            .attachments
            .chips
            .collapse_paste(&"pasted line\n".repeat(120));
        assert_eq!(label, "[Pasted text #1 +120 lines]");
        // The submission cleared the draft, so the paste is archived.
        state.attachments.chips.reconcile("");
        let entry = format!("summarize {label}");

        // A plain report of the loaded entry leaves the placeholder as typed text.
        let _ = update(
            &mut state,
            AppMessage::ForegroundKey(UserAction::DraftEdited {
                text: entry.clone(),
                caret: entry.len(),
                typed: false,
                recalled: false,
                now: Instant::now(),
            }),
        );
        assert!(state.attachments.chips.live(&entry).is_empty());

        let mut composer = Composer::new(KeyMap::defaults());
        let mut reported = (String::new(), 0);
        let action = recall_draft(&mut composer, &mut reported, entry.clone(), Instant::now());
        assert!(matches!(&action, UserAction::DraftEdited {
            text, caret, typed: false, recalled: true, ..
        } if *text == entry && *caret == entry.len()));
        assert_eq!(reported, (entry.clone(), entry.len()));
        assert_eq!(composer.text(), entry);

        let _ = update(&mut state, AppMessage::ForegroundKey(action));
        assert_eq!(state.attachments.chips.live(&entry).len(), 1);
        let expanded = state.attachments.chips.expand(&entry).expect("expands");
        assert_eq!(expanded.text.matches("pasted line").count(), 120);
    }

    #[test]
    fn accepted_search_entry_is_reported_before_the_palette_closes() {
        for was_open in [false, true] {
            let mut composer = Composer::new(KeyMap::defaults());
            let mut reported = (String::new(), 0);
            let mut actions = Vec::new();
            TuiScreen::close_palette(&mut actions);
            actions.insert(
                0,
                recall_draft(&mut composer, &mut reported, "/help".into(), Instant::now()),
            );
            let mut state = AppState::new("search-palette");
            state.palette.open = was_open;
            for action in actions {
                let _ = update(&mut state, AppMessage::ForegroundKey(action));
            }
            assert_eq!(state.draft, "/help");
            assert!(!state.palette.open);
        }
    }

    #[test]
    fn flushed_burst_snapshot_is_the_reported_draft() {
        let typed = |composer: &mut Composer, now| {
            for character in ['@', 's', 'r'] {
                composer_key_actions(
                    composer,
                    Key::from(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
                    ComposerContext {
                        turn_running: false,
                    },
                    now,
                );
            }
            assert!(composer.is_collecting_burst());
        };
        let tab = || {
            vec![UserAction::AttachmentKey {
                action: AttachmentAction::CompletionAccept,
                text: "@sr".into(),
                caret: 3,
                now: Instant::now(),
            }]
        };

        // The composer timer flushes the burst; the loop reads the draft right after.
        let mut composer = Composer::new(KeyMap::defaults());
        let start = Instant::now();
        typed(&mut composer, start);
        let mut reported = (String::new(), 0);
        assert_eq!(reported_snapshot(&composer, &mut reported), None);
        assert_eq!(reported, (String::new(), 0));
        let due = composer.next_tick().expect("the burst waits for its timer");
        let _ = composer.tick(due);
        let snapshot = reported_snapshot(&composer, &mut reported).expect("the burst is flushed");
        assert_eq!(snapshot, ("@sr".to_string(), 3));
        assert_eq!(reported, snapshot);

        // The next Tab goes to the mention list alone.
        let mut actions = tab();
        sync_reported_draft("@sr", 3, &mut reported, &mut actions);
        assert_eq!(actions.len(), 1);
        // A stale report would put a draft edit ahead of it, which closes the list.
        let mut stale = (String::new(), 0);
        let mut actions = tab();
        sync_reported_draft("@sr", 3, &mut stale, &mut actions);
        assert_eq!(actions.len(), 2);

        // An explicit flush before an asynchronous edit records the same way.
        let mut composer = Composer::new(KeyMap::defaults());
        typed(&mut composer, start);
        let mut reported = (String::new(), 0);
        composer.flush_pending();
        let snapshot = reported_snapshot(&composer, &mut reported).expect("the burst is flushed");
        assert_eq!(snapshot, ("@sr".to_string(), 3));
        assert_eq!(reported, snapshot);
    }

    #[test]
    fn modified_search_characters_keep_app_and_pager_in_sync() {
        use talaria_app::app::{AppEffect, InputCode, InputKey, TranscriptAction};
        use talaria_tui::pager::{Pager, PagerAction, SearchIndex};
        for (ctrl, alt) in [(true, false), (false, true)] {
            for ending in [InputCode::Backspace, InputCode::Enter, InputCode::Esc] {
                let mut state = AppState::new("pager-search");
                let mut index = SearchIndex::new();
                let view = state.view();
                let mut pager = Pager::open(&super::transcript_context(&view), 80, 24, &mut index);
                let key = |code, ctrl, alt| InputKey {
                    code,
                    ctrl,
                    alt,
                    shift: false,
                    super_key: false,
                    other: false,
                };
                update(
                    &mut state,
                    AppMessage::Input(key(InputCode::Char('o'), true, false)),
                );
                update(
                    &mut state,
                    AppMessage::Input(key(InputCode::Char('/'), false, false)),
                );
                let view = state.view();
                pager.handle_action(
                    PagerAction::StartSearch,
                    &super::transcript_context(&view),
                    &mut index,
                );
                for input in [
                    key(InputCode::Char('a'), ctrl, alt),
                    key(ending, false, false),
                    key(InputCode::Char('q'), false, false),
                ] {
                    let effects = update(&mut state, AppMessage::Input(input));
                    for effect in effects {
                        if let AppEffect::Transcript(TranscriptAction::Key(key)) = effect
                            && pager.is_typing_search()
                            && let Some(edit) = super::pager_search_input(key)
                        {
                            let view = state.view();
                            pager.handle_search_input(
                                edit,
                                &super::transcript_context(&view),
                                &mut index,
                            );
                        }
                    }
                    assert_eq!(state.pager_typing, pager.is_typing_search());
                }
                assert!(!state.pager_open);
            }
        }
    }

    #[test]
    fn fleet_answer_paste_reaches_only_the_selected_prompt_stack() {
        use talaria_core::session::{PromptCell, PromptKey, PromptKind, PromptState, SessionId};
        use talaria_tui::prompts::{KeyOutcome, PromptStack};
        let make = |session: &str| PromptCell {
            id: CellId::new(1),
            key: PromptKey {
                session_id: SessionId::new(session),
                prompt_id: "question".into(),
                execution_generation: 1,
            },
            kind: PromptKind::Clarify,
            command: None,
            description: None,
            question: Some("Answer?".into()),
            choices: vec![],
            multi_select: false,
            edit: None,
            timeout_s: None,
            state: PromptState::Open,
            sealed: false,
        };
        for foreground in [true, false] {
            let now = Instant::now();
            let main = make("main");
            let background = make("background");
            let mut prompts = PromptStack::new();
            let mut peek = PromptStack::new();
            prompts.sync(Some("main"), &[&main], &[], true, now);
            peek.sync(Some("background"), &[&background], &[], true, now);
            let ready = now + Duration::from_millis(300);
            let actions = super::fleet_paste(
                &super::FleetFocus::Answer,
                foreground,
                &mut prompts,
                &mut peek,
                "pasted answer".into(),
                ready,
            );
            assert!(
                actions
                    .iter()
                    .any(|action| matches!(action, UserAction::FleetAnswerEdited))
            );
            let (selected, other) = if foreground {
                (&mut prompts, &mut peek)
            } else {
                (&mut peek, &mut prompts)
            };
            let enter = Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            let actions = super::fleet_card_actions(selected.handle_key(enter, ready));
            assert!(actions.iter().any(|action| matches!(action,
                UserAction::FleetAnswer(super::CardAnswer::Clarify { key, answer })
                if answer == "pasted answer" && key.session_id.as_str() == if foreground { "main" } else { "background" }
            )));
            assert!(matches!(
                other.handle_key(enter, ready),
                KeyOutcome::Consumed { intent: None, .. }
            ));
        }
    }

    #[test]
    fn fleet_peek_identity_includes_profile() {
        let key = talaria_core::directory::rows::RowKey {
            profile: "other".into(),
            session_id: "same".into(),
        };
        assert!(!super::fleet_peek_is_foreground(&key, "main", Some("same")));
        assert!(super::fleet_peek_is_foreground(&key, "other", Some("same")));
    }
}
