//! The automation reducer: standing state in, effects out.
//!
//! Behavior contract: issue 236.

use std::collections::BTreeSet;

use hermes_contract::schemas::{
    GoalSnapshot, GoalSnapshotWaitBarrier, HeartbeatSnapshot, LoopSnapshot, SessionControlSnapshot,
    Type as WaitTargetType,
};

use super::present::PLAN_MODE_NOTE;
use super::rows::rows;

/// What reached the automation state.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AutomationEvent {
    /// A foreground session attached; read everything once.
    Attached {
        /// The session id.
        session_id: String,
    },
    /// The foreground session went away with no replacement.
    Detached,
    /// A control read answered.
    ControlRead {
        /// The token from its effect.
        token: u64,
        /// The decoded view or the failure.
        result: Result<ControlView, ReadFailure>,
    },
    /// A `session.control.update` event arrived.
    ControlUpdated {
        /// The event's session id.
        session_id: String,
        /// The event payload's `control` value.
        raw: serde_json::Value,
    },
    /// A `status.update` note arrived.
    StatusNote {
        /// The event's session id.
        session_id: String,
        /// The note kind.
        kind: String,
        /// The note text.
        text: String,
    },
    /// A presented command finished, whatever its result.
    CommandFinished {
        /// The session the command was sent to.
        session_id: String,
        /// The canonical command name.
        canonical: String,
        /// What the command did.
        result: CommandResult,
    },
    /// A mode read answered.
    ModeRead {
        /// The session the read went to.
        session_id: String,
        /// Which mode was read.
        key: ModeKey,
        /// The token from its effect.
        token: u64,
        /// The configured value, if any, or the failure.
        result: Result<Option<String>, ReadFailure>,
    },
}

/// What a presented command did, from the submit path's outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResult {
    /// The command ran and answered.
    Output,
    /// The command queued a turn.
    Submit,
    /// The outcome is unknown.
    Unknown,
    /// The command never ran.
    NotRun,
}

/// How a read failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadFailure {
    /// The gateway refused, with its reason.
    Refused(String),
    /// The method does not exist on this Hermes.
    MethodMissing,
    /// No reply before the deadline.
    TimedOut,
    /// The connection was lost or the reply did not decode as a value.
    Lost,
    /// A reply arrived that is not usable.
    BadResult,
}

/// Which session mode a read targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKey {
    /// The approval bypass.
    Yolo,
    /// The speed tier.
    Fast,
}

/// What the app does after one update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutomationEffect {
    /// Read the standing state.
    ReadControl {
        /// The session to read.
        session_id: String,
        /// The token the answer carries back.
        token: u64,
    },
    /// Read one session mode.
    ReadMode {
        /// The session to read.
        session_id: String,
        /// Which mode to read.
        key: ModeKey,
        /// The token the answer carries back.
        token: u64,
    },
    /// Show one transcript notice.
    Notice {
        /// The notice text.
        text: String,
    },
    /// The rows changed; draw again.
    Redraw,
}

/// A session mode value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeValue<T> {
    /// Never read.
    NotRead,
    /// Read cleanly.
    Known(T),
    /// A read failed after the value may have changed.
    Unknown,
}

/// The speed tier of a session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FastMode {
    /// The normal tier.
    Normal,
    /// The fast tier.
    Fast,
    /// The ultrafast tier.
    Ultrafast,
}

/// Why a snapshot did not decode, for logs only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);

/// Whether the standing state is shown, and why not.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ControlStatus {
    /// Never read.
    #[default]
    NotRead,
    /// A read is in flight.
    Reading,
    /// Shown.
    Ready(ControlView),
    /// A refusal or a missing method hid the rows, with its reason.
    Unavailable(String),
}

/// The standing goal, loop and heartbeat of one session.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlView {
    /// The goal, if Hermes reports one.
    pub goal: Option<GoalView>,
    /// The loop, if Hermes reports one.
    pub loop_: Option<LoopView>,
    /// The heartbeat, if Hermes reports one.
    pub heartbeat: Option<HeartbeatView>,
    /// Hermes's revision of the three parts.
    pub revision: String,
}

/// A goal as the rows read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalView {
    /// The goal title.
    pub title: String,
    /// `active`, `paused` or `done`.
    pub status: String,
    /// Turns spent.
    pub turns_used: i64,
    /// The turn budget.
    pub max_turns: i64,
    /// How many subgoals stand.
    pub subgoals: usize,
    /// How many quality gates stand.
    pub gates: usize,
    /// Why it paused, if it did.
    pub paused_reason: Option<String>,
    /// What it waits on, if parked.
    pub wait: Option<WaitView>,
}

/// What a parked goal waits on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitView {
    /// A clock time, in epoch milliseconds.
    Until {
        /// The time, in epoch milliseconds.
        until_ms: u64,
        /// Why it waits.
        reason: String,
    },
    /// Another session.
    Session {
        /// The session it waits on.
        target: String,
        /// Why it waits.
        reason: String,
    },
    /// A process.
    Pid {
        /// The process it waits on.
        target: String,
        /// Why it waits.
        reason: String,
    },
}

/// A loop as the rows read it.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopView {
    /// The recurring prompt.
    pub prompt: String,
    /// `active`, `paused` or `done`.
    pub status: String,
    /// True for a self-paced loop.
    pub self_paced: bool,
    /// The cadence in seconds.
    pub interval_seconds: f64,
    /// The run cap, 0 for none.
    pub times: i64,
    /// Wakeups fired.
    pub ticks_fired: i64,
    /// The next wakeup, in epoch milliseconds.
    pub next_due_ms: u64,
    /// When it was created, in epoch milliseconds.
    pub created_ms: u64,
    /// True while a wakeup turn runs.
    pub awaiting_response: bool,
    /// True while an active goal holds it back.
    pub deferred_by_goal: bool,
    /// Why it paused, if it did.
    pub paused_reason: Option<String>,
}

/// A heartbeat as the rows read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeartbeatView {
    /// The recurring prompt.
    pub prompt: String,
    /// `active` or `paused`.
    pub status: String,
    /// The cadence in seconds.
    pub interval_seconds: i64,
    /// Pings sent.
    pub fire_count: i64,
    /// The next ping, in epoch milliseconds.
    pub next_due_ms: u64,
}

/// The automation state of one foreground session.
#[derive(Clone, Debug, Default)]
pub struct AutomationState {
    session_id: Option<String>,
    control: ControlStatus,
    next_token: u64,
    pending_control: Option<u64>,
    yolo: ModeValue<bool>,
    fast: ModeValue<FastMode>,
    pending_mode: [Option<PendingMode>; 2],
    plan_noted: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug)]
struct PendingMode {
    token: u64,
    after_command: bool,
}

#[allow(clippy::derivable_impls)]
impl<T> Default for ModeValue<T> {
    fn default() -> Self {
        ModeValue::NotRead
    }
}

impl AutomationState {
    /// Both mode values, for the rows and for sibling readers.
    pub fn modes(&self) -> (ModeValue<bool>, ModeValue<FastMode>) {
        (self.yolo, self.fast)
    }

    /// The shown view, when one is ready.
    pub(crate) fn view(&self) -> Option<&ControlView> {
        match &self.control {
            ControlStatus::Ready(view) => Some(view),
            _ => None,
        }
    }

    fn new_token(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token
    }

    /// What the rows read: whether a view is shown, its revision, and both modes.
    fn drawn(&self) -> Drawn {
        (
            self.view().map(|view| view.revision.clone()),
            self.yolo,
            self.fast,
        )
    }

    /// True when a goal, loop or heartbeat row shows. Row presence never depends on the time.
    fn control_rows_visible(&self) -> bool {
        self.view().is_some_and(ControlView::has_rows)
    }

    /// Show `view`, keeping the held one when its revision is equal.
    fn show(&mut self, view: ControlView) {
        let same = self
            .view()
            .is_some_and(|held| held.revision == view.revision);
        if !same {
            self.control = ControlStatus::Ready(view);
        }
    }

    fn pending(&self, key: ModeKey) -> Option<PendingMode> {
        self.pending_mode[mode_index(key)]
    }
}

type Drawn = (Option<String>, ModeValue<bool>, ModeValue<FastMode>);

fn mode_index(key: ModeKey) -> usize {
    match key {
        ModeKey::Yolo => 0,
        ModeKey::Fast => 1,
    }
}

fn shown(status: &str) -> bool {
    status == "active" || status == "paused"
}

impl ControlView {
    /// Decode through the generated snapshot type, then check the epoch times.
    pub fn decode(value: &serde_json::Value) -> Result<ControlView, DecodeError> {
        let snapshot: SessionControlSnapshot = serde_json::from_value(value.clone())
            .map_err(|error| DecodeError(error.to_string()))?;
        Self::from_snapshot(snapshot)
    }

    /// Convert a snapshot the client already decoded, checking the epoch times.
    pub fn from_snapshot(snapshot: SessionControlSnapshot) -> Result<ControlView, DecodeError> {
        Ok(ControlView {
            goal: snapshot.goal.map(goal_view).transpose()?,
            loop_: snapshot.loop_.map(loop_view).transpose()?,
            heartbeat: snapshot.heartbeat.map(heartbeat_view).transpose()?,
            revision: snapshot.revision,
        })
    }

    fn has_rows(&self) -> bool {
        self.goal.as_ref().is_some_and(|goal| shown(&goal.status))
            || self
                .loop_
                .as_ref()
                .is_some_and(|loop_| shown(&loop_.status))
            || self
                .heartbeat
                .as_ref()
                .is_some_and(|heartbeat| shown(&heartbeat.status))
    }
}

/// Epoch seconds as epoch milliseconds; a negative time is a decode error.
fn epoch_ms(seconds: f64, field: &str) -> Result<u64, DecodeError> {
    if seconds < 0.0 {
        return Err(DecodeError(format!("{field} is negative")));
    }
    // A float-to-integer `as` cast saturates, so a huge time clamps to u64::MAX.
    Ok((seconds * 1000.0).round() as u64)
}

fn goal_view(goal: GoalSnapshot) -> Result<GoalView, DecodeError> {
    if let Some(created_at) = goal.created_at {
        epoch_ms(created_at, "goal.created_at")?;
    }
    let wait = match goal.wait_barrier {
        None => None,
        Some(GoalSnapshotWaitBarrier::Until(until)) => Some(WaitView::Until {
            until_ms: epoch_ms(until.until_at, "goal.wait_barrier.until_at")?,
            reason: until.reason,
        }),
        Some(GoalSnapshotWaitBarrier::Target(target)) => {
            let (text, reason) = (target.target.to_string(), target.reason);
            Some(match target.type_ {
                WaitTargetType::Session => WaitView::Session {
                    target: text,
                    reason,
                },
                WaitTargetType::Pid => WaitView::Pid {
                    target: text,
                    reason,
                },
            })
        }
    };
    Ok(GoalView {
        title: goal.title,
        status: goal.status,
        turns_used: goal.turns_used,
        max_turns: goal.max_turns,
        subgoals: goal.subgoals.len(),
        gates: goal.gates.len(),
        paused_reason: goal.paused_reason,
        wait,
    })
}

fn loop_view(loop_: LoopSnapshot) -> Result<LoopView, DecodeError> {
    epoch_ms(loop_.last_fired_at, "loop.last_fired_at")?;
    Ok(LoopView {
        next_due_ms: epoch_ms(loop_.next_due_at, "loop.next_due_at")?,
        created_ms: epoch_ms(loop_.created_at, "loop.created_at")?,
        self_paced: loop_.mode == "self_paced",
        prompt: loop_.prompt,
        status: loop_.status,
        interval_seconds: loop_.interval_seconds,
        times: loop_.times,
        ticks_fired: loop_.ticks_fired,
        awaiting_response: loop_.awaiting_response,
        deferred_by_goal: loop_.deferred_by_goal,
        paused_reason: loop_.paused_reason,
    })
}

fn heartbeat_view(heartbeat: HeartbeatSnapshot) -> Result<HeartbeatView, DecodeError> {
    let created = epoch_ms(heartbeat.created_at, "heartbeat.created_at")?;
    let fired = epoch_ms(heartbeat.last_fired_at, "heartbeat.last_fired_at")?;
    let from = if fired > 0 { fired } else { created };
    let interval_ms = u64::try_from(heartbeat.interval_seconds)
        .unwrap_or(0)
        .saturating_mul(1000);
    Ok(HeartbeatView {
        prompt: heartbeat.prompt,
        status: heartbeat.status,
        interval_seconds: heartbeat.interval_seconds,
        fire_count: heartbeat.fire_count,
        next_due_ms: from.saturating_add(interval_ms),
    })
}

/// Fold one event into the state, answering the app's effects.
pub fn update(state: &mut AutomationState, event: AutomationEvent) -> Vec<AutomationEffect> {
    let before = state.drawn();
    let mut effects = Vec::new();
    match event {
        AutomationEvent::Attached { session_id } => {
            state.session_id = Some(session_id.clone());
            state.control = ControlStatus::Reading;
            state.yolo = ModeValue::NotRead;
            state.fast = ModeValue::NotRead;
            let control = state.new_token();
            state.pending_control = Some(control);
            effects.push(AutomationEffect::ReadControl {
                session_id: session_id.clone(),
                token: control,
            });
            for key in [ModeKey::Yolo, ModeKey::Fast] {
                let token = state.new_token();
                state.pending_mode[mode_index(key)] = Some(PendingMode {
                    token,
                    after_command: false,
                });
                effects.push(AutomationEffect::ReadMode {
                    session_id: session_id.clone(),
                    key,
                    token,
                });
            }
            effects.push(AutomationEffect::Redraw);
            return effects;
        }
        AutomationEvent::Detached => {
            let visible = !rows(state, 0).is_empty();
            state.session_id = None;
            state.control = ControlStatus::NotRead;
            state.pending_control = None;
            state.yolo = ModeValue::NotRead;
            state.fast = ModeValue::NotRead;
            state.pending_mode = [None, None];
            if visible {
                effects.push(AutomationEffect::Redraw);
            }
            return effects;
        }
        AutomationEvent::ControlRead { token, result } => {
            if state.pending_control != Some(token) {
                return effects;
            }
            state.pending_control = None;
            let visible = state.control_rows_visible();
            match result {
                Ok(view) => state.show(view),
                Err(ReadFailure::MethodMissing) => {
                    state.control = ControlStatus::Unavailable("unknown_method".to_owned());
                }
                Err(ReadFailure::Refused(reason)) => {
                    state.control = ControlStatus::Unavailable(reason);
                }
                Err(ReadFailure::TimedOut | ReadFailure::Lost | ReadFailure::BadResult) => {
                    state.control = ControlStatus::NotRead;
                }
            }
            if state.view().is_none() {
                if visible {
                    effects.push(AutomationEffect::Redraw);
                }
                return effects;
            }
        }
        AutomationEvent::ControlUpdated { session_id, raw } => {
            if state.session_id.as_deref() != Some(session_id.as_str()) {
                return effects;
            }
            let Ok(view) = ControlView::decode(&raw) else {
                return effects;
            };
            state.pending_control = None;
            state.show(view);
        }
        AutomationEvent::StatusNote {
            session_id,
            kind,
            text,
        } => {
            let foreground = state.session_id.as_deref() == Some(session_id.as_str());
            let automation = matches!(kind.as_str(), "goal" | "loop" | "heartbeat");
            if foreground && automation && !text.trim().is_empty() {
                effects.push(AutomationEffect::Notice { text });
            }
            return effects;
        }
        AutomationEvent::CommandFinished {
            session_id,
            canonical,
            result,
        } => {
            if state.session_id.as_deref() != Some(session_id.as_str()) {
                return effects;
            }
            let key = match canonical.as_str() {
                "yolo" => Some(ModeKey::Yolo),
                "fast" => Some(ModeKey::Fast),
                _ => None,
            };
            if let Some(key) = key {
                if result != CommandResult::NotRun {
                    let token = state.new_token();
                    state.pending_mode[mode_index(key)] = Some(PendingMode {
                        token,
                        after_command: true,
                    });
                    effects.push(AutomationEffect::ReadMode {
                        session_id,
                        key,
                        token,
                    });
                }
            } else if canonical == "plan"
                && result == CommandResult::Submit
                && state.plan_noted.insert(session_id)
            {
                effects.push(AutomationEffect::Notice {
                    text: PLAN_MODE_NOTE.to_owned(),
                });
            }
            return effects;
        }
        AutomationEvent::ModeRead {
            session_id,
            key,
            token,
            result,
        } => {
            if state.session_id.as_deref() != Some(session_id.as_str()) {
                return effects;
            }
            let Some(pending) = state.pending(key).filter(|pending| pending.token == token) else {
                return effects;
            };
            state.pending_mode[mode_index(key)] = None;
            let quiet = !pending.after_command
                && matches!(
                    result,
                    Err(ReadFailure::MethodMissing | ReadFailure::Refused(_))
                );
            if quiet {
                return effects;
            }
            let value = result.ok().flatten();
            match key {
                ModeKey::Yolo => {
                    state.yolo = match value.as_deref() {
                        Some("1") => ModeValue::Known(true),
                        Some("0") => ModeValue::Known(false),
                        _ => ModeValue::Unknown,
                    };
                }
                ModeKey::Fast => {
                    state.fast = match value.as_deref() {
                        Some("normal") => ModeValue::Known(FastMode::Normal),
                        Some("fast") => ModeValue::Known(FastMode::Fast),
                        Some("ultrafast") => ModeValue::Known(FastMode::Ultrafast),
                        _ => ModeValue::Unknown,
                    };
                }
            }
        }
    }
    if state.drawn() != before {
        effects.push(AutomationEffect::Redraw);
    }
    effects
}
