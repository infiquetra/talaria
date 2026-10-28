//! Tabs and scratch folders are removed on pass, failure, and interruption.
#[path = "support/scripts5.rs"]
mod scripts5;
mod support;

use scripts5::{exec_5, script_5};
use smoke::suite5::{Phase5Config, run_smoke_5};
use smoke::{
    ManualClock, NewTab, PaneId, ReadSource, SmokeConfig, StepOutcome, TabId, TermError, Terminal,
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use support::FakeTerminal;

#[test]
fn tab_closed_after_pass() {
    let exec = exec_5(&script_5(&[]), false, false, 1.0, 600);
    assert_eq!(exec.report.exit_code(), 0, "{}", exec.lines());
    assert_eq!(exec.close_tabs(), 1);
    assert!(!exec.report.scratch.exists());
    assert_eq!(std::fs::read_dir(&exec.root).unwrap().count(), 0);
}

#[test]
fn tab_closed_after_fail() {
    let exec = exec_5(&script_5(&["workflows-missing"]), false, false, 1.0, 600);
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "workflows-empty",
            ..
        })
    ));
    assert_eq!(exec.close_tabs(), 1);
    assert!(!exec.report.scratch.exists());
    assert_eq!(std::fs::read_dir(&exec.root).unwrap().count(), 0);
}

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
    let temp = tempfile::tempdir().unwrap();
    let clock = ManualClock::new();
    let signal = AtomicBool::new(false);
    let terminal = SignalOnRead {
        inner: FakeTerminal::from_builder(&script_5(&[]), &clock),
        signal: &signal,
    };
    let cfg = SmokeConfig {
        talaria: temp.path().join("talaria"),
        profile: "fixture".into(),
        model: "minimax-m3.1-flash".into(),
        provider: "ollama-cloud".into(),
        deadline: Duration::from_secs(600),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: temp.path().to_path_buf(),
        seed: Some(11),
    };
    let cfg5 = Phase5Config {
        profile: cfg.profile.clone(),
        model: cfg.model.clone(),
        provider: cfg.provider.clone(),
    };
    let report = run_smoke_5(&terminal, &clock, &cfg, &cfg5, &signal);
    assert!(signal.load(Ordering::SeqCst));
    assert!(report.interrupted);
    assert_eq!(terminal.inner.close_count(), 1);
    assert!(!report.scratch.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn scratch_removed_after_no_gateway_failure() {
    let exec = exec_5(&script_5(&["created-run"]), false, false, 1.0, 600);
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "no-gateway",
            ..
        })
    ));
    assert_eq!(exec.close_tabs(), 1);
    assert!(!exec.report.scratch.exists());
    assert_eq!(std::fs::read_dir(&exec.root).unwrap().count(), 0);
}

#[test]
fn cleanup_failure_is_not_reported_as_pass() {
    use support::FailBuilder;
    let script = script_5(&[])
        .fail(FailBuilder::call("tab close", "internal_error").message("close failed"));
    let exec = exec_5(&script, false, false, 1.0, 600);
    assert_eq!(exec.report.exit_code(), 1);
    assert!(exec.report.unclosed_tab.is_some());
    assert!(!exec.report.scratch.exists());
}

#[test]
fn installed_signal_handlers_close_the_tab() {
    for signal in ["INT", "TERM"] {
        let harness = support::Harness::plain();
        let bin = harness.root().join("bin");
        let slash = std::path::MAIN_SEPARATOR;
        support::write_executable(&bin.join("jq"), &format!("#!{slash}bin{slash}sh\nexit 0\n"));
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path =
            std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited)))
                .unwrap();
        let mut process = harness
            .cmd()
            .args(["--suite", "phase5"])
            .env("PATH", path)
            .spawn();
        if !harness
            .fake
            .wait_for_call(&["pane", "read"], Duration::from_secs(5))
        {
            process.kill();
            let out = process.finish();
            panic!("the suite ended before {signal}: {}", out.stderr);
        }
        process.signal(signal);
        let out = process.finish();
        assert_eq!(out.code, Some(1));
        assert!(out.stdout.contains("SMOKE_RESULT: FAILED (interrupted)"));
        assert!(harness.fake.saw(&["tab", "close"]));
        assert!(harness.scratch_dirs().is_empty());
        assert_eq!(std::fs::read_dir(harness.tmp_dir()).unwrap().count(), 0);
    }
}
