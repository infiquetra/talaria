//! Run a program to completion in a pseudo-terminal.

use std::path::Path;

use crate::error::BenchError;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(crate) struct Process {
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub writer: Box<dyn Write + Send>,
    pub output: mpsc::Receiver<(Instant, Vec<u8>)>,
    pub started: Instant,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    _home: tempfile::TempDir,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Process {
    pub fn spawn(
        binary: &Path,
        args: &[String],
        env: &[(String, String)],
        size: (u16, u16),
    ) -> Result<Self, BenchError> {
        if cfg!(windows) {
            return Err(BenchError::UnsupportedOs);
        }
        let home = tempfile::tempdir().map_err(|_| BenchError::Io("temporary home".into()))?;
        let pair = portable_pty::native_pty_system()
            .openpty(portable_pty::PtySize {
                rows: size.1,
                cols: size.0,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|_| BenchError::PtyUnavailable("open".into()))?;
        let mut command = portable_pty::CommandBuilder::new(binary);
        command.args(args);
        command.env_clear();
        for (key, value) in [
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("LANG", "C.UTF-8"),
        ] {
            command.env(key, value);
        }
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        command.env("HOME", home.path());
        for (key, value) in env {
            if !key.starts_with("HERMES") && key != "HOME" {
                command.env(key, value);
            }
        }
        // Acquire fallible handles before spawning, so every spawned child has a guard.
        let writer = pair
            .master
            .take_writer()
            .map_err(|_| BenchError::PtyUnavailable("writer".into()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|_| BenchError::PtyUnavailable("reader".into()))?;
        let started = Instant::now();
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|_| BenchError::PtyUnavailable("spawn".into()))?;
        drop(pair.slave);
        let (tx, output) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut bytes = [0; 16384];
            while let Ok(n) = reader.read(&mut bytes) {
                if n == 0 || tx.send((Instant::now(), bytes[..n].to_vec())).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            writer,
            output,
            started,
            _master: pair.master,
            _home: home,
            reader: Some(reader),
        })
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[derive(Debug)]
pub struct PtyRun {
    pub output: Vec<u8>,
    pub exit_code: Option<i32>,
}

pub fn run_in_pty(
    binary: &Path,
    args: &[String],
    env: &[(String, String)],
    size: (u16, u16),
) -> Result<PtyRun, BenchError> {
    let mut process = Process::spawn(binary, args, env, size)?;
    let mut output = Vec::new();
    let mut emulator = crate::Emulator::new(size.1, size.0);
    let mut responder = crate::ResponderState::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok((_, chunk)) = process.output.recv_timeout(Duration::from_millis(10)) {
            let answers = crate::answer_queries(&mut responder, &chunk, &mut emulator);
            let _ = process.writer.write_all(&answers.reply);
            output.extend(chunk);
        }
        if let Ok(Some(status)) = process.child.try_wait() {
            while let Ok((_, chunk)) = process.output.recv_timeout(Duration::from_millis(250)) {
                output.extend(chunk);
            }
            return Ok(PtyRun {
                output,
                exit_code: Some(status.exit_code() as i32),
            });
        }
        if Instant::now() >= deadline {
            return Ok(PtyRun {
                output,
                exit_code: None,
            });
        }
    }
}
