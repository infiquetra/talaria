//! The demo, driven through a pseudo-terminal.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

#[test]
fn bracketed_paste_of_10000_characters_is_one_event() {
    let text = ten_thousand();
    let mut bytes = vec![0x1b];
    bytes.extend_from_slice(b"[200~");
    bytes.extend_from_slice(text.as_bytes());
    bytes.push(0x1b);
    bytes.extend_from_slice(b"[201~");
    bytes.push(0x0d);
    let Some(output) = drive(
        &[],
        &[bytes],
        Duration::ZERO,
        Duration::ZERO,
        false,
        Duration::from_secs(3),
    ) else {
        return;
    };
    assert_eq!(output.matches("PASTE-EVENT").count(), 1, "{output}");
    assert!(output.contains("PASTE-EVENT chars=10000"), "{output}");
    assert!(
        output.contains("SUBMITTED lines=100 chars=10000"),
        "{output}"
    );
}

#[test]
fn ctrl_j_alt_enter_lone_esc_and_no_terminal() {
    let writes = [
        vec![b'a'],
        vec![0x0a],
        vec![b'b'],
        vec![0x1b, 0x0d],
        vec![b'c'],
    ];
    let Some(output) = drive(
        &[],
        &writes,
        Duration::from_millis(50),
        Duration::from_millis(200),
        true,
        Duration::from_secs(2),
    ) else {
        return;
    };
    assert!(
        output.contains("SUBMITTED lines=3 chars=5 text=\"a\\nb\\nc\""),
        "{output}"
    );

    let Some(output) = drive(
        &["--running"],
        &[vec![0x1b]],
        Duration::ZERO,
        Duration::ZERO,
        false,
        Duration::from_millis(500),
    ) else {
        return;
    };
    assert!(output.contains("INTERRUPT"), "{output}");

    let demo = demo_binary();
    assert!(demo.is_file(), "missing {}", demo.display());
    let piped = Command::new(&demo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", demo.display()));
    let stdout = String::from_utf8_lossy(&piped.stdout);
    assert_eq!(
        piped.status.code(),
        Some(2),
        "stdout {stdout} stderr {}",
        String::from_utf8_lossy(&piped.stderr)
    );
    assert_eq!(stdout.lines().count(), 1, "{stdout:?}");
    assert!(!piped.stdout.contains(&0x1b));
    assert!(!piped.stderr.contains(&0x1b));
}

#[test]
#[ignore = "needs tmux"]
fn tmux_paste_buffer_arrives_as_one_event() {
    let probe = Command::new("tmux").arg("-V").output();
    if probe.map(|output| output.status.success()).unwrap_or(false) {
        run_tmux_paste();
    } else {
        eprintln!("SKIP: tmux not found");
    }
}

fn run_tmux_paste() {
    let demo = demo_binary();
    assert!(demo.is_file(), "missing {}", demo.display());
    let name = format!("talaria-composer-{}", std::process::id());
    let _session = TmuxSession::start(&name, &demo);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paste.txt");
    std::fs::write(&file, ten_thousand()).unwrap();
    let loaded = Command::new("tmux")
        .args(["load-buffer", "-t", &name])
        .arg(&file)
        .status()
        .unwrap();
    assert!(loaded.success());
    let pasted = Command::new("tmux")
        .args(["paste-buffer", "-p", "-t", &name])
        .status()
        .unwrap();
    assert!(pasted.success());
    thread::sleep(Duration::from_millis(500));
    let captured = Command::new("tmux")
        .args(["capture-pane", "-p", "-S", "-", "-t", &name])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&captured.stdout);
    assert_eq!(text.matches("PASTE-EVENT").count(), 1, "{text}");
}

struct TmuxSession(String);

impl TmuxSession {
    fn start(name: &str, demo: &PathBuf) -> Self {
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", name, "-x", "80", "-y", "24"])
            .arg(demo)
            .status()
            .unwrap();
        assert!(status.success(), "tmux new-session");
        Self(name.to_string())
    }
}

impl Drop for TmuxSession {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.0])
            .status();
    }
}

fn ten_thousand() -> String {
    let mut text = String::new();
    for _ in 0..99 {
        text.push_str(&"x".repeat(99));
        text.push('\n');
    }
    text.push_str(&"x".repeat(100));
    assert_eq!(text.chars().count(), 10_000);
    text
}

/// Write each chunk `gap` apart, wait `settle`, and return the demo's output.
/// `None` means no pseudo-terminal could be opened; the skip line was printed.
fn drive(
    args: &[&str],
    writes: &[Vec<u8>],
    gap: Duration,
    settle: Duration,
    enter_after: bool,
    timeout: Duration,
) -> Option<String> {
    let mut demo = Demo::open(args)?;
    demo.wait_for_frame(Duration::from_secs(1));
    for (index, chunk) in writes.iter().enumerate() {
        if index > 0 && !gap.is_zero() {
            thread::sleep(gap);
        }
        for piece in chunk.chunks(1_024) {
            if !demo.write(piece) {
                break;
            }
        }
    }
    if !settle.is_zero() {
        thread::sleep(settle);
    }
    if enter_after {
        let _ = demo.write(&[0x0d]);
    }
    Some(demo.collect(timeout))
}

struct Demo {
    child: Box<dyn Child + Send + Sync>,
    writer: Option<Box<dyn Write + Send>>,
    output: Arc<Mutex<Vec<u8>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    reader_done: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Demo {
    fn open(args: &[&str]) -> Option<Self> {
        let demo = demo_binary();
        assert!(demo.is_file(), "missing {}", demo.display());
        let system = native_pty_system();
        let pair = match system.openpty(PtySize {
            rows: 24,
            cols: 80,
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
        let mut command = CommandBuilder::new(&demo);
        for arg in args {
            command.arg(*arg);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .unwrap_or_else(|error| panic!("spawn {}: {error}", demo.display()));
        let mut reader_half = pair.master.try_clone_reader().expect("pty reader");
        let writer = pair.master.take_writer().expect("pty writer");
        let output = Arc::new(Mutex::new(Vec::new()));
        let parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 0)));
        let reader_done = Arc::new(AtomicBool::new(false));
        let sink = Arc::clone(&output);
        let screen = Arc::clone(&parser);
        let done = Arc::clone(&reader_done);
        let reader = thread::spawn(move || {
            let mut buf = [0u8; 1_024];
            loop {
                match reader_half.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        sink.lock().expect("output").extend_from_slice(&buf[..n]);
                        screen.lock().expect("parser").process(&buf[..n]);
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
        });
        Some(Self {
            child,
            writer: Some(writer),
            output,
            parser,
            reader_done,
            reader: Some(reader),
        })
    }

    /// Block until the screen has a drawn cell, the child has exited, or `timeout` elapses.
    fn wait_for_frame(&mut self, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            let drawn = self
                .parser
                .lock()
                .expect("parser")
                .screen()
                .contents()
                .chars()
                .any(|ch| !ch.is_whitespace());
            if drawn {
                return;
            }
            let exited = self.child.try_wait().ok().flatten().is_some();
            if exited && self.reader_done.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn write(&mut self, bytes: &[u8]) -> bool {
        let Some(writer) = self.writer.as_mut() else {
            return false;
        };
        writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .is_ok()
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
        let raw = String::from_utf8_lossy(&self.output.lock().expect("output")).into_owned();
        let parsed = self.parser.lock().expect("parser").screen().contents();
        if raw.contains("PASTE-EVENT") || raw.contains("SUBMITTED") || raw.contains("INTERRUPT") {
            raw
        } else {
            format!("{raw}{parsed}")
        }
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

fn demo_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("test binary");
    let deps = exe.parent().expect("deps directory");
    let profile = deps.parent().expect("profile directory");
    let binary = profile.join("examples").join("composer_demo");
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
                "composer_demo",
                "--target-dir",
            ]);
            command.arg(target_dir);
            if !cfg!(debug_assertions) {
                command.arg("--release");
            }
            let status = command.status().expect("build composer_demo");
            assert!(status.success(), "composer_demo failed to build");
        });
    }
    binary
}
