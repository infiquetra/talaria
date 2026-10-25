//! The tmux command line the phase 2a suite drives: pastes, session watches and cleanup.
//!
//! Pastes go through a tmux buffer and `paste-buffer -p`, which brackets the text only when
//! the program in the pane asked for bracketed paste, as a terminal's own paste does
//! (issue #229).

use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use regex::Regex;

use crate::{CleanupError, TabId, TermError};

/// The start of the scratch folder's name; the rest is the run id.
const SCRATCH_PREFIX: &str = "talaria-smoke-";

/// The file the pasted text is written to before tmux loads it.
const PASTE_FILE: &str = "paste-buffer.txt";

/// The marker line the launch script writes when the wrapped talaria ends.
const EXIT_MARKER: &str = "SMOKE_EXIT:";

/// What tmux says when a session or the whole server is already gone.
const GONE_MESSAGES: [&str; 3] = [
    "can't find session",
    "error connecting to",
    "no server running",
];

/// What the suite needs from tmux, behind one trait for the fake.
pub trait Paster {
    /// Bracketed paste of the text into the session's pane.
    fn paste(&self, session: &str, text: &str) -> Result<(), TermError>;
    /// Whether the named session exists.
    fn session_exists(&self, session: &str) -> Result<bool, TermError>;
    /// Ends the session; a missing session or server is success.
    fn kill_session(&self, session: &str) -> Result<(), TermError>;
    /// The exit code of the wrapped talaria, once the marker file says it ended, else `None`.
    fn exit_code(&self) -> Option<i32>;
    /// The output of `tmux -V`.
    fn version(&self) -> String;
}

/// The real tmux command line.
pub struct TmuxCli {
    /// The tmux executable.
    executable: PathBuf,
    /// The marker file the wrapped talaria writes on exit.
    marker: PathBuf,
    /// The scratch folder the run owns.
    scratch: PathBuf,
    /// The name of the tmux buffer the run pastes through: `smoke-<run id>`.
    buffer: String,
}

impl TmuxCli {
    /// The only constructor: the tmux executable, the marker file path and the scratch folder.
    pub fn new(executable: &Path, marker: &Path, scratch: &Path) -> Self {
        let folder = scratch
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let run_id = folder.strip_prefix(SCRATCH_PREFIX).unwrap_or(&folder);
        Self {
            executable: executable.to_path_buf(),
            marker: marker.to_path_buf(),
            scratch: scratch.to_path_buf(),
            buffer: format!("smoke-{run_id}"),
        }
    }

    /// Runs tmux with these arguments and returns what it printed.
    fn call(&self, args: &[&str]) -> Result<Output, TermError> {
        Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => TermError::NotFound(format!("tmux was not found: {error}")),
                _ => TermError::Io(format!("tmux could not be started: {error}")),
            })
    }

    /// Runs tmux and turns a non-zero exit into an error holding tmux's scrubbed message.
    fn call_checked(&self, args: &[&str]) -> Result<Output, TermError> {
        let output = self.call(args)?;
        if output.status.success() {
            return Ok(output);
        }
        let command = args.first().copied().unwrap_or_default();
        Err(TermError::Io(format!(
            "tmux {command} failed: {}",
            self.failure_text(&output)
        )))
    }

    /// What tmux said on its error output, or its exit status when it said nothing.
    fn failure_text(&self, output: &Output) -> String {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim();
        if said.is_empty() {
            return format!("{}", output.status);
        }
        self.scrubbed(said)
    }

    /// The text with the scratch folder and every other absolute path replaced.
    fn scrubbed(&self, text: &str) -> String {
        static PATH: OnceLock<Option<Regex>> = OnceLock::new();
        let mut text = text.to_owned();
        let mut folders = vec![self.scratch.clone()];
        if let Ok(real) = self.scratch.canonicalize() {
            folders.push(real);
        }
        for folder in folders {
            let folder = folder.to_string_lossy().into_owned();
            if !folder.is_empty() {
                text = text.replace(&folder, "<scratch>");
            }
        }
        let path = PATH.get_or_init(|| Regex::new(r"/[A-Za-z0-9_.\-]+/[A-Za-z0-9_.\-/]*").ok());
        match path {
            Some(path) => path.replace_all(&text, "<path>").into_owned(),
            None => text,
        }
    }

    /// Writes the text to a private file in the scratch folder.
    fn write_paste_file(&self, text: &str) -> Result<PathBuf, TermError> {
        let file = self.scratch.join(PASTE_FILE);
        let _ = fs::remove_file(&file);
        let mut handle = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .map_err(|error| TermError::Io(format!("cannot write the paste file: {error}")))?;
        handle
            .write_all(text.as_bytes())
            .map_err(|error| TermError::Io(format!("cannot write the paste file: {error}")))?;
        Ok(file)
    }

    /// The two tmux calls of a paste: load the file into the buffer, then paste the buffer.
    fn load_and_paste(&self, session: &str, file: &Path) -> Result<(), TermError> {
        let file = file.to_string_lossy();
        self.call_checked(&["load-buffer", "-b", &self.buffer, &file])?;
        let pasted = self.call_checked(&[
            "paste-buffer",
            "-p",
            "-r",
            "-d",
            "-b",
            &self.buffer,
            "-t",
            session,
        ]);
        if pasted.is_err() {
            // The buffer is only deleted by a paste that worked.
            let _ = self.call(&["delete-buffer", "-b", &self.buffer]);
        }
        pasted.map(|_| ())
    }
}

impl Paster for TmuxCli {
    fn paste(&self, session: &str, text: &str) -> Result<(), TermError> {
        let file = self.write_paste_file(text)?;
        let result = self.load_and_paste(session, &file);
        let _ = fs::remove_file(&file);
        result
    }

    fn session_exists(&self, session: &str) -> Result<bool, TermError> {
        let output = self.call(&["has-session", "-t", session])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(TermError::Io(format!(
                "tmux has-session failed: {}",
                self.failure_text(&output)
            ))),
        }
    }

    fn kill_session(&self, session: &str) -> Result<(), TermError> {
        let output = self.call(&["kill-session", "-t", session])?;
        if output.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&output.stderr);
        if GONE_MESSAGES.iter().any(|gone| said.contains(gone)) {
            return Ok(());
        }
        Err(TermError::Io(format!(
            "tmux kill-session failed: {}",
            self.failure_text(&output)
        )))
    }

    fn exit_code(&self) -> Option<i32> {
        let text = fs::read_to_string(&self.marker).ok()?;
        text.lines()
            .filter_map(|line| line.trim().strip_prefix(EXIT_MARKER))
            .filter_map(|code| code.trim().parse::<i32>().ok())
            .next_back()
    }

    fn version(&self) -> String {
        match self.call(&["-V"]) {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            }
            _ => String::new(),
        }
    }
}

/// Ends the tmux session when the run ends, however it ends; `Drop` is only a fallback.
pub struct TmuxGuard<'a> {
    paster: &'a dyn Paster,
    session: String,
    closed: bool,
}

impl<'a> TmuxGuard<'a> {
    /// Watches the session; [`TmuxGuard::close`] ends it.
    pub fn new(paster: &'a dyn Paster, session: &str) -> Self {
        Self {
            paster,
            session: session.to_owned(),
            closed: false,
        }
    }

    /// Ends the session; reports `tmux session still present` or `cannot end tmux session`.
    pub fn close(mut self) -> Result<(), CleanupError> {
        self.closed = true;
        let failed = |detail: &str| CleanupError {
            tab: TabId(self.session.clone()),
            detail: detail.to_owned(),
        };
        if self.paster.kill_session(&self.session).is_err() {
            return Err(failed("cannot end tmux session"));
        }
        match self.paster.session_exists(&self.session) {
            Ok(false) => Ok(()),
            Ok(true) => Err(failed("tmux session still present")),
            Err(_) => Err(failed("cannot end tmux session")),
        }
    }
}

impl Drop for TmuxGuard<'_> {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let _ = self.paster.kill_session(&self.session);
    }
}
