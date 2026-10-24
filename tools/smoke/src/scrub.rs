//! Removing private paths and names from everything the run prints.

use std::path::Path;

use regex::Regex;

/// The words a placeholder is built from; a needle inside one of them protects nothing.
const PLACEHOLDER_WORDS: [&str; 5] = ["scratch", "home", "user", "hermes-home", "tmp"];

/// A path: a slash, a word and another slash.
const ABSOLUTE_PATH: &str = r"/[A-Za-z0-9_.\-]+/[A-Za-z0-9_.\-/]*";

/// The private values of one run, and the placeholders that replace them.
pub struct Scrubber {
    pattern: Option<Regex>,
    needles: Vec<(String, String)>,
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
        add(&scratch.to_string_lossy(), "<scratch>");
        if let Ok(real) = scratch.canonicalize() {
            add(&real.to_string_lossy(), "<scratch>");
        }
        add(&tmp.to_string_lossy(), "<tmp>");
        if let Ok(real) = tmp.canonicalize() {
            add(&real.to_string_lossy(), "<tmp>");
        }
        add(home, "<home>");
        add(login, "<user>");
        if let Some(hermes) = hermes_home {
            add(hermes, "<hermes-home>");
        }
        // The longest needle wins, so a path inside a longer path is replaced as one.
        needles.sort_by_key(|needle| std::cmp::Reverse(needle.0.len()));
        let pattern = if needles.is_empty() {
            None
        } else {
            let alternatives: Vec<String> = needles
                .iter()
                .map(|(needle, _)| regex::escape(needle))
                .collect();
            Regex::new(&alternatives.join("|")).ok()
        };
        Self { pattern, needles }
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
            let Some((_, placeholder)) = self
                .needles
                .iter()
                .find(|(needle, _)| needle == whole.as_str())
            else {
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
