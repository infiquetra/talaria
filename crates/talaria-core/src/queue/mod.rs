//! The gateway's queue, as this session sees it.
//!
//! Issue 232. Queue and steer. The bodies are stubs. A test fails by reaching one.

pub(crate) mod flow;
mod intents;
pub mod notices;
pub mod reorder;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::{
    CanonicalPendingAdmission, CanonicalPendingAdmissionControlsItem, Status,
};

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
#[derive(Clone, Debug, Default, PartialEq)]
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
    /// Rows already reported as started, so a repeat is not reported again.
    pub(crate) started_seen: BTreeSet<AdmissionId>,
    /// Admission to the id of the session's own waiting submit it belongs to.
    pub(crate) fifo_of: BTreeMap<AdmissionId, String>,
    /// Draft submits that send the draft now once their receipt reads `queued`.
    pub(crate) send_now_drafts: BTreeSet<SubmissionId>,
    /// Target of a finished plan whose final order still has to be checked.
    pub(crate) verify: Option<Vec<AdmissionId>>,
    /// Text the person should get back, oldest first, until taken.
    pub(crate) returned: Vec<String>,
    /// True once an unreadable queue frame has been noted for this session.
    pub(crate) decode_noted: bool,
    /// Counter for submission ids minted without an [`IdSource`].
    pub(crate) minted: u64,
    /// Steering rows whose steer the transcript already shows.
    pub(crate) steer_noted: BTreeSet<AdmissionId>,
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
        self.rows = sorted_rows(pending);
        self.generation_fence = Some((authority_epoch, execution_generation));
        self.seq_fence = Some((replay_epoch.to_string(), last_sequence));
        for row in &self.rows {
            if row.steering {
                // The resumed history already holds the steer.
                self.steer_noted.insert(row.admission_id.clone());
            }
            if row.status == RowStatus::Queued && !row.steering {
                self.known.insert(row.admission_id.clone());
            }
            if row.status == RowStatus::Started && !row.steering {
                // The history of a resumed session already holds this prompt.
                self.started_seen.insert(row.admission_id.clone());
                self.drawn.insert(row.admission_id.clone());
            }
        }
    }

    /// Apply one queue fanout. A frame without `pending` changes nothing.
    ///
    /// Issue 232. Queue and steer. An empty `replay_epoch` skips the sequence check.
    pub fn apply_fanout(
        &mut self,
        info: &SessionInfoFrame,
        replay_epoch: &str,
        seq: u64,
    ) -> QueueDelta {
        let mut delta = QueueDelta {
            applied: false,
            started: Vec::new(),
            returned_from_steer: Vec::new(),
            removed: Vec::new(),
        };
        let Some(pending) = info.pending.as_deref() else {
            return delta;
        };
        if self.is_stale(info, replay_epoch, seq) {
            return delta;
        }
        self.advance_fences(info, replay_epoch, seq);
        delta.applied = true;

        let counted_before = self.counted_ids();
        let previous = std::mem::replace(&mut self.rows, sorted_rows(pending));
        for row in &self.rows {
            if row.status == RowStatus::Started && !row.steering {
                if self.started_seen.insert(row.admission_id.clone()) {
                    delta.started.push(row.clone());
                }
            } else if row.status == RowStatus::Queued && !row.steering {
                self.known.insert(row.admission_id.clone());
            }
            let was_steering = previous
                .iter()
                .any(|old| old.admission_id == row.admission_id && old.steering);
            if was_steering && !row.steering && row.status == RowStatus::Queued {
                delta.returned_from_steer.push(row.admission_id.clone());
            }
        }
        for old in &previous {
            let counted =
                !old.steering && matches!(old.status, RowStatus::Queued | RowStatus::Unknown);
            if (counted || old.steering)
                && !self
                    .rows
                    .iter()
                    .any(|row| row.admission_id == old.admission_id)
            {
                delta.removed.push(old.admission_id.clone());
            }
        }

        for (key, entry) in &mut self.ledger {
            if let Some(row) = self.rows.iter().find(|row| row.input_id == *key) {
                entry.admission_id.get_or_insert(row.admission_id.clone());
                self.known.insert(row.admission_id.clone());
            }
            entry.awaiting_reconcile = false;
        }
        self.end_edit_if_gone();
        self.fix_selection(&counted_before);
        delta
    }

    /// How many rows count toward the indicator.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn queued_count(&self) -> usize {
        self.counted().count()
    }

    /// True when the oldest counted row is lost.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn paused(&self) -> bool {
        self.counted()
            .next()
            .is_some_and(|row| row.status == RowStatus::Unknown)
    }

    /// Rows the screen may show. `running` comes from the session turn.
    ///
    /// Issue 232. Queue and steer.
    #[must_use]
    pub fn view(&self, running: bool) -> QueueView {
        let rows: Vec<QueuedRow> = self.counted().cloned().collect();
        let selected = self
            .selection
            .as_ref()
            .and_then(|id| rows.iter().position(|row| row.admission_id == *id));
        QueueView {
            steering: self
                .rows
                .iter()
                .filter(|row| row.steering && row.status == RowStatus::Started)
                .cloned()
                .collect(),
            selected,
            paused: self.paused(),
            running,
            editing: self.edit.as_ref().map(|edit| (edit.index, rows.len())),
            sending: self
                .ledger
                .values()
                .filter(|entry| entry.awaiting_reconcile)
                .count(),
            rows,
        }
    }

    /// Text the person should get back, oldest first. Emptied by the call.
    ///
    /// The reducer cannot see the composer. It records every prompt it hands back, and the
    /// application loads the first one into an empty composer.
    #[must_use]
    pub fn take_returned(&mut self) -> Vec<String> {
        std::mem::take(&mut self.returned)
    }

    /// Rows that count toward the indicator, in sequence order.
    pub(crate) fn counted(&self) -> impl Iterator<Item = &QueuedRow> {
        self.rows.iter().filter(|row| {
            !row.steering && matches!(row.status, RowStatus::Queued | RowStatus::Unknown)
        })
    }

    pub(crate) fn counted_ids(&self) -> Vec<AdmissionId> {
        self.counted().map(|row| row.admission_id.clone()).collect()
    }

    /// Ids of `queued` rows only. Plans never hold anything else.
    pub(crate) fn queued_ids(&self) -> Vec<AdmissionId> {
        self.counted()
            .filter(|row| row.status == RowStatus::Queued)
            .map(|row| row.admission_id.clone())
            .collect()
    }

    pub(crate) fn row(&self, id: &AdmissionId) -> Option<&QueuedRow> {
        self.rows.iter().find(|row| row.admission_id == *id)
    }

    /// True while a plan or a row call is outstanding.
    pub(crate) fn busy(&self) -> bool {
        self.plan.is_some() || self.in_flight.is_some()
    }

    fn is_stale(&self, info: &SessionInfoFrame, replay_epoch: &str, seq: u64) -> bool {
        if let (Some(incoming), Some((epoch, generation))) =
            (info.authority_epoch, self.generation_fence)
        {
            if incoming < epoch {
                return true;
            }
            if incoming == epoch && info.execution_generation.is_some_and(|g| g < generation) {
                return true;
            }
        }
        if !replay_epoch.is_empty()
            && let Some((epoch, last)) = &self.seq_fence
        {
            return epoch == replay_epoch && seq <= *last;
        }
        false
    }

    fn advance_fences(&mut self, info: &SessionInfoFrame, replay_epoch: &str, seq: u64) {
        if let Some(incoming) = info.authority_epoch {
            let highest = match self.generation_fence {
                Some((epoch, generation)) if epoch == incoming => {
                    generation.max(info.execution_generation.unwrap_or(generation))
                }
                _ => info.execution_generation.unwrap_or(0),
            };
            self.generation_fence = Some((incoming, highest));
        }
        if !replay_epoch.is_empty() {
            self.seq_fence = Some((replay_epoch.to_string(), seq));
        }
    }

    /// Edit mode ends when the row it edits started or left the queue.
    fn end_edit_if_gone(&mut self) {
        let still_queued = self.edit.as_ref().is_none_or(|edit| {
            self.rows.iter().any(|row| {
                row.admission_id == edit.admission_id
                    && row.status == RowStatus::Queued
                    && !row.steering
            })
        });
        if !still_queued {
            self.edit = None;
        }
    }

    /// Keep the selection on a row that is still counted.
    ///
    /// A selected row that was replaced follows the id map. One that left moves to the row
    /// now at the same place, or the last row, or ends the selection.
    pub(crate) fn fix_selection(&mut self, counted_before: &[AdmissionId]) {
        let Some(selected) = self.selection.clone() else {
            return;
        };
        let counted = self.counted_ids();
        if counted.contains(&selected) {
            return;
        }
        if let Some(next) = self.id_map.get(&selected)
            && counted.contains(next)
        {
            self.selection = Some(next.clone());
            return;
        }
        // The selected row is being replaced right now. Its receipt moves the selection.
        if self.in_flight.as_ref().is_some_and(|call| {
            call.admission_id.as_ref() == Some(&selected) && call.submission_id.is_some()
        }) {
            return;
        }
        let place = counted_before
            .iter()
            .position(|id| *id == selected)
            .unwrap_or(counted.len());
        self.selection = counted
            .get(place.min(counted.len().saturating_sub(1)))
            .cloned();
    }
}

fn sorted_rows(pending: &[CanonicalPendingAdmission]) -> Vec<QueuedRow> {
    let mut rows: Vec<QueuedRow> = pending.iter().map(QueuedRow::from_wire).collect();
    rows.sort_by_key(|row| row.sequence);
    rows
}

impl QueuedRow {
    fn from_wire(wire: &CanonicalPendingAdmission) -> Self {
        let offered = |item| {
            wire.controls
                .as_deref()
                .is_some_and(|controls| controls.contains(&item))
        };
        Self {
            admission_id: AdmissionId(wire.admission_id.clone()),
            input_id: SubmissionId(wire.input_id.clone()),
            text: wire.text.clone(),
            status: match wire.status {
                Status::Queued => RowStatus::Queued,
                Status::Started => RowStatus::Started,
                Status::Unknown => RowStatus::Unknown,
                Status::Terminal => RowStatus::Other("terminal".to_string()),
            },
            outcome: wire.outcome.clone(),
            controls: Controls {
                steer: offered(CanonicalPendingAdmissionControlsItem::Steer),
                replace: offered(CanonicalPendingAdmissionControlsItem::Replace),
            },
            steering: wire.steering.unwrap_or(false),
            sequence: wire.sequence,
            execution_generation: wire.execution_generation,
        }
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
    intents::update_queue(state, intent, ids)
}
