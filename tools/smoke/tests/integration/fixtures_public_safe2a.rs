//! The phase 2a screens and tmux recordings are public: no path, home directory, login
//! name or profile name of the machine that recorded them may remain.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// The twenty-one screens of the fixture folder (the readme is the twenty-second file).
const SCREENS: [&str; 21] = [
    "tool_shell_failed.txt",
    "tool_edit_diff.txt",
    "palette_open.txt",
    "palette_browse.txt",
    "mention_list.txt",
    "paste_label.txt",
    "file_label.txt",
    "shell_cells.txt",
    "side_answer.txt",
    "side_failed.txt",
    "pager_open.txt",
    "pager_search.txt",
    "pager_nomatch.txt",
    "theme_report.txt",
    "usage_lines.txt",
    "tasks_expanded.txt",
    "tasks_collapsed.txt",
    "tasks_record.txt",
    "pane_get.txt",
    "history_box.txt",
    "editor_result.txt",
];

/// The five tmux recordings (the readme is the sixth file).
const TMUX: [&str; 5] = [
    "version.txt",
    "has_session_ok.txt",
    "has_session_missing.txt",
    "has_session_no_server.txt",
    "kill_session_missing.txt",
];

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
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
    // A login name that is also an ordinary word of the recordings protects nothing.
    for var in ["USER", "LOGNAME"] {
        add(std::env::var(var)
            .ok()
            .filter(|name| !matches!(name.as_str(), "root" | "user")));
    }
    add(Some(std::env::temp_dir().to_string_lossy().into_owned()));
    add(fs::canonicalize(std::env::temp_dir())
        .ok()
        .map(|p| p.to_string_lossy().into_owned()));
    add(Some(env!("CARGO_MANIFEST_DIR").to_owned()));
    add(Some(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ));
    // A scratch folder made now stands for the folders a recording run used.
    let probe = tempfile::tempdir().expect("temporary folder");
    add(Some(probe.path().to_string_lossy().into_owned()));
    add(probe
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned()));
    found.into_iter().collect()
}

/// Profile names in a command line or a JSON field other than `default` and `fixture`.
fn foreign_profiles(text: &str) -> Vec<String> {
    let flag = Regex::new(r#"--profile[ =]+([^\s\"'`,.;:)]+)"#).expect("regex");
    let field = Regex::new(r#""profile"\s*:\s*"([^"]*)""#).expect("regex");
    [flag, field]
        .iter()
        .flat_map(|re| re.captures_iter(text).map(|c| c[1].to_owned()))
        .filter(|name| name != "default" && name != "fixture")
        .collect()
}

#[test]
fn phase2a_fixtures_hold_no_paths_or_names() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let screens = crate_dir.join("tests/fixtures/screens/phase2a");
    let tmux = crate_dir.join("tests/fixtures/tmux");

    for name in SCREENS {
        assert!(
            screens.join(name).is_file(),
            "recorded fixture {name} is missing from tests/fixtures/screens/phase2a"
        );
    }
    for name in TMUX {
        assert!(
            tmux.join(name).is_file(),
            "recorded fixture {name} is missing from tests/fixtures/tmux"
        );
    }
    for readme in [screens.join("README.md"), tmux.join("README.md")] {
        assert!(readme.is_file(), "{} is missing", readme.display());
    }

    let mut to_scan = Vec::new();
    files_under(&screens, &mut to_scan);
    files_under(&tmux, &mut to_scan);

    let scanned: BTreeSet<String> = to_scan
        .iter()
        .map(|p| {
            p.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for name in SCREENS.into_iter().chain(TMUX) {
        assert!(scanned.contains(name), "{name} was not scanned");
    }

    let needles = needles();
    assert!(
        !needles.is_empty(),
        "no needles could be built from the environment"
    );

    // A control: the scanner must see a planted absolute path and a planted name.
    let planted = format!("see /abc/def and {}", needles[0]);
    assert!(
        !smoke::public_safety_violations_2a(&planted, &needles).is_empty(),
        "public_safety_violations_2a missed a planted path and name"
    );

    let mut problems = Vec::new();
    for path in &to_scan {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for violation in smoke::public_safety_violations_2a(&text, &needles) {
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
