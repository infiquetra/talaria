//! The configuration suite's command-line contract.
#[path = "support/scripts4a.rs"]
mod scripts4a;
mod support;
use smoke::Suite;
use support::{Harness, RuleBuilder, ScriptBuilder};
const OVERRIDE: &str = "the phase4a suite checks texts that name profile fixture, model minimax-m3.1-flash and provider ollama-cloud; run it without --profile, --model and --provider";
const JQ: &str = "the phase4a suite needs jq on the PATH to parse the doctor and profiles JSON; install jq and run it again";
fn output(out: &support::Outcome) -> String {
    format!("{}\n{}", out.stdout, out.stderr)
}
fn jq_path(h: &Harness) -> String {
    let bin = h.root().join("bin");
    support::write_executable(&bin.join("jq"), "#!/bin/sh\nexit 0\n");
    format!("{}:/usr/bin:/bin", bin.display())
}
#[test]
fn help_lists_phase4a_suite() {
    let h = Harness::plain();
    let out = h.bare_cmd().args(["run", "--help"]).run();
    assert_eq!(out.code, Some(0));
    for name in ["phase1", "phase2a", "phase2b", "phase3", "phase4a"] {
        assert!(out.stdout.contains(name));
    }
}
#[test]
fn phase4a_refuses_other_profile_model_or_provider() {
    for flag in ["--profile", "--model", "--provider"] {
        let h = Harness::plain();
        let report = h.root().join("report");
        std::fs::create_dir(&report).unwrap();
        let out = h
            .cmd()
            .args(["--suite", "phase4a", flag, "other", "--report-dir"])
            .arg(&report)
            .env("PATH", jq_path(&h))
            .run();
        assert_eq!(out.code, Some(2));
        assert!(output(&out).contains(OVERRIDE));
        assert!(!output(&out).contains("SMOKE_RESULT"));
        assert_eq!(std::fs::read_dir(&report).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(h.tmp_dir()).unwrap().count(), 0);
        assert!(!h.fake.saw(&["tab", "create"]));
    }
    let h = Harness::plain();
    let out = h
        .cmd()
        .args([
            "--suite",
            "phase4a",
            "--profile",
            "fixture",
            "--model",
            "minimax-m3.1-flash",
            "--provider",
            "ollama-cloud",
        ])
        .env("PATH", jq_path(&h))
        .run();
    assert!(!output(&out).contains(OVERRIDE));
    assert!(!output(&out).contains("unknown suite"));
}
#[test]
fn missing_jq_is_exit_2_and_creates_nothing() {
    let h = Harness::plain();
    let report = h.root().join("report");
    std::fs::create_dir(&report).unwrap();
    let out = h
        .cmd()
        .args(["--suite", "phase4a", "--report-dir"])
        .arg(&report)
        .env("PATH", h.root().join("bin"))
        .run();
    assert_eq!(out.code, Some(2));
    assert!(output(&out).contains(JQ));
    assert!(!output(&out).contains("SMOKE_RESULT"));
    assert_eq!(std::fs::read_dir(&report).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(h.tmp_dir()).unwrap().count(), 0);
    assert!(!h.fake.saw(&["tab", "create"]));
}
#[test]
fn deadline_default_is_900_for_phase4a_and_300_for_phase1() {
    assert_eq!(Suite::Phase4a.default_deadline_secs(), 900);
    assert_eq!(Suite::Phase1.default_deadline_secs(), 300);
}
#[test]
#[allow(clippy::type_complexity)]
fn exit_codes_0_1_2_3() {
    let scenarios: &[(&[(&str, &str, &str)], i32, &str)] = &[
        (&[], 0, "SMOKE_RESULT: PASSED (elapsed "),
        (
            &[("config-browse-r1.txt", "general", "")],
            1,
            "SMOKE_RESULT: FAILED (config-browse)",
        ),
        (
            &[(
                "providers-tabs-r1.txt",
                "OpenRouter · OPENROUTER_API_KEY · no key",
                "OpenRouter · OPENROUTER_API_KEY · key …abcd",
            )],
            3,
            "SMOKE_RESULT: PASSED WITH SKIPS (providers-tabs)",
        ),
    ];
    for (replacements, expected, line) in scenarios {
        let h = Harness::new(&scripts4a::script_4a(replacements));
        let out = h
            .cmd()
            .args(["--suite", "phase4a"])
            .env("PATH", jq_path(&h))
            .run();
        assert_eq!(out.code, Some(*expected), "{}", output(&out));
        assert!(output(&out).contains(line), "{}", output(&out));
    }
    let h = Harness::plain();
    let out = h
        .cmd()
        .args(["--suite", "phase4a", "--profile", "other"])
        .env("PATH", jq_path(&h))
        .run();
    assert_eq!(out.code, Some(2));
    assert!(output(&out).contains(OVERRIDE));
}
#[test]
fn failure_report_names_the_pane_and_is_scrubbed() {
    let h = Harness::plain();
    let home = h.home_dir().display().to_string();
    let user = h.user().to_owned();
    let secret = "not-a-real-key";
    let junk = format!("sk-or-v1-{}", "0".repeat(64));
    let planted = format!("$0\nlogin={user} home={home} {secret} {junk}\nSMOKE_EXIT:7\n$ ");
    let script =
        ScriptBuilder::new().on_first(RuleBuilder::run("--no-ensure --profile ").append(&planted));
    std::fs::write(h.fake.dir().join("script.json"), script.to_json()).unwrap();
    let report = h.root().join("report");
    std::fs::create_dir(&report).unwrap();
    let out = h
        .cmd()
        .args(["--suite", "phase4a", "--report-dir"])
        .arg(&report)
        .env("PATH", jq_path(&h))
        .run();
    assert_eq!(out.code, Some(1));
    assert!(output(&out).contains("SMOKE_RESULT: FAILED"));
    let contents = std::fs::read_dir(report)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap_or_default())
        .collect::<String>();
    for text in [&output(&out), &contents] {
        assert!(text.contains("pane: first"));
        assert!(text.contains("<home>"));
        assert!(text.contains("<user>"));
        assert_eq!(text.matches("<typed-secret>").count(), 2);
        for value in [&home, &user, &secret.to_owned(), &junk] {
            assert!(!text.contains(value));
        }
    }
}
