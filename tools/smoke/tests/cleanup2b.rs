//! The tab closes and the scratch is removed on every path: pass, failure, panic and
//! signal.
//!
//! The signal test goes to the real `smoke` binary against the fake multiplexer; the rest
//! run in process. All fail until the stub `run_smoke_2b` is replaced.

mod support;

use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use support::scratch2b::Scratch2b;
use support::scripts2b::{
    ContextEmpty, Shapes, Step, assert_fails_with, assert_one_close_tab, assert_passes, exec_2b,
    script_2b,
};
use support::{Call, FailBuilder, Harness, PANIC_MESSAGE};

fn explain(outcome: &support::Outcome) -> String {
    format!(
        "exit code {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
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

/// No exported Markdown file is left anywhere under the temporary root.
fn assert_no_export_left(root: &std::path::Path) {
    let mut left = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy() == "fixture-smoke-export.md")
            {
                left.push(path);
            }
        }
    }
    assert!(left.is_empty(), "the exported file was removed: {left:?}");
}

#[test]
fn tab_closed_after_pass() {
    let script = script_2b(&Shapes::default(), Step::Headless);
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_passes(&exec, "headless");
    assert_one_close_tab(&exec);
    Scratch2b::assert_removed(&exec.root);
}

#[test]
fn tab_closed_after_failure() {
    let shapes = Shapes {
        context_empty: ContextEmpty::ContextLineMissing,
        ..Default::default()
    };
    let script = script_2b(&shapes, Step::Headless);
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_fails_with(
        &exec,
        "context-empty",
        "/context did not show 'Context \u{00b7} not measured yet' within 5 s",
    );
    assert_one_close_tab(&exec);
    Scratch2b::assert_removed(&exec.root);
}

#[test]
fn tab_closed_after_signal() {
    let script = script_2b(&Shapes::default(), Step::Headless);
    let harness = Harness::new(&script);
    let mut running = harness.cmd().arg("--suite").arg("phase2b").spawn();
    let pid = running.pid();
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut exported: Option<PathBuf> = None;
    while Instant::now() < deadline {
        if let Some(scratch) = Scratch2b::find_scratch(&harness.tmp_dir()) {
            let candidate = scratch.join("a").join("fixture-smoke-export.md");
            if candidate.is_file() {
                exported = Some(candidate);
                break;
            }
        }
        if has_ended(pid) {
            break;
        }
        sleep(Duration::from_millis(100));
    }
    if exported.is_none() {
        running.kill();
        let outcome = running.finish();
        panic!(
            "smoke never exported (it ended or hung before rename-export)\n{}",
            explain(&outcome)
        );
    }
    running.signal("TERM");
    let outcome = running.finish();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        harness.fake.saw(&["tab", "close"]),
        "the tab is closed; log {:?}",
        harness.fake.log()
    );
    assert!(
        outcome
            .stdout
            .contains("SMOKE_RESULT: FAILED (interrupted)"),
        "the result line names the interrupt; {}",
        explain(&outcome)
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "the scratch was removed: {:?}",
        harness.scratch_dirs()
    );
}

#[test]
fn tab_closed_after_panic() {
    let script = script_2b(&Shapes::default(), Step::ModelPicker);
    let exec = exec_2b(
        &script,
        |_| {},
        |term| term.panic_when(|call| matches!(call, Call::Run { text, .. } if text == "/model")),
    );
    assert_fails_with(
        &exec,
        "model-picker",
        &format!("internal error: {PANIC_MESSAGE}"),
    );
    assert_one_close_tab(&exec);
    Scratch2b::assert_removed(&exec.root);
}

#[test]
fn unclosable_tab_gives_cleanup_failure() {
    let script = script_2b(&Shapes::default(), Step::Headless)
        .fail(FailBuilder::call("tab close", "tab_not_found").message("tab not found"));
    let exec = exec_2b(&script, |_| {}, |term| term);
    // The binary also prints `cleanup: tab <name> was not closed`; in process the report
    // carries the failure.
    assert_eq!(exec.report.result_line(), "SMOKE_RESULT: FAILED (cleanup)");
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1, "the close was attempted");
}

#[test]
fn scratch_removed_on_every_path() {
    let script = script_2b(&Shapes::default(), Step::Headless);
    let exec = exec_2b(&script, |_| {}, |term| term);
    Scratch2b::assert_removed(&exec.root);

    let shapes = Shapes {
        context_empty: ContextEmpty::ContextLineMissing,
        ..Default::default()
    };
    let script = script_2b(&shapes, Step::Headless);
    let exec = exec_2b(&script, |_| {}, |term| term);
    Scratch2b::assert_removed(&exec.root);

    let script = script_2b(&Shapes::default(), Step::ModelPicker);
    let exec = exec_2b(
        &script,
        |_| {},
        |term| term.panic_when(|call| matches!(call, Call::Run { text, .. } if text == "/model")),
    );
    Scratch2b::assert_removed(&exec.root);
}

#[test]
fn exported_file_is_removed_with_the_scratch() {
    let script = script_2b(&Shapes::default(), Step::Headless);
    let exec = exec_2b(&script, |_| {}, |term| term);
    assert_passes(&exec, "rename-export");
    Scratch2b::assert_removed(&exec.root);
    assert_no_export_left(&exec.root);
}
