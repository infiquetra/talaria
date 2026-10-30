//! The acceptance server publishes changes through the same tail as the board view.
use hermes_client::{
    Ticket, TicketError, TicketMinter,
    kanban::{KanbanClient, KanbanTail},
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
#[path = "support/mock_kanban.rs"]
mod mock_kanban;
#[derive(Default)]
struct Minter(AtomicUsize);
impl TicketMinter for Minter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async move {
            Ticket::parse(format!(
                "test-grant-{}",
                self.0.fetch_add(1, Ordering::SeqCst)
            ))
        })
    }
}
#[tokio::test]
async fn advance_publishes_status_changes_and_snapshot_cursor() {
    let mock = mock_kanban::MockKanban::start(Default::default()).await;
    let minter: Arc<dyn TicketMinter> = Arc::new(Minter::default());
    let client = KanbanClient::new(&mock.origin(), "mock", minter.clone()).unwrap();
    let snapshot = client.board().await.unwrap();
    let mut tail = KanbanTail::connect(&mock.origin(), "mock", snapshot.latest_event_id, minter)
        .await
        .unwrap();
    mock.advance();
    let batch = tokio::time::timeout(Duration::from_secs(2), tail.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(batch.events.len(), 2);
    assert_eq!(batch.cursor, 125);
    let snapshot = client.board().await.unwrap();
    assert_eq!(snapshot.latest_event_id, 125);
    let cards: Vec<_> = snapshot.columns.into_iter().flat_map(|c| c.tasks).collect();
    assert_eq!(
        cards.iter().find(|c| c.id == "t_running").unwrap().status,
        "done"
    );
    assert_eq!(
        cards.iter().find(|c| c.id == "t_ready").unwrap().status,
        "running"
    );
}
