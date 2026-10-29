//! The comforts demo, driven through a pseudo-terminal.
//!
//! Behavior contract: issue 228.

#![cfg(unix)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

const CTRL_G: u8 = 0x07;
const CTRL_R: u8 = 0x12;
const CTRL_Z: u8 = 0x1a;
const ENTER: u8 = 0x0d;
const ESC: u8 = 0x1b;

fn editor_config(dir: &std::path::Path, command: &str) -> PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, format!("[editor]\ncommand = \"{command}\"\n")).unwrap();
    path
}

#[test]
fn ctrl_g_edits_and_returns() {
    let dir = tempfile::tempdir().unwrap();
    let config = editor_config(
        dir.path(),
        "sh -c 'IFS= read -r line; printf %s \\\"$line\\\" > \\\"$0\\\"'",
    );
    let history = dir.path().join("history.jsonl");
    let args = args_for(&config, &history);
    let Some(output) = drive(
        &args,
        &[b"draft".to_vec(), vec![CTRL_G]],
        Duration::from_millis(50),
        Duration::from_millis(300),
        &[b"typed".to_vec(), vec![ENTER]],
        Duration::from_secs(3),
    ) else {
        return;
    };
    assert!(output.contains("LOADED text=\"typed\""), "{output}");
    assert!(!output.contains("SUBMITTED"), "{output}");
}

#[test]
fn reacquire_probe_keeps_input_before_and_after_its_reply() {
    let dir = tempfile::tempdir().unwrap();
    let config = editor_config(dir.path(), "true");
    let history = dir.path().join("history.jsonl");
    let Some(mut demo) = Demo::open(&args_for(&config, &history)) else {
        return;
    };
    demo.wait_for_frame(Duration::from_secs(5));
    assert!(demo.write(&[CTRL_G]));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if demo
            .output
            .lock()
            .unwrap()
            .windows(4)
            .filter(|bytes| *bytes == b"\x1b[6n")
            .count()
            >= 2
        {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        demo.output
            .lock()
            .unwrap()
            .windows(4)
            .filter(|bytes| *bytes == b"\x1b[6n")
            .count()
            >= 2,
        "{}",
        demo.snapshot()
    );
    let writer = demo.writer.as_mut().unwrap();
    writer.write_all(b"hel\x1b[?62;22clo").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(200));
    assert!(demo.write(&[ENTER]));
    let output = demo.collect(Duration::from_secs(2));
    assert!(output.contains("SUBMITTED text=\"hello\""), "{output}");
}

#[test]
fn ctrl_c_in_editor_keeps_talaria() {
    let dir = tempfile::tempdir().unwrap();
    let config = editor_config(dir.path(), "sh -c \\\"sleep 30\\\"");
    let history = dir.path().join("history.jsonl");
    let args = args_for(&config, &history);
    let Some(output) = drive(
        &args,
        &[b"draft".to_vec(), vec![CTRL_G]],
        Duration::from_millis(50),
        Duration::from_millis(300),
        &[vec![0x03], b"!".to_vec(), vec![ENTER]],
        Duration::from_secs(4),
    ) else {
        return;
    };
    assert!(
        output.contains("The editor was stopped by signal 2; the draft is unchanged."),
        "{output}"
    );
    assert!(
        output.contains("SUBMITTED text=\"draft!\""),
        "the demo lives on with its draft: {output}"
    );
}

fn bash() -> Option<PathBuf> {
    let probe = Command::new("bash").arg("--version").output().ok()?;
    probe.status.success().then(|| PathBuf::from("bash"))
}

fn shell_job_control(shell: &mut Shell) -> bool {
    shell.write_line("set -o | grep monitor");
    let output = shell.drain(Duration::from_secs(3));
    output.contains("monitor") && !output.contains("off")
}

#[test]
fn ctrl_z_then_fg_restores_screen() {
    let Some(_) = bash() else {
        eprintln!("SKIP: bash not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("history.jsonl");
    let Some(mut shell) = Shell::open() else {
        return;
    };
    if !shell_job_control(&mut shell) {
        if std::env::var("CI").is_ok() {
            panic!("job control is off under CI");
        }
        eprintln!("SKIP: job control is off");
        return;
    }
    let demo = demo_binary();
    shell.write_line(&format!(
        "'{}' --history-file '{}'",
        demo.display(),
        history.display()
    ));
    shell.wait_for_text("composer", Duration::from_secs(2));
    shell.write_bytes(b"draft");
    thread::sleep(Duration::from_millis(200));
    shell.write_bytes(&[CTRL_Z]);
    let stopped = shell.drain(Duration::from_secs(3));
    assert!(
        stopped.to_lowercase().contains("stopped") || stopped.to_lowercase().contains("suspended"),
        "the shell reports the stopped job: {stopped}"
    );
    shell.write_line("fg");
    thread::sleep(Duration::from_millis(500));
    shell.write_bytes(b"!");
    shell.write_bytes(&[ENTER]);
    let output = shell.drain(Duration::from_secs(3));
    assert!(output.contains("RESUMED"), "{output}");
    assert!(
        output.contains("SUBMITTED text=\"draft!\""),
        "the draft survived suspend and resume: {output}"
    );
    assert_eq!(
        output.matches("SUBMITTED").count(),
        1,
        "no line is doubled: {output}"
    );
}

#[test]
fn ctrl_z_without_job_control_or_ignored_stop() {
    // No shell above the demo: suspend is refused, the demo keeps running.
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("history.jsonl");
    let args = vec![
        "--history-file".to_owned(),
        history.to_string_lossy().into_owned(),
    ];
    let Some(mut demo) = Demo::open(&args) else {
        return;
    };
    demo.wait_for_frame(Duration::from_secs(10));
    demo.write(b"still here");
    thread::sleep(Duration::from_millis(200));
    demo.write(&[CTRL_Z]);
    thread::sleep(Duration::from_millis(500));
    let output = demo.collect(Duration::from_secs(2));
    assert!(
        output.contains("Ctrl+Z cannot suspend here:"),
        "S1 shows without job control: {output}"
    );
    assert!(demo.is_running(), "the demo keeps running");

    // An ignored stop signal surfaces S2 after the wait, still running.
    let Some(_) = bash() else {
        eprintln!("SKIP: bash not found");
        return;
    };
    let Some(mut shell) = Shell::open() else {
        return;
    };
    let demo = demo_binary();
    shell.write_line(&format!(
        "sh -c \"trap '' TSTP; exec '{}' --history-file '{}'\"",
        demo.display(),
        history.display()
    ));
    thread::sleep(Duration::from_millis(500));
    shell.write_bytes(&[CTRL_Z]);
    let output = shell.drain(Duration::from_secs(4));
    assert!(
        output
            .contains("Suspend did not take effect: the stop signal is ignored in this terminal."),
        "{output}"
    );
}

#[test]
fn ctrl_r_finds_prompt_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("history.jsonl");
    let history_arg = history.to_string_lossy().into_owned();
    let args = vec!["--history-file".to_owned(), history_arg];
    let Some(output) = drive(
        &args,
        &[b"alpha one".to_vec(), vec![ENTER]],
        Duration::from_millis(100),
        Duration::from_millis(200),
        &[b"beta two".to_vec(), vec![ENTER], vec![ESC], vec![ESC]],
        Duration::from_secs(3),
    ) else {
        return;
    };
    assert!(output.contains("SUBMITTED text=\"alpha one\""), "{output}");
    assert!(output.contains("SUBMITTED text=\"beta two\""), "{output}");

    let args = vec![
        "--history-file".to_owned(),
        history.to_string_lossy().into_owned(),
    ];
    let Some(output) = drive(
        &args,
        &[vec![CTRL_R]],
        Duration::ZERO,
        Duration::from_millis(200),
        &[b"alp".to_vec(), vec![ENTER]],
        Duration::from_secs(3),
    ) else {
        return;
    };
    assert!(output.contains("LOADED text=\"alpha one\""), "{output}");
}

fn args_for(config: &std::path::Path, history: &std::path::Path) -> Vec<String> {
    vec![
        "--config".to_owned(),
        config.to_string_lossy().into_owned(),
        "--history-file".to_owned(),
        history.to_string_lossy().into_owned(),
        "--folder".to_owned(),
        "test-workspace".to_owned(),
    ]
}

/// Write the first chunks, wait, write the rest, and return the demo's output.
/// `None` means no pseudo-terminal could be opened; the skip line was printed.
fn drive(
    args: &[String],
    first: &[Vec<u8>],
    gap: Duration,
    settle: Duration,
    rest: &[Vec<u8>],
    timeout: Duration,
) -> Option<String> {
    let mut demo = Demo::open(args)?;
    demo.wait_for_frame(Duration::from_secs(10));
    for (index, chunk) in first.iter().enumerate() {
        if index > 0 && !gap.is_zero() {
            thread::sleep(gap);
        }
        demo.write(chunk);
    }
    if !settle.is_zero() {
        thread::sleep(settle);
    }
    for chunk in rest {
        demo.write(chunk);
        if chunk == &[0x03] {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(2)
                && !demo.snapshot().contains("The editor was stopped by signal")
            {
                thread::sleep(Duration::from_millis(20));
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Some(demo.collect(timeout))
}

struct Demo {
    child: Box<dyn Child + Send + Sync>,
    writer: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader_done: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Demo {
    fn open(args: &[String]) -> Option<Self> {
        Self::spawn("comforts_demo", args, None)
    }

    fn spawn(_label: &str, args: &[String], program: Option<(&str, &[String])>) -> Option<Self> {
        let system = native_pty_system();
        let pair = match system.openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("SKIP: no pseudo-terminal");
                let _ = error;
                return None;
            }
        };
        let command = match program {
            Some((shell, shell_args)) => {
                let mut command = CommandBuilder::new(shell);
                for arg in shell_args {
                    command.arg(arg);
                }
                command
            }
            None => {
                let demo = demo_binary();
                assert!(demo.is_file(), "missing {}", demo.display());
                let mut command = CommandBuilder::new(&demo);
                for arg in args {
                    command.arg(arg);
                }
                command
            }
        };
        let child = pair
            .slave
            .spawn_command(command)
            .unwrap_or_else(|error| panic!("spawn comforts demo: {error}"));
        let mut reader_half = pair.master.try_clone_reader().expect("pty reader");
        let writer = pair.master.take_writer().expect("pty writer");
        let output = Arc::new(Mutex::new(Vec::new()));
        let reader_done = Arc::new(AtomicBool::new(false));
        let sink = Arc::clone(&output);
        let done = Arc::clone(&reader_done);
        let reader = thread::spawn(move || {
            let mut buf = [0u8; 1_024];
            loop {
                match reader_half.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        sink.lock().expect("output").extend_from_slice(&buf[..n]);
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
        });
        Some(Self {
            child,
            writer: Some(writer),
            output,
            reader_done,
            reader: Some(reader),
        })
    }

    fn wait_for_frame(&mut self, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self
                .output
                .lock()
                .expect("output")
                .windows(b"Enter sends".len())
                .any(|bytes| bytes == b"Enter sends")
            {
                return;
            }
            let exited = self.child.try_wait().ok().flatten().is_some();
            if exited && self.reader_done.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("demo did not render its input surface: {}", self.snapshot());
    }

    fn is_running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    fn write(&mut self, bytes: &[u8]) -> bool {
        let Some(writer) = self.writer.as_mut() else {
            return false;
        };
        for byte in bytes {
            if writer
                .write_all(&[*byte])
                .and_then(|()| writer.flush())
                .is_err()
            {
                return false;
            }
            if bytes.len() > 1
                && bytes
                    .iter()
                    .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
            {
                thread::sleep(Duration::from_millis(20));
            }
        }
        true
    }

    fn snapshot(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().expect("output")).into_owned()
    }

    fn collect(&mut self, timeout: Duration) -> String {
        let start = Instant::now();
        while start.elapsed() < timeout {
            let exited = self.child.try_wait().ok().flatten().is_some();
            if exited && self.reader_done.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        self.snapshot()
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.writer.take();
        let start = Instant::now();
        while !self.reader_done.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(20));
        }
        if self.reader_done.load(Ordering::SeqCst)
            && let Some(reader) = self.reader.take()
        {
            let _ = reader.join();
        }
    }
}

/// An interactive shell in a pseudo-terminal, for the suspend tests.
struct Shell {
    demo: Demo,
    seen: usize,
}

impl Shell {
    fn open() -> Option<Self> {
        let demo = Demo::spawn(
            "shell",
            &[],
            Some((
                "bash",
                &[
                    "--noprofile".to_owned(),
                    "--norc".to_owned(),
                    "-i".to_owned(),
                ],
            )),
        )?;
        let mut shell = Self { demo, seen: 0 };
        shell.wait_for_text("$", Duration::from_secs(5));
        Some(shell)
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.demo.write(bytes);
    }

    fn write_line(&mut self, line: &str) {
        let mut command = line.as_bytes().to_vec();
        command.push(b'\n');
        self.demo.write(&command);
    }

    fn wait_for_text(&mut self, needle: &str, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.demo.snapshot()[self.seen..].contains(needle) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Output since the last drain.
    fn drain(&mut self, wait: Duration) -> String {
        thread::sleep(wait);
        let snapshot = self.demo.snapshot();
        let fresh = snapshot[self.seen..].to_owned();
        self.seen = snapshot.len();
        fresh
    }
}

fn demo_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("test binary");
    let deps = exe.parent().expect("deps directory");
    let profile = deps.parent().expect("profile directory");
    let binary = profile.join("examples").join("comforts_demo");
    if !binary.exists() {
        static BUILD: Once = Once::new();
        BUILD.call_once(|| {
            let target_dir = profile.parent().expect("target directory");
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let mut command = Command::new(cargo);
            command.args([
                "build",
                "--quiet",
                "-p",
                "talaria-tui",
                "--example",
                "comforts_demo",
                "--target-dir",
            ]);
            command.arg(target_dir);
            if !cfg!(debug_assertions) {
                command.arg("--release");
            }
            let status = command.status().expect("build comforts_demo");
            assert!(status.success(), "comforts_demo failed to build");
        });
    }
    binary
}
