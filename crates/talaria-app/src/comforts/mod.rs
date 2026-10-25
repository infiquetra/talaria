//! Foreground comforts: attention mapping, editor, history, suspend, Herdr.
//!
//! The application decides; the terminal holder executes. Behavior contract: issue 228.

pub mod editor;
pub mod herdr;
pub mod prompt_history;
pub mod suspend;

use std::time::Instant;

use talaria_core::attention::Activity;
use talaria_core::session::SessionState;

use crate::prompts::MaskedRequestInfo;

/// Compare reduced state and masked lists before and after one message.
pub fn activities_for(
    before: &SessionState,
    after: &SessionState,
    masked_before: &[MaskedRequestInfo],
    masked_after: &[MaskedRequestInfo],
    restored: bool,
    at: Instant,
) -> Vec<Activity> {
    let _ = (before, after, masked_before, masked_after, restored, at);
    todo!("#228")
}
