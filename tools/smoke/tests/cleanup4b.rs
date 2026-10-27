//! Cleanup after the browse run, including interruption.
#[path = "support/scripts4b.rs"]
mod scripts4b;
mod support;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use scripts4b::{exec_4b, script_4b};
use smoke::suite4b::{Phase4bConfig, run_smoke_4b};
use smoke::{
    ManualClock, NewTab, PaneId, ReadSource, SmokeConfig, StepOutcome, TabId, TermError, Terminal,
};
use support::{FakeTerminal, ScriptBuilder};

fn assert_no_artifacts(scratch: &Path, empty_home_parent: &Path) {
    assert!(!scratch.exists(), "scratch folder was removed");
    assert_eq!(
        std::fs::read_dir(empty_home_parent)
            .expect("empty-home parent")
            .count(),
        0,
        "empty-home parent is empty"
    );
}

#[test]
fn tab_closed_after_pass() {
    let run = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    assert_eq!(run.report.exit_code(), 0, "{}", run.report.step_lines());
    assert_eq!(run.term.calls_of("tab close").len(), 1);
    assert_no_artifacts(&run.report.scratch, &run.empty_home_parent);
}

#[test]
fn tab_closed_after_fail() {
    let script = script_4b(&[("mcp-browse-r1.txt", "MCP servers · profile fixture", "")]);
    let run = exec_4b(&script, false, false, 1.0, 900);
    assert!(matches!(
        run.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "mcp-browse",
            ..
        })
    ));
    assert_eq!(run.term.calls_of("tab close").len(), 1);
    assert_no_artifacts(&run.report.scratch, &run.empty_home_parent);
}

/// On the first read after a tab was opened, simulate the signal handler setting its flag.
struct SignalOnRead<'a> {
    inner: FakeTerminal<'a>,
    signal: &'a AtomicBool,
}
impl Terminal for SignalOnRead<'_> {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError> {
        self.inner.create_tab(label, cwd)
    }
    fn split_pane(&self, pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError> {
        self.inner.split_pane(pane, cwd)
    }
    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        self.inner.run(pane, text)
    }
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        self.inner.send_text(pane, text)
    }
    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError> {
        self.inner.send_keys(pane, keys)
    }
    fn read(
        &self,
        pane: &PaneId,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError> {
        let result = self.inner.read(pane, source, lines);
        self.signal.store(true, Ordering::SeqCst);
        result
    }
    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError> {
        self.inner.foreground(pane)
    }
    fn pane_get(&self, pane: &PaneId) -> Result<String, TermError> {
        self.inner.pane_get(pane)
    }
    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError> {
        self.inner.tab_exists(tab)
    }
    fn close_tab(&self, tab: &TabId) -> Result<(), TermError> {
        self.inner.close_tab(tab)
    }
    fn version(&self) -> String {
        self.inner.version()
    }
}

#[test]
fn tab_closed_on_signal() {
    let temp = tempfile::tempdir().expect("test root");
    let empty_home_parent = temp.path().join("empty-homes");
    std::fs::create_dir(&empty_home_parent).unwrap();
    let clock = ManualClock::new();
    let signal = AtomicBool::new(false);
    let script = ScriptBuilder::new();
    let terminal = SignalOnRead {
        inner: FakeTerminal::from_builder(&script, &clock),
        signal: &signal,
    };
    let cfg = SmokeConfig {
        talaria: temp.path().join("talaria"),
        profile: "fixture".into(),
        model: "minimax-m3.1-flash".into(),
        provider: "ollama-cloud".into(),
        deadline: Duration::from_secs(900),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: temp.path().to_path_buf(),
        seed: Some(11),
    };
    let cfg4 = Phase4bConfig {
        empty_home_parent: empty_home_parent.clone(),
    };
    let report = run_smoke_4b(&terminal, &clock, &cfg, &cfg4, &signal);
    assert!(
        signal.load(Ordering::SeqCst),
        "the signal arrived after the tab opened"
    );
    assert!(report.interrupted, "the run reports interruption");
    assert_eq!(terminal.inner.calls_of("tab close").len(), 1);
    assert_no_artifacts(&report.scratch, &empty_home_parent);

    // Exercise the installed signal handlers as well as the shared stop flag.
    for name in ["INT", "TERM"] {
        let harness = support::Harness::plain();
        let jq = harness.root().join("bin").join("jq");
        let slash = std::path::MAIN_SEPARATOR;
        support::write_executable(&jq, &format!("#!{slash}bin{slash}sh\nexit 0\n"));
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let search = std::env::join_paths(
            std::iter::once(harness.root().join("bin")).chain(std::env::split_paths(&inherited)),
        )
        .expect("executable search path");
        let mut process = harness
            .cmd()
            .args(["--suite", "phase4b"])
            .env("PATH", search)
            .spawn();
        if !harness
            .fake
            .wait_for_call(&["pane", "read"], Duration::from_secs(5))
        {
            process.kill();
            let outcome = process.finish();
            panic!("the suite ended before the signal: {}", outcome.stderr);
        }
        process.signal(name);
        let outcome = process.finish();
        assert_eq!(outcome.code, Some(1));
        assert!(
            outcome
                .stdout
                .contains("SMOKE_RESULT: FAILED (interrupted)")
        );
        assert!(harness.fake.saw(&["tab", "close"]));
        assert!(harness.scratch_dirs().is_empty());
        assert_eq!(std::fs::read_dir(harness.tmp_dir()).unwrap().count(), 0);
    }
}

#[test]
fn empty_home_removed_after_no_gateway_step() {
    let run = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    assert!(run.report.outcomes.iter().any(|outcome| matches!(
        outcome,
        StepOutcome::Pass {
            step: "no-gateway",
            ..
        }
    )));
    assert_no_artifacts(&run.report.scratch, &run.empty_home_parent);
}

#[test]
fn scratch_and_empty_home_removed_on_every_path() {
    for script in [script_4b(&[]), script_4b(&[("@home-not-empty", "", "")])] {
        let run = exec_4b(&script, false, false, 1.0, 900);
        assert!(run.report.outcomes.iter().any(|outcome| matches!(
            outcome,
            StepOutcome::Pass {
                step: "no-gateway",
                ..
            } | StepOutcome::Fail {
                step: "no-gateway",
                ..
            }
        )));
        assert_eq!(run.term.calls_of("tab close").len(), 1);
        assert_no_artifacts(&run.report.scratch, &run.empty_home_parent);
    }
}
