//! Time, so every wait can run in milliseconds under test.

use std::time::Duration;

pub trait Clock {
    /// Time since the run started.
    fn now(&self) -> Duration;
    fn sleep(&self, d: Duration);
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        todo!("smoke")
    }

    fn sleep(&self, d: Duration) {
        let _ = d;
        todo!("smoke")
    }
}

/// A clock whose `sleep` advances time without waiting.
pub struct ManualClock {
    _private: (),
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualClock {
    pub fn new() -> Self {
        todo!("smoke")
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        todo!("smoke")
    }

    fn sleep(&self, d: Duration) {
        let _ = d;
        todo!("smoke")
    }
}
