//! The peek session: one extra link used to read, reply, stop or answer a background session.
//!
//! The card intent lives in the terminal crate. This service takes [`CardAnswer`], which has
//! the same fields, so this crate does not depend on the terminal crate.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::liveness::LinkEvent;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::SubscribedView;
use talaria_core::fleet_view::{PeekStatus, ReplyOutcome, StopOutcome};
use talaria_core::session::{PromptKey, SessionState};

use crate::launch::Launch;
use crate::prompts::{PromptNotice, PromptService};

/// Why a peek did not open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekError {
    Connect(String),
    NotFound,
    Refused { reason: String },
    TooLarge,
    Timeout,
    Dropped,
}

/// What changed for the overlay after one link event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekUpdate {
    Unchanged,
    Changed { status: PeekStatus },
}

/// An answer for the open card, with the same fields as the terminal's prompt intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardAnswer {
    Approve {
        key: PromptKey,
        choice: String,
        note: Option<String>,
    },
    Clarify {
        key: PromptKey,
        answer: String,
    },
    /// An empty value means declined.
    Masked {
        request_id: String,
        value: String,
    },
}

/// The three registry calls the peek makes. The real registry handle has no test constructor,
/// so tests pass a fake. The forwarder for the real handle lands with the registry.
pub trait FleetSink: Send + Sync {
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView);
    fn observe_unsubscribed(&self, key: RowKey);
    fn acknowledge(&self, key: RowKey);
}

/// Timeouts for resume, ordinary calls, the deny that runs before a stop, and close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeekTimeouts {
    pub resume: Duration,
    pub call: Duration,
    pub deny: Duration,
    pub close: Duration,
}

impl Default for PeekTimeouts {
    fn default() -> Self {
        Self {
            resume: Duration::from_secs(120),
            call: hermes_client::rpc::DEFAULT_CALL_TIMEOUT,
            deny: Duration::from_secs(2),
            close: Duration::from_secs(2),
        }
    }
}

impl PeekTimeouts {
    /// The same timeouts, each divided by 20, for tests that use a real clock.
    #[must_use]
    pub fn scaled_for_tests() -> Self {
        let base = Self::default();
        Self {
            resume: base.resume / 20,
            call: base.call / 20,
            deny: base.deny / 20,
            close: base.close / 20,
        }
    }
}

/// One peek. Opening returns an error until the link exists.
#[allow(dead_code)]
pub struct PeekService {
    launch: Launch,
    home: PathBuf,
    fleet: Arc<dyn FleetSink>,
    next_submission_id: Box<dyn FnMut() -> String + Send>,
    timeouts: PeekTimeouts,
}

impl PeekService {
    #[must_use]
    pub fn new(
        launch: Launch,
        home: PathBuf,
        fleet: Arc<dyn FleetSink>,
        next_submission_id: Box<dyn FnMut() -> String + Send>,
        timeouts: PeekTimeouts,
    ) -> Self {
        Self {
            launch,
            home,
            fleet,
            next_submission_id,
            timeouts,
        }
    }

    /// Detach any other peek, then resume `key`. This stub never connects.
    pub async fn open(&mut self, key: RowKey) -> Result<(), PeekError> {
        let _ = key;
        Err(PeekError::Connect("stub".to_owned()))
    }

    #[must_use]
    pub fn state(&self) -> Option<(&RowKey, &SessionState)> {
        None
    }

    #[must_use]
    pub fn prompts(&self) -> Option<&PromptService> {
        None
    }

    /// Reduce one link event. The stub reports no change.
    pub fn on_link_event(&mut self, ev: LinkEvent) -> PeekUpdate {
        let _ = ev;
        PeekUpdate::Unchanged
    }

    /// Send a reply on the peeked session.
    pub async fn reply(&mut self, text: String) -> ReplyOutcome {
        let _ = text;
        ReplyOutcome::Unknown {
            why: "no reply".to_owned(),
        }
    }

    /// Send the stored unconfirmed reply again, with the same text and submission id.
    pub async fn resend_unconfirmed(&mut self, key: &RowKey) -> ReplyOutcome {
        let _ = key;
        ReplyOutcome::Unknown {
            why: "no reply".to_owned(),
        }
    }

    /// Deny open approvals, then interrupt. The stub sends nothing.
    pub async fn stop(&mut self) -> StopOutcome {
        StopOutcome::Unknown {
            why: "no reply".to_owned(),
        }
    }

    /// Answer the open card. The stub sends nothing and reports no notice.
    pub async fn answer(&mut self, intent: CardAnswer) -> Vec<PromptNotice> {
        let _ = intent;
        Vec::new()
    }

    /// Detach without an end reason and drop the link.
    pub async fn close(&mut self) {}
}
