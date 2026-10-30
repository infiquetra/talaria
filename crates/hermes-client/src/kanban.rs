//! The board plugin's HTTP routes and independently authenticated events socket.

mod tail;
pub use tail::KanbanTail;

use crate::rest::{Method, RequestOpts, RestClient, RestError, RestOptions, TicketMinter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct LinkCounts {
    pub parents: u64,
    pub children: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    #[serde(deserialize_with = "null_default")]
    pub body: String,
    pub assignee: Option<String>,
    pub status: String,
    pub priority: i64,
    pub tenant: Option<String>,
    pub result: Option<String>,
    pub idempotency_key: Option<String>,
    #[serde(deserialize_with = "null_default")]
    pub skills: Vec<String>,
    pub model_override: Option<String>,
    pub provider_override: Option<String>,
    pub reasoning_effort: Option<String>,
    pub session_id: Option<String>,
    pub workspace_kind: Option<String>,
    pub workspace_path: Option<String>,
    #[serde(deserialize_with = "null_default")]
    pub link_counts: LinkCounts,
    #[serde(deserialize_with = "null_default")]
    pub comment_count: u64,
    #[serde(deserialize_with = "null_default")]
    pub progress: Progress,
    pub latest_summary: Option<String>,
    pub current_run_started_at: Option<i64>,
    pub last_heartbeat_at: Option<i64>,
    pub current_run_id: Option<i64>,
    pub created_at: Option<i64>,
}
fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BoardColumn {
    pub name: String,
    pub tasks: Vec<KanbanTask>,
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BoardSnapshot {
    pub columns: Vec<BoardColumn>,
    pub tenants: Vec<String>,
    pub assignees: Vec<String>,
    pub latest_event_id: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ColumnDef {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub drag_target: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct WorkflowColumns {
    pub columns: Vec<ColumnDef>,
    pub manual: BTreeMap<String, Vec<String>>,
    pub archived: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BoardInfo {
    pub slug: String,
    pub total: u64,
    pub is_current: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BoardsReply {
    pub boards: Vec<BoardInfo>,
    pub current: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OrchestrationSettings {
    pub default_assignee: String,
    pub resolved_default_assignee: String,
    pub active_profile: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskPatch {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TaskDetail {
    pub task: KanbanTask,
    pub comments: Vec<Value>,
    pub parent_ids: Vec<String>,
    pub child_ids: Vec<String>,
    pub events: Vec<Value>,
    pub attachments: Vec<Value>,
    pub link_tasks: Vec<Value>,
    pub child_results: Vec<Value>,
    pub runs: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct TailBatch {
    pub cursor: i64,
    pub events: Vec<Value>,
}
#[derive(Debug, thiserror::Error)]
pub enum KanbanError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Rest(#[from] RestError),
}
pub struct KanbanClient {
    rest: RestClient,
    board: String,
    warnings: std::sync::Mutex<Vec<String>>,
}
impl KanbanClient {
    pub fn new(
        origin: &str,
        board: impl Into<String>,
        minter: Arc<dyn TicketMinter>,
    ) -> Result<Self, KanbanError> {
        let board = board.into().trim().to_ascii_lowercase();
        if board.is_empty()
            || board.len() > 64
            || !board.as_bytes()[0].is_ascii_alphanumeric()
            || !board
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        {
            return Err(KanbanError::Message("invalid kanban board slug".into()));
        }
        Ok(Self {
            rest: RestClient::new(origin, minter, RestOptions::default())?,
            board,
            warnings: std::sync::Mutex::new(Vec::new()),
        })
    }
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.lock().unwrap_or_else(|e| e.into_inner()))
    }
    pub fn board_slug(&self) -> &str {
        &self.board
    }
    async fn request(
        &self,
        method: Method,
        route: &str,
        body: Option<Value>,
        scoped: bool,
    ) -> Result<Value, KanbanError> {
        let mut path = format!("/api/plugins/kanban{route}");
        if scoped {
            path.push_str(if route.contains('?') {
                "&board="
            } else {
                "?board="
            });
            path.push_str(&self.board);
        }
        self.rest.request(method, &path, body.as_ref(), RequestOpts::default()).await.map_err(|err| {
            match err {
                RestError::Unauthorized | RestError::Forbidden { .. } | RestError::Ticket(_) => KanbanError::Message("the gateway refused the board request; check the gateway is running and this profile may use it".into()),
                RestError::NotFound { .. } => KanbanError::Message(format!("no kanban board named {}", self.board)),
                RestError::Refused { detail: Some(detail), .. } => KanbanError::Message(detail),
                RestError::BodyTooLarge => KanbanError::Message(format!("board response too large ({})", route.split('?').next().unwrap_or(route))),
                other => KanbanError::Rest(other),
            }
        })
    }
    pub async fn boards(&self) -> Result<BoardsReply, KanbanError> {
        decode(self.request(Method::Get, "/boards", None, false).await?)
    }
    pub async fn board(&self) -> Result<BoardSnapshot, KanbanError> {
        self.board_with(false).await
    }
    /// The compiler alone includes archived keys for recreation preflight.
    pub async fn board_with(&self, include_archived: bool) -> Result<BoardSnapshot, KanbanError> {
        let route = if include_archived {
            "/board?include_archived=true"
        } else {
            "/board"
        };
        let mut snapshot: BoardSnapshot =
            decode(self.request(Method::Get, route, None, true).await?)?;
        // The wire puts unknown statuses in todo. Preserve status and regroup locally.
        let tasks: Vec<_> = snapshot
            .columns
            .iter_mut()
            .flat_map(|c| std::mem::take(&mut c.tasks))
            .collect();
        for task in tasks {
            if task.status == "archived" && !include_archived {
                continue;
            }
            let index = snapshot.columns.iter().position(|c| c.name == task.status);
            let index = index.unwrap_or_else(|| {
                snapshot
                    .columns
                    .iter()
                    .position(|c| c.name == "Other")
                    .unwrap_or_else(|| {
                        snapshot.columns.push(BoardColumn {
                            name: "Other".into(),
                            tasks: vec![],
                        });
                        snapshot.columns.len() - 1
                    })
            });
            snapshot.columns[index].tasks.push(task);
        }
        Ok(snapshot)
    }
    pub async fn task(&self, id: &str) -> Result<TaskDetail, KanbanError> {
        let value = self
            .request(Method::Get, &format!("/tasks/{}", segment(id)), None, true)
            .await?;
        let strings = |key: &str| {
            value["links"][key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        let rows = |key: &str| value[key].as_array().cloned().unwrap_or_default();
        let mut comments = rows("comments");
        comments.sort_by(|a, b| a["created_at"].as_i64().cmp(&b["created_at"].as_i64()));
        Ok(TaskDetail {
            task: decode(value["task"].clone())?,
            comments,
            parent_ids: strings("parents"),
            child_ids: strings("children"),
            events: rows("events"),
            attachments: rows("attachments"),
            link_tasks: rows("link_tasks"),
            child_results: rows("child_results"),
            runs: rows("runs"),
        })
    }
    pub async fn create_task_with_warning(
        &self,
        req: &CreateTask,
    ) -> Result<(KanbanTask, Option<String>), KanbanError> {
        let body = serde_json::to_value(req)
            .map_err(|_| KanbanError::Message("invalid card payload".into()))?;
        let reply = self
            .request(Method::Post, "/tasks", Some(body), true)
            .await?;
        Ok((
            decode(reply["task"].clone())?,
            reply["warning"].as_str().map(str::to_owned),
        ))
    }
    pub async fn create_task(&self, req: &CreateTask) -> Result<KanbanTask, KanbanError> {
        let (task, warning) = self.create_task_with_warning(req).await?;
        if let Some(warning) = warning {
            self.warnings
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(warning);
        }
        Ok(task)
    }
    pub async fn patch_task(&self, id: &str, patch: &TaskPatch) -> Result<KanbanTask, KanbanError> {
        let reply = self
            .request(
                Method::Patch,
                &format!("/tasks/{}", segment(id)),
                Some(json!(patch)),
                true,
            )
            .await?;
        decode(reply["task"].clone())
    }
    pub async fn comment(&self, id: &str, body: &str) -> Result<(), KanbanError> {
        self.request(
            Method::Post,
            &format!("/tasks/{}/comments", segment(id)),
            Some(json!({"body":body, "author":"talaria"})),
            true,
        )
        .await?;
        Ok(())
    }
    pub async fn link(&self, parent: &str, child: &str) -> Result<(), KanbanError> {
        self.request(
            Method::Post,
            "/links",
            Some(json!({"parent_id":parent,"child_id":child})),
            true,
        )
        .await?;
        Ok(())
    }
    pub async fn dispatch(&self) -> Result<(), KanbanError> {
        self.request(Method::Post, "/dispatch", None, true).await?;
        Ok(())
    }
    pub async fn workflow(&self) -> Result<WorkflowColumns, KanbanError> {
        decode(self.request(Method::Get, "/workflow", None, false).await?)
    }
    pub async fn orchestration(&self) -> Result<OrchestrationSettings, KanbanError> {
        decode(
            self.request(Method::Get, "/orchestration", None, false)
                .await?,
        )
    }
}
fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, KanbanError> {
    serde_json::from_value(v).map_err(|_| KanbanError::Message("invalid board response".into()))
}
fn segment(id: &str) -> String {
    url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
}

/// Report returned by the opt-in live command probe.
pub struct LiveCards {
    pub second_summary: String,
    pub bodies: Vec<String>,
    pub statuses: Vec<String>,
}
/// Drive the installed command, then observe completion through the board API.
pub async fn live_compile_and_complete(
    board: &str,
    source: &str,
) -> Result<LiveCards, KanbanError> {
    let home = crate::default_home();
    let endpoint = Arc::new(
        crate::discover_with(
            &home,
            &crate::DiscoverOptions {
                ensure: crate::EnsurePolicy::Never,
                ..Default::default()
            },
        )
        .await
        .map_err(|_| KanbanError::Message("no gateway for live compile".into()))?,
    );
    let profile = endpoint
        .resolve_profile("fixture")
        .map_err(|_| KanbanError::Message("invalid live profile".into()))?;
    let minter = Arc::new(crate::NativeHttpMinter::new(
        home,
        endpoint.clone(),
        profile,
    ));
    let client = KanbanClient::new(endpoint.api_origin(), board, minter)?;
    let path = std::env::temp_dir().join(format!("talaria-live-compile-{}.js", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| KanbanError::Message("cannot create live workflow input".into()))?;
    use std::io::Write;
    file.write_all(source.as_bytes())
        .map_err(|_| KanbanError::Message("cannot write live workflow input".into()))?;
    let result = tokio::process::Command::new(
        std::env::var_os("TALARIA_BIN").unwrap_or_else(|| "talaria".into()),
    )
    .args(["workflow", "compile"])
    .arg(&path)
    .args(["--board", board, "--profile", "fixture"])
    .output()
    .await;
    let _ = std::fs::remove_file(&path);
    let output =
        result.map_err(|_| KanbanError::Message("cannot run live compile command".into()))?;
    if !output.status.success() {
        return Err(KanbanError::Message("live compile command failed".into()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<_> = text
        .lines()
        .filter_map(|line| line.split_once(" -> "))
        .filter(|(label, _)| *label != "root")
        .filter_map(|(_, tail)| tail.split_whitespace().next().map(str::to_owned))
        .collect();
    if ids.len() != 2 {
        return Err(KanbanError::Message(
            "live compile did not return two cards".into(),
        ));
    }
    let mut last = String::new();
    let mut progress = tokio::time::Instant::now();
    let deadline = progress + std::time::Duration::from_secs(600);
    loop {
        let mut tasks = Vec::new();
        for id in &ids {
            tasks.push(client.task(id).await?.task);
        }
        let statuses: Vec<_> = tasks.iter().map(|t| t.status.clone()).collect();
        let signature = format!(
            "{statuses:?} {:?}",
            tasks
                .iter()
                .map(|t| (
                    &t.latest_summary,
                    t.last_heartbeat_at,
                    t.current_run_started_at
                ))
                .collect::<Vec<_>>()
        );
        if signature != last {
            last = signature;
            progress = tokio::time::Instant::now();
        }
        if statuses.iter().any(|s| s == "archived") || tokio::time::Instant::now() >= deadline {
            return Err(KanbanError::Message(format!(
                "live compile did not complete: {statuses:?}"
            )));
        }
        if tasks[1].status == "done"
            && tasks[1]
                .latest_summary
                .as_deref()
                .is_some_and(|s| s.contains("BANANA BANANA"))
        {
            return Ok(LiveCards {
                second_summary: tasks[1].latest_summary.clone().unwrap_or_default(),
                bodies: tasks.into_iter().map(|t| t.body).collect(),
                statuses,
            });
        }
        if statuses.iter().any(|s| s == "archived")
            || tokio::time::Instant::now() >= deadline
            || progress.elapsed() >= std::time::Duration::from_secs(180)
        {
            return Err(KanbanError::Message(format!(
                "live compile did not complete: {statuses:?}"
            )));
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
