//! The composer demo with vim, driven through a pseudo-terminal.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

#[test]
fn demo_vim_session_in_a_real_terminal() {
    let writes = [
        vec![b'i'],
        vec![b'h'],
        vec![b'i'],
        vec![0x1b],
        vec![b'0'],
        vec![b'x'],
        vec![0x0d],
    ];
    let output = drive(
        &["--vim"],
        &writes,
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_secs(3),
    );
    assert!(
        output.contains("SUBMITTED lines=1 chars=1 text=\"i\""),
        "submitted line for i, got {output}"
    );
    assert!(
        output.contains("-- NORMAL --"),
        "mode string missing: {output}"
    );
}

fn drive(
    args: &[&str],
    writes: &[Vec<u8>],
    gap: Duration,
    settle: Duration,
    timeout: Duration,
) -> String {
    let mut demo = Demo::open(args);
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
    demo.collect(timeout)
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
    fn open(args: &[&str]) -> Self {
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
                panic!("SKIP: no pseudo-terminal ({error})");
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
        Self {
            child,
            writer: Some(writer),
            output,
            parser,
            reader_done,
            reader: Some(reader),
        }
    }

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
            let raw = String::from_utf8_lossy(&self.output.lock().expect("output")).into_owned();
            if raw.contains("SUBMITTED") || raw.contains("composer_demo:") {
                break;
            }
            let exited = self.child.try_wait().ok().flatten().is_some();
            if exited && self.reader_done.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        thread::sleep(Duration::from_millis(100));
        let raw = String::from_utf8_lossy(&self.output.lock().expect("output")).into_owned();
        let parsed = self.parser.lock().expect("parser").screen().contents();
        format!("{raw}\n{parsed}")
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
