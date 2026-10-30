//! Compile command checks. The verb returns before any gateway call.

use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hermes_client::kanban::{CreateTask, KanbanClient, TaskPatch};
use hermes_client::{Ticket, TicketError, TicketMinter};
use serde_json::json;
use talaria_workflow::kanban_compile::{
    BoxFuture, CardSink, CompileRequest, CreateCard, CreatedCard, ExistingCard, SinkError, compile,
};
use talaria_workflow::staffing::{Roster, StaffingResolver, StaffingTable};

#[path = "../../hermes-client/tests/support/mock_kanban.rs"]
mod mock_kanban;

use mock_kanban::{MockKanban, MockOptions};

const SEQUENTIAL: &str = include_str!("../../talaria-workflow/tests/kanban_compile/sequential.js");

#[tokio::test]
async fn dry_run_writes_nothing() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let dir = tempfile::tempdir().expect("temp");
    let script = dir.path().join("sequential.js");
    std::fs::write(&script, SEQUENTIAL).expect("script");
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "workflow",
            "compile",
            script.to_str().expect("script path"),
            "--board",
            "mock",
            "--profile",
            "fixture",
            "--dry-run",
        ])
        .env("TALARIA_CONFIG_DIR", dir.path())
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dry run:") && stdout.contains("nothing written"),
        "verb missing"
    );
    for line in [
        "static check: two captures agreed",
        "kanban does not check card results against your schema",
        "workspaces are temporary:",
        "this board is shared by every profile on this machine",
    ] {
        assert!(stdout.contains(line), "verb missing");
    }
    assert!(mock.hits().is_empty(), "verb missing");
}

#[tokio::test]
async fn compile_then_view_round_trip() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = HttpSink {
        client: KanbanClient::new(&mock.origin(), "mock", minter()).expect("client"),
    };
    let report = compile(
        CompileRequest {
            source: SEQUENTIAL.into(),
            script_name: "sequential".into(),
            board: "mock".into(),
            tenant: None,
            args: json!({}),
            profile: Some("fixture".into()),
            repo: None,
            dry_run: false,
            no_dispatch: false,
            resolver: resolver(),
            minter: minter(),
            origin: mock.origin(),
        },
        &sink,
    )
    .await
    .expect("halves unwired");
    let state = talaria_app::kanban::cards_to_board(&report.cards).expect("halves unwired");
    assert_eq!(report.cards.len(), 2, "halves unwired");
    let child = state
        .card(&report.cards[1].task_id)
        .expect("halves unwired");
    let parent_title = &report.cards[0].title;
    assert!(
        child
            .parent_titles
            .iter()
            .any(|(title, _)| title == parent_title),
        "halves unwired"
    );
}

fn resolver() -> StaffingResolver {
    let table = StaffingTable::parse(
        "[tiers.builder]\nmodel = \"m-small\"\nprovider = \"alpha-cloud\"\neffort = \"high\"\n",
    )
    .expect("table");
    let roster = Roster::from_model_options(
        &json!({
            "model": "m-small",
            "provider": "alpha-cloud",
            "providers": [{
                "slug": "alpha-cloud",
                "name": "Alpha",
                "authenticated": true,
                "models": ["m-small"]
            }]
        }),
        "fixture",
        chrono::DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
            .expect("time")
            .with_timezone(&chrono::Utc),
    )
    .expect("roster");
    StaffingResolver::new(std::sync::Arc::new(table), std::sync::Arc::new(roster))
}

fn minter() -> Arc<dyn TicketMinter> {
    Arc::new(CountingMinter::default())
}

struct HttpSink {
    client: KanbanClient,
}

impl CardSink for HttpSink {
    fn orchestration<'a>(&'a self) -> BoxFuture<'a, Result<String, SinkError>> {
        Box::pin(async move {
            let settings = self
                .client
                .orchestration()
                .await
                .map_err(|err| SinkError(err.to_string()))?;
            Ok(settings.default_assignee)
        })
    }

    fn existing<'a>(&'a self) -> BoxFuture<'a, Result<Vec<ExistingCard>, SinkError>> {
        Box::pin(async move {
            let snap = self
                .client
                .board_with(true)
                .await
                .map_err(|err| SinkError(err.to_string()))?;
            let mut cards = Vec::new();
            for column in snap.columns {
                for task in column.tasks {
                    let detail = self
                        .client
                        .task(&task.id)
                        .await
                        .map_err(|err| SinkError(err.to_string()))?;
                    cards.push(ExistingCard {
                        id: task.id,
                        idempotency_key: task.idempotency_key.unwrap_or_default(),
                        status: task.status.clone(),
                        archived: task.status == "archived",
                        parents: detail.parent_ids,
                    });
                }
            }
            Ok(cards)
        })
    }

    fn create<'a>(&'a self, card: &'a CreateCard) -> BoxFuture<'a, Result<CreatedCard, SinkError>> {
        Box::pin(async move {
            let task = self
                .client
                .create_task(&CreateTask {
                    title: card.title.clone(),
                    body: card.body.clone(),
                    assignee: card.assignee.clone(),
                    parents: card.parents.clone(),
                    idempotency_key: card.idempotency_key.clone(),
                    skills: card.skills.clone(),
                    workspace_kind: card.workspace_kind.clone(),
                    workspace_path: card.workspace_path.clone(),
                    tenant: card.tenant.clone(),
                    model_override: card.model_override.clone(),
                    provider_override: card.provider_override.clone(),
                    reasoning_effort: card.reasoning_effort.clone(),
                })
                .await
                .map_err(|err| SinkError(err.to_string()))?;
            Ok(CreatedCard {
                id: task.id,
                status: task.status,
            })
        })
    }

    fn link<'a>(&'a self, parent: &'a str, child: &'a str) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.client
                .link(parent, child)
                .await
                .map_err(|err| SinkError(err.to_string()))
        })
    }

    fn complete_root<'a>(
        &'a self,
        id: &'a str,
        summary: &'a str,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.client
                .patch_task(
                    id,
                    &TaskPatch {
                        status: "done".into(),
                        summary: Some(summary.into()),
                    },
                )
                .await
                .map(|_| ())
                .map_err(|err| SinkError(err.to_string()))
        })
    }

    fn dispatch<'a>(&'a self) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.client
                .dispatch()
                .await
                .map_err(|err| SinkError(err.to_string()))
        })
    }
}

#[derive(Default)]
struct CountingMinter(AtomicUsize);

impl TicketMinter for CountingMinter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async move {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            Ticket::parse(format!("TICKET-SENTINEL-{n}"))
        })
    }
}
