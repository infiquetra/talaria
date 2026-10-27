//! Synthetic screen playback and captured reads for the read-only configuration suite.
#![allow(dead_code)]
use crate::support::{Call, FakeTerminal, RuleBuilder, ScriptBuilder, StageBuilder};
use smoke::suite4b::{Phase4bConfig, run_smoke_4b};
use smoke::{
    ManualClock, NewTab, PaneId, ReadSource, SmokeConfig, SmokeReport, StepOutcome, TabId,
    TermError, Terminal,
};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, atomic::AtomicBool},
    time::Duration,
};

pub const SECRET_LINE: &str =
    "/configure use my openrouter key sk-or-v1-talaria-sentinel-0000000000000000";
pub const SCREEN_STEPS: [&str; 8] = [
    "mcp-browse",
    "capabilities-browse",
    "memory-open",
    "memory-commands",
    "cron-browse",
    "messaging-browse",
    "connectors-consent",
    "configure-secret-refused",
];
pub const ALL_STEPS: [&str; 14] = [
    "startup",
    "plain-answer",
    "mcp-browse",
    "capabilities-browse",
    "memory-open",
    "memory-commands",
    "cron-browse",
    "messaging-browse",
    "connectors-consent",
    "configure-secret-refused",
    "quit",
    "resume",
    "capabilities-cli",
    "no-gateway",
];

#[derive(Clone, Debug)]
pub struct ReadReceipt {
    pub index: usize,
    pub pane: String,
    pub source: ReadSource,
    pub text: String,
}
pub struct Exec4b {
    pub report: SmokeReport,
    pub term: FakeTerminal<'static>,
    pub reads: Vec<ReadReceipt>,
    pub root: PathBuf,
    pub empty_home_parent: PathBuf,
    _temp: tempfile::TempDir,
}

pub fn fixture(name: &str, replacements: &[(&str, &str, &str)]) -> String {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/phase4b");
    let mut text = std::fs::read_to_string(directory.join(name)).expect("synthetic screen");
    for (file, before, after) in replacements {
        if *file == name {
            assert!(
                !before.is_empty() && text.contains(before),
                "mutation {name}: missing target {before:?}"
            );
            text = text.replace(before, after);
        }
    }
    text.strip_prefix("# synthetic\n")
        .unwrap_or(&text)
        .to_owned()
}

pub fn script_4b(replacements: &[(&str, &str, &str)]) -> ScriptBuilder {
    let flag = |name: &str| replacements.iter().any(|(file, _, _)| *file == name);
    if flag("@startup") {
        return ScriptBuilder::new()
            .first_initial("")
            .first_foreground(&["-bash", "busy"]);
    }
    if flag("@reply") {
        return ScriptBuilder::new().on_first(
            RuleBuilder::run(r"--no-ensure --profile .*--provider")
                .foreground(&["-bash", "$0"])
                .append("minimax-m3.1-flash\n> \n"),
        );
    }
    if flag("@startup-exit") || flag("@missing-gateway") || flag("@missing-fixture") {
        let output = if flag("@missing-gateway") {
            "no Hermes gateway is running for this home\nSMOKE_EXIT:1\n"
        } else if flag("@missing-fixture") {
            "profile fixture was not found\nSMOKE_EXIT:1\n"
        } else {
            "SMOKE_EXIT:7\n"
        };
        return ScriptBuilder::new().on_first(
            RuleBuilder::run(r"--no-ensure --profile .*--provider")
                .foreground(&[])
                .append(output),
        );
    }
    if flag("@startup-unstable") {
        return ScriptBuilder::new().on_first(
            RuleBuilder::run(r"--no-ensure --profile .*--provider")
                .foreground(&["-bash", "$0"])
                .append("starting\n")
                .churn(1_000_000),
        );
    }
    let quit = |typed: bool| {
        let rule = if typed {
            RuleBuilder::send_text(r"^/quit$")
        } else {
            RuleBuilder::run(r"^/quit$")
        };
        if flag("@lost-token") {
            rule.wipe().foreground(&[]).append("SMOKE_EXIT:0\n$ ")
        } else if flag("@quit-exit") {
            rule.foreground(&[]).append("SMOKE_EXIT:7\n")
        } else if flag("@quit-never") {
            rule
        } else if flag("@quit-shell") {
            rule.append("SMOKE_EXIT:0\n")
        } else {
            rule.foreground(&[]).append("SMOKE_EXIT:0\n$ ")
        }
    };
    let resumed = if flag("@missing-token") {
        "minimax-m3.1-flash\n> \n"
    } else if flag("@duplicate-token") || flag("@many-committed-token") {
        "${mem:0}\n${mem:0}\nminimax-m3.1-flash\n> \n"
    } else {
        "${mem:0}\nminimax-m3.1-flash\n> \n"
    };
    let second_start = if flag("@resume-exit") {
        RuleBuilder::run(r"--no-ensure -c --profile")
            .foreground(&[])
            .append("SMOKE_EXIT:7\n")
    } else if flag("@resume-unstable") {
        RuleBuilder::run(r"--no-ensure -c --profile")
            .foreground(&["-bash", "$0"])
            .append(resumed)
            .churn(1_000_000)
    } else {
        RuleBuilder::run(r"--no-ensure -c --profile")
            .foreground(&["-bash", "$0"])
            .append(resumed)
    };
    let prompt =
        r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$";
    let second_reply = if flag("@resume-silent") {
        RuleBuilder::run(prompt)
    } else {
        RuleBuilder::run(prompt)
            .remember("$1-$2")
            .append("agent: $1-$2\n")
    };
    let mut script =
        ScriptBuilder::new()
            .on_first(
                RuleBuilder::run(r"--no-ensure --profile .*--provider")
                    .foreground(&["-bash", "$0"])
                    .append("minimax-m3.1-flash\n> \n"),
            )
            .on_first(quit(false))
            .on_first(quit(true))
            .on_first(
                RuleBuilder::send_text(prompt)
                    .remember("$1-$2")
                    .append(if flag("@many-committed-token") {
                        "agent: $1-$2\n$1-$2\n$1-$2\n"
                    } else {
                        "agent: $1-$2\n"
                    })
                    .then(
                        StageBuilder::new()
                            .after_polls(4)
                            .clear()
                            .append(&fixture("mcp-browse-r0.txt", replacements)),
                    ),
            )
            .on_first(RuleBuilder::send_keys("^enter$"))
            .on_second(second_start)
            .on_second(second_reply)
            .on_second(if flag("@resume-silent") {
                RuleBuilder::send_text(prompt)
            } else {
                RuleBuilder::send_text(prompt)
                    .remember("$1-$2")
                    .append("agent: $1-$2\n")
            })
            .on_second(RuleBuilder::send_text(r"^/quit$").foreground(&[]).append(
                if flag("@resume-quit") {
                    "SMOKE_EXIT:7\n"
                } else {
                    "SMOKE_EXIT:0\n$ "
                },
            ))
            .on_second(RuleBuilder::run(r"^/quit$").foreground(&[]).append(
                if flag("@resume-quit") {
                    "SMOKE_EXIT:7\n"
                } else {
                    "SMOKE_EXIT:0\n$ "
                },
            ));
    if flag("@resume-shell") {
        script = script
            .second_initial("")
            .second_foreground(&["-bash", "busy"]);
    }
    let mut events = vec![
        ("text", "/mcp", Some("mcp-browse-r1.txt")),
        ("key", "enter", None),
        ("key", "tab", Some("mcp-browse-r2.txt")),
        ("text", "deepwiki", Some("mcp-browse-r3.txt")),
        ("key", "esc", Some("mcp-browse-r2.txt")),
        ("key", "esc", Some("mcp-browse-teardown.txt")),
        ("text", "/capabilities", Some("capabilities-browse-r1.txt")),
        ("key", "enter", None),
        ("text", "3", Some("capabilities-browse-r2.txt")),
        ("text", "4", Some("capabilities-browse-r3.txt")),
        ("text", "a", Some("capabilities-browse-r4.txt")),
        ("key", "esc", Some("capabilities-browse-r5.txt")),
        ("text", "/memory-settings", Some("memory-open-r1.txt")),
        ("key", "enter", None),
        ("key", "esc", Some("memory-open-r2.txt")),
        ("text", "/memory pending", Some("memory-commands-r1.txt")),
        ("key", "enter", None),
        ("text", "/curator status", Some("memory-commands-r2.txt")),
        ("key", "enter", None),
        ("text", "/cron", Some("cron-browse-r1.txt")),
        ("key", "enter", None),
        ("key", "esc", Some("cron-browse-r2.txt")),
        ("text", "/messaging", Some("messaging-browse-r1.txt")),
        ("key", "enter", None),
        ("text", "/signal", Some("messaging-browse-r2.txt")),
        ("key", "enter", Some("messaging-browse-r3.txt")),
        ("key", "esc", Some("messaging-browse-teardown1.txt")),
        ("key", "esc", Some("messaging-browse-teardown2.txt")),
        ("key", "esc", Some("messaging-browse-teardown3.txt")),
        ("text", "/connectors", Some("connectors-consent-r1.txt")),
        ("key", "enter", None),
        ("key", "esc", Some("connectors-consent-teardown.txt")),
        ("text", SECRET_LINE, Some("configure-secret-refused-r1.txt")),
        ("key", "enter", None),
        ("key", "up", Some("configure-secret-refused-r2.txt")),
        ("key", "ctrl+c", Some("configure-secret-refused-r3.txt")),
    ];
    if flag("@submit-fallback") {
        events.insert(2, ("key", "enter", Some("mcp-browse-r1.txt")));
    }
    for (kind, value, screen) in events {
        let expression = format!("^{}$", regex::escape(value));
        let mut rule = if kind == "text" {
            RuleBuilder::send_text(&expression)
        } else {
            RuleBuilder::send_keys(&expression)
        };
        if let Some(name) = screen {
            let ended = match name {
                "mcp-browse-teardown.txt" => {
                    Some(("mcp-browse", "MCP servers", "capabilities-browse"))
                }
                "capabilities-browse-r5.txt" => {
                    Some(("capabilities-browse", "Capabilities", "memory-open"))
                }
                "memory-open-r2.txt" => Some(("memory-open", "Memory", "memory-commands")),
                "memory-commands-r2.txt" => Some(("memory-commands", "", "cron-browse")),
                "cron-browse-r2.txt" => Some(("cron-browse", "Scheduled jobs", "messaging-browse")),
                "messaging-browse-teardown3.txt" => Some((
                    "messaging-browse",
                    "Connectors and messaging",
                    "connectors-consent",
                )),
                "connectors-consent-teardown.txt" => Some((
                    "connectors-consent",
                    "Connectors and messaging",
                    "configure-secret-refused",
                )),
                _ => None,
            };
            if kind == "text" && value == "/mcp" && flag("@submit-fallback") {
                rule = rule
                    .clear()
                    .append(&fixture("mcp-browse-r0.txt", replacements).replace('>', "> /mcp"));
            } else if let Some((step, title, _)) =
                ended.filter(|(step, _, _)| flag(&format!("@stuck-{step}")))
            {
                let _ = step;
                rule = rule
                    .repeat()
                    .clear()
                    .append(&format!("{title} · profile fixture\n"));
            } else {
                rule = rule.clear().append(&fixture(name, replacements));
            }
            if let Some((_, _, next)) = ended {
                let next = format!("{next}-r0.txt");
                if replacements.iter().any(|(file, _, _)| *file == next) {
                    rule = rule.then(
                        StageBuilder::new()
                            .after_polls(2)
                            .clear()
                            .append(&fixture(&next, replacements)),
                    );
                }
            }
        }
        script = script.on_first(rule);
    }
    for (pattern, output) in [
        (r"HERMES_HOME=.*capabilities .*nothing", if flag("@unknown-list") { "exit=1\n" } else { "talaria capabilities: unknown list nothing; use skills, toolsets or plugins\nexit=2\n" }.to_owned()),
        (r"HERMES_HOME=.*capabilities .*skills", if flag("@no-gateway") { "exit=0\n" } else { "talaria: no Hermes gateway is running for this home\nNext step: start gateway\nexit=1\n" }.to_owned()),
        (r"capabilities .*--json", if flag("@missing-jq") { "jq: command not found\nexit=127\n".into() } else if flag("@capabilities-json") { "not json\n".into() } else if flag("@capabilities-profile") { "{\"profile\":\"default\"}\n".into() } else if flag("@capabilities-key") { "{\"profile\":\"fixture\",\"items\":[{\"path\":\"hidden\"}]}\n".into() } else if flag("@capabilities-home") { serde_json::json!({"profile":"fixture", "value":std::env::var("HOME").unwrap_or_default()}).to_string() } else { "{\"profile\":\"fixture\",\"items\":[]}\n".into() }),
        (r"capabilities .*skills", fixture("capabilities-cli-r1.txt", replacements)),
        (r"echo .*HOME", format!("{}\n", std::env::var("HOME").unwrap_or_default())),
        (r"ls -A", if flag("@home-not-empty") { "unexpected\n" } else { "" }.to_owned()),
    ] {
        script = script.on_first(RuleBuilder::run(pattern).repeat().append(&format!("{output}SMOKE_EXIT:0\n$ ")));
    }
    script
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

pub fn exec_4b(
    script: &ScriptBuilder,
    require: bool,
    stop: bool,
    step_scale: f32,
    deadline_secs: u64,
) -> Exec4b {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary run folder");
    let root = temp.path().join("scratch");
    let empty_home_parent = temp.path().join("empty-parent");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&empty_home_parent).unwrap();
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
    let cfg4b = Phase4bConfig {
        empty_home_parent: empty_home_parent.clone(),
    };
    let term = FakeTerminal::from_builder(script, clock);
    let observer = ReadTerminal {
        inner: &term,
        reads: Mutex::new(Vec::new()),
    };
    let report = run_smoke_4b(&observer, clock, &cfg, &cfg4b, &AtomicBool::new(stop));
    let reads = observer.reads.into_inner().unwrap();
    Exec4b {
        report,
        term,
        reads,
        root,
        empty_home_parent,
        _temp: temp,
    }
}
impl Exec4b {
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
            .any(|outcome| matches!(outcome,StepOutcome::Pass{step,..} if *step == name))
    }
}
