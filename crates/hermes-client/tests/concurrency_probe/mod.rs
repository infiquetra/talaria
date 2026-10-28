//! Public measurement helpers shared by the independent offline and live probes.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod driver;
mod engine;
use crate::support;
pub use engine::ProviderScript;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Percentile {
    pub n: u64,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}
pub fn percentile(samples: &[u64]) -> Option<Percentile> {
    engine::percentile(samples).map(|v| serde_json::from_value(v).expect("percentile shape"))
}

#[derive(Debug)]
pub struct ProcessTree {
    pub rss_kib: u64,
    pub cpu_seconds: f64,
    pub bad_lines: u32,
}
pub fn sum_process_tree(ps_text: &str, root_pid: u32) -> ProcessTree {
    let tree = engine::parse_ps(ps_text, root_pid).unwrap_or_default();
    ProcessTree {
        rss_kib: tree.processes.iter().map(|p| p.rss_kib).sum(),
        cpu_seconds: tree.cpu_s,
        bad_lines: u32::try_from(tree.bad_lines).unwrap_or(u32::MAX),
    }
}

pub struct LoopbackProvider {
    inner: engine::LoopbackProvider,
}
impl LoopbackProvider {
    pub async fn bind(script: ProviderScript) -> Self {
        Self {
            inner: engine::LoopbackProvider::start(script)
                .await
                .expect("loopback provider bind"),
        }
    }
    pub fn addr(&self) -> std::net::SocketAddr {
        self.inner
            .base_url()
            .strip_prefix("http://")
            .and_then(|s| s.strip_suffix("/v1"))
            .expect("loopback authority")
            .parse()
            .expect("loopback address")
    }
    pub fn reset_peak(&self) {
        self.inner.reset_peak_sync();
    }
    pub fn turn_requests_peak(&self) -> u64 {
        self.inner.stats_sync().turn_requests_peak_min
    }
    pub fn aux_requests(&self) -> u64 {
        self.inner.stats_sync().aux_requests
    }
    pub fn unexpected_routes(&self) -> u64 {
        self.inner.stats_sync().unexpected_routes
    }
}

pub fn create_params(
    series: char,
    run_id: &str,
    record_id: &str,
    session_index: u32,
    port: u16,
) -> Value {
    assert_eq!(record_id.chars().next(), Some(series), "series identity");
    engine::create_params(
        run_id,
        record_id,
        session_index,
        &format!("http://127.0.0.1:{port}/v1"),
    )
}
pub fn submit_params(
    session_id: &str,
    run_id: &str,
    record_id: &str,
    session_index: u32,
    repetition: u32,
) -> Value {
    engine::submit_params(run_id, record_id, session_index, repetition, session_id)
}
pub fn descriptor_gate(described: &Value) -> Result<(), String> {
    engine::descriptor_gate(described)
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
#[derive(Default)]
pub struct EventTracker {
    inner: engine::EventTracker,
}
impl EventTracker {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, session_id: &str, epoch: u64, seq: u64, event_type: &str, at_ms: u64) {
        self.inner.observe(
            &json!({"session_id":session_id,"replay_epoch":epoch,"seq":seq,"type":event_type}),
            at_ms,
        );
    }
    pub fn stats(&self) -> EventStats {
        let report = self.inner.report();
        EventStats {
            seq_repeats: self.inner.seq_repeats,
            seq_decreases: self.inner.seq_decreases,
            seq_gaps: self.inner.seq_gaps,
            epoch_changes: self.inner.epoch_changes,
            replay_gaps: self.inner.replay_gaps,
            peak_events_per_s: report["peak_events_per_s"].as_u64().unwrap(),
        }
    }
}

#[derive(Default)]
pub struct TurnClock {
    starts: BTreeMap<String, u64>,
    admissions: BTreeMap<String, String>,
    durations: BTreeMap<(String, String), u64>,
}
impl TurnClock {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn submit_written(&mut self, session_id: &str, at_ms: u64) {
        self.starts.insert(session_id.into(), at_ms);
        self.durations
            .retain(|(session, _), _| session != session_id);
    }
    pub fn event(&mut self, session_id: &str, kind: &str, at_ms: u64) {
        if let Some(start) = self.starts.get(session_id) {
            self.durations
                .entry((session_id.into(), kind.into()))
                .or_insert_with(|| at_ms.saturating_sub(*start));
        }
    }
    pub fn submit_admitted(&mut self, session_id: &str, admission_id: &str, _at_ms: u64) {
        self.admissions
            .insert(admission_id.into(), session_id.into());
    }
    pub fn duration_ms(&self, admission_id: &str, kind: &str) -> Option<u64> {
        let session = self.admissions.get(admission_id)?;
        self.durations.get(&(session.clone(), kind.into())).copied()
    }
}
/// A wait without a completion source expires without producing a timing sample.
pub async fn wait_for_turn(deadline: Duration) -> Result<u64, ()> {
    tokio::time::sleep(deadline).await;
    Err(())
}
pub fn healthy(record: &Value, baseline: Option<&Value>) -> (bool, Vec<u8>) {
    engine::healthy(record, baseline.unwrap_or(&Value::Null))
}
pub fn highest_healthy(records: &[Value]) -> Option<u32> {
    engine::highest_healthy(records)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Recommendations {
    pub default_max_in_flight: Option<u32>,
    pub topology: Option<String>,
    pub memory_mb_per_session: Option<f64>,
    pub cited_runs: Option<Vec<String>>,
}
pub fn recommend(results: &Value) -> Recommendations {
    serde_json::from_value(engine::recommend(results)).expect("recommendations shape")
}

pub struct RecordWriter {
    path: PathBuf,
}
impl RecordWriter {
    pub fn open(dir: &Path) -> Self {
        Self {
            path: dir.join("results.json"),
        }
    }
    /// Store records after their caller has performed schema validation.
    pub fn merge(&mut self, hermes_release: &str, record: &Value) -> Result<(), String> {
        engine::merge_raw_record(&self.path, hermes_release, "unknown", record).map(|_| ())
    }
    pub fn records(&self) -> Vec<Value> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|v| v["runs"].as_array().cloned())
            .unwrap_or_default()
    }
}

pub async fn run_series_a() -> Vec<Value> {
    driver::series("A").await
}
pub async fn run_series_b() -> Vec<Value> {
    driver::series("B").await
}
pub async fn run_series_c() -> Vec<Value> {
    driver::series("C").await
}
pub async fn run_series_d() -> Vec<Value> {
    driver::series("D").await
}
pub async fn run_series_e() -> Vec<Value> {
    driver::series("E").await
}
pub fn import_operator_lines(text: &str) -> Result<Vec<Value>, String> {
    let records: Vec<Value> = text
        .lines()
        .filter_map(|line| line.strip_prefix("RECORD "))
        .map(|line| serde_json::from_str(line).map_err(|_| "record decode".to_owned()))
        .collect::<Result<_, _>>()?;
    if records.is_empty() {
        return Err("no operator records".into());
    }
    for record in &records {
        engine::validate_record(record)?;
        if record["series"] != "B" {
            return Err("operator series".into());
        }
    }
    for record in &records {
        engine::merge_record(
            &driver::results_path(),
            engine::RELEASE,
            &driver::execution_mode(),
            record,
        )?;
    }
    Ok(records)
}
