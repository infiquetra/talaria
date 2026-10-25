//! Foreground comforts: attention mapping, editor, history, suspend, Herdr.
//!
//! The application decides; the terminal holder executes. Behavior contract: issue 228.

pub mod editor;
pub mod herdr;
pub mod prompt_history;
pub mod suspend;

use std::time::Instant;

use talaria_core::attention::{Activity, NeedKey, NeedSummary, TurnOutcome};
use talaria_core::session::{PromptKind, SessionState, TurnOutcome as ReducedOutcome, TurnState};

use crate::prompts::{MaskedKind, MaskedRequestInfo};

/// Only the reduced values attention needs; transcript text is never copied.
#[derive(Clone, Debug)]
pub struct AttentionSnapshot {
    turn: TurnState,
    needs: Vec<(NeedKey, NeedSummary)>,
}

impl AttentionSnapshot {
    /// Capture the foreground turn and open needs before an application message.
    pub fn from(state: &SessionState, masked: &[MaskedRequestInfo]) -> Self {
        let mut needs: Vec<_> = state
            .open_prompts()
            .map(|prompt| {
                let summary = match prompt.kind {
                    PromptKind::Approval => NeedSummary::Approval {
                        command: prompt.command.clone().unwrap_or_default(),
                        description: prompt.description.clone().unwrap_or_default(),
                    },
                    PromptKind::Clarify => {
                        NeedSummary::Question(prompt.question.clone().unwrap_or_default())
                    }
                };
                (NeedKey::Prompt(prompt.key.clone()), summary)
            })
            .collect();
        needs.extend(masked.iter().map(|request| {
            let summary = match request.kind {
                MaskedKind::Sudo => NeedSummary::Sudo,
                MaskedKind::Secret => NeedSummary::Secret,
                MaskedKind::VaultCode => NeedSummary::VaultCode,
                MaskedKind::VaultUnlockPrompt => NeedSummary::VaultUnlock,
            };
            (NeedKey::Masked(request.id.clone()), summary)
        }));
        Self {
            turn: state.turn().clone(),
            needs,
        }
    }
}

/// Compare reduced state and masked lists before and after one message.
pub fn activities_for(
    before: &SessionState,
    after: &SessionState,
    masked_before: &[MaskedRequestInfo],
    masked_after: &[MaskedRequestInfo],
    restored: bool,
    at: Instant,
) -> Vec<Activity> {
    activities_between(
        &AttentionSnapshot::from(before, masked_before),
        &AttentionSnapshot::from(after, masked_after),
        restored,
        at,
    )
}

/// Compare two small attention snapshots in close, end, start, open order.
pub fn activities_between(
    before: &AttentionSnapshot,
    after: &AttentionSnapshot,
    restored: bool,
    at: Instant,
) -> Vec<Activity> {
    let mut activities = Vec::new();
    for (key, _) in &before.needs {
        if !after.needs.iter().any(|(other, _)| other == key) {
            activities.push(Activity::NeedClosed { key: key.clone() });
        }
    }
    if before.turn == TurnState::Running
        && let TurnState::Finished(outcome) = &after.turn
    {
        let outcome = match outcome {
            ReducedOutcome::Completed => TurnOutcome::Complete,
            ReducedOutcome::Failed => TurnOutcome::Failed,
            ReducedOutcome::Interrupted => TurnOutcome::Interrupted,
        };
        activities.push(Activity::TurnEnded { outcome, at });
    }
    if before.turn != TurnState::Running && after.turn == TurnState::Running {
        activities.push(Activity::TurnStarted { at });
    }
    for (key, summary) in &after.needs {
        if !before.needs.iter().any(|(other, _)| other == key) {
            activities.push(Activity::NeedOpened {
                key: key.clone(),
                summary: summary.clone(),
                restored,
                at,
            });
        }
    }
    activities
}
