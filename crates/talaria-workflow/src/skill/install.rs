use super::{SkillError, SkillInspection, SkillTarget};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
pub struct InstallRecords {
    pub version: u32,
    pub installs: Vec<InstallEntry>,
}
impl InstallRecords {
    pub fn load(_talaria_home: &Path) -> Result<Self, SkillError> {
        todo!()
    }
    pub fn save(&self, _talaria_home: &Path) -> Result<(), SkillError> {
        todo!()
    }
    pub fn entry(&self, _target: &Path) -> Option<&InstallEntry> {
        todo!()
    }
}
pub struct InstallEntry {
    pub target: PathBuf,
    pub profile: String,
    pub hermes_root: PathBuf,
    pub skill_version: String,
    pub command: String,
    pub files: BTreeMap<String, String>,
    pub installed_at: String,
}
pub struct InstallOptions {
    pub command: String,
    pub force: bool,
    pub dry_run: bool,
    pub talaria_home: PathBuf,
    pub now: chrono::DateTime<chrono::Utc>,
    pub lock_wait: Duration,
}
#[derive(Debug, PartialEq, Eq)]
pub enum InstallAction {
    Write,
    Replace,
    None,
    Refuse,
    BackupAndReplace,
    Remove,
    BackupAndRemove,
}
pub struct InstallReport {
    pub before: SkillInspection,
    pub action: InstallAction,
    pub backup: Option<PathBuf>,
    pub warnings: Vec<String>,
}
pub fn install(_target: &SkillTarget, _opts: &InstallOptions) -> Result<InstallReport, SkillError> {
    todo!()
}
pub fn uninstall(
    _target: &SkillTarget,
    _opts: &InstallOptions,
) -> Result<InstallReport, SkillError> {
    todo!()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStage {
    AfterInspection,
    AfterPublish,
    AfterRecord,
}
pub fn install_with_hook(
    _target: &SkillTarget,
    _opts: &InstallOptions,
    _hook: &mut dyn FnMut(InstallStage),
) -> Result<InstallReport, SkillError> {
    todo!()
}
pub fn recover(_target: &SkillTarget, _opts: &InstallOptions) -> Result<Vec<String>, SkillError> {
    todo!()
}
