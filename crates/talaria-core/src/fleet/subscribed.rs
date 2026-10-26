//! What an attached session's owner reports.

use super::{NeedSlot, NeedSummary, Outcome};

/// One need taken from attached session state.
#[derive(Clone, Debug, PartialEq)]
pub struct SubscribedNeed {
    /// Prompt, masked request, or a waiting placeholder.
    pub slot: NeedSlot,
    /// What the need asks for.
    pub summary: NeedSummary,
    /// Offered choices.
    pub choices: Vec<String>,
    /// Generation of the prompt, when the state has one.
    pub execution_generation: Option<i64>,
    /// Timeout hint.
    pub timeout_s: Option<f64>,
}

/// Turn of an attached session.
#[derive(Clone, Debug, PartialEq)]
pub enum SubscribedTurn {
    /// No turn is open.
    Idle,
    /// A turn is running.
    Running {
        /// Seconds since 1970, when the state has a start.
        started_at: Option<f64>,
    },
    /// The turn has finished.
    Finished(Outcome),
}

/// The view an owner sends while it holds the session.
#[derive(Clone, Debug, PartialEq)]
pub struct SubscribedView {
    /// Turn.
    pub turn: SubscribedTurn,
    /// Open needs.
    pub needs: Vec<SubscribedNeed>,
    /// Activity text.
    pub activity: Option<String>,
    /// Usage cost. List calls do not provide this.
    pub cost_usd: Option<f64>,
    /// Model.
    pub model: Option<String>,
    /// Title.
    pub title: Option<String>,
}

impl SubscribedView {
    /// Read attached state. `masked` is `(request id, summary)` pairs the caller already
    /// converted. This crate does not name the caller's request type.
    pub fn from_state(
        state: &crate::session::SessionState,
        masked: &[(String, NeedSummary)],
    ) -> SubscribedView {
        let _ = (state, masked);
        todo!()
    }
}
