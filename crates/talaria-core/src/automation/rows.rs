//! The automation rows: modes, goal, loop, heartbeat.
//!
//! Behavior contract: issue 236. Layouts follow Hermes's terminal interface (`goalStatus.ts`,
//! `loopStatus.ts`); cadences use Hermes's `format_interval` wording.

use super::state::{
    AutomationState, ControlView, FastMode, GoalView, HeartbeatView, LoopView, ModeValue, WaitView,
};

/// How a row is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The accent colour.
    Accent,
    /// The warning colour.
    Warning,
}

/// One row of the automation section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationRow {
    /// How the label is drawn.
    pub tone: Tone,
    /// The fixed label.
    pub label: String,
    /// The computed detail.
    pub detail: String,
    /// The Hermes text tail.
    pub tail: String,
}

const SEPARATOR: &str = " · ";

/// The rows for the state at `now_ms` (epoch milliseconds).
pub fn rows(state: &AutomationState, now_ms: u64) -> Vec<AutomationRow> {
    let mut out = Vec::new();
    if let Some(row) = modes_row(state) {
        out.push(row);
    }
    if let Some(view) = state.view() {
        out.extend(view.goal.as_ref().and_then(|goal| goal_row(goal, now_ms)));
        out.extend(
            view.loop_
                .as_ref()
                .and_then(|loop_| loop_row(loop_, now_ms)),
        );
        out.extend(
            view.heartbeat
                .as_ref()
                .and_then(|heartbeat| heartbeat_row(heartbeat, now_ms)),
        );
    }
    out
}

fn row(tone: Tone, label: &str, detail: Vec<String>, tail: &str) -> AutomationRow {
    AutomationRow {
        tone,
        label: label.to_owned(),
        detail: detail.join(SEPARATOR),
        tail: tail.to_owned(),
    }
}

fn modes_row(state: &AutomationState) -> Option<AutomationRow> {
    let (yolo, fast) = state.modes();
    let fast = match fast {
        ModeValue::Known(FastMode::Fast) => Some("⚡ fast mode"),
        ModeValue::Known(FastMode::Ultrafast) => Some("⚡ ultrafast mode"),
        _ => None,
    };
    let yolo = match yolo {
        ModeValue::Known(true) => Some("commands run without approval in this session"),
        ModeValue::Unknown => Some("state unknown: Hermes did not answer"),
        _ => None,
    };
    match (yolo, fast) {
        (Some(detail), fast) => Some(row(
            Tone::Warning,
            "⚠ YOLO",
            vec![detail.to_owned()],
            fast.unwrap_or_default(),
        )),
        (None, Some(fast)) => Some(row(Tone::Accent, fast, Vec::new(), "")),
        (None, None) => None,
    }
}

fn plural(count: i64, word: &str) -> String {
    if count == 1 {
        format!("{count} {word}")
    } else {
        format!("{count} {word}s")
    }
}

/// A reason as a detail part; an absent or blank reason adds none.
fn reason_part(reason: Option<&str>) -> Option<String> {
    reason
        .filter(|reason| !reason.trim().is_empty())
        .map(str::to_owned)
}

fn goal_row(goal: &GoalView, now_ms: u64) -> Option<AutomationRow> {
    let mut parts = vec![format!("{}/{} turns", goal.turns_used, goal.max_turns)];
    for (count, word) in [(goal.subgoals, "subgoal"), (goal.gates, "gate")] {
        if count > 0 {
            parts.push(plural(i64::try_from(count).unwrap_or(i64::MAX), word));
        }
    }
    let (tone, label, mut detail) = match (goal.status.as_str(), &goal.wait) {
        ("active", None) => (Tone::Accent, "⊙ goal", Vec::new()),
        ("active", Some(wait)) => {
            let (when, why) = match wait {
                WaitView::Until { until_ms, reason } if *until_ms > now_ms => (
                    format!("{} left", countdown(gap(*until_ms, now_ms))),
                    reason,
                ),
                WaitView::Until { reason, .. } => ("due now".to_owned(), reason),
                WaitView::Session { target, reason } => (format!("on session {target}"), reason),
                WaitView::Pid { target, reason } => (format!("on pid {target}"), reason),
            };
            let mut detail = vec![when];
            detail.extend(reason_part(Some(why)));
            (Tone::Warning, "⏳ goal parked", detail)
        }
        ("paused", _) => (
            Tone::Warning,
            "⏸ goal paused",
            reason_part(goal.paused_reason.as_deref())
                .into_iter()
                .collect(),
        ),
        _ => return None,
    };
    detail.extend(parts);
    Some(row(tone, label, detail, &goal.title))
}

fn loop_row(loop_: &LoopView, now_ms: u64) -> Option<AutomationRow> {
    let runs = if loop_.times > 0 {
        format!("{}/{} runs", loop_.ticks_fired, loop_.times)
    } else {
        plural(loop_.ticks_fired, "run")
    };
    match loop_.status.as_str() {
        "active" => {
            let cadence = if loop_.self_paced {
                "self-paced".to_owned()
            } else {
                format!("every {}", format_interval(loop_.interval_seconds))
            };
            let when = if loop_.awaiting_response {
                "running now".to_owned()
            } else if loop_.deferred_by_goal {
                "deferred to the goal".to_owned()
            } else {
                next_in(loop_.next_due_ms, now_ms)
            };
            Some(row(
                Tone::Accent,
                "↻ loop",
                vec![cadence, runs, when],
                &loop_.prompt,
            ))
        }
        "paused" => {
            let mut detail: Vec<String> = reason_part(loop_.paused_reason.as_deref())
                .into_iter()
                .collect();
            detail.push(runs);
            Some(row(Tone::Warning, "⏸ loop paused", detail, &loop_.prompt))
        }
        _ => None,
    }
}

fn heartbeat_row(heartbeat: &HeartbeatView, now_ms: u64) -> Option<AutomationRow> {
    let sent = format!("{} sent", heartbeat.fire_count);
    match heartbeat.status.as_str() {
        "active" => {
            let cadence = format!(
                "every {}",
                format_interval(heartbeat.interval_seconds as f64)
            );
            let when = next_in(heartbeat.next_due_ms, now_ms);
            Some(row(
                Tone::Accent,
                "♥ heartbeat",
                vec![cadence, sent, when],
                &heartbeat.prompt,
            ))
        }
        "paused" => Some(row(
            Tone::Warning,
            "⏸ heartbeat paused",
            vec![sent],
            &heartbeat.prompt,
        )),
        _ => None,
    }
}

fn next_in(due_ms: u64, now_ms: u64) -> String {
    if due_ms > now_ms {
        format!("next in {}", countdown(gap(due_ms, now_ms)))
    } else {
        "due, waiting for idle".to_owned()
    }
}

/// `due_ms - now_ms` as a countdown argument, saturating.
fn gap(due_ms: u64, now_ms: u64) -> i64 {
    i64::try_from(due_ms.saturating_sub(now_ms)).unwrap_or(i64::MAX)
}

/// The next epoch-millisecond instant at which a row's text changes, if any.
pub fn next_tick(state: &AutomationState, now_ms: u64) -> Option<u64> {
    let view = state.view()?;
    countdown_dues(view)
        .into_iter()
        .filter(|due| *due > now_ms)
        .map(|due| {
            let remaining = due - now_ms;
            let step = if remaining > 60_000 { 60_000 } else { 1_000 };
            now_ms + (remaining - (remaining.div_ceil(step) - 1) * step)
        })
        .min()
}

/// The due times of every countdown a row can show; a past one draws no countdown.
fn countdown_dues(view: &ControlView) -> Vec<u64> {
    let mut dues = Vec::new();
    if let Some(goal) = &view.goal
        && goal.status == "active"
        && let Some(WaitView::Until { until_ms, .. }) = goal.wait
    {
        dues.push(until_ms);
    }
    if let Some(loop_) = &view.loop_
        && loop_.status == "active"
        && !loop_.shows_state_word()
    {
        dues.push(loop_.next_due_ms);
    }
    if let Some(heartbeat) = &view.heartbeat
        && heartbeat.status == "active"
    {
        dues.push(heartbeat.next_due_ms);
    }
    dues
}

impl LoopView {
    /// True when the loop row shows a state word instead of a countdown.
    fn shows_state_word(&self) -> bool {
        self.awaiting_response || self.deferred_by_goal
    }
}

/// A countdown in Hermes's wording (`45m`, `1h05m`, `59s`).
pub fn countdown(remaining_ms: i64) -> String {
    let left = u64::try_from(remaining_ms).unwrap_or(0).div_ceil(1000);
    if left < 60 {
        return format!("{left}s");
    }
    let minutes = left.div_ceil(60);
    let hours = minutes / 60;
    if hours > 0 {
        format!("{hours}h{:02}m", minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

/// A cadence in Hermes's wording (`every 45m` is built by the caller).
pub fn format_interval(seconds: f64) -> String {
    // A float-to-integer `as` cast saturates, and a negative or NaN cadence reads as zero.
    let total = seconds.round().max(0.0) as u64;
    let (hours, minutes, secs) = (total / 3600, total % 3600 / 60, total % 60);
    let mut text = String::new();
    if hours > 0 {
        text.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        text.push_str(&format!("{minutes}m"));
    }
    if secs > 0 || text.is_empty() {
        text.push_str(&format!("{secs}s"));
    }
    text
}
