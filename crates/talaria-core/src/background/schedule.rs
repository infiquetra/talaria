use super::*;

impl BackgroundWork {
    pub(super) fn request(&mut self) -> RequestId {
        self.next_request = self.next_request.wrapping_add(1).max(1);
        RequestId(self.next_request)
    }

    pub(super) fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub(super) fn list(&mut self, now_ms: u64, effects: &mut Vec<BackgroundEffect>) {
        if !self.foreground || matches!(self.list_state, ListState::Stopped { .. }) {
            return;
        }
        if self.list_in_flight.is_some() {
            self.list_again = true;
            return;
        }
        let request = self.request();
        self.list_in_flight = Some(request);
        self.last_list_sent_ms = Some(now_ms);
        effects.push(BackgroundEffect::ListProcesses { request });
    }

    pub(super) fn finish_list(&mut self, now_ms: u64, effects: &mut Vec<BackgroundEffect>) {
        self.list_in_flight = None;
        if std::mem::take(&mut self.list_again) {
            self.list(now_ms, effects);
        }
    }

    fn poll_interval(&self) -> Option<u64> {
        if !self.foreground || matches!(self.list_state, ListState::Stopped { .. }) {
            return None;
        }
        let selected = self.selected.as_ref().or_else(|| {
            self.overlay_open
                .then(|| self.rows().next().map(|row| &row.id))
                .flatten()
        });
        let overlay_running = self.overlay_open
            && selected.is_some_and(|id| {
                self.rows
                    .iter()
                    .any(|row| &row.id == id && row.status == ProcessStatus::Running)
            });
        let base = if overlay_running {
            OVERLAY_POLL_MS
        } else if self
            .rows
            .iter()
            .any(|row| row.status == ProcessStatus::Running)
            || matches!(self.list_state, ListState::Stale { .. })
        {
            PROCESS_POLL_MS
        } else {
            return None;
        };
        Some(
            base.saturating_mul(
                1u64.checked_shl(self.consecutive_list_failures)
                    .unwrap_or(u64::MAX),
            )
            .min(MAX_POLL_BACKOFF_MS),
        )
    }

    pub(super) fn clock(&mut self, now_ms: u64, wall_s: f64, effects: &mut Vec<BackgroundEffect>) {
        let previous_ms = self.last_frame_ms;
        self.now_ms = now_ms;
        self.wall_s = wall_s;
        let expire = self.rows.iter().any(|row| {
            row.finished_seen_ms.is_some_and(|at| {
                previous_ms < at.saturating_add(FINISHED_ROW_MS)
                    && now_ms >= at.saturating_add(FINISHED_ROW_MS)
            })
        });
        let second_changed = self
            .last_live_ms
            .is_none_or(|last| now_ms / 1_000 > last / 1_000);
        let mut changed = expire;
        if second_changed {
            self.last_live_ms = Some(now_ms);
            for row in &mut self.rows {
                let line = self
                    .outputs
                    .get(&row.id)
                    .map_or_else(String::new, ProcessOutput::last_line);
                if row.live_line != line {
                    row.live_line = line;
                    changed = true;
                }
            }
            let summary = self.summary();
            changed |= self.foreground
                && (summary.running > 0 || summary.finished_recently > 0 || summary.bg_running > 0);
        }
        changed |= self.overlay_open && self.foreground;
        self.last_frame_ms = now_ms;
        self.overlay_output_dirty = false;
        if changed {
            self.changed();
        }
        if let Some(interval) = self.poll_interval()
            && self
                .last_list_sent_ms
                .is_none_or(|sent| now_ms >= sent.saturating_add(interval))
        {
            self.list(now_ms, effects);
        }
    }
}

pub(super) fn deadline(work: &BackgroundWork, now_ms: u64) -> Option<u64> {
    if !work.foreground {
        return None;
    }
    let mut deadline = None;
    let mut include = |at| {
        deadline = Some(deadline.map_or(at, |old: u64| old.min(at)));
    };
    let timed = !work.bg_tasks.is_empty()
        || work.rows.iter().any(|row| {
            row.status == ProcessStatus::Running
                || row
                    .finished_seen_ms
                    .is_some_and(|at| now_ms < at.saturating_add(FINISHED_ROW_MS))
        });
    if timed || work.overlay_open {
        include((now_ms / 1_000).saturating_add(1).saturating_mul(1_000));
    }
    for row in &work.rows {
        if row.status != ProcessStatus::Running
            && let Some(at) = row.finished_seen_ms
            && now_ms < at.saturating_add(FINISHED_ROW_MS)
        {
            include(at.saturating_add(FINISHED_ROW_MS));
        }
    }
    if work.list_in_flight.is_none()
        && let Some(interval) = work.poll_interval()
    {
        include(
            work.last_list_sent_ms
                .map_or(now_ms, |sent| sent.saturating_add(interval))
                .max(now_ms),
        );
    }
    if work.overlay_open && work.overlay_output_dirty {
        include(now_ms.saturating_add(33));
    }
    deadline
}
