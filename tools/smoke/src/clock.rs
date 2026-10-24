//! Time, so every wait can run in milliseconds under test.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub trait Clock {
    /// Time since the run started.
    fn now(&self) -> Duration;
    fn sleep(&self, d: Duration);
}

/// The real clock: time since the process started, and real sleeps.
pub struct SystemClock {
    started: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.started.elapsed()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// A clock whose `sleep` advances time without waiting.
pub struct ManualClock {
    now: Mutex<Duration>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            now: Mutex::new(Duration::ZERO),
        }
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.now.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn sleep(&self, d: Duration) {
        let mut now = self.now.lock().unwrap_or_else(|error| error.into_inner());
        *now += d;
    }
}
