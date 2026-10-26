use super::*;

impl BackgroundWork {
    pub(super) fn bg_command(
        &mut self,
        text: String,
        now_ms: u64,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        let text = text.trim().to_owned();
        if text.is_empty() {
            effects.push(BackgroundEffect::InsertNotice { level: NoticeLevel::Info, text: "Usage: /bg <prompt> — runs the prompt on a fresh agent in the background; the answer appears here when it finishes.".to_owned() });
            return;
        }
        let request = self.request();
        self.bg_tasks.push(BgTask {
            request,
            text: text.clone(),
            sent_ms: now_ms,
            state: BgState::Starting,
        });
        self.changed();
        effects.push(BackgroundEffect::StartBackground { request, text });
    }

    pub(super) fn bg_accepted(
        &mut self,
        request: RequestId,
        task_id: String,
        now_ms: u64,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        let Some(task) = self
            .bg_tasks
            .iter_mut()
            .find(|task| task.request == request && matches!(task.state, BgState::Starting))
        else {
            return;
        };
        task.state = BgState::Running {
            task_id: task_id.clone(),
            accepted_ms: now_ms,
        };
        self.changed();
        effects.push(BackgroundEffect::InsertNotice { level: NoticeLevel::Info, text: format!("Background task {task_id} started. Its answer appears here when it finishes; it is not part of the conversation.") });
        if self.shown_task_ids.contains(&task_id) {
            self.bg_tasks.retain(|task| task.request != request);
            self.flush_early(effects);
            return;
        }
        if let Some(index) = self.early_results.iter().position(|(id, _)| id == &task_id)
            && let Some((id, text)) = self.early_results.remove(index)
        {
            self.bg_completed(id, text, effects);
        }
        self.flush_early(effects);
    }

    pub(super) fn bg_failed(
        &mut self,
        request: RequestId,
        failure: CallFailure,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        let Some(index) = self
            .bg_tasks
            .iter()
            .position(|task| task.request == request && matches!(task.state, BgState::Starting))
        else {
            return;
        };
        let lost = matches!(failure, CallFailure::Unknown | CallFailure::Unreadable);
        if lost {
            self.bg_tasks[index].state = BgState::ReplyLost;
        } else {
            self.bg_tasks.remove(index);
        }
        effects.push(BackgroundEffect::InsertNotice {
            level: if lost {
                NoticeLevel::Warning
            } else {
                NoticeLevel::Error
            },
            text: bg_failure_text(&failure),
        });
        if matches!(failure, CallFailure::Unreadable) {
            effects.push(BackgroundEffect::Log {
                text: "unreadable prompt.background reply".to_owned(),
            });
        }
        self.changed();
        self.flush_early(effects);
    }

    pub(super) fn bg_completed(
        &mut self,
        task_id: String,
        text: String,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        if self.shown_task_ids.contains(&task_id) {
            return;
        }
        if let Some(index) = self.bg_tasks.iter().position(
            |task| matches!(&task.state, BgState::Running { task_id: id, .. } if id == &task_id),
        ) {
            let task = self.bg_tasks.remove(index);
            self.result(task_id, text, Some(task.text), effects);
        } else if self
            .bg_tasks
            .iter()
            .any(|task| matches!(task.state, BgState::Starting))
        {
            if self.early_results.iter().any(|(id, _)| id == &task_id) {
                return;
            }
            self.early_results.push_back((task_id, text));
            if self.early_results.len() > MAX_EARLY_RESULTS
                && let Some((id, text)) = self.early_results.pop_front()
            {
                self.result(id, text, None, effects);
            }
        } else if let Some(index) = self
            .bg_tasks
            .iter()
            .position(|task| matches!(task.state, BgState::ReplyLost))
        {
            let task = self.bg_tasks.remove(index);
            self.result(task_id, text, Some(task.text), effects);
        } else {
            self.result(task_id, text, None, effects);
        }
    }

    pub(super) fn flush_early(&mut self, effects: &mut Vec<BackgroundEffect>) {
        if self
            .bg_tasks
            .iter()
            .any(|task| matches!(task.state, BgState::Starting))
        {
            return;
        }
        while let Some((id, text)) = self.early_results.pop_front() {
            self.result(id, text, None, effects);
        }
    }

    fn result(
        &mut self,
        task_id: String,
        text: String,
        prompt: Option<String>,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        if !self.shown_task_ids.insert(task_id.clone()) {
            return;
        }
        let result = match text.strip_prefix("error: ") {
            Some(rest) => BgResult::Failed(rest.to_owned()),
            None => BgResult::Answer(text),
        };
        let failed = matches!(result, BgResult::Failed(_));
        effects.push(BackgroundEffect::InsertResult {
            cell: BackgroundCell {
                task_id: task_id.clone(),
                origin: if prompt.is_some() {
                    BgOrigin::Here
                } else {
                    BgOrigin::Elsewhere
                },
                prompt,
                result,
            },
        });
        effects.push(BackgroundEffect::Signal(BackgroundSignal::BgFinished {
            task_id,
            failed,
        }));
        self.changed();
    }
}
