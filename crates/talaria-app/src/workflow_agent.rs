//! Knowing that the session in the window is a workflow agent's.
//!
//! Hermes treats a workflow agent's session like any other, so the window must tell the person
//! what it shows: which run and agent, how the journal records the agent, and whether the run
//! follows this conversation. The identification lives in `talaria_workflow`; this module
//! decides when to look, how often to read the journal again, and who has been warned.

use crate::sessions::OpenedSession;
use crate::workflow_progress::ProgressViews;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use talaria_workflow::progress::agent_session::{
    AgentSessionNote, AgentSessionRead, AgentSessionWatch, WorktreeOwner, worktree_owner,
};

/// How often the journal is read again while the agent has not stopped.
const REFRESH_EVERY_MS: u64 = 2_000;

/// The open session, when it belongs to a workflow agent.
pub struct WorkflowAgentSession {
    watch: AgentSessionWatch,
    note: AgentSessionNote,
    stored_id: String,
    next_refresh_ms: u64,
    /// A read is running on a worker thread; no second one starts until it answers.
    in_flight: bool,
}

/// One journal read to run off the update thread: the run's folder and where to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalReadRequest {
    pub run_dir: PathBuf,
    pub offset: u64,
}

impl WorkflowAgentSession {
    /// Look at a session that has just opened. It is a workflow agent's when its working
    /// folder lies under a run's `worktrees` folder, or when a run the window already loaded
    /// bound this session id to one of its agents.
    #[must_use]
    pub fn identify(
        opened: &OpenedSession,
        views: &ProgressViews,
        runs_dir: &Path,
        now_ms: u64,
    ) -> Option<Self> {
        let owner = opened.info.cwd.as_deref().and_then(worktree_owner);
        let loaded = views.states.iter().find_map(|(run, view)| {
            [&opened.stored_session_id, &opened.session_id]
                .into_iter()
                .any(|id| !id.is_empty() && view.progress.for_session(id).is_some())
                .then(|| run.clone())
        });
        let run_dir = match (&loaded, &owner) {
            (Some(run), _) => run.clone(),
            (None, Some(owner)) => folder_of(owner, runs_dir),
            (None, None) => return None,
        };
        let call = owner
            .as_ref()
            .filter(|owner| run_dir.file_name().and_then(|n| n.to_str()) == Some(&owner.run_id))
            .map(|owner| (owner.digest.clone(), owner.occurrence));
        let watch =
            AgentSessionWatch::new(run_dir, call, &opened.stored_session_id, &opened.session_id);
        let note = watch.note();
        Some(Self {
            watch,
            note,
            stored_id: opened.stored_session_id.clone(),
            next_refresh_ms: now_ms.saturating_add(REFRESH_EVERY_MS),
            in_flight: false,
        })
    }

    /// The stored session id this belongs to; a read's answer carries it back.
    #[must_use]
    pub fn stored_id(&self) -> &str {
        &self.stored_id
    }

    /// What the banner and the warning say.
    #[must_use]
    pub fn note(&self) -> &AgentSessionNote {
        &self.note
    }

    /// The run's folder, for the key that opens the run's view.
    #[must_use]
    pub fn run_dir(&self) -> &Path {
        self.watch.run_dir()
    }

    /// When to read the journal again, while a later read can change what it says.
    #[must_use]
    pub fn next_refresh_ms(&self) -> Option<u64> {
        (!self.in_flight && self.watch.wants_refresh()).then_some(self.next_refresh_ms)
    }

    /// The first read, started when the session opens.
    pub fn first_read(&mut self) -> Option<JournalReadRequest> {
        self.start_read()
    }

    /// A read to start now, when it is time and one can change the note.
    pub fn tick(&mut self, now_ms: u64) -> Option<JournalReadRequest> {
        if now_ms < self.next_refresh_ms || !self.watch.wants_refresh() {
            return None;
        }
        self.next_refresh_ms = now_ms.saturating_add(REFRESH_EVERY_MS);
        self.start_read()
    }

    fn start_read(&mut self) -> Option<JournalReadRequest> {
        if self.in_flight {
            return None;
        }
        self.in_flight = true;
        Some(JournalReadRequest {
            run_dir: self.watch.run_dir().to_owned(),
            offset: self.watch.offset(),
        })
    }

    /// Take the answer of a read. Returns whether the note changed.
    pub fn finish_read(&mut self, result: Result<AgentSessionRead, String>) -> bool {
        self.in_flight = false;
        let changed = self.watch.apply(result);
        if changed {
            self.note = self.watch.note();
        }
        changed
    }

    /// The warning for a prompt the person submits, the first time for this session; `None`
    /// when they have already been told. It speaks from what was last read: submitting never
    /// waits on the disk.
    pub fn first_send_warning(&mut self, warned: &mut HashSet<String>) -> Option<String> {
        warned
            .insert(self.stored_id.clone())
            .then(|| self.note.first_send_warning())
    }
}

/// The run's folder on this machine: the one the working folder named, or the run of that id
/// under the runs folder when the named one is not here.
fn folder_of(owner: &WorktreeOwner, runs_dir: &Path) -> PathBuf {
    if owner.run_dir.is_dir() {
        return owner.run_dir.clone();
    }
    let local = runs_dir.join(&owner.run_id);
    if local.is_dir() {
        local
    } else {
        owner.run_dir.clone()
    }
}
