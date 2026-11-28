use super::{
    FAILED_LISTED, HeaderRole, ProgressKey, WorkflowTheme, agent_prompt, clean, finished_counts,
    map::{Row, Tone},
    put, result_notices, selected_agent, selected_subagent, wait_text, warning_title,
    workflow_block,
};
use crate::theme::surfaces::ViewStyles;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::Widget,
};
use talaria_core::background::format_elapsed;
use talaria_workflow::{
    progress::{
        AgentState, Counts, RunState, format_money, format_tokens,
        live::{NodeKey, ProgressState},
    },
    tokens::tokens_text,
};

pub(super) struct Overview {
    pub(super) map: Rect,
    pub(super) inspector: Rect,
    pub(super) footer: Rect,
    top: Rect,
    notices: Vec<(String, Tone)>,
    evidence_links: Vec<(usize, ProgressKey)>,
}

impl Overview {
    pub(super) fn new(inner: Rect, state: &ProgressState) -> Option<Self> {
        if inner.width < 104 || inner.height < 16 {
            return None;
        }
        let run = &state.progress.run;
        let mut notices = Vec::new();
        if state.no_live_runner {
            notices.push((
                format!(
                    "Runner stopped · talaria workflow resume {} continues it",
                    run.run_id
                ),
                Tone::Warning,
            ));
        } else if let Some(status) = &state.run_suffix {
            notices.push((status.clone(), Tone::Warning));
        }
        if !state.connected {
            notices.push(("Journal only · gateway disconnected".into(), Tone::Warning));
        }
        let room = usize::from(inner.height)
            .saturating_sub(17 + notices.len())
            .clamp(1, FAILED_LISTED);
        let mut evidence_links = Vec::new();
        for (text, role) in result_notices(state, room) {
            let tone = match role {
                HeaderRole::Evidence(key) => {
                    evidence_links.push((notices.len(), key));
                    Tone::Normal
                }
                _ => Tone::Warning,
            };
            notices.push((text, tone));
        }
        if state.progress.last_seq == 0 {
            notices.push(("run starting — nothing journaled yet".into(), Tone::Muted));
        }
        if let Some(previous) = run.previous_execution {
            notices.push((
                format!("Previous execution · {}", previous.as_str()),
                Tone::Muted,
            ));
        }
        let top_height = 3 + u16::try_from(notices.len()).unwrap_or_default();
        // The panel needs one complete card underneath its breadcrumb and viewport status.
        if inner.height < top_height + 2 + 7 {
            return None;
        }
        let top = Rect::new(inner.x, inner.y, inner.width, top_height);
        let footer = Rect::new(inner.x, inner.bottom() - 2, inner.width, 2);
        let body = Rect::new(inner.x, top.bottom(), inner.width, footer.y - top.bottom());
        let inspector_width = (inner.width / 3).clamp(31, 47);
        let map = Rect::new(
            body.x,
            body.y,
            body.width - inspector_width - 1,
            body.height,
        );
        let inspector = Rect::new(map.right() + 1, body.y, inspector_width, body.height);
        Some(Self {
            map,
            inspector,
            footer,
            top,
            notices,
            evidence_links,
        })
    }

    pub(super) fn evidence_at(&self, column: u16, row: u16) -> Option<ProgressKey> {
        if !(self.top.x..self.top.right()).contains(&column) {
            return None;
        }
        let offset = usize::from(row.checked_sub(self.top.y + 3)?);
        self.evidence_links
            .iter()
            .find(|(index, _)| *index == offset)
            .map(|(_, key)| key.clone())
    }

    pub(super) fn draw(
        &self,
        state: &ProgressState,
        theme: &WorkflowTheme,
        styles: Option<&ViewStyles>,
        buf: &mut Buffer,
    ) {
        let run = &state.progress.run;
        let counts = &run.counts;
        let heading = theme.normal.add_modifier(Modifier::BOLD);
        let style = |tone: Tone| tone.style(theme, styles);
        let left_width = (u32::from(self.top.width) * 29 / 100) as u16;
        let middle_width = (u32::from(self.top.width) * 36 / 100) as u16;
        let name = Rect::new(self.top.x + 1, self.top.y, left_width - 2, 3);
        let agents = Rect::new(self.top.x + left_width + 1, self.top.y, middle_width - 2, 3);
        let usage = Rect::new(
            self.top.x + left_width + middle_width + 1,
            self.top.y,
            self.top.width - left_width - middle_width - 2,
            3,
        );
        for divider in [name.right(), agents.right()] {
            let bar = styles.map_or("│", |styles| styles.border_symbol("│"));
            for y in self.top.y..self.top.y + 3 {
                buf[(divider, y)].set_symbol(bar).set_style(theme.muted);
            }
        }
        let summary = Row::of(state, &NodeKey::Run);
        put(name, 0, &summary.label, heading, buf);
        put(name, 1, &summary.status, style(summary.tone), buf);
        if let Some(elapsed) = run_elapsed(state) {
            let label = if run.state == RunState::Running {
                "Elapsed"
            } else {
                "Finished in"
            };
            put(name, 2, &format!("{label} {elapsed}"), theme.muted, buf);
        }
        if run.state == RunState::Running {
            put(
                agents,
                0,
                &format!("Agents  {}/{} done", counts.done, counts.total),
                heading,
                buf,
            );
            let length = usize::from(agents.width.min(28));
            if let Some(done) = counts.done.saturating_mul(length).checked_div(counts.total) {
                let bar = Line::from(vec![
                    Span::styled("━".repeat(done), style(Tone::Success)),
                    Span::styled("─".repeat(length.saturating_sub(done)), theme.muted),
                ]);
                buf.set_line(agents.x, agents.y + 1, &bar, agents.width);
            }
            let failed = counts.failed.saturating_sub(run.budget_stopped);
            let buckets = [
                (counts.running, "working", Tone::Running),
                (counts.needs_you + counts.ended, "attention", Tone::Warning),
                (failed, "failed", Tone::Failure),
                (counts.queued, "queued", Tone::Muted),
                (counts.skipped, "skipped", Tone::Muted),
                (run.refused, "refused", Tone::Warning),
                (run.budget_stopped, "stopped", Tone::Warning),
                (
                    state
                        .progress
                        .agents()
                        .iter()
                        .filter(|agent| agent.cut_short.is_some())
                        .count(),
                    "cut short",
                    Tone::Warning,
                ),
            ];
            let mut spans = Vec::new();
            for (count, label, tone) in buckets {
                if count > 0 {
                    if !spans.is_empty() {
                        spans.push(Span::styled(" · ", theme.muted));
                    }
                    spans.push(Span::styled(format!("{count} {label}"), style(tone)));
                }
            }
            buf.set_line(agents.x, agents.y + 2, &Line::from(spans), agents.width);
        } else {
            put(agents, 0, "Agents", heading, buf);
            let summary = finished_counts(state);
            for (index, line) in crate::history::word_wrapped(&summary, agents.width)
                .iter()
                .take(2)
                .enumerate()
            {
                put(agents, index as u16 + 1, line, theme.normal, buf);
            }
        }
        put(usage, 0, "Usage", heading, buf);
        let tokens = if run.tokens_unknown {
            "Tokens unknown".into()
        } else if run.cache_unknown_turns > 0 {
            format!("Up to {} tokens · partial cache", format_tokens(run.tokens))
        } else {
            tokens_text(run.tokens, run.cache_read, 0, format_tokens)
        };
        put(usage, 1, &tokens, theme.normal, buf);
        let money = state.scheduler.as_ref().and_then(|live| live.spend_money);
        let mut cost = format_money(money.or(run.money));
        if money.is_none() && run.unpriced > 0 {
            cost.push_str(&format!(" · {} unpriced", run.unpriced));
        }
        put(usage, 2, &cost, theme.muted, buf);
        for (index, (notice, tone)) in self.notices.iter().enumerate() {
            put(self.top, 3 + index as u16, notice, style(*tone), buf);
        }
    }
}

pub(super) fn run_elapsed(state: &ProgressState) -> Option<String> {
    let run = &state.progress.run;
    let start = run.started_at_ms?;
    let end = match run.finished_at_ms {
        Some(finish) => finish,
        None if run.state == RunState::Running => state.now_ms,
        None => return None,
    };
    (start > 0 && end >= start).then(|| {
        let seconds = (end - start) as u64 / 1_000;
        if run.finished_at_ms.is_some() && seconds >= 3600 {
            let minutes = (seconds + 30) / 60;
            format!("{}h {}m", minutes / 60, minutes % 60)
        } else {
            format_elapsed(seconds)
        }
    })
}

pub(super) fn draw_inspector(
    area: Rect,
    state: &ProgressState,
    theme: &WorkflowTheme,
    styles: Option<&ViewStyles>,
    buf: &mut Buffer,
) {
    let block = workflow_block(" Inspector ", theme, styles).border_style(theme.muted);
    let inner = block.inner(area);
    block.render(area, buf);
    let inner = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    let row = Row::of(state, &state.selection);
    put(
        inner,
        0,
        &row.label,
        theme.normal.add_modifier(Modifier::BOLD),
        buf,
    );
    put(inner, 1, &row.status, row.tone.style(theme, styles), buf);

    let mut fields = Vec::new();
    let note = if let Some(agent) = selected_agent(state) {
        if let Some(child) = selected_subagent(state, agent) {
            if let Some(model) = &child.model {
                fields.push(("Model", model.clone()));
            }
            if let Some(duration) = child
                .duration_s
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
            {
                fields.push(("Elapsed", format!("{duration:.1}s")));
            }
            if let Some(tools) = child.tool_count.filter(|tools| *tools >= 0) {
                fields.push(("Tool calls", tools.to_string()));
            }
            if let Some(tokens) = child
                .input_tokens
                .and_then(|tokens| u64::try_from(tokens).ok())
            {
                fields.push(("Input", format!("{} tokens", format_tokens(tokens))));
            }
            if let Some(tokens) = child
                .output_tokens
                .and_then(|tokens| u64::try_from(tokens).ok())
            {
                fields.push(("Output", format!("{} tokens", format_tokens(tokens))));
            }
            if let Some(tool) = &child.last_tool {
                fields.push(("Last tool", tool.clone()));
            }
            child
                .activity
                .back()
                .map(|activity| ("Latest activity", activity.text.as_str()))
                .or_else(|| child.summary.as_deref().map(|summary| ("Summary", summary)))
        } else {
            if let Some(model) = &agent.model {
                fields.push(("Model", model.clone()));
            }
            if let Some(provider) = &agent.provider {
                fields.push(("Provider", provider.clone()));
            }
            if let Some(effort) = &agent.effort {
                fields.push(("Effort", effort.clone()));
            }
            if agent.state != AgentState::Queued
                && agent.started_at_ms > 0
                && agent.finished_at_ms.unwrap_or(state.now_ms) >= agent.started_at_ms
            {
                fields.push((
                    "Elapsed",
                    format!("{:.1}s", agent.duration_ms(state.now_ms) as f64 / 1_000.0),
                ));
            }
            if agent.tokens_unknown {
                fields.push(("Tokens", "unknown".into()));
            } else {
                fields.push(("Tokens", format_tokens(agent.tokens)));
                let cache = match (agent.cache_read, agent.cache_unknown_turns) {
                    (known, 0) => format_tokens(known),
                    (0, _) => "unknown".into(),
                    (partial, _) => format!("{} (partial)", format_tokens(partial)),
                };
                fields.push(("Cache reads", cache));
            }
            fields.push(("Cost", format_money(agent.money)));
            if let Some(tool) = &agent.last_tool {
                fields.push(("Last tool", tool.clone()));
            }
            if let Some(file) = &agent.recovered_report {
                Some(("Saved report · d open", file.as_str()))
            } else if let Some(warning) = warning_title(agent) {
                Some(("Review warning · d details", warning))
            } else if let Some(reason) = agent.attention.as_deref().or(agent.failure.as_deref()) {
                Some(("Needs attention", reason))
            } else if let Some(wait) = state.waits.get(&agent.key) {
                Some(("Scheduler", wait_text(wait)))
            } else if let Some(cap) = agent.cut_short.as_deref() {
                Some(("Step limit", cap))
            } else if let Some(output) = agent.tail.back() {
                Some(("Latest output", output.as_str()))
            } else {
                Some(("Prompt", agent_prompt(state, agent)))
            }
        }
    } else {
        let run = &state.progress.run;
        match &state.selection {
            NodeKey::Run => {
                fields.push(("Run ID", run.run_id.clone()));
                run.logs
                    .back()
                    .map(|log| ("Latest workflow log", log.as_str()))
            }
            NodeKey::Phase { path, title } => {
                if let Some(phase) = state
                    .progress
                    .phases
                    .iter()
                    .find(|p| &p.workflow_path == path && &p.title == title)
                {
                    fields.extend(count_fields(&phase.counts));
                }
                None
            }
            NodeKey::Child(path) => {
                let mut counts = Counts::default();
                for agent in state
                    .progress
                    .agents()
                    .iter()
                    .filter(|agent| agent.workflow_path.starts_with(path))
                {
                    counts.add(agent.state);
                }
                fields.push(("Done", format!("{}/{} agents", counts.done, counts.total)));
                fields.extend(count_fields(&counts));
                None
            }
            _ => None,
        }
    };
    let mut y = 3;
    let reserved = if note.is_some() && inner.height >= 8 {
        3
    } else {
        0
    };
    for (label, value) in fields
        .into_iter()
        .take(usize::from(inner.height.saturating_sub(y + reserved)))
    {
        let first = Span::styled(format!("{label:<11} "), theme.muted);
        let value = Span::styled(
            clean(&value, inner.width.saturating_sub(first.width() as u16)),
            theme.normal,
        );
        buf.set_line(
            inner.x,
            inner.y + y,
            &Line::from(vec![first, value]),
            inner.width,
        );
        y += 1;
    }
    if let Some((label, text)) = note {
        put(
            inner,
            y,
            label,
            theme.muted.add_modifier(Modifier::BOLD),
            buf,
        );
        for text in crate::history::word_wrapped(&clean(text, 1024), inner.width)
            .iter()
            .take(usize::from(inner.height.saturating_sub(y + 1)))
        {
            y += 1;
            put(inner, y, text, theme.normal, buf);
        }
    }
}

fn count_fields(counts: &Counts) -> impl Iterator<Item = (&'static str, String)> {
    [
        ("Working", counts.running),
        ("Queued", counts.queued),
        ("Needs you", counts.needs_you),
        ("Failed", counts.failed),
        ("Skipped", counts.skipped),
        ("Ended", counts.ended),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(name, count)| (name, count.to_string()))
}

pub(super) fn draw_footer(
    area: Rect,
    state: &ProgressState,
    theme: &WorkflowTheme,
    buf: &mut Buffer,
) {
    let notice = state.confirmation_text().or(state.footer.as_deref());
    let nav =
        "↑↓ select at level · ← fold / parent · → expand / child · Enter transcript · Esc close";
    put(
        area,
        0,
        notice.unwrap_or(nav),
        if notice.is_some() {
            theme.warning
        } else {
            theme.muted
        },
        buf,
    );
    if let Some(input) = &state.steer_input {
        put(area, 1, &format!("steer> {input}"), theme.selected, buf);
    } else {
        let filters = state.filter.as_str();
        put(
            area,
            1,
            &format!(
                "v evidence · d details/report · a next attention · f filter: {filters} · s steer · x stop · r restart · p pause"
            ),
            theme.muted,
            buf,
        );
    }
}
