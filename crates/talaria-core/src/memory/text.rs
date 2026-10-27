use super::*;
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

pub fn format_bytes(n: u64) -> String {
    if n == 0 {
        return "empty".into();
    }
    if n < 1024 {
        return format!("{n} B");
    }
    let (divisor, unit) = if n < 1_048_576 {
        (1024.0, "KB")
    } else {
        (1_048_576.0, "MB")
    };
    format!("{:.1} {unit}", ((n as f64 / divisor) * 10.0).round() / 10.0)
}

pub fn provider_label(status: &str) -> String {
    match status {
        "ready" => "ready".into(),
        "needs_config" => "needs settings".into(),
        "unavailable" => "not available here".into(),
        "missing" => "not installed".into(),
        other => other.chars().take(24).collect(),
    }
}

fn number(n: f64) -> String {
    let rounded = (n * 10.0).round() / 10.0;
    format!("{rounded:.1}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub fn curator_lines(c: &CuratorView) -> (String, String, String) {
    let status = if !c.enabled {
        "off"
    } else if c.paused {
        "paused"
    } else {
        "active"
    };
    let mut runs = Vec::new();
    if let Some(interval) = c.interval_hours {
        runs.push(if interval == 24.0 {
            "every day".into()
        } else if interval % 24.0 == 0.0 {
            format!("every {} days", number(interval / 24.0))
        } else {
            format!("every {} h", number(interval))
        });
    }
    if let Some(idle) = c.min_idle_hours {
        runs.push(format!("after {} h idle", number(idle)));
    }
    let mut schedule = if runs.is_empty() {
        String::new()
    } else {
        format!("Runs {}.", runs.join(" "))
    };
    let mut maintenance = Vec::new();
    if let Some(stale) = c.stale_after_days {
        maintenance.push(format!(
            "Marks unused skills stale after {} days",
            number(stale)
        ));
    }
    if let Some(archive) = c.archive_after_days {
        maintenance.push(format!("archives after {}", number(archive)));
    }
    if !maintenance.is_empty() {
        if !schedule.is_empty() {
            schedule.push(' ');
        }
        schedule.push_str(&maintenance.join(", "));
        schedule.push('.');
    }
    let last = c
        .last_run_at
        .as_deref()
        .map(|text| {
            OffsetDateTime::parse(text, &Rfc3339)
                .ok()
                .map(|date| {
                    let d = date.to_offset(UtcOffset::UTC);
                    format!(
                        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
                        d.year(),
                        u8::from(d.month()),
                        d.day(),
                        d.hour(),
                        d.minute()
                    )
                })
                .unwrap_or_else(|| text.chars().take(40).collect())
        })
        .unwrap_or_else(|| "none yet".into());
    (
        status.into(),
        schedule,
        format!("Last run or first check: {last}"),
    )
}

pub fn tail_after_header(lines: &[String], max: usize) -> Vec<String> {
    let start = lines
        .iter()
        .rposition(|line| line.starts_with("=== curator-run started"))
        .map_or(0, |i| i + 1);
    lines[start.max(lines.len().saturating_sub(max))..].to_vec()
}

pub fn is_valid_provider_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
