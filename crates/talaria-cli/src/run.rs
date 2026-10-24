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
use talaria_core::session::{Cell, PromptCell, TurnState as SessionTurn};
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome};
use talaria_tui::history::CellLines;
use talaria_tui::keys::{Key, KeyMap};
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::markdown::MarkdownTheme;
use talaria_tui::markdown::cells::MarkdownCellLines;
use talaria_tui::markdown::section::MarkdownTailSection;
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
use tokio::sync::mpsc;

/// How long the start-up probe waits for the terminal's answers.
const PROBE_DEADLINE: Duration = Duration::from_millis(200);
/// The live region's first height, before the first draw sets it.
const LIVE_HEIGHT: u16 = 4;
/// The composer's tallest box, in rows.
const COMPOSER_ROWS: u16 = 10;
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
    cells: MarkdownCellLines,
    sender: mpsc::UnboundedSender<ScreenInput>,
    connecting: bool,
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
                let ctx = ComposerContext {
                    turn_running: view.turn_running(),
                };
                Self::composer_action(self.composer.handle_key(key, ctx, now))
            }
            Event::Paste(text) => {
                if let Some(actions) = Self::prompt_actions(self.prompts.handle_paste(&text, now)) {
                    return actions;
                }
                Self::composer_action(self.composer.handle_paste(&text))
            }
            Event::Resize(cols, rows) => {
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

    fn next_wake(&self, now: Instant) -> Option<Instant> {
        self.status
            .as_ref()
            .and_then(|service| service.next_wake(now))
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
        let width = self.terminal.viewport().width;
        let tail: Vec<Line<'static>> = view
            .state
            .map(|state| {
                state
                    .live_tail()
                    .iter()
                    .flat_map(|cell| self.cells.lines(cell, width))
                    .collect()
            })
            .unwrap_or_default();
        let tail = MarkdownTailSection::new(tail);
        let connecting = PlainTextSection::new(CONNECTING);
        let status = StatusSection::new(&self.inputs, self.external.segment(), &self.palette);
        let prompts = self.prompts.section();
        let composer = self.composer.section(width, COMPOSER_ROWS);
        let mut sections: Vec<&dyn LiveSection> = vec![&tail];
        if let Some(prompts) = &prompts {
            sections.push(prompts);
        }
        if self.connecting {
            sections.push(&connecting);
        }
        sections.push(&composer);
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
        cells: MarkdownCellLines::new(true, MarkdownTheme::default()),
        sender: sender.clone(),
        connecting: true,
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
