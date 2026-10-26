//! One rate limit shared by foreground and background alerts.

use std::time::{Duration, Instant};

use crate::attention::{Alert, AlertKind};

/// Relative importance. Needs-you outranks failed, which outranks done.
pub fn urgency(kind: AlertKind) -> u8 {
    match kind {
        AlertKind::NeedsYou => 2,
        AlertKind::Failed => 1,
        AlertKind::Done => 0,
    }
}

/// The shared gate. A dropped alert does not move the window.
pub struct AlertGate {
    window: Duration,
    last: Option<(Instant, AlertKind)>,
}

impl AlertGate {
    /// A gate that has not emitted yet.
    pub fn new(window: Duration) -> AlertGate {
        AlertGate { window, last: None }
    }

    /// Emit `alert`, or drop it when a recent alert is at least as urgent.
    pub fn admit(&mut self, alert: Alert, at: Instant) -> Option<Alert> {
        if let Some((last_at, last_kind)) = self.last
            && at.saturating_duration_since(last_at) < self.window
            && urgency(alert.kind) <= urgency(last_kind)
        {
            return None;
        }
        self.last = Some((at, alert.kind));
        Some(alert)
    }
}
