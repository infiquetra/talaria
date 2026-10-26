use super::*;
use crate::side::clean_output;

fn reason_text(reason: &str) -> String {
    let reason = clean_output(reason);
    if reason.is_empty() {
        "no reason given".to_owned()
    } else {
        reason.chars().take(64).collect()
    }
}

pub(super) fn bg_failure(failure: &CallFailure) -> String {
    match failure {
        CallFailure::Refused { reason, .. } => match reason.as_str() {
            "not_found" => "Hermes did not start the background task: this session is no longer live. Resume it and try again.".to_owned(),
            "permission_denied" => "Hermes did not start the background task: this window is not subscribed to the session (permission_denied). Re-open the session and try again.".to_owned(),
            "runtime_draining" => "Hermes is restarting, so it did not start the background task. Try again when it is back.".to_owned(),
            "invalid_params" => "Hermes rejected the background task request as malformed (invalid_params). This is a Talaria bug; please report it.".to_owned(),
            _ => format!("Hermes did not start the background task ({}).", reason_text(reason)),
        },
        CallFailure::MethodMissing => "This Hermes does not offer background tasks (prompt.background is missing).".to_owned(),
        CallFailure::Storage => "Hermes could not reach its session store (storage_unavailable). Try again.".to_owned(),
        CallFailure::NotSent => "Not connected to Hermes, so the background task was not started.".to_owned(),
        CallFailure::Unknown | CallFailure::Unreadable => "Hermes did not confirm the background task. It may still run; its answer will appear here if it does.".to_owned(),
    }
}

pub(super) fn list_failure(state: &ListState) -> Option<String> {
    Some(match state {
        ListState::Never | ListState::Fresh { .. } => return None,
        ListState::Stopped { why } => match why {
            ListStop::SessionNotLive => "This session is no longer live in Hermes, so its processes cannot be listed. Resume it to see them.".to_owned(),
            ListStop::Unsupported => "Hermes runs this session in a separate worker and cannot list its processes.".to_owned(),
            ListStop::MethodMissing => "This Hermes does not offer process control (process.list is missing).".to_owned(),
            ListStop::PermissionDenied => "Hermes refused to list this session's processes (permission_denied).".to_owned(),
        },
        ListState::Stale { why, .. } => match why.as_str() {
            "runtime_draining" => "Hermes is restarting; the list will refresh when it is back.".to_owned(),
            "stale_generation" => "Hermes is moving this session; retrying.".to_owned(),
            "storage_unavailable" => "Hermes could not reach its session store; showing the last list.".to_owned(),
            "unknown" | "unreadable" | "not_sent" => "Hermes did not answer; showing the last list.".to_owned(),
            _ => format!("Hermes refused ({}); showing the last list.", reason_text(why)),
        },
    })
}

pub(super) fn kill_note(outcome: &KillOutcome) -> String {
    match outcome {
        KillOutcome::Killed => {
            "Killed. Hermes will not tell the agent about this process's exit.".to_owned()
        }
        KillOutcome::AlreadyExited { scope_stop_failed } => {
            let mut text = "It had already exited.".to_owned();
            if *scope_stop_failed {
                text.push_str(
                    " Hermes could not stop its process group; child processes may still run.",
                );
            }
            text
        }
        KillOutcome::NotFound => "Hermes no longer knows this process.".to_owned(),
        KillOutcome::Error { text } => format!(
            "Kill failed: {}",
            clean_output(text).chars().take(160).collect::<String>()
        ),
        KillOutcome::Failed(failure) => failure_note(failure, false),
    }
}

fn failure_note(failure: &CallFailure, stop: bool) -> String {
    match failure {
        CallFailure::Refused { reason, .. } => match reason.as_str() {
            "process_not_found" => "Hermes says this session does not own that process.".to_owned(),
            "runtime_draining" => "Hermes is restarting. Try again when it is back.".to_owned(),
            _ => format!("Hermes refused ({}).", reason_text(reason)),
        },
        CallFailure::MethodMissing => format!(
            "This Hermes does not offer process.{}.",
            if stop { "stop" } else { "kill" }
        ),
        CallFailure::Storage => {
            "Hermes could not reach its session store (storage_unavailable).".to_owned()
        }
        CallFailure::Unknown | CallFailure::Unreadable if stop => {
            "Hermes did not confirm the stop. Refreshing the list.".to_owned()
        }
        CallFailure::Unknown | CallFailure::Unreadable => {
            "Hermes did not confirm. Refreshing the list.".to_owned()
        }
        CallFailure::NotSent => "Not connected to Hermes.".to_owned(),
    }
}

pub(super) fn stop_notice(outcome: &StopOutcome) -> (NoticeLevel, String) {
    match outcome {
        StopOutcome::Stopped {
            killed,
            delegations,
        } => {
            let mut text = if *killed == 0 {
                "No background processes were running.".to_owned()
            } else {
                format!(
                    "Stopped {killed} background {}.",
                    if *killed == 1 { "process" } else { "processes" }
                )
            };
            if *delegations > 0 {
                text.push_str(&format!(
                    " Also interrupted {delegations} background {}.",
                    if *delegations == 1 {
                        "delegation"
                    } else {
                        "delegations"
                    }
                ));
            }
            (NoticeLevel::Info, text)
        }
        StopOutcome::Failed(failure) => (
            NoticeLevel::Error,
            format!(
                "Could not stop background processes: {}",
                failure_note(failure, true)
            ),
        ),
    }
}
