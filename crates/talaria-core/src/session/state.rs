//! [`SessionState`] and the values its accessors return.

use hermes_contract::schemas::CanonicalSnapshot;

use super::{Cell, PromptCell, SessionId};

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
    session_id: SessionId,
}

impl SessionState {
    /// An empty session. No cells, idle turn, no cursor epoch.
    pub fn empty(session_id: SessionId) -> Self {
        let _ = session_id;
        todo!("P1-05")
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
        todo!("P1-05")
    }

    /// How many leading cells are final. The count never shrinks.
    pub fn final_prefix_len(&self) -> usize {
        todo!("P1-05")
    }

    /// Cells after [`Self::final_prefix_len`].
    pub fn live_tail(&self) -> &[Cell] {
        todo!("P1-05")
    }

    /// Idle, running, or finished.
    pub fn turn(&self) -> &TurnState {
        todo!("P1-05")
    }

    /// Latest `thinking.delta` text. Empty text clears it.
    pub fn activity(&self) -> Option<&str> {
        todo!("P1-05")
    }

    /// Latest status note, if one is set.
    pub fn status(&self) -> Option<&StatusNote> {
        todo!("P1-05")
    }

    /// The last 50 status notes that were recorded. A repeat of the latest is not stored.
    pub fn status_log(&self) -> &[StatusNote] {
        todo!("P1-05")
    }

    /// Merged usage.
    pub fn usage(&self) -> &Usage {
        todo!("P1-05")
    }

    /// Checklist, if a `todo.updated` or a snapshot `todo_state` has been seen.
    pub fn todo(&self) -> Option<&TodoState> {
        todo!("P1-05")
    }

    /// Prompt cells in `Open` or `Answering`. Settled prompts are omitted.
    pub fn open_prompts(&self) -> impl Iterator<Item = &PromptCell> {
        let _ = self;
        todo!("P1-05");
        #[allow(unreachable_code)]
        std::iter::empty::<&PromptCell>()
    }

    /// Epoch and last applied seq.
    pub fn cursor(&self) -> ReplayCursor {
        todo!("P1-05")
    }

    /// Generation of the current or latest turn, from `message.start` or the snapshot.
    pub fn execution_generation(&self) -> Option<i64> {
        todo!("P1-05")
    }

    /// Snapshot `subscription_id`, when this state came from a snapshot.
    pub fn subscription_id(&self) -> Option<&str> {
        todo!("P1-05")
    }

    /// Set on `session.replay_gap` or an epoch change. The reducer emits no resume effect.
    pub fn needs_resume(&self) -> bool {
        todo!("P1-05")
    }

    /// Unknown event types noted on this connection, in the order first seen. At most 8.
    pub fn unknown_event_types(&self) -> &[String] {
        todo!("P1-05")
    }

    /// Queued admissions ignored while building from a snapshot. Zero for [`Self::empty`].
    pub fn queued_admissions_ignored(&self) -> usize {
        todo!("P1-05")
    }
}
