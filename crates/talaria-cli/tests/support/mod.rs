//! Process helpers for the binary tests. A child that does not exit is killed.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

pub fn talaria(args: &[&str], home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .args(args)
        .current_dir(home)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
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

pub struct Screen {
    pub text: String,
    pub exit_code: Option<u32>,
    pub signal: Option<String>,
    pub termios: String,
}

pub fn pty_run(
    args: &[&str],
    home: &Path,
    rows: u16,
    cols: u16,
    keys: &str,
    limit: Duration,
) -> Screen {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("pty");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_talaria"));
    for arg in args {
        command.arg(*arg);
    }
    command.cwd(home);
    command.env_clear();
    command.env("PATH", "/usr/bin:/bin");
    command.env("HOME", home);
    command.env("HERMES_HOME", home);
    command.env("TMPDIR", home);
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let slave = pair.slave;
    let master = pair.master;
    let mut child = slave.spawn_command(command).expect("spawn in pty");
    let mut reader = master.try_clone_reader().expect("reader");
    if !keys.is_empty()
        && let Ok(mut writer) = master.take_writer()
    {
        let _ = writer.write_all(keys.as_bytes());
        let _ = writer.flush();
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let status = wait_pty(&mut *child, limit);
    drop(slave);
    let bytes = receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    let mut parser = vt100::Parser::new(rows, cols, 100);
    parser.process(&bytes);
    let termios = master
        .get_termios()
        .map(|flags| format!("{flags:?}"))
        .unwrap_or_default();
    Screen {
        text: parser.screen().contents(),
        exit_code: status.as_ref().map(portable_pty::ExitStatus::exit_code),
        signal: status.and_then(|state| state.signal().map(str::to_string)),
        termios,
    }
}

fn wait_pty(child: &mut dyn Child, limit: Duration) -> Option<portable_pty::ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            return child.wait().ok();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn signal(pid: u32, name: &str) {
    let _ = Command::new("/bin/kill")
        .args([name, &pid.to_string()])
        .status();
}
