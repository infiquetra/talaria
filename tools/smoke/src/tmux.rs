//! The tmux command line the phase 2a suite drives: pastes, session watches and cleanup.
//!
//! Stage 0 stub: every public function panics `not implemented`.

use std::path::{Path, PathBuf};

use crate::{CleanupError, TermError};

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
#[allow(dead_code)]
pub struct TmuxCli {
    /// The tmux executable.
    executable: PathBuf,
    /// The marker file the wrapped talaria writes on exit.
    marker: PathBuf,
    /// The scratch folder the run owns.
    scratch: PathBuf,
}

impl TmuxCli {
    /// The only constructor: the tmux executable, the marker file path and the scratch folder.
    pub fn new(_executable: &Path, _marker: &Path, _scratch: &Path) -> Self {
        unimplemented!("not implemented")
    }
}

impl Paster for TmuxCli {
    fn paste(&self, _session: &str, _text: &str) -> Result<(), TermError> {
        unimplemented!("not implemented")
    }

    fn session_exists(&self, _session: &str) -> Result<bool, TermError> {
        unimplemented!("not implemented")
    }

    fn kill_session(&self, _session: &str) -> Result<(), TermError> {
        unimplemented!("not implemented")
    }

    fn exit_code(&self) -> Option<i32> {
        unimplemented!("not implemented")
    }

    fn version(&self) -> String {
        unimplemented!("not implemented")
    }
}

/// Ends the tmux session when the run ends, however it ends.
#[allow(dead_code)]
pub struct TmuxGuard<'a> {
    paster: &'a dyn Paster,
    session: String,
}

impl<'a> TmuxGuard<'a> {
    /// Watches the session; [`TmuxGuard::close`] ends it.
    pub fn new(_paster: &'a dyn Paster, _session: &str) -> Self {
        unimplemented!("not implemented")
    }

    /// Ends the session; reports `tmux session still present` or `cannot end tmux session`.
    pub fn close(self) -> Result<(), CleanupError> {
        unimplemented!("not implemented")
    }
}
