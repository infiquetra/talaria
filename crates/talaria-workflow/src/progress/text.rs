//! Plain text shared by terminal presentation and offline journal inspection.
use super::{AgentNode, format_money, format_tokens, live::ProgressState};

pub fn agent_line(a: &AgentNode, now_ms: i64) -> String {
    let provider = a
        .provider
        .as_ref()
        .map(|p| format!(" on {p}"))
        .unwrap_or_default();
    format!(
        "{} {} · {}{provider} · {} · {:.1}s · {} tokens · {}",
        a.state.glyph(),
        a.label,
        a.model.as_deref().unwrap_or("model unknown"),
        a.state.as_str(),
        a.duration_ms(now_ms) as f64 / 1000.0,
        if a.tokens_unknown {
            "unknown".into()
        } else {
            format_tokens(a.tokens)
        },
        format_money(a.money),
    )
}

pub fn run_line(s: &ProgressState) -> String {
    let r = &s.progress.run;
    let c = &r.counts;
    let mut line = format!("{} · {}", r.name, r.state.as_str());
    if let Some(suffix) = &s.run_suffix {
        line.push_str(&format!(" · {suffix}"));
    }
    if s.no_live_runner {
        line.push_str(" · no live runner detected");
    }
    line.push_str(&format!(" · agents {}/{}", c.done, c.total));
    for (n, name) in [
        (c.running, "running"),
        (c.needs_you, "needs you"),
        (c.failed, "failed"),
        (c.queued, "queued"),
        (c.skipped, "skipped"),
        (c.ended, "ended"),
    ] {
        if n > 0 {
            line.push_str(&format!(" · {n} {name}"));
        }
    }
    let money = s.scheduler.as_ref().and_then(|v| v.spend_money).or(r.money);
    line.push_str(&format!(
        " · {} tokens · {}",
        if r.tokens_unknown {
            "unknown".into()
        } else {
            format_tokens(r.tokens)
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
