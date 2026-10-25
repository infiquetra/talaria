//! [`SessionState`] and the values its accessors return.

use hermes_contract::schemas::CanonicalSnapshot;
use std::collections::{HashMap, VecDeque};

use super::{
    Cell, CellId, NoticeCell, NoticeKind, PromptCell, PromptKey, PromptState, SessionId,
    Settlement, ToolStatus, UserCell,
};
use crate::side::SideTasks;

/// Whether a turn is idle, running, or finished.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TurnState {
    /// No turn is open.
    Idle,
    /// A turn is open. Late writes still apply unless the interrupt latch is set.
    Running,
    /// `message.complete` has ended the turn.
    Finished(TurnOutcome),
}

/// How a finished turn ended. Taken from `message.complete.status`, or forced by the latch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnOutcome {
    /// `complete`, or a missing or null status.
    Completed,
    /// `error`, or any status string other than the three Hermes sends.
    Failed,
    /// `interrupted`, or a completion while the interrupt latch is set.
    Interrupted,
}

/// Replay position. A cursor with no epoch adopts the next event's epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCursor {
    /// `replay_epoch`, once one has been seen or restored.
    pub replay_epoch: Option<String>,
    /// Highest applied `seq`. Events at or below this, in the same epoch, are repeats.
    pub last_seq: u64,
}

/// One status note. The latest is also `SessionState::status`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusNote {
    /// `status.update` kind.
    pub kind: String,
    /// `status.update` text.
    pub text: String,
}

/// Checklist stored as raw JSON. The highest revision wins; an equal revision replaces.
#[derive(Clone, Debug, PartialEq)]
pub struct TodoState {
    /// Hermes revision.
    pub revision: i64,
    /// Raw todo items.
    pub todos: Vec<serde_json::Value>,
}

/// Usage figures. Every field stays `None` until a frame sets it. Zero is a reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    /// Model name. An empty string is treated as absent.
    pub model: Option<String>,
    /// Nested `input`.
    pub input: Option<i64>,
    /// Nested `output`.
    pub output: Option<i64>,
    /// Nested `reasoning`.
    pub reasoning: Option<i64>,
    /// Nested `prompt`.
    pub prompt: Option<i64>,
    /// Nested `completion`.
    pub completion: Option<i64>,
    /// Nested `total`.
    pub total: Option<i64>,
    /// Nested `calls`.
    pub calls: Option<i64>,
    /// Nested `context_used`.
    pub context_used: Option<i64>,
    /// Nested `context_max`, or top-level `context_length` while this is empty.
    pub context_max: Option<i64>,
    /// Nested `context_percent`.
    pub context_percent: Option<f64>,
    /// Nested `cache_hit_pct`.
    pub cache_hit_pct: Option<f64>,
    /// Nested `cache_read`, or top-level `cache_read_tokens` while this is empty.
    pub cache_read: Option<i64>,
    /// Nested `cache_write`, or top-level `cache_write_tokens` while this is empty.
    pub cache_write: Option<i64>,
    /// Nested `compressions`, or top-level `compression_count` while this is empty.
    pub compressions: Option<i64>,
    /// Nested `cost_usd`.
    pub cost_usd: Option<f64>,
    /// Nested `cost_status`.
    pub cost_status: Option<String>,
}

/// The whole transcript model for one session.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionState {
    pub(crate) session_id: SessionId,
    pub(crate) cells: Vec<Cell>,
    pub(crate) final_prefix: usize,
    pub(crate) turn: TurnState,
    pub(crate) turn_start: usize,
    pub(crate) assistant: Option<usize>,
    pub(crate) reasoning: Option<usize>,
    pub(crate) had_reasoning: bool,
    pub(crate) tools: HashMap<String, usize>,
    pub(crate) prompts: HashMap<PromptKey, usize>,
    pub(crate) submissions: VecDeque<Submission>,
    pub(crate) interrupted: bool,
    pub(crate) interrupt_notice: bool,
    pub(crate) activity: Option<String>,
    pub(crate) status: Option<StatusNote>,
    pub(crate) status_log: Vec<StatusNote>,
    pub(crate) usage: Usage,
    pub(crate) todo: Option<TodoState>,
    pub(crate) cursor: ReplayCursor,
    pub(crate) execution_generation: Option<i64>,
    pub(crate) subscription_id: Option<String>,
    pub(crate) stored_session_id: Option<String>,
    pub(crate) needs_resume: bool,
    pub(crate) unknown: Vec<String>,
    pub(crate) queued_admissions_ignored: usize,
    /// The tool name `tool.generating` reported, until the call starts or the turn ends.
    pub(crate) drafting: Option<String>,
    pub(crate) side: SideTasks,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Submission {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) placed: bool,
}

impl SessionState {
    /// An empty session. No cells, idle turn, no cursor epoch.
    pub fn empty(session_id: SessionId) -> Self {
        Self {
            session_id,
            cells: Vec::new(),
            final_prefix: 0,
            turn: TurnState::Idle,
            turn_start: 0,
            assistant: None,
            reasoning: None,
            had_reasoning: false,
            tools: HashMap::new(),
            prompts: HashMap::new(),
            submissions: VecDeque::new(),
            interrupted: false,
            interrupt_notice: false,
            activity: None,
            status: None,
            status_log: Vec::new(),
            usage: Usage::default(),
            todo: None,
            cursor: ReplayCursor {
                replay_epoch: None,
                last_seq: 0,
            },
            execution_generation: None,
            subscription_id: None,
            stored_session_id: None,
            needs_resume: false,
            unknown: Vec::new(),
            queued_admissions_ignored: 0,
            drafting: None,
            side: SideTasks::default(),
        }
    }

    /// Build state from a `session.create` or `session.resume` snapshot.
    ///
    /// Rows come from `display_messages` when that array is present, otherwise `messages`.
    /// Open prompts come from `prompts`. `pending` is not turned into cells.
    pub fn from_snapshot(snapshot: CanonicalSnapshot) -> Self {
        super::snapshot::from_snapshot(snapshot)
    }

    /// Every cell, oldest first.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The tool name still being written, if the gateway said one.
    ///
    /// Set by `tool.generating`, which carries no tool id, so it is a status and never a cell.
    /// Issue 221.
    #[must_use]
    pub fn drafting_tool(&self) -> Option<&str> {
        self.drafting.as_deref()
    }

    /// Pending shell and side work.
    pub fn side(&self) -> &SideTasks {
        &self.side
    }

    /// Pending shell and side work, to change.
    pub fn side_mut(&mut self) -> &mut SideTasks {
        &mut self.side
    }

    /// Insert one final cell where it commits at once.
    pub fn insert_final(&mut self, cell: Cell) {
        let at = self.final_prefix.min(self.cells.len());
        self.cells.insert(at, cell);
        self.advance_prefix();
    }

    /// How many leading cells are final. The count never shrinks.
    pub fn final_prefix_len(&self) -> usize {
        self.final_prefix
    }

    /// Cells after [`Self::final_prefix_len`].
    pub fn live_tail(&self) -> &[Cell] {
        &self.cells[self.final_prefix..]
    }

    /// Idle, running, or finished.
    pub fn turn(&self) -> &TurnState {
        &self.turn
    }

    /// Latest `thinking.delta` text. Empty text clears it.
    pub fn activity(&self) -> Option<&str> {
        self.activity.as_deref()
    }

    /// Latest status note, if one is set.
    pub fn status(&self) -> Option<&StatusNote> {
        self.status.as_ref()
    }

    /// The last 50 status notes that were recorded. A repeat of the latest is not stored.
    pub fn status_log(&self) -> &[StatusNote] {
        &self.status_log
    }

    /// Merged usage.
    pub fn usage(&self) -> &Usage {
        &self.usage
    }

    /// Checklist, if a `todo.updated` or a snapshot `todo_state` has been seen.
    pub fn todo(&self) -> Option<&TodoState> {
        self.todo.as_ref()
    }

    /// Prompt cells in `Open` or `Answering`. Settled prompts are omitted.
    pub fn open_prompts(&self) -> impl Iterator<Item = &PromptCell> {
        self.cells.iter().filter_map(|cell| match cell {
            Cell::Prompt(prompt)
                if matches!(prompt.state, PromptState::Open | PromptState::Answering(_)) =>
            {
                Some(prompt)
            }
            _ => None,
        })
    }

    /// Epoch and last applied seq.
    pub fn cursor(&self) -> ReplayCursor {
        self.cursor.clone()
    }

    /// Generation of the current or latest turn, from `message.start` or the snapshot.
    pub fn execution_generation(&self) -> Option<i64> {
        self.execution_generation
    }

    /// Snapshot `subscription_id`, when this state came from a snapshot.
    pub fn subscription_id(&self) -> Option<&str> {
        self.subscription_id.as_deref()
    }

    /// Set on `session.replay_gap` or an epoch change. The reducer emits no resume effect.
    pub fn needs_resume(&self) -> bool {
        self.needs_resume
    }

    /// Unknown event types noted on this connection, in the order first seen. At most 8.
    pub fn unknown_event_types(&self) -> &[String] {
        &self.unknown
    }

    /// Queued admissions ignored while building from a snapshot. Zero for [`Self::empty`].
    pub fn queued_admissions_ignored(&self) -> usize {
        self.queued_admissions_ignored
    }

    pub(crate) fn next_id(&self) -> CellId {
        // A Vec cannot hold more than isize::MAX cells, so this conversion always fits.
        CellId::new(
            u64::try_from(self.cells.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
    }

    pub(crate) fn user(&mut self, text: String) {
        self.cells.push(Cell::User(UserCell {
            id: self.next_id(),
            text,
        }));
    }

    pub(crate) fn notice(
        &mut self,
        kind: NoticeKind,
        text: String,
        detail: Option<serde_json::Value>,
    ) {
        self.cells.push(Cell::Notice(NoticeCell {
            id: self.next_id(),
            kind,
            text,
            detail,
        }));
    }

    pub(crate) fn seal_assistant(&mut self) {
        if let Some(index) = self.assistant.take()
            && let Cell::Assistant(cell) = &mut self.cells[index]
        {
            cell.sealed = true;
        }
    }

    pub(crate) fn seal_reasoning(&mut self) {
        if let Some(index) = self.reasoning.take()
            && let Cell::Reasoning(cell) = &mut self.cells[index]
        {
            cell.sealed = true;
        }
    }

    pub(crate) fn seal_all(&mut self) {
        self.seal_assistant();
        self.seal_reasoning();
        for cell in &mut self.cells[self.final_prefix..] {
            match cell {
                Cell::Tool(tool) if tool.status == ToolStatus::Running => {
                    tool.status = ToolStatus::Unfinished
                }
                Cell::Prompt(prompt)
                    if matches!(prompt.state, PromptState::Settled(Settlement::Answered(_))) =>
                {
                    prompt.sealed = true
                }
                _ => {}
            }
        }
    }

    pub(crate) fn advance_prefix(&mut self) {
        while self
            .cells
            .get(self.final_prefix)
            .is_some_and(Cell::is_final)
        {
            self.final_prefix += 1;
        }
    }

    pub(crate) fn begin_turn(&mut self) {
        self.seal_all();
        self.turn_start = self.cells.len();
        self.turn = TurnState::Running;
        self.tools.clear();
        self.had_reasoning = false;
        self.interrupt_notice = false;
    }
}
