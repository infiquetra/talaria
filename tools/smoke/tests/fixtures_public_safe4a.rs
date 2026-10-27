//! Recorded screen fixtures contain no machine-specific information.
use regex::Regex;
use std::{fs, path::Path};
#[test]
fn phase4a_fixtures_hold_no_paths_or_names() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens/phase4a");
    let paths = fs::read_dir(dir)
        .expect("fixture folder")
        .collect::<Result<Vec<_>, _>>()
        .expect("entries");
    assert!(!paths.is_empty());
    let absolute = Regex::new(r#"(?:^|[\s\"'=])/(?:[A-Za-z0-9_.-]+/)+"#).unwrap();
    let profiles = Regex::new(r#"--profile\s+([^\s\"'\\]+)"#).unwrap();
    let profile_field = Regex::new(r#"\"profile\"\s*:\s*\"([^\"]+)\""#).unwrap();
    let secret =
        Regex::new(r"(?:sk-[A-Za-z0-9_-]{12,}|(?:api[_-]?key|token)\s*[:=]\s*[A-Za-z0-9_-]{12,})")
            .unwrap();
    for entry in paths {
        let text = fs::read_to_string(entry.path()).expect("fixture");
        assert!(text.starts_with("# recorded\n"));
        assert!(!absolute.is_match(&text));
        for var in ["HOME", "USER", "LOGNAME", "HOSTNAME"] {
            if let Ok(value) = std::env::var(var)
                && value.len() >= 4
            {
                assert!(!text.contains(&value));
            }
        }
        for pattern in [&profiles, &profile_field] {
            for capture in pattern.captures_iter(&text) {
                assert!(["fixture", "default", "nosuch", "nosuchprofile"].contains(&&capture[1]));
            }
        }
        assert!(!secret.is_match(&text));
    }
}
