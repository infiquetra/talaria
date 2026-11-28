//! Transcript cells. Text is raw. Wrapping and colour belong to later units.

use std::sync::Arc;

use serde_json::Value;

use super::{CellId, PromptKey};
use crate::automation::HermesCommandCell;
use crate::background::BackgroundCell;
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
    /// One of Hermes's own command replies, final when created. Issue #236.
    HermesCommand(HermesCommandCell),
    /// One finished background task. Final when created.
    Background(BackgroundCell),
}

/// The requested finished assistant reply does not exist.
#[derive(Debug, PartialEq, Eq)]
pub enum ReplyMiss {
    Missing,
}

/// Select a finished, non-interim assistant reply, counting backwards from one.
pub fn assistant_reply(cells: &[Cell], n: u32) -> Result<String, ReplyMiss> {
    let index = n.checked_sub(1).ok_or(ReplyMiss::Missing)?;
    cells
        .iter()
        .rev()
        .filter_map(|cell| match cell {
            Cell::Assistant(reply) if reply.sealed && !reply.interim => Some(reply),
            _ => None,
        })
        .nth(index as usize)
        .map(|reply| super::text::strip_terminal_escapes(&reply.text))
        .ok_or(ReplyMiss::Missing)
}

impl Cell {
    /// A warning or diagnostic that can arrive before a conversation starts.
    pub fn is_advisory_notice(&self) -> bool {
        matches!(
            self,
            Self::Notice(NoticeCell {
                kind: NoticeKind::Warning | NoticeKind::Diagnostic,
                body: None,
                ..
            })
        )
    }

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
            Self::HermesCommand(_) => true,
            Self::Background(_) => true,
        }
    }

    /// Whether this cell opens a turn: a prompt the person sent, or the notice that stands for a
    /// turn Hermes started itself (a notice that keeps what Hermes handed the agent as its body).
    #[must_use]
    pub fn starts_turn(&self) -> bool {
        match self {
            Self::User(_) => true,
            Self::Notice(notice) => notice.body.is_some(),
            _ => false,
        }
    }
}

/// Mark the reasoning that ends each completed turn among `cells` without a reply: in every
/// turn (a run of cells up to the next one that starts a turn) that wrote no reply text, its
/// last reasoning block, unless a tool ran after it.
pub(crate) fn mark_unanswered_reasoning(cells: &mut [Cell]) {
    let mut end = cells.len();
    while end > 0 {
        let start = cells[..end]
            .iter()
            .rposition(Cell::starts_turn)
            .unwrap_or(0);
        let turn = &mut cells[start..end];
        let replied = turn
            .iter()
            .any(|cell| matches!(cell, Cell::Assistant(reply) if !reply.text.trim().is_empty()));
        if !replied
            && let Some(last) = turn
                .iter()
                .rposition(|cell| matches!(cell, Cell::Reasoning(_)))
            && !turn[last..]
                .iter()
                .any(|cell| matches!(cell, Cell::Tool(_)))
            && let Cell::Reasoning(reasoning) = &mut turn[last]
        {
            reasoning.no_reply = true;
        }
        end = start;
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, NoticeCell, UserCell};
    use crate::session::{CellId, NoticeKind};

    fn notice(body: Option<&str>) -> Cell {
        Cell::Notice(NoticeCell {
            id: CellId::new(1),
            kind: NoticeKind::Automation,
            text: "Subagent results returned to the agent".to_owned(),
            detail: None,
            body: body.map(str::to_owned),
        })
    }

    #[test]
    fn a_prompt_and_a_turn_hermes_started_open_a_turn_and_a_plain_notice_does_not() {
        let user = Cell::User(UserCell {
            id: CellId::new(1),
            text: "a question".to_owned(),
            expanded_paste: None,
        });
        assert!(user.starts_turn());
        assert!(notice(Some("synthetic result")).starts_turn());
        assert!(!notice(None).starts_turn());
    }
}

/// A user cell is final as soon as it exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserCell {
    /// Stable id.
    pub id: CellId,
    /// Text shown inline, including any paste labels.
    pub text: String,
    /// Full user text when the inline form abbreviates a paste.
    pub expanded_paste: Option<Arc<str>>,
}

impl UserCell {
    /// The text shown and searched in the transcript.
    #[must_use]
    pub fn transcript_text(&self) -> &str {
        self.expanded_paste.as_deref().unwrap_or(&self.text)
    }
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
    /// Hermes's end-of-turn reasoning for a turn that ran tools. It is the latest step's
    /// thinking, shown once after that step was already drawn.
    pub latest_step: bool,
    /// The turn completed with this block as its last output: it wrote no reply text, and no
    /// tool ran after the block. The block is then all the turn has to show, so the main screen
    /// draws it in full even when reasoning is collapsed. A block committed before its turn
    /// ended, such as one sealed by an approval request, keeps the form it was committed in.
    pub no_reply: bool,
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
    /// What Hermes handed the agent, when the notice stands for a turn Hermes wrote; a resumed
    /// snapshot draws such a notice again. The main screen shows only its line count; the
    /// transcript view shows it whole. A body that only repeats the text shows nowhere.
    pub body: Option<String>,
}

/// Approval or clarification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptKind {
    /// `approval.request`.
    Approval,
    /// `clarify.request`.
    Clarify,
    /// A waiting server request recovered from a snapshot.
    ServerRequest,
}

/// Whether Talaria can answer a recovered server request.
#[must_use]
pub fn is_answerable(method: &str) -> bool {
    matches!(
        method,
        "sudo" | "secret" | "vault.code" | "vault.unlock_prompt"
    )
}

/// The configured wait limit for a recovered request, when known.
#[must_use]
pub fn recovered_limit_seconds(method: &str) -> Option<u32> {
    match method {
        "sudo" | "secret" | "vault.unlock_prompt" => Some(120),
        "vault.code" | "vault.save_login" => Some(180),
        _ => None,
    }
}

/// How a prompt left the open state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Settlement {
    /// This client was answering and Hermes settled it.
    Answered(String),
    /// This client sent a recovered server request answer.
    Sent,
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
