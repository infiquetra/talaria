//! Folding resumed scaffold prompts. Stubs.
//!
//! Behavior contract: issue 236.

/// A folded scaffold prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldedPrompt {
    /// The display text.
    pub display: String,
    /// Which scaffold it was.
    pub kind: FoldKind,
}

/// Which scaffold a folded prompt came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldKind {
    /// A plan prompt.
    Plan,
    /// A goal continuation.
    GoalContinuation,
    /// A goal continuation after a failed quality gate.
    GoalGateFailed,
    /// A loop wakeup.
    LoopWakeup,
}

/// Fold a resumed user row, or `None` when it is not a scaffold.
pub fn fold_user_text(text: &str) -> Option<FoldedPrompt> {
    let _ = text;
    todo!("#236")
}
