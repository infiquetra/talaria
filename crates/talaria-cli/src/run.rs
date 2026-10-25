//! Build the services for one launch and run until the process should exit.
//!
//! Order: the launch folder, the `--record` path and `~/.talaria/config.toml` are checked first
//! (local reads only), then the terminal; the `--resume` list, when asked for, connects and asks
//! on the normal screen; then the terminal is set up and `talaria_app::app::run` draws the first
//! frame before anything touches the network.

use std::io::{self, IsTerminal, Stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ratatui::text::Line;
use talaria_app::app::{
    self, ComposerDeadline, LinkView, Outcome, RunSetup, Screen, StatusHost, UserAction, View,
};
use talaria_app::config::{self, LoadedConfig};
use talaria_app::connection::OpenConnection;
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::launch::{Launch, StartMode, new_launch_id};
use talaria_app::prompts::{MaskedKind, MaskedRequestInfo};
use talaria_core::commands::PaletteInput;
use talaria_core::session::{
    AssistantCell, Cell, PromptCell, ReasoningCell, TurnState as SessionTurn,
};
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome};
use talaria_tui::history::CellLines;
use talaria_tui::keys::{CompletionAction, Key, KeyKind, KeyMap};
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::markdown::MarkdownTheme;
use talaria_tui::markdown::cells::MarkdownCellLines;
use talaria_tui::markdown::section::MarkdownTailSection;
use talaria_tui::palette::PaletteSection;
use talaria_tui::prompts::{KeyOutcome, MaskedView, PromptIntent, PromptStack};
use talaria_tui::status::{
    CommandLimits, CommandResult, CommandSpec, Connection, ExternalDisplay, SessionRef,
    StatusCommandService, StatusDocument, StatusInputs, StatusPalette, StatusSection, TurnState,
    interval_from_config, parse_command,
};
use talaria_tui::terminal::{
    InlineTerminal, InputThread, TerminalError, TerminalGuard, probe_startup, spawn_input,
    terminal_size,
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

/// One raw input for the screen.
pub enum ScreenInput {
    Terminal(crossterm::event::Event),
    Status(CommandResult),
}

/// The terminal side of the loop: the inline terminal, the composer, the prompt cards and the
/// status line.
pub struct TuiScreen {
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
    sender: mpsc::UnboundedSender<ScreenInput>,
    connecting: bool,
    /// The key map, for the keys of an open completion list.
    keys: KeyMap,
    /// The draft and caret last reported to the loop.
    reported: (String, usize),
    /// Terminal rows, for the palette's height cap.
    rows: u16,
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

fn status_inputs(view: &View<'_>, home_dir: Option<String>) -> StatusInputs {
    let open = view.state.map_or(0, |state| state.open_prompts().count());
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
        working_dir: view.info.and_then(|info| info.cwd.clone()),
        home_dir,
        branch: view.info.and_then(|info| info.branch.clone()),
        needs_you: open,
        working: u32::from(turn != TurnState::Idle),
        session: view.session_id.map(|id| SessionRef {
            id: id.to_string(),
            title: view.title.map(str::to_string),
        }),
        turn,
        pending_prompts: open,
        ..StatusInputs::default()
    }
}

impl TuiScreen {
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
        if actions
            .iter()
            .any(|action| matches!(action, UserAction::Submit(_)))
        {
            self.reported = (String::new(), 0);
            return;
        }
        let now = (self.composer.text(), self.composer.caret());
        if (self.reported.0.as_str(), self.reported.1) == now {
            return;
        }
        self.reported = (now.0.to_owned(), now.1);
        actions.push(UserAction::Palette(PaletteInput::DraftChanged {
            text: self.reported.0.clone(),
            caret: self.reported.1,
        }));
    }

    fn composer_action(outcome: ComposerOutcome) -> Vec<UserAction> {
        match outcome {
            ComposerOutcome::Submit(text) => vec![UserAction::Submit(text)],
            ComposerOutcome::Interrupt => vec![UserAction::Interrupt],
            ComposerOutcome::ExitRequested => vec![UserAction::Quit],
            ComposerOutcome::Unchanged | ComposerOutcome::Changed => Vec::new(),
        }
    }

    fn terminal_input(
        &mut self,
        event: crossterm::event::Event,
        view: &View<'_>,
        now: Instant,
    ) -> Vec<UserAction> {
        use crossterm::event::Event;
        match event {
            Event::Key(key) => {
                let key = Key::from(key);
                if let Some(actions) = Self::prompt_actions(self.prompts.handle_key(key, now)) {
                    return actions;
                }
                // An open completion list takes its four keys before the composer does.
                if view.palette.open
                    && key.kind != KeyKind::Release
                    && let Some(action) = self.keys.completion(&key)
                {
                    let input = match action {
                        CompletionAction::Next => PaletteInput::Next,
                        CompletionAction::Prev => PaletteInput::Prev,
                        CompletionAction::Accept => PaletteInput::Accept,
                        CompletionAction::Close => PaletteInput::Close,
                    };
                    return vec![UserAction::Palette(input)];
                }
                let ctx = ComposerContext {
                    turn_running: view.turn_running(),
                };
                let mut actions = Self::composer_action(self.composer.handle_key(key, ctx, now));
                self.report_draft(&mut actions);
                actions
            }
            Event::Paste(text) => {
                if let Some(actions) = Self::prompt_actions(self.prompts.handle_paste(&text, now)) {
                    return actions;
                }
                let mut actions = Self::composer_action(self.composer.handle_paste(&text));
                self.report_draft(&mut actions);
                actions
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

impl ComposerDeadline for TuiScreen {
    fn tick(&mut self, now: Instant) {
        let _ = self.composer.tick(now);
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

    fn input(&mut self, input: ScreenInput, view: &View<'_>, now: Instant) -> Vec<UserAction> {
        self.sync_prompts(view, now);
        let actions = match input {
            ScreenInput::Terminal(event) => self.terminal_input(event, view, now),
            ScreenInput::Status(result) => {
                self.external.apply(&result);
                Vec::new()
            }
        };
        self.draw(view);
        actions
    }

    fn draw(&mut self, view: &View<'_>) {
        let now = Instant::now();
        self.connecting = view.state.is_none() && view.link == LinkView::Connecting;
        self.inputs = status_inputs(view, self.inputs.home_dir.clone());
        self.sync_prompts(view, now);
        self.clock.observe(
            view.state.map(|state| state.cells()).unwrap_or_default(),
            now,
        );
        self.cells.set_clock(&self.clock, now);
        let width = self.terminal.viewport().width;
        let mut tail: Vec<Line<'static>> = view
            .state
            .map(|state| {
                let limit = usize::from(self.terminal.viewport().height)
                    .saturating_mul(2)
                    .max(PREVIEW_MIN_LINES);
                state
                    .live_tail()
                    .iter()
                    .flat_map(|cell| match preview(cell, limit) {
                        Some(trimmed) => self.cells.lines(&trimmed, width),
                        None => self.cells.lines(cell, width),
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
        let composer = self.composer.section(width, COMPOSER_ROWS);
        let mut sections: Vec<&dyn LiveSection> = vec![&tail, &drafting];
        if let Some(prompts) = &prompts {
            sections.push(prompts);
        }
        if self.connecting {
            sections.push(&connecting);
        }
        sections.push(&composer);
        sections.push(&palette);
        sections.push(&status);
        let _ = self.terminal.draw_live(&LiveView { sections });
    }

    fn commit_lines(&mut self, lines: Vec<String>) {
        let _ = self
            .terminal
            .commit_lines(lines.into_iter().map(Line::from).collect());
    }

    fn commit_cells(&mut self, cells: &[Cell]) {
        let _ = self.terminal.commit(cells, &self.cells);
    }

    fn load_draft(&mut self, text: &str) {
        self.composer.load(text.to_string());
        self.reported = (self.composer.text().to_owned(), self.composer.caret());
    }

    fn restore(&mut self) {
        let _ = self.terminal.restore();
    }
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
    let outcome = match session(&runtime, launch, setup, keymap, status) {
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

/// Acquire the terminal, run the loop, restore. The guard drops before the caller prints.
fn session(
    runtime: &tokio::runtime::Runtime,
    launch: Launch,
    setup: RunSetup,
    keymap: KeyMap,
    status: Option<StatusCommandService>,
) -> Result<Outcome, TerminalError> {
    let guard = TerminalGuard::acquire()?;
    let probe = probe_startup(PROBE_DEADLINE)?;
    let size = terminal_size().map_err(TerminalError::Io)?;
    let terminal = InlineTerminal::new(io::stdout(), size, LIVE_HEIGHT, probe, guard.link())
        .map_err(TerminalError::Io)?;
    let (sender, receiver) = mpsc::unbounded_channel();
    let keys = keymap.clone();
    let mut composer = Composer::new(keymap);
    composer.set_keyboard_enhancement(probe.caps.keyboard_enhancement);
    let mut screen = TuiScreen {
        terminal,
        composer,
        prompts: PromptStack::new(),
        status,
        external: ExternalDisplay::default(),
        inputs: StatusInputs {
            home_dir: std::env::var("HOME").ok(),
            ..StatusInputs::default()
        },
        palette: StatusPalette::default(),
        cells: ToolCellLines::new(
            Box::new(MarkdownCellLines::new(true, MarkdownTheme::default())),
            ToolRenderOptions {
                width: 0,
                theme: ToolTheme::default(),
                color: std::env::var_os("NO_COLOR").is_none(),
                detail: Detail::Compact,
                expand_hint: None,
                elapsed: None,
            },
        ),
        clock: ToolClock::new(),
        tool_theme: ToolTheme::default(),
        sender: sender.clone(),
        connecting: true,
        keys,
        reported: (String::new(), 0),
        rows: size.1,
    };
    let input: Option<InputThread> =
        spawn_input(move |event| sender.send(ScreenInput::Terminal(event)).is_ok());
    let outcome = runtime.block_on(app::run(launch, setup, &mut screen, receiver));
    screen.restore();
    drop(input);
    drop(screen);
    drop(guard);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::preview_text;
    use talaria_tui::markdown::{MarkdownOptions, render_markdown};

    /// The bottom `rows` rows of a rendering, as plain text.
    fn bottom(text: &str, rows: usize) -> Vec<String> {
        let lines = render_markdown(text, &MarkdownOptions::default());
        let skip = lines.len().saturating_sub(rows);
        lines.iter().skip(skip).map(ToString::to_string).collect()
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
}
