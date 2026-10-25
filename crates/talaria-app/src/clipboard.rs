//! Clipboard image readers. Tests supply their own [`CommandRunner`].

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// What one command wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    /// The process exited 0.
    pub success: bool,
    /// Standard output. Empty when it was sent to a file.
    pub stdout: Vec<u8>,
}

/// A command could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The program is not installed.
    NotFound,
    /// It ran longer than its limit.
    Timeout,
    /// It could not be started, or it failed another way.
    Failed(String),
}

/// Runs one clipboard tool. Tests fake this.
pub trait CommandRunner: Send + Sync {
    /// Run `program` with `args`. `stdout_to` receives standard output when set.
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdout_to: Option<&Path>,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError>;
}

/// The process runner used outside tests.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        stdout_to: Option<&Path>,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        let _ = (program, args, stdout_to, timeout);
        unimplemented!()
    }
}

/// Which clipboard tools to try.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// `pngpaste`, then `osascript`.
    MacOs,
    /// `wl-paste` when Wayland is set, then `xclip`.
    Linux,
    /// No clipboard image tool.
    Other,
}

/// Bytes, nothing, or no tool at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardResult {
    /// Image bytes.
    Image(Vec<u8>),
    /// The tools ran and found no image.
    Empty,
    /// Linux has neither `wl-paste` nor `xclip`.
    NoTool,
}

/// Reads one clipboard image through a [`CommandRunner`].
pub struct ClipboardImageReader {
    runner: Arc<dyn CommandRunner>,
    platform: Platform,
    wayland: bool,
}

impl ClipboardImageReader {
    /// A reader with an injected runner.
    #[must_use]
    pub fn new(runner: Arc<dyn CommandRunner>, platform: Platform, wayland: bool) -> Self {
        Self {
            runner,
            platform,
            wayland,
        }
    }

    /// The machine's runner, platform, and whether `WAYLAND_DISPLAY` is set.
    #[must_use]
    pub fn system() -> Self {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let platform = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::Other
        };
        Self::new(Arc::new(SystemRunner), platform, wayland)
    }

    /// Read the clipboard image, or report why there is none.
    #[must_use]
    pub fn read(&self) -> ClipboardResult {
        let _ = (&self.runner, self.platform, self.wayland);
        unimplemented!()
    }
}
