//! Board effects return through the application's existing message channel.
use hermes_client::{
    TicketMinter,
    kanban::{BoardSnapshot, KanbanClient, KanbanTail, KanbanTask, TaskDetail, TaskPatch},
};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use talaria_core::kanban::{
    self, BoardCard, CardComment, KanbanBoardState, KanbanEffect, KanbanInput, MoveTable,
};
use talaria_workflow::kanban_compile::PlannedCard;
use tokio::{sync::mpsc, task::JoinHandle};
pub fn cards_to_board(cards: &[PlannedCard]) -> Result<KanbanBoardState, String> {
    let mut state = KanbanBoardState::blank();
    state.load_cards(
        cards
            .iter()
            .map(|c| BoardCard {
                id: c.task_id.clone(),
                title: c.title.clone(),
                assignee: c.assignee.clone().unwrap_or_default(),
                status: c.status.clone(),
                created_at_ms: 0,
                started_at_ms: None,
                parent_titles: c
                    .parent_ids
                    .iter()
                    .filter_map(|id| cards.iter().find(|p| p.task_id == *id))
                    .map(|p| (p.title.clone(), p.status.clone()))
                    .collect(),
                child_titles: cards
                    .iter()
                    .filter(|child| child.parent_ids.contains(&c.task_id))
                    .map(|p| (p.title.clone(), p.status.clone()))
                    .collect(),
                body: c.body.clone(),
                latest_summary: None,
                comments: vec![],
            })
            .collect(),
    );
    Ok(state)
}
fn ms(seconds: Option<i64>) -> u64 {
    (seconds.unwrap_or_default().max(0) as u64).saturating_mul(1000)
}
pub fn card(task: &KanbanTask) -> BoardCard {
    BoardCard {
        id: task.id.clone(),
        title: task.title.clone(),
        assignee: task.assignee.clone().unwrap_or_default(),
        status: task.status.clone(),
        created_at_ms: ms(task.created_at),
        started_at_ms: task.current_run_started_at.map(|t| ms(Some(t))),
        parent_titles: vec![],
        child_titles: if task.link_counts.children > 0 {
            vec![("loading children".into(), "unknown".into())]
        } else {
            vec![]
        },
        body: task.body.clone(),
        latest_summary: task.latest_summary.clone(),
        comments: vec![],
    }
}
pub fn project(snapshot: &BoardSnapshot) -> Vec<BoardCard> {
    snapshot
        .columns
        .iter()
        .flat_map(|c| c.tasks.iter())
        .filter(|t| t.status != "archived")
        .map(card)
        .collect()
}
pub fn detail(detail: TaskDetail, now_ms: u64) -> KanbanInput {
    let mut base = card(&detail.task);
    let linked = |ids: &[String]| {
        ids.iter()
            .map(|id| {
                detail
                    .link_tasks
                    .iter()
                    .find(|v| v["id"].as_str() == Some(id))
                    .map(|v| {
                        (
                            v["title"].as_str().unwrap_or(id).into(),
                            v["status"].as_str().unwrap_or("unknown").into(),
                        )
                    })
                    .unwrap_or_else(|| (id.clone(), "unknown".into()))
            })
            .collect::<Vec<_>>()
    };
    base.parent_titles = linked(&detail.parent_ids);
    base.child_titles = linked(&detail.child_ids);
    base.comments = detail
        .comments
        .iter()
        .map(|v| CardComment {
            author: v["author"].as_str().unwrap_or_default().into(),
            body: v["body"].as_str().unwrap_or_default().into(),
            age: kanban::age(now_ms.saturating_sub(ms(v["created_at"].as_i64()))),
        })
        .collect();
    let text = |v: &str| kanban::truncate(v);
    let parents = |links: &[(String, String)]| {
        if links.is_empty() {
            "(none)".into()
        } else {
            links
                .iter()
                .map(|(t, s)| format!("{t} ({s})"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut fields = vec![
        format!("Title: {}", text(&base.title)),
        format!("Body: {}", text(&base.body)),
        format!("Assignee: {}", text(&base.assignee)),
        format!("Status: {}", base.status),
        format!(
            "Overrides: model={} provider={}",
            detail.task.model_override.as_deref().unwrap_or("-"),
            detail.task.provider_override.as_deref().unwrap_or("-")
        ),
        format!(
            "Effort: {}",
            detail.task.reasoning_effort.as_deref().unwrap_or("-")
        ),
        format!(
            "Workspace: {} {}",
            detail.task.workspace_kind.as_deref().unwrap_or("scratch"),
            detail.task.workspace_path.as_deref().unwrap_or("")
        ),
        format!("Parents: {}", text(&parents(&base.parent_titles))),
        format!("Children: {}", text(&parents(&base.child_titles))),
    ];
    if let Some(summary) = &base.latest_summary {
        fields.push(format!("Latest result: {}", text(summary)));
    }
    for comment in &base.comments {
        fields.push(format!("{} · {}", text(&comment.author), comment.age));
        fields.push(text(&comment.body));
    }
    for run in detail.runs {
        let f = |k: &str| {
            run[k]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| run[k].to_string())
        };
        fields.push(format!(
            "Run: {} {} {} {}",
            f("status"),
            f("outcome"),
            f("started_at"),
            f("ended_at")
        ));
    }
    fields.push(format!("Attachments: {}", detail.attachments.len()));
    KanbanInput::Detail { card: base, fields }
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
pub struct KanbanService {
    origin: String,
    minter: Arc<dyn TicketMinter>,
    tail: Option<JoinHandle<()>>,
}
impl KanbanService {
    pub fn new(origin: String, minter: Arc<dyn TicketMinter>) -> Self {
        Self {
            origin,
            minter,
            tail: None,
        }
    }
    pub fn execute(
        &mut self,
        board: &str,
        effect: KanbanEffect,
        sender: mpsc::UnboundedSender<crate::app::AppMessage>,
    ) {
        if matches!(effect, KanbanEffect::StopTail | KanbanEffect::Close) {
            if let Some(task) = self.tail.take() {
                task.abort();
            }
            return;
        }
        let is_tail = matches!(effect, KanbanEffect::StartTail { .. });
        let is_fetch = matches!(effect, KanbanEffect::Fetch { .. });
        let board = board.to_owned();
        let origin = self.origin.clone();
        let minter = self.minter.clone();
        let task = tokio::spawn(async move {
            let send = |input| {
                let _ = sender.send(crate::app::AppMessage::Kanban {
                    board: board.clone(),
                    input,
                });
            };
            if let KanbanEffect::StartTail { cursor } = effect {
                let mut tail = None;
                for attempt in 0..3 {
                    if attempt > 0 {
                        tokio::time::sleep(std::time::Duration::from_secs(1 << (attempt - 1)))
                            .await;
                    }
                    if let Ok(opened) =
                        KanbanTail::connect(&origin, &board, cursor, minter.clone()).await
                    {
                        tail = Some(opened);
                        break;
                    }
                }
                if let Some(mut tail) = tail {
                    send(KanbanInput::Tail {
                        cursor,
                        events: false,
                        now_ms: now_ms(),
                    });
                    while let Ok(batch) = tail.recv().await {
                        send(KanbanInput::Tail {
                            cursor: batch.cursor,
                            events: !batch.events.is_empty(),
                            now_ms: now_ms(),
                        });
                    }
                }
                send(KanbanInput::TailParked);
                return;
            }
            let client = match KanbanClient::new(&origin, &board, minter) {
                Ok(c) => c,
                Err(e) => {
                    send(KanbanInput::Failed(e.to_string()));
                    return;
                }
            };
            let result = async {
                match effect {
                    KanbanEffect::Fetch {
                        workflow,
                        detail: open,
                    } => {
                        if workflow {
                            let (snapshot, workflow) =
                                tokio::join!(client.board(), client.workflow());
                            let (table, labels) = match workflow {
                                Ok(w) => (
                                    Some(MoveTable {
                                        columns: w.columns.iter().map(|c| c.key.clone()).collect(),
                                        manual: w.manual,
                                    }),
                                    w.columns.into_iter().map(|c| (c.key, c.label)).collect(),
                                ),
                                Err(_) => (None, BTreeMap::new()),
                            };
                            send(KanbanInput::Workflow { table, labels });
                            let snapshot = snapshot?;
                            send(KanbanInput::Snapshot {
                                cards: project(&snapshot),
                                cursor: snapshot.latest_event_id,
                            });
                        } else {
                            let snapshot = client.board().await?;
                            send(KanbanInput::Snapshot {
                                cards: project(&snapshot),
                                cursor: snapshot.latest_event_id,
                            });
                        }
                        if let Some(id) = open {
                            send(detail(client.task(&id).await?, now_ms()));
                        }
                    }
                    KanbanEffect::Detail(id) => send(detail(client.task(&id).await?, now_ms())),
                    KanbanEffect::Boards => send(KanbanInput::Boards(
                        client
                            .boards()
                            .await?
                            .boards
                            .into_iter()
                            .map(|b| b.slug)
                            .collect(),
                    )),
                    KanbanEffect::Patch {
                        id,
                        status,
                        summary,
                    } => {
                        client
                            .patch_task(&id, &TaskPatch { status, summary })
                            .await?;
                        send(KanbanInput::Mutated);
                    }
                    KanbanEffect::Comment { id, body } => {
                        client.comment(&id, &body).await?;
                        send(KanbanInput::Mutated);
                    }
                    KanbanEffect::Dispatch => {
                        client.dispatch().await?;
                        send(KanbanInput::Notice("nudge: dispatcher nudged".into()));
                    }
                    KanbanEffect::StartTail { .. }
                    | KanbanEffect::StopTail
                    | KanbanEffect::Close => {}
                }
                Ok::<_, hermes_client::kanban::KanbanError>(())
            }
            .await;
            if let Err(e) = result {
                send(KanbanInput::Failed(e.to_string()));
            }
            if is_fetch {
                send(KanbanInput::FetchFinished { now_ms: now_ms() });
            }
        });
        if is_tail && let Some(old) = self.tail.replace(task) {
            old.abort();
        }
    }
}
impl Drop for KanbanService {
    fn drop(&mut self) {
        if let Some(task) = self.tail.take() {
            task.abort();
        }
    }
}
