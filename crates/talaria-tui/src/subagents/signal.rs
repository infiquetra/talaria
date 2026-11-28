use std::borrow::Cow;

use super::panel::format_elapsed;
use crate::theme::Motion;
use talaria_core::subagents::{FailKind, RowState, SubagentRow};

/// Hermes can stay silent during a model call. Report time without an update
/// separately from the lifecycle state, which only Hermes can change.
const QUIET_MS: u64 = 30_000;
const FRAME_MS: u64 = 240;

#[derive(Clone, Copy, Debug)]
pub struct AgentClock {
    pub now_ms: u64,
    pub motion: Motion,
}

impl Default for AgentClock {
    fn default() -> Self {
        Self {
            now_ms: 0,
            motion: Motion::Reduced,
        }
    }
}

pub(crate) struct Signal {
    pub glyph: char,
    pub label: Cow<'static, str>,
    pub wake_ms: Option<u64>,
}

impl AgentClock {
    pub(crate) fn signal(self, row: &SubagentRow) -> Signal {
        let glyph = row.state.glyph();
        let label = match row.state {
            _ if row.stop_requested => "stopping",
            RowState::Queued => "queued",
            RowState::Completed => "done",
            RowState::Interrupted => "stopped",
            RowState::Failed(FailKind::Timeout) => "timed out",
            RowState::Failed(_) => "failed",
            RowState::Gone => "ended",
            RowState::Running => {
                let age = self.now_ms.saturating_sub(row.last_event_ms);
                if age >= QUIET_MS {
                    return Signal {
                        glyph: '◌',
                        label: format!("quiet {}", format_elapsed(age as f64 / 1_000.0)).into(),
                        wake_ms: Some(1_000 - age % 1_000),
                    };
                }
                let (glyph, wait) = if self.motion == Motion::Full {
                    let frames = ['◐', '◓', '◑', '◒'];
                    (
                        frames[(self.now_ms / FRAME_MS % 4) as usize],
                        FRAME_MS - self.now_ms % FRAME_MS,
                    )
                } else {
                    (glyph, QUIET_MS - age)
                };
                return Signal {
                    glyph,
                    label: "working".into(),
                    wake_ms: Some(wait.min(QUIET_MS - age)),
                };
            }
        };
        Signal {
            glyph,
            label: label.into(),
            wake_ms: None,
        }
    }
}
