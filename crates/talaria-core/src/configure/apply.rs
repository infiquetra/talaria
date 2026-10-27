use super::{Context, MainModelRow, Row};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub use crate::capabilities::CapabilityChange;
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyStep {
    Settings {
        rows: Vec<(usize, String)>,
        changes: BTreeMap<String, Value>,
        baseline: Value,
    },
    MainModel {
        row: usize,
        provider: String,
        model: String,
        base_url: String,
        baseline: MainModelRow,
        confirmed: bool,
    },
    McpAdd {
        row: usize,
        name: String,
        preset: Option<String>,
        config: Option<Map<String, Value>>,
    },
    McpEnable {
        row: usize,
        name: String,
        enabled: bool,
    },
    McpRemove {
        row: usize,
        name: String,
    },
    Capability {
        row: usize,
        change: CapabilityChange,
    },
    CronAdd {
        row: usize,
        name: String,
        schedule: String,
        prompt: String,
        repeat: Option<u32>,
        deliver: String,
    },
    CronControl {
        row: usize,
        job_id: String,
        action: CronControl,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CronControl {
    Pause,
    Resume,
    Remove,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowOutcome {
    Applied,
    Adjusted(String),
    NoChange(String),
    NotApplied(String),
    Conflict(String),
    Failed(String),
    Unknown(String),
    NotAttempted,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepResult {
    Rows(Vec<(usize, RowOutcome)>),
    NeedsConfirm { row: usize, message: String },
}
pub fn plan_apply(_rows: &[Row], _selected: &[bool], _ctx: &Context) -> Vec<ApplyStep> {
    todo!("configure stub")
}
