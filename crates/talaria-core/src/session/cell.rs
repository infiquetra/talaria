//! Transcript cells. Text is raw. Wrapping and colour belong to later units.

use serde_json::Value;

use super::{CellId, PromptKey};
use crate::side::{ShellCell, SideCell};

/// One transcript row, in arrival order.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// Text the person submitted, or a user row from a snapshot.
    User(UserCell),
    /// One assistant segment.
    Assistant(AssistantCell),
    /// One contiguous reasoning run.
    Reasoning(ReasoningCell),
    /// One tool call.
    Tool(ToolCell),
    /// A local notice. Not model prose.
    Notice(NoticeCell),
    /// An approval or clarification.
    Prompt(PromptCell),
    /// One finished shell command. Final when created.
    Shell(ShellCell),
    /// One finished side question. Final when created.
    Side(SideCell),
}

impl Cell {
    /// Whether this cell is final and ready to be committed to scrollback once every earlier cell is also final.
    #[must_use]
    pub fn is_final(&self) -> bool {
        match self {
            Self::User(_) | Self::Notice(_) => true,
            Self::Assistant(cell) => cell.sealed,
            Self::Reasoning(cell) => cell.sealed,
            Self::Tool(cell) => cell.status != ToolStatus::Running,
            Self::Prompt(cell) => cell.sealed,
            Self::Shell(_) | Self::Side(_) => true,
        }
    }
}

/// A user cell is final as soon as it exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserCell {
    /// Stable id.
    pub id: CellId,
    /// Raw text.
    pub text: String,
}

/// Assistant prose. `sealed` is what makes the cell final.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssistantCell {
    /// Stable id.
    pub id: CellId,
    /// Raw text.
    pub text: String,
    /// Set when the segment boundary has passed.
    pub sealed: bool,
    /// The cell came from `message.interim`.
    pub interim: bool,
    /// A Hermes hook replaced the reply (`response_transformed`).
    pub transformed: bool,
}

/// Reasoning prose. Never truncated by this unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningCell {
    /// Stable id.
    pub id: CellId,
    /// Raw text.
    pub text: String,
    /// Set when a later event seals the run.
    pub sealed: bool,
}

/// Outcome of a tool cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatus {
    /// `tool.start` seen, `tool.complete` not yet applied.
    Running,
    /// `tool.complete` with `is_error` not the JSON boolean `true`.
    Succeeded,
    /// `tool.complete` with `is_error: true`.
    Failed,
    /// The turn ended while the tool was still running.
    Unfinished,
    /// A snapshot tool row. The snapshot does not carry an outcome.
    Unknown,
}

/// Progress on a running tool.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolProgress {
    /// Progress value from the payload.
    pub progress: f64,
    /// Optional total.
    pub total: Option<f64>,
    /// Optional percent.
    pub percent: Option<i64>,
    /// Optional message.
    pub message: Option<String>,
}

/// One tool call. `diff` has terminal escapes removed.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCell {
    /// Stable id.
    pub id: CellId,
    /// Hermes `tool_id`, looked up among the current turn's cells only.
    pub tool_id: String,
    /// Tool name.
    pub name: String,
    /// Short preview from the start event when it had one.
    pub context: Option<String>,
    /// Arguments object from the start event when it had one.
    pub args: Option<Value>,
    /// Labels as raw JSON values.
    pub labels: Vec<Value>,
    /// Running, finished, unfinished or unknown.
    pub status: ToolStatus,
    /// Latest `tool.progress`, if any.
    pub progress: Option<ToolProgress>,
    /// Optional summary. The reducer does not invent one.
    pub summary: Option<String>,
    /// Result text, uncut. The gateway already cuts it.
    pub result: Option<String>,
    /// `inline_diff` with escape sequences removed. `None` when the remainder is blank.
    pub diff: Option<String>,
    /// Duration in seconds, when the payload had a number.
    pub duration_s: Option<f64>,
}

/// Why a notice was added.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeKind {
    /// The turn ended interrupted.
    Interrupted,
    /// An error outcome, an `error` event, or a failed submit.
    Error,
    /// `message.complete.warning`.
    Warning,
    /// Automation `display_text`.
    Automation,
    /// A diagnostic, including an unrecognised event type.
    Diagnostic,
    /// A system row from a snapshot.
    System,
}

/// A notice cell is final as soon as it exists.
#[derive(Clone, Debug, PartialEq)]
pub struct NoticeCell {
    /// Stable id.
    pub id: CellId,
    /// Notice class.
    pub kind: NoticeKind,
    /// Plain text.
    pub text: String,
    /// Raw `error_surface` object, when the completion had one.
    pub detail: Option<Value>,
}

/// Approval or clarification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptKind {
    /// `approval.request`.
    Approval,
    /// `clarify.request`.
    Clarify,
}

/// How a prompt left the open state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Settlement {
    /// This client was answering and Hermes settled it.
    Answered(String),
    /// Someone else, a sweep, or `already_resolved`.
    Elsewhere,
}

/// Prompt lifecycle. `Open` and `Answering` are never sealed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromptState {
    /// Waiting for an answer.
    Open,
    /// An answer effect has been emitted. Holds the choice or the answer text.
    Answering(String),
    /// Hermes has settled the prompt.
    Settled(Settlement),
}

/// One prompt. Final only when `sealed`.
#[derive(Clone, Debug, PartialEq)]
pub struct PromptCell {
    /// Stable id.
    pub id: CellId,
    /// Session, prompt id and generation.
    pub key: PromptKey,
    /// Approval or clarification.
    pub kind: PromptKind,
    /// Approval command, when present.
    pub command: Option<String>,
    /// Approval description, when present.
    pub description: Option<String>,
    /// Clarification question, when present.
    pub question: Option<String>,
    /// Offered approval choices, in wire order. Empty for a free-text clarification.
    pub choices: Vec<String>,
    /// Clarification `multi_select`. Missing or null is false.
    pub multi_select: bool,
    /// Optional approval edit object.
    pub edit: Option<Value>,
    /// Timeout hint for the application. The reducer does not time out.
    pub timeout_s: Option<f64>,
    /// Open, answering or settled.
    pub state: PromptState,
    /// Set for `Settled(Elsewhere)` at once, and for `Settled(Answered)` at `seal_all`
    /// or immediately when the turn is not running.
    pub sealed: bool,
}

/// Why an answer effect was refused. The first failing check wins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnswerRefusal {
    /// No prompt with that key.
    NoSuchPrompt,
    /// The prompt is the other kind.
    WrongKind,
    /// The prompt is already `Answering` or `Settled`.
    AlreadyAnswered,
    /// The approval choice is not in `choices`.
    ChoiceNotOffered,
}
