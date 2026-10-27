use super::text::clean;
use super::{JobRow, parse_iso_instant, relative};

pub fn job_state_label(job: &JobRow, wall_unix: i64) -> String {
    match job.state.as_str() {
        "paused" => "paused".into(),
        "completed" => "finished".into(),
        "error" => "schedule error".into(),
        "scheduled" => match job.next_run_at.as_deref().and_then(parse_iso_instant) {
            Some(next) if next.unix.saturating_sub(wall_unix) > 60 => {
                format!("next {}", relative(wall_unix, next.unix))
            }
            Some(_) => "due now".into(),
            None => "scheduled".into(),
        },
        other => clean(other),
    }
}

pub fn last_run_label(job: &JobRow, wall_unix: i64) -> String {
    let Some(last) = job.last_run_at.as_deref().and_then(parse_iso_instant) else {
        return String::new();
    };
    let status = match job.last_status.as_deref() {
        Some("ok") => "ok".into(),
        Some("error") => "failed".into(),
        Some("delivery_failed") => "delivery failed".into(),
        Some(other) => clean(&other.replace('_', " ")),
        None => "ran".into(),
    };
    format!("{status} {}", relative(wall_unix, last.unix))
}

pub(super) fn sort_jobs(jobs: &mut [JobRow]) {
    fn group(state: &str) -> u8 {
        match state {
            "scheduled" => 0,
            "paused" => 1,
            "completed" => 2,
            "error" => 3,
            _ => 4,
        }
    }
    jobs.sort_by(|a, b| {
        group(&a.state)
            .cmp(&group(&b.state))
            .then_with(|| {
                if group(&a.state) == 4 {
                    a.state.to_lowercase().cmp(&b.state.to_lowercase())
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| {
                if a.state == "scheduled" && b.state == "scheduled" {
                    let key = |job: &JobRow| {
                        job.next_run_at
                            .as_deref()
                            .and_then(parse_iso_instant)
                            .map(|t| t.unix)
                    };
                    match (key(a), key(b)) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    }
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.job_id.cmp(&b.job_id))
    });
}
