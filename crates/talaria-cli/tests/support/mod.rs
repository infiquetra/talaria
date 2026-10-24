//! Process helpers for the binary tests. A child that does not exit is killed, and so is
//! anything it started.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, SlavePty, native_pty_system};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn search_path(prefix: Option<&Path>) -> String {
    match prefix {
        Some(dir) => format!("{}:/usr/bin:/bin", dir.display()),
        None => "/usr/bin:/bin".to_string(),
    }
}

/// Run the binary once before the first timed launch. Cargo links a new file for the binary on
/// every run, and the operating system checks a new file when it first runs, which can add
/// several hundred milliseconds to that one launch; the time bound is for the program, not
/// for that check.
fn warm_up() {
    static WARM: Once = Once::new();
    WARM.call_once(|| {
        let _ = Command::new(env!("CARGO_BIN_EXE_talaria"))
            .arg("--version")
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

/// The binary with no terminal: standard input is null, output and error are pipes.
pub fn talaria(args: &[&str], home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .args(args)
        .current_dir(home)
        .env_clear()
        .env("PATH", search_path(None))
        .env("HOME", home)
        .env("HERMES_HOME", home)
        .env("TMPDIR", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command
}

pub fn captured(mut command: Command, limit: Duration) -> Output {
    let mut child = command.spawn().expect("spawn talaria");
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = stderr.read_to_end(&mut err);
        (out, err)
    });
    let status = wait_child(&mut child, limit);
    let (out, err) = reader.join().unwrap_or_default();
    Output {
        status,
        stdout: out,
        stderr: err,
    }
}

fn wait_child(child: &mut std::process::Child, limit: Duration) -> std::process::ExitStatus {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            return child.wait().expect("wait after kill");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What a finished pseudo-terminal run left behind.
pub struct Screen {
    pub text: String,
    pub exit_code: Option<u32>,
    pub signal: Option<String>,
    pub termios: String,
}

/// How a run is set up. The binary gets a clean environment: `HERMES_HOME` and `HOME` are the
/// given folder, and `PATH` is the system path behind an optional folder of stand-in programs.
pub struct Launch {
    args: Vec<String>,
    home: PathBuf,
    path_prefix: Option<PathBuf>,
    rows: u16,
    cols: u16,
    script: Option<String>,
}

impl Launch {
    pub fn new(home: &Path) -> Self {
        Self {
            args: Vec::new(),
            home: home.to_path_buf(),
            path_prefix: None,
            rows: 30,
            cols: 100,
            script: None,
        }
    }

    pub fn args(mut self, args: &[&str]) -> Self {
        self.args = args.iter().map(|arg| (*arg).to_string()).collect();
        self
    }

    /// Put this folder of stand-in programs first on `PATH`.
    pub fn path_prefix(mut self, dir: &Path) -> Self {
        self.path_prefix = Some(dir.to_path_buf());
        self
    }

    pub fn size(mut self, rows: u16, cols: u16) -> Self {
        self.rows = rows;
        self.cols = cols;
        self
    }

    /// Run `sh -c <script>` instead of the binary. The script names the binary itself.
    pub fn script(mut self, script: &str) -> Self {
        self.script = Some(script.to_string());
        self
    }

    pub fn spawn(self) -> Terminal {
        warm_up();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: self.rows,
                cols: self.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("pty");
        let mut command = match &self.script {
            Some(script) => {
                let mut command = CommandBuilder::new("/bin/sh");
                command.arg("-c");
                command.arg(script);
                command
            }
            None => {
                let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_talaria"));
                for arg in &self.args {
                    command.arg(arg);
                }
                command
            }
        };
        command.cwd(&self.home);
        command.env_clear();
        command.env("PATH", search_path(self.path_prefix.as_deref()));
        command.env("HOME", &self.home);
        command.env("HERMES_HOME", &self.home);
        command.env("TMPDIR", &self.home);
        if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        let spawned = Instant::now();
        let child = pair.slave.spawn_command(command).expect("spawn in pty");
        let mut reader = pair.master.try_clone_reader().expect("reader");
        let writer = pair.master.take_writer().expect("writer");
        let parser = Arc::new(Mutex::new(vt100::Parser::new(self.rows, self.cols, 200)));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let first_screen = Arc::new(Mutex::new(None));
        {
            let parser = Arc::clone(&parser);
            let raw = Arc::clone(&raw);
            let first_screen = Arc::clone(&first_screen);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                loop {
                    let count = match reader.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => count,
                    };
                    let read_at = Instant::now();
                    lock(&raw).extend_from_slice(&chunk[..count]);
                    let mut parser = lock(&parser);
                    parser.process(&chunk[..count]);
                    let mut first = lock(&first_screen);
                    if first.is_none() && !parser.screen().contents().trim().is_empty() {
                        *first = Some(read_at);
                    }
                }
            });
        }
        Terminal {
            pid: child.process_id(),
            child,
            master: pair.master,
            _slave: pair.slave,
            writer,
            parser,
            raw,
            first_screen,
            spawned,
        }
    }
}

/// A running binary inside a pseudo-terminal, with a `vt100` screen that follows its output.
pub struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    _slave: Box<dyn SlavePty + Send>,
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
    first_screen: Arc<Mutex<Option<Instant>>>,
    spawned: Instant,
    pid: Option<u32>,
}

/// How the child ended.
pub struct Exit {
    pub code: Option<u32>,
    pub signal: Option<String>,
    pub timed_out: bool,
}

impl Terminal {
    /// The process id of what was spawned: the binary, or the shell when a script runs.
    pub fn pid(&self) -> u32 {
        self.pid.expect("the child has a process id")
    }

    /// When the harness spawned the child.
    pub fn spawned_at(&self) -> Instant {
        self.spawned
    }

    /// The screen as text, lines trimmed on the right.
    pub fn screen_text(&self) -> String {
        lock(&self.parser).screen().contents()
    }

    /// Every byte the child has written so far.
    pub fn raw_output(&self) -> Vec<u8> {
        lock(&self.raw).clone()
    }

    /// When the first read that left the screen non-empty returned.
    pub fn first_screen_at(&self) -> Option<Instant> {
        *lock(&self.first_screen)
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Wait for the first non-empty screen, the child's exit, or the limit. Returns the time
    /// from spawn to that screen.
    pub fn wait_first_frame(&mut self, limit: Duration) -> Option<Duration> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(at) = self.first_screen_at() {
                return Some(at.saturating_duration_since(self.spawned));
            }
            if self.exited() || Instant::now() >= deadline {
                return self
                    .first_screen_at()
                    .map(|at| at.saturating_duration_since(self.spawned));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Wait until the screen shows this text. False when the limit passes or the child exits
    /// first and the text is not there.
    pub fn wait_for_text(&mut self, needle: &str, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if self.screen_text().contains(needle) {
                return true;
            }
            if self.exited() || Instant::now() >= deadline {
                return self.screen_text().contains(needle);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Write keys to the terminal, as typed.
    pub fn type_keys(&mut self, keys: &str) {
        let _ = self.writer.write_all(keys.as_bytes());
        let _ = self.writer.flush();
    }

    /// Wait for the child to exit; kill it if the limit passes first.
    pub fn wait_exit(&mut self, limit: Duration) -> Exit {
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return Exit {
                    code: Some(status.exit_code()),
                    signal: status.signal().map(str::to_string),
                    timed_out: false,
                };
            }
            if started.elapsed() > limit {
                self.kill_all();
                let status = self.child.wait().ok();
                return Exit {
                    code: status.as_ref().map(portable_pty::ExitStatus::exit_code),
                    signal: status.and_then(|state| state.signal().map(str::to_string)),
                    timed_out: true,
                };
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Wait for the exit, let the last output arrive, and read the screen and the terminal
    /// flags.
    pub fn finish(mut self, limit: Duration) -> Screen {
        let exit = self.wait_exit(limit);
        self.settle();
        Screen {
            text: self.screen_text(),
            exit_code: exit.code,
            signal: exit.signal,
            termios: self.termios(),
        }
    }

    /// Wait until no output has arrived for a short while.
    pub fn settle(&self) {
        let mut last = self.raw_output().len();
        let mut quiet_since = Instant::now();
        let started = Instant::now();
        while quiet_since.elapsed() < Duration::from_millis(120)
            && started.elapsed() < Duration::from_secs(2)
        {
            std::thread::sleep(Duration::from_millis(10));
            let now = self.raw_output().len();
            if now != last {
                last = now;
                quiet_since = Instant::now();
            }
        }
    }

    /// The pseudo-terminal's flags as text, or empty when they cannot be read.
    pub fn termios(&self) -> String {
        self.master
            .get_termios()
            .map(|flags| format!("{flags:?}"))
            .unwrap_or_default()
    }

    /// The processes started by `parent`, directly.
    pub fn children_of(&self, parent: u32) -> Vec<u32> {
        process_table()
            .into_iter()
            .filter(|(_, ppid)| *ppid == parent)
            .map(|(pid, _)| pid)
            .collect()
    }

    fn kill_all(&mut self) {
        if let Some(pid) = self.pid {
            for descendant in descendants(pid) {
                signal(descendant, "-KILL");
            }
        }
        let _ = self.child.kill();
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.kill_all();
            let _ = self.child.wait();
        } else if let Some(pid) = self.pid {
            for descendant in descendants(pid) {
                signal(descendant, "-KILL");
            }
        }
    }
}

fn process_table() -> Vec<(u32, u32)> {
    let Ok(output) = Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "ppid="])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse().ok()?;
            let ppid = parts.next()?.parse().ok()?;
            Some((pid, ppid))
        })
        .collect()
}

fn descendants(root: u32) -> Vec<u32> {
    let table = process_table();
    let mut found = Vec::new();
    let mut frontier = vec![root];
    while let Some(parent) = frontier.pop() {
        for (pid, ppid) in &table {
            if *ppid == parent && !found.contains(pid) {
                found.push(*pid);
                frontier.push(*pid);
            }
        }
    }
    found
}

/// The names of the flags set in the `Debug` text of a pseudo-terminal's `local_flags`.
pub fn local_flags(termios: &str) -> Vec<String> {
    let Some(start) = termios.find("local_flags") else {
        return Vec::new();
    };
    let rest = &termios[start..];
    let end = rest.find(')').unwrap_or(rest.len());
    let section = &rest[..end];
    let open = section.find('(').map_or(0, |index| index + 1);
    section[open..]
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// True when the flags show the terminal in its normal mode: line editing and echo on.
pub fn cooked_mode(termios: &str) -> bool {
    let flags = local_flags(termios);
    flags.iter().any(|name| name == "ICANON") && flags.iter().any(|name| name == "ECHO")
}

/// Run the binary in a pseudo-terminal, wait for its first frame (or its exit, or two seconds),
/// then type the keys, and wait for the exit.
pub fn pty_run(
    args: &[&str],
    home: &Path,
    rows: u16,
    cols: u16,
    keys: &str,
    limit: Duration,
) -> Screen {
    let mut terminal = Launch::new(home).args(args).size(rows, cols).spawn();
    terminal.wait_first_frame(Duration::from_secs(2).min(limit));
    if !keys.is_empty() && !terminal.exited() {
        terminal.type_keys(keys);
    }
    terminal.finish(limit)
}

/// Poll for a file to appear and keep the first time it was seen.
pub struct FileWatch {
    seen: Arc<Mutex<Option<Instant>>>,
    stop: Arc<AtomicBool>,
}

impl FileWatch {
    pub fn start(path: &Path) -> Self {
        let seen = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let path = path.to_path_buf();
        {
            let seen = Arc::clone(&seen);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if path.exists() {
                        *lock(&seen) = Some(Instant::now());
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
        }
        Self { seen, stop }
    }

    /// When the file was first seen, which is never before it was written.
    pub fn seen_at(&self) -> Option<Instant> {
        *lock(&self.seen)
    }
}

impl Drop for FileWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn signal(pid: u32, name: &str) {
    let _ = Command::new("/bin/kill")
        .args([name, &pid.to_string()])
        .status();
}
