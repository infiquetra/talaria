//! Remembering where a person came from when they opened a workflow agent's session from the
//! run view, so one key can take them back.
//!
//! The memory belongs to one trip: the Enter that opened the agent's session starts it, the
//! switch that lands in that session confirms it, and anything that puts a different session
//! in the window ends it. It never holds an agent's session as the place to return to.

use std::path::PathBuf;

use talaria_core::directory::rows::RowKey;
use talaria_workflow::progress::live::NodeKey;

use crate::sessions::OpenedSession;

/// The session to return to, the run to show again and the row that was selected in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowOrigin {
    /// Where the key goes: the session the person was in. Never an agent's session.
    session: RowKey,
    /// The run whose view opens again.
    run: PathBuf,
    /// The row that was selected when Enter opened the agent's session.
    selection: NodeKey,
    /// The runtime id of the agent session the person is in, once the switch has landed.
    away_in: Option<String>,
    /// The agent session an Enter is opening now. The trip is not confirmed until it lands.
    heading_to: Option<RowKey>,
}

/// A trip back that has started: the view opens when the switch lands in `session`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowReturn {
    pub(crate) session: RowKey,
    pub(crate) run: PathBuf,
    pub(crate) selection: NodeKey,
}

impl WorkflowOrigin {
    /// The person is leaving `session` for the agent session `target`.
    #[must_use]
    pub fn leaving(session: RowKey, run: PathBuf, selection: NodeKey, target: RowKey) -> Self {
        Self {
            session,
            run,
            selection,
            away_in: None,
            heading_to: Some(target),
        }
    }

    /// An Enter in an agent session opens another one: the place to return to stays, and the
    /// run and row to show again are the ones that were just entered.
    pub fn retarget(&mut self, run: PathBuf, selection: NodeKey, target: RowKey) {
        self.run = run;
        self.selection = selection;
        self.heading_to = Some(target);
    }

    /// Whether the key may go back from the session with this runtime id.
    #[must_use]
    pub fn is_away_in(&self, session_id: &str) -> bool {
        self.heading_to.is_none() && self.away_in.as_deref() == Some(session_id)
    }

    /// Whether a switch to the agent session is under way.
    #[must_use]
    pub fn is_leaving(&self) -> bool {
        self.heading_to.is_some()
    }

    /// The switch finished in `opened`, on `profile`. True when it is the session the Enter
    /// asked for, which confirms the trip.
    pub fn landed(&mut self, opened: &OpenedSession, profile: &str) -> bool {
        let Some(target) = self.heading_to.take() else {
            return false;
        };
        let here = target.profile == profile
            && (target.session_id == opened.session_id
                || target.session_id == opened.stored_session_id);
        if here {
            self.away_in = Some(opened.session_id.clone());
        }
        here
    }

    /// The switch did not open the session. The trip stands only if the person was already in
    /// an agent session.
    pub fn failed(&mut self) -> bool {
        self.heading_to = None;
        self.away_in.is_some()
    }

    /// The person's own session opened again under another runtime id (a reconnect), with the
    /// same stored id. True when the memory still applies.
    pub fn follow_reopen(
        &mut self,
        old: &OpenedSession,
        new: &OpenedSession,
        same_profile: bool,
    ) -> bool {
        if !same_profile
            || self.away_in.as_deref() != Some(old.session_id.as_str())
            || old.stored_session_id != new.stored_session_id
        {
            return false;
        }
        self.away_in = Some(new.session_id.clone());
        true
    }

    /// The trip back starts: the memory ends and the plan for the view remains.
    #[must_use]
    pub fn into_return(self) -> WorkflowReturn {
        WorkflowReturn {
            session: self.session,
            run: self.run,
            selection: self.selection,
        }
    }

    /// The place the key goes to.
    #[must_use]
    pub fn session(&self) -> &RowKey {
        &self.session
    }
}

impl WorkflowReturn {
    /// The switch finished in `opened`, on `profile`: whether it is the session to return to.
    #[must_use]
    pub fn matches(&self, opened: &OpenedSession, profile: &str) -> bool {
        self.session.profile == profile
            && (self.session.session_id == opened.session_id
                || self.session.session_id == opened.stored_session_id)
    }
}

/// The place to return to for the session now open: its profile and stored id. The Hermes
/// session authority reports one id for a session as both its runtime and its stored id, so an
/// equal pair is the normal case and is remembered. `None` only when there is no id at all.
#[must_use]
pub fn origin_row(opened: &OpenedSession, profile: &str) -> Option<RowKey> {
    let stored = opened.stored_session_id.as_str();
    (!stored.is_empty()).then(|| RowKey {
        profile: profile.to_owned(),
        session_id: stored.to_owned(),
    })
}
