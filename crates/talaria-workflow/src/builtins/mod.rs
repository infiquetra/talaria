//! Embedded workflows and safe installation of editable copies.
mod install;
mod registry;
pub use install::*;
pub use registry::*;

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
