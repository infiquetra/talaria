//! Background fleet attention: quiet window, watched keys, and alert text.
//!
//! Pure: no clock, no input or output, no terminal library.

#![allow(dead_code, unused_variables)]

use std::time::{Duration, Instant};

use crate::attention::{Alert, AlertKind, NeedSummary};
use crate::fleet::{FleetRow, NeedDetail, NeedId, Outcome, RowKey};

/// Which background rows may raise an alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FleetScope {
    /// Every background row.
    All,
    /// Rows this process has watched.
    Watched,
    /// Never.
    Off,
}

/// Channels, the done threshold, and the two scopes.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetAttentionConfig {
    /// Kinds that ring the terminal bell.
    pub bell: Vec<AlertKind>,
    /// Kinds that send a desktop notification.
    pub desktop: Vec<AlertKind>,
    /// A finished turn alerts only at or above this duration.
    pub done_after: Duration,
    /// Scope for needs-you alerts. Default is every background row.
    pub needs_you: FleetScope,
    /// Scope for done and failed alerts. Default is watched rows.
    pub finished: FleetScope,
}

/// Quiet time after the link connects, before the first full listing shortens it.
pub const QUIET_AFTER_CONNECT: Duration = Duration::from_secs(4);
/// Quiet time after the first snapshot whose profiles are all listed.
pub const QUIET_AFTER_SETTLED: Duration = Duration::from_secs(3);
/// The longest a quiet window stays open.
pub const QUIET_CAP: Duration = Duration::from_secs(15);

/// Watched keys, turn marks, and the quiet window for background alerts.
pub struct FleetAttention;

impl FleetAttention {
    /// A tracker with an empty watched set and no open window.
    pub fn new(cfg: FleetAttentionConfig, foreground_profile: String) -> FleetAttention {
        let _ = (cfg, foreground_profile);
        todo!("fleet attention")
    }

    /// The profile whose rows are not prefixed in a label.
    pub fn set_foreground_profile(&mut self, profile: String) {
        let _ = profile;
        todo!("fleet attention")
    }

    /// A link transition. Connecting opens a window and returns its provisional end.
    pub fn observe_link(&mut self, connected: bool, at: Instant) -> Option<Instant> {
        let _ = (connected, at);
        todo!("fleet attention")
    }

    /// The first full listing inside an open window fixes the window end.
    pub fn observe_settled(&mut self, at: Instant) -> Option<Instant> {
        let _ = at;
        todo!("fleet attention")
    }

    /// One snapshot: update the watched set and the turn marks.
    pub fn observe_rows(&mut self, rows: &[FleetRow], at: Instant) {
        let _ = (rows, at);
        todo!("fleet attention")
    }

    /// One attention notice, after the hub has finished holding it.
    #[allow(clippy::too_many_arguments)]
    pub fn on_attention(
        &mut self,
        kind: AlertKind,
        key: &RowKey,
        need: Option<&NeedId>,
        detail: Option<&NeedDetail>,
        foreground: bool,
        row: Option<&FleetRow>,
        at: Instant,
    ) -> Option<Alert> {
        let _ = (kind, key, need, detail, foreground, row, at);
        todo!("fleet attention")
    }

    /// A wake. Returns the start-up notice at most once per window.
    pub fn on_wake(
        &mut self,
        at: Instant,
        sessions_needing_you: usize,
        key_label: Option<&str>,
    ) -> Option<String> {
        let _ = (at, sessions_needing_you, key_label);
        todo!("fleet attention")
    }
}

/// The label of a row: title, or a short session id, with a profile prefix when needed.
pub fn row_label(key: &RowKey, title: &str, foreground_profile: &str) -> String {
    let _ = (key, title, foreground_profile);
    todo!("row label")
}

/// The phrase after `needs you: `, matching the foreground tracker.
pub fn need_phrase(summary: &NeedSummary) -> String {
    let _ = summary;
    todo!("need phrase")
}

/// Alert text before the product prefix. `None` means no alert.
pub fn fleet_alert_text(
    kind: AlertKind,
    label: &str,
    detail: Option<&NeedDetail>,
    outcome: Option<Outcome>,
) -> Option<String> {
    let _ = (kind, label, detail, outcome);
    todo!("fleet alert text")
}

/// The start-up notice for sessions already waiting.
pub fn startup_notice_text(sessions: usize, key_label: Option<&str>) -> String {
    let _ = (sessions, key_label);
    todo!("startup notice")
}
