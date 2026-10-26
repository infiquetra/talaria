//! Find a session to attach to, across served profiles. Issue #246.
//!
//! Every function here is a red-run stub: pure functions return a fixed wrong
//! value, and the network step returns a canned failure, so each test fails at
//! its own assertion until the resolver exists.

use std::time::Duration;

use talaria_core::directory::rows::{DirectoryRow, RowKey};

use crate::failure::LaunchFailure;

/// The longest session reference `attach` accepts, counted in characters.
pub const MAX_REFERENCE_CHARS: usize = 200;
/// The fewest characters after an optional leading `local-` that still count as an id prefix.
pub const MIN_PREFIX_CHARS: usize = 4;
/// Rows asked for per profile and scope.
pub const LIST_LIMIT: u32 = 200;
/// How long one list call may take.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Candidate rows printed before the `…and <n> more` line.
pub const MAX_CANDIDATE_LINES: usize = 10;

/// Why a session reference was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceError {
    Empty,
    TooLong { chars: usize },
    ControlCharacter,
}

/// Trims surrounding whitespace; refuses empty, longer than
/// [`MAX_REFERENCE_CHARS`] (counted in chars), or containing any char in C0
/// (U+0000..U+001F), DEL (U+007F) or C1 (U+0080..U+009F).
pub fn validate_reference(_raw: &str) -> Result<String, ReferenceError> {
    Err(ReferenceError::Empty)
}

/// The shape of a validated reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceShape {
    FullLocal,
    Stored,
    LocalShort,
    Other,
}

/// Case-sensitive, matched exactly: FullLocal `^local-[0-9a-f]{64}$`; Stored
/// `^[0-9]{8}_[0-9]{6}_[0-9a-z]+$`; LocalShort `^local-[0-9a-f]{16}$`; Other:
/// anything else (upper-case forms included).
#[must_use]
pub fn reference_shape(_reference: &str) -> ReferenceShape {
    ReferenceShape::Other
}

/// Which resolution tier matched a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind {
    ExactId,
    IdPrefix,
    Title,
    TitleIgnoringCase,
}

/// One matching row and how it matched.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub row: DirectoryRow,
    pub kind: MatchKind,
}

/// What `resolve` found.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    One(Candidate),
    Many(Vec<Candidate>),
    NoMatch,
}

/// Pure. Tiers in order, stopping at the first tier with a match: exact id,
/// id prefix, exact title, title ignoring case.
#[must_use]
pub fn resolve(_reference: &str, _rows: &[DirectoryRow]) -> Resolution {
    Resolution::NoMatch
}

/// Pure. The candidate block of text, at most [`MAX_CANDIDATE_LINES`] rows.
#[must_use]
pub fn candidate_lines(_candidates: &[Candidate], _now: f64) -> String {
    String::new()
}

/// What the operator asked to attach to.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachRequest {
    pub reference: String,
    pub profile: Option<String>,
    pub no_ensure: bool,
}

/// The session to open, with the notes for standard error gathered while finding it.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachTarget {
    pub key: RowKey,
    pub notes: Vec<String>,
}

/// A failed find. `failure` is the resume path's [`LaunchFailure`] (issue
/// #209), built by its public fields; `notes` are printed before `render()`.
#[derive(Clone, Debug)]
pub struct AttachFailure {
    pub notes: Vec<String>,
    pub failure: LaunchFailure,
}

/// Pure. Some(key) exactly when `profile` is Some and the reference shape is
/// FullLocal or Stored.
#[must_use]
pub fn fast_path(_request: &AttachRequest) -> Option<RowKey> {
    None
}

/// Pure. The profile whose id the side connection's ticket names:
/// `request.profile` when given; else `default` when served; else the first
/// served name.
#[must_use]
pub fn ticket_profile(_request: &AttachRequest, _served: &[String]) -> Option<String> {
    None
}

/// Network step. `served` is the names from discovery, in that order. `now` is
/// Unix time in seconds. Returns the session to open, or an [`AttachFailure`].
/// Opens and closes the side connection; never resumes.
pub async fn find_session(
    _home: &std::path::Path,
    _request: &AttachRequest,
    _served: &[String],
    _now: f64,
) -> Result<AttachTarget, AttachFailure> {
    Err(AttachFailure {
        notes: Vec::new(),
        failure: LaunchFailure {
            what: "stub".to_owned(),
            next_step: "stub".to_owned(),
            exit_code: 1,
        },
    })
}
