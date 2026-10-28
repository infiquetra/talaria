//! Complete an explicit launch before any transport operation.
#![allow(dead_code)]
use crate::engine::{AgentRequest, Effort};
use crate::journal::{LaunchResolver, ResolvedLaunch};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct LaunchDefaults {
    pub profile: String,
    pub cwd: PathBuf,
    pub toolsets: Option<Vec<String>>,
    pub worktrees: Option<Arc<dyn WorktreeProvider>>,
}

pub trait WorktreeProvider: Send + Sync + 'static {
    fn worktree_for(&self, request: &AgentRequest) -> Result<PathBuf, String>;
}

#[derive(Clone)]
pub struct ExplicitLaunch {
    defaults: LaunchDefaults,
}

impl ExplicitLaunch {
    pub fn new(defaults: LaunchDefaults) -> Self {
        Self { defaults }
    }

    pub fn complete(
        &self,
        request: &AgentRequest,
        model: String,
        provider: String,
        effort: Option<Effort>,
    ) -> Result<ResolvedLaunch, String> {
        let _ = (self, request, model, provider, effort);
        todo!("hermes binding stub")
    }
}

impl LaunchResolver for ExplicitLaunch {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String> {
        let _ = (self, request);
        todo!("hermes binding stub")
    }
}
