//! A fleet seam that records registry calls in order.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use std::sync::Mutex;

use talaria_app::peek::FleetSink;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::SubscribedView;

/// One recorded registry call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetCall {
    Subscribed(RowKey),
    Unsubscribed(RowKey),
    Acknowledge(RowKey),
}

/// Records [`FleetSink`] calls in the order they happen.
#[derive(Debug, Default)]
pub struct FakeFleetSink {
    calls: Mutex<Vec<FleetCall>>,
}

impl FakeFleetSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<FleetCall> {
        self.calls.lock().expect("fleet sink lock").clone()
    }
}

impl FleetSink for FakeFleetSink {
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView) {
        let _ = view;
        self.calls
            .lock()
            .expect("fleet sink lock")
            .push(FleetCall::Subscribed(key));
    }

    fn observe_unsubscribed(&self, key: RowKey) {
        self.calls
            .lock()
            .expect("fleet sink lock")
            .push(FleetCall::Unsubscribed(key));
    }

    fn acknowledge(&self, key: RowKey) {
        self.calls
            .lock()
            .expect("fleet sink lock")
            .push(FleetCall::Acknowledge(key));
    }
}
