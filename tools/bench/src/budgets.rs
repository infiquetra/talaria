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
        let text = std::fs::read_to_string(path)
            .map_err(|_| BenchError::BudgetsInvalid("cannot read budgets file".into()))?;
        let budgets: Self = toml::from_str(&text)
            .map_err(|_| BenchError::BudgetsInvalid("invalid budgets file".into()))?;
        if budgets.terminal.cols == 0
            || budgets.terminal.rows == 0
            || budgets.launch.runs == 0
            || budgets.cpu.sample_ms == 0
            || budgets.cpu.window_samples == 0
            || budgets.keystroke.keys == 0
            || budgets.keystroke.keys > 40
            || budgets.memory.unit != "MB"
            || budgets.scenario.turn_seconds == 0
        {
            return Err(BenchError::BudgetsInvalid(
                "invalid budget dimensions or units".into(),
            ));
        }
        for value in [
            budgets.launch.median_ms,
            budgets.keystroke.p95_ms,
            budgets.keystroke.max_ms,
            budgets.keystroke.visible_within_ms,
            budgets.keystroke.interval_ms,
            budgets.keystroke.start_ms,
            budgets.keystroke.interval_ms_fast,
            budgets.keystroke.start_ms_fast,
            budgets.cpu.mean_pct,
            budgets.cpu.peak_pct,
            budgets.worst_frame.max_ms,
            budgets.memory.start_max,
            budgets.memory.long_session_max,
            budgets.unit_choice.bytes_median,
            budgets.unit_choice.late_over_early,
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(BenchError::BudgetsInvalid(
                    "budgets must be finite and nonnegative".into(),
                ));
            }
        }
        Ok(budgets)
    }
}

/// Key times from the end of the start frame. Speeds up to 1 use the slow pair.
pub fn keystroke_schedule(budgets: &Budgets, speed: f64) -> Vec<Duration> {
    let k = &budgets.keystroke;
    let (start, step) = if speed <= 1.0 {
        (k.start_ms, k.interval_ms)
    } else {
        (k.start_ms_fast, k.interval_ms_fast)
    };
    (0..k.keys)
        .map(|i| Duration::from_secs_f64((start + step * i as f64) / 1000.0))
        .collect()
}

/// Two turns of `turn_seconds`, divided by the speed.
pub fn replay_duration(budgets: &Budgets, speed: f64) -> Duration {
    Duration::from_secs_f64(budgets.scenario.turn_seconds as f64 * 2.0 / speed)
}

/// The last record's `tMs`, 0 when there is none.
pub fn last_t_ms(fixture: &Path) -> Result<u64, BenchError> {
    let text = std::fs::read_to_string(fixture)
        .map_err(|_| BenchError::FixturesMissing("scenario".into()))?;
    let mut last = 0;
    for line in text.lines().skip(1) {
        let record: serde_json::Value = serde_json::from_str(line)
            .map_err(|_| BenchError::FixturesMissing("invalid scenario".into()))?;
        last = record["tMs"].as_u64().unwrap_or(last);
    }
    Ok(last)
}

pub fn median(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

/// `p` in 0 to 100.
pub fn percentile_nearest_rank(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (p.clamp(0.0, 100.0) / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

pub fn window_means(samples: &[f64], window: usize) -> Vec<f64> {
    if window == 0 {
        return Vec::new();
    }
    samples
        .windows(window)
        .map(|values| values.iter().sum::<f64>() / window as f64)
        .collect()
}

pub fn peak_window(samples: &[f64], window: usize) -> f64 {
    window_means(samples, window)
        .into_iter()
        .reduce(f64::max)
        .unwrap_or(0.0)
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
    if measured.is_finite() && measured <= limit {
        Status::Pass
    } else {
        Status::Fail
    }
}

/// Pass when any attempt is within the limit; also the lowest attempt.
pub fn best_of(attempts: &[f64], limit: f64) -> (Status, f64) {
    let best = attempts
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .reduce(f64::min)
        .unwrap_or(f64::NAN);
    (verdict(best, limit), best)
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
        Row {
            name: name.into(),
            measured: measured.into(),
            limit: "-".into(),
            status: Status::Info,
            attempts: Vec::new(),
        }
    }

    pub fn skipped(name: &str, limit: &str) -> Row {
        Row {
            name: name.into(),
            measured: "not measured".into(),
            limit: limit.into(),
            status: Status::Skipped,
            attempts: Vec::new(),
        }
    }
}
