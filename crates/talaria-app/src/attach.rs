//! Find a session to attach to, across served profiles.

use std::time::Duration;

use serde_json::{Value, json};
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin, format_age, merge, order};

use crate::failure::LaunchFailure;
use crate::side_connection::{SideCallError, SideConnection, SideScope};

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
pub fn validate_reference(raw: &str) -> Result<String, ReferenceError> {
    if raw.chars().any(char::is_control) {
        return Err(ReferenceError::ControlCharacter);
    }
    let reference = raw.trim();
    let chars = reference.chars().count();
    if chars == 0 {
        Err(ReferenceError::Empty)
    } else if chars > MAX_REFERENCE_CHARS {
        Err(ReferenceError::TooLong { chars })
    } else {
        Ok(reference.to_owned())
    }
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
pub fn reference_shape(reference: &str) -> ReferenceShape {
    if let Some(hex) = reference.strip_prefix("local-")
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        match hex.len() {
            64 => return ReferenceShape::FullLocal,
            16 => return ReferenceShape::LocalShort,
            _ => {}
        }
    }
    let parts: Vec<_> = reference.split('_').collect();
    if let [date, time, suffix] = parts.as_slice()
        && date.len() == 8
        && date.bytes().all(|b| b.is_ascii_digit())
        && time.len() == 6
        && time.bytes().all(|b| b.is_ascii_digit())
        && !suffix.is_empty()
        && suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
    {
        ReferenceShape::Stored
    } else {
        ReferenceShape::Other
    }
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
pub fn resolve(reference: &str, rows: &[DirectoryRow]) -> Resolution {
    let lower = reference.to_lowercase();
    let prefix_allowed = lower
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        && !"local-".starts_with(&lower)
        && lower.strip_prefix("local-").unwrap_or(&lower).len() >= MIN_PREFIX_CHARS;
    for kind in [
        MatchKind::ExactId,
        MatchKind::IdPrefix,
        MatchKind::Title,
        MatchKind::TitleIgnoringCase,
    ] {
        let mut matches: Vec<_> = rows
            .iter()
            .filter(|row| match kind {
                MatchKind::ExactId => row.key.session_id == reference,
                MatchKind::IdPrefix => {
                    let sid = row.key.session_id.to_lowercase();
                    prefix_allowed
                        && (sid.starts_with(&lower)
                            || sid
                                .strip_prefix("local-")
                                .is_some_and(|tail| tail.starts_with(&lower)))
                }
                MatchKind::Title => !row.title.is_empty() && row.title == reference,
                MatchKind::TitleIgnoringCase => {
                    !row.title.is_empty() && row.title.to_lowercase() == lower
                }
            })
            .cloned()
            .collect();
        order(&mut matches);
        if matches.len() == 1 {
            return Resolution::One(Candidate {
                row: matches.remove(0),
                kind,
            });
        }
        if !matches.is_empty() {
            return Resolution::Many(
                matches
                    .into_iter()
                    .map(|row| Candidate { row, kind })
                    .collect(),
            );
        }
    }
    Resolution::NoMatch
}

/// Pure. The candidate block of text, at most [`MAX_CANDIDATE_LINES`] rows.
#[must_use]
pub fn candidate_lines(candidates: &[Candidate], now: f64) -> String {
    let mut lines: Vec<_> = candidates
        .iter()
        .take(MAX_CANDIDATE_LINES)
        .map(|candidate| {
            let row = &candidate.row;
            let title = if row.title.is_empty() {
                "(untitled)".to_owned()
            } else {
                format!("\"{}\"", shortened(&sanitize(&row.title), 40))
            };
            let origin = match row.origin {
                RowOrigin::Live => "live",
                RowOrigin::Stored => "earlier",
            };
            let running = if row.running { "  running" } else { "" };
            format!(
                "  {}  {}  {title}  {origin}{running}  {}",
                sanitize(&row.key.profile),
                shortened(&sanitize(&row.key.session_id), 14),
                format_age(row.started_at, now)
            )
        })
        .collect();
    if candidates.len() > MAX_CANDIDATE_LINES {
        lines.push(format!(
            "  …and {} more",
            candidates.len() - MAX_CANDIDATE_LINES
        ));
    }
    lines.join("\n")
}

fn shortened(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut result: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

/// The same visible control and direction-mark replacements as the terminal history.
fn sanitize(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        let code = u32::from(c);
        match code {
            9 => result.push_str("    "),
            0..=31 => result.push(char::from_u32(0x2400 + code).unwrap_or('�')),
            127 => result.push('␡'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x202a..=0x202e
            | 0x2066..=0x2069
            | 0x2028
            | 0x2029
            | 0xe0000..=0xe007f => result.push_str(&format!("‹U+{code:04X}›")),
            _ => result.push(c),
        }
    }
    result
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
pub fn fast_path(request: &AttachRequest) -> Option<RowKey> {
    let profile = request.profile.as_ref()?;
    matches!(
        reference_shape(&request.reference),
        ReferenceShape::FullLocal | ReferenceShape::Stored
    )
    .then(|| RowKey {
        profile: profile.clone(),
        session_id: request.reference.clone(),
    })
}

/// Pure. The profile whose id the side connection's ticket names:
/// `request.profile` when given; else `default` when served; else the first
/// served name.
#[must_use]
pub fn ticket_profile(request: &AttachRequest, served: &[String]) -> Option<String> {
    request.profile.clone().or_else(|| {
        served
            .iter()
            .find(|p| p.as_str() == "default")
            .or_else(|| served.first())
            .cloned()
    })
}

/// Network step. `served` is the names from discovery, in that order. `now` is
/// Unix time in seconds. Returns the session to open, or an [`AttachFailure`].
/// Opens and closes the side connection; never resumes.
pub async fn find_session(
    home: &std::path::Path,
    request: &AttachRequest,
    served: &[String],
    now: f64,
) -> Result<AttachTarget, AttachFailure> {
    if let Some(profile) = &request.profile
        && !served.contains(profile)
    {
        let profile = sanitize(profile);
        return Err(attach_failure(
            Vec::new(),
            format!(
                "the profile {profile} is not served by the gateway (served: {}).",
                served
                    .iter()
                    .map(|p| sanitize(p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!("pick a served profile, or ask the operator to serve {profile}."),
            2,
        ));
    }
    let Some(profile) = ticket_profile(request, served) else {
        return Err(list_failure(Vec::new(), "gateway not found"));
    };
    let connection = SideConnection::open(home, &profile)
        .await
        .map_err(|error| list_failure(Vec::new(), error.phrase()))?;
    let mut notes = Vec::new();
    let host = matches!(connection.scope(), SideScope::Host { .. });
    let profiles = match (&request.profile, connection.scope()) {
        (Some(profile), _) => vec![profile.clone()],
        (None, SideScope::Host { .. }) => served.to_vec(),
        (None, SideScope::ProfileOnly { profile, why }) => {
            notes.push(format!(
                "talaria: note: searched only the profile {} ({}).",
                sanitize(profile),
                sanitize(why)
            ));
            vec![profile.clone()]
        }
    };
    let calls: Vec<_> = profiles
        .iter()
        .flat_map(|profile| {
            [
                (profile, "live", RowOrigin::Live),
                (profile, "stored", RowOrigin::Stored),
            ]
        })
        .collect();
    let replies = list_all(&connection, &calls, host).await;
    connection.close().await;
    let mut live = Vec::new();
    let mut stored = Vec::new();
    let mut searched = vec![false; profiles.len()];
    let mut first_failure = None;
    for (index, ((profile, scope, origin), reply)) in calls.iter().zip(replies).enumerate() {
        match reply {
            Ok(value) => {
                searched[index / 2] = true;
                let rows = if *origin == RowOrigin::Live {
                    &mut live
                } else {
                    &mut stored
                };
                if let Some(values) = value["sessions"].as_array() {
                    rows.extend(
                        values
                            .iter()
                            .filter_map(|row| DirectoryRow::from_list_row(profile, *origin, row)),
                    );
                }
            }
            Err(error) => {
                let reason = if error == SideCallError::TooLarge {
                    "reply too large".to_owned()
                } else {
                    error.phrase()
                };
                first_failure.get_or_insert_with(|| reason.clone());
                notes.push(format!("talaria: note: could not list the {scope} sessions of {} ({}); those were not searched.", sanitize(profile), sanitize(&reason)));
            }
        }
    }
    if !searched.iter().any(|ok| *ok) {
        return Err(list_failure(
            notes,
            first_failure.as_deref().unwrap_or("gateway not found"),
        ));
    }
    match resolve(&request.reference, &merge(live, stored)) {
        Resolution::One(candidate) => Ok(AttachTarget {
            key: candidate.row.key,
            notes,
        }),
        Resolution::Many(candidates) => Err(attach_failure(
            notes,
            format!(
                "{} sessions match \"{}\":\n{}",
                candidates.len(),
                sanitize(&request.reference),
                candidate_lines(&candidates, now)
            ),
            "use more of the id, or add --profile <name>.".to_owned(),
            2,
        )),
        Resolution::NoMatch
            if profiles.len() == 1
                && reference_shape(&request.reference) != ReferenceShape::Other =>
        {
            Ok(AttachTarget {
                key: RowKey {
                    profile: profiles[0].clone(),
                    session_id: request.reference.clone(),
                },
                notes,
            })
        }
        Resolution::NoMatch => {
            let names = profiles
                .iter()
                .zip(searched)
                .filter(|(_, ok)| *ok)
                .map(|(profile, _)| sanitize(profile))
                .collect::<Vec<_>>();
            let scope = if names.len() == 1 {
                "profile"
            } else {
                "profiles"
            };
            Err(attach_failure(notes, format!("no session matches \"{}\" in the {scope} {}.", sanitize(&request.reference), names.join(", ")), "run \"talaria fleet list\" or \"talaria --resume\" to see sessions, or pass the full id with --profile <name>.".to_owned(), 1))
        }
    }
}

fn attach_failure(
    notes: Vec<String>,
    what: String,
    next_step: String,
    exit_code: u8,
) -> AttachFailure {
    AttachFailure {
        notes,
        failure: LaunchFailure {
            what,
            next_step,
            exit_code,
        },
    }
}

fn list_failure(notes: Vec<String>, reason: &str) -> AttachFailure {
    attach_failure(
        notes,
        format!("could not list sessions ({}).", sanitize(reason)),
        "try again; if it keeps failing, pass the full id with --profile <name>.".to_owned(),
        1,
    )
}

/// Poll every call together and keep results in served-profile, then scope order.
async fn list_all(
    connection: &SideConnection,
    calls: &[(&String, &str, RowOrigin)],
    host: bool,
) -> Vec<Result<Value, SideCallError>> {
    use std::future::{Future, poll_fn};
    use std::task::Poll;

    let mut pending: Vec<_> = calls
        .iter()
        .map(|(profile, scope, _)| {
            let mut params = json!({ "scope": scope, "limit": LIST_LIMIT });
            if host {
                params["profile"] = json!(profile);
            }
            Some(Box::pin(connection.call(
                "session.list",
                params,
                LIST_TIMEOUT,
            )))
        })
        .collect();
    let mut results: Vec<_> = (0..calls.len()).map(|_| None).collect();
    poll_fn(|cx| {
        for (index, slot) in pending.iter_mut().enumerate() {
            if let Some(future) = slot
                && let Poll::Ready(reply) = future.as_mut().poll(cx)
            {
                results[index] = Some(reply);
                *slot = None;
            }
        }
        if pending.iter().all(Option::is_none) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    results.into_iter().flatten().collect()
}
