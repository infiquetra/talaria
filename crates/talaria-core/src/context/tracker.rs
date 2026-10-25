//! The context tracker: what each session's readings mean (#234).

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

/// Per-session context state: staleness, revisions, compaction lines.
pub struct ContextTracker {
    // The wave-2 builder owns the shape; the red-run tests only need the type.
    _held: (),
}

impl ContextTracker {
    /// A tracker that has seen nothing.
    pub fn new() -> Self {
        unimplemented!("#234")
    }

    /// Mark this Talaria's compaction as running; a second one is refused.
    pub fn begin_compact(&mut self, _sid: &str, _now_ms: u64) -> Result<(), AlreadyRunning> {
        unimplemented!("#234")
    }

    /// Close this Talaria's compaction with how it ended.
    pub fn finish_compact(&mut self, _sid: &str, _end: CompactEnd) {
        unimplemented!("#234")
    }

    /// Feed one event by its type name and payload; returns notices to show.
    pub fn observe(
        &mut self,
        _sid: &str,
        _type_name: &str,
        _payload: &serde_json::Value,
        _now_ms: u64,
    ) -> Vec<ContextNotice> {
        unimplemented!("#234")
    }

    /// Whether the context figure must stay hidden for this session.
    pub fn context_hidden(&self, _sid: &str) -> bool {
        unimplemented!("#234")
    }

    /// The compacting line to show, if any. Manual wins over automatic.
    pub fn compacting_line(&self, _sid: &str) -> Option<CompactingLine> {
        unimplemented!("#234")
    }

    /// Drop everything known about a session.
    pub fn forget(&mut self, _sid: &str) {
        unimplemented!("#234")
    }
}

impl Default for ContextTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Blocks held back while a turn runs, committed in order at its end.
pub struct HeldBlocks<T> {
    _held: std::marker::PhantomData<T>,
}

impl<T> HeldBlocks<T> {
    /// Empty holder.
    pub fn new() -> Self {
        unimplemented!("#234")
    }

    /// Offer a block: held while a turn is active, else committed at once
    /// with any older held blocks first. Returns what to commit now.
    pub fn push(&mut self, _block: T, _turn_active: bool) -> Vec<T> {
        unimplemented!("#234")
    }

    /// Release everything held, in order, at the turn's end.
    pub fn release(&mut self) -> Vec<T> {
        unimplemented!("#234")
    }
}

impl<T> Default for HeldBlocks<T> {
    fn default() -> Self {
        Self::new()
    }
}
