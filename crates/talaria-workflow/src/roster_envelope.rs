use crate::skill::SkillInspection;
use crate::staffing::RosterReport;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, serde::Serialize)]
pub struct WorkflowEntry {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
}
pub fn list_workflows(_dir: &Path) -> (Vec<WorkflowEntry>, Vec<String>) {
    todo!()
}
pub fn drafts_dir(_talaria_home: &Path) -> std::io::Result<PathBuf> {
    todo!()
}
#[derive(Clone, Copy, Debug)]
pub enum RosterErrorKind {
    HomeUnavailable,
    Filesystem,
    StaffingInvalid,
    GatewayUnavailable,
    ProfileUnserved,
    RosterFailed,
}
pub struct LocalParts {
    pub talaria_home: Option<PathBuf>,
    pub runs_dir: Option<PathBuf>,
    pub workflows_dir: Option<PathBuf>,
    pub drafts_dir: Option<PathBuf>,
    pub workflows: Vec<WorkflowEntry>,
    pub skill: Option<SkillInspection>,
    pub warnings: Vec<String>,
}
#[derive(serde::Serialize)]
pub struct RosterEnvelope {
    pub version: u32,
    pub profile: String,
    pub report: Option<RosterReport>,
    pub paths: serde_json::Value,
    pub workflows: Vec<WorkflowEntry>,
    pub skill: Option<serde_json::Value>,
    pub warnings: Vec<String>,
    pub error: Option<serde_json::Value>,
}
pub fn build_envelope(
    _profile: &str,
    _local: LocalParts,
    _report: Result<RosterReport, (RosterErrorKind, String)>,
) -> (RosterEnvelope, i32) {
    todo!()
}
