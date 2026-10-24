//! Real terminal check: run the demo inside tmux, resize the window twice while it commits,
//! and read the whole scrollback back. Every committed line must appear once, in order, and no
//! live region row may be left in the history.
//!
//! Ignored by default because it needs tmux and builds the demo. Run it with
//! `cargo test -p talaria-tui --test tmux_scrollback -- --ignored --nocapture`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

const TOTAL_LINES: u32 = 200;
const WAIT_LIMIT: Duration = Duration::from_secs(30);
const POLL_STEP: Duration = Duration::from_millis(50);

/// Everything one tmux server needs: a private socket and one session, both named for this
/// process so parallel runs and the operator's own sessions never collide.
struct Tmux {
    name: String,
}

impl Tmux {
    fn new() -> Self {
        Self {
            name: format!("p106-{}", std::process::id()),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new("tmux");
        command.env_remove("TMUX").args(["-L", &self.name]);
        command
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|error| panic!("cannot run tmux {args:?}: {error}"));
        assert!(
            output.status.success(),
            "tmux {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn capture(&self) -> String {
        self.run(&["capture-pane", "-p", "-S", "-", "-t", &self.name])
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        let _ = self
            .command()
            .args(["kill-session", "-t", &self.name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn tmux_is_installed() -> bool {
    Command::new("tmux")
        .arg("-V")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn tag(number: u32) -> String {
    format!("line {number:04}")
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < WAIT_LIMIT {
        if done() {
            return true;
        }
        sleep(POLL_STEP);
    }
    done()
}

fn has_row(capture: &str, wanted: &str) -> bool {
    capture.lines().any(|row| row.trim_end() == wanted)
}

/// Build the demo for the profile this test runs under and return the directory holding it.
fn build_demo() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let release = !cfg!(debug_assertions);
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(cargo);
    build
        .current_dir(&manifest)
        .args(["build", "-p", "talaria-tui", "--example", "inline_demo"]);
    if release {
        build.arg("--release");
    }
    let status = build.status().expect("run cargo build for the demo");
    assert!(status.success(), "building the inline_demo example failed");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../target"));
    let directory = target
        .join(if release { "release" } else { "debug" })
        .join("examples");
    assert!(
        directory.join("inline_demo").is_file(),
        "the built demo was not found"
    );
    directory
}

/// Problems with the captured history: missing, repeated or misordered tags, or live rows.
fn history_problems(capture: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut previous: Option<usize> = None;
    for number in 1..=TOTAL_LINES {
        let wanted = tag(number);
        let positions: Vec<usize> = capture
            .lines()
            .enumerate()
            .filter(|(_, row)| row.trim_end() == wanted)
            .map(|(index, _)| index)
            .collect();
        match positions.as_slice() {
            [] => problems.push(format!("{wanted} is missing")),
            [position] => {
                if previous.is_some_and(|earlier| *position < earlier) {
                    problems.push(format!("{wanted} is out of order"));
                }
                previous = Some(*position);
            }
            many => problems.push(format!("{wanted} appears {} times", many.len())),
        }
    }
    if capture.contains("LIVE#") {
        problems.push("a live region row reached the history".to_string());
    }
    problems
}

fn summarize(problems: &[String]) -> String {
    let shown: Vec<&str> = problems.iter().take(15).map(String::as_str).collect();
    format!(
        "{} problems, first ones: {}",
        problems.len(),
        shown.join("; ")
    )
}

#[test]
#[ignore = "needs tmux and builds the demo; run with --ignored"]
fn tmux_capture_shows_each_line_once() {
    if !tmux_is_installed() {
        println!("TMUX_SCROLLBACK_RESULT: SKIPPED (tmux not found on PATH)");
        return;
    }
    let examples = build_demo();
    let tmux = Tmux::new();
    let directory = examples.to_string_lossy().into_owned();
    tmux.run(&[
        "new-session",
        "-d",
        "-s",
        &tmux.name,
        "-x",
        "100",
        "-y",
        "30",
        "-c",
        &directory,
        "bash --noprofile --norc",
    ]);
    tmux.run(&[
        "send-keys",
        "-t",
        &tmux.name,
        "./inline_demo --lines 200 --interval-ms 20",
        "Enter",
    ]);

    let reached = |number: u32| wait_until(|| has_row(&tmux.capture(), &tag(number)));
    let first_wait = reached(100);
    if first_wait {
        tmux.run(&["resize-window", "-t", &tmux.name, "-x", "80", "-y", "20"]);
        sleep(Duration::from_millis(150));
        tmux.run(&["resize-window", "-t", &tmux.name, "-x", "100", "-y", "40"]);
    }
    let last_wait = first_wait && reached(TOTAL_LINES);
    if last_wait {
        tmux.run(&["send-keys", "-t", &tmux.name, "q"]);
        let exited = wait_until(|| {
            tmux.run(&[
                "display-message",
                "-p",
                "-t",
                &tmux.name,
                "#{pane_current_command}",
            ])
            .trim()
                == "bash"
        });
        assert!(exited, "the demo did not exit after q");
    }

    let capture = tmux.capture();
    let problems = history_problems(&capture);
    let tail: Vec<&str> = capture
        .lines()
        .filter(|row| !row.trim().is_empty())
        .rev()
        .take(8)
        .collect();
    assert!(
        last_wait && problems.is_empty(),
        "reached line 0100: {first_wait}, reached line 0200: {last_wait}; {}; last rows (newest \
         first): {tail:?}",
        summarize(&problems)
    );
    println!("TMUX_SCROLLBACK_RESULT: PASSED");
}
