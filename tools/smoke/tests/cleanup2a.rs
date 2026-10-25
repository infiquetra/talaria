//! The tab and the tmux session close on every path: pass, failure, panic and signal.
//!
//! The signal tests go to the real `smoke` binary against the fake multiplexer and the fake
//! tmux; the rest run in process. All fail until the stub `run_smoke_2a` is replaced.

mod support;

use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use support::scripts2a::{
    Exec2a, PasteCollapse, Shapes, Step, assert_fails_with, assert_one_close_tab, exec_2a,
    script_2a,
};
use support::tmux_fake::TmuxFake;
use support::{Call, Harness, PANIC_MESSAGE};

fn explain(outcome: &support::Outcome) -> String {
    format!(
        "exit code {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
}

/// The run's leftover folders (`talaria-smoke-*`) under its temporary root.
fn residue(exec: &Exec2a) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(&exec.root) else {
        return Vec::new();
    };
    read.flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("talaria-smoke-"))
        })
        .collect()
}

/// True once the process is gone or only waiting to be collected.
fn has_ended(pid: u32) -> bool {
    match Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => {
            let stat = String::from_utf8_lossy(&output.stdout);
            let stat = stat.trim();
            stat.is_empty() || stat.starts_with('Z')
        }
        Err(_) => false,
    }
}

/// Starts `smoke --suite phase2a` against the fakes on a script that runs up to the first
/// paste, waits (in real time) until the paste is loaded, sends the signal and returns what
/// the process did.
fn run_and_signal(signal: &str) -> (Harness, TmuxFake, support::Outcome) {
    let shapes = Shapes {
        checklist_end_polls: Some(40),
        ..Default::default()
    };
    let script = script_2a(&shapes, Step::PasteCollapse);
    let harness = Harness::new(&script);
    let tmux = TmuxFake::new();
    let mut running = harness
        .cmd()
        .arg("--suite")
        .arg("phase2a")
        .arg("--tmux")
        .arg(&tmux.wrapper)
        .spawn();
    let pid = running.pid();
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut pasted = false;
    while Instant::now() < deadline {
        if tmux.saw(&["load-buffer"]) {
            pasted = true;
            break;
        }
        if has_ended(pid) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    if !pasted {
        running.kill();
        let outcome = running.finish();
        panic!(
            "smoke never loaded a paste buffer (it ended or hung before the tmux section); tmux saw {:?}\n{}",
            tmux.log(),
            explain(&outcome)
        );
    }
    running.signal(signal);
    let outcome = running.finish();
    (harness, tmux, outcome)
}

#[test]
fn tab_and_session_closed_after_pass() {
    let script = script_2a(&Shapes::default(), Step::TmuxQuit);
    let exec = exec_2a(&script, |_| {}, |term| term, |_| {});
    assert!(
        exec.report
            .result_line()
            .starts_with("SMOKE_RESULT: PASSED"),
        "the run passes: {}",
        exec.report.result_line()
    );
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");
}

#[test]
fn tab_and_session_closed_after_a_failing_step() {
    let shapes = Shapes {
        paste_collapse: PasteCollapse::NoLabel,
        ..Default::default()
    };
    let script = script_2a(&shapes, Step::PasteCollapse);
    let exec = exec_2a(&script, |_| {}, |term| term, |_| {});
    assert_fails_with(
        &exec,
        "paste-collapse",
        "the pasted text did not collapse into '[Pasted text #1 +120 lines]' within 5 s",
    );
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");
}

#[test]
fn tab_and_session_closed_after_a_panic() {
    let script = script_2a(&Shapes::default(), Step::PasteEditor);
    let exec = exec_2a(
        &script,
        |_| {},
        |term| {
            term.panic_when(
                |call| matches!(call, Call::SendText { text, .. } if text.contains("of the paste")),
            )
        },
        |_| {},
    );
    assert_fails_with(
        &exec,
        "paste-editor",
        &format!("internal error: {PANIC_MESSAGE}"),
    );
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");
}

#[test]
fn sigint_closes_tab_and_session() {
    let (harness, tmux, outcome) = run_and_signal("INT");
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        harness.fake.saw(&["tab", "close"]),
        "the tab is closed; log {:?}",
        harness.fake.log()
    );
    assert!(
        tmux.saw(&["kill-session"]),
        "the session is killed; log {:?}",
        tmux.log()
    );
    assert!(
        outcome
            .stdout
            .contains("SMOKE_RESULT: FAILED (interrupted)"),
        "the result line names the interrupt; {}",
        explain(&outcome)
    );
}

#[test]
fn sigterm_closes_tab_and_session() {
    let (harness, tmux, outcome) = run_and_signal("TERM");
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        harness.fake.saw(&["tab", "close"]),
        "the tab is closed; log {:?}",
        harness.fake.log()
    );
    assert!(
        tmux.saw(&["kill-session"]),
        "the session is killed; log {:?}",
        tmux.log()
    );
    assert!(
        outcome
            .stdout
            .contains("SMOKE_RESULT: FAILED (interrupted)"),
        "the result line names the interrupt; {}",
        explain(&outcome)
    );
}

#[test]
fn an_unkillable_session_ends_with_cleanup_failed() {
    let script = script_2a(&Shapes::default(), Step::TmuxQuit);
    let exec = exec_2a(
        &script,
        |_| {},
        |term| term,
        |paster| paster.fail_kill_with("permission denied"),
    );
    // The binary also prints `cleanup: tmux session <name> was not closed`; in process the
    // report carries the failure.
    assert_eq!(exec.report.result_line(), "SMOKE_RESULT: FAILED (cleanup)");
    assert_eq!(exec.report.exit_code(), 1);
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "the kill was attempted");
}

#[test]
fn scratch_and_drop_folders_removed_on_every_path() {
    let script = script_2a(&Shapes::default(), Step::TmuxQuit);
    let exec = exec_2a(&script, |_| {}, |term| term, |_| {});
    assert!(
        residue(&exec).is_empty(),
        "nothing is left after a pass: {:?}",
        residue(&exec)
    );

    let shapes = Shapes {
        paste_collapse: PasteCollapse::NoLabel,
        ..Default::default()
    };
    let script = script_2a(&shapes, Step::PasteCollapse);
    let exec = exec_2a(&script, |_| {}, |term| term, |_| {});
    assert!(
        residue(&exec).is_empty(),
        "nothing is left after a failure: {:?}",
        residue(&exec)
    );

    let script = script_2a(&Shapes::default(), Step::PasteEditor);
    let exec = exec_2a(
        &script,
        |_| {},
        |term| {
            term.panic_when(
                |call| matches!(call, Call::SendText { text, .. } if text.contains("of the paste")),
            )
        },
        |_| {},
    );
    assert!(
        residue(&exec).is_empty(),
        "nothing is left after a panic: {:?}",
        residue(&exec)
    );
}
