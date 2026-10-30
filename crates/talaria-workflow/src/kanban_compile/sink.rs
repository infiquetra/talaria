//! The HTTP adapter for the compiler's emission seam.
use super::*;
use hermes_client::kanban::{CreateTask, KanbanClient, TaskPatch};
impl From<&CreateCard> for CreateTask {
    fn from(c: &CreateCard) -> Self {
        Self {
            title: c.title.clone(),
            body: c.body.clone(),
            assignee: c.assignee.clone(),
            parents: c.parents.clone(),
            idempotency_key: c.idempotency_key.clone(),
            skills: c.skills.clone(),
            workspace_kind: c.workspace_kind.clone(),
            workspace_path: c.workspace_path.clone(),
            tenant: c.tenant.clone(),
            model_override: c.model_override.clone(),
            provider_override: c.provider_override.clone(),
            reasoning_effort: c.reasoning_effort.clone(),
        }
    }
}
impl CardSink for KanbanClient {
    fn warnings(&self) -> Vec<String> {
        self.take_warnings()
    }
    fn orchestration(&self) -> BoxFuture<'_, Result<String, SinkError>> {
        Box::pin(async move {
            Ok(KanbanClient::orchestration(self)
                .await
                .map_err(error)?
                .default_assignee)
        })
    }
    fn existing(&self) -> BoxFuture<'_, Result<Vec<ExistingCard>, SinkError>> {
        Box::pin(async move {
            let mut cards = Vec::new();
            for task in self
                .board_with(true)
                .await
                .map_err(error)?
                .columns
                .into_iter()
                .flat_map(|c| c.tasks)
            {
                let detail = self.task(&task.id).await.map_err(error)?;
                cards.push(ExistingCard {
                    id: task.id,
                    idempotency_key: task.idempotency_key.unwrap_or_default(),
                    archived: task.status == "archived",
                    status: task.status,
                    parents: detail.parent_ids,
                });
            }
            Ok(cards)
        })
    }
    fn create<'a>(&'a self, c: &'a CreateCard) -> BoxFuture<'a, Result<CreatedCard, SinkError>> {
        Box::pin(async move {
            let task = self.create_task(&c.into()).await.map_err(error)?;
            Ok(CreatedCard {
                id: task.id,
                status: task.status,
            })
        })
    }
    fn link<'a>(&'a self, parent: &'a str, child: &'a str) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move { KanbanClient::link(self, parent, child).await.map_err(error) })
    }
    fn complete_root<'a>(
        &'a self,
        id: &'a str,
        summary: &'a str,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.patch_task(
                id,
                &TaskPatch {
                    status: "done".into(),
                    summary: Some(summary.into()),
                },
            )
            .await
            .map_err(error)?;
            Ok(())
        })
    }
    fn dispatch(&self) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move { KanbanClient::dispatch(self).await.map_err(error) })
    }
}
fn error(e: impl std::fmt::Display) -> SinkError {
    SinkError(e.to_string())
}
