use super::*;
use std::cmp::Ordering;

fn row_order(a: &ProcessRow, b: &ProcessRow) -> Ordering {
    match (
        a.status == ProcessStatus::Running,
        b.status == ProcessStatus::Running,
    ) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => match (a.started_epoch_s, b.started_epoch_s) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.first_seen_ms.cmp(&b.first_seen_ms),
        }
        .then_with(|| a.id.cmp(&b.id)),
        (false, false) => match (a.exited_at_s, b.exited_at_s) {
            (Some(a), Some(b)) => b.total_cmp(&a),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => b.finished_seen_ms.cmp(&a.finished_seen_ms),
        }
        .then_with(|| a.id.cmp(&b.id)),
    }
}

impl BackgroundWork {
    pub(super) fn apply(&mut self, event: BackgroundEvent) -> Vec<BackgroundEffect> {
        let previous_version = self.version;
        let mut effects = Vec::new();
        if matches!(
            event,
            BackgroundEvent::KillRequested { .. }
                | BackgroundEvent::StopAllRequested { .. }
                | BackgroundEvent::Dismiss { .. }
                | BackgroundEvent::RefreshRequested { .. }
                | BackgroundEvent::Selected { .. }
                | BackgroundEvent::OverlayClosed
        ) && self.overlay_note.take().is_some()
        {
            self.changed();
        }
        match event {
            BackgroundEvent::TerminalOutput {
                process_id,
                chunk,
                now_ms,
            } => {
                self.now_ms = now_ms;
                self.chunk(process_id, &chunk, now_ms, &mut effects);
            }
            BackgroundEvent::TerminalClosed { process_id } => {
                self.closed_view_ids.insert(process_id.clone());
                if let Some(row) = self.rows.iter_mut().find(|row| row.id.0 == process_id)
                    && !row.agent_closed_view
                {
                    row.agent_closed_view = true;
                    self.changed();
                }
            }
            BackgroundEvent::ToolCompleted { name, now_ms } => {
                self.now_ms = now_ms;
                if is_process_tool(&name) {
                    self.list(now_ms, &mut effects);
                }
            }
            BackgroundEvent::BgCompleted {
                task_id,
                text,
                now_ms,
            } => {
                self.now_ms = now_ms;
                self.bg_completed(task_id, text, &mut effects);
            }
            BackgroundEvent::Clock { now_ms, wall_s } => self.clock(now_ms, wall_s, &mut effects),
            BackgroundEvent::ForegroundChanged { foreground, now_ms } => {
                self.now_ms = now_ms;
                self.foreground = foreground;
                if foreground {
                    self.list(now_ms, &mut effects);
                }
            }
            BackgroundEvent::Attached { now_ms } => {
                self.now_ms = now_ms;
                if matches!(self.list_state, ListState::Stopped { .. }) {
                    self.list_state = ListState::Never;
                    self.changed();
                }
                self.list_in_flight = None;
                self.stop_in_flight = None;
                self.list_again = false;
                self.consecutive_list_failures = 0;
                let mut changed = false;
                for row in &mut self.rows {
                    if matches!(row.kill, KillState::Sending { .. }) {
                        row.kill = KillState::Note {
                            text: "Hermes did not confirm. Refreshing the list.".to_owned(),
                        };
                        changed = true;
                    }
                }
                for task in &mut self.bg_tasks {
                    if matches!(task.state, BgState::Starting) {
                        task.state = BgState::ReplyLost;
                        changed = true;
                    }
                }
                if changed {
                    self.changed();
                }
                self.flush_early(&mut effects);
                self.list(now_ms, &mut effects);
            }
            BackgroundEvent::Listed {
                request,
                processes,
                now_ms,
            } => {
                if self.list_in_flight != Some(request) {
                    return effects;
                }
                self.now_ms = now_ms;
                self.reconcile(processes, now_ms, &mut effects);
                self.consecutive_list_failures = 0;
                self.finish_list(now_ms, &mut effects);
            }
            BackgroundEvent::ListFailed {
                request,
                failure,
                now_ms,
            } => {
                if self.list_in_flight != Some(request) {
                    return effects;
                }
                self.now_ms = now_ms;
                self.list_failed(failure, now_ms, &mut effects);
                self.finish_list(now_ms, &mut effects);
            }
            BackgroundEvent::KillReplied {
                request,
                process_id,
                outcome,
                now_ms,
            } => {
                self.now_ms = now_ms;
                if let Some(row) = self
                    .rows
                    .iter_mut()
                    .find(|row| row.id == process_id && row.kill == KillState::Sending { request })
                {
                    if matches!(outcome, KillOutcome::Killed) {
                        let running = row.status == ProcessStatus::Running;
                        row.status = ProcessStatus::Killed;
                        row.exit_code = Some(-15);
                        row.finished_seen_ms.get_or_insert(now_ms);
                        if running {
                            effects.push(BackgroundEffect::Signal(
                                BackgroundSignal::ProcessFinished {
                                    id: process_id,
                                    status: ProcessStatus::Killed,
                                },
                            ));
                        }
                    }
                    row.kill = KillState::Note {
                        text: kill_note_text(&outcome),
                    };
                    if matches!(outcome, KillOutcome::Failed(CallFailure::Unreadable)) {
                        effects.push(BackgroundEffect::Log {
                            text: "unreadable process.kill reply".to_owned(),
                        });
                    }
                    self.rows.sort_by(row_order);
                    self.changed();
                    self.list(now_ms, &mut effects);
                }
            }
            BackgroundEvent::StopReplied {
                request,
                outcome,
                now_ms,
            } => {
                self.now_ms = now_ms;
                if let Some((pending, origin)) = self.stop_in_flight
                    && pending == request
                {
                    self.stop_in_flight = None;
                    let (level, text) = stop_notice_text(&outcome);
                    match origin {
                        StopOrigin::Command => {
                            effects.push(BackgroundEffect::InsertNotice { level, text })
                        }
                        StopOrigin::Overlay => {
                            self.overlay_note = Some(text);
                            self.changed();
                        }
                    }
                    if matches!(outcome, StopOutcome::Failed(CallFailure::Unreadable)) {
                        effects.push(BackgroundEffect::Log {
                            text: "unreadable process.stop reply".to_owned(),
                        });
                    }
                    self.list(now_ms, &mut effects);
                }
            }
            BackgroundEvent::BgAccepted {
                request,
                task_id,
                now_ms,
            } => {
                self.now_ms = now_ms;
                self.bg_accepted(request, task_id, now_ms, &mut effects);
            }
            BackgroundEvent::BgFailed {
                request,
                failure,
                now_ms,
            } => {
                self.now_ms = now_ms;
                self.bg_failed(request, failure, &mut effects);
            }
            BackgroundEvent::BgCommand { text, now_ms } => {
                self.now_ms = now_ms;
                self.bg_command(text, now_ms, &mut effects);
            }
            BackgroundEvent::StopCommand {
                turn_running,
                now_ms,
            } => {
                self.now_ms = now_ms;
                self.stop(StopOrigin::Command, turn_running, &mut effects);
            }
            BackgroundEvent::StopAllRequested { now_ms } => {
                self.now_ms = now_ms;
                self.stop(StopOrigin::Overlay, false, &mut effects);
            }
            BackgroundEvent::KillRequested { id, now_ms } => {
                self.now_ms = now_ms;
                self.kill(id, &mut effects);
            }
            BackgroundEvent::Dismiss { id } => {
                if let Some(index) = self.rows.iter().position(|row| row.id == id) {
                    if self.rows[index].status == ProcessStatus::Running {
                        self.overlay_note = Some(
                            "A running process cannot be dismissed. Kill it first with x."
                                .to_owned(),
                        );
                    } else {
                        self.dismissed.insert(id.clone());
                        self.rows.remove(index);
                        self.outputs.remove(&id);
                        self.fix_selection(index);
                    }
                    self.changed();
                }
            }
            BackgroundEvent::Selected { id } => {
                if self.rows.iter().any(|row| row.id == id) && self.selected.as_ref() != Some(&id) {
                    self.selected = Some(id);
                    self.changed();
                }
            }
            BackgroundEvent::RefreshRequested { now_ms } => {
                self.now_ms = now_ms;
                self.list(now_ms, &mut effects);
            }
            BackgroundEvent::OverlayOpened { now_ms } => {
                self.now_ms = now_ms;
                if !self.overlay_open {
                    self.overlay_open = true;
                    self.changed();
                }
                self.list(now_ms, &mut effects);
            }
            BackgroundEvent::OverlayClosed => {
                if self.overlay_open {
                    self.overlay_open = false;
                    self.overlay_output_dirty = false;
                    self.changed();
                }
            }
        }
        if self.version != previous_version {
            self.version = previous_version.wrapping_add(1);
        }
        effects
    }

    fn stop(
        &mut self,
        origin: StopOrigin,
        turn_running: bool,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        if self.stop_in_flight.is_some() {
            self.overlay_note = Some("Already stopping; waiting for Hermes.".to_owned());
            self.changed();
            return;
        }
        let request = self.request();
        self.stop_in_flight = Some((request, origin));
        if turn_running {
            effects.push(BackgroundEffect::Interrupt);
        }
        effects.push(BackgroundEffect::StopProcesses { request });
    }

    fn kill(&mut self, id: ProcessId, effects: &mut Vec<BackgroundEffect>) {
        let Some(index) = self.rows.iter().position(|row| row.id == id) else {
            return;
        };
        if self.rows[index].status != ProcessStatus::Running {
            self.rows[index].kill = KillState::Note {
                text: "This process has already ended.".to_owned(),
            };
        } else if matches!(self.rows[index].kill, KillState::Sending { .. }) {
            self.overlay_note =
                Some("Already killing this process; waiting for Hermes.".to_owned());
        } else {
            let request = self.request();
            self.rows[index].kill = KillState::Sending { request };
            effects.push(BackgroundEffect::KillProcess { request, id });
        }
        self.changed();
    }

    fn fix_selection(&mut self, previous_index: usize) {
        if self
            .selected
            .as_ref()
            .is_some_and(|selected| !self.rows.iter().any(|row| &row.id == selected))
        {
            self.selected = self
                .rows
                .get(previous_index.min(self.rows.len().saturating_sub(1)))
                .map(|row| row.id.clone());
        }
    }

    fn reconcile(
        &mut self,
        processes: Vec<ListedProcess>,
        now_ms: u64,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        let old_rows = self.rows.clone();
        let selected_index = self
            .selected
            .as_ref()
            .and_then(|id| self.rows.iter().position(|row| &row.id == id))
            .unwrap_or(0);
        // The open schema permits duplicate ids. Reconcile the last entry once, so an
        // identical list cannot manufacture running-to-finished transitions each time.
        let mut unique = Vec::<ListedProcess>::new();
        for entry in processes {
            if let Some(index) = unique.iter().position(|old| old.id == entry.id) {
                unique[index] = entry;
            } else {
                unique.push(entry);
            }
        }
        let ids: HashSet<_> = unique.iter().map(|entry| entry.id.clone()).collect();
        self.dismissed.retain(|id| ids.contains(id));
        let mut output_changed = false;
        for entry in unique {
            if self.dismissed.contains(&entry.id) {
                continue;
            }
            if !matches!(entry.status_raw.as_str(), "running" | "exited")
                && !self.unknown_status_logged
            {
                effects.push(BackgroundEffect::Log {
                    text: format!(
                        "unknown process status {} for {}",
                        entry.status_raw, entry.id.0
                    ),
                });
                self.unknown_status_logged = true;
            }
            let status = status_of(&entry);
            if let Some(row) = self.rows.iter_mut().find(|row| row.id == entry.id) {
                if row.status == ProcessStatus::Running && status != ProcessStatus::Running {
                    row.finished_seen_ms = Some(now_ms);
                    effects.push(BackgroundEffect::Signal(
                        BackgroundSignal::ProcessFinished {
                            id: row.id.clone(),
                            status: status.clone(),
                        },
                    ));
                }
                if status == ProcessStatus::Running {
                    row.finished_seen_ms = None;
                }
                row.command = entry.command;
                row.status = status;
                row.exit_code = entry.exit_code;
                row.started_epoch_s = entry.started_epoch_s;
                row.exited_at_s = entry.exited_at_s;
                row.misses = 0;
            } else {
                self.rows.push(ProcessRow {
                    id: entry.id.clone(),
                    command: entry.command,
                    finished_seen_ms: (status != ProcessStatus::Running).then_some(now_ms),
                    status,
                    exit_code: entry.exit_code,
                    started_epoch_s: entry.started_epoch_s,
                    exited_at_s: entry.exited_at_s,
                    first_seen_ms: now_ms,
                    misses: 0,
                    kill: KillState::Idle,
                    agent_closed_view: self.closed_view_ids.contains(&entry.id.0),
                    live_line: String::new(),
                });
            }
            if let Some(index) = self
                .pending_unknown
                .iter()
                .position(|(id, _)| id == &entry.id.0)
            {
                let (_, output) = self.pending_unknown.remove(index);
                output_changed |= !output.raw().is_empty();
                self.outputs.insert(entry.id.clone(), output);
            }
            if let Some(tail) = entry.output_tail.filter(|tail| !tail.is_empty()) {
                let output = self.outputs.entry(entry.id).or_default();
                let before = output.raw().to_owned();
                let trimmed = output.trimmed_front;
                output.merge_snapshot(&tail, now_ms);
                output_changed |= output.raw() != before || output.trimmed_front != trimmed;
            }
        }
        self.rows.retain_mut(|row| {
            if ids.contains(&row.id) {
                return true;
            }
            if row.status == ProcessStatus::Running {
                row.misses = row.misses.saturating_add(1);
                row.misses < 2
            } else {
                false
            }
        });
        self.outputs
            .retain(|id, _| self.rows.iter().any(|row| &row.id == id));
        self.rows.sort_by(row_order);
        self.fix_selection(selected_index);
        self.cap_outputs();
        let freshness_changed = matches!(
            self.list_state,
            ListState::Stale { .. } | ListState::Stopped { .. }
        );
        self.list_state = ListState::Fresh { at_ms: now_ms };
        // Miss counters are bookkeeping rather than a visible field.
        let rows_changed = self.rows.len() != old_rows.len()
            || self.rows.iter().zip(&old_rows).any(|(new, old)| {
                let mut old = old.clone();
                old.misses = new.misses;
                new != &old
            });
        if rows_changed || output_changed || freshness_changed {
            self.changed();
        }
    }

    fn chunk(
        &mut self,
        process_id: String,
        chunk: &str,
        now_ms: u64,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        if chunk.is_empty() {
            return;
        }
        let id = ProcessId(process_id.clone());
        if self.rows.iter().any(|row| row.id == id) {
            self.outputs
                .entry(id.clone())
                .or_default()
                .push_chunk(chunk, now_ms);
            if self.overlay_open
                && self
                    .selected
                    .as_ref()
                    .or_else(|| self.rows().next().map(|row| &row.id))
                    == Some(&id)
            {
                self.overlay_output_dirty = true;
                self.changed();
            }
            self.cap_outputs();
        } else {
            if let Some((_, output)) = self
                .pending_unknown
                .iter_mut()
                .find(|(id, _)| id == &process_id)
            {
                output.push_chunk(chunk, now_ms);
            } else {
                if self.pending_unknown.len() >= MAX_PENDING_UNKNOWN {
                    let oldest = self
                        .pending_unknown
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, (_, output))| output.last_write_ms)
                        .map(|(index, _)| index);
                    if let Some(index) = oldest {
                        self.pending_unknown.remove(index);
                    }
                }
                let mut output = ProcessOutput::default();
                output.push_chunk(chunk, now_ms);
                self.pending_unknown.push((process_id, output));
            }
            self.cap_outputs();
            if self.foreground
                && self
                    .last_unknown_list_ms
                    .is_none_or(|last| now_ms.saturating_sub(last) >= UNKNOWN_CHUNK_LIST_MS)
            {
                self.last_unknown_list_ms = Some(now_ms);
                self.list(now_ms, effects);
            }
        }
    }

    fn cap_outputs(&mut self) {
        self.output_total_chars = self
            .outputs
            .values()
            .map(|output| output.chars)
            .sum::<usize>()
            + self
                .pending_unknown
                .iter()
                .map(|(_, output)| output.chars)
                .sum::<usize>();
        let protected = self.selected.clone().or_else(|| {
            self.overlay_open
                .then(|| self.rows().next().map(|row| row.id.clone()))
                .flatten()
        });
        while self.outputs.len() > MAX_TRACKED_OUTPUTS
            || self.output_total_chars > OUTPUT_TOTAL_CHARS
        {
            let known = self
                .outputs
                .iter()
                .filter(|(id, _)| Some(*id) != protected.as_ref())
                .min_by(|(a_id, a), (b_id, b)| {
                    a.last_write_ms
                        .cmp(&b.last_write_ms)
                        .then_with(|| a_id.cmp(b_id))
                })
                .map(|(id, output)| (id.clone(), output.last_write_ms));
            let pending = self
                .pending_unknown
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, output))| output.last_write_ms)
                .map(|(index, (_, output))| (index, output.last_write_ms));
            if self.outputs.len() <= MAX_TRACKED_OUTPUTS
                && pending
                    .is_some_and(|(_, at)| known.as_ref().is_none_or(|(_, other)| at < *other))
            {
                if let Some((index, _)) = pending {
                    self.output_total_chars -= self.pending_unknown.remove(index).1.chars;
                }
            } else if let Some((id, _)) = known {
                if let Some(output) = self.outputs.remove(&id) {
                    self.output_total_chars -= output.chars;
                }
            } else {
                break;
            }
        }
    }

    fn list_failed(
        &mut self,
        failure: CallFailure,
        now_ms: u64,
        effects: &mut Vec<BackgroundEffect>,
    ) {
        let stop = match &failure {
            CallFailure::MethodMissing => Some(ListStop::MethodMissing),
            CallFailure::Refused { reason, .. } => match reason.as_str() {
                "not_found" => Some(ListStop::SessionNotLive),
                "unsupported_projection" => Some(ListStop::Unsupported),
                "permission_denied" => Some(ListStop::PermissionDenied),
                _ => None,
            },
            _ => None,
        };
        let reason = match &failure {
            CallFailure::Refused { reason, .. } => reason.clone(),
            CallFailure::MethodMissing => "method_missing".to_owned(),
            CallFailure::Storage => "storage_unavailable".to_owned(),
            CallFailure::Unknown => "unknown".to_owned(),
            CallFailure::NotSent => "not_sent".to_owned(),
            CallFailure::Unreadable => "unreadable".to_owned(),
        };
        let state = if let Some(why) = stop {
            ListState::Stopped { why }
        } else {
            self.consecutive_list_failures = self.consecutive_list_failures.saturating_add(1);
            let since_ms = match self.list_state {
                ListState::Fresh { at_ms } => at_ms,
                ListState::Stale { since_ms, .. } => since_ms,
                _ => now_ms,
            };
            ListState::Stale {
                since_ms,
                why: reason.clone(),
            }
        };
        if self.list_state != state {
            self.list_state = state;
            self.changed();
        }
        if self.logged_list_reasons.insert(reason.clone()) {
            let bug = if reason == "invalid_params" {
                " (Talaria bug)"
            } else {
                ""
            };
            effects.push(BackgroundEffect::Log {
                text: format!("process.list failed: {reason}{bug}"),
            });
        }
    }
}
