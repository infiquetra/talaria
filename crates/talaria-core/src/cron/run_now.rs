use super::text::{clean, cut};
use super::{JobRow, TriggerOutcome};

pub fn stale_run_notice(
    name: &str,
    profile: &str,
    same_profile: bool,
    outcome: &TriggerOutcome,
) -> String {
    let name = clean(name);
    let profile = clean(profile);
    let phrase = match outcome {
        TriggerOutcome::Accepted => "ended".into(),
        TriggerOutcome::Failed(f) => format!("did not complete: {}", super::rest_reason(f)),
    };
    let next = if same_profile {
        "Open /cron to see its result.".into()
    } else {
        format!("Switch back to {profile} and open /cron to see its result.")
    };
    format!("The run of {name} in profile {profile} {phrase}. {next}")
}

pub(super) fn confirmation(job: &JobRow, profile: &str) -> String {
    let name = clean(if job.name.is_empty() {
        &job.job_id
    } else {
        &job.name
    });
    let how = if job.no_agent {
        "It runs its script only, with no agent".into()
    } else if job.pinned {
        format!(
            "It uses its pinned model {}{}",
            clean(job.model.as_deref().unwrap_or("")),
            job.provider
                .as_ref()
                .filter(|p| !p.is_empty())
                .map_or(String::new(), |p| format!(" on {}", clean(p)))
        )
    } else {
        "It uses this profile's cron model (cron.model, else the main model)".into()
    };
    let lead = if job.state == "paused" {
        format!(
            "{name} is paused. Running it now also resumes its schedule (Hermes does this). Run and resume?"
        )
    } else {
        format!("Run {name} now in profile {}?", clean(profile))
    };
    let warning = if job.deliver == "local" {
        ""
    } else {
        " Run now sends from the dashboard without the gateway's live connections, so some platforms may not receive it."
    };
    format!(
        "{lead} {how}; it delivers to {}.{warning}  y run · n cancel",
        clean(&job.deliver)
    )
}

pub(super) fn result_notice(name: &str, baseline: &Option<String>, row: Option<&JobRow>) -> String {
    let name = clean(name);
    let Some(row) = row else {
        return format!(
            "Ran {name}. Hermes no longer lists it (a one-shot job removes itself after its last run)."
        );
    };
    if row.last_run_at == *baseline {
        return format!(
            "Hermes accepted the run of {name} but has not recorded a result yet. Its history shows the run."
        );
    }
    match row.last_status.as_deref() {
        Some("ok") => format!("Ran {name}: finished ok."),
        Some("error") => format!(
            "Ran {name}: failed: {}.",
            cut(row.last_error.as_deref().unwrap_or(""), 400)
        ),
        Some("delivery_failed") => format!(
            "Ran {name}, but delivery failed: {}.",
            cut(row.last_delivery_error.as_deref().unwrap_or(""), 400)
        ),
        status => format!(
            "Ran {name}: {}.",
            clean(&status.unwrap_or("ran").replace('_', " "))
        ),
    }
}
