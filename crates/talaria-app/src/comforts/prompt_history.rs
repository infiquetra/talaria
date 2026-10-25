//! Submitted prompt history: file store with in-memory views.
//!
//! Stubs. Behavior contract: issue 228.

use std::path::Path;
use std::sync::Arc;

use talaria_core::prompt_history::HistoryEntry;

/// The history store for one run.
#[derive(Debug)]
pub struct PromptHistory {
    /// Placeholder until the build adds the store.
    _private: (),
}

impl PromptHistory {
    /// Open the store, loading the newest entries for this workspace.
    pub fn open(dir: &Path, save: bool, workspace_key: &str) -> (Self, Option<String>) {
        let _ = (dir, save, workspace_key);
        todo!("#228")
    }

    /// Every entry known to this run, oldest first.
    pub fn snapshot(&self) -> Arc<[HistoryEntry]> {
        todo!("#228")
    }

    /// This workspace's newest entries for seeding the composer, oldest first.
    pub fn composer_seed(&self) -> Vec<String> {
        todo!("#228")
    }

    /// Record one submit, on disk and in memory.
    pub fn append(&mut self, text: &str) -> Option<String> {
        let _ = text;
        todo!("#228")
    }

    /// Shrink an oversized file to its newest lines. True when it compacted.
    pub fn compact(&self) -> std::io::Result<bool> {
        todo!("#228")
    }
}

/// The notice for a failed compaction. The caller shows it once per run.
pub fn compact_failed_notice(reason: &str) -> String {
    let _ = reason;
    todo!("#228")
}
