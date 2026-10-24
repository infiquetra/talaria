//! Removing private paths and names from everything the run prints.

use std::path::Path;

pub struct Scrubber {
    _private: (),
}

impl Scrubber {
    pub fn new(
        home: &str,
        login: &str,
        scratch: &Path,
        hermes_home: Option<&str>,
        tmp: &Path,
    ) -> Self {
        let _ = (home, login, scratch, hermes_home, tmp);
        todo!("smoke")
    }

    pub fn scrub(&self, text: &str) -> String {
        let _ = text;
        todo!("smoke")
    }
}

/// Reports planted absolute paths, home directories and names found in `text`.
pub fn public_safety_violations(text: &str, needles: &[String]) -> Vec<String> {
    let _ = (text, needles);
    todo!("smoke")
}
