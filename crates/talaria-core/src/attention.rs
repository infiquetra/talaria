//! Foreground attention: pane state, alerts, and the rules that raise them.
//!
//! Pure: the tracker reads [`Activity`] values only. Behavior contract: issue 228.

use std::time::{Duration, Instant};

use crate::session::PromptKey;

/// What the foreground pane is doing, as Herdr and the screen show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneState {
    /// Ready for input.
    Idle,
    /// A turn is running.
    Working,
    /// A need is open: the session waits on the person.
    Blocked,
}

/// Why an alert fired. Urgency, highest first: needs you, failed, done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertKind {
    /// A need opened.
    NeedsYou,
    /// A long turn finished.
    Done,
    /// A turn failed.
    Failed,
}

/// How a finished turn ended, from the reduced turn outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnOutcome {
    /// The turn completed.
    Complete,
    /// The turn failed.
    Failed,
    /// The turn was interrupted.
    Interrupted,
}

/// One open need: an approval or clarification prompt, or a masked request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NeedKey {
    /// A prompt in the reduced session state.
    Prompt(PromptKey),
    /// A masked request id.
    Masked(String),
}

/// What a need asks for, for alert and pane message text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NeedSummary {
    /// An approval: raw command and description; the tracker picks the first line.
    Approval {
        /// Raw command text, may be blank.
        command: String,
        /// Raw description text, may be blank.
        description: String,
    },
    /// A clarification question.
    Question(String),
    /// A sudo password prompt.
    Sudo,
    /// A secret prompt.
    Secret,
    /// A vault code prompt.
    VaultCode,
    /// A password manager unlock prompt.
    VaultUnlock,
}

/// One thing that happened in the foreground session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// The turn state moved into running.
    TurnStarted {
        /// When the tracker saw it start.
        at: Instant,
    },
    /// The turn state moved from running to finished.
    TurnEnded {
        /// The reduced outcome.
        outcome: TurnOutcome,
        /// When it ended.
        at: Instant,
    },
    /// An approval, clarification, or masked request appeared.
    NeedOpened {
        /// Which need.
        key: NeedKey,
        /// What it asks for.
        summary: NeedSummary,
        /// True only when it came from the launch snapshot.
        restored: bool,
        /// When it opened.
        at: Instant,
    },
    /// A prompt left the open set, or a masked id left the masked list.
    NeedClosed {
        /// Which need.
        key: NeedKey,
    },
    /// The foreground session changed, or its title became known.
    SessionChanged {
        /// The new session id.
        session_id: String,
        /// Title, else folder name, else `session`.
        label: String,
    },
}

/// Which alert kinds ring the bell or raise a desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionConfig {
    /// Kinds that ring the terminal bell.
    pub bell: Vec<AlertKind>,
    /// Kinds that send a desktop notification.
    pub desktop: Vec<AlertKind>,
    /// A finished turn alerts only at or above this duration.
    pub done_after: Duration,
}

/// One alert the tracker emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Why it fired.
    pub kind: AlertKind,
    /// Ring the bell.
    pub bell: bool,
    /// Send a desktop notification.
    pub desktop: bool,
    /// Text before the product prefix is added.
    pub text: String,
}

/// The current pane report, for the screen and Herdr.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaneReport {
    /// The derived state.
    pub state: PaneState,
    /// Set only for the blocked state.
    pub message: Option<String>,
}

/// What one observed activity produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionOutput {
    /// The alert to write, if any.
    pub alert: Option<Alert>,
    /// The pane report, only when the state or its message changed.
    pub pane: Option<PaneReport>,
}

/// Tracks open needs and the running turn for the foreground session.
#[derive(Debug)]
pub struct AttentionTracker {
    /// Configuration. The remaining tracker state lands here with the build.
    _config: AttentionConfig,
}

impl AttentionTracker {
    /// A tracker with no open needs and no running turn.
    pub fn new(config: AttentionConfig) -> Self {
        let _ = config;
        todo!("#228")
    }

    /// Fold one activity into the tracker.
    pub fn observe(&mut self, activity: Activity) -> AttentionOutput {
        let _ = activity;
        todo!("#228")
    }

    /// The current report, for re-sending after a resume.
    pub fn pane(&self) -> PaneReport {
        todo!("#228")
    }
}
