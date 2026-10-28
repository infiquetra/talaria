//! Journal-owned rows with optional, non-authoritative live enrichment.
pub mod blobs;
pub mod control;
pub mod live;
mod nodes;
pub mod tail;
pub mod text;
pub use blobs::{BlobError, BlobRef, load_text};
pub use control::*;
pub use live::{
    Filter, ProgressEffect, ProgressInput, ProgressKey, ProgressState, SchedulerSnapshot,
    SchedulerWait, SchedulerWaitEvent, WaitView, update,
};
pub use nodes::*;
pub use tail::Tail;
pub mod journal_tree {
    pub use super::{
        AgentNode, AgentState, ChildNode, Counts, PhaseNode, RunNode, RunProgress, RunState, Tail,
    };
}

use crate::journal::{Manifest, RecordLine};
use serde_json::Value;
use std::collections::HashMap;

pub struct RunProgress {
    pub run: RunNode,
    pub phases: Vec<PhaseNode>,
    pub children: Vec<ChildNode>,
    pub agents: Vec<AgentNode>,
    pub last_seq: u64,
    pub last_at_ms: Option<i64>,
    pub last_intent_ms: Option<i64>,
    indices: HashMap<String, usize>,
    sessions: HashMap<String, usize>,
    calls: HashMap<String, usize>,
    starts: HashMap<String, (u64, i64)>,
    next_order: u64,
    phase_indices: HashMap<(Vec<String>, String), usize>,
}
impl RunProgress {
    pub fn blank(manifest: &Manifest) -> Self {
        Self::from_records(manifest, &[])
    }
    pub fn agents(&self) -> &[AgentNode] {
        &self.agents
    }
    pub fn empty(name: String) -> Self {
        Self {
            run: RunNode {
                name,
                ..RunNode::default()
            },
            phases: vec![],
            children: vec![],
            agents: vec![],
            last_seq: 0,
            last_at_ms: None,
            last_intent_ms: None,
            indices: HashMap::new(),
            sessions: HashMap::new(),
            calls: HashMap::new(),
            starts: HashMap::new(),
            next_order: 0,
            phase_indices: HashMap::new(),
        }
    }
    pub fn from_records(manifest: &Manifest, lines: &[RecordLine]) -> Self {
        let mut result = Self::empty(manifest.workflow.name.clone());
        for line in lines {
            result.apply_inner(line);
        }
        result.recount();
        result
    }
    pub fn run(&self) -> &RunNode {
        &self.run
    }
    pub fn agent(&self, key: &str) -> Option<&AgentNode> {
        self.indices.get(key).map(|i| &self.agents[*i])
    }
    pub fn agent_mut(&mut self, key: &str) -> Option<&mut AgentNode> {
        let i = *self.indices.get(key)?;
        self.agents.get_mut(i)
    }
    pub fn for_session_mut(&mut self, session: &str) -> Option<&mut AgentNode> {
        let i = *self.sessions.get(session)?;
        self.agents.get_mut(i)
    }
    pub fn bind_runtime(&mut self, stored: &str, runtime: &str) {
        if let Some(i) = self.sessions.get(stored).copied() {
            self.sessions.insert(runtime.into(), i);
            let a = &mut self.agents[i];
            if let Some(previous) = a.session_id.replace(runtime.into())
                && previous != runtime
            {
                if let Some(n) = a.settled_sessions.remove(&previous) {
                    *a.settled_sessions.entry(runtime.into()).or_default() += n;
                }
                if let Some(n) = a.live_sessions.remove(&previous) {
                    a.live_sessions
                        .entry(runtime.into())
                        .and_modify(|old| *old = (*old).max(n))
                        .or_insert(n);
                }
            }
        }
    }
    pub fn apply(&mut self, line: &RecordLine) {
        self.apply_inner(line);
        self.recount();
    }
    pub fn apply_all(&mut self, lines: &[RecordLine]) {
        for line in lines {
            self.apply_inner(line);
        }
        self.recount();
    }
    fn phase(&mut self, path: &[String], title: &str) -> usize {
        let id = (path.to_vec(), title.to_string());
        if let Some(i) = self.phase_indices.get(&id) {
            return *i;
        }
        let i = self.phases.len();
        self.phase_indices.insert(id, i);
        self.phases.push(PhaseNode {
            title: title.into(),
            workflow_path: path.to_vec(),
            agents: vec![],
            counts: Counts::default(),
        });
        i
    }
    fn apply_inner(&mut self, line: &RecordLine) {
        if line.seq <= self.last_seq {
            return;
        }
        self.last_seq = line.seq;
        self.last_at_ms = Some(line.at_ms);
        // The journal owns the versioned record encoding, including opaque receipt values.
        let Ok(record) = serde_json::to_value(&line.record) else {
            return;
        };
        let kind = string(&record, "t");
        match kind.as_str() {
            "run_started" | "run_resumed" => {
                if self.run.state != RunState::Running {
                    self.run.previous_execution = Some(self.run.state);
                }
                self.run.state = RunState::Running;
                self.run.finished_at_ms = None;
                if self.run.started_at_ms.is_none() {
                    self.run.started_at_ms = Some(line.at_ms);
                }
                for a in &mut self.agents {
                    a.stopping = false;
                    a.controls_disabled = false;
                    a.derive();
                }
                return;
            }
            "run_finished" => {
                self.run.state = match string(&record, "outcome").as_str() {
                    "completed" => RunState::Completed,
                    "aborted" => RunState::Aborted,
                    _ => RunState::Failed,
                };
                self.run.finished_at_ms = Some(line.at_ms);
                return;
            }
            "engine" => {
                self.engine(&record["event"], line.at_ms);
                return;
            }
            _ => {}
        }
        let Some(key) = record["key"].as_str() else {
            return;
        };
        let key = key.to_string();
        let i = if let Some(i) = self.indices.get(&key) {
            *i
        } else {
            if !matches!(kind.as_str(), "call_intent" | "call_finished") {
                return;
            }
            let i = self.agents.len();
            self.indices.insert(key.clone(), i);
            self.agents.push(AgentNode::new(key.clone(), line.at_ms));
            if kind == "call_finished" {
                let phase = self.phase(&[], "Agents");
                self.phases[phase].agents.push(key.clone());
            }
            i
        };
        if kind == "call_intent" {
            let phase = record["phase"].as_str().map(str::to_owned);
            let path = strings(&record["workflow_path"]);
            let old = &self.agents[i];
            let old_phase = (
                old.workflow_path.clone(),
                old.phase.clone().unwrap_or_else(|| "Agents".into()),
            );
            let new_phase = (
                path.clone(),
                phase.clone().unwrap_or_else(|| "Agents".into()),
            );
            if old_phase != new_phase
                && let Some(index) = self.phase_indices.get(&old_phase)
            {
                self.phases[*index]
                    .agents
                    .retain(|existing| existing != &key);
            }
            let p = self.phase(&path, phase.as_deref().unwrap_or("Agents"));
            if !self.phases[p].agents.contains(&key) {
                self.phases[p].agents.push(key.clone());
            }
            self.last_intent_ms = Some(line.at_ms);
        }
        let a = &mut self.agents[i];
        a.controls_disabled = false;
        let attempt = record["attempt"].as_u64().unwrap_or(0) as u32;
        if attempt < a.attempt {
            return;
        }
        if attempt > a.attempt {
            a.session_id = None;
            a.stored_session_id = None;
            self.sessions.retain(|_, index| *index != i);
            a.attempt = attempt;
            a.turn = 0;
            a.last_outcome = None;
            a.attention = None;
            a.ended = false;
            a.stopping = false;
            a.stopped = false;
            a.live_active = false;
            a.admission_status = None;
            a.admission_id = None;
            a.execution_generation = None;
            a.finished_at_ms = None;
            a.started_at_ms = line.at_ms;
            a.prompts.clear();
            a.tail.clear();
            a.prompt = None;
            a.prompt_ref = None;
            a.last_tool = None;
            a.subagents = talaria_core::subagents::SubagentTree::new();
            a.restart_requested = false;
        }
        match kind.as_str() {
            "call_intent" => {
                a.label = string(&record, "label");
                a.phase = record["phase"].as_str().map(str::to_owned);
                a.workflow_path = strings(&record["workflow_path"]);
                a.call_id = serde_json::from_value(record["call_id"].clone()).ok();
                if let Some(call) = &a.call_id {
                    let id = format!("{}#{}", call.digest, call.occurrence);
                    if let Some((order, at_ms)) = self.starts.get(&id) {
                        a.order = *order;
                        a.started_at_ms = *at_ms;
                    }
                    self.calls.insert(id, i);
                }
                a.model = record["launch"]["model"].as_str().map(str::to_owned);
                a.provider = record["launch"]["provider"].as_str().map(str::to_owned);
                a.effort = record["launch"]["effort"].as_str().map(str::to_owned);
            }
            "session_bound" => {
                a.session_id = record["session_id"].as_str().map(str::to_owned);
                a.stored_session_id = record["stored_session_id"].as_str().map(str::to_owned);
                for s in [&a.session_id, &a.stored_session_id].into_iter().flatten() {
                    self.sessions.insert(s.clone(), i);
                }
            }
            "turn_intent" => {
                a.turn = record["turn"].as_u64().unwrap_or(0) as u32;
                a.prompt = record["text"].as_str().map(str::to_owned);
                a.prompt_ref = serde_json::from_value(record["text_ref"].clone()).ok();
                a.admission_status = None;
                a.admission_id = None;
                a.live_active = false;
            }
            "turn_admitted" => {
                a.admission_id = record["admission_id"].as_str().map(str::to_owned);
                a.admission_status = record["status"].as_str().map(str::to_owned);
                if a.admission_status.as_deref() == Some("started") {
                    // Queued calls already have an intent; admission is also proof of resume.
                    self.last_intent_ms = Some(line.at_ms);
                }
                hold_generation(a, record["execution_generation"].as_i64());
            }
            "turn_settled" => {
                let admission = string(&record, "admission_id");
                if a.settlements.insert(admission) {
                    if let Some(tokens) = settled_tokens(&record["usage"]) {
                        a.settled_tokens = a.settled_tokens.saturating_add(tokens);
                        if let Some(session) = &a.session_id {
                            *a.settled_sessions.entry(session.clone()).or_default() += tokens;
                        }
                    } else {
                        a.tokens_unknown = true;
                    }
                    a.money = settled_money(&record["usage"]);
                }
                a.stopped = record["outcome"].as_str() == Some("interrupted");
                if a.stopped {
                    a.last_outcome = Some("failed".into());
                    a.finished_at_ms = Some(line.at_ms);
                    a.live_active = false;
                    a.prompts.clear();
                }
                a.stopping = false;
                if let Some(text) = record["final_text"].as_str() {
                    a.push_tail(text);
                }
            }
            "call_attention" => {
                a.attention = Some(string(&record, "cause"));
                hold_generation(a, record["execution_generation"].as_i64());
            }
            "call_resolved" => {
                a.attention = None;
            }
            "call_finished" => {
                // Engine outcomes and durable outcomes both use a tagged kind.
                let outcome = record["outcome"]
                    .as_str()
                    .or_else(|| record["outcome"]["kind"].as_str())
                    .unwrap_or("failed");
                a.last_outcome = Some(outcome.into());
                a.finished_at_ms = Some(line.at_ms);
                a.stopping = false;
                if let Some(reason) = record["outcome"]["reason"].as_str() {
                    a.stopped |= matches!(reason, "interrupted" | "stopped" | "cancelled");
                    a.push_tail(reason);
                }
            }
            _ => {}
        }
        a.derive();
    }
    fn engine(&mut self, event: &Value, at_ms: i64) {
        let path = strings(&event["workflow_path"]);
        match string(event, "event").as_str() {
            "started" | "child_started" => {
                if event["event"] == "child_started"
                    && !self.children.iter().any(|c| c.workflow_path == path)
                {
                    self.children.push(ChildNode {
                        workflow_path: path.clone(),
                        name: string(&event["meta"], "name"),
                    });
                }
                if let Some(phases) = event["meta"]["phases"].as_array() {
                    for phase in phases {
                        self.phase(&path, &string(phase, "title"));
                    }
                }
            }
            "phase" => {
                self.phase(&path, &string(event, "title"));
            }
            "log" => {
                self.run.logs.push_back(string(event, "message"));
                while self.run.logs.len() > 50 {
                    self.run.logs.pop_front();
                }
            }
            "agent_started" => {
                let call = &event["request"]["call_id"];
                let id = format!(
                    "{}#{}",
                    string(call, "digest"),
                    call["occurrence"].as_u64().unwrap_or(0)
                );
                if let Some(start) = self.starts.get_mut(&id) {
                    start.1 = at_ms;
                } else {
                    self.starts.insert(id, (self.next_order, at_ms));
                    self.next_order += 1;
                }
            }
            "agent_finished" => {
                let id = format!(
                    "{}#{}",
                    string(&event["call_id"], "digest"),
                    event["call_id"]["occurrence"].as_u64().unwrap_or(0)
                );
                if let Some(i) = self.calls.get(&id) {
                    let a = &mut self.agents[*i];
                    if matches!(
                        event["outcome"]["kind"].as_str(),
                        Some("aborted" | "abandoned")
                    ) && a.last_outcome.is_none()
                    {
                        a.ended = true;
                        a.finished_at_ms = Some(at_ms);
                        a.derive();
                    }
                }
            }
            _ => {}
        }
    }
    pub fn recount(&mut self) {
        self.run.counts = Counts::default();
        self.run.tokens = 0;
        self.run.tokens_unknown = false;
        self.run.unpriced = 0;
        let mut money = 0.0;
        for a in &mut self.agents {
            a.derive();
            self.run.counts.add(a.state);
            self.run.tokens = self.run.tokens.saturating_add(a.settled_tokens);
            self.run.tokens_unknown |= a.tokens_unknown;
            if let Some(n) = a.money {
                money += n;
            } else {
                self.run.unpriced += 1;
            }
        }
        self.run.money = (self.run.unpriced < self.agents.len()).then_some(money);
        for phase in &mut self.phases {
            phase.agents.sort_by_key(|key| {
                self.indices
                    .get(key)
                    .map(|i| self.agents[*i].order)
                    .unwrap_or(u64::MAX)
            });
            phase.counts = Counts::default();
            for key in &phase.agents {
                if let Some(i) = self.indices.get(key) {
                    phase.counts.add(self.agents[*i].state);
                }
            }
        }
    }
}
pub(crate) fn hold_generation(a: &mut AgentNode, generation: Option<i64>) {
    if let Some(g) = generation {
        a.execution_generation = Some(a.execution_generation.map_or(g, |old| old.max(g)));
    }
}
fn string(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().into()
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
pub(crate) fn settled_tokens(v: &Value) -> Option<u64> {
    v["total_tokens"].as_u64().or_else(|| {
        Some(
            v["input_tokens"]
                .as_u64()?
                .saturating_add(v["output_tokens"].as_u64()?),
        )
    })
}
pub(crate) fn live_tokens(v: &Value) -> Option<u64> {
    v["total"]
        .as_u64()
        .or_else(|| {
            Some(
                v["prompt"]
                    .as_u64()?
                    .saturating_add(v["completion"].as_u64()?),
            )
        })
        .or_else(|| Some(v["input"].as_u64()?.saturating_add(v["output"].as_u64()?)))
}
fn settled_money(v: &Value) -> Option<f64> {
    let c = &v["session_cost"];
    if !matches!(
        c["cost_status"].as_str(),
        Some("actual" | "estimated" | "included")
    ) {
        return None;
    }
    c["estimated_cost_usd"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
}
