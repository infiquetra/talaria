use std::path::{Path, PathBuf};

pub fn runs_dir(_explicit: Option<&Path>) -> PathBuf {
    unimplemented!()
}
pub fn workflows_dir(_explicit: Option<&Path>) -> PathBuf {
    unimplemented!()
}
pub fn discover_runs(_runs_dir: &Path) -> Vec<RunSummary> {
    unimplemented!()
}
pub fn find_run(_runs_dir: &Path, _id_or_path: &str) -> Result<PathBuf, FindError> {
    unimplemented!()
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub name: String,
    pub profile: String,
    pub started_at_ms: i64,
    pub status: RunStatus,
    pub agents_finished: u32,
    pub agents_total_known: u32,
    pub parked: u32,
    pub tokens_spent: Option<u64>,
    pub money_spent_usd: Option<f64>,
    pub run_dir: String,
    pub warning: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Parked,
    Completed,
    Failed,
    Aborted,
    Interrupted,
    Unreadable,
}

#[derive(Debug)]
pub enum FindError {
    UnknownId { id: String, searched: PathBuf },
    NotAFolder { path: PathBuf },
    Io(std::io::Error),
}
