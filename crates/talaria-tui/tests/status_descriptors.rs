//! Repeated command timeouts must not leak file descriptors. This file holds a single test
//! so no other test running in parallel changes the process's descriptor count.

#![cfg(unix)]

mod status_support;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use talaria_tui::status::output::{Failure, ParsedOutput};
use talaria_tui::status::{CommandLimits, CommandSpec, run_command};

/// Entries of the process's descriptor directory right now.
fn open_descriptors() -> usize {
    let dir: PathBuf = ["/", "dev", "fd"].iter().collect();
    fs::read_dir(dir).expect("descriptor directory").count()
}

#[tokio::test]
async fn repeated_timeouts_do_not_leak_descriptors() {
    let dir = status_support::temp_dir();
    let script = status_support::write_script(dir.path(), "sleeper.sh", "sleep 30");
    let spec = CommandSpec {
        argv: vec![script.to_string_lossy().into_owned()],
        launch_dir: dir.path().to_path_buf(),
        env_allowlist: Vec::new(),
    };
    let env = vec![(
        "PATH".to_string(),
        std::env::var("PATH").unwrap_or_default(),
    )];
    let limits = CommandLimits {
        timeout: Duration::from_millis(300),
        stdout_bytes: 16384,
        stderr_bytes: 4096,
        rows: 8,
    };

    let first = run_command(&spec, env.clone(), Vec::new(), &limits).await;
    assert_eq!(first.parsed, ParsedOutput::Failure(Failure::TimedOut));
    let before = open_descriptors();

    for round in 0..20 {
        let result = run_command(&spec, env.clone(), Vec::new(), &limits).await;
        assert_eq!(
            result.parsed,
            ParsedOutput::Failure(Failure::TimedOut),
            "run {round} should time out"
        );
    }
    let after = open_descriptors();
    assert!(
        after <= before + 3,
        "descriptors rose from {before} to {after} over twenty timeouts"
    );
}
