//! Snapshot rebuilds for foreground and retained sessions.

use serde_json::Value;
use talaria_core::automation::AutomationEvent;
use talaria_core::background::BackgroundEvent;
use talaria_core::directory::rows::RowKey;
use talaria_core::models::ModelsInput;
use talaria_core::permissions::PermissionsInput;
use talaria_core::rewind::text as rewind_text;
use talaria_core::session::{Cell, NoticeKind, PromptKind, ToolCell, tool_view};
use talaria_core::side::carry_over;
use talaria_core::workspace::SessionFolder;

use crate::app::{
    AppEffect, AppState, REFRESH_FAILED_NOTICE, REFRESHED_NOTICE, SESSION_GONE_NOTICE,
    TranscriptAction, automation_event, background_event, close_processes, models_input,
    notice_lines, permissions, retained_key,
};
use crate::launch::RESUME_HISTORY_CELLS;
use crate::sessions::{OpenError, OpenedSession};

/// How a foreground rebuild was caused.
pub enum RebuildMode {
    /// Gap recovery, unchanged.
    AfterGap,
    /// A rewind or a restore finished. `notice` is committed and nothing above it is reprinted.
    AfterRewind { notice: Vec<String> },
}

/// Replace the foreground session from a fresh snapshot.
///
/// `AfterGap` calls the existing gap recovery. `AfterRewind` is issue 233.
pub fn rebuild_foreground(
    state: &mut AppState,
    snapshot: Value,
    mode: RebuildMode,
) -> Vec<AppEffect> {
    match mode {
        RebuildMode::AfterGap => rebuild_after_gap(state, snapshot),
        RebuildMode::AfterRewind { notice } => rebuild_after_rewind(state, snapshot, notice),
    }
}

pub(crate) fn rebuild_after_gap(state: &mut AppState, snapshot: Value) -> Vec<AppEffect> {
    let result =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => Ok(crate::sessions::OpenedSession::from_snapshot(decoded)),
            Err(_) => Err(crate::sessions::OpenError::Link(
                "the session reply did not decode".to_string(),
            )),
        };
    recovered(state, result)
}

/// Replace the foreground state after a rewind or a restore without printing anything again.
///
/// Cells already in scrollback are matched in order and skipped; the notice is committed, then
/// any final cell this pane never showed (another window's new turn), once.
pub(crate) fn rebuild_after_rewind(
    state: &mut AppState,
    snapshot: Value,
    notice: Vec<String>,
) -> Vec<AppEffect> {
    let now_ms = state.now_ms;
    let mut fresh =
        match serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(snapshot) {
            Ok(decoded) => crate::sessions::OpenedSession::from_snapshot(decoded),
            Err(_) => {
                return vec![notice_lines(&rewind_text::reload_failed(
                    "the session reply did not decode",
                ))];
            }
        };
    let mut old = state.opened.take();
    if let Some(old) = &mut old {
        carry_over(&mut old.state, &mut fresh.state);
        fresh.state.queue.carry_waiting_from(&old.state.queue);
        fresh.carry_background_from(old);
    }
    let shown: &[Cell] = old.as_ref().map_or(&[], |old| {
        &old.state.cells()[..state.committed_cells.min(old.state.cells().len())]
    });
    let cells = fresh.state.cells();
    let total = final_prefix(cells);
    let mut position = 0;
    let mut unseen: Vec<usize> = Vec::new();
    for (index, cell) in cells[..total].iter().enumerate() {
        match shown[position..].iter().position(|seen| seen == cell) {
            Some(offset) => position += offset + 1,
            None => unseen.push(index),
        }
    }
    let mut effects = Vec::new();
    if !notice.is_empty() {
        effects.push(AppEffect::CommitLines(notice));
    }
    let mut start = 0;
    while start < unseen.len() {
        let mut end = start + 1;
        while end < unseen.len() && unseen[end] == unseen[end - 1] + 1 {
            end += 1;
        }
        effects.push(AppEffect::CommitCells(unseen[start]..unseen[end - 1] + 1));
        start = end;
    }
    state.committed_cells = total;
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    if fresh.title.is_none() {
        fresh.title = old.and_then(|old| old.title);
    }
    state.folder =
        SessionFolder::from_snapshot(fresh.info.cwd.as_deref(), fresh.info.branch.as_deref());
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    state
        .ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    fresh.state.publish_commit_cursor(state.committed_cells);
    state.opened = Some(fresh);
    if let Some(session_id) = state
        .opened
        .as_ref()
        .map(|opened| opened.session_id.clone())
    {
        effects.extend(background_event(
            state,
            &session_id,
            BackgroundEvent::Attached { now_ms },
        ));
    }
    state.source_epoch = state.source_epoch.wrapping_add(1);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    effects
}

/// A live tool cell and its stored row are the same call when the tool, what it acted on and
/// its result agree. A stored row has no outcome, duration or summary, and the pinned gateway
/// leaves most results out, so a result only disagrees when both sides hold one.
fn same_tool_call(live: &ToolCell, stored: &ToolCell) -> bool {
    live.tool_id == stored.tool_id
        && live.name == stored.name
        && tool_view(live).subject == tool_view(stored).subject
        && match (&live.result, &stored.result) {
            (Some(live), Some(stored)) => live == stored,
            _ => true,
        }
}

fn same_cell_content(left: &Cell, right: &Cell) -> bool {
    let mut normalized = left.clone();
    match (&mut normalized, right) {
        (Cell::User(a), Cell::User(b)) => a.id = b.id,
        (Cell::Assistant(a), Cell::Assistant(b)) => a.id = b.id,
        (Cell::Reasoning(a), Cell::Reasoning(b)) => a.id = b.id,
        (Cell::Tool(a), Cell::Tool(b)) => return same_tool_call(a, b),
        (Cell::Notice(a), Cell::Notice(b)) => a.id = b.id,
        (Cell::Prompt(a), Cell::Prompt(b)) => a.id = b.id,
        _ => {}
    }
    normalized == *right
}

pub(crate) fn recovered(
    state: &mut AppState,
    result: Result<OpenedSession, OpenError>,
) -> Vec<AppEffect> {
    let now_ms = state.now_ms;
    let mut fresh = match result {
        Ok(fresh) => fresh,
        Err(OpenError::NotFound) => {
            let mut effects = close_processes(state);
            state.session_gone = true;
            effects.push(notice_lines(SESSION_GONE_NOTICE));
            effects.extend(permissions(state, PermissionsInput::SessionClosed));
            effects.extend(automation_event(state, AutomationEvent::Detached));
            return effects;
        }
        Err(OpenError::MismatchedSession { .. }) => return vec![notice_lines(&mismatch_notice())],
        Err(_) => return vec![notice_lines(REFRESH_FAILED_NOTICE)],
    };
    let Some(mut old) = state.opened.take() else {
        return Vec::new();
    };
    carry_over(&mut old.state, &mut fresh.state);
    fresh.state.queue.carry_waiting_from(&old.state.queue);
    fresh.carry_background_from(&old);
    let shown = state.committed_cells.min(old.state.cells().len());
    // A snapshot draws system rows and the notices of turns Hermes wrote (those carry a body);
    // every other notice was this client's own and has no snapshot counterpart. A settled
    // question or approval has none either: the stored tool row of its call stands for it.
    let canonical_prefix: Vec<_> = old.state.cells()[..shown]
        .iter()
        .filter(|cell| {
            !matches!(cell, Cell::Notice(notice)
                if notice.kind != NoticeKind::System
                    && notice.body.is_none()
                    && notice.text != talaria_core::side::SIDE_UNKNOWN_NOTICE)
                && !matches!(cell, Cell::Prompt(_))
        })
        .collect();
    let count = canonical_prefix.len();
    let mut effects = Vec::new();
    let keeps_prefix = fresh.state.cells().len() >= count
        && canonical_prefix
            .iter()
            .zip(&fresh.state.cells()[..count])
            .all(|(old, fresh)| same_cell_content(old, fresh));
    if keeps_prefix {
        state.committed_cells = count;
    } else {
        let cells = fresh.state.cells();
        let total = final_prefix(cells);
        let start = total.saturating_sub(RESUME_HISTORY_CELLS);
        let mut lines = vec!["\u{2500}\u{2500} Transcript refreshed \u{2500}\u{2500}".to_string()];
        if start > 0 {
            lines.push(format!("({start} earlier messages not shown)"));
        }
        effects.push(AppEffect::CommitLines(lines));
        effects.push(AppEffect::CommitCells(start..total));
        state.committed_cells = total;
    }
    effects.push(AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    });
    effects.extend(models_input(
        state,
        ModelsInput::SessionOpened {
            session_id: fresh.session_id.clone(),
            revision: Some(fresh.revision),
            execution_generation: Some(fresh.execution_generation),
        },
    ));
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    state
        .ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    if fresh.title.is_none() {
        fresh.title = old.title;
    }
    let session_id = fresh.session_id.clone();
    let ids: Vec<String> = fresh
        .state
        .open_prompts()
        .filter(|prompt| prompt.kind == PromptKind::ServerRequest)
        .map(|prompt| prompt.key.prompt_id.clone())
        .collect();
    state.folder =
        SessionFolder::from_snapshot(fresh.info.cwd.as_deref(), fresh.info.branch.as_deref());
    state
        .cd_ledger
        .fences_mut(&fresh.session_id)
        .adopt(Some(fresh.revision), Some(fresh.execution_generation));
    fresh.state.publish_commit_cursor(state.committed_cells);
    state.opened = Some(fresh);
    if !ids.is_empty() {
        effects.push(AppEffect::InstallRecovered { ids });
    }
    if let (Some(hub), Some(key)) = (
        state.subagents.as_mut(),
        Some(RowKey {
            profile: state.profile.clone(),
            session_id: session_id.clone(),
        }),
    ) {
        hub.on_reconnected(&key);
    }
    effects.extend(background_event(
        state,
        &session_id,
        BackgroundEvent::Attached { now_ms },
    ));
    state.source_epoch = state.source_epoch.wrapping_add(1);
    if state.pager_open {
        effects.push(AppEffect::Transcript(TranscriptAction::Changed));
    }
    // A re-attached session reads the standing state again.
    effects.extend(automation_event(
        state,
        AutomationEvent::Attached {
            session_id: session_id.clone(),
        },
    ));
    effects.push(notice_lines(REFRESHED_NOTICE));
    effects
}

pub(crate) fn recover_retained(
    state: &mut AppState,
    session_id: &str,
    result: Result<OpenedSession, OpenError>,
) -> Vec<AppEffect> {
    let now_ms = state.now_ms;
    let Some(key) = retained_key(state, session_id) else {
        return Vec::new();
    };
    let mut fresh = match result {
        Ok(fresh) => fresh,
        Err(OpenError::NotFound) => {
            state.retained_sessions.remove(&key);
            return Vec::new();
        }
        Err(OpenError::MismatchedSession { .. }) => return vec![notice_lines(&mismatch_notice())],
        Err(_) => return Vec::new(),
    };
    let Some(mut old) = state.retained_sessions.remove(&key) else {
        return Vec::new();
    };
    carry_over(&mut old.state, &mut fresh.state);
    fresh.state.queue.carry_waiting_from(&old.state.queue);
    fresh.carry_background_from(&old);
    fresh.state.queue.observe_time(now_ms);
    let mut effects = vec![AppEffect::Resync {
        session_id: fresh.session_id.clone(),
        cursor: fresh.cursor.clone(),
    }];
    let ids: Vec<String> = fresh
        .state
        .open_prompts()
        .filter(|prompt| prompt.kind == PromptKind::ServerRequest)
        .map(|prompt| prompt.key.prompt_id.clone())
        .collect();
    state.retained_sessions.insert(key, fresh);
    if !ids.is_empty() {
        effects.push(AppEffect::InstallRecovered { ids });
    }
    effects.extend(background_event(
        state,
        session_id,
        BackgroundEvent::Attached { now_ms },
    ));
    effects
}

fn mismatch_notice() -> String {
    format!("{REFRESH_FAILED_NOTICE} The gateway returned a snapshot for a different session.")
}

pub(crate) fn final_prefix(cells: &[Cell]) -> usize {
    cells.iter().take_while(|cell| cell.is_final()).count()
}
