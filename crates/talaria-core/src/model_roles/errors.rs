//! Words for a failed read or write.

use super::{RolesError, text};

/// What to show when a write fails. Hermes's own detail text follows its heading.
pub(super) fn write_lines(error: &RolesError) -> Vec<String> {
    match error {
        RolesError::NotSent { reason } => vec![format!("The change was not sent: {reason}.")],
        RolesError::OutcomeUnknown | RolesError::BadReply => vec![text::UNKNOWN.to_owned()],
        RolesError::Refused { status, detail } => match (*status, detail) {
            (422, Some(detail)) => vec![text::PROBLEM.to_owned(), detail.clone()],
            (422, None) => vec![text::PROBLEM_BARE.to_owned()],
            (_, Some(detail)) => vec![text::REFUSED.to_owned(), detail.clone()],
            (_, None) => vec![text::REFUSED.to_owned()],
        },
        RolesError::RestartRequired => {
            vec![text::RESTART.to_owned(), text::DOCTOR_RESTART.to_owned()]
        }
        RolesError::Server { .. } => vec![text::NOT_SAVED_SERVER.to_owned()],
        RolesError::Forbidden { detail } => vec![format!(
            "Hermes refused this profile ({detail}). This is a Talaria defect; please report it."
        )],
        RolesError::NotFound { route } => vec![text::unavailable(route)],
        RolesError::SignIn => vec![text::SIGN_IN.to_owned(), text::DOCTOR.to_owned()],
    }
}

/// What to show when a read fails. `what` names the thing being read.
pub(super) fn read_lines(error: &RolesError, what: &str) -> Vec<String> {
    match error {
        RolesError::SignIn => vec![text::SIGN_IN.to_owned(), text::DOCTOR.to_owned()],
        RolesError::BadReply => vec![text::BAD_REPLY.to_owned()],
        RolesError::Server { .. } => {
            vec![format!("Hermes could not read {what}. Ctrl+R to retry.")]
        }
        RolesError::Forbidden { detail } => vec![format!(
            "Hermes refused this profile ({detail}). This is a Talaria defect; please report it."
        )],
        RolesError::NotFound { route } => vec![text::unavailable(route)],
        _ => vec![text::NO_ANSWER.to_owned()],
    }
}

/// A failed write is followed by a read-back unless nothing could have changed.
pub(super) fn needs_read_back(error: &RolesError) -> bool {
    !matches!(error, RolesError::NotSent { .. } | RolesError::SignIn)
}

pub(super) fn is_unknown(error: &RolesError) -> bool {
    matches!(error, RolesError::OutcomeUnknown | RolesError::BadReply)
}
