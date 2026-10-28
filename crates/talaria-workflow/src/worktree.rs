//! Per-call Git worktrees retained until their owner explicitly removes them.

mod git;
mod provenance;

pub use git::{GitWorktrees, WorktreeInfo};

use crate::{engine::AgentRequest, hermes::WorktreeProvider};
use std::path::PathBuf;

impl WorktreeProvider for GitWorktrees {
    fn worktree_for(&self, request: &AgentRequest) -> Result<PathBuf, String> {
        self.for_call(&request.call_id.digest, request.call_id.occurrence)
    }
}
