//! Pull-request status: query, parse, label. Pure. Issue #245.

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
    let _ = (status, stale);
    todo!()
}

/// The GraphQL query that looks up `branches` in `owner`/`name`.
pub fn pr_query(owner: &str, name: &str, branches: &[String]) -> String {
    let _ = (owner, name, branches);
    todo!()
}

/// Parse one status reply: the first own-repository node per branch alias.
pub fn parse_pr_reply(json: &str, branches: &[String]) -> Result<Vec<PrStatus>, String> {
    let _ = (json, branches);
    todo!()
}

/// De-duplicate branches, keep the first 300, split into 50s.
pub fn pr_chunks(branches: &[String]) -> Vec<Vec<String>> {
    let _ = branches;
    todo!()
}
