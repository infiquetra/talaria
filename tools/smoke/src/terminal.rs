//! The terminal multiplexer seen through a trait, and its command line implementation.

use std::path::Path;

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

pub struct HerdrCli {
    _executable: std::path::PathBuf,
    _workspace: String,
}

impl HerdrCli {
    pub fn new(executable: &Path, workspace: &str) -> Self {
        Self {
            _executable: executable.to_path_buf(),
            _workspace: workspace.to_owned(),
        }
    }
}

impl Terminal for HerdrCli {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError> {
        let _ = (label, cwd);
        todo!("smoke")
    }

    fn split_pane(&self, pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError> {
        let _ = (pane, cwd);
        todo!("smoke")
    }

    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        let _ = (pane, text);
        todo!("smoke")
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        let _ = (pane, text);
        todo!("smoke")
    }

    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError> {
        let _ = (pane, keys);
        todo!("smoke")
    }

    fn read(
        &self,
        pane: &PaneId,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError> {
        let _ = (pane, source, lines);
        todo!("smoke")
    }

    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError> {
        let _ = pane;
        todo!("smoke")
    }

    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError> {
        let _ = tab;
        todo!("smoke")
    }

    fn close_tab(&self, tab: &TabId) -> Result<(), TermError> {
        let _ = tab;
        todo!("smoke")
    }

    fn version(&self) -> String {
        todo!("smoke")
    }
}
