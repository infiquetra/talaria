//! Helpers shared by the status line test files: temporary directories, executable
//! scripts and process lookups. No absolute paths are written in test source.

#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// A fresh temporary directory; removed when dropped. Its file name is unique, so it
/// doubles as the token a test uses to find its own processes.
pub fn temp_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("statustest")
        .tempdir()
        .expect("temporary directory")
}

/// The unique token of a temporary directory: its file name.
pub fn token(dir: &Path) -> String {
    dir.file_name()
        .expect("directory name")
        .to_string_lossy()
        .into_owned()
}

/// Write an executable `#!/bin/sh` script into `dir` and return its path.
pub fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).expect("create script");
    writeln!(file, "#!/bin/sh\n{body}").expect("write script");
    drop(file);
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod script");
    path
}

/// How many processes have `needle` in their command line.
pub fn pgrep_count(needle: &str) -> usize {
    let out = Command::new("pgrep")
        .arg("-f")
        .arg(needle)
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Poll up to `limit` for the condition; true when it became true.
pub fn wait_until(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if condition() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// True when no process with the token remains, polling up to one second.
pub fn no_process_with(token: &str) -> bool {
    wait_until(Duration::from_secs(1), || pgrep_count(token) == 0)
}

/// Like `wait_until`, but yields to the Tokio runtime between checks so a command the
/// test started on the same thread keeps running while the test waits.
pub async fn wait_until_async(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if condition() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Kills every process whose command line carries the token when dropped, so a test that
/// fails midway never leaves a helper behind.
pub struct KillByToken(pub String);

impl Drop for KillByToken {
    fn drop(&mut self) {
        let _ = Command::new("pkill").arg("-f").arg(&self.0).status();
    }
}

/// The parent environment a child needs to find ordinary tools: only `PATH`.
pub fn path_env() -> Vec<(String, String)> {
    std::env::var("PATH")
        .map(|path| vec![("PATH".to_string(), path)])
        .unwrap_or_default()
}

/// The process id written into a file by a script, once it exists.
pub fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

/// Wait up to `limit` for a script to write its process id into `path`.
pub async fn wait_for_pid(path: &Path, limit: Duration) -> Option<u32> {
    let mut found = None;
    wait_until_async(limit, || {
        found = read_pid(path);
        found.is_some()
    })
    .await;
    found
}

/// True while a process with this id exists (`kill -0`).
pub fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// True when the process is gone within `limit`.
pub async fn pid_gone_within(pid: u32, limit: Duration) -> bool {
    wait_until_async(limit, || !pid_alive(pid)).await
}

/// Like `no_process_with`, but yields to the runtime between checks.
pub async fn no_process_with_async(token: &str, limit: Duration) -> bool {
    wait_until_async(limit, || pgrep_count(token) == 0).await
}
