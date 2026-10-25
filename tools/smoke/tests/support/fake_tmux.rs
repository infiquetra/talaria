//! A fake `tmux` command line for the phase 2a suite tests: it answers `-V`, the buffer
//! commands, `has-session` and `kill-session` from recorded output, keeps one state file for
//! killed sessions, treats a `SMOKE_EXIT:` marker file as a dead session, and logs its arguments.
//!
//! Environment: `FAKE_TMUX_DIR` holds the log (`log.txt`, one JSON array of arguments per line)
//! and the state (`sessions.txt`, one `killed <session>` line per ended session).
//! `FAKE_TMUX_TMPDIR` is the scratch root the suite runs in; a session whose
//! `<root>/<session>/tmux-exit.txt` holds a `SMOKE_EXIT:` line counts as gone, exactly as the
//! real server dropping an ended session. Recorded output lives in
//! `tests/fixtures/tmux/` and is compiled in.

use std::fs;
use std::path::{Path, PathBuf};

const VERSION: &str = include_str!("../fixtures/tmux/version.txt");

fn dir() -> PathBuf {
    std::env::var("FAKE_TMUX_DIR")
        .map(PathBuf::from)
        .expect("FAKE_TMUX_DIR must name the fake-tmux folder")
}

fn tmp_root() -> Option<PathBuf> {
    std::env::var("FAKE_TMUX_TMPDIR").map(PathBuf::from).ok()
}

fn log_call(dir: &Path, args: &[String]) {
    let line = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_owned());
    let log = fs::read_to_string(dir.join("log.txt")).unwrap_or_default();
    let _ = fs::write(dir.join("log.txt"), format!("{log}{line}\n"));
}

fn killed(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("sessions.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("killed "))
        .map(str::to_owned)
        .collect()
}

fn record_kill(dir: &Path, session: &str) {
    let mut gone = killed(dir);
    if !gone.iter().any(|s| s == session) {
        gone.push(session.to_owned());
    }
    let text = gone
        .iter()
        .map(|s| format!("killed {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = if text.is_empty() {
        text
    } else {
        format!("{text}\n")
    };
    let _ = fs::write(dir.join("sessions.txt"), text);
}

/// Whether the session ended on its own: its marker file holds a `SMOKE_EXIT:` line.
fn exited_by_itself(session: &str) -> bool {
    let Some(root) = tmp_root() else {
        return false;
    };
    let marker = root.join(session).join("tmux-exit.txt");
    fs::read_to_string(marker)
        .map(|text| text.lines().any(|line| line.contains("SMOKE_EXIT:")))
        .unwrap_or(false)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find_map(|pair| {
        if pair[0] == flag {
            Some(pair[1].clone())
        } else {
            None
        }
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = dir();
    log_call(&dir, &args);
    if args.first().is_some_and(|first| first == "-V") {
        print!("{VERSION}");
        return;
    }
    let command = args.first().cloned().unwrap_or_default();
    match command.as_str() {
        "load-buffer" => {
            // The suite always writes the file first; keep a copy and its mode so the
            // tests can check what was loaded after the suite removes the file.
            if let Some(file) = args.last() {
                if let Ok(text) = fs::read_to_string(file) {
                    let _ = fs::write(dir.join("buffer_copy.txt"), text);
                }
                if let Ok(meta) = fs::metadata(file) {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::write(
                        dir.join("buffer_mode.txt"),
                        format!("{:o}", meta.permissions().mode() & 0o777),
                    );
                }
            }
            std::process::exit(0);
        }
        "paste-buffer" => {
            let session = flag_value(&args, "-t").unwrap_or_default();
            if killed(&dir).iter().any(|s| s == &session) || exited_by_itself(&session) {
                eprintln!("can't find session: {session}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        "has-session" => {
            let session = flag_value(&args, "-t").unwrap_or_default();
            if session.is_empty() {
                eprintln!("missing session");
                std::process::exit(2);
            }
            if std::env::var("FAKE_TMUX_NEVER_UP").is_ok_and(|v| v == "1") {
                eprintln!("can't find session: {session}");
                std::process::exit(1);
            }
            if std::env::var("FAKE_TMUX_NO_SERVER").is_ok_and(|v| v == "1") {
                eprintln!("error connecting to <tmp> (No such file or directory)");
                std::process::exit(1);
            }
            if killed(&dir).iter().any(|s| s == &session) || exited_by_itself(&session) {
                eprintln!("can't find session: {session}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        "kill-session" => {
            let session = flag_value(&args, "-t").unwrap_or_default();
            if std::env::var("FAKE_TMUX_NO_SERVER").is_ok_and(|v| v == "1") {
                eprintln!("error connecting to <tmp> (No such file or directory)");
                std::process::exit(1);
            }
            if killed(&dir).iter().any(|s| s == &session) || exited_by_itself(&session) {
                // The real tmux says so too; the suite treats it as success.
                eprintln!("can't find session: {session}");
                std::process::exit(1);
            }
            record_kill(&dir, &session);
            std::process::exit(0);
        }
        _ => {
            eprintln!("fake_tmux: unknown command: {command}");
            std::process::exit(2);
        }
    }
}
