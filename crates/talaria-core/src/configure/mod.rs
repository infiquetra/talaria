//! Pure types for reviewing configuration proposals.
mod apply;
mod brief;
mod reply;
mod state;
mod validate;

pub use apply::*;
pub use brief::*;
pub use reply::*;
pub use state::*;
pub use validate::*;

use crate::settings::ConfigSchemaView;
use serde_json::Value;

pub const BRIEF_MAX_BYTES: usize = 49_152;
pub const SKILL_MAX_BYTES: usize = 6_000;
pub const CONTEXT_REPLY_MAX_BYTES: usize = 24_576;
pub const REPLY_BLOCK_MAX_BYTES: usize = 65_536;
pub const MAX_CHANGES: usize = 20;
pub const MAX_CONTEXT_ROUNDS: u8 = 3;
pub const WITHHELD: &str = "<withheld: looks like a secret>";
pub type SecretDetector = fn(&str) -> bool;

#[derive(Clone, Debug, PartialEq)]
pub enum Section<T> {
    Available(Vec<T>),
    Unavailable(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct McpRow {
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub plugin: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CatalogueRow {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub installed: bool,
    pub enabled: bool,
    pub requires: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CapRow {
    pub kind: CapKind,
    pub name: String,
    pub enabled: bool,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CronRow {
    pub job_id: String,
    pub name: String,
    pub schedule: String,
    pub state: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRow {
    pub slug: String,
    pub configured: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RouteRow {
    pub slug: String,
    pub authenticated: bool,
    pub models: Vec<String>,
    pub api_url: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MainModelRow {
    pub provider: String,
    pub model: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub mcp_servers: Section<McpRow>,
    pub mcp_catalogue: Section<CatalogueRow>,
    pub skills: Section<CapRow>,
    pub toolsets: Section<CapRow>,
    pub plugins: Section<CapRow>,
    pub cron_jobs: Section<CronRow>,
    pub providers: Section<ProviderRow>,
    pub main_model: Section<MainModelRow>,
    pub routes: Section<RouteRow>,
    pub profiles: Section<String>,
    pub connectors: Section<(String, bool)>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub profile: String,
    pub hermes_version: Option<String>,
    pub schema: ConfigSchemaView,
    pub effective: Value,
    pub raw: Value,
    pub defaults: Value,
    pub inventory: Inventory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Gathering,
    Starting,
    Thinking,
    Question,
    Review,
    Confirming,
    Applying,
    Done,
    Answer,
    Cannot,
    NoProposal,
    Failed,
    Cancelled,
    Closing,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewLine {
    pub text: String,
    pub row: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigureView {
    pub title: String,
    pub phase: Phase,
    pub lines: Vec<ViewLine>,
    /// The active question, kept visible independently of the proposal scroll.
    pub confirmation: Option<String>,
    pub footer: String,
    pub selected_row: Option<usize>,
}
impl ConfigureView {
    pub fn text_lines(&self) -> Vec<String> {
        std::iter::once(self.title.clone())
            .chain(self.lines.iter().map(|line| line.text.clone()))
            .chain(self.confirmation.iter().cloned())
            .chain(std::iter::once(self.footer.clone()))
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigureKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Space,
    Enter,
    Esc,
    Char(char),
    Backspace,
    Paste(String),
}
