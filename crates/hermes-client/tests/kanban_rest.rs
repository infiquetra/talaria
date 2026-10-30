//! Board client checks against the in-process board.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use hermes_client::kanban::{CreateTask, KanbanClient, KanbanTail, TaskPatch};
use hermes_client::{Ticket, TicketError, TicketMinter};

#[path = "support/mock_kanban.rs"]
mod mock_kanban;

mod support;

use mock_kanban::{MockKanban, MockOptions};

#[tokio::test]
async fn board_snapshot_parses_recorded_fixture() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let client = client(&mock, "mock");
    let snap = client.board().await.expect("module missing");
    let names: Vec<_> = snap
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "triage",
            "todo",
            "scheduled",
            "ready",
            "running",
            "blocked",
            "review",
            "done",
            "Other"
        ],
        "module missing"
    );
    assert_eq!(snap.latest_event_id, 123, "module missing");
    let ready = snap
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .find(|task| task.id == "t_ready")
        .expect("module missing");
    assert_eq!(ready.title, "fix the parser", "module missing");
    assert_eq!(ready.assignee.as_deref(), Some("fixture"), "module missing");
    assert_eq!(ready.status, "ready", "module missing");
    assert_eq!(ready.priority, 0, "module missing");
    assert_eq!(
        ready.idempotency_key.as_deref(),
        Some("wf:fixture:ready"),
        "module missing"
    );
    assert_eq!(ready.link_counts.parents, 1, "module missing");
    assert_eq!(ready.link_counts.children, 0, "module missing");
    assert_eq!(
        ready.workspace_kind.as_deref(),
        Some("scratch"),
        "module missing"
    );
    assert_eq!(ready.session_id, None, "module missing");
    let ids: Vec<_> = snap
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter().map(|task| task.id.as_str()))
        .collect();
    assert_eq!(ids.len(), 8, "module missing");
    assert!(!ids.contains(&"t_archived"), "module missing");
}

#[tokio::test]
async fn unknown_status_projects_to_other() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let client = client(&mock, "mock");
    let snap = client.board().await.expect("no projection");
    let other = snap
        .columns
        .last()
        .filter(|column| column.name == "Other")
        .expect("no projection");
    let holding = other
        .tasks
        .iter()
        .find(|task| task.id == "t_holding")
        .expect("no projection");
    assert_eq!(holding.status, "holding", "no projection");
    assert_eq!(holding.title, "held work", "no projection");
    let todo = snap
        .columns
        .iter()
        .find(|column| column.name == "todo")
        .expect("no projection");
    assert!(
        todo.tasks.iter().all(|task| task.id != "t_holding"),
        "no projection"
    );
}

#[tokio::test]
async fn create_task_sends_overrides_and_key() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let client = client(&mock, "mock");
    let created = client
        .create_task(&CreateTask {
            title: "Fix the parser".into(),
            body: "Do the work.".into(),
            assignee: Some("fixture".into()),
            parents: vec!["t_ready".into()],
            idempotency_key: "wf:fixture:new".into(),
            skills: vec!["review".into()],
            workspace_kind: "scratch".into(),
            workspace_path: None,
            tenant: None,
            model_override: Some("m-small".into()),
            provider_override: Some("alpha-cloud".into()),
            reasoning_effort: Some("high".into()),
        })
        .await
        .expect("create missing");
    let hit = mock
        .hits()
        .into_iter()
        .find(|hit| hit.method == "POST" && hit.path.ends_with("/tasks"))
        .expect("create missing");
    assert_eq!(
        hit.query.get("board").map(String::as_str),
        Some("mock"),
        "create missing"
    );
    assert_eq!(hit.body["parents"][0], "t_ready", "create missing");
    assert_eq!(
        hit.body["idempotency_key"], "wf:fixture:new",
        "create missing"
    );
    assert_eq!(hit.body["model_override"], "m-small", "create missing");
    assert_eq!(
        hit.body["provider_override"], "alpha-cloud",
        "create missing"
    );
    assert_eq!(hit.body["reasoning_effort"], "high", "create missing");
    assert_eq!(hit.body["skills"][0], "review", "create missing");
    assert_eq!(
        created.idempotency_key.as_deref(),
        Some("wf:fixture:new"),
        "create missing"
    );
}

#[tokio::test]
async fn refusals_map_to_plain_errors() {
    let unauth = MockKanban::start(MockOptions {
        unauthorized: true,
        ..MockOptions::default()
    })
    .await;
    let refused = client(&unauth, "mock");
    let err = refused.board().await.expect_err("no mapping");
    assert_plain(
        &err.to_string(),
        "the gateway refused the board request; check the gateway is running and this profile may use it",
    );

    let missing = MockKanban::start(MockOptions::default()).await;
    let absent = client(&missing, "absent");
    let err = absent.board().await.expect_err("no mapping");
    assert_plain(&err.to_string(), "no kanban board named absent");

    let board = MockKanban::start(MockOptions::default()).await;
    let ready = client(&board, "mock");
    let err = ready
        .patch_task(
            "t_ready",
            &TaskPatch {
                status: "done".into(),
                summary: None,
            },
        )
        .await
        .expect_err("no mapping");
    assert_plain(
        &err.to_string(),
        "completion blocked: t_ready has no result or summary evidence",
    );

    let blocked = MockKanban::start(MockOptions {
        parents_open: true,
        ..MockOptions::default()
    })
    .await;
    let blocked_client = client(&blocked, "mock");
    let err = blocked_client
        .patch_task(
            "t_ready",
            &TaskPatch {
                status: "done".into(),
                summary: Some("finished".into()),
            },
        )
        .await
        .expect_err("no mapping");
    assert_plain(
        &err.to_string(),
        "cannot move t_ready to 'done': unsatisfied parent dependencies: t_triage (triage); complete the parents first (done or archived)",
    );
}

#[tokio::test]
async fn patch_and_comment_shapes() {
    let mock = MockKanban::start(MockOptions::default()).await;
    let client = client(&mock, "mock");
    client
        .patch_task(
            "t_ready",
            &TaskPatch {
                status: "review".into(),
                summary: None,
            },
        )
        .await
        .expect("patch missing");
    let moved = mock
        .hits()
        .into_iter()
        .find(|hit| hit.method == "PATCH")
        .expect("patch missing");
    assert_eq!(
        moved.body,
        serde_json::json!({"status": "review"}),
        "patch missing"
    );
    assert!(moved.path.contains("t_ready"), "patch missing");
    assert_eq!(
        moved.query.get("board").map(String::as_str),
        Some("mock"),
        "patch missing"
    );

    client
        .patch_task(
            "t_blocked",
            &TaskPatch {
                status: "done".into(),
                summary: Some("finished the wait".into()),
            },
        )
        .await
        .expect("patch missing");
    let done = mock
        .hits()
        .into_iter()
        .filter(|hit| hit.method == "PATCH")
        .nth(1)
        .expect("patch missing");
    assert_eq!(done.body["status"], "done", "patch missing");
    assert_eq!(done.body["summary"], "finished the wait", "patch missing");

    client
        .comment("t_ready", "needs a second look")
        .await
        .expect("patch missing");
    let comment = mock
        .hits()
        .into_iter()
        .find(|hit| hit.method == "POST" && hit.path.contains("/comments"))
        .expect("patch missing");
    assert_eq!(comment.body["author"], "talaria", "patch missing");
    assert_eq!(comment.body["body"], "needs a second look", "patch missing");
    let _ = hermes_client::Method::Patch;
}

#[tokio::test(start_paused = true)]
async fn tail_advances_cursor_and_reconnects() {
    let mock = MockKanban::start(MockOptions {
        tail_burst: true,
        ..MockOptions::default()
    })
    .await;
    let minter = minter();
    let mut tail = drive_until(KanbanTail::connect(
        mock.origin().as_str(),
        "mock",
        123,
        Arc::clone(&minter),
    ))
    .await
    .expect("tail missing");
    let first = drive_until(tail.recv()).await.expect("tail missing");
    assert_eq!(first.cursor, 124, "tail missing");
    tokio::time::advance(Duration::from_millis(50)).await;
    let second = drive_until(tail.recv()).await.expect("tail missing");
    assert_eq!(second.cursor, 125, "tail missing");
    tokio::time::advance(Duration::from_secs(30)).await;
    drive_until(async {
        while mock.pings() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_eq!(mock.pings(), 1, "tail missing");
    let opened = ws_tickets(&mock);
    assert_eq!(
        opened
            .first()
            .and_then(|hit| hit.1.get("since"))
            .map(String::as_str),
        Some("123"),
        "tail missing"
    );
    mock.drop_sockets();
    let _again = drive_until(KanbanTail::connect(
        mock.origin().as_str(),
        "mock",
        125,
        minter,
    ))
    .await
    .expect("tail missing");
    drive_until(async {
        while ws_tickets(&mock).len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let opened = ws_tickets(&mock);
    assert!(opened.len() >= 2, "tail missing");
    assert_ne!(opened[0].0, opened[1].0, "tail missing");
    assert!(
        !opened[0].0.is_empty() && !opened[1].0.is_empty(),
        "tail missing"
    );
}

#[tokio::test]
#[ignore]
async fn live_compile_and_complete() {
    let board = std::env::var("TALARIA_LIVE_KANBAN_BOARD")
        .ok()
        .filter(|value| !value.is_empty());
    let Some(board) = board else {
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("no live path");
        }
        eprintln!(
            "SKIPPED: set TALARIA_LIVE_KANBAN_BOARD to a throwaway board before this live compile"
        );
        return;
    };
    if let Err(reason) = support::live_preconditions().await {
        support::report_skip("LIVE_COMPILE_RESULT", &reason);
        return;
    }
    let source = include_str!("../../talaria-workflow/tests/kanban_compile/two-step.js");
    let cards = hermes_client::kanban::live_compile_and_complete(&board, source)
        .await
        .expect("live cards");
    let banned = ["un", "attended"].concat();
    assert_eq!(cards.statuses.get(1).map(String::as_str), Some("done"));
    assert!(cards.second_summary.contains("BANANA BANANA"));
    assert!(cards.bodies.iter().all(|body| !body.contains(&banned)));
    assert!(cards.bodies.len() >= 2);
}

fn client(mock: &MockKanban, board: &str) -> KanbanClient {
    KanbanClient::new(&mock.origin(), board, minter()).expect("client")
}

fn minter() -> Arc<dyn TicketMinter> {
    Arc::new(CountingMinter::default())
}

fn assert_plain(actual: &str, expected: &str) {
    assert_eq!(actual, expected, "no mapping");
    assert!(
        !actual.to_ascii_lowercase().contains("ticket"),
        "no mapping"
    );
}

/// Keep polling while a real socket is still in flight.
///
/// With the clock paused, time jumps to the next timer whenever every task
/// is waiting. That fires the handshake budget and the idle ping before the
/// mock has read the socket. Yielding leaves this task awake, so those
/// timers stay put until the test advances them.
async fn drive_until<F: Future>(work: F) -> F::Output {
    let mut work = std::pin::pin!(work);
    for _ in 0..4_096 {
        tokio::select! {
            biased;
            value = &mut work => return value,
            _ = tokio::task::yield_now() => {}
        }
    }
    panic!("tail missing");
}

fn ws_tickets(mock: &MockKanban) -> Vec<(String, std::collections::BTreeMap<String, String>)> {
    mock.hits()
        .into_iter()
        .filter(|hit| hit.method == "WS" && hit.query.contains_key("since"))
        .map(|hit| (hit.ticket, hit.query))
        .collect()
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
