//! The measurements and the run.

use std::path::PathBuf;

use crate::budgets::Budgets;
use crate::error::BenchError;
use crate::report::Report;

pub struct RunOptions {
    pub binary: PathBuf,
    pub fixtures: PathBuf,
    pub budgets: Budgets,
    pub report: Option<PathBuf>,
    pub attempts: u32,
    pub idle_seconds: Option<u32>,
    pub report_grace_seconds: u32,
    pub binary_args: Vec<String>,
    pub extra_fixtures: Vec<PathBuf>,
    pub scenario_dir: Option<PathBuf>,
    pub only: Vec<String>,
}

impl RunOptions {
    pub fn new(binary: PathBuf, budgets: Budgets) -> RunOptions {
        RunOptions {
            binary,
            fixtures: PathBuf::from("fixtures/frames"),
            budgets,
            report: None,
            attempts: 3,
            idle_seconds: None,
            report_grace_seconds: 10,
            binary_args: Vec::new(),
            extra_fixtures: Vec::new(),
            scenario_dir: None,
            only: Vec::new(),
        }
    }
}

pub fn run(options: &RunOptions) -> Result<Report, BenchError> {
    let _ = options;
    todo!("bench")
}

/// 0 when no row failed, else 1.
pub fn exit_code(report: &Report) -> u8 {
    let _ = report;
    todo!("bench")
}
