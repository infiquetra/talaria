//! The replay entry point the performance harness drives.

use std::path::PathBuf;
use std::process::ExitCode;

use hermes_client::replay_source::Speed;

pub struct BenchReplayArgs {
    pub fixture: PathBuf,
    pub speed: Speed,
    pub report: Option<PathBuf>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ArgError {
    MissingFixture,
    BadSpeed(String),
    MissingValue(String),
}

/// `Ok(None)` when the flag is absent.
pub fn parse_args(args: &[String]) -> Result<Option<BenchReplayArgs>, ArgError> {
    if !args.iter().any(|arg| arg == "--bench-replay") {
        return Ok(None);
    }
    let mut fixture = None;
    let mut speed = Speed::Recorded;
    let mut report = None;
    let mut index = 0;
    while index < args.len() {
        let key = &args[index];
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| ArgError::MissingValue(key.clone()))?;
        match key.as_str() {
            "--bench-replay" => fixture = Some(PathBuf::from(value)),
            "--bench-speed" => {
                speed = Speed::parse(value).map_err(|_| ArgError::BadSpeed(value.clone()))?
            }
            "--bench-report" => report = Some(PathBuf::from(value)),
            _ => return Err(ArgError::MissingValue("unknown replay option".into())),
        }
        index += 1;
    }
    Ok(Some(BenchReplayArgs {
        fixture: fixture.ok_or(ArgError::MissingFixture)?,
        speed,
        report,
    }))
}

pub fn run(args: BenchReplayArgs) -> ExitCode {
    use std::io::IsTerminal;
    let source = match hermes_client::replay_source::ReplaySource::open(&args.fixture) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("talaria: {}: {error}", args.fixture.display());
            return ExitCode::from(2);
        }
    };
    if source.dropped_frames() > 0 {
        eprintln!(
            "talaria: recording is incomplete: {} dropped frames",
            source.dropped_frames()
        );
        return ExitCode::from(2);
    }
    if source.truncated() {
        eprintln!("talaria: recording is incomplete: truncated final record");
        return ExitCode::from(2);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("talaria: --bench-replay needs a terminal");
        return ExitCode::from(2);
    }
    match play(source, args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("talaria: replay failed: {error}");
            ExitCode::from(2)
        }
    }
}

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadSpeed(_) => "--bench-speed must be instant, recorded or a positive number",
            Self::MissingFixture => "--bench-replay needs a fixture",
            Self::MissingValue(_) => "replay option needs a value",
        })
    }
}

use hermes_client::replay_source::{ReplayItem, ReplaySource};
use hermes_client::{Decoded, Generation, Inbound, Responder, decode_frame};
use ratatui::crossterm::event::{Event as InputEvent, KeyCode, KeyEventKind, KeyModifiers};
use std::io::{self, Write};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
use talaria_core::session::{Event, SessionId, SessionState, update};
use talaria_tui::history::{CellLines, CommitLedger, PlainCellLines};
use talaria_tui::live::{LiveView, PlainTextSection, max_live_height};
use talaria_tui::terminal::{
    InlineTerminal, TerminalError, TerminalGuard, probe_startup, spawn_input, terminal_size,
};

struct CountingWriter<W> {
    inner: W,
    bytes: Arc<AtomicU64>,
}
impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
enum Message {
    Replay(ReplayItem),
    Input(InputEvent),
    Finished(bool),
}

#[derive(Default)]
struct Counters {
    replay_frames: u64,
    frames_skipped: u64,
    loop_passes: u64,
    frames_drawn: u64,
    worst_pass_us: u64,
}

fn play(source: ReplaySource, args: BenchReplayArgs) -> io::Result<()> {
    let id = source
        .inbound()
        .find_map(|item| {
            (item.frame["method"] == "event")
                .then(|| item.frame["params"]["session_id"].as_str())
                .flatten()
        })
        .unwrap_or("bench")
        .to_owned();
    let mut state = SessionState::empty(SessionId::new(id));
    let guard = TerminalGuard::acquire().map_err(terminal_error)?;
    let probe = probe_startup(Duration::from_millis(300)).map_err(terminal_error)?;
    let size = terminal_size()?;
    let bytes = Arc::new(AtomicU64::new(0));
    let writer = CountingWriter {
        inner: io::stdout(),
        bytes: bytes.clone(),
    };
    let mut terminal =
        InlineTerminal::new(writer, size, max_live_height(size.1), probe, guard.link())?;
    let (sender, receiver) = mpsc::channel();
    let input_sender = sender.clone();
    let input = spawn_input(move |event| input_sender.send(Message::Input(event)).is_ok());
    let (stop, mut cancelled) = tokio::sync::watch::channel(false);
    let replay = std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            let _ = sender.send(Message::Finished(false));
            return;
        };
        runtime.block_on(async move {
            let (frames, mut incoming) = tokio::sync::mpsc::channel(1024);
            let job = tokio::spawn(async move { source.run(args.speed, frames).await });
            loop {
                tokio::select! {
                    _=cancelled.changed()=>{job.abort();return;},
                    item=incoming.recv()=>match item {
                        Some(item)=>if sender.send(Message::Replay(item)).is_err(){job.abort();return;},
                        None=>break,
                    }
                }
            }
            let _ = sender.send(Message::Finished(matches!(job.await, Ok(Ok(_)))));
        });
    });
    let result = event_loop(
        &mut terminal,
        &mut state,
        &receiver,
        args.report.as_deref(),
        &bytes,
        size,
    );
    let _ = stop.send(true);
    drop(receiver);
    if let Some(input) = input {
        input.stop();
    }
    let restored = terminal.restore();
    let _ = replay.join();
    result.and(restored)
}

fn event_loop<W: Write>(
    terminal: &mut InlineTerminal<W>,
    state: &mut SessionState,
    receiver: &mpsc::Receiver<Message>,
    report: Option<&std::path::Path>,
    bytes: &AtomicU64,
    mut size: (u16, u16),
) -> io::Result<()> {
    let mut ledger = CommitLedger::new();
    let mut draft = String::new();
    let mut counters = Counters::default();
    let mut reported = false;
    let start = Instant::now();
    draw(
        terminal,
        state,
        &mut ledger,
        &draft,
        bytes,
        &mut counters,
        size,
    )?;
    counters.loop_passes += 1;
    counters.worst_pass_us = start.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    loop {
        let first = receiver
            .recv()
            .map_err(|_| io::Error::other("replay input channel closed"))?;
        let start = Instant::now();
        let mut finished = false;
        let mut changed = false;
        let mut quit = false;
        for message in std::iter::once(first).chain(std::iter::from_fn(|| receiver.try_recv().ok()))
        {
            match message {
                Message::Replay(item) => {
                    counters.replay_frames += 1;
                    if !apply_frame(state, item) {
                        counters.frames_skipped += 1;
                    }
                    changed = true;
                }
                Message::Finished(ok) => {
                    if !ok {
                        return Err(io::Error::other("recording replay failed"));
                    }
                    finished = true;
                }
                Message::Input(event) => match event {
                    InputEvent::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                        KeyCode::Char('c' | 'd')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            quit = true
                        }
                        KeyCode::Char(ch)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                        {
                            draft.push(ch);
                            changed = true;
                        }
                        KeyCode::Backspace => {
                            changed = draft.pop().is_some() || changed;
                        }
                        KeyCode::Enter => {
                            changed = !draft.is_empty() || changed;
                            draft.clear();
                        }
                        _ => {}
                    },
                    InputEvent::Paste(text) => {
                        draft.extend(text.chars().filter(|c| !c.is_control()));
                        changed = true;
                    }
                    InputEvent::Resize(cols, rows) => {
                        size = (cols, rows);
                        terminal.resize(cols, rows)?;
                        changed = true;
                    }
                    _ => {}
                },
            }
        }
        if changed || finished {
            draw(
                terminal,
                state,
                &mut ledger,
                &draft,
                bytes,
                &mut counters,
                size,
            )?;
        }
        counters.loop_passes += 1;
        counters.worst_pass_us = counters
            .worst_pass_us
            .max(start.elapsed().as_micros().try_into().unwrap_or(u64::MAX));
        if finished && !reported {
            if let Some(path) = report {
                write_report(path, state, &counters, bytes.load(Ordering::Relaxed))?;
            }
            reported = true;
        }
        if quit {
            return Ok(());
        }
    }
}

fn apply_frame(state: &mut SessionState, item: ReplayItem) -> bool {
    if let Decoded::Inbound(Inbound::Event(event)) = decode_frame(
        Generation(item.generation),
        item.text.as_bytes(),
        &mut |_| Responder::detached(Arc::new(AtomicU64::new(0))),
    ) && let Some(id) = event.frame.session_id.clone()
    {
        let _ = update(
            state,
            Event::Session {
                session_id: SessionId::new(id),
                frame: event.frame,
                raw: event.raw,
            },
        );
        true
    } else {
        false
    }
}

fn draw<W: Write>(
    terminal: &mut InlineTerminal<W>,
    state: &SessionState,
    ledger: &mut CommitLedger,
    draft: &str,
    bytes: &AtomicU64,
    counters: &mut Counters,
    size: (u16, u16),
) -> io::Result<()> {
    let before = bytes.load(Ordering::Relaxed);
    terminal.commit(ledger.take_ready(state.cells()), &PlainCellLines)?;
    if bytes.load(Ordering::Relaxed) > before {
        counters.frames_drawn += 1;
    }
    let tail = state
        .live_tail()
        .iter()
        .flat_map(|cell| PlainCellLines.lines(cell, size.0))
        .map(|line| {
            line.spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let tail = PlainTextSection::new(&tail);
    let composer = PlainTextSection::new(&format!("> {draft}"));
    let view = LiveView {
        sections: vec![&tail, &composer],
    };
    if terminal.draw_live(&view)?.drew {
        counters.frames_drawn += 1;
    }
    Ok(())
}

fn write_report(
    path: &std::path::Path,
    state: &SessionState,
    counters: &Counters,
    bytes: u64,
) -> io::Result<()> {
    let value = serde_json::json!({"format":1,"replay_frames":counters.replay_frames,"frames_skipped":counters.frames_skipped,
        "cells":state.cells().len(),"loop_passes":counters.loop_passes,"frames_drawn":counters.frames_drawn,
        "bytes_written":bytes,"worst_pass_us":counters.worst_pass_us});
    let temporary = path.with_extension(format!("bench-{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        writeln!(file, "{value}")?;
        file.flush()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
fn terminal_error(error: TerminalError) -> io::Error {
    match error {
        TerminalError::Io(error) => error,
        TerminalError::NotATerminal => io::Error::other("replay needs a terminal"),
    }
}
