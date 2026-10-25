//! A fake `Paster` for the phase 2a suite: it shares the fake terminal's machine, logs every
//! call, derives sessions from typed text and marker files, and takes scripted answers for the
//! failing paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use smoke::{Clock, Paster, TermError};

use super::{FakeTerminal, SECOND};

/// One tmux call the fake paster logged.
#[derive(Clone, Debug)]
pub enum PasterCall {
    /// `exit_code` was asked.
    ExitCode,
    /// `session_exists` was asked.
    SessionExists {
        /// The session name.
        session: String,
    },
    /// `paste` was asked.
    Paste {
        /// The session name.
        session: String,
        /// The pasted text (a drop path pastes as its path string).
        text: String,
    },
    /// `kill_session` was asked.
    KillSession {
        /// The session name.
        session: String,
    },
    /// `version` was asked.
    Version,
}

/// A logged call with the clock time.
#[derive(Clone, Debug)]
pub struct PasterRecorded {
    /// When the call happened.
    pub at: Duration,
    /// The call.
    pub call: PasterCall,
}

/// The fake paster. Pastes feed the shared machine as `paste` inputs on the split pane, so
/// script rules answer them; `session_exists` is true once a `tmux new-session` naming the
/// session was typed, until the session is killed or its marker file shows an exit.
pub struct FakePaster<'a> {
    term: &'a FakeTerminal<'a>,
    clock: &'a dyn Clock,
    temp_root: PathBuf,
    calls: Mutex<Vec<PasterRecorded>>,
    killed: Mutex<HashSet<String>>,
    exit_override: Mutex<Option<Option<i32>>>,
    exists_override: Mutex<Option<bool>>,
    exists_error: Mutex<Option<String>>,
    kill_error: Mutex<Option<String>>,
    version: Mutex<String>,
}

impl<'a> FakePaster<'a> {
    /// A paster sharing the terminal's machine, stamping calls with the clock and scanning
    /// marker files under the run's temporary root.
    pub fn new(term: &'a FakeTerminal<'a>, clock: &'a dyn Clock, temp_root: &Path) -> Self {
        FakePaster {
            term,
            clock,
            temp_root: temp_root.to_owned(),
            calls: Mutex::new(Vec::new()),
            killed: Mutex::new(HashSet::new()),
            exit_override: Mutex::new(None),
            exists_override: Mutex::new(None),
            exists_error: Mutex::new(None),
            kill_error: Mutex::new(None),
            version: Mutex::new("tmux 3.7c".to_owned()),
        }
    }

    /// Forces `exit_code` to the answer (scripts that need an early non-zero exit use this).
    pub fn set_exit_code(&self, code: Option<i32>) {
        *self.exit_override.lock().expect("fake paster lock") = Some(code);
    }

    /// Forces `session_exists` to the answer.
    pub fn set_exists(&self, exists: bool) {
        *self.exists_override.lock().expect("fake paster lock") = Some(exists);
    }

    /// Makes `session_exists` fail with the detail (a tmux error).
    pub fn fail_exists_with(&self, detail: &str) {
        *self.exists_error.lock().expect("fake paster lock") = Some(detail.to_owned());
    }

    /// Makes `kill_session` fail with the detail (an unkillable session).
    pub fn fail_kill_with(&self, detail: &str) {
        *self.kill_error.lock().expect("fake paster lock") = Some(detail.to_owned());
    }

    /// Every call, oldest first.
    pub fn calls(&self) -> Vec<PasterRecorded> {
        self.calls.lock().expect("fake paster lock").clone()
    }

    /// Every pasted `(session, text)` pair, oldest first.
    pub fn pastes(&self) -> Vec<(String, String)> {
        self.calls()
            .iter()
            .filter_map(|recorded| match &recorded.call {
                PasterCall::Paste { session, text } => Some((session.clone(), text.clone())),
                _ => None,
            })
            .collect()
    }

    /// Every killed session, oldest first.
    pub fn kills(&self) -> Vec<String> {
        self.calls()
            .iter()
            .filter_map(|recorded| match &recorded.call {
                PasterCall::KillSession { session } => Some(session.clone()),
                _ => None,
            })
            .collect()
    }

    fn record(&self, call: PasterCall) {
        self.calls
            .lock()
            .expect("fake paster lock")
            .push(PasterRecorded {
                at: self.clock.now(),
                call,
            });
    }
}

/// The first `word-number` token in the text, if any; a sentence's closing period is not part
/// of it (the dropped file reads `The secret word is <token>.`).
fn first_token(content: &str) -> Option<&str> {
    content
        .split_whitespace()
        .map(|word| word.trim_end_matches('.'))
        .find(|word| {
            let Some(dash) = word.find('-') else {
                return false;
            };
            let (head, tail) = word.split_at(dash);
            let tail = &tail[1..];
            !head.is_empty()
                && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !tail.is_empty()
                && tail.chars().all(|c| c.is_ascii_digit())
        })
}

/// Whether the session's marker file shows an exit.
fn marker_shows_exit(temp_root: &Path, session: &str) -> bool {
    if session.contains('/') || session.contains("..") {
        return false;
    }
    let marker = temp_root.join(session).join("tmux-exit.txt");
    std::fs::read_to_string(marker)
        .map(|text| text.lines().any(|line| line.contains("SMOKE_EXIT:")))
        .unwrap_or(false)
}

/// The last `SMOKE_EXIT:` code under the temporary root, if any marker file holds one.
fn marker_exit_code(temp_root: &Path) -> Option<i32> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(temp_root)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("talaria-smoke-"))
        })
        .collect();
    folders.sort();
    folders.into_iter().find_map(|folder| {
        let text = std::fs::read_to_string(folder.join("tmux-exit.txt")).ok()?;
        text.lines()
            .filter_map(|line| line.split("SMOKE_EXIT:").nth(1))
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .next_back()
    })
}

impl Paster for FakePaster<'_> {
    fn paste(&self, session: &str, text: &str) -> Result<(), TermError> {
        self.record(PasterCall::Paste {
            session: session.to_owned(),
            text: text.to_owned(),
        });
        // A drop path names a real file: Hermes reads it, so the fake notes the token it holds
        // for the reply rule. Pasted content never names an existing file.
        let path = Path::new(text);
        if path.is_absolute()
            && let Ok(content) = std::fs::read_to_string(path)
            && let Some(token) = first_token(&content)
        {
            self.term.note_remembered(token.to_owned());
        }
        self.term.paste_into(SECOND, text);
        Ok(())
    }

    fn session_exists(&self, session: &str) -> Result<bool, TermError> {
        self.record(PasterCall::SessionExists {
            session: session.to_owned(),
        });
        if let Some(detail) = self.exists_error.lock().expect("fake paster lock").clone() {
            return Err(TermError::Io(detail));
        }
        if let Some(forced) = *self.exists_override.lock().expect("fake paster lock") {
            return Ok(forced);
        }
        if self
            .killed
            .lock()
            .expect("fake paster lock")
            .contains(session)
        {
            return Ok(false);
        }
        if marker_shows_exit(&self.temp_root, session) {
            return Ok(false);
        }
        // The session name in typed text is `talaria-smoke-<run id>`; the marker file sits
        // beside it, so a bare name and a name with a path prefix both match.
        let bare = session.rsplit('/').next().unwrap_or(session);
        Ok(self
            .term
            .typed_texts(SECOND)
            .iter()
            .any(|text| text.contains(session) || text.contains(bare)))
    }

    fn kill_session(&self, session: &str) -> Result<(), TermError> {
        self.record(PasterCall::KillSession {
            session: session.to_owned(),
        });
        if let Some(detail) = self.kill_error.lock().expect("fake paster lock").clone() {
            return Err(TermError::Io(detail));
        }
        self.killed
            .lock()
            .expect("fake paster lock")
            .insert(session.to_owned());
        Ok(())
    }

    fn exit_code(&self) -> Option<i32> {
        self.record(PasterCall::ExitCode);
        if let Some(forced) = *self.exit_override.lock().expect("fake paster lock") {
            return forced;
        }
        marker_exit_code(&self.temp_root)
    }

    fn version(&self) -> String {
        self.record(PasterCall::Version);
        self.version.lock().expect("fake paster lock").clone()
    }
}
