use std::path::{Path, PathBuf};
pub mod install;
pub use install::*;
pub const SKILL_NAME: &str = "talaria-workflows";
pub const SKILL_VERSION: &str = "1.0.0";
pub const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../skill/talaria-workflows/SKILL.md"),
    ),
    (
        "references/commands.md",
        include_str!("../../skill/talaria-workflows/references/commands.md"),
    ),
    (
        "references/examples.md",
        include_str!("../../skill/talaria-workflows/references/examples.md"),
    ),
    (
        "references/script-api.md",
        include_str!("../../skill/talaria-workflows/references/script-api.md"),
    ),
];
#[derive(Debug)]
pub struct RenderedFile {
    pub path: &'static str,
    pub bytes: Vec<u8>,
}
pub fn render(
    _profile: &str,
    _command: &str,
    _hermes_root: &Path,
) -> Result<Vec<RenderedFile>, SkillError> {
    todo!()
}
pub fn normalise_profile(_raw: &str) -> Result<String, SkillError> {
    todo!()
}
pub fn hermes_root_of(_home: &Path, _native_home: &Path) -> PathBuf {
    todo!()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    Current,
    Outdated,
    Modified,
    Foreign,
}
pub struct SkillTarget {
    pub profile: String,
    pub home: PathBuf,
    pub hermes_root: PathBuf,
    pub dir: PathBuf,
    pub via_gateway: bool,
}
impl SkillTarget {
    pub fn resolve_local(_profile: &str, _hermes_home: &Path) -> Result<Self, SkillError> {
        todo!()
    }
    pub fn from_profile_row(_profile: &str, _row: &serde_json::Value) -> Result<Self, SkillError> {
        todo!()
    }
}
#[derive(Debug)]
pub struct SkillInspection {
    pub state: SkillState,
    pub installed_version: Option<String>,
    pub changed_files: Vec<String>,
}
pub fn inspect(
    _target: &SkillTarget,
    _records: &InstallRecords,
    _rendered: &[RenderedFile],
) -> std::io::Result<SkillInspection> {
    todo!()
}
#[derive(Debug)]
pub enum SkillError {
    InvalidProfile(String),
    InvalidCommand(String),
    InvalidRoot(PathBuf),
    ProfileNotFound {
        profile: String,
        home: PathBuf,
    },
    GatewayMismatch {
        served: String,
        wanted: String,
    },
    LockTimeout {
        profile: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Record {
        path: PathBuf,
        message: String,
    },
    Template(String),
}
impl SkillError {
    pub fn exit_code(&self) -> i32 {
        todo!()
    }
}
