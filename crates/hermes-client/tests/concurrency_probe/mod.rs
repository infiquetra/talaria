//! Shared helpers for the gateway concurrency measurement.
//!
//! Behavior is left unimplemented. The offline tests name these functions and
//! fail until a later change fills them in.

#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

#[derive(Debug, PartialEq, Eq)]
pub struct Percentile {
    pub n: u64,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}

pub fn percentile(samples: &[u64]) -> Option<Percentile> {
    let _ = samples;
    todo!("gateway concurrency stub");
}

#[derive(Debug)]
pub struct ProcessTree {
    pub rss_kib: u64,
    pub cpu_seconds: f64,
    pub bad_lines: u32,
}

pub fn sum_process_tree(ps_text: &str, root_pid: u32) -> ProcessTree {
    let _ = (ps_text, root_pid);
    todo!("gateway concurrency stub");
}

#[derive(Debug, Clone, Copy)]
pub enum ProviderScript {
    Ready,
    Slow,
    Todo,
}

pub struct LoopbackProvider;

impl LoopbackProvider {
    pub async fn bind(script: ProviderScript) -> Self {
        let _ = script;
        todo!("gateway concurrency stub");
    }

    pub fn addr(&self) -> std::net::SocketAddr {
        todo!("gateway concurrency stub");
    }

    pub fn reset_peak(&self) {
        todo!("gateway concurrency stub");
    }

    pub fn turn_requests_peak(&self) -> u64 {
        todo!("gateway concurrency stub");
    }

    pub fn aux_requests(&self) -> u64 {
        todo!("gateway concurrency stub");
    }

    pub fn unexpected_routes(&self) -> u64 {
        todo!("gateway concurrency stub");
    }
}

pub fn create_params(
    series: char,
    run_id: &str,
    record_id: &str,
    session_index: u32,
    port: u16,
) -> Value {
    let _ = (series, run_id, record_id, session_index, port);
    todo!("gateway concurrency stub");
}

pub fn submit_params(
    session_id: &str,
    run_id: &str,
    record_id: &str,
    session_index: u32,
    repetition: u32,
) -> Value {
    let _ = (session_id, run_id, record_id, session_index, repetition);
    todo!("gateway concurrency stub");
}

pub fn descriptor_gate(described: &Value) -> Result<(), String> {
    let _ = described;
    todo!("gateway concurrency stub");
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EventStats {
    pub seq_repeats: u64,
    pub seq_decreases: u64,
    pub seq_gaps: u64,
    pub epoch_changes: u64,
    pub replay_gaps: u64,
    pub peak_events_per_s: u64,
}

pub struct EventTracker;

impl EventTracker {
    pub fn new() -> Self {
        Self
    }

    pub fn push(&mut self, session_id: &str, epoch: u64, seq: u64, event_type: &str, at_ms: u64) {
        let _ = (session_id, epoch, seq, event_type, at_ms);
        todo!("gateway concurrency stub");
    }

    pub fn stats(&self) -> EventStats {
        todo!("gateway concurrency stub");
    }
}

pub struct TurnClock;

impl TurnClock {
    pub fn new() -> Self {
        Self
    }

    pub fn submit_written(&mut self, session_id: &str, at_ms: u64) {
        let _ = (session_id, at_ms);
        todo!("gateway concurrency stub");
    }

    pub fn event(&mut self, session_id: &str, kind: &str, at_ms: u64) {
        let _ = (session_id, kind, at_ms);
        todo!("gateway concurrency stub");
    }

    pub fn submit_admitted(&mut self, session_id: &str, admission_id: &str, at_ms: u64) {
        let _ = (session_id, admission_id, at_ms);
        todo!("gateway concurrency stub");
    }

    pub fn duration_ms(&self, admission_id: &str, kind: &str) -> Option<u64> {
        let _ = (admission_id, kind);
        todo!("gateway concurrency stub");
    }
}

/// Wait until a turn completes. A turn that never completes returns `Err`
/// when `deadline` elapses and does not produce a sample.
pub async fn wait_for_turn(deadline: Duration) -> Result<u64, ()> {
    let _ = deadline;
    todo!("gateway concurrency stub");
}

pub fn healthy(record: &Value, baseline: Option<&Value>) -> (bool, Vec<u8>) {
    let _ = (record, baseline);
    todo!("gateway concurrency stub");
}

pub fn highest_healthy(records: &[Value]) -> Option<u32> {
    let _ = records;
    todo!("gateway concurrency stub");
}

#[derive(Debug)]
pub struct Recommendations {
    pub default_max_in_flight: Option<u32>,
    pub topology: Option<String>,
    pub memory_mb_per_session: Option<f64>,
    pub cited_runs: Option<Vec<String>>,
}

pub fn recommend(results: &Value) -> Recommendations {
    let _ = results;
    todo!("gateway concurrency stub");
}

pub struct RecordWriter;

impl RecordWriter {
    pub fn open(dir: &Path) -> Self {
        let _ = dir;
        todo!("gateway concurrency stub");
    }

    pub fn merge(&mut self, hermes_release: &str, record: &Value) -> Result<(), String> {
        let _ = (hermes_release, record);
        todo!("gateway concurrency stub");
    }

    pub fn records(&self) -> Vec<Value> {
        todo!("gateway concurrency stub");
    }
}

pub async fn run_series_a() -> Vec<Value> {
    todo!("gateway concurrency stub");
}

pub async fn run_series_b() -> Vec<Value> {
    todo!("gateway concurrency stub");
}

pub async fn run_series_c() -> Vec<Value> {
    todo!("gateway concurrency stub");
}

pub async fn run_series_d() -> Vec<Value> {
    todo!("gateway concurrency stub");
}

pub async fn run_series_e() -> Vec<Value> {
    todo!("gateway concurrency stub");
}

pub fn import_operator_lines(text: &str) -> Result<Vec<Value>, String> {
    let _ = text;
    todo!("gateway concurrency stub");
}
