//! CPU time and resident memory of a child process.

use std::time::Instant;

use crate::error::BenchError;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub at: Instant,
    pub cpu_seconds: f64,
    pub rss_bytes: u64,
}

pub fn sample(pid: u32) -> Result<Sample, BenchError> {
    let _ = pid;
    todo!("bench")
}

/// CPU seconds used over wall seconds elapsed, times 100.
pub fn cpu_percent(earlier: &Sample, later: &Sample) -> f64 {
    let _ = (earlier, later);
    todo!("bench")
}
