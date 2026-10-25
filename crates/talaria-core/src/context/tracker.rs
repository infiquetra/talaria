//! The context tracker: what each session's readings mean (#234).

use std::collections::HashMap;

use serde_json::Value;

use super::compact::CompactEnd;

/// A compaction in progress, manual or automatic, with its start in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactingLine {
    Manual { since_ms: u64 },
    Auto { since_ms: u64 },
}

/// A notice the tracker raises for the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextNotice {
    AutoCompacted,
    CompactedElsewhere {
        revision: i64,
        headline: Option<String>,
    },
}

/// A compaction is already running for this session.
#[derive(Debug, PartialEq, Eq)]
pub struct AlreadyRunning;

/// What one session's readings say.
#[derive(Clone, Debug, Default)]
struct SessionContext {
    /// The context figure must stay hidden.
    stale: bool,
    /// This Talaria has a compaction call outstanding.
    own_in_flight: bool,
    /// The highest compaction revision this view knows.
    known_revision: Option<i64>,
    /// When the outstanding manual compaction started.
    manual_since: Option<u64>,
    /// When the current automatic compaction started.
    auto_since: Option<u64>,
    /// An automatic notice was given and no `compacting` has arrived since.
    last_notice_auto: bool,
    /// This Talaria's last compaction ended with an unknown outcome.
    own_unsettled: bool,
}

/// Per-session context state: staleness, revisions, compaction lines.
pub struct ContextTracker {
    sessions: HashMap<String, SessionContext>,
}

impl ContextTracker {
    /// A tracker that has seen nothing.
    pub fn new() -> Self {
        ContextTracker {
            sessions: HashMap::new(),
        }
    }

    /// Mark this Talaria's compaction as running; a second one is refused.
    pub fn begin_compact(&mut self, sid: &str, now_ms: u64) -> Result<(), AlreadyRunning> {
        let session = self.sessions.entry(sid.to_owned()).or_default();
        if session.own_in_flight {
            return Err(AlreadyRunning);
        }
        session.own_in_flight = true;
        session.manual_since = Some(now_ms);
        session.own_unsettled = false;
        Ok(())
    }

    /// Close this Talaria's compaction with how it ended.
    pub fn finish_compact(&mut self, sid: &str, end: CompactEnd) {
        let session = self.sessions.entry(sid.to_owned()).or_default();
        session.own_in_flight = false;
        session.manual_since = None;
        match end {
            CompactEnd::Committed { revision } => {
                session.known_revision = Some(
                    session
                        .known_revision
                        .map_or(revision, |known| known.max(revision)),
                );
                session.stale = true;
            }
            CompactEnd::Unknown => {
                session.own_unsettled = true;
                session.stale = true;
            }
            CompactEnd::NothingChanged => {}
        }
    }

    /// Feed one event by its type name and payload; returns notices to show.
    pub fn observe(
        &mut self,
        sid: &str,
        type_name: &str,
        payload: &Value,
        now_ms: u64,
    ) -> Vec<ContextNotice> {
        let session = self.sessions.entry(sid.to_owned()).or_default();
        let mut notices = Vec::new();
        match type_name {
            "session.usage" => {
                if context_percent(payload).is_some() {
                    session.stale = false;
                }
            }
            "message.complete" => {
                if context_percent(payload).is_some() {
                    session.stale = false;
                }
                session.auto_since = None;
            }
            "status.update" => match payload["kind"].as_str() {
                Some("compacting") => {
                    session.auto_since = session.auto_since.or(Some(now_ms));
                    session.last_notice_auto = false;
                }
                Some("compacted") => {
                    session.auto_since = None;
                    session.stale = true;
                    if !session.last_notice_auto {
                        session.last_notice_auto = true;
                        notices.push(ContextNotice::AutoCompacted);
                    }
                }
                _ => {}
            },
            "session.updated" => {
                if payload["operation"].as_str() != Some("compress") {
                    return notices;
                }
                let revision = payload["revision"].as_i64();
                if session.own_in_flight {
                    session.stale = true;
                    if let Some(revision) = revision {
                        session.known_revision = Some(
                            session
                                .known_revision
                                .map_or(revision, |known| known.max(revision)),
                        );
                    }
                    return notices;
                }
                if session.own_unsettled {
                    session.stale = true;
                    session.own_unsettled = false;
                    if let Some(revision) = revision {
                        session.known_revision = Some(
                            session
                                .known_revision
                                .map_or(revision, |known| known.max(revision)),
                        );
                    }
                    return notices;
                }
                match revision {
                    Some(revision) => {
                        if session
                            .known_revision
                            .is_some_and(|known| revision <= known)
                        {
                            return notices;
                        }
                        session.stale = true;
                        session.known_revision = Some(revision);
                        notices.push(ContextNotice::CompactedElsewhere {
                            revision,
                            headline: payload["summary"]["headline"].as_str().map(str::to_owned),
                        });
                    }
                    None => session.stale = true,
                }
            }
            _ => {}
        }
        notices
    }

    /// Whether the context figure must stay hidden for this session.
    pub fn context_hidden(&self, sid: &str) -> bool {
        self.sessions.get(sid).is_some_and(|session| session.stale)
    }

    /// The compacting line to show, if any. Manual wins over automatic.
    pub fn compacting_line(&self, sid: &str) -> Option<CompactingLine> {
        let session = self.sessions.get(sid)?;
        if let Some(since_ms) = session.manual_since {
            return Some(CompactingLine::Manual { since_ms });
        }
        session
            .auto_since
            .map(|since_ms| CompactingLine::Auto { since_ms })
    }

    /// Drop everything known about a session.
    pub fn forget(&mut self, sid: &str) {
        self.sessions.remove(sid);
    }
}

impl Default for ContextTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// The context percentage of a reading, when it is an integer.
fn context_percent(payload: &Value) -> Option<i64> {
    payload["usage"]["context_percent"].as_i64()
}

/// Blocks held back while a turn runs, committed in order at its end.
pub struct HeldBlocks<T> {
    held: Vec<T>,
}

impl<T> HeldBlocks<T> {
    /// Empty holder.
    pub fn new() -> Self {
        HeldBlocks { held: Vec::new() }
    }

    /// Offer a block: held while a turn is active, else committed at once
    /// with any older held blocks first. Returns what to commit now.
    pub fn push(&mut self, block: T, turn_active: bool) -> Vec<T> {
        if turn_active {
            self.held.push(block);
            return Vec::new();
        }
        let mut ready = std::mem::take(&mut self.held);
        ready.push(block);
        ready
    }

    /// Release everything held, in order, at the turn's end.
    pub fn release(&mut self) -> Vec<T> {
        std::mem::take(&mut self.held)
    }
}

impl<T> Default for HeldBlocks<T> {
    fn default() -> Self {
        Self::new()
    }
}
