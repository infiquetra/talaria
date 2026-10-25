//! Clipboard image readers. Tests supply their own [`CommandRunner`].

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
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
        let mut captured = tempfile::tempfile().map_err(command_error)?;
        let output = match stdout_to {
            Some(path) => std::fs::File::create(path).map_err(command_error)?,
            None => captured.try_clone().map_err(command_error)?,
        };
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(Stdio::null())
            .spawn()
            .map_err(command_error)?;
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(match result {
                        Err(error) => command_error(error),
                        _ => CommandError::Timeout,
                    });
                }
            }
        };
        let mut stdout = Vec::new();
        if stdout_to.is_none() {
            captured.seek(SeekFrom::Start(0)).map_err(command_error)?;
            captured
                .take(IMAGE_READ_LIMIT)
                .read_to_end(&mut stdout)
                .map_err(command_error)?;
        }
        Ok(CommandOutput {
            success: status.success(),
            stdout,
        })
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
#[derive(Clone)]
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
        let Ok(directory) = tempfile::tempdir() else {
            return ClipboardResult::Empty;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .is_err()
            {
                return ClipboardResult::Empty;
            }
        }
        let path = directory.path().join("clip.png");
        match self.platform {
            Platform::Other => ClipboardResult::NoTool,
            Platform::MacOs => self.read_macos(&path),
            Platform::Linux => self.read_linux(&path),
        }
    }

    fn command(
        &self,
        program: &str,
        args: &[&str],
        path: Option<&Path>,
    ) -> Result<CommandOutput, CommandError> {
        self.runner.run(program, args, path, Duration::from_secs(3))
    }

    fn read_macos(&self, path: &Path) -> ClipboardResult {
        if let Some(path_text) = path.to_str() {
            if self
                .command("pngpaste", &[path_text], None)
                .is_ok_and(|out| out.success)
                && let Some(bytes) = read_image(path)
            {
                return ClipboardResult::Image(bytes);
            }
            let _ = std::fs::remove_file(path);
            let info = self.command("osascript", &["-e", "clipboard info"], None);
            let has_bitmap = info.is_ok_and(|out| {
                let text = String::from_utf8_lossy(&out.stdout);
                out.success && (text.contains("«class PNGf»") || text.contains("«class TIFF»"))
            });
            if has_bitmap {
                let escaped = path_text.replace('\\', "\\\\").replace('"', "\\\"");
                let script = format!(
                    "try\n  set imgData to the clipboard as «class PNGf»\n  set f to open for access POSIX file \"{escaped}\" with write permission\n  write imgData to f\n  close access f\non error\n  return \"fail\"\nend try\n"
                );
                if self
                    .command("osascript", &["-e", &script], None)
                    .is_ok_and(|out| {
                        out.success && !String::from_utf8_lossy(&out.stdout).contains("fail")
                    })
                    && let Some(bytes) = read_image(path)
                {
                    return ClipboardResult::Image(bytes);
                }
            }
        }
        ClipboardResult::Empty
    }

    fn read_linux(&self, path: &Path) -> ClipboardResult {
        let mut any_tool = false;
        if self.wayland {
            let types = self.command("wl-paste", &["--list-types"], None);
            any_tool |= !matches!(types, Err(CommandError::NotFound));
            if types.is_ok_and(|out| {
                out.success
                    && String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .any(|line| line == "image/png")
            }) && self
                .command("wl-paste", &["--type", "image/png"], Some(path))
                .is_ok_and(|out| out.success)
                && let Some(bytes) = read_image(path)
            {
                return ClipboardResult::Image(bytes);
            }
            let _ = std::fs::remove_file(path);
        }
        let types = self.command(
            "xclip",
            &["-selection", "clipboard", "-t", "TARGETS", "-o"],
            None,
        );
        any_tool |= !matches!(types, Err(CommandError::NotFound));
        if types.is_ok_and(|out| {
            out.success && String::from_utf8_lossy(&out.stdout).contains("image/png")
        }) && self
            .command(
                "xclip",
                &["-selection", "clipboard", "-t", "image/png", "-o"],
                Some(path),
            )
            .is_ok_and(|out| out.success)
            && let Some(bytes) = read_image(path)
        {
            return ClipboardResult::Image(bytes);
        }
        if any_tool {
            ClipboardResult::Empty
        } else {
            ClipboardResult::NoTool
        }
    }
}

const IMAGE_READ_LIMIT: u64 = 20 * 1024 * 1024 + 1;

fn read_image(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(IMAGE_READ_LIMIT)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty() && bytes != b"fail").then_some(bytes)
}

fn command_error(error: std::io::Error) -> CommandError {
    if error.kind() == std::io::ErrorKind::NotFound {
        CommandError::NotFound
    } else {
        CommandError::Failed(error.to_string())
    }
}
