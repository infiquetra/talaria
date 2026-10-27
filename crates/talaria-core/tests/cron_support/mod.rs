#![allow(dead_code)]

use serde_json::Value;
use std::time::Instant;
use talaria_core::cron::{CronEffect, FireError, JobRow};

/// `2026-10-23T09:00:00Z`.
pub const WALL: i64 = 1_792_746_000;

pub fn clock() -> Instant {
    Instant::now()
}

pub fn list_seq(effects: &[CronEffect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            CronEffect::List { seq } => Some(*seq),
            _ => None,
        })
        .expect("a list request")
}

pub fn runs_seq(effects: &[CronEffect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            CronEffect::Runs { seq, .. } => Some(*seq),
            _ => None,
        })
        .expect("a runs request")
}

pub fn job(id: &str, name: &str, state: &str) -> JobRow {
    JobRow {
        job_id: id.into(),
        name: name.into(),
        schedule: "0 9 * * *".into(),
        repeat: "forever".into(),
        deliver: "local".into(),
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        last_delivery_error: None,
        last_fire_error: None,
        enabled: state != "paused" && state != "completed",
        state: state.into(),
        paused_at: None,
        paused_reason: None,
        prompt_preview: String::new(),
        skills: Vec::new(),
        model: None,
        provider: None,
        pinned: false,
        workdir: None,
        script: false,
        no_agent: false,
        continuity: false,
    }
}

pub fn jobs_from_fixture(text: &str) -> (Vec<JobRow>, Option<String>) {
    let value: Value = serde_json::from_str(text).expect("list fixture");
    let warning = value
        .get("warning")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let jobs = value["jobs"]
        .as_array()
        .expect("jobs")
        .iter()
        .map(job_from_value)
        .collect();
    (jobs, warning)
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Number(number)) => number.as_i64().is_some_and(|n| n != 0),
        _ => false,
    }
}

fn job_from_value(value: &Value) -> JobRow {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let fire = value
        .get("last_fire_error")
        .and_then(hermes_contract::overrides::last_fire_error_parts)
        .map(|(at, detail)| FireError { at, detail });
    JobRow {
        job_id: text("job_id").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        schedule: text("schedule").unwrap_or_default(),
        repeat: value
            .get("repeat")
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "forever".into()),
        deliver: text("deliver").unwrap_or_else(|| "local".into()),
        next_run_at: text("next_run_at"),
        last_run_at: text("last_run_at"),
        last_status: text("last_status"),
        last_error: text("last_error"),
        last_delivery_error: text("last_delivery_error"),
        last_fire_error: fire,
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        state: text("state").unwrap_or_default(),
        paused_at: text("paused_at"),
        paused_reason: text("paused_reason"),
        prompt_preview: text("prompt_preview").unwrap_or_default(),
        skills: value
            .get("skills")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        model: text("model"),
        provider: text("provider"),
        pinned: value
            .get("pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        workdir: text("workdir"),
        script: truthy(value.get("script")),
        no_agent: truthy(value.get("no_agent")),
        continuity: truthy(value.get("continuity")),
    }
}
