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
    #[cfg(target_os = "linux")]
    {
        static TICKS: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
        let ticks = TICKS
            .get_or_init(|| {
                std::process::Command::new("getconf")
                    .arg("CLK_TCK")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|s| s.trim().parse().ok())
                    .filter(|value: &f64| value.is_finite() && *value > 0.0)
            })
            .ok_or_else(|| BenchError::Io("CPU clock rate unavailable".into()))?;
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|_| BenchError::Io("process sample".into()))?;
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .ok_or_else(|| BenchError::Io("process stat".into()))?
            .1
            .split_whitespace()
            .collect();
        let number = |index: usize| {
            fields
                .get(index)
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| BenchError::Io("process stat".into()))
        };
        let cpu_seconds = (number(11)? + number(12)?) / ticks;
        let memory = std::fs::read_to_string(format!("/proc/{pid}/status"))
            .map_err(|_| BenchError::Io("process memory".into()))?;
        let rss_bytes = memory
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))
            .and_then(|l| l.split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
            .ok_or_else(|| BenchError::Io("process memory".into()))?
            * 1024;
        Ok(Sample {
            at: Instant::now(),
            cpu_seconds,
            rss_bytes,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=,time=", "-p", &pid.to_string()])
            .output()
            .map_err(|_| BenchError::Io("process sample".into()))?;
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split_whitespace();
        let rss_bytes = fields
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| BenchError::Io("process memory".into()))?
            * 1024;
        let time = fields
            .next()
            .ok_or_else(|| BenchError::Io("process time".into()))?;
        let mut seconds = 0.0;
        for field in time.split(':') {
            seconds = seconds * 60.0
                + field
                    .parse::<f64>()
                    .map_err(|_| BenchError::Io("process time".into()))?;
        }
        Ok(Sample {
            at: Instant::now(),
            cpu_seconds: seconds,
            rss_bytes,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        Err(BenchError::UnsupportedOs)
    }
}

/// CPU seconds used over wall seconds elapsed, times 100.
pub fn cpu_percent(earlier: &Sample, later: &Sample) -> f64 {
    let wall = later.at.saturating_duration_since(earlier.at).as_secs_f64();
    if wall == 0.0 {
        0.0
    } else {
        (later.cpu_seconds - earlier.cpu_seconds).max(0.0) / wall * 100.0
    }
}
