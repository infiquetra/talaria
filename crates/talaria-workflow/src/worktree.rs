use crate::{engine::AgentRequest, hermes::WorktreeProvider};
use std::path::{Path, PathBuf};

pub struct GitWorktrees;
pub struct WorktreeInfo {
    pub call: String,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
    pub dirty: bool,
}
impl GitWorktrees {
    pub fn new(_run_dir: PathBuf, _source_cwd: PathBuf, _run_id: String) -> Self {
        todo!()
    }
    pub fn list(&self) -> Vec<WorktreeInfo> {
        todo!()
    }
    pub fn remove(&self, _path: &Path, _force: bool) -> Result<(), String> {
        todo!()
    }
}
impl WorktreeProvider for GitWorktrees {
    fn worktree_for(&self, _request: &AgentRequest) -> Result<PathBuf, String> {
        todo!()
    }
}
