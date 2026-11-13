//! The phase 3 screens are public: no path, home directory, login name or profile name
//! of the machine that recorded them may remain.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// The twenty-five screens of the fixture folder.
const SCREENS: [&str; 25] = [
    "fleet-quiet-empty.txt",
    "fleet-quiet-busy.txt",
    "size-narrow.txt",
    "attach-ambiguous.screen.txt",
    "bg-launch-ok.json.txt",
    "bg-launch-refusal.err.txt",
    "worktree-launch.out.txt",
    "fleet-list-rows.json",
    "fleet-list-needs-you.json",
    "fleet-list-unserved.txt",
    "fleet-list-no-gateway.txt",
    "attach-resumed.screen.txt",
    "attach-no-match.screen.txt",
    "local-tasks.screen.txt",
    "local-agents-option.screen.txt",
    "local-processes.screen.txt",
    "local-bg-usage.screen.txt",
    "fleet-view-here.screen.txt",
    "fleet-view-no-match.screen.txt",
    "open-in-pane-notice.screen.txt",
    "open-in-pane-not-in-herdr.screen.txt",
    "needs-you-notice.screen.txt",
    "needs-you-peek.screen.txt",
    "needs-you-card.screen.txt",
    "needs-you-cleared.screen.txt",
];

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Things that must never appear, built from the machine the test runs on.
fn needles() -> Vec<String> {
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut add = |value: Option<String>| {
        if let Some(value) = value {
            let value = value.trim_end_matches('/').to_owned();
            if value.chars().count() >= 4 {
                found.insert(value);
            }
        }
    };
    add(std::env::var("HOME").ok());
    for var in ["USER", "LOGNAME"] {
        add(std::env::var(var)
            .ok()
            .filter(|name| !matches!(name.as_str(), "root" | "user")));
    }
    add(Some(std::env::temp_dir().to_string_lossy().into_owned()));
    add(fs::canonicalize(std::env::temp_dir())
        .ok()
        .map(|path| path.to_string_lossy().into_owned()));
    add(Some(env!("CARGO_MANIFEST_DIR").to_owned()));
    add(Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ));
    let probe = tempfile::tempdir().expect("temporary folder");
    add(Some(probe.path().to_string_lossy().into_owned()));
    add(probe
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned()));
    found.into_iter().collect()
}

/// Profile names in a command line or a JSON field other than `default` and `fixture`.
fn foreign_profiles(text: &str) -> Vec<String> {
    let flag = Regex::new(r#"--profile[ =]+([^\s\"'`,.;:)]+)"#).expect("regex");
    let field = Regex::new(r#""profile"\s*:\s*"([^"]*)""#).expect("regex");
    [flag, field]
        .iter()
        .flat_map(|pattern| {
            pattern
                .captures_iter(text)
                .map(|captures| captures[1].to_owned())
        })
        .filter(|name| name != "default" && name != "fixture")
        .collect()
}

#[test]
fn phase3_fixtures_hold_no_paths_or_names() {
    let screens = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/phase3");
    for name in SCREENS {
        assert!(
            screens.join(name).is_file(),
            "recorded fixture {name} is missing from tests/fixtures/screens/phase3"
        );
    }

    let mut to_scan = Vec::new();
    files_under(&screens, &mut to_scan);
    let scanned: BTreeSet<String> = to_scan
        .iter()
        .map(|path| {
            path.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for name in SCREENS {
        assert!(scanned.contains(name), "{name} was not scanned");
    }

    let needles = needles();
    assert!(
        !needles.is_empty(),
        "no needles could be built from the environment"
    );
    let planted = format!("see /abc/def and {}", needles[0]);
    assert!(
        !smoke::public_safety_violations(&planted, &needles).is_empty(),
        "public_safety_violations missed a planted path and name"
    );

    let mut problems = Vec::new();
    for path in &to_scan {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        for violation in smoke::public_safety_violations(&text, &needles) {
            problems.push(format!("{}: {violation}", path.display()));
        }
        for profile in foreign_profiles(&text) {
            problems.push(format!("{}: profile name {profile}", path.display()));
        }
    }
    assert!(
        problems.is_empty(),
        "fixtures hold private data:\n{}",
        problems.join("\n")
    );
}
