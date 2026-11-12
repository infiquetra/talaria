//! The terminal multiplexer seen through a trait, and its command line implementation.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

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
    /// The agent-state report of the pane (`herdr pane get`), for the pane-state step.
    /// The default errors, so the first suite's fakes compile unchanged.
    fn pane_get(&self, _pane: &PaneId) -> Result<String, TermError> {
        Err(TermError::Parse("pane get is not supported".to_owned()))
    }
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
        let mut attempts = 0u32;
        let output = loop {
            match Command::new(&self.executable).args(args).output() {
                Ok(output) => break output,
                Err(error) if error.raw_os_error() == Some(26) && attempts < 20 => {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => {
                    return Err(match error.kind() {
                        ErrorKind::NotFound => TermError::NotFound(format!(
                            "the command line {} is not there: {error}",
                            self.executable.display()
                        )),
                        _ => TermError::Io(format!("{}: {error}", self.executable.display())),
                    });
                }
            }
        };
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

    /// The agent Herdr recognised in the pane and its state, one per line. The rest of the
    /// answer names folders and titles, which no step reads.
    fn pane_get(&self, pane: &PaneId) -> Result<String, TermError> {
        let answer = self.call(&["pane", "get", &pane.0])?;
        let agent = Self::text_at(&answer, &["result", "pane", "agent"]).unwrap_or_default();
        let status =
            Self::text_at(&answer, &["result", "pane", "agent_status"]).unwrap_or_default();
        Ok(format!("{agent}\n{status}"))
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

/// The command line of herdr core, with independently started split terminals.
pub struct HcCli {
    executable: PathBuf,
    splits: Mutex<Vec<String>>,
}

impl HcCli {
    pub fn new(executable: &Path) -> Self {
        Self {
            executable: executable.to_path_buf(),
            splits: Mutex::new(Vec::new()),
        }
    }

    /// Runs one command without the harness's terminal identity.
    fn call(&self, args: &[&str]) -> Result<String, TermError> {
        let output = Command::new(&self.executable)
            .args(args)
            .env_remove("HC_TERMINAL_ID")
            .env_remove("HC_SCOPE")
            .output()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => TermError::NotFound(format!(
                    "the command line {} is not there: {error}",
                    self.executable.display()
                )),
                _ => TermError::Io(format!("{}: {error}", self.executable.display())),
            })?;
        if !output.status.success() {
            return Err(TermError::Io(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn check(&self) -> Result<(), TermError> {
        self.call(&["ls"]).map(|_| ())
    }

    fn json(&self, args: &[&str]) -> Result<Value, TermError> {
        let answer = self.call(args)?;
        serde_json::from_str(&answer)
            .map_err(|error| TermError::Parse(format!("{error}: {answer}")))
    }

    fn address(&self, args: &[&str]) -> Result<String, TermError> {
        self.json(args)?
            .get("address")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| TermError::Parse("hc run: address is not a string".to_owned()))
    }
}

impl Terminal for HcCli {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError> {
        let address = self.address(&[
            "run",
            "--bg",
            "--no-parent",
            "--name",
            label,
            "--cwd",
            &cwd.to_string_lossy(),
        ])?;
        Ok(NewTab {
            tab: TabId(address.clone()),
            pane: PaneId(address),
        })
    }

    fn split_pane(&self, _pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError> {
        let mut splits = self
            .splits
            .lock()
            .map_err(|error| TermError::Io(error.to_string()))?;
        let address = self.address(&[
            "run",
            "--bg",
            "--no-parent",
            "--cwd",
            &cwd.to_string_lossy(),
        ])?;
        splits.push(address.clone());
        Ok(PaneId(address))
    }

    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        self.call(&["send", &pane.0, text]).map(|_| ())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        if text.is_empty() {
            return Ok(());
        }
        let keys: Vec<String> = text
            .chars()
            .map(|character| match character {
                ' ' => Ok("space".to_owned()),
                'a'..='z'
                | '0'..='9'
                | '\''
                | ','
                | '-'
                | '.'
                | '/'
                | ';'
                | '='
                | '['
                | '\\'
                | ']'
                | '`' => Ok(character.to_string()),
                'A'..='Z' => Ok(format!("shift+{}", character.to_ascii_lowercase())),
                _ => Err(TermError::Parse(format!(
                    "hc keys cannot type '{character}'"
                ))),
            })
            .collect::<Result<_, _>>()?;
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        self.send_keys(pane, &keys)
    }

    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError> {
        let mut args = vec!["keys", pane.0.as_str()];
        args.extend_from_slice(keys);
        self.call(&args).map(|_| ())
    }

    fn read(
        &self,
        pane: &PaneId,
        _source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError> {
        let count = lines.map(|lines| lines.to_string());
        let mut args = vec!["read", pane.0.as_str()];
        if let Some(count) = &count {
            args.extend_from_slice(&["--lines", count]);
        }
        self.call(&args)
    }

    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError> {
        let value = self.json(&["get", &pane.0, "--json"])?;
        let Some(foreground) = value.get("foreground").filter(|value| !value.is_null()) else {
            return Ok(Vec::new());
        };
        let argv = foreground
            .get("argv")
            .and_then(Value::as_array)
            .and_then(|argv| argv.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
            .ok_or_else(|| {
                TermError::Parse("hc get: foreground.argv is not a list of strings".to_owned())
            })?;
        if argv.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![argv.join(" ")])
        }
    }

    fn pane_get(&self, pane: &PaneId) -> Result<String, TermError> {
        let value = self.json(&["get", &pane.0, "--json"])?;
        let status = &value["status"];
        let app = status
            .get("app")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let state = status
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default();
        Ok(format!("{app}\n{state}"))
    }

    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError> {
        let value = self.json(&["ls"])?;
        let terminals = value
            .get("terminals")
            .and_then(Value::as_array)
            .ok_or_else(|| TermError::Parse("hc ls: terminals is not an array".to_owned()))?;
        Ok(terminals.iter().any(|terminal| {
            terminal.get("address").and_then(Value::as_str) == Some(tab.0.as_str())
                && terminal.get("state").and_then(Value::as_str) == Some("live")
        }))
    }

    fn close_tab(&self, tab: &TabId) -> Result<(), TermError> {
        let splits = std::mem::take(
            &mut *self
                .splits
                .lock()
                .map_err(|error| TermError::Io(error.to_string()))?,
        );
        let mut first_error = None;
        for address in splits.iter().map(String::as_str).chain([tab.0.as_str()]) {
            for command in ["kill", "rm"] {
                if let Err(error) = self.call(&[command, address]) {
                    first_error.get_or_insert(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn version(&self) -> String {
        self.call(&["--version"])
            .unwrap_or_default()
            .trim()
            .to_owned()
    }
}
