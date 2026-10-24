//! The built `smoke` binary against the fake `herdr`: options, preflight, result lines and exit
//! codes, the scratch project and the failure report. Every run gets its own private temporary
//! directory, home directory and generated login name from the harness.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use support::{
    Approval, FailBuilder, Harness, Outcome, RuleBuilder, Scenario, ScriptBuilder, Startup,
    write_executable,
};

const SKIP_REASON: &str = "no approval card appeared and the command ran; the profile fixture must set approvals.mode: manual (the default is smart, in which Hermes may approve by itself; a hosted-room policy can also override the mode)";

const STEPS: [&str; 6] = [
    "startup",
    "plain-answer",
    "approval",
    "interrupt",
    "quit",
    "resume",
];

const OPTIONS: [&str; 8] = [
    "--talaria",
    "--herdr",
    "--profile",
    "--model",
    "--provider",
    "--deadline-secs",
    "--step-scale",
    "--report-dir",
];

/// The text of the recorded `herdr --version`.
fn version() -> &'static str {
    include_str!("fixtures/herdr/version.txt").trim()
}

fn explain(outcome: &Outcome) -> String {
    format!(
        "exit code {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
}

fn everything(outcome: &Outcome) -> String {
    format!("{}\n{}", outcome.stdout, outcome.stderr)
}

/// A run that ends at the first step: the binary exits with code 7 at once.
fn quick_failure() -> Harness {
    Harness::from_scenario(&Scenario::happy().startup(Startup::ExitsWith(7)))
}

/// Exit 2, no result line, and nothing was created in the multiplexer or on disk.
fn assert_preflight_failure(harness: &Harness, outcome: &Outcome, what: &str) {
    assert_eq!(outcome.code, Some(2), "{what}: {}", explain(outcome));
    assert!(
        !everything(outcome).contains("SMOKE_RESULT"),
        "{what}: a preflight failure prints no result line; {}",
        explain(outcome)
    );
    assert!(
        !harness.fake.saw(&["tab", "create"]),
        "{what}: no tab may be created; log {:?}",
        harness.fake.log()
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "{what}: no scratch project may be created: {:?}",
        harness.scratch_dirs()
    );
    assert!(
        !everything(outcome).trim().is_empty(),
        "{what}: a plain sentence says what to do"
    );
}

/// The first `pane run` line the fake saw (the launch line of the first step).
fn first_launch_line(harness: &Harness) -> String {
    harness
        .fake
        .calls_starting(&["pane", "run"])
        .first()
        .and_then(|args| args.get(3).cloned())
        .unwrap_or_else(|| panic!("the fake saw no `pane run`; log {:?}", harness.fake.log()))
}

fn default_launch_line(talaria: &Path) -> String {
    let real = fs::canonicalize(talaria).expect("the binary under test exists");
    format!(
        "'{}' --no-ensure --profile fixture --model minimax-m3.1-flash --provider ollama-cloud; echo SMOKE_EXIT:$?",
        real.display()
    )
}

fn step_names(outcome: &Outcome, prefix: &str) -> Vec<String> {
    outcome
        .lines()
        .iter()
        .filter_map(|l| l.strip_prefix(prefix))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn help_lists_every_option() {
    let harness = Harness::plain();
    for args in [vec!["--help"], vec!["run", "--help"]] {
        let outcome = harness.bare_cmd().outside_herdr().args(&args).run();
        assert_eq!(outcome.code, Some(0), "{args:?}: {}", explain(&outcome));
        for option in OPTIONS {
            assert!(
                outcome.stdout.contains(option),
                "{args:?}: --help must list {option}; {}",
                explain(&outcome)
            );
        }
    }
    let top = harness.bare_cmd().outside_herdr().arg("--help").run();
    assert!(
        top.stdout.contains("run"),
        "the help names the run command; {}",
        explain(&top)
    );
}

#[test]
fn outside_herdr_exits_2_no_tab() {
    let harness = Harness::plain();

    let outcome = harness.cmd().outside_herdr().run();

    assert_preflight_failure(&harness, &outcome, "outside Herdr");
    assert!(
        everything(&outcome).contains(
            "This test must run inside a Herdr pane (HERDR_ENV=1). Start it from a Herdr tab."
        ),
        "{}",
        explain(&outcome)
    );

    // One of the two variables alone does not make a Herdr pane.
    let harness = Harness::plain();
    let outcome = harness.cmd().env_remove("HERDR_WORKSPACE_ID").run();
    assert_preflight_failure(&harness, &outcome, "no workspace id");
    let harness = Harness::plain();
    let outcome = harness.cmd().env("HERDR_ENV", "0").run();
    assert_preflight_failure(&harness, &outcome, "HERDR_ENV is not 1");
}

#[test]
fn missing_talaria_binary_exits_2() {
    let harness = Harness::plain();
    let missing = harness.root().join("bin").join("no-such-talaria");
    let outcome = harness
        .cmd_without_talaria()
        .arg("--talaria")
        .arg(&missing)
        .run();
    assert_preflight_failure(&harness, &outcome, "missing binary");
    assert!(
        everything(&outcome).contains("No talaria binary at"),
        "{}",
        explain(&outcome)
    );

    // A file that exists but cannot be run is no binary either.
    let harness = Harness::plain();
    let plain_file = harness.root().join("bin").join("not-executable");
    fs::write(&plain_file, "#!/bin/sh\nexit 0\n").expect("write a plain file");
    let outcome = harness
        .cmd_without_talaria()
        .arg("--talaria")
        .arg(&plain_file)
        .run();
    assert_preflight_failure(&harness, &outcome, "not executable");

    // And a folder is not one.
    let harness = Harness::plain();
    let outcome = harness
        .cmd_without_talaria()
        .arg("--talaria")
        .arg(harness.root().join("bin"))
        .run();
    assert_preflight_failure(&harness, &outcome, "a folder");
}

#[test]
fn relative_path_made_absolute() {
    let harness = quick_failure();

    let outcome = harness
        .cmd_without_talaria()
        .arg("--talaria")
        .arg("bin/../bin/talaria")
        .current_dir(harness.root())
        .run();

    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert_eq!(
        first_launch_line(&harness),
        default_launch_line(harness.talaria()),
        "the launch line holds the resolved absolute path in single quotes"
    );
}

#[test]
fn env_variable_used_without_flag() {
    // The variable alone.
    let harness = quick_failure();
    let outcome = harness
        .cmd_without_talaria()
        .env("TALARIA_SMOKE_BIN", harness.talaria())
        .run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert_eq!(
        first_launch_line(&harness),
        default_launch_line(harness.talaria()),
        "TALARIA_SMOKE_BIN names the binary when there is no flag"
    );

    // The flag wins over the variable.
    let harness = quick_failure();
    let other = harness.root().join("bin").join("other-talaria");
    write_executable(&other, "#!/bin/sh\nexit 0\n");
    let outcome = harness
        .cmd_without_talaria()
        .env("TALARIA_SMOKE_BIN", harness.talaria())
        .arg("--talaria")
        .arg(&other)
        .run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    let line = first_launch_line(&harness);
    assert_eq!(line, default_launch_line(&other));
    let from_variable = fs::canonicalize(harness.talaria()).expect("the stand-in exists");
    assert!(
        !line.contains(&format!("'{}'", from_variable.display())),
        "the variable must lose against the flag: {line}"
    );
}

#[test]
fn invalid_option_values_exit_2() {
    let missing_dir = |h: &Harness| h.root().join("no-such-report-dir");
    let cases: [(&str, Vec<String>); 7] = [
        ("--step-scale", vec!["--step-scale=0".to_owned()]),
        ("--step-scale", vec!["--step-scale=-1".to_owned()]),
        ("--step-scale", vec!["--step-scale=nan".to_owned()]),
        ("--step-scale", vec!["--step-scale=inf".to_owned()]),
        ("--step-scale", vec!["--step-scale=abc".to_owned()]),
        ("--deadline-secs", vec!["--deadline-secs=0".to_owned()]),
        ("--report-dir", Vec::new()),
    ];
    for (option, extra) in cases {
        let harness = Harness::plain();
        let mut cmd = harness.cmd().args(&extra);
        if extra.is_empty() {
            cmd = cmd.arg("--report-dir").arg(missing_dir(&harness));
        }
        let outcome = cmd.run();
        let what = format!("{option} {extra:?}");
        assert_preflight_failure(&harness, &outcome, &what);
        assert!(
            everything(&outcome).contains(option),
            "{what}: the sentence names the option; {}",
            explain(&outcome)
        );
    }
}

#[test]
fn tab_list_failure_exits_2() {
    let script = ScriptBuilder::new()
        .fail(FailBuilder::call("tab list", "internal_error").message("server is down"));
    let harness = Harness::new(&script);

    let outcome = harness.cmd().run();

    assert_preflight_failure(&harness, &outcome, "tab list fails");
    let text = everything(&outcome);
    assert!(text.contains("tab list"), "names the command: {text}");
    assert!(text.contains("server is down"), "carries its error: {text}");

    // A herdr that is not there fails the same check.
    let harness = Harness::plain();
    let outcome = harness
        .bare_cmd()
        .arg("run")
        .arg("--herdr")
        .arg(harness.root().join("no-such-herdr"))
        .arg("--talaria")
        .arg(harness.talaria())
        .run();
    assert_preflight_failure(&harness, &outcome, "herdr missing");
    assert!(
        everything(&outcome).contains("tab list"),
        "names the command that failed: {}",
        explain(&outcome)
    );
}

#[test]
fn short_login_exits_2() {
    let harness = Harness::plain();
    let outcome = harness.cmd().user("abc").run();
    assert_preflight_failure(&harness, &outcome, "three letter login");
    assert!(
        everything(&outcome).contains(
            "Your login name is too short to scrub safely from pane text; run the smoke test from an account whose login name has four or more characters."
        ),
        "{}",
        explain(&outcome)
    );

    let harness = Harness::plain();
    let outcome = harness.cmd().no_user().run();
    assert_preflight_failure(&harness, &outcome, "no login at all");
    assert!(
        everything(&outcome).contains(
            "Neither USER nor LOGNAME is set; the smoke test cannot scrub the login name from pane text."
        ),
        "{}",
        explain(&outcome)
    );

    // USER comes first: a short USER is refused whatever LOGNAME says.
    let harness = Harness::plain();
    let outcome = harness
        .cmd()
        .user("abc")
        .env("LOGNAME", "a-longer-login")
        .run();
    assert_preflight_failure(&harness, &outcome, "short USER, long LOGNAME");

    // Four characters are enough, and LOGNAME is only the fallback.
    let harness = quick_failure();
    let outcome = harness.cmd().user("abcd").run();
    assert_eq!(
        outcome.code,
        Some(1),
        "four characters: {}",
        explain(&outcome)
    );
    let harness = quick_failure();
    let outcome = harness
        .cmd()
        .user("a-longer-login")
        .env("LOGNAME", "abc")
        .run();
    assert_eq!(outcome.code, Some(1), "long USER: {}", explain(&outcome));
}

#[test]
fn over_long_launch_line_exits_2() {
    let harness = Harness::plain();

    let outcome = harness.cmd().arg("--model").arg("a".repeat(600)).run();

    assert_preflight_failure(&harness, &outcome, "a 600 character model");
}

#[test]
fn scratch_is_private_with_cache_marker_only() {
    let harness = quick_failure();

    let outcome = harness.cmd().run();

    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    let at_run = harness
        .fake
        .cwd_at_run()
        .unwrap_or_else(|| panic!("the fake never saw `pane run`; {}", explain(&outcome)));
    assert_eq!(at_run.mode, "700", "the scratch project is private");
    assert_eq!(
        at_run.files(),
        [".pytest_cache/CACHEDIR.TAG"],
        "the only file is the cache marker"
    );
    assert_eq!(
        at_run.entries,
        [".pytest_cache/", ".pytest_cache/CACHEDIR.TAG"],
        "the only folder is the cache folder"
    );

    // Its place and name: the system temporary directory, `talaria-smoke-<run id>`.
    let create = harness
        .fake
        .calls_starting(&["tab", "create"])
        .into_iter()
        .next()
        .expect("the tab was created");
    let value_of = |flag: &str| {
        let at = create
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("{flag} missing in {create:?}"));
        create[at + 1].clone()
    };
    let cwd = PathBuf::from(value_of("--cwd"));
    let label = value_of("--label");
    let shape = Regex::new(r"^talaria-smoke-[a-z0-9]{8}$").expect("regex");
    assert!(shape.is_match(&label), "label {label}");
    assert_eq!(
        cwd.file_name().and_then(|n| n.to_str()),
        Some(label.as_str()),
        "the folder and the tab carry the same run id"
    );
    assert_eq!(
        cwd.parent()
            .expect("a parent folder")
            .canonicalize()
            .expect("the parent exists"),
        harness
            .tmp_dir()
            .canonicalize()
            .expect("the temporary directory exists"),
        "the scratch project is created in the system temporary directory"
    );
    assert!(!cwd.exists(), "and it is gone at the end of the run");
}

#[test]
fn exit_codes_and_result_lines() {
    let pass_line = Regex::new(r"^PASS (\S+) \d+\.\d s$").expect("regex");
    let passed = Regex::new(r"^SMOKE_RESULT: PASSED \(elapsed \d+ s\)$").expect("regex");

    // Exit 0 and exit 3 are whole runs; run them side by side to save real time.
    let (zero, three) = std::thread::scope(|scope| {
        let zero = scope.spawn(|| {
            let harness = Harness::from_scenario(&Scenario::happy());
            let outcome = harness.cmd().run();
            (harness, outcome)
        });
        let three = scope.spawn(|| {
            let scenario = Scenario::happy().approval(Approval::RanWithoutCard);
            let harness = Harness::from_scenario(&scenario);
            let outcome = harness.cmd().run();
            (harness, outcome)
        });
        (
            zero.join().expect("the exit 0 run"),
            three.join().expect("the exit 3 run"),
        )
    });

    // 0: every step passed.
    let (harness, outcome) = zero;
    assert_eq!(outcome.code, Some(0), "{}", explain(&outcome));
    let lines = outcome.lines();
    assert!(lines.contains(&"tab: w1:t5"), "{}", explain(&outcome));
    assert!(
        lines.iter().any(|l| l.starts_with("talaria: ")),
        "{}",
        explain(&outcome)
    );
    assert!(
        lines.contains(&format!("herdr: {}", version()).as_str()),
        "{}",
        explain(&outcome)
    );
    for line in lines.iter().filter(|l| l.starts_with("PASS ")) {
        assert!(pass_line.is_match(line), "step line {line:?}");
    }
    assert_eq!(
        step_names(&outcome, "PASS "),
        STEPS,
        "{}",
        explain(&outcome)
    );
    assert!(
        outcome.result_line().is_some_and(|l| passed.is_match(l)),
        "{}",
        explain(&outcome)
    );
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("SMOKE_RESULT:"))
            .count(),
        1
    );
    assert_eq!(outcome.last_line(), outcome.result_line());
    assert_eq!(harness.fake.calls_starting(&["tab", "close"]).len(), 1);
    assert!(harness.scratch_dirs().is_empty());

    // 3: the approval step skipped, everything else passed.
    let (_harness, outcome) = three;
    assert_eq!(outcome.code, Some(3), "{}", explain(&outcome));
    assert!(
        outcome
            .lines()
            .contains(&format!("SKIP approval ({SKIP_REASON})").as_str()),
        "{}",
        explain(&outcome)
    );
    assert_eq!(
        step_names(&outcome, "PASS "),
        ["startup", "plain-answer", "interrupt", "quit", "resume"],
        "a skip does not end the run; {}",
        explain(&outcome)
    );
    assert_eq!(
        outcome.last_line(),
        Some("SMOKE_RESULT: PASSED WITH SKIPS (approval)"),
        "{}",
        explain(&outcome)
    );

    // 1: a failing step ends the run.
    let harness = quick_failure();
    let outcome = harness.cmd().run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome
            .lines()
            .contains(&"FAIL startup (talaria exited with code 7 before showing its screen)"),
        "{}",
        explain(&outcome)
    );
    assert_eq!(
        outcome.last_line(),
        Some("SMOKE_RESULT: FAILED (startup)"),
        "{}",
        explain(&outcome)
    );
    assert!(
        outcome.lines().iter().all(|l| {
            !(l.starts_with("PASS ") || l.starts_with("SKIP ") || l.starts_with("FAIL "))
                || l.contains(" startup")
        }),
        "later steps print nothing; {}",
        explain(&outcome)
    );
    assert!(
        outcome
            .lines()
            .iter()
            .any(|l| l.starts_with("----- pane text (step: startup, pane: first) -----")),
        "the pane text follows the failing step; {}",
        explain(&outcome)
    );
    assert!(harness.scratch_dirs().is_empty());

    // 2: preflight failed.
    let harness = Harness::plain();
    let outcome = harness.cmd().outside_herdr().run();
    assert_preflight_failure(&harness, &outcome, "exit 2");
}

#[test]
fn live_require_turns_a_skip_into_exit_1() {
    let scenario = Scenario::happy().approval(Approval::RanWithoutCard);
    let harness = Harness::from_scenario(&scenario);

    let outcome = harness.cmd().live_require().run();

    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.lines().contains(
            &format!("FAIL approval (skipped: {SKIP_REASON}; TALARIA_LIVE_REQUIRE=1)").as_str()
        ),
        "{}",
        explain(&outcome)
    );
    assert!(
        !outcome.has_line_starting("SKIP "),
        "a skip is a failure here; {}",
        explain(&outcome)
    );
    assert_eq!(
        outcome.last_line(),
        Some("SMOKE_RESULT: FAILED (approval)"),
        "{}",
        explain(&outcome)
    );
    assert_eq!(
        step_names(&outcome, "PASS "),
        ["startup", "plain-answer"],
        "the failing step ends the run; {}",
        explain(&outcome)
    );
    assert_eq!(harness.fake.calls_starting(&["tab", "close"]).len(), 1);
}

#[test]
fn failure_report_is_written_and_scrubbed() {
    // Run 1 names the report folder, run 2 uses the default one.
    for explicit in [true, false] {
        let harness = Harness::plain();
        let tmp = harness.tmp_dir();
        let tmp_real = tmp.canonicalize().expect("tmp exists");
        let home = harness.home_dir();
        let hermes = harness.root().join("hermes-data");
        fs::create_dir(&hermes).expect("hermes home");
        let user = harness.user().to_owned();

        // The pane shows private values, as a shell prompt and a stack trace would.
        let planted = format!(
            "$0\nlogin={user} home={} tmp={}/stuff tmp_real={}/stuff hermes={}\nSMOKE_EXIT:7\n$ ",
            home.display(),
            tmp.display(),
            tmp_real.display(),
            hermes.display(),
        );
        let script = ScriptBuilder::new()
            .on_first(RuleBuilder::run("--no-ensure --profile ").append(&planted));
        fs::write(harness.fake.dir().join("script.json"), script.to_json()).expect("script");
        // The binary under test sits in the temporary directory, so its path is scrubbed too.
        let talaria = tmp.join("stub-talaria");
        write_executable(&talaria, "#!/bin/sh\nexit 0\n");
        let reports = tmp.join("reports");
        fs::create_dir(&reports).expect("report folder");

        let mut cmd = harness
            .cmd_without_talaria()
            .arg("--talaria")
            .arg(&talaria)
            .env("HERMES_HOME", &hermes);
        if explicit {
            cmd = cmd.arg("--report-dir").arg(&reports);
        }
        let outcome = cmd.run();

        let name = if explicit {
            "named folder"
        } else {
            "default folder"
        };
        assert_eq!(outcome.code, Some(1), "{name}: {}", explain(&outcome));
        let report_path = if explicit {
            reports.join("smoke-report.txt")
        } else {
            find_report(&tmp)
        };
        let report = fs::read_to_string(&report_path)
            .unwrap_or_else(|e| panic!("{name}: no report at {report_path:?}: {e}"));

        let needles = [
            ("login name", user),
            ("home", home.display().to_string()),
            ("tmp", tmp.display().to_string()),
            ("real tmp", tmp_real.display().to_string()),
            ("hermes home", hermes.display().to_string()),
        ];
        let printed = everything(&outcome);
        for (kind, needle) in &needles {
            assert!(
                !printed.contains(needle.as_str()),
                "{name}: the {kind} leaked into the output; {}",
                explain(&outcome)
            );
            assert!(
                !report.contains(needle.as_str()),
                "{name}: the {kind} leaked into the report:\n{report}"
            );
        }

        let scrubbed =
            "login=<user> home=<home> tmp=<tmp>/stuff tmp_real=<tmp>/stuff hermes=<hermes-home>";
        for (place, text) in [("output", &outcome.stdout), ("report", &report)] {
            assert!(
                text.contains(scrubbed),
                "{name}: the {place} must show the pane text with placeholders; {}",
                explain(&outcome)
            );
            assert!(
                text.contains("'<tmp>/stub-talaria' --no-ensure"),
                "{name}: the {place} shows the binary path scrubbed"
            );
            assert!(
                text.contains("----- pane text (step: startup, pane: first) -----"),
                "{name}: the {place} marks the pane text"
            );
        }
        assert!(
            report.contains(version()),
            "{name}: the report holds the herdr version:\n{report}"
        );
        assert!(
            report.contains("startup"),
            "{name}: the report names the failing step"
        );
        if !explicit {
            let mention = outcome
                .lines()
                .into_iter()
                .find(|l| l.contains("smoke-report.txt"))
                .unwrap_or_else(|| {
                    panic!("{name}: the report path is printed; {}", explain(&outcome))
                });
            assert!(
                mention.contains("<tmp>"),
                "{name}: the printed report path is scrubbed: {mention}"
            );
        }
    }
}

/// The one `smoke-report.txt` under the temporary directory (the default report folder).
fn find_report(root: &Path) -> PathBuf {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read a folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|n| n == "smoke-report.txt") {
                found.push(path);
            }
        }
    }
    assert_eq!(found.len(), 1, "exactly one report is written: {found:?}");
    found.remove(0)
}
