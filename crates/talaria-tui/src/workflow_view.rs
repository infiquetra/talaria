//! Windowed alternate-screen progress presentation. Untrusted content stays literal.
use crate::history::sanitize;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};
pub use talaria_workflow::progress::live::ProgressKey;
use talaria_workflow::progress::{
    format_money, format_tokens,
    live::{NodeKey, ProgressState, WaitView},
};

#[derive(Clone, Debug)]
pub struct WorkflowTheme {
    pub normal: Style,
    pub selected: Style,
    pub muted: Style,
    pub warning: Style,
}
impl Default for WorkflowTheme {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            selected: Style::default().bg(Color::DarkGray),
            muted: Style::default().fg(Color::Gray),
            warning: Style::default().fg(Color::Yellow),
        }
    }
}
pub struct WorkflowView;
impl WorkflowView {
    pub fn render_picker(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        picker: &talaria_workflow::runner::ProgressPicker,
        theme: &WorkflowTheme,
    ) {
        let block = Block::bordered().title("Workflows").style(theme.normal);
        let inner = block.inner(area);
        block.render(area, buf);
        if let Some(message) = picker.empty_message() {
            Paragraph::new(message)
                .style(theme.muted)
                .render(inner, buf);
            return;
        }
        let room = inner.height.saturating_sub(1) as usize;
        let start = picker
            .selected_index()
            .saturating_sub(room.saturating_sub(1));
        for (i, row) in picker.rows().iter().enumerate().skip(start).take(room) {
            let text = format!(
                "{}  {}  {}  agents {}/{}  parked {}  tokens {}  money {}{}",
                row.run_id,
                row.name,
                row.status.as_str(),
                row.agents_finished,
                row.agents_total_known,
                row.parked,
                row.tokens_spent
                    .map_or_else(|| "-".into(), |n| n.to_string()),
                row.money_spent_usd
                    .map_or_else(|| "-".into(), |n| format!("{n:.4}")),
                row.warning
                    .as_ref()
                    .map_or_else(String::new, |w| format!("  {w}")),
            );
            Paragraph::new(sanitize(&text).into_owned())
                .style(if i == picker.selected_index() {
                    theme.selected
                } else {
                    theme.normal
                })
                .render(
                    Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1),
                    buf,
                );
        }
        if inner.height > 0 {
            Paragraph::new("↑↓ select · enter open · esc close")
                .style(theme.muted)
                .render(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1), buf);
        }
    }
    pub fn draw(frame: &mut Frame, area: Rect, state: &ProgressState, theme: &WorkflowTheme) {
        Self::render(area, frame.buffer_mut(), state, theme);
    }
    pub fn render(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &ProgressState,
        theme: &WorkflowTheme,
    ) {
        let block = Block::bordered().title("Workflows").style(theme.normal);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height == 0 || inner.width == 0 {
            return;
        }
        let mut y = inner.y;
        let put = |y: u16, text: String, style: Style, buf: &mut ratatui::buffer::Buffer| {
            Paragraph::new(Line::from(sanitize(&text).into_owned()))
                .style(style)
                .render(Rect::new(inner.x, y, inner.width, 1), buf);
        };
        if let Some(error) = &state.error {
            put(y, format!("{error} · esc closes"), theme.warning, buf);
            return;
        }
        put(y, run_line(state), theme.normal, buf);
        y += 1;
        if !state.connected && y < inner.bottom() {
            put(
                y,
                "journal only — live state and controls unavailable".into(),
                theme.warning,
                buf,
            );
            y += 1;
        }
        if state.progress.last_seq == 0 && y < inner.bottom() {
            put(
                y,
                "run starting — nothing journaled yet".into(),
                theme.muted,
                buf,
            );
            y += 1;
        }
        let detail_height = if state.selected_agent_key().is_some() {
            9
        } else {
            0
        };
        let room = inner
            .bottom()
            .saturating_sub(y)
            .saturating_sub(2 + detail_height) as usize;
        let nodes = state.visible_nodes();
        let selection = nodes
            .iter()
            .position(|n| n.key == state.selection)
            .unwrap_or(0);
        let start = selection.saturating_sub(room.saturating_sub(1));
        if nodes.len() == 1 && !state.progress.agents.is_empty() && y < inner.bottom() {
            put(y, "no agents match this filter".into(), theme.muted, buf);
            y += 1;
        }
        for node in nodes.iter().skip(start).take(room) {
            if node.key == NodeKey::Run {
                continue;
            }
            let text = node_line(state, &node.key);
            let text = format!(
                "{}{}{}",
                "  ".repeat(node.depth as usize),
                if state.collapsed.contains(&node.key) {
                    "▸ "
                } else {
                    ""
                },
                text
            );
            put(
                y,
                text,
                if node.key == state.selection {
                    theme.selected
                } else {
                    theme.normal
                },
                buf,
            );
            y += 1;
        }
        if let Some(key) = state.selected_agent_key()
            && let Some(a) = state.progress.agent(key)
        {
            let prompt = state
                .prompt_first_line
                .as_deref()
                .or_else(|| a.prompt.as_deref().and_then(|s| s.lines().next()))
                .unwrap_or(if a.prompt_ref.is_some() {
                    "(prompt unavailable — journal blob missing)"
                } else {
                    "(prompt unavailable)"
                });
            let detail = [
                format!("prompt: {prompt}"),
                format!(
                    "session: {} · stored: {}",
                    a.session_id.as_deref().unwrap_or("none"),
                    a.stored_session_id.as_deref().unwrap_or("none")
                ),
                format!(
                    "attempt {} · turn {} · admission {} ({})",
                    a.attempt,
                    a.turn,
                    a.admission_id.as_deref().unwrap_or("none"),
                    a.admission_status.as_deref().unwrap_or("none")
                ),
                format!(
                    "settled {} · displayed {} tokens · {}",
                    format_tokens(a.settled_tokens),
                    format_tokens(a.tokens),
                    format_money(a.money)
                ),
            ];
            for text in detail.into_iter().chain(a.tail.iter().cloned()) {
                if y >= inner.bottom().saturating_sub(2) {
                    break;
                }
                put(y, text, theme.muted, buf);
                y += 1;
            }
            if let NodeKey::Subagent { id, .. } = &state.selection
                && let Some(row) = a.subagents.get(id)
            {
                for activity in row.activity.iter().rev().take(4).rev() {
                    if y >= inner.bottom().saturating_sub(2) {
                        break;
                    }
                    put(y, activity.text.clone(), theme.muted, buf);
                    y += 1;
                }
            }
        }
        if inner.height >= 2 {
            let footer=state.confirmation_text().map(str::to_owned).or_else(||state.footer.clone()).unwrap_or_else(||"↑↓ select · enter open · s steer · x stop · r restart · p pause · f filter · esc close".into());
            put(inner.bottom() - 2, footer, theme.muted, buf);
        }
        if let Some(text) = &state.steer_input {
            put(
                inner.bottom() - 1,
                format!("steer> {text}"),
                theme.selected,
                buf,
            );
        }
    }
}
pub use talaria_workflow::progress::text::run_line;

pub fn picker_key_input(map: &crate::keys::KeyMap, key: crate::keys::Key) -> Option<ProgressKey> {
    use crate::keys::{Action, KeyContext, KeyKind};
    if key.kind == KeyKind::Release {
        return None;
    }
    match map.resolve(&key, KeyContext::WorkflowProgress)? {
        Action::ProgressUp => Some(ProgressKey::Up),
        Action::ProgressDown => Some(ProgressKey::Down),
        Action::OpenAgent => Some(ProgressKey::OpenAgent),
        Action::CloseView => Some(ProgressKey::CloseView),
        _ => None,
    }
}
fn node_line(s: &ProgressState, node: &NodeKey) -> String {
    match node {
        NodeKey::Run => run_line(s),
        NodeKey::Child(path) => s
            .progress
            .children
            .iter()
            .find(|c| &c.workflow_path == path)
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        NodeKey::Phase { path, title } => s
            .progress
            .phases
            .iter()
            .find(|p| &p.workflow_path == path && &p.title == title)
            .map(|p| {
                format!(
                    "{} · agents {}/{}{}",
                    title,
                    p.counts.done,
                    p.counts.total,
                    if p.counts.needs_you > 0 {
                        format!(" · {} needs you", p.counts.needs_you)
                    } else {
                        String::new()
                    }
                )
            })
            .unwrap_or_default(),
        NodeKey::Agent(key) => s
            .progress
            .agent(key)
            .map(|a| {
                let model = a.model.as_deref().unwrap_or("model unknown");
                let provider = a
                    .provider
                    .as_ref()
                    .map(|p| format!(" on {p}"))
                    .unwrap_or_default();
                let mut state = a.state.as_str().to_string();
                if a.attempt > 0 {
                    state.push_str(&format!(" · attempt {}", a.attempt));
                }
                if let Some(cause) = &a.attention {
                    state.push_str(&format!(" · {cause}"));
                }
                if a.state == talaria_workflow::progress::AgentState::Ended {
                    state.push_str(" · ended without a journal outcome");
                }
                if a.restart_requested {
                    state.push_str(" · restart requested — runs again on resume");
                }
                if a.stopped || a.state == talaria_workflow::progress::AgentState::Failed {
                    state.push_str(" · stopped — restart (r) runs it again");
                }
                if let Some(wait) = s.waits.get(key) {
                    state.push_str(&format!(" · {}", wait_text(wait)));
                }
                format!(
                    "{} {} · {model}{provider} · {state} · {:.1}s · {} tokens · {} · {}",
                    a.state.glyph(),
                    a.label,
                    a.duration_ms(s.now_ms) as f64 / 1000.0,
                    if a.tokens_unknown {
                        "unknown".into()
                    } else {
                        format_tokens(a.tokens)
                    },
                    format_money(a.money),
                    a.last_tool
                        .as_deref()
                        .or_else(|| a.tail.back().map(String::as_str))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default(),
        NodeKey::Subagent { key, id } => s
            .progress
            .agent(key)
            .and_then(|a| a.subagents.get(id))
            .map(|r| {
                let stopping = s.subagent_stopping(key, id);
                format!(
                    "{} {} · {} · {}{}",
                    if stopping { '◌' } else { r.state.glyph() },
                    r.goal,
                    r.model.as_deref().unwrap_or("model unknown"),
                    if stopping { "stopping · " } else { "" },
                    r.last_tool.as_deref().unwrap_or_default()
                )
            })
            .unwrap_or_default(),
    }
}
fn wait_text(w: &WaitView) -> &'static str {
    match w {
        WaitView::GlobalCap => "waiting for global capacity",
        WaitView::ProviderCap => "waiting for provider capacity",
        WaitView::ProviderLimited { .. } => "provider limited",
        WaitView::ModelLimited { .. } => "model limited",
        WaitView::ProviderPaused { .. } => "provider paused",
    }
}

/// The overlay consumes every key before the composer or foreground turn.
pub fn key_input(
    map: &crate::keys::KeyMap,
    key: crate::keys::Key,
    state: &ProgressState,
) -> Option<talaria_workflow::progress::live::ProgressKey> {
    use crate::keys::{Action, KeyCode, KeyContext, KeyKind};
    use talaria_workflow::progress::live::ProgressKey as K;
    if key.kind == KeyKind::Release {
        return None;
    }
    if state.steer_input.is_some() {
        return match key.code {
            KeyCode::Char(c) if !key.mods.ctrl && !key.mods.alt && !key.mods.super_key => {
                Some(K::Char(c))
            }
            KeyCode::Backspace => Some(K::Backspace),
            KeyCode::Enter => Some(K::Enter),
            KeyCode::Esc => Some(K::CloseView),
            _ => None,
        };
    }
    Some(
        match map
            .resolve(&key, KeyContext::WorkflowProgress)
            .or_else(|| state.confirmation_text().map(|_| Action::CloseView))?
        {
            Action::ProgressUp => K::Up,
            Action::ProgressDown => K::Down,
            Action::ProgressLeft => K::Left,
            Action::ProgressRight => K::Right,
            Action::OpenAgent => K::OpenAgent,
            Action::Steer => K::Steer,
            Action::StopAgent => K::StopAgent,
            Action::RestartAgent => K::RestartAgent,
            Action::PauseRun => K::PauseRun,
            Action::StopRun => K::StopRun,
            Action::CycleFilter => K::CycleFilter,
            Action::CloseView => K::CloseView,
            Action::ProgressConfirm => K::Confirm,
            _ => return None,
        },
    )
}
