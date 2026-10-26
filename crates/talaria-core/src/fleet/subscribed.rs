//! What an attached session's owner reports.

use super::{NeedSlot, NeedSummary, Outcome};
use crate::session::{PromptKind, TurnOutcome, TurnState};

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
        let turn = match state.turn() {
            TurnState::Idle => SubscribedTurn::Idle,
            TurnState::Running => SubscribedTurn::Running { started_at: None },
            TurnState::Finished(outcome) => SubscribedTurn::Finished(match outcome {
                TurnOutcome::Completed => Outcome::Completed,
                TurnOutcome::Failed => Outcome::Failed,
                TurnOutcome::Interrupted => Outcome::Interrupted,
            }),
        };
        let mut needs: Vec<SubscribedNeed> = state
            .open_prompts()
            .map(|prompt| SubscribedNeed {
                slot: NeedSlot::Prompt(prompt.key.prompt_id.clone()),
                summary: match prompt.kind {
                    PromptKind::Approval => NeedSummary::Approval {
                        command: prompt.command.clone().unwrap_or_default(),
                        description: prompt.description.clone().unwrap_or_default(),
                    },
                    PromptKind::Clarify => {
                        NeedSummary::Question(prompt.question.clone().unwrap_or_default())
                    }
                },
                choices: prompt.choices.clone(),
                execution_generation: Some(prompt.key.execution_generation),
                timeout_s: prompt.timeout_s,
            })
            .collect();
        needs.extend(masked.iter().map(|(request_id, summary)| SubscribedNeed {
            slot: NeedSlot::Masked(request_id.clone()),
            summary: summary.clone(),
            choices: Vec::new(),
            execution_generation: None,
            timeout_s: None,
        }));
        SubscribedView {
            turn,
            needs,
            activity: state.activity().map(str::to_string),
            cost_usd: state.usage().cost_usd,
            model: state.usage().model.clone(),
            title: None,
        }
    }
}
