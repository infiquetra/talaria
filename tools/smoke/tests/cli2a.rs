//! The built `smoke` binary against the fake multiplexer and the fake tmux: the new
//! options, preflight, result lines and exit codes, the scratch project and the failure
//! report. All fail until the dispatch in `src/main.rs` and the options exist.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use regex::Regex;
use smoke::Suite;
use support::scripts2a::{Shapes, Step, script_2a};
use support::tmux_fake::TmuxFake;
use support::{Harness, Outcome, RuleBuilder, Scenario, ScriptBuilder, Startup, write_executable};

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
fn help_lists_suite_and_tmux_options() {
    let harness = Harness::plain();
    let outcome = harness.bare_cmd().arg("run").arg("--help").run();
    assert_eq!(outcome.code, Some(0), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("--suite"),
        "help lists --suite:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("--tmux"),
        "help lists --tmux:\n{}",
        outcome.stdout
    );
}

#[test]
fn default_suite_is_phase1_and_its_output_is_unchanged() {
    let default_harness = quick_failure();
    let default = default_harness.cmd().run();
    assert_eq!(default.code, Some(1), "{}", explain(&default));
    assert!(
        default.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "the default run behaves as the first suite: {}",
        explain(&default)
    );
    let explicit_harness = quick_failure();
    let explicit = explicit_harness.cmd().arg("--suite").arg("phase1").run();
    // The `report:` line holds a random run id, and each harness has its own temporary
    // folder; everything else is byte for byte.
    let stable = |harness: &Harness, text: &str| {
        let tmp = harness.root().to_string_lossy().into_owned();
        let real = harness
            .root()
            .canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        text.lines()
            .filter(|line| !line.starts_with("report: "))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(&real, "<tmp>")
            .replace(&tmp, "<tmp>")
    };
    assert_eq!(explicit.code, default.code, "{}", explain(&explicit));
    assert_eq!(
        stable(&explicit_harness, &explicit.stdout),
        stable(&default_harness, &default.stdout),
        "no --suite equals --suite phase1"
    );
    assert_eq!(
        stable(&explicit_harness, &explicit.stderr),
        stable(&default_harness, &default.stderr),
        "the error stream is unchanged as well"
    );
}

#[test]
fn phase2a_without_working_tmux_exits_2_and_creates_nothing() {
    let harness = Harness::plain();
    let outcome = harness
        .cmd()
        .arg("--suite")
        .arg("phase2a")
        .arg("--tmux")
        .arg("/no/such/tmux")
        .run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    assert!(
        everything(&outcome).contains("tmux was not found or did not run: /no/such/tmux"),
        "one sentence says what is missing: {}",
        explain(&outcome)
    );
    assert!(
        !everything(&outcome).contains("SMOKE_RESULT"),
        "a preflight failure prints no result line: {}",
        explain(&outcome)
    );
    assert!(
        !harness.fake.saw(&["tab", "create"]),
        "no tab may be created; log {:?}",
        harness.fake.log()
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "no scratch project may be created: {:?}",
        harness.scratch_dirs()
    );
}

#[test]
fn scratch_path_with_space_or_quote_exits_2_for_phase2a_only() {
    for name in ["with space", "with'quote"] {
        let tmux = TmuxFake::new();
        let harness = Harness::plain();
        let spaced = harness.tmp_dir().join(name);
        fs::create_dir(&spaced).expect("spaced temporary folder");
        let outcome = harness
            .cmd()
            .arg("--suite")
            .arg("phase2a")
            .arg("--tmux")
            .arg(&tmux.wrapper)
            .env("TMPDIR", &spaced)
            .run();
        assert_eq!(outcome.code, Some(2), "{name}: {}", explain(&outcome));
        assert!(
            everything(&outcome).contains(name),
            "{name}: the failure names the path: {}",
            explain(&outcome)
        );
        assert!(
            !everything(&outcome).contains("SMOKE_RESULT"),
            "{name}: a preflight failure prints no result line: {}",
            explain(&outcome)
        );
        assert!(
            !harness.fake.saw(&["tab", "create"]),
            "{name}: no tab may be created; log {:?}",
            harness.fake.log()
        );

        // The first suite does not refuse the same path: its output is unchanged. Each
        // run uses its own temporary folder and report id, so both are normalized away.
        let normalize = |harness: &Harness, text: &str| {
            let tmp = harness.root().to_string_lossy().into_owned();
            let real = harness
                .root()
                .canonicalize()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            text.lines()
                .filter(|line| !line.starts_with("report: "))
                .collect::<Vec<_>>()
                .join("\n")
                .replace(&real, "<tmp>")
                .replace(&tmp, "<tmp>")
        };
        let spaced_harness = quick_failure();
        let spaced_run = spaced_harness.cmd().env("TMPDIR", &spaced).run();
        let plain_harness = quick_failure();
        let plain_run = plain_harness.cmd().run();
        assert_eq!(
            spaced_run.code,
            plain_run.code,
            "{name}: {}",
            explain(&spaced_run)
        );
        assert_eq!(
            normalize(&spaced_harness, &spaced_run.stdout),
            normalize(&plain_harness, &plain_run.stdout),
            "{name}: the first suite output is unchanged"
        );
        assert_eq!(
            normalize(&spaced_harness, &spaced_run.stderr),
            normalize(&plain_harness, &plain_run.stderr),
            "{name}: the first suite error stream is unchanged"
        );
    }
}

#[test]
fn unknown_suite_exits_2_with_the_parse_text() {
    let harness = Harness::plain();
    let outcome = harness.cmd().arg("--suite").arg("bogus").run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    assert!(
        everything(&outcome).contains("unknown suite 'bogus'; use phase1 or phase2a"),
        "the parse text is shown: {}",
        explain(&outcome)
    );
}

#[test]
fn phase2a_deadline_defaults_to_1800_and_an_explicit_value_wins() {
    assert_eq!(Suite::Phase2a.default_deadline_secs(), 1800);
    assert_eq!(Suite::Phase1.default_deadline_secs(), 300);

    let tmux = TmuxFake::new();
    let harness = Harness::plain();
    let outcome = harness
        .cmd()
        .arg("--suite")
        .arg("phase2a")
        .arg("--tmux")
        .arg(&tmux.wrapper)
        .arg("--deadline-secs")
        .arg("3")
        .arg("--step-scale")
        .arg("100")
        .run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("deadline exceeded in step startup"),
        "the explicit deadline wins: {}",
        explain(&outcome)
    );
}

#[test]
fn scratch_folder_is_private_and_holds_the_launch_and_editor_scripts() {
    let shapes = Shapes {
        checklist_end_polls: Some(40),
        ..Default::default()
    };
    let script = script_2a(&shapes, Step::TmuxPane);
    let harness = Harness::new(&script);
    let tmux = TmuxFake::with_env(&["FAKE_TMUX_NEVER_UP=1"]);
    let mut running = harness
        .cmd()
        .arg("--suite")
        .arg("phase2a")
        .arg("--tmux")
        .arg(&tmux.wrapper)
        .arg("--step-scale")
        .arg("100")
        .spawn();
    let pid = running.pid();
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut launch: Option<PathBuf> = None;
    while Instant::now() < deadline {
        for dir in harness.scratch_dirs() {
            let candidate = dir.join("launch.sh");
            if candidate.is_file() {
                launch = Some(candidate);
                break;
            }
        }
        if launch.is_some() || has_ended(pid) {
            break;
        }
        sleep(Duration::from_millis(100));
    }
    let launch = match launch {
        Some(path) => path,
        None => {
            running.kill();
            let outcome = running.finish();
            panic!(
                "smoke never wrote launch.sh (it ended or hung before the tmux section)\n{}",
                explain(&outcome)
            );
        }
    };
    let scratch = launch.parent().expect("the scratch project").to_path_buf();
    let mode_of = |path: &Path| {
        format!(
            "{:o}",
            fs::metadata(path)
                .unwrap_or_else(|e| panic!("cannot stat {}: {e}", path.display()))
                .permissions()
                .mode()
                & 0o777
        )
    };
    assert_eq!(mode_of(&scratch), "700", "the scratch project is private");
    let editor = scratch.join("smoke-editor.sh");
    assert_eq!(mode_of(&editor), "700", "the editor script is private");
    assert!(
        fs::read_to_string(&editor)
            .expect("read the editor script")
            .contains("printf ' plus-two' >> \"$1\""),
        "the editor script appends the marker"
    );
    assert_eq!(mode_of(&launch), "700", "the launch script is private");
    let launch_text = fs::read_to_string(&launch).expect("read the launch script");
    let mut lines = launch_text.lines();
    assert_eq!(lines.next(), Some("#!/bin/sh"));
    assert_eq!(
        lines.next(),
        Some("unset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH"),
        "the second line unsets the Herdr variables"
    );
    assert!(
        launch_text.contains("--no-ensure") && launch_text.contains("/smoke-editor.sh'"),
        "the launch line sets the editor and never ensures: {launch_text:?}"
    );
    assert!(
        launch_text.contains("EDITOR='"),
        "the editor rides on the launch line: {launch_text:?}"
    );
    assert!(
        launch_text.contains("SMOKE_EXIT:$?"),
        "the launch line reports the exit: {launch_text:?}"
    );
    // The setup writes alpha and beta; by the tmux section the tool-edit step has turned
    // beta into gamma.
    let notes = fs::read_to_string(scratch.join("notes.txt")).expect("notes.txt");
    let notes: Vec<&str> = notes.lines().collect();
    assert!(
        notes == ["alpha", "beta"] || notes == ["alpha", "gamma"],
        "the setup writes the notes file: {notes:?}"
    );
    let calc = fs::read_to_string(scratch.join("src/calc.py")).expect("src/calc.py");
    assert!(
        calc.contains("def average(values): return sum(values) / len(values)"),
        "the setup writes the source file"
    );
    let keep = fs::read_to_string(scratch.join("keepme/keep.txt")).expect("keepme/keep.txt");
    assert_eq!(keep.trim(), "keep", "the setup writes the kept file");
    let marker = Regex::new(r"^the marker is (\w+)-(\d+)$").expect("regex");
    for big in ["big1.txt", "big2.txt"] {
        let text = fs::read_to_string(scratch.join(big)).expect(big);
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 120, "{big} holds 120 lines");
        assert!(
            marker.is_match(rows[86]),
            "{big} line 87 holds the marker: {:?}",
            rows[86]
        );
    }
    let mut drops = Vec::new();
    for entry in fs::read_dir(harness.tmp_dir()).expect("read the temporary folder") {
        let folder = entry.expect("a directory entry").path();
        if folder
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("-drop"))
        {
            drops.push(folder);
        }
    }
    assert_eq!(drops.len(), 1, "one drop folder: {drops:?}");
    let mut secrets = Vec::new();
    for entry in fs::read_dir(&drops[0]).expect("read the drop folder") {
        secrets.push(entry.expect("a directory entry").path());
    }
    assert_eq!(secrets.len(), 1, "one dropped file: {secrets:?}");
    let name = secrets[0]
        .file_name()
        .expect("a file name")
        .to_string_lossy()
        .into_owned();
    assert!(
        Regex::new(r"^notes-[0-9a-f]{8}\.txt$")
            .expect("regex")
            .is_match(&name),
        "the dropped file is uniquely named: {name}"
    );
    let secret = fs::read_to_string(&secrets[0]).expect("read the dropped file");
    assert!(
        Regex::new(r"^The secret word is \w+-\d+\.$")
            .expect("regex")
            .is_match(secret.trim()),
        "the dropped file holds the secret: {secret:?}"
    );
    running.kill();
}

#[test]
fn failure_report_names_three_panes_and_is_scrubbed() {
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
    let script =
        ScriptBuilder::new().on_first(RuleBuilder::run("--no-ensure --profile ").append(&planted));
    fs::write(harness.fake.dir().join("script.json"), script.to_json()).expect("script");
    let talaria = tmp.join("stub-talaria");
    write_executable(&talaria, "#!/bin/sh\nexit 0\n");
    let reports = tmp.join("reports");
    fs::create_dir(&reports).expect("report folder");
    let tmux = TmuxFake::new();

    let outcome = harness
        .cmd_without_talaria()
        .arg("--talaria")
        .arg(&talaria)
        .arg("--suite")
        .arg("phase2a")
        .arg("--tmux")
        .arg(&tmux.wrapper)
        .arg("--report-dir")
        .arg(&reports)
        .env("HERMES_HOME", &hermes)
        .run();

    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    let report_path = reports.join("smoke-report.txt");
    let report = fs::read_to_string(&report_path)
        .unwrap_or_else(|e| panic!("no report at {report_path:?}: {e}"));
    for pane in ["first", "second", "third"] {
        assert!(
            report.contains(&format!("pane: {pane}")),
            "the failure report names {pane}:\n{report}"
        );
    }
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
            "the {kind} leaked into the output; {}",
            explain(&outcome)
        );
        assert!(
            !report.contains(needle.as_str()),
            "the {kind} leaked into the report:\n{report}"
        );
    }
}
