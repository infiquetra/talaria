//! How one pasted string is routed. Pure: no file and no clipboard.

use std::path::Path;

use super::chips::CollapseRule;
use super::drop::DropCandidate;

/// What to do with one paste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteRoute {
    /// Look for a clipboard image. Insert `fallback_text` only when none is found.
    ProbeClipboard { fallback_text: Option<String> },
    /// Resolve these paths off the input thread.
    Drop(Vec<DropCandidate>),
    /// The paste is large enough to collapse.
    Collapse,
    /// Insert the paste as typed.
    Inline,
}

/// Route `text` after the paste sanitiser has already run.
#[must_use]
pub fn route_paste(text: &str, rule: &CollapseRule, home: &Path) -> PasteRoute {
    let _ = (text, rule, home);
    unimplemented!()
}
