//! The real `talaria` binary, built in debug mode and driven in a pseudo-terminal.
//!
//! These tests assert behaviour: what is on the screen, what the report file counts, how
//! the process ends. They do not assert budgets; a debug build is slow, and the budgets are
//! checked on the release build by the full run at the bottom of this file.

mod support;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use talaria_bench::pty::run_in_pty;
use talaria_bench::{
    Budgets, Emulator, FrameSplitter, Query, ResponderState, RunOptions, answer_queries, exit_code,
    generate, keystroke_schedule, run, typed_prefix_visible,
};

const ROWS: u16 = 48;
const COLS: u16 = 160;

/// The 40 characters the keystroke measurement types. Generated text never holds `^`.
const PROBE: &str = "^kbdprobe-0123456789-abcdefghijklmnopqrs";

/// The recorded reply in the plain-answer recording, as it reads on screen.
const PLAIN_ANSWER_REPLY: &str = "The directory contains a git repository and a tests directory.";

const MISSING_FIXTURE: &str = "no-such-fixture.jsonl";

/// Where cargo puts build output: `CARGO_TARGET_DIR` when set, else `target` in the repository.
fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => root.join(dir),
        None => root.join("target"),
    }
}

/// Builds the debug `talaria` once for the whole test binary and returns its path. A build
/// failure fails the test that asked.
fn debug_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = support::workspace_root();
        let output = Command::new(env!("CARGO"))
            .args(["build", "-p", "talaria-cli"])
            .current_dir(&root)
            .output()
            .expect("start cargo to build the debug talaria binary");
        assert!(
            output.status.success(),
            "building the debug talaria binary failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = target_dir(&root).join("debug/talaria");
        assert!(
            path.is_file(),
            "the build succeeded but there is no debug talaria binary where cargo puts it"
        );
        path
    })
}

fn fixture(name: &str) -> PathBuf {
    support::workspace_root().join("fixtures/frames").join(name)
}

fn path_arg(path: &Path) -> String {
    path.to_str().expect("test paths are text").to_string()
}

fn clean_env(home: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("TERM".to_string(), "xterm-256color".to_string()),
        ("COLORTERM".to_string(), "truecolor".to_string()),
        ("LANG".to_string(), "C.UTF-8".to_string()),
        ("HOME".to_string(), path_arg(home)),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        env.push(("PATH".to_string(), path.to_string_lossy().into_owned()));
    }
    env
}

/// Kills and reaps the child however the test ends, so no process is left behind.
struct ChildGuard {
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn MasterPty + Send>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One run of the binary in a 160 by 48 pseudo-terminal, read on the calling thread:
/// capability queries are answered, output is split into synchronized frames and fed to an
/// emulator, the way the harness reads it.
struct Session {
    guard: ChildGuard,
    writer: Box<dyn Write + Send>,
    chunks: Receiver<(Instant, Vec<u8>)>,
    emulator: Emulator,
    responder: ResponderState,
    splitter: FrameSplitter,
    /// The end of the first frame that left a non-blank cell.
    start_frame_end: Option<Instant>,
    /// Every complete frame since the process started.
    frames: usize,
    bytes: usize,
    /// Queries that arrived after the start frame.
    late_queries: Vec<Query>,
    closed: bool,
}

impl Session {
    fn spawn(binary: &Path, args: &[String], home: &Path) -> Session {
        // The stubs of the harness are touched first, before any process exists.
        let emulator = Emulator::new(ROWS, COLS);
        let responder = ResponderState::new();
        let splitter = FrameSplitter::new();

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pseudo-terminal");
        let mut command = CommandBuilder::new(binary);
        command.env_clear();
        for (key, value) in clean_env(home) {
            command.env(key, value);
        }
        command.cwd(home);
        command.args(args);
        let child = pair
            .slave
            .spawn_command(command)
            .expect("start the binary in the pseudo-terminal");
        drop(pair.slave);
        let guard = ChildGuard {
            child,
            _master: pair.master,
        };
        let mut reader = guard
            ._master
            .try_clone_reader()
            .expect("read the pseudo-terminal");
        let writer = guard
            ._master
            .take_writer()
            .expect("write the pseudo-terminal");

        let (sender, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender
                            .send((Instant::now(), buffer[..count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        Session {
            guard,
            writer,
            chunks,
            emulator,
            responder,
            splitter,
            start_frame_end: None,
            frames: 0,
            bytes: 0,
            late_queries: Vec::new(),
            closed: false,
        }
    }

    fn handle(&mut self, at: Instant, chunk: &[u8]) {
        self.bytes += chunk.len();
        let answers = answer_queries(&mut self.responder, chunk, &mut self.emulator);
        if !answers.reply.is_empty() {
            let _ = self.writer.write_all(&answers.reply);
            let _ = self.writer.flush();
        }
        if self.start_frame_end.is_some() {
            self.late_queries.extend(answers.seen);
        }
        let frames = self.splitter.push(chunk, at);
        self.frames += frames.len();
        if self.start_frame_end.is_none()
            && !frames.is_empty()
            && self.emulator.has_non_blank_cell()
        {
            self.start_frame_end = Some(at);
        }
    }

    /// Reads one chunk if one arrives within `wait`.
    fn pump(&mut self, wait: Duration) {
        match self.chunks.recv_timeout(wait) {
            Ok((at, chunk)) => self.handle(at, &chunk),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => self.closed = true,
        }
    }

    /// Reads output until `done` holds (true) or the deadline passes or the output ends (false).
    fn pump_until(&mut self, deadline: Instant, mut done: impl FnMut(&Session) -> bool) -> bool {
        loop {
            if done(self) {
                return true;
            }
            let now = Instant::now();
            if self.closed || now >= deadline {
                return false;
            }
            self.pump((deadline - now).min(Duration::from_millis(10)));
        }
    }

    /// Keeps reading until `until`, and fails if the binary stops before then.
    fn pump_to(&mut self, until: Instant) {
        self.pump_until(until, |_| false);
        assert!(
            !self.closed,
            "{}",
            self.describe("the binary ended while it should have run")
        );
    }

    fn write_key(&mut self, key: u8) {
        self.writer.write_all(&[key]).expect("write a key");
        self.writer.flush().expect("flush a key");
    }

    fn alive(&mut self) -> bool {
        matches!(self.guard.child.try_wait(), Ok(None))
    }

    /// What a failure message needs: the reason, the exit status when there is one, and the
    /// visible screen text.
    fn describe(&mut self, reason: &str) -> String {
        let status = match self.guard.child.try_wait() {
            Ok(Some(status)) => format!("exited with status {}", status.exit_code()),
            Ok(None) => "still running".to_string(),
            Err(error) => format!("status unknown ({error})"),
        };
        let screen: Vec<String> = self
            .emulator
            .rows_text()
            .into_iter()
            .map(|row| row.trim_end().to_string())
            .filter(|row| !row.is_empty())
            .collect();
        format!(
            "{reason}; the binary is {status}; screen:\n{}",
            screen.join("\n")
        )
    }

    fn wait_for_start_frame(&mut self, timeout: Duration) -> Instant {
        let deadline = Instant::now() + timeout;
        if !self.pump_until(deadline, |s| s.start_frame_end.is_some()) {
            let reason = format!(
                "no frame with content within {timeout:?}; did the binary answer the capability queries?"
            );
            panic!("{}", self.describe(&reason));
        }
        self.start_frame_end.expect("start frame was seen")
    }
}

fn read_report(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).expect("read the report file");
    serde_json::from_str(&text).expect("the report file is JSON")
}

fn report_number(report: &serde_json::Value, key: &str) -> u64 {
    report[key]
        .as_u64()
        .unwrap_or_else(|| panic!("the report has no number named {key}: {report}"))
}

#[test]
fn replay_shows_reply_composer_and_echoes_typing() {
    let work = tempfile::tempdir().expect("temporary directory");
    let report = work.path().join("report.json");
    let args = vec![
        "--bench-replay".to_string(),
        path_arg(&fixture("plain-answer.jsonl")),
        "--bench-report".to_string(),
        path_arg(&report),
    ];
    let mut session = Session::spawn(debug_binary(), &args, work.path());
    let origin = session.wait_for_start_frame(Duration::from_secs(30));

    // Type the whole probe, one key every 25 ms, never Enter.
    for (index, key) in PROBE.bytes().enumerate() {
        let due = origin + Duration::from_millis(100 + 25 * index as u64);
        session.pump_to(due);
        session.write_key(key);
    }

    let echoed = session.pump_until(Instant::now() + Duration::from_secs(10), |s| {
        typed_prefix_visible(&s.emulator, PROBE, PROBE.chars().count())
    });
    assert!(
        echoed,
        "{}",
        session.describe("the 40 typed characters did not all appear in the composer")
    );

    // The recording is 80 ms long; the report marks the end of the replay.
    let ended = session.pump_until(Instant::now() + Duration::from_secs(10), |_| {
        report.exists()
    });
    assert!(
        ended,
        "{}",
        session.describe("the replay never wrote its report")
    );
    session.pump_to(Instant::now() + Duration::from_millis(500));

    let rows = session.emulator.rows_text();
    let reply_row = rows.iter().position(|row| row.contains(PLAIN_ANSWER_REPLY));
    let composer_row = rows.iter().position(|row| row.contains(PROBE));
    let reply_row = reply_row
        .unwrap_or_else(|| panic!("{}", session.describe("the reply is not on the screen")));
    let composer_row = composer_row.unwrap_or_else(|| {
        panic!(
            "{}",
            session.describe("the typed probe is not on the screen")
        )
    });
    assert!(
        reply_row < composer_row,
        "{}",
        session.describe("the composer should sit below the reply")
    );

    // Typing without Enter submits nothing and ends nothing.
    assert!(
        session.alive(),
        "{}",
        session.describe("the process should still be running")
    );
    assert!(
        session.late_queries.is_empty(),
        "queries arrived after start-up: {:?}",
        session.late_queries
    );
    assert!(session.frames > 0, "no synchronized frames were seen");
}

#[test]
fn report_counts_match_idle_is_silent_and_10x_completes() {
    let budgets = Budgets::load(&support::shipped_budgets_path()).expect("load the budgets");
    let work = tempfile::tempdir().expect("temporary directory");
    let scenarios = work.path().join("scenarios");
    std::fs::create_dir(&scenarios).expect("create the scenario directory");
    generate(
        &support::workspace_root().join("fixtures/frames"),
        &scenarios,
        &budgets,
    )
    .expect("generate the replay files");
    let schedule = keystroke_schedule(&budgets, 10.0);
    assert_eq!(
        schedule.len(),
        PROBE.len(),
        "the schedule types one key per probe character"
    );
    let binary = debug_binary();

    // The reference stream at ten times speed, typed over on the fast schedule.
    let report_path = work.path().join("reference-report.json");
    let args = vec![
        "--bench-replay".to_string(),
        path_arg(&scenarios.join("reference-stream.jsonl")),
        "--bench-speed".to_string(),
        "10".to_string(),
        "--bench-report".to_string(),
        path_arg(&report_path),
    ];
    let mut session = Session::spawn(binary, &args, work.path());
    let origin = session.wait_for_start_frame(Duration::from_secs(30));
    let finish_by = origin + Duration::from_secs(15);
    let probe = PROBE.as_bytes();
    let mut written = 0;
    let report_seen = loop {
        let now = Instant::now();
        while written < schedule.len() && now >= origin + schedule[written] {
            session.write_key(probe[written]);
            written += 1;
        }
        if report_path.exists() {
            break now;
        }
        assert!(
            now < finish_by,
            "{}",
            session.describe("the ten times replay did not finish within 15 s")
        );
        assert!(
            !session.closed,
            "{}",
            session.describe("the binary ended before it wrote its report")
        );
        session.pump(Duration::from_millis(10));
    };
    assert_eq!(
        written,
        schedule.len(),
        "the report appeared after only {written} of {} keys were written",
        schedule.len()
    );

    // The harness count runs to two seconds after the report appears.
    session.pump_to(report_seen + Duration::from_secs(2));
    let counted = session.frames as u64;
    let report = read_report(&report_path);
    assert_eq!(report_number(&report, "format"), 1);
    let drawn = report_number(&report, "frames_drawn");
    let allowed = counted.div_ceil(100);
    assert!(
        drawn.abs_diff(counted) <= allowed,
        "the binary says it drew {drawn} frames, the terminal side counted {counted} (allowed difference {allowed})"
    );

    // Then nothing more is written: no bytes, so no frames.
    let (frames_before, bytes_before) = (session.frames, session.bytes);
    session.pump_to(Instant::now() + Duration::from_secs(2));
    assert_eq!(
        (session.frames, session.bytes),
        (frames_before, bytes_before),
        "the idle screen was redrawn after the replay"
    );
    assert!(
        session.alive(),
        "{}",
        session.describe("the process should keep running when idle")
    );
    drop(session);

    // The long session at instant speed holds two cells per turn.
    let report_path = work.path().join("long-report.json");
    let args = vec![
        "--bench-replay".to_string(),
        path_arg(&scenarios.join("long-session.jsonl")),
        "--bench-speed".to_string(),
        "instant".to_string(),
        "--bench-report".to_string(),
        path_arg(&report_path),
    ];
    let mut session = Session::spawn(binary, &args, work.path());
    session.wait_for_start_frame(Duration::from_secs(30));
    let ended = session.pump_until(Instant::now() + Duration::from_secs(90), |_| {
        report_path.exists()
    });
    assert!(
        ended,
        "{}",
        session.describe("the long session never wrote its report")
    );
    let report = read_report(&report_path);
    assert_eq!(
        report_number(&report, "cells"),
        2 * budgets.scenario.long_session_turns,
        "the long session should hold one assistant cell and one tool cell per turn"
    );
}

#[test]
#[ignore = "needs the release binary and passes only when it meets every budget"]
fn release_budgets_hold() {
    let root = support::workspace_root();
    let binary = target_dir(&root).join("release/talaria");
    assert!(
        binary.is_file(),
        "build the release binary first: cargo build --release -p talaria-cli"
    );
    let budgets = Budgets::load(&support::shipped_budgets_path()).expect("load the budgets");
    let mut options = RunOptions::new(binary, budgets);
    options.fixtures = root.join("fixtures/frames");
    let report = run(&options).expect("the harness should run");
    assert_eq!(
        exit_code(&report),
        0,
        "a budget is exceeded on the release binary:\n{}",
        report.render()
    );
}

#[test]
fn missing_fixture_in_a_terminal_exits_2_without_touching_it() {
    let binary = debug_binary();
    let home = tempfile::tempdir().expect("temporary directory");
    let args = vec!["--bench-replay".to_string(), MISSING_FIXTURE.to_string()];
    let run = run_in_pty(binary, &args, &clean_env(home.path()), (ROWS, COLS))
        .expect("run the binary in a pseudo-terminal");

    assert_eq!(run.exit_code, Some(2), "a missing fixture exits 2");
    assert!(
        !run.output.contains(&0x1b),
        "no terminal control byte may be written before the fixture opens"
    );
    let text = String::from_utf8_lossy(&run.output).into_owned();
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected one plain line, got {text:?}");
    assert!(
        lines[0].contains(MISSING_FIXTURE),
        "the line should name the path as given: {:?}",
        lines[0]
    );
}
