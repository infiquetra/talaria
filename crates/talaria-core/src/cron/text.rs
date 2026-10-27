use super::{CallFailure, CronOpen, RestFailure};

pub(super) const NOT_CONNECTED: &str = "Not connected to Hermes. Scheduled jobs need the gateway.";
pub(super) const TARGET_FAILURE: &str = "Could not load delivery targets; only local is offered.";
pub(super) const UNKNOWN_WRITE: &str = "Talaria lost the reply; the change may or may not have happened. The list reloads when the connection returns.";

pub(super) fn clean(text: &str) -> String {
    split_and_sanitise(text, 1)
        .into_iter()
        .next()
        .unwrap_or_default()
}
pub(super) fn cut(text: &str, count: usize) -> String {
    clean(text).chars().take(count).collect()
}

pub fn split_and_sanitise(text: &str, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let normal = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = normal.split('\n');
    let mut result: Vec<_> = lines
        .by_ref()
        .take(max_lines)
        .map(crate::attachments::sanitize)
        .collect();
    if lines.next().is_some()
        && let Some(last) = result.last_mut()
    {
        last.push('…');
    }
    result
}

pub fn rest_reason(failure: &RestFailure) -> String {
    match failure {
        RestFailure::Forbidden(d) => format!("Hermes refused the request (403 {})", cut(d, 200)),
        RestFailure::NotFound => "Hermes answered 404 (the job or the route is not there)".into(),
        RestFailure::Refused { status, detail } => {
            format!("Hermes refused the request ({status} {})", cut(detail, 200))
        }
        RestFailure::Server(status) => format!("Hermes failed ({status})"),
        RestFailure::Unauthorized => "Hermes refused the ticket".into(),
        RestFailure::Unknown => "the reply was lost".into(),
        RestFailure::NotSent(reason) => clean(reason),
        RestFailure::BadBody(_) => "the reply did not decode".into(),
    }
}

pub fn parse_arguments(args: &str) -> Result<CronOpen, String> {
    let words: Vec<_> = args.split_whitespace().collect();
    match words.as_slice() {
        [] | ["list"] => Ok(CronOpen::List),
        ["new"] | ["add"] => Ok(CronOpen::NewJob),
        _ => Err("Usage: /cron [list|new]. Talaria creates jobs in a form; it does not read /cron add arguments.".into()),
    }
}

pub(super) fn call_reason(failure: &CallFailure, profile: &str) -> String {
    match failure {
        CallFailure::Tool { matches, .. } if *matches > 0 => {
            "More than one job matches; reload with ctrl+r and try again.".into()
        }
        CallFailure::Tool {
            job_saved: true,
            job_id,
            error,
            ..
        } => {
            let id = job_id.as_deref().unwrap_or("");
            super::saved_notice(id, id, error)
        }
        CallFailure::Tool { error, .. } => format!("Hermes refused: {}.", cut(error, 400)),
        CallFailure::Rpc {
            code: 4000,
            message,
        } => format!(
            "Hermes and Talaria disagree about cron.manage (code 4000): {}. Talaria may be newer or older than this Hermes.",
            clean(message)
        ),
        CallFailure::Rpc { code: 4016, .. } => {
            "This Hermes does not know that cron action (code 4016).".into()
        }
        CallFailure::Rpc { code: 4064, .. } => {
            format!("Hermes cannot find profile {} (code 4064).", clean(profile))
        }
        CallFailure::Rpc {
            code: 5023,
            message,
        } => format!("Hermes failed: {}.", cut(message, 400)),
        CallFailure::Rpc { code: -32601, .. } => {
            "This Hermes does not offer cron.manage on Talaria's connection.".into()
        }
        CallFailure::Rpc { code, message } => {
            format!("Hermes error {code}: {}.", cut(message, 400))
        }
        CallFailure::Unknown => UNKNOWN_WRITE.into(),
        CallFailure::NotSent(_) => NOT_CONNECTED.into(),
        CallFailure::BadResult(reason) => {
            format!("Hermes's reply did not decode: {}.", clean(reason))
        }
    }
}
