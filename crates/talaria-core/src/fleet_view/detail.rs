//! One-line row text. Not sanitized and not cut; the screen does both.

use crate::attention::NeedSummary;
use crate::fleet::{FleetBackend, FleetRow, NeedDetail, NeedItem, Outcome, RowState};

/// The one-line detail for a row: approval, question, sudo, secret, waiting, working,
/// starting, failed, completed, interrupted, finished or idle.
#[must_use]
pub fn row_detail(row: &FleetRow, items_of_row: &[&NeedItem]) -> String {
    let mut detail = match row.state {
        RowState::NeedsYou => items_of_row.first().map_or_else(
            || "waiting for an answer".to_owned(),
            |item| need_text(item),
        ),
        RowState::Working => match &row.activity {
            Some(activity) if !activity.is_empty() => activity.clone(),
            // A turn that has not produced its first message yet is starting.
            _ if row.message_count == 0 => "starting".to_owned(),
            _ => "working".to_owned(),
        },
        RowState::Failed => "failed".to_owned(),
        RowState::Done => match row.outcome {
            Some(Outcome::Completed) => "completed".to_owned(),
            Some(Outcome::Interrupted) => "interrupted".to_owned(),
            Some(Outcome::Failed) => "failed".to_owned(),
            Some(Outcome::Unknown) | None => "finished".to_owned(),
        },
        RowState::Idle
            if row.backend() == crate::fleet::FleetBackend::Herdr
                && row.outcome == Some(Outcome::Unknown) =>
        {
            "unknown".to_owned()
        }
        RowState::Idle => "idle".to_owned(),
    };
    if row.backend == FleetBackend::Acp {
        detail.push_str(&format!(
            " [acp {}]",
            row.agent.as_deref().unwrap_or_default()
        ));
    } else if row.backend() == crate::fleet::FleetBackend::Herdr {
        detail.push_str(&format!(" [herdr {}]", row.agent().unwrap_or("agent")));
        if let Some(location) = row.terminal_location() {
            detail.push(' ');
            detail.push_str(location);
        }
    }
    detail
}

fn need_text(item: &NeedItem) -> String {
    match &item.detail {
        NeedDetail::Waiting => "waiting for an answer".to_owned(),
        NeedDetail::Known(summary) => match summary {
            NeedSummary::Approval {
                command,
                description,
            } => {
                let shown = if command.is_empty() {
                    description
                } else {
                    command
                };
                format!("approval: {shown}")
            }
            NeedSummary::Question(question) => format!("question: {question}"),
            NeedSummary::Sudo => "sudo password".to_owned(),
            NeedSummary::Secret => "secret".to_owned(),
            NeedSummary::VaultCode => "vault code".to_owned(),
            NeedSummary::VaultUnlock => "vault unlock".to_owned(),
        },
    }
}

/// The mark in front of a row.
#[must_use]
pub fn glyph(state: RowState) -> char {
    match state {
        RowState::NeedsYou => '\u{25cf}',
        RowState::Working => '\u{25d0}',
        RowState::Failed => '\u{2717}',
        RowState::Done => '\u{2713}',
        RowState::Idle => '\u{00b7}',
    }
}

/// The title, or `(untitled)` when the title is empty.
#[must_use]
pub fn title_or_untitled(row: &FleetRow) -> String {
    if row.title.is_empty() {
        "(untitled)".to_owned()
    } else {
        row.title.clone()
    }
}

/// The model name after its provider prefix: the text after the last `/`.
#[must_use]
pub fn short_model(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

/// Whether a lower-cased query is a substring of the title, profile, short model, session id
/// or detail. An empty query matches every row.
#[must_use]
pub fn matches(row: &FleetRow, detail: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    let model = row.model.as_deref().map(short_model).unwrap_or_default();
    [
        row.title.as_str(),
        row.key.profile.as_str(),
        model,
        row.key.session_id.as_str(),
        detail,
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&query))
}
