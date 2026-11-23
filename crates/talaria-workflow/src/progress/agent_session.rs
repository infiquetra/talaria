//! Telling the person which workflow agent a Hermes session belongs to.
//!
//! A workflow agent is an ordinary Hermes session. Opening one from `/workflows`, from the
//! `/resume` picker, or reaching a follow-on session Hermes started after an automatic reset all
//! look the same to the window. Two facts identify one: the session's working folder lies under
//! a run's `worktrees` folder (`<runs>/<run id>/worktrees/<call>`), or the run's journal bound
//! that session id to an agent. This module reads both and says, in plain words, what the run
//! knows about the conversation.

use super::{AgentNode, AgentState, RunProgress, RunState};
use crate::journal::{JournalReader, Manifest, RecordLine, RunDir};
use std::path::{Component, Path, PathBuf};

/// The longest label or run name the texts quote.
const QUOTE_LIMIT: usize = 60;

/// What a working folder under a run's `worktrees` folder names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeOwner {
    /// The run's own folder: everything before `worktrees`. It may not exist on this machine.
    pub run_dir: PathBuf,
    /// The run id, the folder's name.
    pub run_id: String,
    /// The call's digest, 64 lower-case hexadecimal characters.
    pub digest: String,
    /// Which repeat of the call the folder belongs to.
    pub occurrence: u32,
}

/// The run and call a working folder belongs to, when it is a workflow agent's worktree or lies
/// inside one. A folder that only looks similar (a run id that is not one, a call name that is
/// not a digest and a count) names nobody.
#[must_use]
pub fn worktree_owner(cwd: &str) -> Option<WorktreeOwner> {
    let parts: Vec<Component<'_>> = Path::new(cwd).components().collect();
    parts.windows(3).enumerate().find_map(|(at, window)| {
        let [run, worktrees, call] = window else {
            return None;
        };
        if worktrees.as_os_str() != "worktrees" {
            return None;
        }
        let run_id = run.as_os_str().to_str()?;
        crate::journal::RunId::parse(run_id).ok()?;
        let (digest, occurrence) = call.as_os_str().to_str()?.rsplit_once('-')?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        Some(WorktreeOwner {
            run_dir: parts[..=at].iter().collect(),
            run_id: run_id.to_owned(),
            digest: digest.to_owned(),
            occurrence: occurrence.parse().ok()?,
        })
    })
}

/// Where an agent stands, as the journal records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPhase {
    Queued,
    Running,
    Stopping,
    NeedsYou,
    /// A stop ended the agent's turn and the run waits for the operator to answer its parked
    /// call; a reply in its session counts once the operator answers `use-reply`.
    Stopped,
    Completed,
    /// Hermes stopped the agent at its step cap and the run kept its reply.
    CutShort,
    Failed,
    Skipped,
    Ended,
}

impl AgentPhase {
    fn of(agent: &AgentNode) -> Self {
        if agent.attention.as_deref() == Some("stopped") {
            return Self::Stopped;
        }
        match agent.state {
            AgentState::Queued => Self::Queued,
            AgentState::Running => Self::Running,
            AgentState::Stopping => Self::Stopping,
            AgentState::Waiting | AgentState::Unknown => Self::NeedsYou,
            AgentState::Done if agent.cut_short.is_some() => Self::CutShort,
            AgentState::Done => Self::Completed,
            AgentState::Failed => Self::Failed,
            AgentState::Skipped => Self::Skipped,
            AgentState::Ended => Self::Ended,
        }
    }
    /// The word the banner shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "not started",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::NeedsYou => "waiting for you",
            Self::Stopped => "stopped, waiting for you",
            Self::Completed => "completed",
            Self::CutShort => "cut short",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Ended => "ended",
        }
    }
    /// The agent will not run again in this attempt, so its journal state stops changing.
    #[must_use]
    pub fn finished(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::CutShort | Self::Failed | Self::Skipped | Self::Ended
        )
    }
}

/// Whether the run follows the conversation on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tracking {
    /// The journal bound this very session to the agent.
    Followed,
    /// The journal knows the agent, and its session is a different one: a follow-on session
    /// Hermes started, an earlier attempt, or a child session. The run does not follow this one.
    NotFollowed,
    /// The journal could not be read here, or does not name this agent.
    Unknown,
}

/// What the run's journal says about the session the window shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSessionNote {
    /// The run's id, from its folder name.
    pub run_id: String,
    pub run_name: Option<String>,
    pub label: Option<String>,
    pub phase: Option<AgentPhase>,
    pub tracking: Tracking,
    /// The journal has not been read yet, so nothing above is known.
    pub pending: bool,
}

fn quote(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() > QUOTE_LIMIT {
        let cut: String = flat.chars().take(QUOTE_LIMIT - 1).collect();
        format!("{cut}…")
    } else {
        flat.to_owned()
    }
}

impl AgentSessionNote {
    /// The run, named the way a person recognises it: its workflow name and the end of its id,
    /// which is the part that differs between runs.
    fn run_text(&self) -> String {
        let tail: String = self
            .run_id
            .chars()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        match self
            .run_name
            .as_deref()
            .map(quote)
            .filter(|n| !n.is_empty())
        {
            Some(name) => format!("run {name} (…{tail})"),
            None => format!("run …{tail}"),
        }
    }

    fn agent_text(&self) -> String {
        match self.label.as_deref().map(quote).filter(|l| !l.is_empty()) {
            Some(label) => format!("Workflow agent {label}"),
            None => "Workflow agent session".to_owned(),
        }
    }

    /// Whether the agent has stopped for good, so the journal state will not change.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.phase.is_some_and(AgentPhase::finished)
    }

    /// The banner line, most important words first so a narrow screen cuts the run name.
    #[must_use]
    pub fn banner(&self) -> String {
        let mut parts = vec![self.agent_text()];
        if let Some(phase) = self.phase {
            parts.push(phase.label().to_owned());
        }
        match self.tracking {
            Tracking::Followed => {}
            Tracking::NotFollowed => {
                parts.push("this conversation is not tracked by the run".to_owned());
            }
            Tracking::Unknown if self.pending => {
                parts.push("reading the run's journal".to_owned());
            }
            Tracking::Unknown if self.run_name.is_some() => {
                parts.push("not found in the run's journal".to_owned());
            }
            Tracking::Unknown => parts.push("journal state not readable here".to_owned()),
        }
        parts.push(self.run_text());
        parts.join(" · ")
    }

    /// The short form for the status line.
    #[must_use]
    pub fn status_text(&self) -> String {
        let mut text = match self.label.as_deref().map(quote).filter(|l| !l.is_empty()) {
            Some(label) => format!("agent {label}"),
            None => "agent session".to_owned(),
        };
        if let Some(phase) = self.phase {
            text.push(' ');
            text.push_str(phase.label());
        }
        match self.tracking {
            Tracking::Followed => {}
            Tracking::NotFollowed => text.push_str(" (untracked)"),
            Tracking::Unknown if self.pending => text.push_str(" (reading)"),
            Tracking::Unknown => text.push_str(" (state unknown)"),
        }
        text
    }

    /// The one line shown the first time a prompt is submitted into the session. It says what
    /// the run will make of the message and how to go ahead.
    #[must_use]
    pub fn first_send_warning(&self) -> String {
        let who = match (self.label.as_deref().map(quote), self.phase) {
            (Some(label), Some(phase)) if !label.is_empty() => {
                format!("{label}, {}", phase.label())
            }
            (Some(label), None) if !label.is_empty() => label,
            _ => self.run_text(),
        };
        let consequence = match (self.tracking, self.phase) {
            (Tracking::Followed, Some(AgentPhase::Stopped)) => {
                "this agent was stopped and the run is waiting for an answer; for its reply to count as the agent's result, answer the parked call with use-reply (talaria workflow resolve … --resolution use-reply, as the runner printed it)"
                    .to_owned()
            }
            (Tracking::Followed, Some(phase)) if !phase.finished() => {
                "your message goes to the agent mid-run and can change its work".to_owned()
            }
            (Tracking::Followed, Some(phase)) => format!(
                "this agent has {}, so what you do here, including edits and commits, will not reach the run",
                match phase {
                    AgentPhase::Failed => "failed",
                    AgentPhase::CutShort => "been cut short",
                    AgentPhase::Skipped => "been skipped",
                    AgentPhase::Ended => "ended",
                    _ => "finished",
                }
            ),
            (Tracking::NotFollowed, _) => {
                "the run does not track this conversation, so what you do here, including edits and commits, will not reach the run"
                    .to_owned()
            }
            _ if self.pending => {
                "Talaria is still reading the run's journal, so it cannot yet say whether the run follows this conversation"
                    .to_owned()
            }
            _ if self.run_name.is_some() => {
                "the run's journal does not name this agent, so Talaria cannot say whether the run follows this conversation"
                    .to_owned()
            }
            _ => "Talaria cannot read the run's journal here, so it cannot say whether the run follows this conversation"
                .to_owned(),
        };
        format!("Workflow agent session ({who}): {consequence}. Submit again to send.")
    }
}

/// What one read of a run's journal brought back. Reading is the only part that touches the
/// disk, so it is a plain function a caller can run on a worker thread; the watch only applies
/// the result.
pub struct AgentSessionRead {
    /// The run's manifest, present when the read started at the beginning of the journal.
    manifest: Option<Manifest>,
    lines: Vec<RecordLine>,
    offset: u64,
    /// Whether a runner holds the run lock, when the lock could be tested.
    runner_alive: Option<bool>,
}

/// Read the run's journal from `offset` (0 for the first read, which also opens the manifest).
///
/// # Errors
/// The reason the run folder or its journal could not be read.
pub fn read_agent_journal(run_dir: &Path, offset: u64) -> Result<AgentSessionRead, String> {
    let manifest = if offset == 0 {
        Some(
            RunDir::open(run_dir)
                .map_err(|e| e.to_string())?
                .manifest()
                .clone(),
        )
    } else {
        None
    };
    let runner_alive = crate::runner::runs::runner_alive(run_dir).ok();
    let (lines, offset) = JournalReader::read_from(run_dir, offset).map_err(|e| e.to_string())?;
    Ok(AgentSessionRead {
        manifest,
        lines,
        offset,
        runner_alive,
    })
}

/// A run's journal, followed for one open session. It holds what was read; it never reads.
pub struct AgentSessionWatch {
    run_dir: PathBuf,
    run_id: String,
    call: Option<(String, u32)>,
    stored_id: String,
    runtime_id: String,
    progress: Option<RunProgress>,
    offset: u64,
    runner_alive: Option<bool>,
    /// A read failed: nothing more will be learned from this run folder.
    failed: bool,
}

impl AgentSessionWatch {
    /// Start following the run in `run_dir` for the session that has these ids. `call` is the
    /// worktree's call when the working folder named one. Nothing is read yet: until the first
    /// [`apply`](Self::apply) the note says it is still reading.
    #[must_use]
    pub fn new(
        run_dir: PathBuf,
        call: Option<(String, u32)>,
        stored_id: &str,
        runtime_id: &str,
    ) -> Self {
        let run_id = run_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        Self {
            run_dir,
            run_id,
            call,
            stored_id: stored_id.to_owned(),
            runtime_id: runtime_id.to_owned(),
            progress: None,
            offset: 0,
            runner_alive: None,
            failed: false,
        }
    }

    /// The run's folder, for opening its view and for reading it.
    #[must_use]
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// Where the next read starts.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Whether a later read can change the note: the journal was read, the run is still
    /// running with a runner alive, the journal names the agent, and the agent has not
    /// stopped for good.
    #[must_use]
    pub fn wants_refresh(&self) -> bool {
        let Some(progress) = &self.progress else {
            return false;
        };
        !self.failed
            && progress.run().state == RunState::Running
            && self.runner_alive != Some(false)
            && self.note().phase.is_some_and(|phase| !phase.finished())
    }

    /// Take the result of a read. Returns whether the note changed. A failed read keeps what
    /// was last read and stops further reads.
    pub fn apply(&mut self, read: Result<AgentSessionRead, String>) -> bool {
        let before = self.note();
        match read {
            Ok(read) => {
                self.runner_alive = read.runner_alive;
                match (&mut self.progress, read.manifest) {
                    (None, Some(manifest)) => {
                        self.progress = Some(RunProgress::from_records(&manifest, &read.lines));
                    }
                    (Some(progress), None) => progress.apply_all(&read.lines),
                    // A repeat of the first read, or a continuation with nothing to continue.
                    _ => {
                        self.failed = self.progress.is_none();
                        return before != self.note();
                    }
                }
                self.offset = read.offset;
            }
            Err(_) => self.failed = true,
        }
        before != self.note()
    }

    /// What the journal says about the session, as of the last read.
    #[must_use]
    pub fn note(&self) -> AgentSessionNote {
        let unknown = AgentSessionNote {
            run_id: self.run_id.clone(),
            run_name: None,
            label: None,
            phase: None,
            tracking: Tracking::Unknown,
            pending: self.progress.is_none() && !self.failed,
        };
        let Some(progress) = &self.progress else {
            return unknown;
        };
        let bound = [&self.stored_id, &self.runtime_id]
            .into_iter()
            .filter(|id| !id.is_empty())
            .find_map(|id| progress.for_session(id));
        let (agent, tracking) = match bound {
            Some(agent) => (Some(agent), Tracking::Followed),
            None => (
                self.call
                    .as_ref()
                    .and_then(|(digest, occurrence)| progress.for_call(digest, *occurrence)),
                Tracking::NotFollowed,
            ),
        };
        let Some(agent) = agent else {
            return AgentSessionNote {
                run_name: Some(progress.run().name.clone()),
                ..unknown
            };
        };
        AgentSessionNote {
            run_name: Some(progress.run().name.clone()),
            label: Some(agent.label.clone()),
            phase: Some(AgentPhase::of(agent)),
            tracking,
            ..unknown
        }
    }
}

#[cfg(test)]
mod tests;
