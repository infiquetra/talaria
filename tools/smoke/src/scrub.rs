//! Removing private paths and names from everything the run prints.

use std::path::Path;

use regex::Regex;

/// The words a placeholder is built from; a needle inside one of them protects nothing.
const PLACEHOLDER_WORDS: [&str; 5] = ["scratch", "home", "user", "hermes-home", "tmp"];

/// A path: a slash, a word and another slash.
const ABSOLUTE_PATH: &str = r"/[A-Za-z0-9_.\-]+/[A-Za-z0-9_.\-/]*";

/// A home directory, with or without anything below it.
const HOME_DIRECTORY: &str = r"/(?:Users|home)/[A-Za-z0-9_.\-]+";

/// A profile named on a launch line: `--profile fixture`.
const PROFILE_FLAG: &str = r#"--profile[ =]+([^\s"'`,.;:)]+)"#;

/// A profile named in a JSON field: `"profile": "name"`.
const PROFILE_FIELD: &str = r#""profile"\s*:\s*"([^"]*)""#;

/// The profile names a public recording may hold.
const PUBLIC_PROFILES: [&str; 2] = ["default", "fixture"];

/// The private values of one run, and the placeholders that replace them.
pub struct Scrubber {
    pattern: Option<Regex>,
    needles: Vec<(String, String)>,
    scratch_suffix: Option<String>,
}

impl Scrubber {
    /// The needles of this run: the scratch project, the temporary directory, the home
    /// directory, the login name and the Hermes home, each with its resolved real path.
    pub fn new(
        home: &str,
        login: &str,
        scratch: &Path,
        hermes_home: Option<&str>,
        tmp: &Path,
    ) -> Self {
        let mut needles: Vec<(String, String)> = Vec::new();
        let mut add = |value: &str, placeholder: &str| {
            if !keeps(value) {
                return;
            }
            needles.push((value.to_owned(), placeholder.to_owned()));
        };
        add(trim_separator(&scratch.to_string_lossy()), "<scratch>");
        if let Ok(real) = scratch.canonicalize() {
            add(trim_separator(&real.to_string_lossy()), "<scratch>");
        }
        add(trim_separator(&tmp.to_string_lossy()), "<tmp>");
        if let Ok(real) = tmp.canonicalize() {
            add(trim_separator(&real.to_string_lossy()), "<tmp>");
        }
        add(home, "<home>");
        add(login, "<user>");
        if let Some(hermes) = hermes_home {
            add(hermes, "<hermes-home>");
        }
        // The longest needle wins, so a path inside a longer path is replaced as one.
        needles.sort_by_key(|needle| std::cmp::Reverse(needle.0.len()));
        let scratch_suffix = scratch
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| keeps(name))
            .map(|name| format!("/{name}"));
        // Exact child paths keep their more specific placeholders; shortened scratch paths
        // must still match before a home or temporary prefix can consume them.
        let scratch_child = scratch_suffix.as_ref().map(|suffix| format!("{suffix}/"));
        let (children, other): (Vec<_>, Vec<_>) = needles.iter().partition(|(needle, _)| {
            scratch_child
                .as_deref()
                .is_some_and(|child| needle.contains(child))
        });
        let mut alternatives: Vec<_> = children
            .iter()
            .map(|(needle, _)| regex::escape(needle))
            .collect();
        if let Some(suffix) = &scratch_suffix {
            alternatives.push(format!(r"[A-Za-z0-9_.\-/]+{}", regex::escape(suffix)));
        }
        alternatives.extend(other.iter().map(|(needle, _)| regex::escape(needle)));
        let pattern = if alternatives.is_empty() {
            None
        } else {
            Regex::new(&alternatives.join("|")).ok()
        };
        Self {
            pattern,
            needles,
            scratch_suffix,
        }
    }

    /// Replaces every needle in one pass; the output is never read again.
    pub fn scrub(&self, text: &str) -> String {
        let Some(pattern) = &self.pattern else {
            return text.to_owned();
        };
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for found in pattern.captures_iter(text) {
            let whole = found.get(0).expect("the whole match");
            let placeholder = self
                .needles
                .iter()
                .find(|(needle, _)| needle == whole.as_str())
                .map(|(_, placeholder)| placeholder.as_str())
                .or_else(|| {
                    self.scratch_suffix
                        .as_deref()
                        .is_some_and(|suffix| whole.as_str().ends_with(suffix))
                        .then_some("<scratch>")
                });
            let Some(placeholder) = placeholder else {
                continue;
            };
            out.push_str(&text[last..whole.start()]);
            out.push_str(placeholder);
            last = whole.end();
        }
        out.push_str(&text[last..]);
        out
    }
}

fn trim_separator(value: &str) -> &str {
    if value.len() > 1 {
        value.trim_end_matches('/')
    } else {
        value
    }
}

/// Whether a needle is worth replacing: long enough, no placeholder of its own, and not
/// part of a placeholder word.
fn keeps(value: &str) -> bool {
    if value.chars().count() < 4 {
        return false;
    }
    if value.contains('<') || value.contains('>') {
        return false;
    }
    let lower = value.to_lowercase();
    !PLACEHOLDER_WORDS.iter().any(|word| word.contains(&lower))
}

/// Reports planted absolute paths, home directories and names found in `text`.
pub fn public_safety_violations(text: &str, needles: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(pattern) = Regex::new(ABSOLUTE_PATH) {
        for path in pattern.find_iter(text) {
            found.push(format!("an absolute path: {}", path.as_str()));
        }
    }
    for needle in needles {
        if needle.chars().count() >= 4 && text.contains(needle.as_str()) {
            found.push(format!("a private name: {needle}"));
        }
    }
    found
}

/// Reports scrub failures in the text of one phase 2a fixture file: planted absolute paths,
/// home directories, login names and profile names other than `default` and `fixture`.
pub fn public_safety_violations_2a(text: &str, needles: &[String]) -> Vec<String> {
    let mut found = public_safety_violations(text, needles);
    if let Ok(pattern) = Regex::new(HOME_DIRECTORY) {
        for home in pattern.find_iter(text) {
            found.push(format!("a home directory: {}", home.as_str()));
        }
    }
    for source in [PROFILE_FLAG, PROFILE_FIELD] {
        let Ok(pattern) = Regex::new(source) else {
            continue;
        };
        for captures in pattern.captures_iter(text) {
            let name = &captures[1];
            if !PUBLIC_PROFILES.contains(&name) {
                found.push(format!("a profile name: {name}"));
            }
        }
    }
    found
}
