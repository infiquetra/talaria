//! Needs-you queue identity.

use super::{NeedSummary, RowKey};

/// What a queue item is waiting on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NeedSlot {
    /// An approval or clarification prompt id.
    Prompt(String),
    /// A masked request id from an attached session.
    Masked(String),
    /// The response belongs to an external backend.
    External(String),
    /// The detail is not known yet.
    Waiting,
}

/// One queue item's identity. `seq` is not part of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NeedId {
    /// The row the item belongs to.
    pub key: RowKey,
    /// The slot.
    pub slot: NeedSlot,
}

/// Known detail, or still waiting to learn it.
#[derive(Clone, Debug, PartialEq)]
pub enum NeedDetail {
    /// The summary is known.
    Known(NeedSummary),
    /// The row is waiting and the read has not named an item.
    Waiting,
}

/// One queued need.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedItem {
    /// Identity.
    pub id: NeedId,
    /// Order. Assigned once and kept when a placeholder is refined.
    pub seq: u64,
    /// Seconds since 1970 when the item first appeared.
    pub since: f64,
    /// Summary, or still waiting.
    pub detail: NeedDetail,
    /// Offered choices. Empty when there are none.
    pub choices: Vec<String>,
    /// Set only for an exact item.
    pub execution_generation: Option<i64>,
    /// Timeout hint, when one was sent.
    pub timeout_s: Option<f64>,
    /// True only while an owner is reporting the item.
    pub exact: bool,
    /// The row is the foreground session.
    pub foreground: bool,
}

impl NeedItem {
    pub fn external_options(&self) -> &[super::ExternalOption] {
        todo!("add external fleet sessions")
    }
}
