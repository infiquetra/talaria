//! Deterministic view state. Clocks, file observations and call results are inputs.
use super::{
    AgentState, ControlId, ControlOutcome, ControlRequest, PendingControls, RunProgress, RunState,
    hold_generation, live_tokens,
};
use crate::journal::RecordLine;
use hermes_contract::events::Notification;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use talaria_core::subagents::{Clock, SubagentEvent};

pub struct SchedulerWait {
    pub key: String,
    pub reason: WaitView,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitView {
    GlobalCap,
    ProviderCap,
    ProviderLimited { until: Option<String> },
    ModelLimited { until: Option<String> },
    ProviderPaused { until: Option<String> },
}
pub enum SchedulerWaitEvent {
    Queued(SchedulerWait),
    Admitted { key: String },
    Released { key: String },
    Skipped { key: String },
}
#[derive(Clone, Debug, PartialEq)]
pub struct SchedulerSnapshot {
    pub spend_tokens: u64,
    pub spend_money: Option<f64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Working,
    NeedsYou,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeKey {
    Run,
    Child(Vec<String>),
    Phase { path: Vec<String>, title: String },
    Agent(String),
    Subagent { key: String, id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleNode {
    pub key: NodeKey,
    pub depth: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgressKey {
    Up,
    Down,
    Left,
    Right,
    OpenAgent,
    Steer,
    StopAgent,
    RestartAgent,
    PauseRun,
    StopRun,
    CycleFilter,
    CloseView,
    Enter,
    Char(char),
    Backspace,
    Confirm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayAction {
    Steer,
    Interrupt,
    Cancel,
    SubSteer,
    SubInterrupt,
    Restart,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayFailure {
    pub reason: String,
    pub stale_generation: bool,
    pub disconnected: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingStage {
    Ready,
    Refreshing,
    Calling,
    Settling,
}
#[derive(Clone, Debug)]
struct PendingAgent {
    action: GatewayAction,
    text: Option<String>,
    subagent: Option<String>,
    at_ms: i64,
    refreshed: bool,
    stage: PendingStage,
    attempt: u32,
    receipt_at_ms: i64,
    receipt_in_flight: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Confirmation {
    Restart(String),
    StopRun,
}

pub struct ProgressState {
    pub progress: RunProgress,
    pub selection: NodeKey,
    pub collapsed: HashSet<NodeKey>,
    pub filter: Filter,
    pub connected: bool,
    pub now_ms: i64,
    pub opened_at_ms: i64,
    pub last_journal_seen_ms: i64,
    pub footer: Option<String>,
    pub error: Option<String>,
    pub steer_input: Option<String>,
    pub closed: bool,
    pub revision: u64,
    pub pending_controls: PendingControls,
    pub run_suffix: Option<String>,
    pub no_live_runner: bool,
    pub scheduler: Option<SchedulerSnapshot>,
    pub waits: HashMap<String, WaitView>,
    pub opened_session: Option<String>,
    pub prompt_first_line: Option<String>,
    pub prompt_identity: Option<(String, u32, u32)>,
    next_control: u64,
    pending_files: HashMap<ControlId, (ControlRequest, i64)>,
    pending_agents: HashMap<String, PendingAgent>,
    confirmation: Option<(Confirmation, i64)>,
    unknown_events: HashSet<String>,
    requested_attaches: HashSet<String>,
    attaching: HashSet<String>,
    warned_controls: HashSet<String>,
}
impl ProgressState {
    pub fn blank(manifest: &crate::journal::Manifest) -> Self {
        Self::new(RunProgress::from_records(manifest, &[]))
    }
    pub fn progress(&self) -> &RunProgress {
        &self.progress
    }
    pub fn progress_mut(&mut self) -> &mut RunProgress {
        &mut self.progress
    }
    pub fn subagent_stopping(&self, key: &str, id: &str) -> bool {
        self.pending_agents.get(key).is_some_and(|p| {
            p.action == GatewayAction::SubInterrupt && p.subagent.as_deref() == Some(id)
        })
    }
    pub fn new(progress: RunProgress) -> Self {
        Self {
            progress,
            selection: NodeKey::Run,
            collapsed: HashSet::new(),
            filter: Filter::All,
            connected: false,
            now_ms: 0,
            opened_at_ms: 0,
            last_journal_seen_ms: 0,
            footer: None,
            error: None,
            steer_input: None,
            closed: false,
            revision: 0,
            pending_controls: PendingControls::default(),
            run_suffix: None,
            no_live_runner: false,
            scheduler: None,
            waits: HashMap::new(),
            opened_session: None,
            prompt_first_line: None,
            prompt_identity: None,
            next_control: 0,
            pending_files: HashMap::new(),
            pending_agents: HashMap::new(),
            confirmation: None,
            unknown_events: HashSet::new(),
            requested_attaches: HashSet::new(),
            attaching: HashSet::new(),
            warned_controls: HashSet::new(),
        }
    }
    pub fn select_agent(&mut self, key: &str) {
        self.selection = NodeKey::Agent(key.into());
    }
    pub fn selected_agent_key(&self) -> Option<&str> {
        match &self.selection {
            NodeKey::Agent(k) | NodeKey::Subagent { key: k, .. } => Some(k),
            _ => None,
        }
    }
    pub fn confirmation_text(&self) -> Option<&'static str> {
        self.confirmation.as_ref().map(|(c, _)| match c {
            Confirmation::Restart(_) => "restart? r or y confirms · esc cancels",
            Confirmation::StopRun => "stop run? shift+x or y confirms · esc cancels",
        })
    }
    fn matches(&self, a: &super::AgentNode) -> bool {
        match self.filter {
            Filter::All => true,
            Filter::Working => matches!(
                a.state,
                AgentState::Queued
                    | AgentState::Running
                    | AgentState::Stopping
                    | AgentState::Waiting
            ),
            Filter::NeedsYou => a.state.needs_you(),
            Filter::Failed => matches!(
                a.state,
                AgentState::Failed | AgentState::Unknown | AgentState::Ended
            ),
        }
    }
    pub fn visible_nodes(&self) -> Vec<VisibleNode> {
        let mut out = vec![VisibleNode {
            key: NodeKey::Run,
            depth: 0,
        }];
        if self.collapsed.contains(&NodeKey::Run) {
            return out;
        }
        self.walk_path(&[], 0, &mut out);
        out
    }
    fn walk_path(&self, path: &[String], depth: u16, out: &mut Vec<VisibleNode>) {
        for p in self
            .progress
            .phases
            .iter()
            .filter(|p| p.workflow_path == path)
        {
            if self.filter != Filter::All
                && !p
                    .agents
                    .iter()
                    .filter_map(|k| self.progress.agent(k))
                    .any(|a| self.matches(a) || a.state.needs_you())
            {
                continue;
            }
            let pk = NodeKey::Phase {
                path: path.to_vec(),
                title: p.title.clone(),
            };
            out.push(VisibleNode {
                key: pk.clone(),
                depth: depth + 1,
            });
            if self.collapsed.contains(&pk) {
                continue;
            }
            for a in p
                .agents
                .iter()
                .filter_map(|k| self.progress.agent(k))
                .filter(|a| self.matches(a))
            {
                let ak = NodeKey::Agent(a.key.clone());
                out.push(VisibleNode {
                    key: ak.clone(),
                    depth: depth + 2,
                });
                if self.collapsed.contains(&ak) {
                    continue;
                }
                for row in a.subagents.rows_in_order() {
                    let sub = a.subagents.row(row.index);
                    let sk = NodeKey::Subagent {
                        key: a.key.clone(),
                        id: sub.id.clone(),
                    };
                    // A collapsed subagent hides its descendants using the existing parent ids.
                    let mut parent = sub.parent_id.as_deref();
                    let mut hidden = false;
                    let mut seen = HashSet::new();
                    while let Some(id) = parent {
                        if !seen.insert(id) {
                            break;
                        }
                        if self.collapsed.contains(&NodeKey::Subagent {
                            key: a.key.clone(),
                            id: id.into(),
                        }) {
                            hidden = true;
                            break;
                        }
                        parent = a.subagents.get(id).and_then(|p| p.parent_id.as_deref());
                    }
                    if !hidden {
                        out.push(VisibleNode {
                            key: sk,
                            depth: depth + 3 + u16::from(row.depth),
                        });
                    }
                }
            }
        }
        for c in self.progress.children.iter().filter(|c| {
            c.workflow_path.len() == path.len() + 1 && c.workflow_path.starts_with(path)
        }) {
            let ck = NodeKey::Child(c.workflow_path.clone());
            out.push(VisibleNode {
                key: ck.clone(),
                depth: depth + 1,
            });
            if !self.collapsed.contains(&ck) {
                self.walk_path(&c.workflow_path, depth + 1, out);
            }
        }
    }
    fn repair_selection(&mut self) {
        let visible = self.visible_nodes();
        let keys: HashSet<_> = visible.iter().map(|n| n.key.clone()).collect();
        while !keys.contains(&self.selection) {
            self.selection = self.parent(&self.selection);
        }
    }
    fn parent(&self, key: &NodeKey) -> NodeKey {
        match key {
            NodeKey::Run => NodeKey::Run,
            NodeKey::Agent(k) => self
                .progress
                .agent(k)
                .map(|a| NodeKey::Phase {
                    path: a.workflow_path.clone(),
                    title: a.phase.clone().unwrap_or_else(|| "Agents".into()),
                })
                .unwrap_or(NodeKey::Run),
            NodeKey::Subagent { key, id } => self
                .progress
                .agent(key)
                .and_then(|a| a.subagents.get(id))
                .and_then(|a| a.parent_id.clone())
                .map_or_else(
                    || NodeKey::Agent(key.clone()),
                    |id| NodeKey::Subagent {
                        key: key.clone(),
                        id,
                    },
                ),
            NodeKey::Phase { path, .. } => {
                if path.is_empty() {
                    NodeKey::Run
                } else {
                    NodeKey::Child(path.clone())
                }
            }
            NodeKey::Child(path) => {
                if path.len() < 2 {
                    NodeKey::Run
                } else {
                    NodeKey::Child(path[..path.len() - 1].to_vec())
                }
            }
        }
    }
    fn write(&mut self, request: ControlRequest) -> ProgressEffect {
        self.next_control += 1;
        let id = ControlId(self.next_control);
        self.pending_files
            .insert(id, (request.clone(), self.now_ms));
        ProgressEffect::WriteControl(id, request)
    }
    fn refuse(&mut self, text: impl Into<String>) {
        self.footer = Some(text.into());
        self.revision += 1;
    }
}
#[allow(clippy::large_enum_variant)]
pub enum ProgressInput {
    Journal(Vec<RecordLine>),
    Gateway {
        session_id: String,
        event: Notification,
    },
    Subagent {
        session_id: String,
        event: SubagentEvent,
    },
    ReplayGap {
        session_id: String,
    },
    SchedulerEvent(SchedulerWaitEvent),
    SchedulerStatus(Option<SchedulerSnapshot>),
    ControlDone(ControlId, ControlOutcome),
    Key(ProgressKey),
    Tick,
    Clock {
        now_ms: i64,
    },
    Connected(bool),
    Reopen {
        connected: bool,
    },
    Generation {
        session_id: String,
        generation: i64,
    },
    Controls(PendingControls),
    Attached {
        session_id: String,
        runtime_id: String,
        generation: Option<i64>,
    },
    GatewayDone {
        key: String,
        result: Result<Value, GatewayFailure>,
    },
    GatewayResponse {
        key: String,
        session_id: String,
        result: Result<Value, GatewayFailure>,
    },
    StopReceipt {
        session_id: String,
        admission_id: String,
        attempt: u32,
        outcome: Option<String>,
    },
    Reconciled {
        session_id: String,
        list: hermes_contract::schemas::SubagentListResult,
    },
    Hydrated {
        session_id: String,
        snapshot: hermes_contract::schemas::CanonicalSnapshot,
    },
    Opened(String),
    Error(String),
    PromptLoaded(String),
    PromptLoadedFor {
        key: String,
        attempt: u32,
        turn: u32,
        text: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum ProgressEffect {
    Attach {
        session_id: String,
    },
    AttachAndReconcile {
        session_id: String,
    },
    Steer {
        session_id: String,
        generation: i64,
        text: String,
    },
    Interrupt {
        session_id: String,
        generation: i64,
    },
    CancelQueued {
        session_id: String,
        admission_id: String,
    },
    CheckStopped {
        session_id: String,
        admission_id: String,
        attempt: u32,
    },
    SubSteer {
        session_id: String,
        subagent_id: String,
        text: String,
    },
    SubInterrupt {
        session_id: String,
        subagent_id: String,
    },
    OpenSession {
        session_id: String,
    },
    WriteControl(ControlId, ControlRequest),
}
pub fn update(state: &mut ProgressState, input: ProgressInput) -> Vec<ProgressEffect> {
    let mut effects = vec![];
    match input {
        ProgressInput::Journal(lines) => {
            if !lines.is_empty() {
                let before = state.progress.last_seq;
                let was_terminal = state.progress.run.state != RunState::Running;
                state.progress.apply_all(&lines);
                if state.progress.last_seq > before {
                    state.last_journal_seen_ms = state.now_ms;
                    state.no_live_runner = false;
                    for a in &mut state.progress.agents {
                        a.controls_disabled = false;
                        a.timeout_seq = None;
                    }
                    state.revision += 1;
                    if state.run_suffix.as_deref() == Some("resuming")
                        && lines.iter().any(|l| {
                            l.seq > before
                                && matches!(
                                    &l.record,
                                    crate::journal::Record::CallIntent { .. }
                                        | crate::journal::Record::TurnAdmitted {
                                            status: crate::journal::AdmissionState::Started,
                                            ..
                                        }
                                )
                        })
                    {
                        state.run_suffix = None;
                    }
                }
                // An execution boundary clears the old display even when it resumed a failed run.
                let boundary = state.progress.last_seq > before
                    && lines.iter().any(|l| {
                        serde_json::to_value(&l.record)
                            .ok()
                            .and_then(|v| {
                                v["t"]
                                    .as_str()
                                    .map(|s| s == "run_resumed" || s == "run_started")
                            })
                            .unwrap_or(false)
                    });
                if boundary || (was_terminal && state.progress.run.state == RunState::Running) {
                    state.run_suffix = None;
                    state.pending_files.clear();
                    state.pending_agents.clear();
                    state.warned_controls.clear();
                    state.requested_attaches.clear();
                    state.attaching.clear();
                }
                state.pending_agents.retain(|key, p| {
                    state
                        .progress
                        .agent(key)
                        .is_some_and(|a| a.attempt == p.attempt)
                });
                let settled: Vec<_> = state
                    .pending_agents
                    .keys()
                    .filter(|key| {
                        state
                            .progress
                            .agent(key)
                            .is_some_and(|a| a.state.terminal())
                    })
                    .cloned()
                    .collect();
                for key in settled {
                    if let Some(p) = state.pending_agents.remove(&key) {
                        if p.action == GatewayAction::Restart && p.stage != PendingStage::Settling {
                            effects.push(state.write(ControlRequest::Restart { key: key.clone() }));
                        } else if matches!(
                            p.action,
                            GatewayAction::Interrupt | GatewayAction::Cancel
                        ) {
                            let completed = state.progress.agent(&key).is_some_and(|a| {
                                a.state == AgentState::Done
                                    || a.last_outcome.as_deref() == Some("completed")
                            });
                            state.refuse(if completed {
                                "finished before stop"
                            } else {
                                "stopped — restart (r) runs it again"
                            });
                        }
                    }
                }
                if state.connected {
                    effects.extend(attach_new_sessions(state));
                }
            }
        }
        ProgressInput::Gateway { session_id, event } => {
            effects.extend(gateway(state, &session_id, event))
        }
        ProgressInput::Subagent { session_id, event } => {
            let now = clock(state);
            if let Some(a) = state.progress.for_session_mut(&session_id) {
                if !a.state.terminal() {
                    a.live_active = true;
                    if a.timeout_seq.is_none() {
                        a.controls_disabled = false;
                    }
                }
                let applied = a.subagents.apply(&event, now);
                if applied.changed {
                    state.revision += 1;
                }
                a.derive();
            }
        }
        ProgressInput::ReplayGap { session_id } => {
            if state.progress.for_session_mut(&session_id).is_some() && state.connected {
                effects.push(ProgressEffect::AttachAndReconcile { session_id });
            }
        }
        ProgressInput::SchedulerEvent(event) => {
            match event {
                SchedulerWaitEvent::Queued(wait) => {
                    state.waits.insert(wait.key, wait.reason);
                }
                SchedulerWaitEvent::Admitted { key }
                | SchedulerWaitEvent::Released { key }
                | SchedulerWaitEvent::Skipped { key } => {
                    state.waits.remove(&key);
                }
            }
            state.revision += 1;
        }
        ProgressInput::SchedulerStatus(status) => {
            if state.scheduler != status {
                state.scheduler = status;
                state.revision += 1;
            }
        }
        ProgressInput::Generation {
            session_id,
            generation,
        } => {
            if let Some(a) = state.progress.for_session_mut(&session_id) {
                hold_generation(a, Some(generation));
                if a.timeout_seq.is_none() {
                    a.controls_disabled = false;
                }
            }
        }
        ProgressInput::Attached {
            session_id,
            runtime_id,
            generation,
        } => {
            state.progress.bind_runtime(&session_id, &runtime_id);
            let key = state.progress.for_session_mut(&runtime_id).map(|a| {
                hold_generation(a, generation);
                a.key.clone()
            });
            if let Some(key) = key
                && state
                    .pending_agents
                    .get(&key)
                    .is_some_and(|p| p.stage == PendingStage::Refreshing)
            {
                effects.extend(send_pending(state, &key));
            }
        }
        ProgressInput::GatewayDone { key, result } => {
            effects.extend(gateway_done(state, &key, result))
        }
        ProgressInput::GatewayResponse {
            key,
            session_id,
            result,
        } => {
            if result.is_err() {
                state.attaching.remove(&session_id);
            }
            if state.progress.agent(&key).is_some_and(|a| {
                a.session_id.as_deref() == Some(&session_id)
                    || a.stored_session_id.as_deref() == Some(&session_id)
            }) {
                effects.extend(gateway_done(state, &key, result));
            }
        }
        ProgressInput::StopReceipt {
            session_id,
            admission_id,
            attempt,
            outcome,
        } => {
            let key = state
                .progress
                .for_session_mut(&session_id)
                .filter(|a| {
                    a.attempt == attempt && a.admission_id.as_deref() == Some(&admission_id)
                })
                .map(|a| a.key.clone());
            if let Some(key) = key
                && let Some(pending) = state.pending_agents.get_mut(&key)
                && pending.attempt == attempt
                && matches!(
                    pending.action,
                    GatewayAction::Interrupt | GatewayAction::Cancel
                )
            {
                pending.receipt_in_flight = false;
                if let Some(outcome) = outcome.filter(|o| {
                    matches!(
                        o.as_str(),
                        "interrupted" | "failed" | "cancelled" | "completed"
                    )
                }) {
                    state.pending_agents.remove(&key);
                    if let Some(a) = state.progress.agent_mut(&key) {
                        a.stopped = outcome != "completed";
                        a.last_outcome =
                            Some(if a.stopped { "failed" } else { "completed" }.into());
                        a.finished_at_ms = Some(state.now_ms);
                        a.stopping = false;
                        a.live_active = false;
                        a.prompts.clear();
                        a.derive();
                    }
                    state.refuse(if outcome == "completed" {
                        "finished before stop"
                    } else {
                        "stopped — restart (r) runs it again"
                    });
                }
            }
        }
        ProgressInput::Reconciled { session_id, list } => {
            let now = clock(state);
            if let Some(a) = state.progress.for_session_mut(&session_id)
                && a.subagents.reconcile(&list, now).changed
            {
                state.revision += 1;
            }
        }
        ProgressInput::Hydrated {
            session_id,
            snapshot,
        } => {
            state.attaching.remove(&session_id);
            if let Some(a) = state.progress.for_session_mut(&session_id)
                && !a.state.terminal()
            {
                if let Some(stored) = &a.stored_session_id {
                    state.attaching.remove(stored);
                }
                a.prompts.clear();
                for p in snapshot.prompts {
                    a.prompts
                        .insert((session_id.clone(), p.prompt_id, p.execution_generation));
                }
                a.live_active = snapshot.running;
                hold_generation(a, Some(snapshot.execution_generation));
                a.derive();
                state.revision += 1;
            }
        }
        ProgressInput::Connected(connected) => {
            if state.connected != connected {
                state.connected = connected;
                state.unknown_events.clear();
                state.requested_attaches.clear();
                state.attaching.clear();
                state.revision += 1;
                if connected {
                    effects.extend(attach_new_sessions(state));
                }
            }
        }
        ProgressInput::Reopen { connected } => {
            if state.connected != connected {
                state.unknown_events.clear();
            }
            state.connected = connected;
            state.requested_attaches.clear();
            state.attaching.clear();
            state.closed = false;
            state.revision += 1;
            if connected {
                effects.extend(attach_new_sessions(state));
            }
        }
        ProgressInput::Controls(controls) => {
            if state.pending_controls != controls {
                let removed = state
                    .pending_controls
                    .pause
                    .as_ref()
                    .filter(|_| controls.pause.is_none());
                let resumed_already = removed
                    .is_some_and(|p| state.progress.last_intent_ms.is_some_and(|at| at > p.at_ms));
                let removed = removed.is_some();
                state.pending_controls = controls;
                if removed {
                    state.run_suffix = if resumed_already {
                        None
                    } else {
                        Some("resuming".into())
                    };
                }
                for a in &mut state.progress.agents {
                    a.restart_requested |= state
                        .pending_controls
                        .restarts
                        .iter()
                        .any(|r| r.key.as_deref() == Some(&a.key));
                }
                state.revision += 1;
            }
        }
        ProgressInput::ControlDone(id, outcome) => {
            if let Some((request, at_ms)) = state.pending_files.remove(&id) {
                match outcome {
                    ControlOutcome::Published => {
                        state.footer = Some("requested".into());
                        match request {
                            ControlRequest::Pause => {
                                state.run_suffix = Some("pausing".into());
                                state.pending_controls.pause = Some(super::PendingRequest {
                                    v: 1,
                                    kind: "pause".into(),
                                    key: None,
                                    at_ms,
                                });
                            }
                            ControlRequest::Stop => {
                                state.run_suffix = Some("stopping".into());
                                state.pending_controls.stop = Some(super::PendingRequest {
                                    v: 1,
                                    kind: "stop".into(),
                                    key: None,
                                    at_ms,
                                });
                            }
                            ControlRequest::Restart { key } => {
                                if let Some(a) = state.progress.agent_mut(&key) {
                                    a.restart_requested = true;
                                    state.footer =
                                        Some("restart requested — runs again on resume".into());
                                }
                            }
                            ControlRequest::Resume => {}
                        }
                    }
                    ControlOutcome::AlreadyRequested => {
                        state.footer = Some("already requested".into())
                    }
                    ControlOutcome::Removed => {
                        if let Some(pause) = state.pending_controls.pause.take() {
                            state.run_suffix = if state
                                .progress
                                .last_intent_ms
                                .is_some_and(|at| at > pause.at_ms)
                            {
                                None
                            } else {
                                Some("resuming".into())
                            };
                        }
                    }
                    ControlOutcome::Refused { reason } => state.footer = Some(reason),
                }
                state.revision += 1;
            }
        }
        ProgressInput::Key(key) => effects.extend(key_input(state, key)),
        ProgressInput::Clock { now_ms } => {
            if state.now_ms / 1000 != now_ms / 1000 && state.progress.run.counts.running > 0 {
                state.revision += 1;
            }
            state.now_ms = now_ms;
            effects.extend(tick(state));
        }
        ProgressInput::Tick => effects.extend(tick(state)),
        ProgressInput::Opened(session) => {
            state.opened_session = Some(session);
            state.closed = true;
            state.revision += 1;
        }
        ProgressInput::Error(reason) => {
            state.error = Some(reason);
            state.revision += 1;
        }
        ProgressInput::PromptLoaded(text) => {
            state.prompt_first_line = Some(text);
            state.revision += 1;
        }
        ProgressInput::PromptLoadedFor {
            key,
            attempt,
            turn,
            text,
        } => {
            if state.selected_agent_key() == Some(&key)
                && state
                    .progress
                    .agent(&key)
                    .is_some_and(|a| a.attempt == attempt && a.turn == turn)
            {
                if text == "(prompt unavailable — journal blob missing)" {
                    state.footer = Some(text.clone());
                }
                state.prompt_first_line = Some(text);
                state.revision += 1;
            }
        }
    }
    state.pending_agents.retain(|key, pending| {
        pending.action != GatewayAction::SubInterrupt
            || !pending.subagent.as_deref().is_some_and(|id| {
                state
                    .progress
                    .agent(key)
                    .and_then(|a| a.subagents.get(id))
                    .is_some_and(|row| row.state.is_terminal())
            })
    });
    state.progress.recount();
    state.repair_selection();
    effects
}
fn attach_new_sessions(state: &mut ProgressState) -> Vec<ProgressEffect> {
    attach_in_tree_order(state)
        .into_iter()
        .filter(|effect| {
            matches!(effect,
                ProgressEffect::Attach { session_id }
                    if state.requested_attaches.insert(session_id.clone())
                        && state.attaching.insert(session_id.clone())
            )
        })
        .collect()
}
fn attach_in_tree_order(state: &ProgressState) -> Vec<ProgressEffect> {
    fn walk(
        state: &ProgressState,
        path: &[String],
        seen: &mut HashSet<String>,
        out: &mut Vec<ProgressEffect>,
    ) {
        for phase in state
            .progress
            .phases
            .iter()
            .filter(|p| p.workflow_path == path)
        {
            for key in &phase.agents {
                if let Some(a) = state.progress.agent(key) {
                    push(a, seen, out);
                }
            }
        }
        for child in state.progress.children.iter().filter(|c| {
            c.workflow_path.len() == path.len() + 1 && c.workflow_path.starts_with(path)
        }) {
            walk(state, &child.workflow_path, seen, out);
        }
    }
    fn push(a: &super::AgentNode, seen: &mut HashSet<String>, out: &mut Vec<ProgressEffect>) {
        if seen.insert(a.key.clone())
            && matches!(
                a.state,
                AgentState::Queued
                    | AgentState::Running
                    | AgentState::Waiting
                    | AgentState::Stopping
            )
            && let Some(session_id) = a.stored_session_id.clone().or_else(|| a.session_id.clone())
        {
            out.push(ProgressEffect::Attach { session_id });
        }
    }
    let mut out = vec![];
    let mut seen = HashSet::new();
    walk(state, &[], &mut seen, &mut out);
    for agent in &state.progress.agents {
        push(agent, &mut seen, &mut out);
    }
    out
}
fn clock(s: &ProgressState) -> Clock {
    Clock {
        mono_ms: s.now_ms.max(0) as u64,
        wall_s: s.now_ms as f64 / 1000.0,
    }
}
fn tick(s: &mut ProgressState) -> Vec<ProgressEffect> {
    let mut effects = vec![];
    if s.connected {
        // A queued submit can resume before the gateway starts its turn. Retry once
        // the previous snapshot has returned, without piling up in-flight resumes.
        for effect in attach_in_tree_order(s) {
            if let ProgressEffect::Attach { session_id } = &effect
                && s.progress.agents.iter().any(|a| {
                    a.state == AgentState::Queued
                        && (a.session_id.as_ref() == Some(session_id)
                            || a.stored_session_id.as_ref() == Some(session_id))
                })
                && s.attaching.insert(session_id.clone())
            {
                effects.push(effect);
            }
        }
        for (key, p) in &mut s.pending_agents {
            if p.stage == PendingStage::Settling
                && matches!(p.action, GatewayAction::Interrupt | GatewayAction::Cancel)
                && !p.receipt_in_flight
                && s.now_ms - p.receipt_at_ms >= 1000
                && s.now_ms - p.at_ms < 10000
                && let Some(a) = s.progress.agent(key)
                && let (Some(session_id), Some(admission_id)) = (&a.session_id, &a.admission_id)
            {
                p.receipt_in_flight = true;
                p.receipt_at_ms = s.now_ms;
                effects.push(ProgressEffect::CheckStopped {
                    session_id: session_id.clone(),
                    admission_id: admission_id.clone(),
                    attempt: p.attempt,
                });
            }
        }
    }
    if s.confirmation
        .as_ref()
        .is_some_and(|(_, at)| s.now_ms - *at >= 3000)
    {
        s.confirmation = None;
        s.refuse("confirmation expired");
    }
    let timed: Vec<_> = s
        .pending_agents
        .iter()
        .filter(|(_, p)| s.now_ms - p.at_ms >= 10000)
        .map(|(k, _)| k.clone())
        .collect();
    for key in timed {
        s.pending_agents.remove(&key);
        let last_seq = s.progress.last_seq;
        if let Some(a) = s.progress.agent_mut(&key) {
            a.stopping = false;
            a.controls_disabled = true;
            a.timeout_seq = Some(last_seq);
            a.derive();
        }
        s.refuse("no answer — the journal has not moved");
    }
    let missing =
        s.progress.run.state == RunState::Running && s.now_ms - s.last_journal_seen_ms >= 60000;
    if s.no_live_runner != missing {
        s.no_live_runner = missing;
        s.revision += 1;
    }
    for (name, request) in [
        ("pause", s.pending_controls.pause.clone()),
        ("stop", s.pending_controls.stop.clone()),
    ] {
        if s.progress.run.state != RunState::Running {
            break;
        }
        if let Some(request) = request {
            let age = s.now_ms - request.at_ms;
            let runner_recent = s.last_journal_seen_ms > s.opened_at_ms
                && s.now_ms - s.last_journal_seen_ms < 10000;
            if age >= 10000
                && !runner_recent
                && s.warned_controls
                    .insert(format!("{name}:{}", request.at_ms))
            {
                s.refuse(
                    "no runner answered — it may be detached; talaria workflow stop still works",
                );
            }
            if name == "pause"
                && age >= 1000
                && runner_recent
                && s.progress
                    .last_intent_ms
                    .is_none_or(|at| at <= request.at_ms)
                && s.run_suffix.as_deref() != Some("paused")
            {
                s.run_suffix = Some("paused".into());
                s.revision += 1;
            }
        }
    }
    if s.progress.run.state != RunState::Running && s.run_suffix.take().is_some() {
        s.revision += 1;
    }
    effects
}
fn gateway(s: &mut ProgressState, session: &str, event: Notification) -> Vec<ProgressEffect> {
    if matches!(event, Notification::SessionReplayGap(_)) {
        return update(
            s,
            ProgressInput::ReplayGap {
                session_id: session.into(),
            },
        );
    }
    let supported = matches!(
        event,
        Notification::MessageStart(_)
            | Notification::MessageDelta(_)
            | Notification::MessageComplete(_)
            | Notification::ToolStart(_)
            | Notification::ToolComplete(_)
            | Notification::ToolProgress(_)
            | Notification::SessionUsage(_)
            | Notification::ApprovalRequest(_)
            | Notification::ApprovalSettled(_)
            | Notification::ClarifyRequest(_)
            | Notification::ClarifySettled(_)
    );
    if !supported {
        let frame = hermes_contract::events::EventFrame {
            session_id: None,
            seq: None,
            replay_epoch: None,
            authority_epoch: None,
            execution_generation: None,
            admission_id: None,
            notification: event,
        };
        let name = frame.type_name().unwrap_or("unknown").to_string();
        if s.unknown_events.insert(name.clone()) {
            s.refuse(format!("unknown event type {name} ignored"));
        }
        return vec![];
    }
    let Some(a) = s.progress.for_session_mut(session) else {
        return vec![];
    };
    if a.state.terminal() {
        return vec![];
    }
    let before = (
        a.state,
        a.tokens,
        a.last_tool.clone(),
        a.tail.clone(),
        a.prompts.clone(),
        a.controls_disabled,
    );
    a.live_active = true;
    if a.timeout_seq.is_none() {
        a.controls_disabled = false;
    }
    match event {
        Notification::MessageDelta(p) => {
            if let Some(text) = p.text
                && !text.is_empty()
            {
                a.push_tail(&text);
            }
        }
        Notification::MessageComplete(p) => {
            if let Some(text) = p.text.as_ref().and_then(Value::as_str) {
                a.push_tail(text);
            }
            if let Some(usage) = p
                .usage
                .and_then(|u| serde_json::to_value(u).ok())
                .and_then(|u| live_tokens(&u))
            {
                a.live_sessions
                    .entry(a.session_id.clone().unwrap_or_else(|| session.into()))
                    .and_modify(|n| *n = (*n).max(usage))
                    .or_insert(usage);
            }
        }
        Notification::SessionUsage(p) => {
            let usage = p
                .total_tokens
                .or_else(|| Some(p.prompt_tokens? + p.completion_tokens?))
                .or_else(|| Some(p.input_tokens? + p.output_tokens?));
            if let Some(n) = usage.and_then(|n| u64::try_from(n).ok()) {
                a.live_sessions
                    .entry(a.session_id.clone().unwrap_or_else(|| session.into()))
                    .and_modify(|old| *old = (*old).max(n))
                    .or_insert(n);
            }
        }
        Notification::ToolStart(p) => {
            a.last_tool = Some(format!("{} {}", p.name, p.preview.unwrap_or_default()));
        }
        Notification::ToolComplete(p) => {
            a.last_tool = Some(p.name);
            if let Some(text) = p.result_text {
                a.push_tail(&text);
            }
        }
        Notification::ToolProgress(p) => {
            if let Ok(v) = serde_json::to_value(p)
                && let Some(text) = v["text"].as_str().or_else(|| v["message"].as_str())
            {
                a.push_tail(text);
            }
        }
        Notification::ApprovalRequest(p) => {
            a.prompts.insert((
                session.to_string(),
                p.prompt_id,
                p.execution_generation.unwrap_or(-1),
            ));
        }
        Notification::ClarifyRequest(p) => {
            a.prompts.insert((
                session.to_string(),
                p.prompt_id,
                p.execution_generation.unwrap_or(-1),
            ));
        }
        Notification::ApprovalSettled(p) => {
            a.prompts.remove(&(
                session.to_string(),
                p.prompt_id,
                p.execution_generation.unwrap_or(-1),
            ));
        }
        Notification::ClarifySettled(p) => {
            a.prompts.remove(&(
                session.to_string(),
                p.prompt_id,
                p.execution_generation.unwrap_or(-1),
            ));
        }
        _ => {}
    }
    a.derive();
    if before
        != (
            a.state,
            a.tokens,
            a.last_tool.clone(),
            a.tail.clone(),
            a.prompts.clone(),
            a.controls_disabled,
        )
    {
        s.revision += 1;
    }
    vec![]
}
fn key_input(s: &mut ProgressState, key: ProgressKey) -> Vec<ProgressEffect> {
    if s.error.is_some() {
        if key == ProgressKey::CloseView {
            s.closed = true;
            s.revision += 1;
        }
        return vec![];
    }
    if let Some((confirmation, _)) = s.confirmation.take() {
        let accepted = matches!(key, ProgressKey::Confirm | ProgressKey::Char('y'))
            || matches!(
                (&confirmation, &key),
                (Confirmation::Restart(_), ProgressKey::RestartAgent)
                    | (Confirmation::StopRun, ProgressKey::StopRun)
            );
        s.revision += 1;
        if accepted {
            return match confirmation {
                Confirmation::StopRun => vec![s.write(ControlRequest::Stop)],
                Confirmation::Restart(key) => restart(s, &key),
            };
        }
        return vec![];
    }
    if s.steer_input.is_some() {
        match key {
            ProgressKey::CloseView => s.steer_input = None,
            ProgressKey::Char(c) => {
                if !c.is_control()
                    && let Some(text) = &mut s.steer_input
                {
                    text.push(c);
                }
            }
            ProgressKey::Backspace => {
                if let Some(text) = &mut s.steer_input {
                    text.pop();
                }
            }
            ProgressKey::Enter | ProgressKey::OpenAgent => {
                let text = s.steer_input.take().unwrap_or_default();
                if text.trim().is_empty() {
                    s.steer_input = Some(text);
                    s.refuse("nothing to steer — type a message first");
                } else if let Some(key) = s.selected_agent_key().map(str::to_owned) {
                    return start_agent(s, &key, GatewayAction::Steer, Some(text));
                }
            }
            _ => {}
        }
        s.revision += 1;
        return vec![];
    }
    match key {
        ProgressKey::Up | ProgressKey::Down => {
            let rows = s.visible_nodes();
            if let Some(i) = rows.iter().position(|n| n.key == s.selection) {
                let next = if key == ProgressKey::Up {
                    i.saturating_sub(1)
                } else {
                    (i + 1).min(rows.len() - 1)
                };
                if next != i {
                    s.selection = rows[next].key.clone();
                    s.prompt_first_line = None;
                    s.revision += 1;
                }
            }
        }
        ProgressKey::Left => {
            if !s.collapsed.insert(s.selection.clone()) {
                s.selection = s.parent(&s.selection);
            }
            s.revision += 1;
        }
        ProgressKey::Right => {
            if s.collapsed.remove(&s.selection) {
                s.revision += 1;
            }
        }
        ProgressKey::CycleFilter => {
            s.filter = match s.filter {
                Filter::All => Filter::Working,
                Filter::Working => Filter::NeedsYou,
                Filter::NeedsYou => Filter::Failed,
                Filter::Failed => Filter::All,
            };
            s.revision += 1;
        }
        ProgressKey::CloseView => {
            s.closed = true;
            s.revision += 1;
        }
        ProgressKey::PauseRun => {
            if s.progress.run.state != RunState::Running {
                s.refuse("run already finished");
            } else {
                return vec![s.write(if s.pending_controls.pause.is_some() {
                    ControlRequest::Resume
                } else {
                    ControlRequest::Pause
                })];
            }
        }
        ProgressKey::StopRun => {
            if s.progress.run.state != RunState::Running {
                s.refuse("run already finished");
            } else {
                s.confirmation = Some((Confirmation::StopRun, s.now_ms));
                s.revision += 1;
            }
        }
        ProgressKey::OpenAgent | ProgressKey::Enter => {
            if !s.connected {
                s.refuse("gateway disconnected — showing the journal");
                return vec![];
            }
            if let Some(key) = s.selected_agent_key() {
                if let Some(a) = s.progress.agent(key) {
                    let session = match &s.selection {
                        NodeKey::Subagent { id, .. } => {
                            a.subagents.get(id).and_then(|r| r.child_session_id.clone())
                        }
                        _ => a.stored_session_id.clone().or_else(|| a.session_id.clone()),
                    };
                    if let Some(session_id) = session {
                        return vec![ProgressEffect::OpenSession { session_id }];
                    }
                    if matches!(s.selection, NodeKey::Subagent { .. }) {
                        s.refuse("child session unavailable — showing its tail on the parent");
                    } else {
                        s.refuse("no session bound yet");
                    }
                }
            } else {
                s.refuse("select an agent first");
            }
        }
        ProgressKey::Steer | ProgressKey::StopAgent | ProgressKey::RestartAgent => {
            let Some(agent) = s.selected_agent_key().map(str::to_owned) else {
                s.refuse("select an agent first");
                return vec![];
            };
            if key == ProgressKey::RestartAgent {
                if matches!(s.selection, NodeKey::Subagent { .. }) {
                    s.refuse("restart is available for workflow agents");
                    return vec![];
                }
                let Some(a) = s.progress.agent(&agent) else {
                    return vec![];
                };
                if matches!(a.state, AgentState::Done | AgentState::Skipped) {
                    s.refuse("finished agents are kept — restart would pay for the work twice");
                } else if !matches!(
                    a.state,
                    AgentState::Running | AgentState::Failed | AgentState::Ended
                ) {
                    s.refuse("restart needs a settled or running agent");
                } else {
                    s.confirmation = Some((Confirmation::Restart(agent), s.now_ms));
                    s.revision += 1;
                }
                return vec![];
            }
            if key == ProgressKey::Steer {
                if can_agent(s, &agent, GatewayAction::Steer) {
                    s.steer_input = Some(String::new());
                    s.revision += 1;
                }
                return vec![];
            }
            return start_agent(s, &agent, GatewayAction::Interrupt, None);
        }
        _ => {}
    }
    vec![]
}
fn can_agent(s: &mut ProgressState, key: &str, action: GatewayAction) -> bool {
    if !s.connected {
        s.refuse("gateway disconnected — showing the journal");
        return false;
    }
    if s.pending_agents.contains_key(key) {
        s.refuse("still working on it");
        return false;
    }
    let Some(a) = s.progress.agent(key) else {
        return false;
    };
    if a.controls_disabled {
        s.refuse("generation unknown — controls unavailable until it next speaks");
        return false;
    }
    if let NodeKey::Subagent { id, .. } = &s.selection
        && let Some(row) = a.subagents.get(id)
    {
        if row.state.is_terminal() {
            s.refuse("already finished or not started");
            return false;
        }
        if row.state == talaria_core::subagents::RowState::Queued && action == GatewayAction::Steer
        {
            s.refuse("queued subagent — steer is unavailable until it starts");
            return false;
        }
        return true;
    }
    if a.state == AgentState::Running {
        return true;
    }
    if action == GatewayAction::Interrupt && a.state == AgentState::Queued {
        if a.admission_id.is_some() {
            return true;
        }
        s.refuse("nothing to cancel yet — the runner has not queued it");
        return false;
    }
    s.refuse(format!("{} agent — controls unavailable", a.state.as_str()));
    false
}
fn start_agent(
    s: &mut ProgressState,
    key: &str,
    action: GatewayAction,
    text: Option<String>,
) -> Vec<ProgressEffect> {
    if !can_agent(s, key, action) {
        return vec![];
    }
    let subagent = match &s.selection {
        NodeKey::Subagent { id, .. } => Some(id.clone()),
        _ => None,
    };
    let Some(a) = s.progress.agent(key) else {
        return vec![];
    };
    let action = if subagent.is_some() {
        if action == GatewayAction::Steer {
            GatewayAction::SubSteer
        } else {
            GatewayAction::SubInterrupt
        }
    } else if a.state == AgentState::Queued {
        GatewayAction::Cancel
    } else {
        action
    };
    s.pending_agents.insert(
        key.into(),
        PendingAgent {
            action,
            text,
            subagent,
            at_ms: s.now_ms,
            refreshed: false,
            stage: PendingStage::Ready,
            attempt: a.attempt,
            receipt_at_ms: s.now_ms,
            receipt_in_flight: false,
        },
    );
    send_pending(s, key)
}
fn send_pending(s: &mut ProgressState, key: &str) -> Vec<ProgressEffect> {
    let Some(p) = s.pending_agents.get_mut(key) else {
        return vec![];
    };
    let Some(a) = s.progress.agent_mut(key) else {
        return vec![];
    };
    let Some(session) = a.session_id.clone() else {
        s.pending_agents.remove(key);
        s.refuse("no session bound yet");
        return vec![];
    };
    if matches!(
        p.action,
        GatewayAction::Steer | GatewayAction::Interrupt | GatewayAction::Restart
    ) && a.execution_generation.is_none()
    {
        if p.refreshed {
            s.pending_agents.remove(key);
            a.controls_disabled = true;
            s.refuse("generation unknown — controls unavailable until it next speaks");
            return vec![];
        }
        p.refreshed = true;
        p.stage = PendingStage::Refreshing;
        return vec![ProgressEffect::Attach {
            session_id: a.stored_session_id.clone().unwrap_or(session),
        }];
    }
    p.stage = PendingStage::Calling;
    let generation = a.execution_generation.unwrap_or(0);
    let effect = match p.action {
        GatewayAction::Steer => ProgressEffect::Steer {
            session_id: session,
            generation,
            text: p.text.clone().unwrap_or_default(),
        },
        GatewayAction::Interrupt | GatewayAction::Restart => {
            a.stopping = true;
            a.derive();
            ProgressEffect::Interrupt {
                session_id: session,
                generation,
            }
        }
        GatewayAction::Cancel => {
            a.stopping = true;
            a.derive();
            ProgressEffect::CancelQueued {
                session_id: session,
                admission_id: a.admission_id.clone().unwrap_or_default(),
            }
        }
        GatewayAction::SubSteer => ProgressEffect::SubSteer {
            session_id: session,
            subagent_id: p.subagent.clone().unwrap_or_default(),
            text: p.text.clone().unwrap_or_default(),
        },
        GatewayAction::SubInterrupt => ProgressEffect::SubInterrupt {
            session_id: session,
            subagent_id: p.subagent.clone().unwrap_or_default(),
        },
    };
    s.footer = Some("sent".into());
    s.revision += 1;
    vec![effect]
}
fn gateway_done(
    s: &mut ProgressState,
    key: &str,
    result: Result<Value, GatewayFailure>,
) -> Vec<ProgressEffect> {
    let Some(p) = s.pending_agents.get(key).cloned() else {
        return vec![];
    };
    match result {
        Err(error) => {
            if error.stale_generation && !p.refreshed {
                if let Some(p) = s.pending_agents.get_mut(key) {
                    p.refreshed = true;
                    p.stage = PendingStage::Refreshing;
                }
                if let Some(a) = s.progress.agent(key)
                    && let Some(session_id) =
                        a.stored_session_id.clone().or_else(|| a.session_id.clone())
                {
                    return vec![ProgressEffect::Attach { session_id }];
                }
            }
            s.pending_agents.remove(key);
            if let Some(a) = s.progress.agent_mut(key) {
                a.stopping = false;
                if error.stale_generation {
                    a.controls_disabled = true;
                }
                a.derive();
            }
            if error.disconnected {
                s.connected = false;
                s.refuse("gateway disconnected — showing the journal");
            } else if error.stale_generation {
                s.refuse("generation unknown — controls unavailable until it next speaks");
            } else {
                s.refuse(error.reason);
            }
        }
        Ok(value) => {
            if let Some(a) = s.progress.agent_mut(key) {
                hold_generation(a, value["execution_generation"].as_i64());
            }
            match p.action {
                GatewayAction::Steer | GatewayAction::SubSteer => {
                    s.pending_agents.remove(key);
                    if value["voice_stopped"].as_bool() == Some(true) {
                        s.refuse("steer refused — Hermes treated it as a voice stop");
                    } else if matches!(value["status"].as_str(), Some("queued" | "redirected")) {
                        s.refuse(if p.action == GatewayAction::SubSteer {
                            "steer queued for its next tool result"
                        } else {
                            "steer queued for its next step"
                        });
                    } else {
                        s.refuse(value["text"].as_str().unwrap_or("steer refused"));
                    }
                }
                GatewayAction::Cancel if value["status"].as_str() == Some("started") => {
                    s.pending_agents.remove(key);
                    if let Some(a) = s.progress.agent_mut(key) {
                        a.stopping = false;
                        a.admission_status = Some("started".into());
                        a.derive();
                    }
                    s.refuse("already started — stop (x) interrupts it");
                }
                GatewayAction::Restart => {
                    if let Some(p) = s.pending_agents.get_mut(key) {
                        p.stage = PendingStage::Settling;
                    }
                    return vec![s.write(ControlRequest::Restart { key: key.into() })];
                }
                GatewayAction::SubInterrupt => {
                    if value["found"].as_bool() == Some(true) {
                        if let Some(p) = s.pending_agents.get_mut(key) {
                            p.stage = PendingStage::Settling;
                        }
                        s.refuse("stopping");
                    } else {
                        s.pending_agents.remove(key);
                        s.refuse("already finished or not started");
                    }
                }
                _ => {
                    if let Some(p) = s.pending_agents.get_mut(key) {
                        p.stage = PendingStage::Settling;
                    }
                    s.refuse("stopping");
                }
            }
        }
    }
    vec![]
}
fn restart(s: &mut ProgressState, key: &str) -> Vec<ProgressEffect> {
    let Some(a) = s.progress.agent(key) else {
        return vec![];
    };
    if matches!(a.state, AgentState::Done | AgentState::Skipped) {
        s.refuse("finished agents are kept — restart would pay for the work twice");
        return vec![];
    }
    if a.state == AgentState::Running {
        let effects = start_agent(s, key, GatewayAction::Restart, None);
        return effects;
    }
    if matches!(a.state, AgentState::Failed | AgentState::Ended) {
        return vec![s.write(ControlRequest::Restart { key: key.into() })];
    }
    s.refuse("restart needs a settled or running agent");
    vec![]
}
