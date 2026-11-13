//! Fixture text is safe to publish.
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn inspect(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("fixture folder") {
        let entry = entry.expect("fixture entry");
        let kind = entry.file_type().expect("fixture type");
        assert!(!kind.is_symlink(), "fixture must not be a symlink");
        if kind.is_dir() {
            files.extend(inspect(&entry.path()));
        } else {
            assert!(kind.is_file());
            files.push(entry.path());
        }
    }
    files
}

#[test]
fn phase5_fixtures_hold_no_paths_or_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/phase5");
    let files = inspect(&dir);
    assert!(!files.is_empty());
    let absolute = Regex::new(r#"(?:^|[\s\"'=])/(?:[A-Za-z0-9_.-]+/)+"#).unwrap();
    let windows = Regex::new(r#"(?:[A-Za-z]:\\|\\\\)[^\s]+"#).unwrap();
    let home = Regex::new(r#"(?:^|[\s\"'=])~/"#).unwrap();
    let profiles = Regex::new(r"--profile(?:\s+|=)(\S+)").unwrap();
    let profile_field = Regex::new(r#"\"profile\"\s*:\s*\"([^\"]+)\""#).unwrap();
    let profile_title = Regex::new(r"profile ([a-z0-9][a-z0-9_-]{0,63})").unwrap();
    let secret =
        Regex::new(r"(?:sk-[A-Za-z0-9_-]{12,}|(?:api[_-]?key|token)\s*[:=]\s*[A-Za-z0-9_-]{12,})")
            .unwrap();
    for file in files {
        let text = fs::read_to_string(&file).expect("fixture text");
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(!absolute.is_match(&text), "{name}: absolute path");
        assert!(!windows.is_match(&text), "{name}: platform path");
        assert!(!home.is_match(&text), "{name}: home directory");
        assert!(!secret.is_match(&text), "{name}: secret shape");
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
        if name.ends_with(".txt") {
            assert!(
                text.starts_with("# synthetic\n") || text.starts_with("# recorded\n"),
                "{name}: fixture origin"
            );
        }
    }
}
