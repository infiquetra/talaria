//! The tab closes and the scratch is removed on every path: pass, failure, panic and signal.
//!
//! The signal test goes to the real `smoke` binary against the fake multiplexer. With the
//! phase 3 name still refused, that process ends before any scratch marker, so the test
//! fails at once. The rest run in process and fail until the stub `run_smoke_3` is replaced.

mod support;

use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use support::scratch3::Scratch3;
use support::scripts3::{
    FleetQuiet, Shapes, Step, assert_fails_with, assert_one_close_tab, assert_passes, exec_3,
    script_3,
};
use support::{Call, FailBuilder, Harness, Outcome, PANIC_MESSAGE};

fn explain(outcome: &Outcome) -> String {
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

#[test]
fn tab_closed_after_pass() {
    let script = script_3(&Shapes::default(), Step::NeedsYou);
    let exec = exec_3(&script, |_| {}, |term| term);
    assert_passes(&exec, "needs-you");
    assert_one_close_tab(&exec);
    Scratch3::assert_removed(&exec.root);
}

#[test]
fn tab_closed_after_failure() {
    let shapes = Shapes {
        fleet_quiet: FleetQuiet::Exit(7),
        ..Default::default()
    };
    let script = script_3(&shapes, Step::FleetQuiet);
    let exec = exec_3(&script, |_| {}, |term| term);
    assert_fails_with(
        &exec,
        "fleet-quiet",
        "talaria fleet list exited with code 7 (is the Hermes gateway running for profile fixture?)",
    );
    assert_one_close_tab(&exec);
    Scratch3::assert_removed(&exec.root);
}

#[test]
fn tab_closed_after_panic() {
    let script = script_3(&Shapes::default(), Step::FleetQuiet);
    let exec = exec_3(
        &script,
        |_| {},
        |term| {
            term.panic_when(
                |call| matches!(call, Call::Run { text, .. } if text.contains("size.out")),
            )
        },
    );
    assert_fails_with(
        &exec,
        "fleet-quiet",
        &format!("internal error: {PANIC_MESSAGE}"),
    );
    assert_one_close_tab(&exec);
    Scratch3::assert_removed(&exec.root);
}

#[test]
fn tab_closed_after_signal() {
    let script = script_3(&Shapes::default(), Step::NeedsYou);
    let harness = Harness::new(&script);
    let mut running = harness.cmd().arg("--suite").arg("phase3").spawn();
    let pid = running.pid();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut marker = false;
    while Instant::now() < deadline {
        if let Some(scratch) = Scratch3::find_scratch(&harness.tmp_dir()) {
            let worktrees = scratch.join("a").join(".worktrees");
            let receipt = scratch.join("wt.out");
            if worktrees.is_dir() || receipt.is_file() {
                marker = true;
                break;
            }
        }
        if has_ended(pid) {
            break;
        }
        sleep(Duration::from_millis(50));
    }
    if !marker {
        running.kill();
        let outcome = running.finish();
        panic!(
            "smoke ended before the mid-run marker (the phase 3 suite is not running yet)\n{}",
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
fn scratch_removed_unless_kept() {
    let script = script_3(&Shapes::default(), Step::NeedsYou);
    let exec = exec_3(&script, |_| {}, |term| term);
    Scratch3::assert_removed(&exec.root);

    let shapes = Shapes {
        fleet_quiet: FleetQuiet::Exit(7),
        ..Default::default()
    };
    let script = script_3(&shapes, Step::FleetQuiet);
    let exec = exec_3(&script, |_| {}, |term| term);
    Scratch3::assert_removed(&exec.root);

    let kept = tempfile::tempdir().expect("report folder");
    let script = script_3(&Shapes::default(), Step::NeedsYou);
    let exec = exec_3(
        &script,
        |cfg| cfg.report_dir = Some(kept.path().to_path_buf()),
        |term| term,
    );
    assert!(
        Scratch3::find_scratch(&exec.root).is_some(),
        "a report directory keeps the scratch"
    );
}

#[test]
fn unclosed_tab_gives_failed_cleanup() {
    let script = script_3(&Shapes::default(), Step::NeedsYou)
        .fail(FailBuilder::call("tab close", "tab_not_found").message("tab not found"));
    let exec = exec_3(&script, |_| {}, |term| term);
    assert_eq!(exec.report.result_line(), "SMOKE_RESULT: FAILED (cleanup)");
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1, "the close was attempted");
}

#[test]
fn scratch_holds_every_file_and_worktree_the_suite_made() {
    let kept = tempfile::tempdir().expect("report folder");
    let script = script_3(&Shapes::default(), Step::NeedsYou);
    let exec = exec_3(
        &script,
        |cfg| cfg.report_dir = Some(kept.path().to_path_buf()),
        |term| term,
    );
    let scratch = Scratch3::find_scratch(&exec.root).expect("the scratch was kept");
    for name in ["origin.git", "a", "other", "empty-home"] {
        assert!(scratch.join(name).is_dir(), "{name} is in the scratch");
    }
    assert!(
        scratch.join("probe").join("talaria-smoke-probe").is_dir(),
        "the probe folder is in the scratch"
    );
    let trees = scratch.join("a").join(".worktrees");
    let count = std::fs::read_dir(&trees)
        .expect("the worktree folder")
        .count();
    assert!(count > 0, "the scratch holds the worktree the suite made");
}
