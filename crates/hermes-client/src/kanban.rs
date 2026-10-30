//! Board HTTP and the events tail. Methods return until the board client exists.

use std::sync::Arc;

use serde_json::Value;

use crate::rest::{RestClient, RestOptions, TicketMinter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCounts {
    pub parents: u64,
    pub children: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub body: String,
    pub assignee: Option<String>,
    pub status: String,
    pub priority: i64,
    pub tenant: Option<String>,
    pub result: Option<String>,
    pub idempotency_key: Option<String>,
    pub skills: Vec<String>,
    pub model_override: Option<String>,
    pub provider_override: Option<String>,
    pub reasoning_effort: Option<String>,
    pub session_id: Option<String>,
    pub workspace_kind: Option<String>,
    pub workspace_path: Option<String>,
    pub link_counts: LinkCounts,
    pub comment_count: u64,
    pub progress: Progress,
    pub latest_summary: Option<String>,
    pub current_run_started_at: Option<i64>,
    pub created_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardColumn {
    pub name: String,
    pub tasks: Vec<KanbanTask>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardSnapshot {
    pub columns: Vec<BoardColumn>,
    pub tenants: Vec<String>,
    pub assignees: Vec<String>,
    pub latest_event_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDef {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub drag_target: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowColumns {
    pub columns: Vec<ColumnDef>,
    pub manual: std::collections::BTreeMap<String, Vec<String>>,
    pub archived: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardInfo {
    pub slug: String,
    pub total: u64,
    pub is_current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardsReply {
    pub boards: Vec<BoardInfo>,
    pub current: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrchestrationSettings {
    pub default_assignee: String,
    pub resolved_default_assignee: String,
    pub active_profile: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateTask {
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
pub struct TaskPatch {
    pub status: String,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskDetail {
    pub task: KanbanTask,
    pub comments: Vec<Value>,
    pub parent_ids: Vec<String>,
    pub child_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TailBatch {
    pub cursor: i64,
    pub events: Vec<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum KanbanError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Rest(#[from] crate::rest::RestError),
}

pub struct KanbanClient {
    rest: RestClient,
    board: String,
}

impl KanbanClient {
    pub fn new(
        origin: &str,
        board: impl Into<String>,
        minter: Arc<dyn TicketMinter>,
    ) -> Result<Self, KanbanError> {
        let rest = RestClient::new(origin, minter, RestOptions::default())?;
        Ok(Self {
            rest,
            board: board.into(),
        })
    }

    pub fn board_slug(&self) -> &str {
        &self.board
    }

    pub async fn boards(&self) -> Result<BoardsReply, KanbanError> {
        let _ = &self.rest;
        Err(KanbanError::Message("board list is not available".into()))
    }

    pub async fn board(&self) -> Result<BoardSnapshot, KanbanError> {
        self.board_with(false).await
    }

    pub async fn board_with(&self, include_archived: bool) -> Result<BoardSnapshot, KanbanError> {
        let _ = (include_archived, &self.rest);
        Err(KanbanError::Message(
            "board snapshot is not available".into(),
        ))
    }

    pub async fn task(&self, id: &str) -> Result<TaskDetail, KanbanError> {
        let _ = (id, &self.rest);
        Err(KanbanError::Message("task detail is not available".into()))
    }

    pub async fn create_task(&self, req: &CreateTask) -> Result<KanbanTask, KanbanError> {
        let _ = (req, &self.rest);
        Err(KanbanError::Message("task create is not available".into()))
    }

    pub async fn patch_task(&self, id: &str, patch: &TaskPatch) -> Result<KanbanTask, KanbanError> {
        let _ = (id, patch, &self.rest);
        Err(KanbanError::Message("task patch is not available".into()))
    }

    pub async fn comment(&self, id: &str, body: &str) -> Result<(), KanbanError> {
        let _ = (id, body, &self.rest);
        Err(KanbanError::Message("task comment is not available".into()))
    }

    pub async fn link(&self, parent: &str, child: &str) -> Result<(), KanbanError> {
        let _ = (parent, child, &self.rest);
        Err(KanbanError::Message("task link is not available".into()))
    }

    pub async fn dispatch(&self) -> Result<(), KanbanError> {
        let _ = &self.rest;
        Err(KanbanError::Message("dispatch is not available".into()))
    }

    pub async fn workflow(&self) -> Result<WorkflowColumns, KanbanError> {
        let _ = &self.rest;
        Err(KanbanError::Message(
            "workflow columns are not available".into(),
        ))
    }

    pub async fn orchestration(&self) -> Result<OrchestrationSettings, KanbanError> {
        let _ = &self.rest;
        Err(KanbanError::Message(
            "orchestration settings are not available".into(),
        ))
    }
}

pub struct KanbanTail;

impl KanbanTail {
    pub async fn connect(
        origin: &str,
        board: &str,
        since: i64,
        minter: Arc<dyn TicketMinter>,
    ) -> Result<Self, KanbanError> {
        let _ = (origin, board, since, minter);
        Err(KanbanError::Message("event tail is not available".into()))
    }

    pub async fn recv(&mut self) -> Result<TailBatch, KanbanError> {
        Err(KanbanError::Message("event tail is not available".into()))
    }
}

pub struct LiveCards {
    pub second_summary: String,
    pub bodies: Vec<String>,
    pub statuses: Vec<String>,
}

pub async fn live_compile_and_complete(
    board: &str,
    source: &str,
) -> Result<LiveCards, KanbanError> {
    let _ = (board, source);
    Err(KanbanError::Message("live compile is not available".into()))
}
