//! Folding resumed scaffold prompts.
//!
//! Behavior contract: issue 236. Hermes persists a plan, goal-continuation or loop-wakeup turn
//! with its whole scaffold prompt; on resume the user cell shows a one-line summary instead.

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

const PLAN: &str = "[/plan — plan mode]";
const PLAN_TASK: &str = "\nTask to plan:\n";
const PLAN_CRAFT: &str = "\n\nWrite the plan for an implementer";
const GOAL_GATE: &str = "[Continuing toward your standing goal — a quality gate failed]";
const GOAL: &str = "[Continuing toward your standing goal]";
const WAKEUP: &str = "[/loop wakeup #";

/// Fold a resumed user row, or `None` when it is not a scaffold.
pub fn fold_user_text(text: &str) -> Option<FoldedPrompt> {
    let text = text.trim_start();
    if text.starts_with(PLAN) {
        let display = match plan_task(text) {
            Some(task) => format!("/plan {task}"),
            None => "/plan".to_owned(),
        };
        return Some(FoldedPrompt {
            display,
            kind: FoldKind::Plan,
        });
    }
    for (prefix, label, kind) in [
        (
            GOAL_GATE,
            "↻ goal continues after a failed quality gate",
            FoldKind::GoalGateFailed,
        ),
        (GOAL, "↻ goal continues", FoldKind::GoalContinuation),
    ] {
        if text.starts_with(prefix) {
            return Some(FoldedPrompt {
                display: with_value(label, line_value(text, "Goal: ")),
                kind,
            });
        }
    }
    let rest = text.strip_prefix(WAKEUP)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    Some(FoldedPrompt {
        display: with_value(
            &format!("↻ loop run #{digits}"),
            line_value(text, "Recurring task: "),
        ),
        kind: FoldKind::LoopWakeup,
    })
}

/// The task between the task marker and the last craft block, trimmed; `None` when empty.
fn plan_task(text: &str) -> Option<&str> {
    let start = text.find(PLAN_TASK)? + PLAN_TASK.len();
    let rest = &text[start..];
    let task = rest[..rest.rfind(PLAN_CRAFT)?].trim();
    (!task.is_empty()).then_some(task)
}

/// The rest of the first line starting with `marker`, trimmed; `None` when absent or empty.
fn line_value<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(marker))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn with_value(label: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{label}: {value}"),
        None => label.to_owned(),
    }
}
