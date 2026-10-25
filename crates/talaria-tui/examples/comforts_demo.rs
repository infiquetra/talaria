//! Comforts demo: `--config <file>`, `--history-file <file>`, `--folder <key>`,
//! `--alert-after-submit <needs_you|done|failed>`.
//!
//! Exercises the real composer, history, editor, signals and terminal without a gateway.
//! Behavior contract: issue 228.

use crossterm::event::Event;
use ratatui::text::Line;
use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime};
use talaria_app::{
    comforts::{editor, prompt_history::PromptHistory, suspend},
    config,
};
use talaria_tui::{
    composer::{Composer, ComposerContext, ComposerOutcome},
    history_search::{HistorySearch, SearchOutcome},
    keys::{Action, Key, KeyMap},
    live::{LiveSection, LiveView},
    terminal::{
        InlineTerminal, TerminalGuard, alert_bytes, probe_reacquire, probe_startup, spawn_input,
        terminal_size,
    },
};

fn commit(term: &mut InlineTerminal<io::Stdout>, text: String) -> io::Result<()> {
    term.commit_lines(vec![Line::from(text)])
}

enum DemoInput {
    Terminal(Event),
    Continued,
}

fn run() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut config = config::Config::default();
    let mut file = None;
    let mut folder = "demo".to_string();
    let mut alert = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let path = args
                    .next()
                    .ok_or_else(|| io::Error::other("missing configuration file"))?;
                config = config::load_file(std::path::Path::new(&path)).config;
            }
            "--history-file" => file = args.next().map(std::path::PathBuf::from),
            "--folder" => folder = args.next().unwrap_or_default(),
            "--alert-after-submit" => alert = args.next(),
            _ => return Err(io::Error::other("unknown demo option")),
        }
    }
    let scratch = tempfile::tempdir()?;
    if alert
        .as_deref()
        .is_some_and(|kind| !matches!(kind, "needs_you" | "done" | "failed"))
    {
        return Err(io::Error::other("unknown alert kind"));
    }
    let file = file.unwrap_or_else(|| scratch.path().join("history.jsonl"));
    let (mut history, notice) = PromptHistory::open_file(&file, config.history.save, &folder);
    let (map, notices) = KeyMap::from_entries(&config.keys);
    let mut composer = Composer::with_history(map.clone(), history.composer_seed());
    let guard = TerminalGuard::acquire().map_err(|_| io::Error::other("terminal unavailable"))?;
    let probe = probe_startup(Duration::from_millis(200))
        .map_err(|_| io::Error::other("terminal probe failed"))?;
    composer.set_keyboard_enhancement(probe.caps.keyboard_enhancement);
    let mut term = InlineTerminal::new(io::stdout(), terminal_size()?, 3, probe, guard.link())?;
    for notice in notices.into_iter().chain(notice) {
        commit(&mut term, format!("NOTICE: {notice}"))?;
    }
    let (tx, rx) = mpsc::channel();
    let keys = tx.clone();
    let input = spawn_input(move |event| keys.send(DemoInput::Terminal(event)).is_ok());
    let continued = Arc::new(AtomicBool::new(false));
    let expected = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    suspend::spawn_sigcont_watch(
        expected.clone(),
        released.clone(),
        continued.clone(),
        move || {
            let _ = tx.send(DemoInput::Continued);
        },
    )?;
    suspend::catch_interrupts_while_released()?;
    let mut search: Option<HistorySearch> = None;
    let mut due = None;
    loop {
        {
            let width = term.viewport().width;
            let section = composer.section(width, 10);
            let search_section = search.as_ref().map(|s| s.section(SystemTime::now()));
            let live: &dyn LiveSection = search_section
                .as_ref()
                .map_or(&section as &dyn LiveSection, |s| s as &dyn LiveSection);
            term.draw_live(&LiveView {
                sections: vec![live],
            })?;
        }
        let deadline = match (composer.next_tick(), due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let event = match deadline {
            Some(at) => rx
                .recv_timeout(at.saturating_duration_since(Instant::now()))
                .ok(),
            None => rx.recv().ok(),
        };
        let now = Instant::now();
        if matches!(event, Some(DemoInput::Continued)) {
            guard.enter_raw()?;
            term.reassert_modes()?;
            continue;
        }
        let event = match event {
            Some(DemoInput::Terminal(event)) => Some(event),
            _ => None,
        };
        if due.is_some_and(|at| now >= at) {
            let (kind, text) = match alert.as_deref() {
                Some("needs_you") => (
                    talaria_core::attention::AlertKind::NeedsYou,
                    "needs you: approval: demo",
                ),
                Some("failed") => (talaria_core::attention::AlertKind::Failed, "failed: demo"),
                _ => (talaria_core::attention::AlertKind::Done, "done: demo"),
            };
            term.alert(
                alert_bytes(
                    text,
                    config.notifications.bell.contains(&kind),
                    config.notifications.desktop.contains(&kind),
                    std::env::var_os("TMUX").is_some_and(|value| !value.is_empty()),
                ),
                talaria_core::attention::urgency(kind),
            )?;
            due = None;
        }
        let outcome = match event {
            Some(Event::Key(event)) => {
                let key = Key::from(event);
                let action = map.resolve(&key);
                if matches!(action, Some(Action::OpenEditor | Action::HistorySearch)) {
                    let _ = composer.handle_key(
                        key,
                        ComposerContext {
                            turn_running: false,
                        },
                        now,
                    );
                }
                if action == Some(Action::Suspend) {
                    if !suspend::job_control_available() {
                        commit(&mut term,"NOTICE: Ctrl+Z cannot suspend here: Talaria was not started from a shell with job control. Use /quit; the session keeps running in Hermes.".into())?;
                        continue;
                    }
                    continued.store(false, Ordering::SeqCst);
                    expected.store(true, Ordering::SeqCst);
                    if let Some(input) = &input {
                        input.pause()?;
                    }
                    term.release()?;
                    released.store(true, Ordering::SeqCst);
                    guard.leave_raw()?;
                    let result = suspend::suspend_process(&continued);
                    guard.enter_raw()?;
                    term.reacquire(&mut || probe_reacquire(Duration::from_millis(200)))?;
                    released.store(false, Ordering::SeqCst);
                    expected.store(false, Ordering::SeqCst);
                    if let Some(input) = &input {
                        input.resume();
                    }
                    commit(
                        &mut term,
                        if result == suspend::SuspendResult::Resumed {
                            "RESUMED".into()
                        } else {
                            "NOTICE: Suspend did not take effect: the stop signal is ignored in this terminal.".into()
                        },
                    )?;
                    continue;
                }
                if let Some(boxed) = &mut search {
                    match boxed.handle_key(key, SystemTime::now()) {
                        SearchOutcome::Accept(text) => {
                            composer.load(text.clone());
                            commit(&mut term, format!("LOADED text={text:?}"))?;
                            search = None;
                        }
                        SearchOutcome::Cancel => search = None,
                        _ => {}
                    }
                    continue;
                }
                if action == Some(Action::HistorySearch) {
                    search = Some(HistorySearch::open(
                        history.snapshot(),
                        composer.text().into(),
                        map.chords(Action::HistorySearch).to_vec(),
                    ));
                    continue;
                }
                if action == Some(Action::OpenEditor) {
                    let argv = match editor::resolve_editor(
                        Some(&config.editor.command),
                        &|key| std::env::var(key).ok(),
                        &|name| {
                            std::env::var_os("PATH").is_some_and(|paths| {
                                std::env::split_paths(&paths).any(|dir| dir.join(name).is_file())
                            })
                        },
                    ) {
                        Ok(argv) => argv,
                        Err(editor::EditorError::Parse(reason)) => {
                            commit(
                                &mut term,
                                format!("NOTICE: {}", editor::parse_notice(&reason)),
                            )?;
                            continue;
                        }
                    };
                    if let Some(input) = &input {
                        input.pause()?;
                    }
                    term.release()?;
                    released.store(true, Ordering::SeqCst);
                    guard.leave_raw()?;
                    let result = editor::edit_draft(
                        &argv,
                        composer.text(),
                        &std::env::current_dir()?,
                        scratch.path(),
                    );
                    guard.enter_raw()?;
                    term.reacquire(&mut || probe_reacquire(Duration::from_millis(200)))?;
                    released.store(false, Ordering::SeqCst);
                    if let Some(input) = &input {
                        input.resume();
                    }
                    match result {
                        editor::EditResult::Loaded(text) => {
                            composer.load(text.clone());
                            commit(&mut term, format!("LOADED text={text:?}"))?;
                        }
                        editor::EditResult::Cleared => {
                            composer.clear();
                            commit(&mut term, format!("NOTICE: {}", editor::cleared_notice()))?;
                        }
                        editor::EditResult::Unchanged { hint: true } => commit(
                            &mut term,
                            format!("NOTICE: {}", editor::unchanged_hint_notice()),
                        )?,
                        editor::EditResult::Failed(text) => {
                            commit(&mut term, format!("NOTICE: {text}"))?
                        }
                        _ => {}
                    }
                    continue;
                }
                composer.handle_key(
                    key,
                    ComposerContext {
                        turn_running: false,
                    },
                    now,
                )
            }
            Some(Event::Paste(text)) => {
                if let Some(boxed) = &mut search {
                    boxed.handle_paste(&text);
                    continue;
                }
                composer.handle_paste(&text)
            }
            Some(Event::Resize(cols, rows)) => {
                term.resize(cols, rows)?;
                continue;
            }
            Some(_) => ComposerOutcome::Unchanged,
            None => composer.tick(now),
        };
        match outcome {
            ComposerOutcome::Submit(text) => {
                if let Some(notice) = history.append(&text) {
                    commit(&mut term, format!("NOTICE: {notice}"))?;
                }
                commit(&mut term, format!("SUBMITTED text={text:?}"))?;
                if alert.is_some() {
                    due = Some(now + Duration::from_secs(1));
                }
            }
            ComposerOutcome::ExitRequested => break,
            _ => {}
        }
    }
    drop(input);
    term.restore()
}

fn main() -> ExitCode {
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if !terminal {
        println!("comforts demo needs a terminal");
        return ExitCode::from(2);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("comforts demo: {error}");
            ExitCode::from(1)
        }
    }
}
