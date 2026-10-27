//! Command-line preflight and reporting for the browse suite.
#[path = "support/scripts4b.rs"]
mod scripts4b;
mod support;

use smoke::Suite;
use support::{Harness, RuleBuilder, ScriptBuilder};

const OVERRIDE: &str = "the phase4b suite checks texts that name profile fixture, model minimax-m3.1-flash and provider ollama-cloud; run it without --profile, --model and --provider";
const JQ: &str = "the phase4b suite needs jq on the PATH to parse the capabilities JSON; install jq and run it again";
const SENTINEL: &str = "sk-or-v1-talaria-sentinel-0000000000000000";

fn output(out: &support::Outcome) -> String {
    format!("{}\n{}", out.stdout, out.stderr)
}

fn jq_path(harness: &Harness) -> String {
    let bin = harness.root().join("bin");
    let slash = std::path::MAIN_SEPARATOR;
    support::write_executable(&bin.join("jq"), &format!("#!{slash}bin{slash}sh\nexit 0\n"));
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited)))
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

fn assert_nothing_created(harness: &Harness, report: &std::path::Path) {
    assert_eq!(std::fs::read_dir(report).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(harness.tmp_dir()).unwrap().count(), 0);
    assert!(!harness.fake.saw(&["tab", "create"]));
}

#[test]
fn help_lists_phase4b_suite() {
    let harness = Harness::plain();
    let out = harness.bare_cmd().args(["run", "--help"]).run();
    assert_eq!(out.code, Some(0));
    for name in [
        "phase1", "phase2a", "phase2b", "phase3", "phase4a", "phase4b",
    ] {
        assert!(out.stdout.contains(name), "missing suite {name}");
    }
}

#[test]
fn phase4b_refuses_other_profile_model_or_provider() {
    for flag in ["--profile", "--model", "--provider"] {
        let harness = Harness::plain();
        let report = harness.root().join("report");
        std::fs::create_dir(&report).unwrap();
        let out = harness
            .cmd()
            .args(["--suite", "phase4b", flag, "other", "--report-dir"])
            .arg(&report)
            .env("PATH", jq_path(&harness))
            .run();
        assert_eq!(out.code, Some(2), "{}", output(&out));
        assert!(output(&out).contains(OVERRIDE), "{}", output(&out));
        assert!(!output(&out).contains("SMOKE_RESULT"));
        assert_nothing_created(&harness, &report);
    }
    let harness = Harness::plain();
    let out = harness
        .cmd()
        .args([
            "--suite",
            "phase4b",
            "--profile",
            "fixture",
            "--model",
            "minimax-m3.1-flash",
            "--provider",
            "ollama-cloud",
        ])
        .env("PATH", jq_path(&harness))
        .step_scale(0.01)
        .run();
    assert!(!output(&out).contains(OVERRIDE));
    assert!(!output(&out).contains("unknown suite"), "{}", output(&out));
}

#[test]
fn missing_jq_is_exit_2_and_creates_nothing() {
    for extra in [vec![], vec!["--profile", "other"]] {
        let harness = Harness::plain();
        let report = harness.root().join("report");
        std::fs::create_dir(&report).unwrap();
        let out = harness
            .cmd()
            .args(["--suite", "phase4b", "--report-dir"])
            .arg(&report)
            .args(extra)
            .env("PATH", harness.root().join("bin"))
            .run();
        assert_eq!(out.code, Some(2));
        assert!(output(&out).contains(JQ), "{}", output(&out));
        assert!(!output(&out).contains(OVERRIDE));
        assert!(!output(&out).contains("SMOKE_RESULT"));
        assert_nothing_created(&harness, &report);
    }
}

#[test]
fn deadline_default_is_900_for_phase4b_and_300_for_phase1() {
    assert_eq!(Suite::Phase4b.default_deadline_secs(), 900);
    assert_eq!(Suite::Phase1.default_deadline_secs(), 300);
}

#[test]
fn exit_codes_are_0_1_2_and_never_3() {
    struct Case {
        mutation: &'static [(&'static str, &'static str, &'static str)],
        expected: i32,
        result: &'static str,
    }
    let cases = [
        Case {
            mutation: &[],
            expected: 0,
            result: "SMOKE_RESULT: PASSED (elapsed ",
        },
        Case {
            mutation: &[("mcp-browse-r1.txt", "MCP servers · profile fixture", "")],
            expected: 1,
            result: "SMOKE_RESULT: FAILED (mcp-browse)",
        },
        Case {
            mutation: &[("@no-gateway", "", "")],
            expected: 1,
            result: "SMOKE_RESULT: FAILED (no-gateway)",
        },
    ];
    for case in cases {
        for require in [false, true] {
            let harness = Harness::new(&scripts4b::script_4b(case.mutation));
            let mut command = harness
                .cmd()
                .args(["--suite", "phase4b"])
                .env("PATH", jq_path(&harness))
                .step_scale(0.01);
            if require {
                command = command.live_require();
            }
            let out = command.run();
            assert_eq!(out.code, Some(case.expected), "{}", output(&out));
            assert_ne!(out.code, Some(3));
            assert!(output(&out).contains(case.result), "{}", output(&out));
            assert!(!output(&out).contains("SKIP"));
        }
    }
    let harness = Harness::plain();
    let out = harness
        .cmd()
        .args(["--suite", "phase4b", "--profile", "other"])
        .env("PATH", jq_path(&harness))
        .run();
    assert_eq!(out.code, Some(2));
    assert_ne!(out.code, Some(3));
    assert!(output(&out).contains(OVERRIDE));
}

#[test]
fn failure_report_names_the_pane_and_is_scrubbed() {
    let harness = Harness::plain();
    let home = harness.home_dir().display().to_string();
    let user = harness.user().to_owned();
    let planted = format!("$0\nlogin={user} home={home} {SENTINEL}\nSMOKE_EXIT:7\n$ ");
    let script =
        ScriptBuilder::new().on_first(RuleBuilder::run("--no-ensure --profile ").append(&planted));
    std::fs::write(harness.fake.dir().join("script.json"), script.to_json()).unwrap();
    let report = harness.root().join("report");
    std::fs::create_dir(&report).unwrap();
    let out = harness
        .cmd()
        .args(["--suite", "phase4b", "--report-dir"])
        .arg(&report)
        .env("PATH", jq_path(&harness))
        .step_scale(0.01)
        .run();
    assert_eq!(out.code, Some(1), "{}", output(&out));
    assert!(output(&out).contains("SMOKE_RESULT: FAILED"));
    let files: Vec<_> = std::fs::read_dir(&report)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(!files.is_empty(), "a failure report was written");
    let contents = files
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("failure report text"))
        .collect::<String>();
    for text in [&output(&out), &contents] {
        for expected in ["pane: first", "<home>", "<user>", "<typed-secret>"] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        for value in [&home, &user, SENTINEL] {
            assert!(!text.contains(value), "private value escaped scrubbing");
        }
    }
}
