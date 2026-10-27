//! Synthetic terminal script and execution helper for the configuration suite.
#![allow(dead_code)]
use crate::support::{Call, FakeTerminal, RuleBuilder, ScriptBuilder, StageBuilder};
use smoke::suite4a::{Phase4aConfig, run_smoke_4a};
use smoke::{ManualClock, SmokeConfig, SmokeReport, StepOutcome};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};

pub struct Exec4a {
    pub report: SmokeReport,
    pub term: FakeTerminal<'static>,
    pub root: PathBuf,
    pub empty_home_parent: PathBuf,
    _temp: tempfile::TempDir,
}

fn fixture(name: &str, replacements: &[(&str, &str, &str)]) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/phase4a")
        .join(name);
    let mut text = std::fs::read_to_string(path).expect("synthetic screen");
    for (file, before, after) in replacements {
        if *file == name {
            text = text.replace(before, after);
        }
    }
    text.strip_prefix("# synthetic\n")
        .unwrap_or(&text)
        .to_owned()
}

fn next_idle(name: &str) -> Option<&'static str> {
    match name {
        "config-browse-r4.txt" => Some("config-invalid-input-r0.txt"),
        "config-invalid-input-teardown.txt" => Some("config-secret-refused-r0.txt"),
        "config-secret-refused-r3.txt" => Some("providers-tabs-r0.txt"),
        "providers-tabs-teardown.txt" => Some("providers-key-masked-r0.txt"),
        "providers-key-masked-teardown.txt" => Some("models-page-r0.txt"),
        "models-page-teardown.txt" => Some("models-bad-preset-r0.txt"),
        "models-bad-preset-teardown.txt" => Some("profiles-list-r0.txt"),
        "profiles-list-r3.txt" => Some("profiles-bad-names-r0.txt"),
        "profiles-bad-names-teardown.txt" => Some("doctor-overlay-r0.txt"),
        _ => None,
    }
}

/// Each replacement changes one screen, without modifying the fixture on disk.
pub fn script_4a(replacements: &[(&str, &str, &str)]) -> ScriptBuilder {
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
    if flag("@startup-exit") {
        return ScriptBuilder::new().on_first(
            RuleBuilder::run(r"--no-ensure --profile .*--provider")
                .foreground(&[])
                .append("SMOKE_EXIT:7\n"),
        );
    }
    if flag("@startup-unstable") {
        return ScriptBuilder::new().on_first(
            RuleBuilder::run(r"--no-ensure --profile .*--provider")
                .foreground(&["-bash", "$0"])
                .append("starting\n")
                .churn(1000000),
        );
    }
    let quit = if flag("@lost-token") {
        RuleBuilder::run(r"^/quit$")
            .wipe()
            .foreground(&[])
            .append("SMOKE_EXIT:0\n$ ")
    } else if flag("@quit-exit") {
        RuleBuilder::run(r"^/quit$")
            .foreground(&[])
            .append("SMOKE_EXIT:7\n")
    } else if flag("@quit-never") {
        RuleBuilder::run(r"^/quit$")
    } else if flag("@quit-shell") {
        RuleBuilder::run(r"^/quit$").append("SMOKE_EXIT:0\n")
    } else {
        RuleBuilder::run(r"^/quit$")
            .foreground(&[])
            .append("SMOKE_EXIT:0\n$ ")
    };
    let resume = if flag("@duplicate-token") {
        "${mem:0}\n${mem:0}\nminimax-m3.1-flash\n> \n"
    } else if flag("@missing-token") {
        "minimax-m3.1-flash\n> \n"
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
            .append(resume)
            .churn(1000000)
    } else {
        RuleBuilder::run(r"--no-ensure -c --profile")
            .foreground(&["-bash", "$0"])
            .append(resume)
    };
    let second_reply = if flag("@resume-silent") {
        RuleBuilder::run(
            r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$",
        )
    } else {
        RuleBuilder::run(
            r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$",
        )
        .remember("$1-$2")
        .append("agent: $1-$2\n")
    };
    let second_quit = if flag("@resume-quit") {
        RuleBuilder::run(r"^/quit$")
            .foreground(&[])
            .append("SMOKE_EXIT:7\n")
    } else {
        RuleBuilder::run(r"^/quit$")
            .foreground(&[])
            .append("SMOKE_EXIT:0\n$ ")
    };
    let mut script = ScriptBuilder::new()
        .on_first(RuleBuilder::run(r"--no-ensure --profile .*--provider")
            .foreground(&["-bash", "$0"]).append("minimax-m3.1-flash\n> \n"))
        .on_first(RuleBuilder::run(r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$")
            .remember("$1-$2").append("agent: $1-$2\n").then(StageBuilder::new().after_polls(4).clear().append(&fixture("config-browse-r0.txt", replacements))))
        .on_first(quit)
        .on_second(second_start)
        .on_second(second_reply)
        .on_second(second_quit);
    if flag("@resume-shell") {
        script = script
            .second_initial("")
            .second_foreground(&["-bash", "busy"]);
    }
    // The first matching unused rule for each input wins; this list follows the complete
    // interaction order, including keys that do not change the synthetic screen.
    let junk_key = format!("sk-or-v1-{}", "0".repeat(64));
    let events: &[(&str, &str, Option<&str>)] = &[
        ("text", "/config", Some("config-browse-r1.txt")),
        ("key", "enter", None),
        ("key", "right", None),
        ("key", "right", None),
        ("text", "/", None),
        ("text", "compact", Some("config-browse-r2.txt")),
        ("key", "esc", None),
        ("text", "/", None),
        ("text", "api_key", Some("config-browse-r3.txt")),
        ("key", "esc", None),
        ("key", "esc", Some("config-browse-r4.txt")),
        ("text", "/config", Some("config-browse-r1.txt")),
        ("key", "enter", None),
        ("text", "/", None),
        ("text", "resume_exchanges", None),
        ("key", "enter", None),
        ("key", "enter", None),
        ("text", "abc", None),
        ("key", "enter", Some("config-invalid-input-r1.txt")),
        ("key", "esc", Some("config-invalid-input-r2.txt")),
        ("text", "a", Some("config-invalid-input-r3.txt")),
        ("key", "esc", Some("config-invalid-input-teardown.txt")),
        (
            "text",
            "/config delegation.api_key=not-a-real-key",
            Some("config-secret-refused-r1.txt"),
        ),
        ("key", "enter", None),
        ("key", "up", Some("config-secret-refused-r2.txt")),
        ("key", "ctrl+c", Some("config-secret-refused-r3.txt")),
        ("text", "/providers", Some("providers-tabs-r1.txt")),
        ("key", "enter", None),
        ("key", "tab", Some("providers-tabs-r2.txt")),
        ("key", "tab", Some("providers-tabs-r3.txt")),
        ("key", "esc", Some("providers-tabs-teardown.txt")),
        ("text", "/providers", Some("providers-tabs-r1.txt")),
        ("key", "enter", None),
        ("text", "openrouter", Some("providers-key-masked-r1.txt")),
        ("key", "enter", Some("providers-key-masked-r2.txt")),
        ("text", &junk_key, Some("providers-key-masked-r3.txt")),
        ("key", "esc", Some("providers-key-masked-r4.txt")),
        ("key", "esc", Some("providers-key-masked-teardown.txt")),
        ("text", "/models", Some("models-page-r1.txt")),
        ("key", "enter", None),
        ("text", "2", Some("models-page-r2.txt")),
        ("text", "3", Some("models-page-r3.txt")),
        ("text", "4", Some("models-page-r4.txt")),
        ("text", "5", Some("models-page-r5.txt")),
        ("key", "esc", Some("models-page-teardown.txt")),
        ("text", "/models", Some("models-page-r1.txt")),
        ("key", "enter", None),
        ("text", "5", Some("models-page-r5.txt")),
        ("key", "left", None),
        ("text", "a", None),
        ("text", "uat-test", None),
        ("key", "enter", None),
        ("key", "right", None),
        ("text", "a", None),
        ("key", "esc", None),
        ("key", "ctrl+s", Some("models-bad-preset-r1.txt")),
        ("key", "esc", Some("models-bad-preset-r2.txt")),
        ("text", "d", Some("models-bad-preset-r3.txt")),
        ("text", "/models", Some("models-page-r1.txt")),
        ("key", "enter", None),
        ("text", "5", Some("models-bad-preset-r4.txt")),
        ("key", "esc", Some("models-bad-preset-teardown.txt")),
        ("text", "/profiles", Some("profiles-list-r1.txt")),
        ("key", "enter", None),
        ("text", "/fixture", Some("profiles-list-r1.txt")),
        ("key", "enter", Some("profiles-list-r2.txt")),
        ("key", "esc", None),
        ("key", "esc", Some("profiles-list-r3.txt")),
        ("text", "/profiles", Some("profiles-bad-names-r4.txt")),
        ("key", "enter", None),
        ("text", "n", None),
        ("text", "Bad Name!", None),
        ("key", "enter", Some("profiles-bad-names-r1.txt")),
        ("key", "ctrl+u", None),
        ("text", "default", None),
        ("key", "enter", Some("profiles-bad-names-r2.txt")),
        ("key", "ctrl+u", None),
        ("text", " Fixture ", None),
        ("key", "enter", Some("profiles-bad-names-r3.txt")),
        ("key", "esc", Some("profiles-bad-names-r4.txt")),
        ("key", "esc", Some("profiles-bad-names-teardown.txt")),
        ("text", "/doctor", Some("doctor-overlay-r1.txt")),
        ("key", "enter", None),
        ("key", "down", None),
        ("key", "down", None),
        ("key", "enter", Some("doctor-overlay-r1.txt")),
        ("key", "esc", Some("doctor-overlay-r2.txt")),
    ];
    for (kind, value, screen) in events {
        let expression = format!("^{}$", regex::escape(value));
        let mut rule = if *kind == "text" {
            RuleBuilder::send_text(&expression)
        } else {
            RuleBuilder::send_keys(&expression)
        };
        if let Some(name) = screen {
            let stuck = match *name {
                "config-browse-r4.txt" => {
                    Some(("@stuck-config-browse", "Settings · profile fixture"))
                }
                "config-invalid-input-teardown.txt" => {
                    Some(("@stuck-config-invalid-input", "Settings · profile fixture"))
                }
                "providers-tabs-teardown.txt" => {
                    Some(("@stuck-providers-tabs", "Providers · profile fixture"))
                }
                "providers-key-masked-teardown.txt" => {
                    Some(("@stuck-providers-key-masked", "Providers · profile fixture"))
                }
                "models-page-teardown.txt" => {
                    Some(("@stuck-models-page", "Model roles - profile fixture"))
                }
                "models-bad-preset-teardown.txt" => {
                    Some(("@stuck-models-bad-preset", "Model roles - profile fixture"))
                }
                "profiles-list-r3.txt" => Some(("@stuck-profiles-list", "Profiles (2)")),
                "profiles-bad-names-teardown.txt" => {
                    Some(("@stuck-profiles-bad-names", "Profiles (2)"))
                }
                "doctor-overlay-r2.txt" => Some((
                    "@stuck-doctor-overlay",
                    "Doctor: 7 ok, 0 warnings, 0 failures",
                )),
                _ => None,
            };
            rule = if let Some((marker, title)) = stuck.filter(|(marker, _)| flag(marker)) {
                let _ = marker;
                rule.repeat().clear().append(title)
            } else if *name == "doctor-overlay-r2.txt" && flag("@doctor-close-once") {
                rule.clear()
                    .append("Doctor: 7 ok, 0 warnings, 0 failures\n")
            } else {
                rule.clear().append(&fixture(name, replacements))
            };
            if let Some(next) = next_idle(name) {
                if replacements.iter().any(|(file, _, _)| *file == next) {
                    rule = rule.then(
                        StageBuilder::new()
                            .after_polls(2)
                            .clear()
                            .append(&fixture(next, replacements)),
                    );
                }
            }
        }
        script = script.on_first(rule);
    }
    if flag("@doctor-close-once") {
        script = script.on_first(
            RuleBuilder::send_keys("^esc$")
                .clear()
                .append(&fixture("doctor-overlay-r2.txt", replacements)),
        );
    }
    for (pattern, output) in [
        (r"HERMES_HOME=.*doctor", if flag("@no-gateway-doctor") { "FAIL Gateway\nexit=1\n" } else { "FAIL Gateway\nFix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.\nexit=1\n" }.to_owned()),
        (r"HERMES_HOME=.*profiles", if flag("@no-gateway-profiles") { "no gateway\nexit=1\n" } else { "no Hermes gateway is running for this home\nNext step: start gateway\nexit=1\n" }.to_owned()),
        (r"profiles .*nosuchprofile", if flag("@profiles-unknown") { "exit=2\n" } else if flag("@profiles-exit") { "talaria profiles: no profile named nosuchprofile\nexit=1\n" } else { "talaria profiles: no profile named nosuchprofile\nexit=2\n" }.to_owned()),
        (r"profiles .*--json", if flag("@profiles-json") { "not json\n" } else { "{\"profile\":\"fixture\",\"state\":\"served\"}\n" }.to_owned()),
        (r"profiles .*fixture", fixture("profiles-cli-r1.txt", replacements).split('{').next().unwrap_or("").to_owned()),
        (r"doctor .*nosuchprofile", if flag("@doctor-unknown") { "ok Profiles\nexit=1\n" } else { "FAIL Profiles\nthe gateway never discovered a profile named nosuchprofile\nskip Attach\nexit=1\n" }.to_owned()),
        (r#"doctor .*--profile (?:\"\"|'')"#, if flag("@doctor-blank") { "usage error\nexit=1\n" } else { "usage error\nexit=2\n" }.to_owned()),
        (r"doctor .*--json", if flag("@doctor-json") { "{\"format\":\"unknown\",\"checks\":[]}\n" } else { "{\"format\":\"talaria-doctor-v1\",\"checks\":[1,2,3,4,5,6,7]}\n" }.to_owned()),
        (r"doctor .*fixture", {
            let mut text = fixture("doctor-cli-r1.txt", replacements).split('{').next().unwrap_or("").to_owned();
            if flag("@doctor-home") { text.push_str(&std::env::var("HOME").unwrap_or_default()); text.push('\n'); }
            text
        }),
        (r"echo .*HOME", format!("{}\n", std::env::var("HOME").unwrap_or_default())),
        (r"ls -A", if flag("@home-not-empty") { "unexpected\n" } else { "" }.to_owned()),
    ] {
        script = script.on_first(
            RuleBuilder::run(pattern)
                .repeat()
                .append(&format!("{output}SMOKE_EXIT:0\n$ ")),
        );
    }
    script
}

pub fn exec_4a(
    script: &ScriptBuilder,
    require: bool,
    stop: bool,
    step_scale: f32,
    deadline_secs: u64,
) -> Exec4a {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary run folder");
    let root = temp.path().join("scratch");
    let empty_home_parent = temp.path().join("empty-parent");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&empty_home_parent).unwrap();
    let talaria = temp.path().join("talaria");
    crate::support::write_executable(&talaria, "#!/bin/sh\nexit 0\n");
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
    let cfg4a = Phase4aConfig {
        empty_home_parent: empty_home_parent.clone(),
    };
    let term = FakeTerminal::from_builder(script, clock);
    let report = run_smoke_4a(&term, clock, &cfg, &cfg4a, &AtomicBool::new(stop));
    Exec4a {
        report,
        term,
        root,
        empty_home_parent,
        _temp: temp,
    }
}

impl Exec4a {
    pub fn lines(&self) -> String {
        self.report.step_lines()
    }
    pub fn close_tabs(&self) -> usize {
        self.term
            .calls()
            .iter()
            .filter(|r| matches!(r.call, Call::CloseTab { .. }))
            .count()
    }
    pub fn passed(&self, name: &str) -> bool {
        self.report
            .outcomes
            .iter()
            .any(|o| matches!(o, StepOutcome::Pass { step, .. } if *step == name))
    }
}
