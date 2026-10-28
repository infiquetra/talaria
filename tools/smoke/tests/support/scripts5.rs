//! Synthetic workflow screen playback and captured reads.
#![allow(dead_code)]

use crate::support::{Call, FakeTerminal, RuleBuilder, ScriptBuilder, StageBuilder};
use smoke::suite5::{Phase5Config, run_smoke_5};
use smoke::{
    ManualClock, NewTab, PaneId, ReadSource, SmokeConfig, SmokeReport, StepOutcome, TabId,
    TermError, Terminal,
};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, atomic::AtomicBool},
    time::Duration,
};

pub const STEPS: [&str; 10] = [
    "startup",
    "plain-answer",
    "workflows-empty",
    "ultracode-status",
    "quit",
    "resume",
    "builtins-cli",
    "roster-cli",
    "list-cli",
    "no-gateway",
];
pub const LIMITS: [u64; 10] = [30, 60, 30, 30, 20, 90, 40, 40, 30, 40];

pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/phase5")
        .join(name);
    let text = std::fs::read_to_string(path).expect("synthetic fixture");
    text.strip_prefix("# synthetic\n")
        .or_else(|| text.strip_prefix("# recorded\n"))
        .unwrap_or(&text)
        .to_owned()
}

fn shell_rule(pattern: &str, output: &str, typed: bool) -> RuleBuilder {
    let rule = if typed {
        RuleBuilder::send_text(pattern)
    } else {
        RuleBuilder::run(pattern)
    };
    rule.append(&format!("{output}\nSMOKE_EXIT:0\n$ "))
}

/// A mode changes one observable state or command result, not the runner's report.
pub fn script_5(modes: &[&str]) -> ScriptBuilder {
    let has = |mode: &str| modes.contains(&mode);
    let idle = fixture("idle.txt");
    let mut workflow = fixture("workflows-empty.txt");
    let mut status = fixture("ultracode-status.txt");
    if has("workflows-missing") {
        workflow = workflow.replace(
            "no runs yet — talaria workflow run starts one",
            "no entries",
        );
    }
    if has("status-missing") {
        status = status.replace(
            "Ultracode is off for this session. Skill: not found in profile fixture.",
            "status unavailable",
        );
    }
    if has("turn-started") {
        status.push_str("∴ thinking\n");
    }
    if has("toggle-on") {
        status.push_str("model │ ultracode │ approvals: manual\n");
    }

    let start = if has("startup-exit-1") {
        RuleBuilder::run(r"--no-ensure --profile .*--provider")
            .foreground(&[])
            .append("SMOKE_EXIT:1\n$ ")
    } else if has("startup-exit") || has("missing-precondition") {
        RuleBuilder::run(r"--no-ensure --profile .*--provider")
            .foreground(&[])
            .append("SMOKE_EXIT:7\n$ ")
    } else if has("startup-missing") {
        RuleBuilder::run(r"--no-ensure --profile .*--provider")
            .foreground(&["-bash", "$0"])
            .append("connecting\n")
    } else {
        RuleBuilder::run(r"--no-ensure --profile .*--provider")
            .foreground(&["-bash", "$0"])
            .append(&fixture("startup.txt"))
    };
    let prompt =
        r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$";
    let reply_screen = if has("before-workflows") {
        "Talaria · profile fixture\n> unfinished draft\n".to_owned()
    } else {
        idle.clone()
    };
    let mut first_reply = RuleBuilder::send_text(prompt);
    if !has("reply-missing") {
        first_reply = first_reply
            .remember("$1-$2")
            .append(&fixture("plain-answer.txt").replace("<token>", "$1-$2"));
    }
    if has("reply-busy") {
        first_reply = first_reply.churn(1_000_000);
    } else {
        first_reply = first_reply.then(StageBuilder::new().after_polls(4).append(&reply_screen));
    }
    let first_quit = |typed: bool| {
        let rule = if typed {
            RuleBuilder::send_text(r"^/quit$")
        } else {
            RuleBuilder::run(r"^/quit$")
        };
        if has("quit-stuck") {
            rule
        } else if has("token-lost") {
            rule.wipe().foreground(&[]).append("SMOKE_EXIT:0\n$ ")
        } else {
            rule.foreground(&[])
                .append(&fixture("quit.txt").replace("<token>", "${mem:0}"))
        }
    };
    let resumed = if has("resume-token-missing") {
        idle.clone()
    } else {
        fixture("resume.txt").replace("<token>", "${mem:0}")
    };
    let second_start = RuleBuilder::run(r"--no-ensure -c --profile")
        .clear()
        .foreground(&["-bash", "$0"])
        .append(&resumed);
    let mut second_reply = RuleBuilder::send_text(prompt);
    if !has("resume-reply-missing") {
        second_reply = second_reply
            .remember("$1-$2")
            .append(&fixture("fresh-answer.txt").replace("<token>", "$1-$2"));
    }
    let second_quit = |typed: bool| {
        let rule = if typed {
            RuleBuilder::send_text(r"^/quit$")
        } else {
            RuleBuilder::run(r"^/quit$")
        };
        if has("resume-quit-stuck") {
            rule
        } else if has("resume-shell-missing") {
            rule.foreground(&["-bash", "busy"]).append("SMOKE_EXIT:0\n")
        } else {
            rule.foreground(&[]).append("SMOKE_EXIT:0\n$ ")
        }
    };
    let after_workflow = if has("workflows-stuck") {
        workflow.clone()
    } else if has("after-workflows") {
        "Talaria · profile fixture\n> unfinished draft\n".to_owned()
    } else {
        idle.clone()
    };
    let after_status = if has("after-status") {
        "Talaria · profile fixture\n> unfinished draft\n".to_owned()
    } else {
        idle.clone()
    };
    let mut workflow_esc = RuleBuilder::send_keys("^esc$")
        .clear()
        .append(&after_workflow);
    if has("before-status") {
        workflow_esc = workflow_esc.then(
            StageBuilder::new()
                .after_polls(2)
                .clear()
                .append("Talaria · profile fixture\n> unfinished draft\n"),
        );
    }
    let mut script = ScriptBuilder::new()
        .on_first(start)
        .on_first(first_reply)
        .on_first(RuleBuilder::send_keys("^enter$").repeat())
        .on_first(
            RuleBuilder::send_text(r"^/workflows$")
                .clear()
                .append(&workflow),
        )
        .on_first(workflow_esc)
        .on_first(
            RuleBuilder::send_text(r"^/ultracode status$")
                .clear()
                .append(&status),
        )
        .on_first(
            RuleBuilder::send_keys("^esc$")
                .clear()
                .append(&after_status),
        )
        .on_first(first_quit(true))
        .on_first(first_quit(false))
        .on_second(second_start)
        .on_second(second_reply)
        .on_second(RuleBuilder::send_keys("^enter$").repeat())
        .on_second(second_quit(true))
        .on_second(second_quit(false));

    let mut install = fixture("builtins-install.txt");
    let mut install_again = fixture("builtins-install-again.txt");
    let mut list_builtins = fixture("builtins-list.txt");
    let mut check = fixture("builtins-check.txt");
    if has("install-bad") {
        install = install.replace("created folder", "folder");
    }
    if has("install-again-bad") {
        install_again = install_again.replace("unchanged", "created");
    }
    if has("install-missing-one") {
        install = install.replace("created <workflows>/research.js\n", "");
    }
    if has("install-again-missing-one") {
        install_again = install_again.replace("unchanged <workflows>/research.js\n", "");
    }
    if has("builtins-list-bad") {
        let changed = list_builtins.replacen("installed: installed", "installed: absent", 1);
        assert_ne!(changed, list_builtins, "recorded installed marker");
        list_builtins = changed;
    }
    if has("check-bad") {
        check = check
            .replace("missing tiers:", "ok")
            .replace("exit=2", "exit=0");
    }
    let mut list = fixture("list.txt");
    let mut json_list = fixture("list-json.txt");
    if has("list-text-bad") {
        list = list.replace("No workflow runs found.", "Nothing found.");
    }
    if has("list-json-bad") {
        json_list = json_list.replace("[]", "[{}]");
    }
    let mut gateway = fixture("no-gateway.txt");
    if has("gateway-incorrect") {
        gateway = "script parse failed\nexit=2\n".into();
    }
    if has("gateway-bad-exit") {
        gateway = gateway.replace("exit=1", "exit=0");
    }
    if has("gateway-missing-line") {
        gateway = gateway.replace("gateway not found", "other failure");
    }
    let mut last_list = fixture("list.txt");
    if has("gateway-list-fails") {
        last_list = last_list.replace("exit=0", "exit=1");
    }

    for typed in [false, true] {
        script = script
            .on_first(shell_rule(r"workflow builtins install", &install, typed))
            .on_first(shell_rule(
                r"workflow builtins install",
                &install_again,
                typed,
            ))
            .on_first(shell_rule(r"workflow builtins list", &list_builtins, typed))
            .on_first(shell_rule(r"workflow builtins check", &check, typed));
        let roster_pattern = r#"TALARIA_CONFIG_DIR=['"]?([^'";\s]+).*workflow roster"#;
        let roster = if has("roster-invalid") {
            "not json\nexit=0\n".to_owned()
        } else if has("roster-exit") {
            "{}\nexit=1\n".to_owned()
        } else {
            let mut value: serde_json::Value =
                serde_json::from_str(&fixture("roster.json")).unwrap();
            for field in ["talaria_home", "runs_dir", "workflows_dir", "drafts_dir"] {
                if has(&format!("roster-missing-{field}")) {
                    value["paths"].as_object_mut().unwrap().remove(field);
                }
                if has(&format!("roster-outside-{field}")) {
                    value["paths"][field] = "<scratch>/config/../outside".into();
                }
            }
            if has("roster-missing-workflow") {
                value["workflows"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("path");
            }
            if has("roster-outside-workflow") {
                value["workflows"][0]["path"] = "<scratch>/config/../outside.js".into();
            }
            let value = serde_json::to_string(&value)
                .unwrap()
                .replace("<config>", "$1")
                .replace("<runs>", "$1/runs")
                .replace("<workflows>", "$1/workflows")
                .replace("<drafts>", "$1/drafts")
                .replace("<review-workflow>", "$1/workflows/review-branch.js")
                .replace("<review-branch-workflow>", "$1/workflows/review-branch.js")
                .replace("<research-workflow>", "$1/workflows/research.js")
                .replace(
                    "<fix-failing-tests-workflow>",
                    "$1/workflows/fix-failing-tests.js",
                )
                .replace("<ultracode-workflow>", "$1/workflows/ultracode.js")
                .replace("<scratch>/config", "$1");
            format!("{value}\nexit=0\n")
        };
        script = script
            .on_first(shell_rule(roster_pattern, &roster, typed))
            .on_first(shell_rule(r"workflow list.*--json", &json_list, typed))
            .on_first(shell_rule(r"workflow list", &list, typed));
        let mut run = shell_rule(r"HERMES_HOME=.*workflow run", &gateway, typed);
        if has("created-run") {
            run = run
                .write_in_cwd("config/runs/unexpected/journal.json", "{}")
                .write_in_scratch("runs/unexpected/journal.json", "{}");
        }
        script = script
            .on_first(run)
            .on_first(shell_rule(r"workflow list", &last_list, typed));
    }
    script
}

#[derive(Clone, Debug)]
pub struct ReadReceipt {
    pub index: usize,
    pub pane: String,
    pub source: ReadSource,
    pub text: String,
}
pub struct Exec5 {
    pub report: SmokeReport,
    pub term: FakeTerminal<'static>,
    pub reads: Vec<ReadReceipt>,
    pub root: PathBuf,
    _temp: tempfile::TempDir,
}

struct ReadTerminal<'a> {
    inner: &'a FakeTerminal<'static>,
    reads: Mutex<Vec<ReadReceipt>>,
}
impl Terminal for ReadTerminal<'_> {
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
        let text = self.inner.read(pane, source, lines)?;
        self.reads.lock().unwrap().push(ReadReceipt {
            index: self.inner.calls().len() - 1,
            pane: pane.0.clone(),
            source,
            text: text.clone(),
        });
        Ok(text)
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

pub fn exec_5(
    script: &ScriptBuilder,
    require: bool,
    stop: bool,
    step_scale: f32,
    deadline_secs: u64,
) -> Exec5 {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary run folder");
    let root = temp.path().join("scratch");
    std::fs::create_dir_all(&root).unwrap();
    let talaria = temp.path().join("talaria");
    let slash = std::path::MAIN_SEPARATOR;
    crate::support::write_executable(&talaria, &format!("#!{slash}bin{slash}sh\nexit 0\n"));
    let cfg = SmokeConfig {
        talaria,
        profile: "fixture".into(),
        model: "minimax-m3.1-flash".into(),
        provider: "ollama-cloud".into(),
        deadline: Duration::from_secs(deadline_secs),
        step_scale,
        report_dir: None,
        require,
        temp_root: root.clone(),
        seed: Some(11),
    };
    let cfg5 = Phase5Config {
        profile: cfg.profile.clone(),
        model: cfg.model.clone(),
        provider: cfg.provider.clone(),
    };
    let term = FakeTerminal::from_builder(script, clock);
    let observer = ReadTerminal {
        inner: &term,
        reads: Mutex::new(Vec::new()),
    };
    let report = run_smoke_5(&observer, clock, &cfg, &cfg5, &AtomicBool::new(stop));
    let reads = observer.reads.into_inner().unwrap();
    Exec5 {
        report,
        term,
        reads,
        root,
        _temp: temp,
    }
}
impl Exec5 {
    pub fn lines(&self) -> String {
        self.report.step_lines()
    }
    pub fn close_tabs(&self) -> usize {
        self.term
            .calls()
            .iter()
            .filter(|entry| matches!(entry.call, Call::CloseTab { .. }))
            .count()
    }
    pub fn passed(&self, name: &str) -> bool {
        self.report
            .outcomes
            .iter()
            .any(|outcome| matches!(outcome, StepOutcome::Pass { step, .. } if *step == name))
    }
}
