//! Command-line selection, defaults, preflight, and result codes.
#[path = "support/scripts5.rs"]
mod scripts5;
mod support;
use smoke::{Suite, suite5::Phase5Config};
use std::str::FromStr;
use support::Harness;

fn output(out: &support::Outcome) -> String {
    format!("{}\n{}", out.stdout, out.stderr)
}
fn jq_path(harness: &Harness) -> std::ffi::OsString {
    let bin = harness.root().join("bin");
    let slash = std::path::MAIN_SEPARATOR;
    support::write_executable(&bin.join("jq"), &format!("#!{slash}bin{slash}sh\nexit 0\n"));
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited))).unwrap()
}
fn no_artifacts(harness: &Harness, report: &std::path::Path) {
    assert_eq!(std::fs::read_dir(report).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(harness.tmp_dir()).unwrap().count(), 0);
    assert!(!harness.fake.saw(&["tab", "create"]));
}

#[test]
fn help_lists_phase5_suite() {
    let harness = Harness::plain();
    let out = harness.bare_cmd().args(["run", "--help"]).run();
    assert_eq!(out.code, Some(0));
    for suite in [
        "phase1", "phase2a", "phase2b", "phase3", "phase4a", "phase4b", "phase5",
    ] {
        assert!(out.stdout.contains(suite), "missing {suite}");
    }
}

#[test]
fn phase5_is_selectable_and_other_suite_parsing_is_unchanged() {
    assert_eq!(Suite::from_str("phase5"), Ok(Suite::Phase5));
    assert_eq!(Suite::from_str("phase1"), Ok(Suite::Phase1));
    assert_eq!(
        Suite::from_str("typo").unwrap_err(),
        "unknown suite 'typo'; use phase1 or phase2a"
    );
}

#[test]
fn default_configuration_and_deadline() {
    let cfg = Phase5Config::default();
    assert_eq!(
        (
            cfg.profile.as_str(),
            cfg.model.as_str(),
            cfg.provider.as_str()
        ),
        ("fixture", "minimax-m3.1-flash", "ollama-cloud")
    );
    assert_eq!(Suite::Phase5.default_deadline_secs(), 600);
    assert_eq!(Suite::Phase1.default_deadline_secs(), 300);
    assert_eq!(scripts5::LIMITS.iter().sum::<u64>(), 410);
}

#[test]
fn refuses_other_profile_model_or_provider_before_creating_anything() {
    for flag in ["--profile", "--model", "--provider"] {
        let harness = Harness::plain();
        let report = harness.root().join("report");
        std::fs::create_dir(&report).unwrap();
        let out = harness
            .cmd()
            .args(["--suite", "phase5", flag, "other", "--report-dir"])
            .arg(&report)
            .env("PATH", jq_path(&harness))
            .run();
        assert_eq!(out.code, Some(2), "{}", output(&out));
        assert!(!output(&out).contains("unknown suite"));
        assert!(
            output(&out).contains("fixture")
                && output(&out).contains("minimax-m3.1-flash")
                && output(&out).contains("ollama-cloud"),
            "{}",
            output(&out)
        );
        assert!(!output(&out).contains("SMOKE_RESULT"));
        no_artifacts(&harness, &report);
    }
}

#[test]
fn missing_jq_is_exit_2_and_creates_nothing() {
    let harness = Harness::plain();
    let report = harness.root().join("report");
    std::fs::create_dir(&report).unwrap();
    let out = harness
        .cmd()
        .args(["--suite", "phase5", "--report-dir"])
        .arg(&report)
        .env("PATH", harness.root().join("bin"))
        .run();
    assert_eq!(out.code, Some(2));
    assert!(output(&out).contains("jq"), "{}", output(&out));
    assert!(!output(&out).contains("SMOKE_RESULT"));
    no_artifacts(&harness, &report);
}

#[test]
fn exit_codes_are_0_1_2_and_never_3() {
    for (modes, code, result) in [
        (&[][..], 0, "SMOKE_RESULT: PASSED (elapsed "),
        (
            &["workflows-missing"][..],
            1,
            "SMOKE_RESULT: FAILED (workflows-empty)",
        ),
        (
            &["gateway-incorrect"][..],
            1,
            "SMOKE_RESULT: FAILED (no-gateway)",
        ),
    ] {
        for require in [false, true] {
            let harness = Harness::new(&scripts5::script_5(modes));
            let mut cmd = harness
                .cmd()
                .args(["--suite", "phase5"])
                .env("PATH", jq_path(&harness))
                .step_scale(0.01);
            if require {
                cmd = cmd.live_require();
            }
            let out = cmd.run();
            assert_eq!(out.code, Some(code), "{}", output(&out));
            assert_ne!(out.code, Some(3));
            assert!(output(&out).contains(result), "{}", output(&out));
            assert!(!output(&out).contains("SKIP"));
        }
    }
    let harness = Harness::plain();
    let out = harness
        .cmd()
        .args(["--suite", "phase5", "--profile", "other"])
        .env("PATH", jq_path(&harness))
        .run();
    assert_eq!(out.code, Some(2));
    assert!(!output(&out).contains("unknown suite"));
    assert!(!output(&out).contains("SMOKE_RESULT"));
}

#[test]
fn outside_herdr_fails_preflight_without_a_result() {
    let harness = Harness::plain();
    let out = harness
        .bare_cmd()
        .args(["run", "--suite", "phase5"])
        .env("PATH", jq_path(&harness))
        .env_remove("HERDR_ENV")
        .run();
    assert_eq!(out.code, Some(2));
    assert!(output(&out).contains("Herdr pane"), "{}", output(&out));
    assert!(!output(&out).contains("SMOKE_RESULT"));
}
