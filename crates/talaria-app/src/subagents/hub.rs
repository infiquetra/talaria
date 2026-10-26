//! The subagent hub and its messages. Issue #242.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::events::Notification;
use hermes_contract::schemas::{DelegationStatusResult, SubagentListResult, SubagentTailResult};
use serde_json::{Value, json};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{
    Caps, Clock, FinishedBatch, RowState, Spawning, SubagentCounts, SubagentEvent, SubagentTree,
};
use tokio::task::JoinHandle;

use super::AppSender;

/// Settings from `[subagents]`: list poll seconds, tail poll milliseconds,
/// dock rows. Defaults 5, 2000, 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubagentsConfig {
    pub poll_s: u64,
    pub tail_poll_ms: u64,
    pub dock_rows: u16,
}

impl Default for SubagentsConfig {
    fn default() -> Self {
        Self {
            poll_s: 5,
            tail_poll_ms: 2000,
            dock_rows: 3,
        }
    }
}

/// One control the panel or `/agents` asks the hub to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubagentAction {
    Steer { id: String, text: String },
    Stop(String),
    StopAll,
    ConfirmStopAll,
    RequestPause,
    ConfirmPause(bool),
    Resume,
    OpenTail(String),
    CloseTail,
    Refresh,
}

/// What one control came back with. `goal` is the row's goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    SteerQueued {
        goal: String,
    },
    SteerRejected {
        goal: String,
    },
    StopRequested {
        goal: String,
    },
    AlreadyFinished {
        goal: String,
    },
    NotStarted {
        goal: String,
    },
    AlreadyStopping {
        goal: String,
    },
    StopAllSent {
        sent: u32,
        of: u32,
    },
    Paused(bool),
    Refused {
        method: &'static str,
        reason: String,
    },
    MethodMissing {
        method: &'static str,
    },
    Unknown {
        action: &'static str,
        secs: u64,
    },
    Lost {
        action: &'static str,
    },
    NotSent {
        action: &'static str,
    },
}

/// The hub's own messages, carried by `AppMessage::Subagents`.
#[derive(Clone, Debug)]
pub enum SubagentMsg {
    Listed {
        key: RowKey,
        generation: u64,
        result: Result<SubagentListResult, CallError>,
    },
    Tail {
        key: RowKey,
        generation: u64,
        view: u64,
        id: String,
        result: Result<SubagentTailResult, CallError>,
    },
    Control {
        key: RowKey,
        generation: u64,
        id: Option<String>,
        outcome: ControlOutcome,
    },
    DelegationStatus {
        key: RowKey,
        generation: u64,
        result: Result<DelegationStatusResult, CallError>,
    },
    PollTick {
        key: RowKey,
    },
    TailTick {
        key: RowKey,
        id: String,
    },
    ElapsedTick,
    /// Timer tagged with its attachment and connection generation.
    Scheduled {
        key: RowKey,
        generation: u64,
        kind: TimerKind,
    },
}

#[derive(Clone, Debug)]
pub enum TimerKind {
    Poll,
    Tail { id: String, view: u64 },
    Status,
    Elapsed,
}

/// Transcript data; terminal escaping is the presentation layer's job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubagentTailView {
    pub text: Option<String>,
    pub truncated: bool,
    pub ever_failed: bool,
}

struct Tracker {
    tree: SubagentTree,
    client: Client,
    generation: u64,
    failures: u8,
    list_disabled: bool,
    missing_id_noticed: bool,
    list_inflight: bool,
    poll_scheduled: bool,
    tail_id: Option<String>,
    tail_view: u64,
    tail_inflight: bool,
    tail: SubagentTailView,
    stopping: HashSet<String>,
    armed: Option<u64>,
    jobs: Vec<JoinHandle<()>>,
}

impl Drop for Tracker {
    fn drop(&mut self) {
        for job in &self.jobs {
            job.abort();
        }
    }
}

/// One tracker per attached session: tree, client, poll and tail state.
pub struct SubagentHub {
    cfg: SubagentsConfig,
    tx: AppSender,
    trackers: HashMap<RowKey, Tracker>,
    next_generation: u64,
    foreground: Option<RowKey>,
    panel: Option<RowKey>,
    spawning: Spawning,
    active: Option<usize>,
    pause_known: Option<bool>,
    confirm_pause: bool,
    flash: Option<String>,
    batches: Vec<FinishedBatch>,
    status_inflight: bool,
    status_disabled: bool,
    status_scheduled: bool,
}

impl SubagentHub {
    /// Select the inline view. Other attached owners keep their trackers.
    pub fn set_foreground(&mut self, key: Option<RowKey>) {
        if self.foreground == key {
            return;
        }
        if let Some(old) = self.foreground.take() {
            self.detach(&old);
        }
        self.foreground = key;
        self.batches.clear();
    }

    pub fn foreground_key(&self) -> Option<&RowKey> {
        self.foreground.as_ref()
    }
    pub fn panel_key(&self) -> Option<&RowKey> {
        self.panel.as_ref()
    }
    pub fn cfg(&self) -> SubagentsConfig {
        self.cfg
    }
    pub fn flash(&self) -> Option<&str> {
        self.flash.as_deref()
    }
    pub fn clear_flash(&mut self) {
        self.flash = None;
    }
    pub fn pause_confirmation_pending(&self) -> bool {
        self.confirm_pause
    }
    pub fn show_paused(&mut self) {
        self.set_flash(outcome_text(&ControlOutcome::Paused(true)));
    }
    pub fn gateway_active(&self) -> Option<u32> {
        self.active.map(|n| n as u32)
    }
    pub fn tail(&self, key: &RowKey) -> Option<&SubagentTailView> {
        self.trackers
            .get(key)
            .filter(|t| t.tail_id.is_some())
            .map(|t| &t.tail)
    }
    /// Once listing is known, replace the gateway-wide usage count.
    pub fn known_counts(&self, key: &RowKey) -> Option<SubagentCounts> {
        self.trackers
            .get(key)
            .filter(|t| t.tree.hydrated() || t.list_disabled)
            .map(|t| t.tree.counts())
    }
    pub fn take_finished_batches(&mut self) -> Vec<FinishedBatch> {
        std::mem::take(&mut self.batches)
    }
    pub fn attached_keys(&self) -> Vec<RowKey> {
        let mut keys: Vec<_> = self.trackers.keys().cloned().collect();
        keys.sort_by(|a, b| {
            (a.profile.as_str(), a.session_id.as_str())
                .cmp(&(b.profile.as_str(), b.session_id.as_str()))
        });
        if let Some(foreground) = &self.foreground {
            keys.sort_by_key(|k| k != foreground);
        }
        keys
    }
    /// Tab only visits attached sessions that contain rows.
    pub fn next_panel_session(&mut self) {
        let keys: Vec<_> = self
            .attached_keys()
            .into_iter()
            .filter(|k| self.tree(k).is_some_and(|t| !t.rows_in_order().is_empty()))
            .collect();
        if keys.len() < 2 {
            return;
        }
        let index = keys
            .iter()
            .position(|k| Some(k) == self.panel.as_ref())
            .unwrap_or(0);
        self.open_panel(keys[(index + 1) % keys.len()].clone());
    }
    fn set_flash(&mut self, text: String) -> bool {
        let changed = self.flash.as_ref() != Some(&text);
        self.flash = Some(text);
        changed
    }
    fn current(&self, key: &RowKey, generation: u64) -> bool {
        self.trackers
            .get(key)
            .is_some_and(|t| t.generation == generation)
    }
    fn collect_batches(&mut self, key: &RowKey) {
        if let Some(tracker) = self.trackers.get_mut(key) {
            let batches = tracker.tree.take_finished_batches();
            if self.foreground.as_ref() == Some(key) {
                self.batches.extend(batches);
            }
        }
    }
    fn schedule(&mut self, key: &RowKey, kind: TimerKind, delay: Duration) {
        let Some(tracker) = self.trackers.get_mut(key) else {
            return;
        };
        let generation = tracker.generation;
        let key = key.clone();
        let tx = self.tx.clone();
        tracker.jobs.retain(|job| !job.is_finished());
        tracker.jobs.push(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx.send(crate::app::AppMessage::Subagents(SubagentMsg::Scheduled {
                key,
                generation,
                kind,
            }));
        }));
    }
    fn schedule_poll(&mut self, key: &RowKey) {
        let Some(tracker) = self.trackers.get_mut(key) else {
            return;
        };
        if tracker.list_disabled
            || tracker.failures >= 3
            || tracker.list_inflight
            || tracker.poll_scheduled
            || (tracker.tree.counts().live == 0 && self.panel.as_ref() != Some(key))
        {
            return;
        }
        tracker.poll_scheduled = true;
        self.schedule(key, TimerKind::Poll, Duration::from_secs(self.cfg.poll_s));
    }
    fn poll(&mut self, key: &RowKey, now: Clock) {
        if let Some(tracker) = self.trackers.get_mut(key) {
            tracker.tree.expire(now);
            if tracker.failures >= 3
                || (tracker.tree.counts().live == 0 && self.panel.as_ref() != Some(key))
            {
                return;
            }
        }
        self.collect_batches(key);
        self.list(key);
    }
    fn list(&mut self, key: &RowKey) {
        let Some(tracker) = self.trackers.get_mut(key) else {
            return;
        };
        if tracker.list_inflight || tracker.list_disabled {
            return;
        }
        tracker.list_inflight = true;
        let generation = tracker.generation;
        let client = tracker.client.clone();
        let key = key.clone();
        let tx = self.tx.clone();
        tracker.jobs.push(tokio::spawn(async move {
            let result = call(
                &client,
                "subagent.list",
                json!({"session_id":key.session_id}),
                5,
            )
            .await
            .and_then(|value| {
                if !value.get("subagents").is_some_and(Value::is_array) {
                    return Err(bad_result("subagent.list"));
                }
                decode("subagent.list", value)
            });
            let _ = tx.send(crate::app::AppMessage::Subagents(SubagentMsg::Listed {
                key,
                generation,
                result,
            }));
        }));
    }
    fn fetch_tail(&mut self, key: &RowKey, id: String) {
        let Some(tracker) = self.trackers.get_mut(key) else {
            return;
        };
        if tracker.tail_inflight || tracker.tail_id.as_deref() != Some(&id) {
            return;
        }
        tracker.tail_inflight = true;
        let generation = tracker.generation;
        let view = tracker.tail_view;
        let client = tracker.client.clone();
        let key = key.clone();
        let tx = self.tx.clone();
        tracker.jobs.push(tokio::spawn(async move {
            let result = call(
                &client,
                "subagent.tail",
                json!({"session_id":key.session_id,"subagent_id":id}),
                5,
            )
            .await
            .and_then(|v| decode("subagent.tail", v));
            let _ = tx.send(crate::app::AppMessage::Subagents(SubagentMsg::Tail {
                key,
                generation,
                view,
                id,
                result,
            }));
        }));
    }
    fn schedule_status(&mut self, key: &RowKey) {
        if self.panel.as_ref() == Some(key) && !self.status_disabled && !self.status_scheduled {
            self.status_scheduled = true;
            self.schedule(key, TimerKind::Status, Duration::from_secs(5));
        }
    }
    fn status(&mut self, key: &RowKey) {
        if self.status_inflight || self.status_disabled {
            return;
        }
        let Some(tracker) = self.trackers.get_mut(key) else {
            return;
        };
        self.status_inflight = true;
        let generation = tracker.generation;
        let client = tracker.client.clone();
        let key = key.clone();
        let tx = self.tx.clone();
        tracker.jobs.push(tokio::spawn(async move {
            let result = call(&client, "delegation.status", json!({}), 10)
                .await
                .and_then(|v| decode("delegation.status", v));
            let _ = tx.send(crate::app::AppMessage::Subagents(
                SubagentMsg::DelegationStatus {
                    key,
                    generation,
                    result,
                },
            ));
        }));
    }
    fn goal(&self, key: &RowKey, id: &str) -> String {
        self.tree(key)
            .and_then(|tree| tree.get(id))
            .map_or_else(|| id.to_owned(), |row| row.goal.clone())
    }
    fn emit(&self, key: &RowKey, id: Option<String>, outcome: ControlOutcome) {
        if let Some(tracker) = self.trackers.get(key) {
            let _ = self
                .tx
                .send(crate::app::AppMessage::Subagents(SubagentMsg::Control {
                    key: key.clone(),
                    generation: tracker.generation,
                    id,
                    outcome,
                }));
        }
    }
    fn control<F>(
        &mut self,
        key: &RowKey,
        id: Option<String>,
        method: &'static str,
        action: &'static str,
        params: Value,
        interpret: F,
    ) where
        F: FnOnce(Value) -> Result<ControlOutcome, CallError> + Send + 'static,
    {
        let Some(tracker) = self.trackers.get_mut(key) else {
            self.set_flash(outcome_text(&ControlOutcome::NotSent { action }));
            return;
        };
        let client = tracker.client.clone();
        let generation = tracker.generation;
        let key = key.clone();
        let tx = self.tx.clone();
        tracker.jobs.push(tokio::spawn(async move {
            let outcome = match call(&client, method, params, 10).await.and_then(interpret) {
                Ok(outcome) => outcome,
                Err(error) => error_outcome(method, action, error),
            };
            let _ = tx.send(crate::app::AppMessage::Subagents(SubagentMsg::Control {
                key,
                generation,
                id,
                outcome,
            }));
        }));
    }
    fn stop(&mut self, key: &RowKey, id: String) {
        let goal = self.goal(key, &id);
        let outcome = self.tree(key).and_then(|t| t.get(&id)).and_then(|row| {
            if row.state == RowState::Queued {
                Some(ControlOutcome::NotStarted { goal: goal.clone() })
            } else if row.state.is_terminal() {
                Some(ControlOutcome::AlreadyFinished { goal: goal.clone() })
            } else if row.stop_requested
                || self
                    .trackers
                    .get(key)
                    .is_some_and(|t| t.stopping.contains(&id))
            {
                Some(ControlOutcome::AlreadyStopping { goal: goal.clone() })
            } else {
                None
            }
        });
        if let Some(outcome) = outcome {
            self.emit(key, None, outcome);
            return;
        }
        if let Some(tracker) = self.trackers.get_mut(key) {
            tracker.stopping.insert(id.clone());
        }
        self.control(
            key,
            Some(id.clone()),
            "subagent.interrupt",
            "stop",
            json!({"session_id":key.session_id,"subagent_id":id}),
            move |value| {
                let result: hermes_contract::schemas::SubagentInterruptResult =
                    decode("subagent.interrupt", value)?;
                Ok(if result.found {
                    ControlOutcome::StopRequested { goal }
                } else {
                    ControlOutcome::AlreadyFinished { goal }
                })
            },
        );
    }
    fn pause(&mut self, key: &RowKey, paused: bool) {
        self.confirm_pause = false;
        self.control(
            key,
            None,
            "delegation.pause",
            if paused { "pause" } else { "resume" },
            json!({"paused":paused}),
            |value| {
                let result: hermes_contract::schemas::DelegationPauseResult =
                    decode("delegation.pause", value)?;
                Ok(ControlOutcome::Paused(result.paused))
            },
        );
    }
    #[must_use]
    pub fn new(cfg: SubagentsConfig, tx: AppSender) -> SubagentHub {
        Self {
            cfg,
            tx,
            trackers: HashMap::new(),
            next_generation: 0,
            foreground: None,
            panel: None,
            spawning: Spawning::Unknown,
            active: None,
            pause_known: None,
            confirm_pause: false,
            flash: None,
            batches: Vec::new(),
            status_inflight: false,
            status_disabled: false,
            status_scheduled: false,
        }
    }

    pub fn attach(&mut self, key: RowKey, client: Client) {
        if self.trackers.contains_key(&key) {
            return;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        self.trackers.insert(
            key.clone(),
            Tracker {
                tree: SubagentTree::new(),
                client,
                generation: self.next_generation,
                failures: 0,
                list_disabled: false,
                missing_id_noticed: false,
                list_inflight: false,
                poll_scheduled: false,
                tail_id: None,
                tail_view: 0,
                tail_inflight: false,
                tail: SubagentTailView::default(),
                stopping: HashSet::new(),
                armed: None,
                jobs: Vec::new(),
            },
        );
        self.list(&key);
    }

    pub fn detach(&mut self, key: &RowKey) {
        self.trackers.remove(key);
        if self.panel.as_ref() == Some(key) {
            self.close_panel();
        }
    }

    /// Feed one notification of an attached session. True when visible state
    /// changed. The hub also takes `message.start` here, to prune.
    pub fn on_notification(&mut self, key: &RowKey, n: &Notification, now: Clock) -> bool {
        let Some(tracker) = self.trackers.get_mut(key) else {
            return false;
        };
        let before = tracker.tree.version();
        if matches!(n, Notification::MessageStart(_)) {
            tracker.tree.prune_finished();
        } else if let Some(event) = SubagentEvent::from_notification(n) {
            let applied = tracker.tree.apply(&event, now);
            if applied.missing_id && !tracker.missing_id_noticed {
                tracker.missing_id_noticed = true;
                let _ = self.tx.send(crate::app::AppMessage::User(
                    crate::app::UserAction::Notice(
                        "Hermes sent a subagent event without an id; it was ignored.".into(),
                    ),
                ));
            }
            tracker.tree.expire(now);
        } else {
            return false;
        }
        let changed = before != tracker.tree.version();
        self.collect_batches(key);
        self.schedule_poll(key);
        changed
    }

    /// A reconnect or a resolved replay gap: list again, reset failures.
    pub fn on_reconnected(&mut self, key: &RowKey) {
        self.next_generation = self.next_generation.wrapping_add(1);
        if let Some(tracker) = self.trackers.get_mut(key) {
            for job in tracker.jobs.drain(..) {
                job.abort();
            }
            tracker.generation = self.next_generation;
            tracker.failures = 0;
            tracker.list_disabled = false;
            tracker.missing_id_noticed = false;
            tracker.list_inflight = false;
            tracker.poll_scheduled = false;
            tracker.tail_inflight = false;
            tracker.stopping.clear();
        }
        self.status_inflight = false;
        self.status_disabled = false;
        self.status_scheduled = false;
        self.list(key);
        if self.panel.as_ref() == Some(key) {
            self.status(key);
        }
        if let Some(id) = self.trackers.get(key).and_then(|t| t.tail_id.clone()) {
            self.fetch_tail(key, id);
        }
    }

    /// Feed one of the hub's own messages. True when visible state changed.
    pub fn on_message(&mut self, msg: SubagentMsg, now: Clock) -> bool {
        match msg {
            SubagentMsg::Listed {
                key,
                generation,
                result,
            } => {
                if !self.current(&key, generation) {
                    return false;
                }
                let Some(tracker) = self.trackers.get_mut(&key) else {
                    return false;
                };
                tracker.list_inflight = false;
                let before = tracker.tree.version();
                match result {
                    Ok(list) => {
                        tracker.failures = 0;
                        tracker.tree.reconcile(&list, now);
                    }
                    Err(CallError::Rpc(error)) if error.code == -32601 => {
                        if !tracker.list_disabled {
                            let _ = self.tx.send(crate::app::AppMessage::User(
                                crate::app::UserAction::Notice(
                                    "This Hermes does not list subagents; showing events only."
                                        .into(),
                                ),
                            ));
                        }
                        tracker.list_disabled = true;
                    }
                    Err(error) => {
                        tracker.failures = tracker.failures.saturating_add(1);
                        self.flash =
                            Some(outcome_text(&error_outcome("subagent.list", "list", error)));
                    }
                }
                let changed = before != tracker.tree.version();
                self.collect_batches(&key);
                self.schedule_poll(&key);
                changed
            }
            SubagentMsg::Tail {
                key,
                generation,
                view,
                id,
                result,
            } => {
                if !self.current(&key, generation) {
                    return false;
                }
                let Some(tracker) = self.trackers.get_mut(&key) else {
                    return false;
                };
                if tracker.tail_view != view || tracker.tail_id.as_deref() != Some(&id) {
                    return false;
                }
                tracker.tail_inflight = false;
                let before = tracker.tail.clone();
                match result {
                    Ok(result) if result.subagent_id == id => {
                        tracker.tail.truncated = result.truncated;
                        let text = if result.truncated {
                            result
                                .text
                                .split_once('\n')
                                .map_or("", |(_, rest)| rest)
                                .to_owned()
                        } else {
                            result.text
                        };
                        if result.available {
                            tracker.tail.text = Some(text);
                        }
                    }
                    _ => tracker.tail.ever_failed = true,
                }
                let changed = before != tracker.tail;
                self.schedule(
                    &key,
                    TimerKind::Tail { id, view },
                    Duration::from_millis(self.cfg.tail_poll_ms),
                );
                changed
            }
            SubagentMsg::Control {
                key,
                generation,
                id,
                outcome,
            } => {
                if !self.current(&key, generation) {
                    return false;
                }
                if let Some(id) = id {
                    let Some(tracker) = self.trackers.get_mut(&key) else {
                        return false;
                    };
                    tracker.stopping.remove(&id);
                    if matches!(outcome, ControlOutcome::StopRequested { .. }) {
                        tracker.tree.mark_stop_requested(&id);
                    }
                }
                if let ControlOutcome::Paused(paused) = outcome {
                    self.pause_known = Some(paused);
                    self.spawning = if paused {
                        Spawning::Paused(None)
                    } else {
                        Spawning::On(None)
                    };
                    self.confirm_pause = false;
                    self.status(&key);
                }
                self.set_flash(outcome_text(&outcome))
            }
            SubagentMsg::DelegationStatus {
                key,
                generation,
                result,
            } => {
                if !self.current(&key, generation) {
                    return false;
                }
                self.status_inflight = false;
                let before = self.spawning;
                match result {
                    Ok(result) => {
                        self.active = Some(result.active.len());
                        let caps = Some(Caps {
                            max_spawn_depth: result.max_spawn_depth,
                            max_concurrent_children: result.max_concurrent_children,
                        });
                        self.spawning = if result.paused {
                            Spawning::Paused(caps)
                        } else {
                            Spawning::On(caps)
                        };
                    }
                    Err(CallError::Rpc(error)) if error.code == -32601 => {
                        self.active = None;
                        self.status_disabled = true;
                        self.spawning = match self.pause_known {
                            Some(true) => Spawning::Paused(None),
                            Some(false) => Spawning::On(None),
                            None => Spawning::Unknown,
                        };
                    }
                    Err(_) => {}
                }
                self.schedule_status(&key);
                before != self.spawning
            }
            SubagentMsg::PollTick { key } => {
                self.poll(&key, now);
                false
            }
            SubagentMsg::TailTick { key, id } => {
                self.fetch_tail(&key, id);
                false
            }
            SubagentMsg::ElapsedTick => self
                .panel
                .as_ref()
                .and_then(|k| self.tree(k))
                .is_some_and(|t| t.counts().live > 0),
            SubagentMsg::Scheduled {
                key,
                generation,
                kind,
            } => {
                if !self.current(&key, generation) {
                    return false;
                }
                match kind {
                    TimerKind::Poll => {
                        self.trackers
                            .get_mut(&key)
                            .map(|t| t.poll_scheduled = false);
                        self.poll(&key, now);
                        false
                    }
                    TimerKind::Tail { id, view } => {
                        if self.trackers[&key].tail_view == view {
                            self.fetch_tail(&key, id);
                        }
                        false
                    }
                    TimerKind::Status => {
                        self.status_scheduled = false;
                        if self.panel.as_ref() == Some(&key) {
                            self.status(&key);
                        }
                        false
                    }
                    TimerKind::Elapsed => {
                        let live = self.panel.as_ref() == Some(&key)
                            && self.trackers[&key].tree.counts().live > 0;
                        if self.panel.as_ref() == Some(&key) {
                            self.schedule(&key, TimerKind::Elapsed, Duration::from_secs(1));
                        }
                        live
                    }
                }
            }
        }
    }

    /// Run one panel or `/agents` control.
    pub fn act(&mut self, key: &RowKey, action: SubagentAction, now: Clock) {
        match action {
            SubagentAction::Steer { id, text } => {
                let text: String = text.trim().chars().take(4000).collect();
                if text.is_empty() {
                    self.set_flash("Type an instruction first.".into());
                    return;
                }
                let goal = self.goal(key, &id);
                if self
                    .tree(key)
                    .and_then(|tree| tree.get(&id))
                    .is_some_and(|r| r.accepting_steer == Some(false))
                {
                    self.emit(key, None, ControlOutcome::SteerRejected { goal });
                    return;
                }
                self.control(
                    key,
                    None,
                    "subagent.steer",
                    "steer",
                    json!({"session_id":key.session_id,"subagent_id":id,"text":text}),
                    move |value| {
                        let result: hermes_contract::schemas::SubagentSteerResult =
                            decode("subagent.steer", value)?;
                        Ok(if result.status.to_string() == "queued" {
                            ControlOutcome::SteerQueued { goal }
                        } else {
                            ControlOutcome::SteerRejected { goal }
                        })
                    },
                );
            }
            SubagentAction::Stop(id) => self.stop(key, id),
            SubagentAction::StopAll | SubagentAction::ConfirmStopAll => {
                let Some(tracker) = self.trackers.get_mut(key) else {
                    return;
                };
                let ids: Vec<_> = tracker
                    .tree
                    .live_ids()
                    .into_iter()
                    .filter(|id| {
                        tracker
                            .tree
                            .get(id)
                            .is_some_and(|r| r.state == RowState::Running && !r.stop_requested)
                            && !tracker.stopping.contains(id)
                    })
                    .collect();
                if ids.is_empty() {
                    tracker.armed = None;
                    self.set_flash("No running subagents to stop.".into());
                    return;
                }
                if tracker
                    .armed
                    .is_some_and(|armed| now.mono_ms.saturating_sub(armed) <= 3000)
                {
                    tracker.armed = None;
                    let count = ids.len() as u32;
                    for id in ids {
                        self.stop(key, id);
                    }
                    self.emit(
                        key,
                        None,
                        ControlOutcome::StopAllSent {
                            sent: count,
                            of: count,
                        },
                    );
                } else {
                    tracker.armed = Some(now.mono_ms);
                    self.set_flash(format!("Press X again within 3 s to stop all {} running subagents of this session.", ids.len()));
                }
            }
            SubagentAction::RequestPause => {
                if matches!(self.spawning, Spawning::Paused(_)) {
                    self.act(key, SubagentAction::Resume, now);
                } else {
                    self.confirm_pause = true;
                    let middle = self.active.map_or_else(
                        || "Running ones will continue.".into(),
                        |n| format!("{n} running now will continue."),
                    );
                    self.set_flash(format!(
                        "Pause new subagents for every session on this Hermes gateway? {middle} y/n"
                    ));
                }
            }
            SubagentAction::ConfirmPause(false) => {
                self.confirm_pause = false;
                self.clear_flash();
            }
            SubagentAction::ConfirmPause(true) => self.pause(key, true),
            SubagentAction::Resume => self.pause(key, false),
            SubagentAction::OpenTail(id) => {
                if let Some(tracker) = self.trackers.get_mut(key) {
                    tracker.tail_view = tracker.tail_view.wrapping_add(1);
                    tracker.tail_inflight = false;
                    tracker.tail_id = Some(id.clone());
                    tracker.tail = SubagentTailView::default();
                }
                self.fetch_tail(key, id);
            }
            SubagentAction::CloseTail => {
                if let Some(tracker) = self.trackers.get_mut(key) {
                    tracker.tail_id = None;
                    tracker.tail_view = tracker.tail_view.wrapping_add(1);
                    tracker.tail_inflight = false;
                }
            }
            SubagentAction::Refresh => {
                if let Some(tracker) = self.trackers.get_mut(key) {
                    tracker.failures = 0;
                }
                self.list(key);
            }
        }
    }

    #[must_use]
    pub fn tree(&self, key: &RowKey) -> Option<&SubagentTree> {
        self.trackers.get(key).map(|t| &t.tree)
    }

    #[must_use]
    pub fn counts(&self, key: &RowKey) -> Option<SubagentCounts> {
        self.tree(key).map(SubagentTree::counts)
    }

    pub fn open_panel(&mut self, key: RowKey) {
        self.close_panel();
        self.panel = Some(key.clone());
        self.status_disabled = false;
        if let Some(tracker) = self.trackers.get_mut(&key) {
            tracker.failures = 0;
        }
        self.list(&key);
        self.status(&key);
        self.schedule(&key, TimerKind::Elapsed, Duration::from_secs(1));
    }

    pub fn close_panel(&mut self) {
        if let Some(key) = self.panel.take() {
            self.act(
                &key,
                SubagentAction::CloseTail,
                Clock {
                    mono_ms: 0,
                    wall_s: 0.0,
                },
            );
        }
        self.confirm_pause = false;
        self.clear_flash();
    }

    #[must_use]
    pub fn spawning(&self) -> Spawning {
        self.spawning
    }
}

/// The exact flash and notice text for one outcome.
#[must_use]
pub fn outcome_text(o: &ControlOutcome) -> String {
    let goal = |s: &str| talaria_core::attention::cut(&literal(s), 40);
    match o {
        ControlOutcome::SteerQueued { goal: g } => format!(
            "Steer queued for \"{}\". It reaches the subagent at its next step.",
            goal(g)
        ),
        ControlOutcome::SteerRejected { goal: g } => {
            format!("\"{}\" is no longer accepting steering.", goal(g))
        }
        ControlOutcome::StopRequested { goal: g } => format!("Stop requested for \"{}\".", goal(g)),
        ControlOutcome::AlreadyFinished { goal: g } => {
            format!("\"{}\" had already finished.", goal(g))
        }
        ControlOutcome::NotStarted { goal: g } => format!(
            "\"{}\" has not started yet; stop it once it is running.",
            goal(g)
        ),
        ControlOutcome::AlreadyStopping { goal: g } => {
            format!("Stop already requested for \"{}\".", goal(g))
        }
        ControlOutcome::StopAllSent { sent, of } => {
            format!("Stop requested for {sent} of {of} subagents.")
        }
        ControlOutcome::Paused(true) => {
            "New subagents are paused for every session on this gateway.".into()
        }
        ControlOutcome::Paused(false) => "New subagents are allowed again.".into(),
        ControlOutcome::Refused { method, reason } => {
            let reason = match reason.as_str() {
                "invalid_params" => {
                    "Talaria sent a request Hermes did not accept (please report this)"
                }
                "not_found" => "the session is not live on the gateway",
                "permission_denied" if *method == "delegation.pause" => {
                    "this connection may not change the gateway's delegation setting"
                }
                "permission_denied" => "this connection may not control the session",
                "profile_mismatch" => "the session belongs to another profile",
                "stale_generation" => "the session moved on; reopen it",
                "unsupported_projection" => {
                    "the session runs in a separate worker that Talaria cannot reach"
                }
                "storage_unavailable" => "Hermes storage is busy; try again",
                other => other,
            };
            format!("Hermes refused {method}: {}.", literal(reason))
        }
        ControlOutcome::MethodMissing { method } => format!("This Hermes does not offer {method}."),
        ControlOutcome::Unknown { action, secs } => {
            format!("No answer from Hermes within {secs} s; the outcome of {action} is unknown.")
        }
        ControlOutcome::Lost { action } => {
            format!("The connection to Hermes dropped; the outcome of {action} is unknown.")
        }
        ControlOutcome::NotSent { action } => {
            format!("Not connected to the gateway; {action} was not sent.")
        }
    }
}

fn literal(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '\u{1b}' => out.push('␛'),
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}'..='\u{200f}' => {
                out.push_str(&format!("‹U+{:04X}›", c as u32))
            }
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn bad_result(method: &str) -> CallError {
    CallError::BadResult {
        method: method.into(),
        reason: "result did not decode",
    }
}
fn decode<T: serde::de::DeserializeOwned>(method: &str, value: Value) -> Result<T, CallError> {
    serde_json::from_value(value).map_err(|_| bad_result(method))
}
async fn call(client: &Client, method: &str, params: Value, secs: u64) -> Result<Value, CallError> {
    client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(secs)),
            },
        )
        .await
}
fn error_outcome(method: &'static str, action: &'static str, error: CallError) -> ControlOutcome {
    match error {
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
            ControlOutcome::NotSent { action }
        }
        CallError::TimedOut { after, .. } => ControlOutcome::Unknown {
            action,
            secs: after.as_secs(),
        },
        CallError::Unknown { .. } | CallError::BadResult { .. } => ControlOutcome::Lost { action },
        CallError::Rpc(error) if error.code == -32601 => ControlOutcome::MethodMissing { method },
        CallError::Rpc(error) => ControlOutcome::Refused {
            method,
            reason: if error.code == 5001 {
                "storage_unavailable".into()
            } else {
                error.reason.unwrap_or_else(|| "unknown refusal".into())
            },
        },
    }
}
