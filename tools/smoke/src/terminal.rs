//! The terminal multiplexer seen through a trait, and its command line implementation.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TabId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTab {
    pub tab: TabId,
    pub pane: PaneId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadSource {
    Visible,
    Recent,
    RecentUnwrapped,
}

impl ReadSource {
    /// The name the command line uses.
    pub fn name(self) -> &'static str {
        match self {
            ReadSource::Visible => "visible",
            ReadSource::Recent => "recent",
            ReadSource::RecentUnwrapped => "recent-unwrapped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TermError {
    #[error("{code}: {message}")]
    Herdr { code: String, message: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("could not parse: {0}")]
    Parse(String),
    #[error("{0}")]
    Io(String),
}

pub trait Terminal {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError>;
    /// Direction right, no focus.
    fn split_pane(&self, pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError>;
    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError>;
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError>;
    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError>;
    fn read(
        &self,
        pane: &PaneId,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError>;
    /// Command lines of the foreground processes.
    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError>;
    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError>;
    fn close_tab(&self, tab: &TabId) -> Result<(), TermError>;
    /// The output of `--version`.
    fn version(&self) -> String;
}

/// The command line of the terminal multiplexer.
pub struct HerdrCli {
    executable: PathBuf,
    workspace: String,
}

impl HerdrCli {
    pub fn new(executable: &Path, workspace: &str) -> Self {
        Self {
            executable: executable.to_path_buf(),
            workspace: workspace.to_owned(),
        }
    }

    /// Runs the command line and returns its standard output.
    fn call(&self, args: &[&str]) -> Result<String, TermError> {
        let output =
            Command::new(&self.executable)
                .args(args)
                .output()
                .map_err(|error| match error.kind() {
                    ErrorKind::NotFound => TermError::NotFound(format!(
                        "the command line {} is not there: {error}",
                        self.executable.display()
                    )),
                    _ => TermError::Io(format!("{}: {error}", self.executable.display())),
                })?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        Err(herdr_error(&String::from_utf8_lossy(&output.stderr)))
    }

    /// Asks the command line whether it answers at all.
    pub fn check(&self) -> Result<(), TermError> {
        self.call(&["tab", "list", "--workspace", &self.workspace])
            .map(|_| ())
    }

    /// The value at a path of the answer, as text.
    fn text_at(answer: &str, path: &[&str]) -> Result<String, TermError> {
        let value: Value = serde_json::from_str(answer)
            .map_err(|error| TermError::Parse(format!("{error}: {answer}")))?;
        let mut current = &value;
        for key in path {
            current = current
                .get(*key)
                .ok_or_else(|| TermError::Parse(format!("{answer} holds no {key}")))?;
        }
        current
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| TermError::Parse(format!("{answer} holds no text at {path:?}")))
    }
}

/// The error of a command that answered with JSON on its standard error.
fn herdr_error(stderr: &str) -> TermError {
    let text = stderr.trim();
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return TermError::Parse(text.to_owned());
    };
    let error = value.get("error");
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("herdr")
        .to_owned();
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(text)
        .to_owned();
    TermError::Herdr { code, message }
}

impl Terminal for HerdrCli {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError> {
        let answer = self.call(&[
            "tab",
            "create",
            "--workspace",
            &self.workspace,
            "--cwd",
            &cwd.to_string_lossy(),
            "--label",
            label,
            "--no-focus",
        ])?;
        Ok(NewTab {
            tab: TabId(Self::text_at(&answer, &["result", "tab", "tab_id"])?),
            pane: PaneId(Self::text_at(&answer, &["result", "root_pane", "pane_id"])?),
        })
    }

    fn split_pane(&self, pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError> {
        let answer = self.call(&[
            "pane",
            "split",
            &pane.0,
            "--direction",
            "right",
            "--no-focus",
            "--cwd",
            &cwd.to_string_lossy(),
        ])?;
        Ok(PaneId(Self::text_at(
            &answer,
            &["result", "pane", "pane_id"],
        )?))
    }

    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        self.call(&["pane", "run", &pane.0, text]).map(|_| ())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        self.call(&["pane", "send-text", &pane.0, text]).map(|_| ())
    }

    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError> {
        let mut args = vec!["pane", "send-keys", pane.0.as_str()];
        args.extend_from_slice(keys);
        self.call(&args).map(|_| ())
    }

    fn read(
        &self,
        pane: &PaneId,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError> {
        let count = lines.map(|lines| lines.to_string());
        let mut args = vec!["pane", "read", pane.0.as_str(), "--source", source.name()];
        if let Some(count) = &count {
            args.push("--lines");
            args.push(count);
        }
        self.call(&args)
    }

    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError> {
        let answer = self.call(&["pane", "process-info", "--pane", &pane.0])?;
        let value: Value = serde_json::from_str(&answer)
            .map_err(|error| TermError::Parse(format!("{error}: {answer}")))?;
        let processes = value
            .get("result")
            .and_then(|result| result.get("process_info"))
            .and_then(|info| info.get("foreground_processes"))
            .and_then(Value::as_array)
            .ok_or_else(|| TermError::Parse(format!("{answer} holds no foreground processes")))?;
        Ok(processes
            .iter()
            .filter_map(|process| process.get("cmdline"))
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect())
    }

    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError> {
        let answer = self.call(&["tab", "list", "--workspace", &self.workspace])?;
        let value: Value = serde_json::from_str(&answer)
            .map_err(|error| TermError::Parse(format!("{error}: {answer}")))?;
        let tabs = value
            .get("result")
            .and_then(|result| result.get("tabs"))
            .and_then(Value::as_array)
            .ok_or_else(|| TermError::Parse(format!("{answer} holds no tab list")))?;
        Ok(tabs.iter().any(|entry| {
            entry
                .get("tab_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == tab.0)
        }))
    }

    fn close_tab(&self, tab: &TabId) -> Result<(), TermError> {
        self.call(&["tab", "close", &tab.0]).map(|_| ())
    }

    fn version(&self) -> String {
        self.call(&["--version"])
            .unwrap_or_default()
            .trim()
            .to_owned()
    }
}
