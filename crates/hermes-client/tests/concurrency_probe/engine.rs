//! Offline analysis and loopback transport for the gateway concurrency experiment.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub const PROMPT: &str = "Reply with the single word READY.";
pub const WIDTHS: [u32; 5] = [1, 5, 10, 16, 20];
pub const RELEASE: &str = hermes_contract::pinned::TAG;

pub fn percentile(samples: &[u64]) -> Option<Value> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    Some(
        json!({"n":n,"p50":sorted[n.div_ceil(2)-1],"p95":sorted[(95*n).div_ceil(100)-1],"max":sorted[n-1]}),
    )
}

pub fn create_params(run: &str, id: &str, n: u32, base_url: &str) -> Value {
    let mut out = json!({"request_id":format!("talaria-probe:{run}:{id}:{n}"),"source":"tool","hidden":true,
        "model":"talaria-probe-stub","provider":"custom","base_url":base_url,
        "api_key":"talaria-probe-placeholder","toolsets":[]});
    if id.starts_with('B') {
        out["model"] = json!("minimax-m3.1-flash");
        out["provider"] = json!("ollama-cloud");
        out.as_object_mut().unwrap().remove("base_url");
        out.as_object_mut().unwrap().remove("api_key");
    }
    if id.starts_with('C') {
        out["toolsets"] = json!(["todo"]);
    }
    out
}
pub fn submit_params(run: &str, id: &str, n: u32, rep: u32, session: &str) -> Value {
    json!({"session_id":session,"submission_id":format!("talaria-probe:{run}:{id}:{n}:{rep}"),"text":PROMPT,"finite":true})
}
pub fn descriptor_gate(descriptor: &Value) -> Result<(), String> {
    let create = &descriptor["session_create"];
    if !create["sources"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == "tool"))
    {
        return Err("descriptor lacks tool".into());
    }
    for name in [
        "request_id",
        "source",
        "model",
        "provider",
        "base_url",
        "api_key",
        "toolsets",
    ] {
        if !create["parameters"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v == name))
        {
            return Err(format!("descriptor lacks {name}"));
        }
    }
    let contract: Value = serde_json::from_str(include_str!(
        "../../../hermes-contract/vendor/gateway-contract.openrpc.json"
    ))
    .map_err(|_| "contract did not decode")?;
    if contract
        .pointer("/components/schemas/CanonicalCreateParams/properties/hidden")
        .is_none()
    {
        return Err("contract lacks hidden".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub rss_kib: u64,
    pub cpu_s: f64,
}
#[derive(Clone, Debug, Default)]
pub struct ProcessTree {
    pub processes: Vec<Process>,
    pub rss_mb: f64,
    pub cpu_s: f64,
    pub children: u64,
    pub bad_lines: u64,
}
pub fn parse_cpu_time(text: &str) -> Option<f64> {
    let (days, rest) = match text.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, text),
    };
    let parts: Vec<_> = rest.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut seconds = days * 86400.0;
    let mut elapsed = 0.0;
    for p in parts {
        elapsed = elapsed * 60.0 + p.parse::<f64>().ok()?;
    }
    seconds += elapsed;
    seconds.is_finite().then_some(seconds)
}
pub fn parse_ps(text: &str, root: u32) -> Option<ProcessTree> {
    let mut all = Vec::new();
    let mut bad_lines = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let p: Vec<_> = line.split_whitespace().collect();
        let parsed = (|| {
            if p.len() != 4 {
                return None;
            }
            Some(Process {
                pid: p[0].parse().ok()?,
                ppid: p[1].parse().ok()?,
                rss_kib: p[2].parse().ok()?,
                cpu_s: parse_cpu_time(p[3])?,
            })
        })();
        if let Some(p) = parsed {
            all.push(p);
        } else {
            bad_lines += 1;
        }
    }
    if !all.iter().any(|p| p.pid == root) {
        return None;
    }
    let mut ids = BTreeSet::from([root]);
    loop {
        let size = ids.len();
        for p in &all {
            if ids.contains(&p.ppid) {
                ids.insert(p.pid);
            }
        }
        if size == ids.len() {
            break;
        }
    }
    let processes: Vec<_> = all.into_iter().filter(|p| ids.contains(&p.pid)).collect();
    Some(ProcessTree {
        rss_mb: processes.iter().map(|p| p.rss_kib).sum::<u64>() as f64 / 1024.0,
        cpu_s: processes.iter().map(|p| p.cpu_s).sum(),
        children: processes.len().saturating_sub(1) as u64,
        processes,
        bad_lines,
    })
}

#[derive(Default)]
struct Sequence {
    last: u64,
    contiguous: u64,
    seen: BTreeSet<u64>,
    missing: BTreeSet<u64>,
    needs_replay: bool,
}
#[derive(Default)]
pub struct EventTracker {
    positions: BTreeMap<(String, String), Sequence>,
    epochs: BTreeMap<String, String>,
    arrivals: Vec<u64>,
    completions: BTreeMap<String, u64>,
    turns: BTreeMap<String, u64>,
    pub observed: u64,
    pub seq_repeats: u64,
    pub seq_decreases: u64,
    pub seq_gaps: u64,
    pub epoch_changes: u64,
    pub replay_gaps: u64,
}
impl EventTracker {
    pub fn observe(&mut self, frame: &Value, arrival_ms: u64) {
        let p = frame.get("params").unwrap_or(frame);
        let Some(session) = p["session_id"].as_str() else {
            return;
        };
        self.observed += 1;
        self.arrivals.push(arrival_ms);
        let epoch_value = p
            .get("replay_epoch")
            .or_else(|| p.pointer("/payload/replay_epoch"))
            .unwrap_or(&Value::Null);
        let epoch = epoch_value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| epoch_value.to_string());
        let changed = self.epochs.get(session).is_some_and(|old| old != &epoch);
        if changed {
            self.epoch_changes += 1;
        }
        self.epochs.insert(session.into(), epoch.clone());
        let seq = p["seq"].as_u64().unwrap_or(0);
        let state = self.positions.entry((session.into(), epoch)).or_default();
        if changed {
            state.needs_replay = true;
        }
        if p["type"] == "session.replay_gap" {
            self.replay_gaps += 1;
            state.needs_replay = true;
        }
        if seq > 0 {
            if state.last > 0 {
                if seq == state.last {
                    self.seq_repeats += 1;
                } else if seq < state.last {
                    self.seq_decreases += 1;
                } else if seq > state.last + 1 {
                    self.seq_gaps += 1;
                    state.missing.extend(state.last + 1..seq);
                    state.needs_replay = true;
                }
            }
            state.last = seq;
            state.seen.insert(seq);
            if state.contiguous == 0 {
                state.contiguous = seq;
            }
            while state.seen.contains(&(state.contiguous + 1)) {
                state.contiguous += 1;
            }
        }
        if p["type"] == "message.start" {
            *self.turns.entry(session.into()).or_default() += 1;
        }
        if p["type"] == "message.complete" {
            let admission = p["admission_id"]
                .as_str()
                .or_else(|| p.pointer("/payload/admission_id").and_then(Value::as_str))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{session}:{}", self.turns.get(session).unwrap_or(&0)));
            *self.completions.entry(admission).or_default() += 1;
        }
    }
    pub fn replay_requests(&self) -> Vec<Value> {
        self.positions.iter().filter(|(_,s)|s.needs_replay).map(|((session,epoch),s)|json!({"session_id":session,"last_sequence":s.contiguous,"replay_epoch":epoch})).collect()
    }
    pub fn resolve(&mut self, session: &str, epoch: &str, replay: &Value) {
        if let Some(state) = self.positions.get_mut(&(session.into(), epoch.into())) {
            if replay["truncated"] != false {
                return;
            }
            let seqs: BTreeSet<_> = replay["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.get("params").unwrap_or(v)["seq"].as_u64())
                .collect();
            state.missing.retain(|s| !seqs.contains(s));
            state.needs_replay = !state.missing.is_empty();
        }
    }
    pub fn position(&self, session: &str) -> (u64, String) {
        let epoch = self.epochs.get(session).cloned().unwrap_or_default();
        (
            self.positions
                .get(&(session.into(), epoch.clone()))
                .map_or(0, |s| s.last),
            epoch,
        )
    }
    pub fn report(&self) -> Value {
        let mut a = self.arrivals.clone();
        a.sort_unstable();
        let mut end = 0;
        let mut peak = 0;
        for start in 0..a.len() {
            while end < a.len() && a[end] < a[start].saturating_add(1000) {
                end += 1;
            }
            peak = peak.max(end - start);
        }
        json!({"observed":self.observed,"seq_repeats":self.seq_repeats,"seq_decreases":self.seq_decreases,"seq_gaps":self.seq_gaps,
            "epoch_changes":self.epoch_changes,"replay_gaps":self.replay_gaps,"unresolved_gaps":self.positions.values().filter(|s|s.needs_replay).count(),
            "peak_events_per_s":peak,"complete_per_admission":self.completions.values().filter(|n|**n==1).count()})
    }
}

#[derive(Default, Debug)]
pub struct TurnClock {
    starts: BTreeMap<String, u64>,
    admissions: BTreeMap<String, String>,
    pub start_delay_ms: Vec<u64>,
    pub first_delta_ms: Vec<u64>,
    pub turn_ms: Vec<u64>,
    seen_start: BTreeSet<String>,
    seen_delta: BTreeSet<String>,
    complete: BTreeSet<String>,
}
impl TurnClock {
    pub fn sent(&mut self, session: &str, at_ms: u64) {
        self.starts.insert(session.into(), at_ms);
        self.seen_start.remove(session);
        self.seen_delta.remove(session);
        self.complete.remove(session);
        self.admissions.remove(session);
    }
    pub fn admitted(&mut self, session: &str, admission: &str) {
        self.admissions.insert(session.into(), admission.into());
    }
    pub fn event(&mut self, p: &Value, at_ms: u64) {
        let p = p.get("params").unwrap_or(p);
        let Some(session) = p["session_id"].as_str() else {
            return;
        };
        if let Some(id) = p["admission_id"].as_str()
            && self.admissions.get(session).is_some_and(|a| a != id)
        {
            return;
        }
        let Some(start) = self.starts.get(session) else {
            return;
        };
        let elapsed = at_ms.saturating_sub(*start);
        match p["type"].as_str().unwrap_or("") {
            "message.start" if self.seen_start.insert(session.into()) => {
                self.start_delay_ms.push(elapsed)
            }
            "message.delta" if self.seen_delta.insert(session.into()) => {
                self.first_delta_ms.push(elapsed)
            }
            "message.complete" if self.complete.insert(session.into()) => {
                self.turn_ms.push(elapsed)
            }
            _ => {}
        }
    }
    pub fn completed(&self, session: &str) -> bool {
        self.complete.contains(session)
    }
    pub fn started(&self, session: &str) -> bool {
        self.seen_start.contains(session)
    }
}
pub async fn wait_for_turn(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>,
    session: &str,
    deadline: Duration,
) -> Result<Value, &'static str> {
    tokio::time::timeout(deadline, async {
        while let Some(v) = rx.recv().await {
            let p = v.get("params").unwrap_or(&v);
            if p["session_id"] == session && p["type"] == "message.complete" {
                return Ok(v);
            }
        }
        Err("connection closed")
    })
    .await
    .map_err(|_| "timeout")?
}

#[path = "report.rs"]
mod report;
pub use report::*;

#[path = "provider.rs"]
mod provider;
pub use provider::{LoopbackProvider, ProviderScript};

#[cfg(test)]
#[path = "offline.rs"]
mod offline;
