//! Complete explicit launch requests before any transport operation.
use super::validate::check_schema;
use crate::engine::{AgentRequest, Effort, Isolation};
use crate::journal::LaunchResolver;
pub use crate::journal::ResolvedLaunch;
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
        let toolsets = request
            .toolsets
            .clone()
            .or_else(|| self.defaults.toolsets.clone())
            .ok_or("name toolsets for this agent, or set a run default")?;
        let cwd = if request.isolation == Some(Isolation::Worktree) {
            self.defaults
                .worktrees
                .as_ref()
                .ok_or("worktree isolation needs a worktree provider (P5-07)")?
                .worktree_for(request)?
        } else {
            self.defaults.cwd.clone()
        };
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err("cwd must be an absolute path to an existing directory".into());
        }
        if let Some(schema) = &request.schema {
            check_schema(schema)?;
        }
        Ok(ResolvedLaunch {
            profile: request
                .profile
                .clone()
                .unwrap_or_else(|| self.defaults.profile.clone()),
            model,
            provider: Some(provider),
            effort,
            toolsets,
            skills: request.skills.clone().unwrap_or_default(),
            max_turns: request.max_turns,
            isolation: request.isolation,
            cwd: Some(
                cwd.to_str()
                    .ok_or("cwd must be an absolute path to an existing directory")?
                    .into(),
            ),
        })
    }
}
impl LaunchResolver for ExplicitLaunch {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String> {
        let model=request.model.clone().ok_or_else(||match &request.tier {Some(tier)=>format!("tier '{tier}' needs the staffing table; name model and provider, or run with staffing"),None=>"no model: name model and provider, or run with staffing".into()})?;
        let provider = request
            .provider
            .clone()
            .ok_or_else(|| format!("model '{model}' needs a provider"))?;
        self.complete(request, model, provider, request.effort)
    }
}
