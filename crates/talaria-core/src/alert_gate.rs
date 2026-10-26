//! One rate limit shared by foreground and background alerts.

#![allow(dead_code, unused_variables)]

use std::time::{Duration, Instant};

use crate::attention::{Alert, AlertKind};

/// Relative importance. Needs-you outranks failed, which outranks done.
pub fn urgency(kind: AlertKind) -> u8 {
    let _ = kind;
    todo!("urgency")
}

/// The shared gate. A dropped alert does not move the window.
pub struct AlertGate;

impl AlertGate {
    /// A gate that has not emitted yet.
    pub fn new(window: Duration) -> AlertGate {
        let _ = window;
        todo!("alert gate")
    }

    /// Emit `alert`, or drop it when a recent alert is at least as urgent.
    pub fn admit(&mut self, alert: Alert, at: Instant) -> Option<Alert> {
        let _ = (alert, at);
        todo!("alert gate")
    }
}
