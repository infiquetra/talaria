//! The recorded fixtures and the readmes are public: no path, home directory, login name or
//! profile name of the machine that recorded them may remain.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

/// The sixteen recorded files of the fixture folder (the readme is the seventeenth file).
const RECORDED: [&str; 16] = [
    "tab_create.json",
    "pane_split.json",
    "pane_run_ok.txt",
    "send_text_ok.txt",
    "send_keys_ok.txt",
    "pane_read_visible.txt",
    "pane_read_recent_unwrapped.txt",
    "pane_process_info_shell.json",
    "pane_process_info_talaria.json",
    "tab_list.json",
    "tab_list_without.json",
    "tab_close_ok.json",
    "version.txt",
    "error_invalid_key.json",
    "error_tab_not_found.json",
    "error_pane_not_found.json",
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
    // A login name that is also an ordinary word of the recordings (`root_pane`) protects nothing.
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
    let flag = Regex::new(r#"--profile[ =]+([^\s"'`,.;:)]+)"#).expect("regex");
    let field = Regex::new(r#""profile"\s*:\s*"([^"]*)""#).expect("regex");
    [flag, field]
        .iter()
        .flat_map(|re| re.captures_iter(text).map(|c| c[1].to_owned()))
        .filter(|name| name != "default" && name != "fixture")
        .collect()
}

#[test]
fn fixtures_and_readme_hold_no_paths_or_names() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures = crate_dir.join("tests/fixtures");
    let herdr = fixtures.join("herdr");

    for name in RECORDED {
        assert!(
            herdr.join(name).is_file(),
            "recorded fixture {name} is missing from tests/fixtures/herdr"
        );
    }
    assert!(
        herdr.join("README.md").is_file(),
        "tests/fixtures/herdr/README.md is missing"
    );
    assert!(
        crate_dir.join("README.md").is_file(),
        "the crate README.md is missing"
    );

    let mut to_scan = Vec::new();
    files_under(&fixtures, &mut to_scan);
    to_scan.push(crate_dir.join("README.md"));

    let scanned: BTreeSet<String> = to_scan
        .iter()
        .map(|p| {
            p.strip_prefix(&herdr)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for name in RECORDED {
        assert!(scanned.contains(name), "{name} was not scanned");
    }
    assert!(
        to_scan.len() > RECORDED.len(),
        "expected the sixteen recordings plus the readmes, found {} files",
        to_scan.len()
    );

    let needles = needles();
    assert!(
        !needles.is_empty(),
        "no needles could be built from the environment"
    );

    // A control: the scanner must see a planted absolute path and a planted name.
    let planted = format!("see /abc/def and {}", needles[0]);
    assert!(
        !smoke::public_safety_violations(&planted, &needles).is_empty(),
        "public_safety_violations missed a planted path and name"
    );

    let mut problems = Vec::new();
    for path in &to_scan {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
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
