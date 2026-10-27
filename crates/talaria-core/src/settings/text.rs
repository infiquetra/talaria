use super::{FailReason, ReadOnlyReason};
pub(crate) const APPLYING: &str = "Applying… (checking, saving, then reading back)";
pub(crate) fn reason(reason: &FailReason) -> String {
    match reason {
        FailReason::NotConnected => "the gateway is not reachable".into(),
        FailReason::ProfileNotServed => "the profile is not served by this gateway".into(),
        FailReason::Unauthorized => {
            "the gateway refused the request (401); it may be restarting".into()
        }
        FailReason::Timeout { secs } => format!("no reply within {secs} s"),
        FailReason::Dropped => "the connection dropped".into(),
        FailReason::Http { status } => format!("HTTP {status}"),
        FailReason::NotFound { route } => format!("this Hermes does not offer {route} (HTTP 404)"),
        FailReason::BadData => "the reply was not valid settings data".into(),
        FailReason::Ticket(detail) => format!("could not get a ticket: {detail}"),
        FailReason::NotLoopback => "the gateway address is not a local plain-HTTP address".into(),
    }
}
pub(crate) fn save_failed(why: &FailReason) -> String {
    format!(
        "Could not save: {}. Your changes are kept. Enter tries again · Esc back",
        reason(why)
    )
}
pub(crate) fn outside(count: usize) -> String {
    format!("{count} setting(s) changed outside Talaria; showing the new values.")
}
pub(crate) fn readonly(key: &str, reason: &ReadOnlyReason) -> String {
    match reason {
        ReadOnlyReason::OwnedElsewhere => {
            if ["providers", "custom_providers"].contains(&key) {
                "edit in Providers".into()
            } else {
                "The default model is set on the model roles screen; /model changes this session's model. edit in Model roles".into()
            }
        }
        ReadOnlyReason::EnvReference(names) => format!(
            "Comes from the environment variable(s) {}. Change it with the keys screen.",
            names.join(", ")
        ),
        ReadOnlyReason::HoldsSecret => {
            "This value holds a secret inside it, so it cannot be shown or edited here.".into()
        }
        ReadOnlyReason::Unsupported(kind) => {
            format!("This kind of setting ({kind}) cannot be edited here yet.")
        }
    }
}
pub(crate) fn limited(items: &[String]) -> String {
    let mut shown = items.iter().take(5).cloned().collect::<Vec<_>>();
    if items.len() > 5 {
        shown.push(format!("and {} more", items.len() - 5));
    }
    shown.join(", ")
}
