//! Decide fleet notices: hold them until the snapshot, then emit effects.

#![allow(dead_code, unused_variables)]

use std::time::{Duration, Instant};

use talaria_core::alert_gate::AlertGate;
use talaria_core::attention::Alert;
use talaria_core::fleet::RowKey;
use talaria_core::fleet_attention::{FleetAttention, FleetAttentionConfig};

use crate::fleet::{FleetNotice, FleetSnapshot};

/// Status-line counters. `needs_you` counts requests; the others count background rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusCounts {
    /// Foreground requests plus background queue items.
    pub needs_you: u32,
    /// Background rows working.
    pub working: u32,
    /// Background rows failed.
    pub failed: u32,
    /// Background rows done.
    pub done: u32,
}

/// Counters for the status line. Without a snapshot, only `foreground_open` is kept.
pub fn status_counts(foreground_open: u32, snapshot: Option<&FleetSnapshot>) -> StatusCounts {
    let _ = (foreground_open, snapshot);
    todo!("status counts")
}

/// Where the jump key goes.
#[derive(Clone, Debug, PartialEq)]
pub enum NextNeed {
    /// Open the fleet view on this row.
    Peek(RowKey),
    /// A sentence for the notice path. It does not ring.
    Notice(String),
}

/// The oldest background request, or the sentence that says why there is none.
pub fn next_need(snapshot: Option<&FleetSnapshot>, fleet_enabled: bool) -> NextNeed {
    let _ = (snapshot, fleet_enabled);
    todo!("next need")
}

/// What one notice or wake asks the application to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubOutput {
    /// Alerts the shared gate admitted.
    pub alerts: Vec<Alert>,
    /// A start-up or jump notice. Never rings.
    pub notice: Option<String>,
    /// The next wake, when a window end or a hold deadline moved.
    pub wake_at: Option<Instant>,
}

/// How long a notice may wait for the snapshot that describes it.
pub const HOLD_LIMIT: Duration = Duration::from_secs(1);
/// The most notices held at once. The oldest is decided when another arrives.
pub const HOLD_MAX: usize = 64;

/// The fleet tracker, the shared gate, and notices waiting for a snapshot.
pub struct AttentionHub;

impl AttentionHub {
    /// A hub with an empty hold queue.
    pub fn new(
        cfg: FleetAttentionConfig,
        foreground_profile: String,
        key_label: Option<String>,
    ) -> AttentionHub {
        let _ = (cfg, foreground_profile, key_label);
        todo!("attention hub")
    }

    /// One fleet notice, decided now or held until the snapshot agrees.
    pub fn on_notice(
        &mut self,
        notice: &FleetNotice,
        latest: &FleetSnapshot,
        at: Instant,
    ) -> HubOutput {
        let _ = (notice, latest, at);
        todo!("attention hub")
    }

    /// A foreground alert passes the same gate as fleet alerts.
    pub fn on_foreground_alert(&mut self, alert: Alert, at: Instant) -> Option<Alert> {
        let _ = (alert, at);
        todo!("attention hub")
    }

    /// A wake: decide held notices whose deadline has passed, then the start-up notice.
    pub fn on_wake(&mut self, at: Instant, latest: Option<&FleetSnapshot>) -> HubOutput {
        let _ = (at, latest);
        todo!("attention hub")
    }

    /// The profile whose rows are not prefixed in a label.
    pub fn set_foreground_profile(&mut self, profile: String) {
        let _ = profile;
        todo!("attention hub")
    }
}

/// Wait until `at`, then call `send` once.
pub fn wake_task(at: Instant, send: impl FnOnce() + Send + 'static) -> tokio::task::JoinHandle<()> {
    let _ = (at, send);
    todo!("wake task")
}

/// Keep the gate type in this module's public surface for the application wiring.
#[allow(dead_code)]
fn _gate(gate: AlertGate) -> AlertGate {
    gate
}

/// Keep the tracker type referenced until the hub stores one.
#[allow(dead_code)]
fn _tracker(tracker: FleetAttention) -> FleetAttention {
    tracker
}
