use std::path::{Path, PathBuf};

pub struct Role {
    pub role: &'static str,
    pub default_tiers: &'static [&'static str],
    pub list: bool,
    pub toolsets: &'static [&'static str],
    pub worktree: bool,
}
pub struct Builtin {
    pub name: &'static str,
    pub source: &'static str,
    pub summary: &'static str,
    pub roles: &'static [Role],
    pub max_agents_default: u32,
}
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        name: "fix-failing-tests",
        source: include_str!("../workflows/fix-failing-tests.js"),
        summary: "",
        roles: &[],
        max_agents_default: 0,
    },
    Builtin {
        name: "research",
        source: include_str!("../workflows/research.js"),
        summary: "",
        roles: &[],
        max_agents_default: 0,
    },
    Builtin {
        name: "review-branch",
        source: include_str!("../workflows/review-branch.js"),
        summary: "",
        roles: &[],
        max_agents_default: 0,
    },
    Builtin {
        name: "ultracode",
        source: include_str!("../workflows/ultracode.js"),
        summary: "",
        roles: &[],
        max_agents_default: 0,
    },
];
pub fn get(_name: &str) -> Option<&'static Builtin> {
    todo!("P5-08")
}
pub fn tiers_for(
    _builtin: &Builtin,
    _args: Option<&serde_json::Value>,
) -> Result<Vec<(String, String)>, String> {
    todo!("P5-08")
}
pub enum InstallMode {
    MissingOnly,
    Overwrite,
}
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallAction {
    Created,
    Unchanged,
    KeptModified,
    Overwritten,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallReport {
    pub name: String,
    pub path: PathBuf,
    pub action: InstallAction,
    pub backup: Option<PathBuf>,
}
pub fn install(_dir: &Path, _mode: InstallMode) -> std::io::Result<Vec<InstallReport>> {
    todo!("P5-08")
}
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstalledState {
    NotInstalled,
    Installed,
    Modified,
}
pub fn installed_state(_dir: &Path, _builtin: &Builtin) -> InstalledState {
    todo!("P5-08")
}
#[derive(Debug, PartialEq, Eq)]
pub enum ScriptArg {
    Path(PathBuf),
    Named {
        name: String,
        path: PathBuf,
        builtin: bool,
    },
    MissingBuiltin {
        name: String,
        path: PathBuf,
    },
}
pub fn resolve_script_arg(_arg: &str, _workflows_dir: &Path) -> Result<ScriptArg, String> {
    todo!("P5-08")
}
