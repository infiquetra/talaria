//! A fake tmux reached through a wrapper script, so parallel tests never share environment.

use std::fs;
use std::path::PathBuf;

use super::write_executable;

/// A fake tmux with its own state folder, reached through a wrapper script.
pub struct TmuxFake {
    _tmp: tempfile::TempDir,
    /// The state folder (`log.txt`, `sessions.txt`, buffer copies).
    pub dir: PathBuf,
    /// The wrapper executable to pass as `--tmux`.
    pub wrapper: PathBuf,
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

impl TmuxFake {
    /// The wrapper sets `FAKE_TMUX_DIR` to its own folder and exports the extra `KEY=value`
    /// lines, so they reach the fake.
    pub fn with_env(extra: &[&str]) -> Self {
        let tmp = tempfile::tempdir().expect("temporary folder for the fake");
        let dir = tmp.path().join("tmux");
        fs::create_dir(&dir).expect("fake tmux folder");
        let exe = PathBuf::from(env!("CARGO_BIN_EXE_fake_tmux"));
        let mut body = String::from("#!/bin/sh\n");
        for line in extra {
            body.push_str("export ");
            body.push_str(line);
            body.push('\n');
        }
        body.push_str(&format!(
            "FAKE_TMUX_DIR=\"$(dirname \"$0\")\" exec {} \"$@\"\n",
            shell_quote(&exe.to_string_lossy())
        ));
        let wrapper = dir.join("tmux");
        write_executable(&wrapper, &body);
        TmuxFake {
            _tmp: tmp,
            dir,
            wrapper,
        }
    }

    /// A fake with no extra environment.
    pub fn new() -> Self {
        Self::with_env(&[])
    }

    /// Every call so far, as argument lists, in order.
    pub fn log(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.dir.join("log.txt")) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| serde_json::from_str::<Vec<String>>(line).ok())
            .collect()
    }

    /// True once a call with these first arguments is logged.
    pub fn saw(&self, prefix: &[&str]) -> bool {
        self.log()
            .iter()
            .any(|args| args.len() >= prefix.len() && args.iter().zip(prefix).all(|(a, p)| a == p))
    }
}

impl Default for TmuxFake {
    fn default() -> Self {
        Self::new()
    }
}
