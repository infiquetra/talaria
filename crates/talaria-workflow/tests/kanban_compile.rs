//! Compiler checks against the in-process board.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hermes_client::kanban::{CreateTask, KanbanClient, TaskPatch};
use hermes_client::{Ticket, TicketError, TicketMinter};
use serde_json::{Value, json};
use talaria_workflow::kanban_compile::{
    BoxFuture, CardSink, CompileReport, CompileRequest, CreateCard, CreatedCard, ExistingCard,
    SinkError, compile,
};
use talaria_workflow::staffing::{Roster, StaffingResolver, StaffingTable};

#[path = "../../hermes-client/tests/support/mock_kanban.rs"]
mod mock_kanban;

use mock_kanban::{MockKanban, MockOptions};

const SEQUENTIAL: &str = include_str!("kanban_compile/sequential.js");
const PIPELINE: &str = include_str!("kanban_compile/pipeline.js");
const FANOUT: &str = include_str!("kanban_compile/fanout.js");
const DYNAMIC: &str = include_str!("kanban_compile/dynamic.js");
const SEEDED: &str = include_str!("kanban_compile/seeded.js");
const NUMERIC: &str = include_str!("kanban_compile/numeric.js");
const TOOLED: &str = include_str!("kanban_compile/tooled.js");
const LIMITED: &str = include_str!("kanban_compile/limited.js");
const TIERED: &str = include_str!("kanban_compile/tiered.js");
const WORKTREE: &str = include_str!("kanban_compile/worktree.js");
const SCHEMA: &str = include_str!("kanban_compile/schema.js");

const FIVE: &str = r#"export const meta = {name: 'five', description: 'five calls'};
phase('Work');
await agent('one', {label: 'one', profile: 'fixture'});
await agent('two', {label: 'two', profile: 'fixture'});
await agent('three', {label: 'three', profile: 'fixture'});
await agent('four', {label: 'four', profile: 'fixture'});
await agent('five', {label: 'five', profile: 'fixture'});
"#;

const MISSING_MODEL: &str = r#"export const meta = {name: 'missing', description: 'missing model'};
phase('Work');
await agent('missing model', {label: 'missing', profile: 'fixture', model: 'not-on-roster'});
"#;

const PROVIDER_ONLY: &str = r#"export const meta = {name: 'provider', description: 'provider without a model'};
phase('Work');
await agent('provider only', {label: 'provider', profile: 'fixture', provider: 'alpha-cloud'});
"#;

#[tokio::test]
async fn sequential_calls_chain_by_control() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let report = compile(request(SEQUENTIAL, "sequential", &mock, None), &sink)
        .await
        .expect("no control edges");
    assert_eq!(report.cards.len(), 2, "no control edges");
    assert_eq!(report.cards[1].label, "write", "no control edges");
    assert_eq!(
        report.cards[1].parent_labels,
        vec!["read".to_string()],
        "no control edges"
    );
    assert_eq!(
        report.cards[1].parent_ids,
        vec![report.cards[0].task_id.clone()],
        "no control edges"
    );
}

#[tokio::test]
async fn sentinel_in_prompt_becomes_parent_edge() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let report = compile(request(PIPELINE, "pipeline", &mock, None), &sink)
        .await
        .expect("no edge rule");
    let child = &report.cards[1];
    assert_eq!(
        child.parent_ids,
        vec![report.cards[0].task_id.clone()],
        "no edge rule"
    );
    assert!(
        child.body.contains("Use the result of card")
            && child.body.contains(&report.cards[0].task_id),
        "no edge rule"
    );
}

#[tokio::test]
async fn parallel_barrier_joins_without_sibling_edges() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let report = compile(request(FANOUT, "fanout", &mock, None), &sink)
        .await
        .expect("no join rule");
    let sibling_ids: Vec<String> = ["one", "two", "three"]
        .into_iter()
        .map(|label| find(&report, label).task_id.clone())
        .collect();
    for label in ["one", "two", "three"] {
        let card = find(&report, label);
        assert!(
            card.parent_labels
                .iter()
                .all(|parent| !["one", "two", "three"].contains(&parent.as_str())),
            "no join rule"
        );
        assert!(
            card.parent_ids.iter().all(|id| !sibling_ids.contains(id)),
            "no join rule"
        );
    }
    let synthesis = find(&report, "synthesis");
    for label in ["one", "two", "three"] {
        assert!(
            synthesis.parent_labels.iter().any(|parent| parent == label),
            "no join rule"
        );
    }
}

#[tokio::test]
async fn value_branch_refused_by_subset_guard() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let err = compile(request(DYNAMIC, "dynamic", &mock, None), &sink)
        .await
        .expect_err("no guard");
    assert!(
        err.stderr.contains("not compilable:")
            && err.stderr.contains(" at line ")
            && err
                .stderr
                .contains("run it with talaria workflow run instead"),
        "no guard: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no guard");
    assert_eq!(creates(&mock), 0, "no guard");
}

#[tokio::test]
async fn divergent_captures_refuse_compile() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let err = compile(request(SEEDED, "seeded", &mock, None), &sink)
        .await
        .expect_err("no capture check");
    assert!(
        err.stderr.contains("not a static graph:")
            && err.stderr.contains("changed between captures")
            && err
                .stderr
                .contains("run it with talaria workflow run instead"),
        "no capture check: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no capture check");
    assert_eq!(creates(&mock), 0, "no capture check");
}

#[tokio::test]
async fn numeric_schema_field_becomes_parent_edge() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let report = compile(request(NUMERIC, "numeric", &mock, None), &sink)
        .await
        .expect("no marker rule");
    let child = &report.cards[1];
    assert_eq!(
        child.parent_ids,
        vec![report.cards[0].task_id.clone()],
        "no marker rule"
    );
    assert!(
        child.body.contains("Use the result of card")
            && child.body.contains(&report.cards[0].task_id),
        "no marker rule"
    );
}

#[tokio::test]
async fn unsupported_options_refuse_before_writes() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let err = compile(request(TOOLED, "tooled", &mock, None), &sink)
        .await
        .expect_err("no option gate");
    assert!(
        err.stderr
            .contains("not compilable: call tools sets toolsets")
            && err.stderr.contains("kanban cards cannot enforce"),
        "no option gate: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no option gate");
    assert_eq!(creates(&mock), 0, "no option gate");

    let err = compile(request(LIMITED, "limited", &mock, None), &sink)
        .await
        .expect_err("no option gate");
    assert!(
        err.stderr
            .contains("not compilable: call short sets maxTurns")
            && err.stderr.contains("kanban cards cannot enforce"),
        "no option gate: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no option gate");
    assert_eq!(creates(&mock), 0, "no option gate");
}

#[tokio::test]
async fn tier_resolves_and_unusable_route_refuses() {
    let tiered = MockKanban::start(MockOptions::default()).await;
    let tiered_sink = sink(&tiered);
    let report = compile(request(TIERED, "tiered", &tiered, None), &tiered_sink)
        .await
        .expect("no resolution");
    assert_eq!(
        report.cards[0].model_override.as_deref(),
        Some("m-small"),
        "no resolution"
    );
    assert_eq!(
        report.cards[0].provider_override.as_deref(),
        Some("alpha-cloud"),
        "no resolution"
    );
    assert_eq!(
        report.cards[0].reasoning_effort.as_deref(),
        Some("high"),
        "no resolution"
    );

    let missing = MockKanban::start(MockOptions::default()).await;
    let missing_sink = sink(&missing);
    let err = compile(
        request(MISSING_MODEL, "missing", &missing, None),
        &missing_sink,
    )
    .await
    .expect_err("no resolution");
    assert!(
        err.stderr.contains("no usable route for call missing"),
        "no resolution: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no resolution");
    assert_eq!(creates(&missing), 0, "no resolution");

    let provider = MockKanban::start(MockOptions::default()).await;
    let provider_sink = sink(&provider);
    let err = compile(
        request(PROVIDER_ONLY, "provider", &provider, None),
        &provider_sink,
    )
    .await
    .expect_err("no resolution");
    assert!(
        err.stderr.contains("no usable route for call provider"),
        "no resolution: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no resolution");
    assert_eq!(creates(&provider), 0, "no resolution");
}

#[tokio::test]
async fn worktree_needs_repo_anchor() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let err = compile(request(WORKTREE, "worktree", &mock, None), &sink)
        .await
        .expect_err("no anchor rule");
    assert!(
        err.stderr.contains(
            "call edit uses a worktree workspace but --repo is missing or not a git repository"
        ),
        "no anchor rule: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "no anchor rule");
    assert_eq!(creates(&mock), 0, "no anchor rule");

    let plain = tempfile::tempdir().expect("temp");
    let err = compile(
        request(
            WORKTREE,
            "worktree",
            &mock,
            Some(plain.path().to_path_buf()),
        ),
        &sink,
    )
    .await
    .expect_err("no anchor rule");
    assert!(
        err.stderr.contains("not a git repository"),
        "no anchor rule: {}",
        err.stderr
    );
    assert_eq!(creates(&mock), 0, "no anchor rule");

    let repo = git_repo();
    let report = compile(
        request(WORKTREE, "worktree", &mock, Some(repo.path().to_path_buf())),
        &sink,
    )
    .await
    .expect("no anchor rule");
    let anchor = std::fs::canonicalize(repo.path()).expect("no anchor rule");
    assert_eq!(report.cards[0].workspace_kind, "worktree", "no anchor rule");
    assert_eq!(
        report.cards[0].workspace_path.as_deref(),
        anchor.to_str(),
        "no anchor rule"
    );
}

#[tokio::test]
async fn default_assignee_gate_refuses_before_writes() {
    let mock = MockKanban::start(MockOptions {
        default_assignee: "fixture".into(),
        ..MockOptions::default()
    })
    .await;
    let sink = sink(&mock);
    let err = compile(request(SEQUENTIAL, "sequential", &mock, None), &sink)
        .await
        .expect_err("no gate");
    assert_eq!(
        err.stderr.trim(),
        "kanban.default_assignee is set to fixture; unset it for the compile, then re-run",
        "no gate: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 1, "no gate");
    assert_eq!(creates(&mock), 0, "no gate");
}

#[tokio::test]
async fn recompile_is_idempotent() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let _first = compile(request(SEQUENTIAL, "sequential", &mock, None), &sink)
        .await
        .expect("no preflight");
    let marker = mock.hits().len();
    let second = compile(request(SEQUENTIAL, "sequential", &mock, None), &sink)
        .await
        .expect("no preflight");
    assert_eq!(second.stdout.trim(), "already compiled", "no preflight");
    let extra = mock
        .hits()
        .into_iter()
        .skip(marker)
        .filter(|hit| hit.method == "POST" || hit.method == "PATCH")
        .count();
    assert_eq!(extra, 0, "no preflight");
}

#[tokio::test]
async fn partial_compile_resumes() {
    let mock = MockKanban::start(MockOptions {
        fail_task_post: Some(4),
        ..MockOptions::default()
    })
    .await;
    let sink = sink(&mock);
    let err = compile(request(FIVE, "five", &mock, None), &sink)
        .await
        .expect_err("no resume");
    assert!(
        err.stderr.contains("created 2 of 5 cards") && err.stderr.contains("card 3 (three)"),
        "no resume: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 1, "no resume");
    assert_eq!(mock.task_count(), 12, "no resume");
    let again = compile(request(FIVE, "five", &mock, None), &sink)
        .await
        .expect("no resume");
    assert!(again.stdout.contains("compiled 5 cards"), "no resume");
    assert_eq!(again.exit_code, 0, "no resume");
}

#[tokio::test]
async fn root_card_completes_last() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let report = compile(request(SEQUENTIAL, "sequential", &mock, None), &sink)
        .await
        .expect("no root rule");
    let hits = mock.hits();
    let creates: Vec<_> = hits
        .iter()
        .filter(|hit| hit.method == "POST" && hit.path.ends_with("/tasks"))
        .collect();
    assert!(
        creates[0].body["idempotency_key"]
            .as_str()
            .unwrap_or("")
            .ends_with(":root"),
        "no root rule"
    );
    let patch_at = hits
        .iter()
        .position(|hit| hit.method == "PATCH")
        .expect("no root rule");
    let last_create = hits
        .iter()
        .rposition(|hit| hit.method == "POST" && hit.path.ends_with("/tasks"))
        .expect("no root rule");
    assert!(patch_at > last_create, "no root rule");
    let patch = &hits[patch_at];
    assert_eq!(patch.body["status"], "done", "no root rule");
    assert_eq!(patch.body["summary"], "compiled 2 cards", "no root rule");
    assert!(report.stdout.contains("root ->"), "no root rule");
}

#[tokio::test]
async fn body_contract_first_and_size_gate() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let sink = sink(&mock);
    let huge = format!(
        "export const meta = {{name: 'oversize', description: 'too big'}};\nphase('Work');\nawait agent('{}', {{label: 'huge', profile: 'fixture'}});\n",
        "a".repeat(9_000)
    );
    let err = compile(request(&huge, "oversize", &mock, None), &sink)
        .await
        .expect_err("contract missing");
    assert!(
        err.stderr.contains("card body for call huge is ")
            && err.stderr.contains("bytes (limit 8192)")
            && err
                .stderr
                .contains("shorten the prompt or use talaria workflow run"),
        "contract missing: {}",
        err.stderr
    );
    assert_eq!(err.exit_code, 2, "contract missing");
    assert_eq!(creates(&mock), 0, "contract missing");

    let report = compile(request(SCHEMA, "schema", &mock, None), &sink)
        .await
        .expect("contract missing");
    let body = &report.cards[0].body;
    assert!(body.starts_with("## Result contract"), "contract missing");
    assert!(body.contains("```json"), "contract missing");
    assert!(body.contains("result_json"), "contract missing");
    assert!(body.contains("result_artifact"), "contract missing");
    assert!(
        body.contains("3,500") || body.contains("3500"),
        "contract missing"
    );
}

fn find<'a>(
    report: &'a CompileReport,
    label: &str,
) -> &'a talaria_workflow::kanban_compile::PlannedCard {
    report
        .cards
        .iter()
        .find(|card| card.label == label)
        .expect("card")
}

fn request(source: &str, name: &str, mock: &MockKanban, repo: Option<PathBuf>) -> CompileRequest {
    CompileRequest {
        source: source.to_string(),
        script_name: name.to_string(),
        board: "mock".into(),
        tenant: None,
        args: json!({}),
        profile: Some("fixture".into()),
        repo,
        dry_run: false,
        no_dispatch: false,
        resolver: resolver(),
        minter: minter(),
        origin: mock.origin(),
    }
}

fn creates(mock: &MockKanban) -> usize {
    mock.hits()
        .iter()
        .filter(|hit| hit.method == "POST" && hit.path.ends_with("/tasks"))
        .count()
}

fn sink(mock: &MockKanban) -> HttpSink {
    HttpSink {
        client: KanbanClient::new(&mock.origin(), "mock", minter()).expect("client"),
    }
}

fn resolver() -> StaffingResolver {
    let table = StaffingTable::parse(
        "[tiers.builder]\nmodel = \"m-small\"\nprovider = \"alpha-cloud\"\neffort = \"high\"\n",
    )
    .expect("table");
    let roster =
        Roster::from_model_options(&roster_value(), "fixture", roster_time()).expect("roster");
    StaffingResolver::new(Arc::new(table), Arc::new(roster))
}

fn roster_value() -> Value {
    json!({
        "model": "m-small",
        "provider": "alpha-cloud",
        "providers": [{
            "slug": "alpha-cloud",
            "name": "Alpha",
            "authenticated": true,
            "models": ["m-small"]
        }]
    })
}

fn roster_time() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
        .expect("time")
        .with_timezone(&chrono::Utc)
}

fn minter() -> Arc<dyn TicketMinter> {
    Arc::new(CountingMinter::default())
}

fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git");
    assert!(status.success());
    dir
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
