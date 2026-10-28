use crate::journal::OnUnknown;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

pub struct RunRequest {
    pub script: PathBuf,
    pub args: Value,
    pub profile: String,
    pub staffing: Option<PathBuf>,
    pub cwd: PathBuf,
    pub budget_tokens: Option<u64>,
    pub budget_usd: Option<f64>,
    pub on_unknown: OnUnknown,
    pub runs_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub talaria_version: String,
    pub json: bool,
    pub record: Option<PathBuf>,
    pub settle_bound: Duration,
    pub receipt_poll: Duration,
}

pub struct ResumeRequest {
    pub target: String,
    pub script: Option<PathBuf>,
    pub staffing: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub budget_tokens: Option<u64>,
    pub budget_usd: Option<f64>,
    pub record: Option<PathBuf>,
    pub settle_bound: Duration,
    pub receipt_poll: Duration,
    pub runs_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub json: bool,
}

#[derive(Debug)]
pub enum RunnerEvent {
    Diagnostic {
        text: String,
    },
    RunStarted {
        run_id: String,
        run_dir: PathBuf,
    },
    Phase {
        title: String,
    },
    Log {
        message: String,
    },
    AgentStarted {
        label: String,
        model: String,
        provider: String,
    },
    AgentFinished {
        label: String,
        state: String,
    },
    Scheduler(Value),
    Parked {
        run_id: String,
        label: String,
        key: String,
        attempt: u32,
        episode: u32,
    },
    Finished {
        run_id: String,
        outcome: String,
        result: Option<Value>,
        spend: SpendView,
    },
}

#[derive(Debug)]
pub struct SpendView {
    pub tokens: u64,
    pub money_usd: Option<f64>,
    pub money_source: String,
}

#[derive(Debug)]
pub struct RunExit {
    pub code: i32,
    pub run_id: Option<String>,
    pub problems: Vec<String>,
}

pub async fn run_new(_req: RunRequest, _out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    unimplemented!()
}
pub async fn run_resume(_req: ResumeRequest, _out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    unimplemented!()
}
pub async fn run_child(_run_dir: PathBuf, _out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    unimplemented!()
}

#[cfg(feature = "mock")]
#[doc(hidden)]
pub struct RunnerServices {
    pub wires: std::sync::Arc<dyn crate::hermes::WireFactory>,
    pub rosters: crate::staffing::RosterCache,
    pub clock: std::sync::Arc<dyn crate::staffing::Clock>,
    pub port_config: crate::hermes::PortConfig,
}

#[cfg(feature = "mock")]
impl RunnerServices {
    pub fn new(
        _wires: std::sync::Arc<dyn crate::hermes::WireFactory>,
        _source: std::sync::Arc<dyn crate::staffing::RosterSource>,
        _clock: std::sync::Arc<dyn crate::staffing::Clock>,
        _port_config: crate::hermes::PortConfig,
    ) -> Self {
        unimplemented!()
    }
}

#[cfg(feature = "mock")]
#[doc(hidden)]
pub async fn run_new_with_services(
    _req: RunRequest,
    _services: RunnerServices,
    _out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    unimplemented!()
}

#[cfg(feature = "mock")]
#[doc(hidden)]
pub async fn run_resume_with_services(
    _req: ResumeRequest,
    _services: RunnerServices,
    _out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    unimplemented!()
}
