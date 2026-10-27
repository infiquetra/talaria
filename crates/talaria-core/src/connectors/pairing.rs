//! Pairing rows. Names and identifiers stay out of debug output.

use serde::Deserialize;

/// Pending and approved pairing rows.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct Pairing {
    #[serde(default)]
    pub pending: Vec<PendingPair>,
    #[serde(default)]
    pub approved: Vec<ApprovedPair>,
}

impl std::fmt::Debug for Pairing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pairing")
            .field("pending", &self.pending.len())
            .field("approved", &self.approved.len())
            .finish()
    }
}

/// Someone waiting to pair.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct PendingPair {
    pub platform: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub age_minutes: i64,
}

impl std::fmt::Debug for PendingPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingPair")
            .field("platform", &self.platform)
            .field("request_len", &self.request_id.len())
            .field("user_len", &self.user_id.len())
            .finish()
    }
}

/// Someone already paired.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct ApprovedPair {
    pub platform: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
}

impl std::fmt::Debug for ApprovedPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApprovedPair")
            .field("platform", &self.platform)
            .field("user_len", &self.user_id.len())
            .finish()
    }
}
