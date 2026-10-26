//! Pull-request status: query, parse, label. Pure. Issue #245.

use std::collections::BTreeSet;

use serde_json::Value;

/// How many branches one status request carries.
pub const PR_QUERY_CHUNK: usize = 50;
/// How many branches one refresh looks up in all.
pub const PR_QUERY_CAP: usize = 300;

/// The state of a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// The combined check state of a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    Passing,
    Failing,
    Pending,
    None,
}

/// The review decision of a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

/// One pull request's status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrStatus {
    /// The pull-request number.
    pub number: u64,
    /// Open, closed or merged.
    pub state: PrState,
    /// A draft pull request.
    pub draft: bool,
    /// The combined check state.
    pub checks: Checks,
    /// The review decision, when one is known.
    pub review: Option<ReviewDecision>,
    /// The pull-request URL.
    pub url: String,
    /// The head branch.
    pub branch: String,
}

/// The tone a pull-request label draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelTone {
    Success,
    Error,
    Warning,
    Muted,
    Accent,
}

/// One pull-request label: text and tone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrLabel {
    /// The label text, glyph included.
    pub text: String,
    /// The tone it draws in.
    pub tone: LabelTone,
}

/// The label for a pull-request status. Stale labels keep the text in `Muted`.
pub fn pr_label(status: &PrStatus, stale: bool) -> PrLabel {
    let number = status.number;
    let (text, tone) = match status.state {
        PrState::Merged => (format!("#{number} merged"), LabelTone::Accent),
        PrState::Closed => (format!("#{number} closed"), LabelTone::Muted),
        PrState::Open => {
            let kind = if status.draft { "draft" } else { "open" };
            let (glyph, tone) = match status.checks {
                Checks::Passing => (" ✓", LabelTone::Success),
                Checks::Failing => (" ✗", LabelTone::Error),
                Checks::Pending => (" ◐", LabelTone::Warning),
                Checks::None => ("", LabelTone::Muted),
            };
            (format!("#{number} {kind}{glyph}"), tone)
        }
    };
    PrLabel {
        text,
        tone: if stale { LabelTone::Muted } else { tone },
    }
}

/// The GraphQL query that looks up `branches` in `owner`/`name`.
pub fn pr_query(owner: &str, name: &str, branches: &[String]) -> String {
    let mut query = format!(
        "query {{ repository(owner: {}, name: {}) {{",
        quoted(owner),
        quoted(name)
    );
    for (index, branch) in branches.iter().enumerate() {
        query.push_str(&format!(
            " b{index}: pullRequests(headRefName: {}, first: 5, orderBy: {{field: CREATED_AT, direction: DESC}}) {{ nodes {{ number state isDraft isCrossRepository url headRefName reviewDecision commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ state }} }} }} }} }} }}",
            quoted(branch)
        ));
    }
    query.push_str(" } }");
    query
}

/// A JSON string literal, which GraphQL reads as the same string.
fn quoted(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// Parse one status reply: the first own-repository node per branch alias.
pub fn parse_pr_reply(json: &str, branches: &[String]) -> Result<Vec<PrStatus>, String> {
    let reply: Value =
        serde_json::from_str(json).map_err(|error| format!("the reply is not JSON: {error}"))?;
    let Some(repository) = reply
        .pointer("/data/repository")
        .filter(|value| value.is_object())
    else {
        let reason = reply
            .pointer("/errors/0/message")
            .and_then(Value::as_str)
            .unwrap_or("the reply holds no repository");
        return Err(reason.to_owned());
    };
    let mut statuses = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        let Some(nodes) = repository
            .get(format!("b{index}"))
            .and_then(|alias| alias.get("nodes"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let own = nodes
            .iter()
            .find(|node| node.get("isCrossRepository").and_then(Value::as_bool) != Some(true));
        if let Some(node) = own {
            statuses.push(status_of(node, branch)?);
        }
    }
    Ok(statuses)
}

fn status_of(node: &Value, branch: &str) -> Result<PrStatus, String> {
    let number = node
        .get("number")
        .and_then(Value::as_u64)
        .ok_or("a pull request has no number")?;
    let state = match node.get("state").and_then(Value::as_str) {
        Some("OPEN") => PrState::Open,
        Some("CLOSED") => PrState::Closed,
        Some("MERGED") => PrState::Merged,
        other => return Err(format!("unknown pull request state {other:?}")),
    };
    let rollup = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
        .and_then(Value::as_str);
    let checks = match rollup {
        Some("SUCCESS") => Checks::Passing,
        Some("FAILURE" | "ERROR") => Checks::Failing,
        Some("PENDING" | "EXPECTED") => Checks::Pending,
        _ => Checks::None,
    };
    let review = match node.get("reviewDecision").and_then(Value::as_str) {
        Some("APPROVED") => Some(ReviewDecision::Approved),
        Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
        Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
        _ => None,
    };
    Ok(PrStatus {
        number,
        state,
        draft: node
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        checks,
        review,
        url: node
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        branch: branch.to_owned(),
    })
}

/// De-duplicate branches, keep the first 300, split into 50s.
pub fn pr_chunks(branches: &[String]) -> Vec<Vec<String>> {
    let mut seen = BTreeSet::new();
    let unique: Vec<String> = branches
        .iter()
        .filter(|branch| seen.insert(branch.as_str()))
        .take(PR_QUERY_CAP)
        .cloned()
        .collect();
    unique
        .chunks(PR_QUERY_CHUNK)
        .map(<[String]>::to_vec)
        .collect()
}
