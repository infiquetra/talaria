//! Plain text shared by terminal presentation and offline journal inspection.
use super::{AgentNode, format_money, format_tokens, live::ProgressState};
use crate::tokens::tokens_text;

/// The note on an agent's row when Talaria started it again after it ran out of context.
fn restart_note(a: &AgentNode) -> String {
    if a.overflow_restarts == 0 {
        return String::new();
    }
    format!(
        " · restarted {} of {} after a context overflow",
        a.overflow_restarts,
        crate::journal::MAX_OVERFLOW_RESTARTS
    )
}

pub fn agent_line(a: &AgentNode, now_ms: i64) -> String {
    let provider = a
        .provider
        .as_ref()
        .map(|p| format!(" on {p}"))
        .unwrap_or_default();
    format!(
        "{} {} · {}{provider} · {} · {:.1}s · {} · {}{}",
        a.state.glyph(),
        a.label,
        a.model.as_deref().unwrap_or("model unknown"),
        a.state.as_str(),
        a.duration_ms(now_ms) as f64 / 1000.0,
        if a.tokens_unknown {
            "unknown tokens".into()
        } else {
            tokens_text(a.tokens, a.cache_read, a.cache_unknown_turns, format_tokens)
        },
        format_money(a.money),
        restart_note(a),
    ) + &a
        .progress_note()
        .map(|note| format!(" · {note}"))
        .unwrap_or_default()
}

pub fn run_line(s: &ProgressState) -> String {
    let r = &s.progress.run;
    let c = &r.counts;
    let mut line = format!("{} · {}", r.name, r.state.as_str());
    if let Some(suffix) = &s.run_suffix {
        line.push_str(&format!(" · {suffix}"));
    }
    if s.no_live_runner {
        line.push_str(&format!(
            " · runner not running — talaria workflow resume {} continues it",
            r.run_id
        ));
    }
    if s.filter != super::Filter::All {
        line.push_str(&format!(" · filter: {}", s.filter.as_str()));
    }
    line.push_str(&format!(" · agents {}/{}", c.done, c.total));
    let cut_short = s
        .progress
        .agents()
        .iter()
        .filter(|a| a.cut_short.is_some())
        .count();
    for (n, name) in [
        (c.running, "running"),
        (c.needs_you, "needs you"),
        // An agent the budget stopped is counted once, as stopped.
        (c.failed.saturating_sub(r.budget_stopped), "failed"),
        (c.queued, "queued"),
        (c.skipped, "skipped"),
        (c.ended, "ended"),
        (r.refused, "refused"),
        (r.budget_stopped, "stopped"),
        (cut_short, "cut short"),
    ] {
        if n > 0 {
            line.push_str(&format!(" · {n} {name}"));
        }
    }
    let money = s.scheduler.as_ref().and_then(|v| v.spend_money).or(r.money);
    line.push_str(&format!(
        " · {} · {}",
        if r.tokens_unknown {
            "unknown tokens".into()
        } else {
            tokens_text(r.tokens, r.cache_read, r.cache_unknown_turns, format_tokens)
        },
        format_money(money)
    ));
    if s.scheduler.as_ref().and_then(|v| v.spend_money).is_none() && r.unpriced > 0 {
        line.push_str(&format!(" · {} unpriced", r.unpriced));
    }
    if let Some(previous) = r.previous_execution {
        line.push_str(&format!(" · previous execution: {}", previous.as_str()));
    }
    line
}
