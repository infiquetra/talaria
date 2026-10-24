//! The sampler reads CPU time and resident memory of a child process.

mod support;

use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use talaria_bench::{Sample, cpu_percent, sample};

/// Kills and reaps the child however the test ends, so no process is left behind.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_fake_tui(args: &[&str]) -> ChildGuard {
    let child = Command::new(support::fake_tui_path())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fake-tui");
    ChildGuard(child)
}

/// Samples the child twice, about one second apart, after giving it time to start.
fn sample_twice(guard: &mut ChildGuard) -> (Sample, Sample) {
    sleep(Duration::from_millis(500));
    let pid = guard.0.id();
    let earlier = sample(pid).unwrap_or_else(|error| panic!("first sample failed: {error}"));
    sleep(Duration::from_secs(1));
    let later = sample(pid).unwrap_or_else(|error| panic!("second sample failed: {error}"));
    if let Ok(Some(status)) = guard.0.try_wait() {
        panic!("fake-tui exited with {status} before the measurement ended");
    }
    (earlier, later)
}

#[test]
fn sampler_reports_cpu_and_rss_of_a_child() {
    // A child that burns a core uses most of one over a second.
    {
        let mut child = spawn_fake_tui(&["--spin"]);
        let (earlier, later) = sample_twice(&mut child);
        assert!(later.at > earlier.at, "the samples are not in time order");
        assert!(
            later.cpu_seconds > earlier.cpu_seconds,
            "cpu time did not grow: {} then {}",
            earlier.cpu_seconds,
            later.cpu_seconds
        );
        let percent = cpu_percent(&earlier, &later);
        assert!(
            percent > 80.0,
            "a spinning child used {percent:.1}% of a core, not over 80%"
        );
    }

    // A child that holds 64 MiB of touched memory has at least 64 MB resident.
    {
        let mut child = spawn_fake_tui(&["--ballast-mib", "64"]);
        let (earlier, later) = sample_twice(&mut child);
        for (label, sample) in [("first", &earlier), ("second", &later)] {
            assert!(
                sample.rss_bytes > 64 * 1_000_000,
                "{label} sample: resident memory was {} bytes, not over 64 MB",
                sample.rss_bytes
            );
        }
    }
}
