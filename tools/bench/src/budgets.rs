//! The budgets file, statistics and verdicts.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::error::BenchError;

#[derive(Clone, Debug, Deserialize)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Launch {
    pub median_ms: f64,
    pub runs: u32,
    pub warmups: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Keystroke {
    pub p95_ms: f64,
    pub max_ms: f64,
    pub keys: usize,
    pub visible_within_ms: f64,
    pub interval_ms: f64,
    pub start_ms: f64,
    pub interval_ms_fast: f64,
    pub start_ms_fast: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Cpu {
    pub mean_pct: f64,
    pub peak_pct: f64,
    pub sample_ms: u64,
    pub window_samples: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorstFrame {
    pub max_ms: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Idle {
    pub seconds: u64,
    pub settle_seconds: u64,
    pub frames: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Memory {
    pub unit: String,
    pub start_max: f64,
    pub long_session_max: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LiveGateway {
    pub composer_ms: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnitChoice {
    pub bytes_median: f64,
    pub late_over_early: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Scenario {
    pub turn_seconds: u64,
    pub long_session_turns: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Budgets {
    pub terminal: TerminalSize,
    pub launch: Launch,
    pub keystroke: Keystroke,
    pub cpu: Cpu,
    pub worst_frame: WorstFrame,
    pub idle: Idle,
    pub memory: Memory,
    pub live_gateway: LiveGateway,
    pub unit_choice: UnitChoice,
    pub scenario: Scenario,
}

impl Budgets {
    pub fn load(path: &Path) -> Result<Budgets, BenchError> {
        let _ = path;
        todo!("bench")
    }
}

/// Key times from the end of the start frame. Speeds up to 1 use the slow pair.
pub fn keystroke_schedule(budgets: &Budgets, speed: f64) -> Vec<Duration> {
    let _ = (budgets, speed);
    todo!("bench")
}

/// Two turns of `turn_seconds`, divided by the speed.
pub fn replay_duration(budgets: &Budgets, speed: f64) -> Duration {
    let _ = (budgets, speed);
    todo!("bench")
}

/// The last record's `tMs`, 0 when there is none.
pub fn last_t_ms(fixture: &Path) -> Result<u64, BenchError> {
    let _ = fixture;
    todo!("bench")
}

pub fn median(samples: &[f64]) -> f64 {
    let _ = samples;
    todo!("bench")
}

/// `p` in 0 to 100.
pub fn percentile_nearest_rank(samples: &[f64], p: f64) -> f64 {
    let _ = (samples, p);
    todo!("bench")
}

pub fn window_means(samples: &[f64], window: usize) -> Vec<f64> {
    let _ = (samples, window);
    todo!("bench")
}

pub fn peak_window(samples: &[f64], window: usize) -> f64 {
    let _ = (samples, window);
    todo!("bench")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Info,
    Skipped,
}

/// Pass when measured is at most the limit.
pub fn verdict(measured: f64, limit: f64) -> Status {
    let _ = (measured, limit);
    todo!("bench")
}

/// Pass when any attempt is within the limit; also the lowest attempt.
pub fn best_of(attempts: &[f64], limit: f64) -> (Status, f64) {
    let _ = (attempts, limit);
    todo!("bench")
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub measured: String,
    pub limit: String,
    pub status: Status,
    pub attempts: Vec<String>,
}

impl Row {
    pub fn info(name: &str, measured: &str) -> Row {
        let _ = (name, measured);
        todo!("bench")
    }

    pub fn skipped(name: &str, limit: &str) -> Row {
        let _ = (name, limit);
        todo!("bench")
    }
}
