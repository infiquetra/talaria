//! Compile a static script to board cards. The entry point refuses until the compiler exists.

mod capture;
mod plan;

pub use capture::{CapturedCall, capture_twice};
pub use futures::future::BoxFuture;
pub use plan::{card_body, compile_id};

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::staffing::StaffingResolver;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedCard {
    pub label: String,
    pub task_id: String,
    pub status: String,
    pub parent_ids: Vec<String>,
    pub parent_labels: Vec<String>,
    pub title: String,
    pub body: String,
    pub assignee: Option<String>,
    pub idempotency_key: String,
    pub model_override: Option<String>,
    pub provider_override: Option<String>,
    pub reasoning_effort: Option<String>,
    pub workspace_kind: String,
    pub workspace_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileReport {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
    pub cards: Vec<PlannedCard>,
}

#[derive(Debug)]
pub struct CompileFailure {
    pub exit_code: u8,
    pub stderr: String,
}

impl std::fmt::Display for CompileFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.stderr)
    }
}

impl std::error::Error for CompileFailure {}

impl CompileFailure {
    pub fn stub() -> Self {
        Self {
            exit_code: 1,
            stderr: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateCard {
    pub title: String,
    pub body: String,
    pub assignee: Option<String>,
    pub parents: Vec<String>,
    pub idempotency_key: String,
    pub skills: Vec<String>,
    pub workspace_kind: String,
    pub workspace_path: Option<String>,
    pub tenant: Option<String>,
    pub model_override: Option<String>,
    pub provider_override: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedCard {
    pub id: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExistingCard {
    pub id: String,
    pub idempotency_key: String,
    pub status: String,
    pub archived: bool,
    pub parents: Vec<String>,
}

#[derive(Debug)]
pub struct SinkError(pub String);

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SinkError {}

pub trait CardSink: Send + Sync {
    fn orchestration<'a>(&'a self) -> BoxFuture<'a, Result<String, SinkError>>;
    fn existing<'a>(&'a self) -> BoxFuture<'a, Result<Vec<ExistingCard>, SinkError>>;
    fn create<'a>(&'a self, card: &'a CreateCard) -> BoxFuture<'a, Result<CreatedCard, SinkError>>;
    fn link<'a>(&'a self, parent: &'a str, child: &'a str) -> BoxFuture<'a, Result<(), SinkError>>;
    fn complete_root<'a>(
        &'a self,
        id: &'a str,
        summary: &'a str,
    ) -> BoxFuture<'a, Result<(), SinkError>>;
    fn dispatch<'a>(&'a self) -> BoxFuture<'a, Result<(), SinkError>>;
}

pub struct CompileRequest {
    pub source: String,
    pub script_name: String,
    pub board: String,
    pub tenant: Option<String>,
    pub args: Value,
    pub profile: Option<String>,
    pub repo: Option<PathBuf>,
    pub dry_run: bool,
    pub no_dispatch: bool,
    pub resolver: StaffingResolver,
    pub minter: Arc<dyn hermes_client::rest::TicketMinter>,
    pub origin: String,
}

pub async fn compile(
    request: CompileRequest,
    sink: &dyn CardSink,
) -> Result<CompileReport, CompileFailure> {
    let _ = (request, sink);
    Err(CompileFailure::stub())
}
