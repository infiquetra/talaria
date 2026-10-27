//! Synthetic screen fixtures contain no machine-specific information.

use regex::Regex;
use std::{fs, path::Path};

fn inspect(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("fixture folder") {
        let entry = entry.expect("fixture entry");
        let kind = entry.file_type().expect("fixture type");
        assert!(
            !kind.is_symlink(),
            "fixture must be a regular file or folder"
        );
        if kind.is_dir() {
            files.extend(inspect(&entry.path()));
        } else {
            assert!(kind.is_file(), "fixture must be a regular file");
            files.push(entry.path());
        }
    }
    files
}

#[test]
fn phase4b_fixtures_hold_no_paths_or_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/phase4b");
    let files = inspect(&dir);
    assert!(!files.is_empty(), "screen fixtures must exist");
    let absolute = Regex::new(r#"(?:^|[\s\"'=])/(?:[A-Za-z0-9_.-]+/)+"#).unwrap();
    let windows = Regex::new(r#"(?:[A-Za-z]:\\|\\\\)[^\s]+"#).unwrap();
    let home = Regex::new(r#"(?:^|[\s\"'=])~/"#).unwrap();
    let profiles = Regex::new(r"--profile(?:\s+|=)(\S+)").unwrap();
    let profile_field = Regex::new(r#"\"profile\"\s*:\s*\"([^\"]+)\""#).unwrap();
    let profile_title = Regex::new(r"profile ([a-z0-9][a-z0-9_-]{0,63})").unwrap();
    let secret =
        Regex::new(r"(?:sk-[A-Za-z0-9_-]{12,}|(?:api[_-]?key|token)\s*[:=]\s*[A-Za-z0-9_-]{12,})")
            .unwrap();
    let sentinel = "sk-or-v1-talaria-sentinel-0000000000000000";
    for file in files {
        let text = fs::read_to_string(&file).expect("fixture text");
        let name = file.file_name().expect("fixture name").to_string_lossy();
        if file.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("recorded")) {
            assert!(text.starts_with("# recorded\n"), "{name}: recorded header");
        } else {
            assert!(
                text.starts_with("# synthetic\n"),
                "{name}: synthetic header"
            );
        }
        assert!(!absolute.is_match(&text), "{name}: absolute path");
        assert!(!windows.is_match(&text), "{name}: platform path");
        assert!(!home.is_match(&text), "{name}: home directory");
        for variable in ["HOME", "USER", "LOGNAME", "HOST", "HOSTNAME", "USERPROFILE"] {
            if let Ok(value) = std::env::var(variable)
                && value.len() >= 4
            {
                assert!(
                    !text.to_lowercase().contains(&value.to_lowercase()),
                    "{name}: machine value"
                );
            }
        }
        for pattern in [&profiles, &profile_field, &profile_title] {
            for capture in pattern.captures_iter(&text) {
                assert!(
                    ["default", "fixture", "nosuch", "nosuchprofile"].contains(&&capture[1]),
                    "{name}: unexpected profile"
                );
            }
        }
        if text.contains(sentinel) {
            assert_eq!(name, "configure-secret-refused-input.txt");
            assert_eq!(text.matches(sentinel).count(), 1);
            assert_eq!(
                text.lines().find(|line| line.contains(sentinel)),
                Some(format!("> /configure use my openrouter key {sentinel}").as_str())
            );
        }
        assert!(
            !secret.is_match(&text.replace(sentinel, "<typed-secret>")),
            "{name}: secret shape"
        );
    }
}
