//! Build the services for one launch and run until the process should exit.
//!
//! Order: the launch folder, the `--record` path and `~/.talaria/config.toml` are checked first
//! (local reads only), then the terminal; the `--resume` list, when asked for, connects and asks
//! on the normal screen; then the terminal is set up and `talaria_app::app::run` draws the first
//! frame before anything touches the network.

use std::collections::HashMap;
use std::io::{self, IsTerminal, Stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

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
use talaria_app::comforts::{editor, herdr::HerdrReporter, prompt_history::PromptHistory, suspend};
use talaria_app::config::{self, LoadedConfig};
use talaria_app::connection::OpenConnection;
use talaria_app::context::ContextBlock;
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::launch::{Launch, StartMode, new_launch_id};
use talaria_app::prompts::{MaskedKind, MaskedRequestInfo};
use talaria_core::appearance::ThemeReport;
use talaria_core::attachments::CollapseRule;
use talaria_core::attention::{Alert, AttentionConfig, PaneReport, PaneState};
use talaria_core::checklist::ChecklistCommit;
use talaria_core::commands::PaletteInput;
use talaria_core::models::{ConfirmKey, PickerKey, PickerRow, PickerView};
use talaria_core::mutation::Confirmation;
use talaria_core::permissions::{ApprovalSegmentText, ApprovalTone};
use talaria_core::queue::{AdmissionId, QueueIntent, QueueView};
use talaria_core::session::{
    AssistantCell, Cell, CellId, PromptCell, ReasoningCell, TurnState as SessionTurn,
};
use talaria_tui::automation::{AutomationSection, AutomationTheme};
use talaria_tui::checklist::{
    ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS, SHORT_TERMINAL_ROWS, committed_block_lines,
};
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome, clean_text};
use talaria_tui::context_view::{
    CompactingSection, ContextStyle, compact_block, context_block, error_block, notice_block,
    prebuild_block, usage_block,
};
use talaria_tui::history::{CellLines, PlainCellLines};
use talaria_tui::history_search::{HistorySearch, SearchOutcome};
use talaria_tui::keys::{
    Action, CompletionAction, Key, KeyCode, KeyContext, KeyKind, KeyMap, ModelAction,
};
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
use talaria_tui::prompts::{KeyOutcome, MaskedView, PromptIntent, PromptStack};
use talaria_tui::queue::{composer_title, handle_selection_key, render_preview};
use talaria_tui::rewind::{RewindTheme, draw_rewind, key_to_rewind, visible_rows};
use talaria_tui::side::{SideTasksSection, SideTheme, shell_mode_title};
use talaria_tui::status::{
    ApprovalsSegment, CommandLimits, CommandResult, CommandSpec, Connection, ExternalDisplay,
    SessionRef, StatusCommandService, StatusDocument, StatusInputs, StatusPalette, StatusSection,
    Tone, TurnState, interval_from_config, parse_command,
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
    Terminal(crossterm::event::Event),
    Status(CommandResult),
    Edited(editor::EditResult),
    Resumed(suspend::SuspendResult),
    Continued,
    ComfortNotice(String),
}

/// The terminal side of the loop: the inline terminal, the composer, the prompt cards and the
/// status line.
pub struct TuiScreen {
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
    picker_open: bool,
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
fn tool_cells(theme: &Theme) -> ToolCellLines {
    ToolCellLines::new(
        Box::new(MarkdownCellLines::new(true, MarkdownTheme::default())),
        ToolRenderOptions {
            width: 0,
            theme: theme.tool.clone(),
            color: theme.color,
            detail: Detail::Compact,
            expand_hint: None,
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
        let theme = themed_cell_id(cell)
            .and_then(|id| self.captured.get(&id))
            .map_or(self.current, |theme| theme.as_ref());
        ThemedCells { theme }.lines(cell, width)
    }
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
    let eligible =
        outcome == ComposerOutcome::Unchanged && composer.is_empty() && !ctx.turn_running;
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
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    if event.kind != KeyEventKind::Press {
        return None;
    }
    let code = match event.code {
        KeyCode::Char(character) => InputCode::Char(character),
        KeyCode::Enter => InputCode::Enter,
        KeyCode::Esc => InputCode::Esc,
        KeyCode::Backspace => InputCode::Backspace,
        KeyCode::Up => InputCode::Up,
        KeyCode::Down => InputCode::Down,
        KeyCode::PageUp => InputCode::PageUp,
        KeyCode::PageDown => InputCode::PageDown,
        KeyCode::Home => InputCode::Home,
        KeyCode::End => InputCode::End,
        _ => return None,
    };
    Some(InputKey {
        code,
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
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
    StatusInputs {
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
        needs_you: open,
        working: u32::from(turn != TurnState::Idle),
        session: view.session_id.map(|id| SessionRef {
            id: id.to_string(),
            title: view.title.map(str::to_string),
        }),
        turn,
        pending_prompts: open,
        context_percent,
        approvals: view.approvals.as_ref().map(approvals_segment),
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
    fn confirmation_takes_keys(&self) -> bool {
        !self.picker_open
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

    fn draw_picker(&mut self) {
        if let Some(picker) = &self.picker {
            let view = picker.view();
            let _ = self
                .terminal
                .draw_overlay(|area, buf| draw_model_picker(&view, area, buf));
        }
    }

    /// Take a newly resolved theme for the composer, the status line and later cells.
    fn set_theme(&mut self, theme: Theme) {
        self.color = theme.color;
        self.checklist_style = if theme.color {
            ChecklistStyle::default()
        } else {
            ChecklistStyle::plain()
        };
        self.composer.set_style(theme.composer);
        self.palette = theme.status;
        self.cells = tool_cells(&theme);
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
                    && let Some(notice) = self.history.append(&text)
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
        let draft = self.composer.text().to_string();
        let intent = match bound {
            Some(Action::ComposerSendNow) if !draft.trim().is_empty() => {
                QueueIntent::SendDraftNow(draft)
            }
            Some(Action::ComposerSteer) if !draft.trim().is_empty() => {
                QueueIntent::SteerText(draft)
            }
            _ => return None,
        };
        self.composer.clear();
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
        match event {
            Event::Key(raw) => {
                if self.terminal.is_released() {
                    return Vec::new();
                }
                let input = input_key(&raw);
                if view.pager_open {
                    return input
                        .map(|key| vec![UserAction::Input(key)])
                        .unwrap_or_default();
                }
                let key = Key::from(raw);
                let action = self.keymap.resolve(&key, KeyContext::Composer);
                let focus = if self.search.is_some() {
                    talaria_tui::comfort_route::Focus::Search
                } else if self.composer.is_empty() && self.prompts.section().is_some() {
                    talaria_tui::comfort_route::Focus::Card
                } else {
                    talaria_tui::comfort_route::Focus::Composer
                };
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
                if self.picker_open {
                    return picker_key(&self.keymap, key)
                        .map(UserAction::ModelPicker)
                        .into_iter()
                        .collect();
                }
                if let Some(search) = &mut self.search {
                    match search.handle_key(key, std::time::SystemTime::now()) {
                        SearchOutcome::Accept(text) => {
                            self.composer.load(text);
                            self.search = None;
                        }
                        SearchOutcome::Cancel => self.search = None,
                        _ => {}
                    }
                    let mut actions = Vec::new();
                    Self::close_palette(&mut actions);
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
            Event::Paste(text) => {
                if self.terminal.is_released() {
                    return Vec::new();
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
                            })
                        })
                        .collect();
                }
                if view.pager_open || view.rewind.is_some() || self.picker_open {
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
                Vec::new()
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
        InputCode::Up => KeyCode::Up,
        InputCode::Down => KeyCode::Down,
        InputCode::PageUp => KeyCode::PageUp,
        InputCode::PageDown => KeyCode::PageDown,
        InputCode::Home => KeyCode::Home,
        InputCode::End => KeyCode::End,
    };
    Key {
        code,
        mods: talaria_tui::keys::Mods {
            ctrl: key.ctrl,
            alt: key.alt,
            shift: key.shift,
            super_key: false,
            other: false,
        },
        kind: KeyKind::Press,
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
                    let action =
                        if key.code == InputCode::Esc && !key.ctrl && !key.alt && !key.shift {
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
                        let input = match key.code {
                            InputCode::Enter => Some(SearchInput::Enter),
                            InputCode::Esc => Some(SearchInput::Escape),
                            InputCode::Backspace => Some(SearchInput::Backspace),
                            InputCode::Char(character) => Some(SearchInput::Char(character)),
                            _ => None,
                        };
                        if let Some(input) = input {
                            transcript.pager.handle_search_input(
                                input,
                                &ctx,
                                &mut transcript.index,
                            );
                        }
                    } else if let Some(action) = action {
                        transcript
                            .pager
                            .handle_action(action, &ctx, &mut transcript.index);
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
        let renderer = ToolCellLines::new(
            Box::new(PagerCellLines {
                details,
                markdown: MarkdownOptions {
                    width: width.saturating_sub(2).max(8),
                    color: self.color,
                    ..MarkdownOptions::default()
                },
                theme: PagerTheme::default(),
                other: &PLAIN_CELLS,
            }),
            ToolRenderOptions {
                width,
                theme: self.tool_theme.clone(),
                color: self.color,
                detail: match details {
                    Details::Expanded => Detail::Full,
                    Details::Compact => Detail::Compact,
                },
                expand_hint: None,
                elapsed: None,
            },
        );
        let _ = self
            .terminal
            .draw_overlay(|area, buf| transcript.pager.draw(area, buf, &ctx, &renderer));
    }
}

impl ComposerDeadline for TuiScreen {
    fn tick(&mut self, now: Instant) {
        let _ = self.composer.tick(now);
        self.sync_title();
    }

    fn next_tick(&self) -> Option<Instant> {
        self.composer.next_tick()
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
    type Input = ScreenInput;

    fn model_list_height(&self) -> usize {
        terminal_size().map_or(10, |(_, rows)| usize::from(rows.saturating_sub(3)).max(3))
    }

    fn sync_models(&mut self, picker: Option<PickerView<'_>>, confirmation: Option<&Confirmation>) {
        match picker {
            Some(view) => {
                if !self.picker_open {
                    let _ = self.terminal.open_overlay();
                    self.picker_open = true;
                }
                self.picker = Some(ModelOverlay::from_view(view));
                self.draw_picker();
            }
            None => {
                self.picker = None;
                if self.picker_open {
                    let _ = self.terminal.close_overlay();
                    self.picker_open = false;
                }
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

    fn input(&mut self, input: ScreenInput, view: &View<'_>, now: Instant) -> Vec<UserAction> {
        self.sync_prompts(view, now);
        self.sync_confirmation_focus(now);
        let mut actions = match input {
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
        self.apply_transcript(view);
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
        if self.picker_open && self.transcript.is_none() {
            self.draw_picker();
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
            sections.push(search);
        } else {
            sections.push(&queue);
            if let Some(mention) = &mention {
                sections.push(mention);
            }
            if let Some(checklist) = &checklist {
                sections.push(checklist);
            }
            sections.push(&composer);
            if !composer_is_empty || prompts.is_none() {
                sections.push(&palette);
            }
        }
        if let Some(side) = &side {
            sections.push(side);
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
        self.composer.forget_submitted(text);
    }

    fn offer_draft(&mut self, text: &str) {
        if self.composer.is_empty() && self.editing.is_none() {
            self.composer.load(text.to_string());
        }
    }

    fn load_draft(&mut self, text: &str) {
        self.composer.load(text.to_string());
        self.reported = (self.composer.text().to_owned(), self.composer.caret());
        self.sync_title();
    }

    fn edit_draft(&mut self, range: std::ops::Range<usize>, text: &str) -> Option<(String, usize)> {
        self.composer.replace_range(range, text);
        self.draft_snapshot()
    }

    fn draft_snapshot(&self) -> Option<(String, usize)> {
        (!self.composer.is_collecting_burst())
            .then(|| (self.composer.text().to_string(), self.composer.caret()))
    }

    fn flush_draft(&mut self) -> Option<(String, usize)> {
        self.composer.flush_pending();
        self.draft_snapshot()
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
) -> Result<Option<(OpenConnection, String, Option<String>)>, LaunchFailure> {
    let (connection, rows) = runtime.block_on(async {
        let connection = app::connect(launch, home).await?;
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

pub fn run(mut launch: Launch) -> ExitCode {
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
        home: home.clone(),
        launch_id: launch_id.clone(),
        preopened: None,
        list_title: None,
        notices,
        keys: loaded.config.keys.clone(),
    };
    if launch.mode == StartMode::Resume(None) {
        match pick_session(&runtime, &launch, &home, &launch_id) {
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
    let mut composer = Composer::with_history(keymap.clone(), history.composer_seed());
    composer.set_keyboard_enhancement(probe.caps.keyboard_enhancement);
    composer.set_style(theme.composer);
    let mut screen = TuiScreen {
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
        status,
        external: ExternalDisplay::default(),
        inputs: StatusInputs {
            home_dir: std::env::var("HOME").ok(),
            ..StatusInputs::default()
        },
        palette: theme.status,
        cells: tool_cells(&theme),
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
        picker_open: false,
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
    let outcome = runtime.block_on(app::run(launch, setup, &mut screen, receiver));
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
        TuiScreen, attachment_key, composer_key_actions, confirmation_key, paste_actions,
        picker_key, preview_text, should_record_submission, sync_reported_draft,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};
    use talaria_app::app::AttachmentView;
    use talaria_app::app::{AppMessage, AppState, UserAction, update};
    use talaria_app::attachments::{Action as AttachmentAction, LabelStyle};
    use talaria_core::attachments::MentionView;
    use talaria_core::models::{ConfirmKey, PickerKey};
    use talaria_tui::composer::{Composer, ComposerContext};
    use talaria_tui::keys::{Key, KeyMap};
    use talaria_tui::markdown::{MarkdownOptions, render_markdown};

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
}
