//! The gateway's queue, as this session sees it.
//!
//! Issue 232. Queue and steer. The bodies are stubs. A test fails by reaching one.

pub mod notices;
pub mod reorder;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::CanonicalPendingAdmission;

use crate::session::{Effect, SessionState};

/// One admission id from the gateway.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AdmissionId(pub String);

/// One client-chosen submission id.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SubmissionId(pub String);

/// Status of one pending row. `terminal` is [`RowStatus::Other`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RowStatus {
    /// Waiting to run.
    Queued,
    /// Claimed by a turn.
    Started,
    /// Lost across a gateway restart.
    Unknown,
    /// Any other status word, including `terminal`.
    Other(String),
}

/// Which actions the gateway offers for a row. Absent, null, or empty means neither.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Controls {
    /// The row can be steered.
    pub steer: bool,
    /// The row can be replaced.
    pub replace: bool,
}

/// One row from a snapshot or a queue fanout.
#[derive(Clone, Debug, PartialEq)]
pub struct QueuedRow {
    /// Gateway admission id.
    pub admission_id: AdmissionId,
    /// Submitter's submission id.
    pub input_id: SubmissionId,
    /// Prompt text.
    pub text: String,
    /// Row status.
    pub status: RowStatus,
    /// Outcome word, when the row has one.
    pub outcome: Option<String>,
    /// Offered actions.
    pub controls: Controls,
    /// True when this row is a steer into the running turn.
    pub steering: bool,
    /// Sort key. Lower comes first.
    pub sequence: i64,
    /// Generation stored on the row. Null when the gateway sent null.
    pub execution_generation: Option<i64>,
}

/// How long to wait for a fanout after a lost queue reply.
pub const RECONCILE_WAIT: Duration = Duration::from_secs(5);

/// One of this client's submits whose user cell is not drawn yet.
#[derive(Clone, Debug, PartialEq)]
pub struct LedgerEntry {
    /// Submitted text.
    pub text: String,
    /// Admission id from the receipt, once it arrives.
    pub admission_id: Option<AdmissionId>,
    /// Attachment objects to resend with an identical retry. Empty until submits carry them.
    pub attachments: Vec<serde_json::Value>,
    /// True when the idle rule already drew the cell at submit time.
    pub cell_drawn: bool,
    /// True while a lost submit is waiting for a fanout or the reconcile deadline.
    pub awaiting_reconcile: bool,
    /// True after the one allowed retry.
    pub retried: bool,
}

/// The one outstanding row call of a session. A submit is not stored here.
#[derive(Clone, Debug, PartialEq)]
pub struct InFlight {
    /// The call that is waiting.
    pub action: QueueAction,
    /// Row the call named. None for typed steer.
    pub admission_id: Option<AdmissionId>,
    /// Id sent with a replace.
    pub submission_id: Option<SubmissionId>,
    /// Text sent, so a retry can repeat it.
    pub params_text: Option<String>,
    /// True after the one allowed retry.
    pub retried: bool,
    /// True after a lost reply, until a fanout or the deadline.
    pub awaiting_reconcile: bool,
}

/// What the session knows about the gateway queue.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueState {
    /// Rows, sorted by sequence once a frame has been applied.
    pub rows: Vec<QueuedRow>,
    /// Highest authority epoch and, within it, the highest execution generation.
    pub generation_fence: Option<(i64, i64)>,
    /// Replay epoch and the last applied sequence of a queue frame.
    pub seq_fence: Option<(String, u64)>,
    /// Selected admission, when selection mode is open.
    pub selection: Option<AdmissionId>,
    /// Open edit, when one is open.
    pub edit: Option<EditState>,
    /// Move or send-now plan in progress.
    pub plan: Option<MovePlan>,
    /// The one outstanding row call.
    pub in_flight: Option<InFlight>,
    /// This client's submits whose cells are not drawn yet.
    pub ledger: BTreeMap<SubmissionId, LedgerEntry>,
    /// Old admission id to the id that replaced it.
    pub id_map: BTreeMap<AdmissionId, AdmissionId>,
    /// Admissions seen queued, or returned by a receipt.
    pub known: BTreeSet<AdmissionId>,
    /// Admissions whose user cell already exists.
    pub drawn: BTreeSet<AdmissionId>,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            generation_fence: None,
            seq_fence: None,
            selection: None,
            edit: None,
            plan: None,
            in_flight: None,
            ledger: BTreeMap::new(),
            id_map: BTreeMap::new(),
            known: BTreeSet::new(),
            drawn: BTreeSet::new(),
        }
    }
}

impl QueueState {
    /// Replace the rows from a snapshot and set both fences.
    ///
    /// Issue 232. Queue and steer.
    pub fn apply_snapshot(
        &mut self,
        pending: &[CanonicalPendingAdmission],
        authority_epoch: i64,
        execution_generation: i64,
        replay_epoch: &str,
        last_sequence: u64,
    ) {
        let _ = (
            pending,
            authority_epoch,
            execution_generation,
            replay_epoch,
            last_sequence,
        );
        todo!("Issue 232. Queue and steer.")
    }

    /// Apply one queue fanout. A frame without `pending` changes nothing.
    ///
    /// Issue 232. Queue and steer.
    pub fn apply_fanout(
        &mut self,
        info: &SessionInfoFrame,
        replay_epoch: &str,
        seq: u64,
    ) -> QueueDelta {
        let _ = (info, replay_epoch, seq);
        todo!("Issue 232. Queue and steer.")
    }

    /// How many rows count toward the indicator.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn queued_count(&self) -> usize {
        let _ = self;
        todo!("Issue 232. Queue and steer.")
    }

    /// True when the oldest counted row is lost.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn paused(&self) -> bool {
        let _ = self;
        todo!("Issue 232. Queue and steer.")
    }

    /// Rows the screen may show. `running` comes from the session turn.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn view(&self, running: bool) -> QueueView {
        let _ = (self, running);
        todo!("Issue 232. Queue and steer.")
    }
}

/// What one applied fanout changed.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueDelta {
    /// False when the frame was dropped or carried no queue.
    pub applied: bool,
    /// Rows that became started, and had not been reported yet.
    pub started: Vec<QueuedRow>,
    /// Rows that came back from steering.
    pub returned_from_steer: Vec<AdmissionId>,
    /// Rows that left the queue.
    pub removed: Vec<AdmissionId>,
}

/// Which way a selected row moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Toward the head.
    Up,
    /// Toward the tail.
    Down,
}

/// A queue call, including a plain submit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueAction {
    /// `prompt.submit`.
    Submit,
    /// Save an edit.
    Edit,
    /// Cancel a queued row.
    Remove,
    /// Discard a lost row.
    Discard,
    /// Steer a queued row.
    Steer,
    /// Steer typed text.
    SteerText,
    /// Reorder.
    Move,
    /// Move to the head and interrupt.
    SendNow,
}

/// An edit that has not been saved or cancelled.
#[derive(Clone, Debug, PartialEq)]
pub struct EditState {
    /// Row being edited.
    pub admission_id: AdmissionId,
    /// Text at the moment the edit opened.
    pub original_text: String,
    /// 1-based place among counted rows when the edit opened.
    pub index: usize,
}

/// One replace in a move plan.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveStep {
    /// Row to retire.
    pub admission_id: AdmissionId,
    /// Text to admit at the back.
    pub text: String,
    /// Fresh id for that admission.
    pub submission_id: SubmissionId,
}

/// A series of replaces that reaches a target order, then maybe an interrupt.
#[derive(Clone, Debug, PartialEq)]
pub struct MovePlan {
    /// Replaces still to send, in order.
    pub steps: Vec<MoveStep>,
    /// Index of the next step.
    pub next: usize,
    /// Wanted order of the queued rows.
    pub target: Vec<AdmissionId>,
    /// Generation to interrupt after the last step. None for a plain move.
    pub then_interrupt: Option<i64>,
}

/// What the queue section draws.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueView {
    /// Counted rows, in sequence order.
    pub rows: Vec<QueuedRow>,
    /// Steering rows. The newest is last.
    pub steering: Vec<QueuedRow>,
    /// Index into `rows` of the selection, when selection is open.
    pub selected: Option<usize>,
    /// The oldest counted row is lost.
    pub paused: bool,
    /// A turn is running.
    pub running: bool,
    /// Edit title numbers `(i, n)` when an edit is open.
    pub editing: Option<(usize, usize)>,
    /// Own submits still waiting for a lost reply to be settled.
    pub sending: usize,
}

/// A key or command aimed at the queue.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueIntent {
    /// Move the selection.
    Select(Direction),
    /// Leave selection. Does not interrupt.
    Leave,
    /// Load the selected row into the composer.
    BeginEdit,
    /// Save the composer text over the open edit.
    SaveEdit(String),
    /// Close the edit and keep the original row.
    CancelEdit,
    /// Remove or discard the selected row.
    Remove,
    /// Steer the selected row.
    Steer,
    /// Send the selected row now.
    SendNow,
    /// Move the selected row.
    Move(Direction),
    /// Steer this typed text.
    SteerText(String),
    /// Send the draft now.
    SendDraftNow(String),
    /// Enter selection on the newest counted row.
    EnterSelection,
}

/// Mints the submission ids a plan and a submit need.
pub trait IdSource {
    /// The next id. Tests use `sub-1`, `sub-2`, and so on.
    fn submission_id(&mut self) -> SubmissionId;
}

/// Apply one queue intent. Issue 232. Queue and steer.
pub fn update_queue(
    state: &mut SessionState,
    intent: QueueIntent,
    ids: &mut dyn IdSource,
) -> Vec<Effect> {
    let _ = (state, intent, ids);
    todo!("Issue 232. Queue and steer.")
}
