//! Opening one gateway link for a launch, and the git workspace lookup.

use std::path::{Path, PathBuf};

use hermes_client::liveness::{Link, LinkEvent};
use hermes_client::record::Recorder;
use hermes_contract::schemas::RuntimeDescribeResult;
use tokio::sync::mpsc;

use crate::failure::LaunchFailure;
use crate::launch::Launch;

#[must_use]
pub fn find_git_toplevel(folder: &Path) -> Option<PathBuf> {
    let _ = folder;
    todo!("binary")
}

/// How many times this launch runs gateway ensure. Zero when ensure is turned off.
#[must_use]
pub fn ensure_invocations(no_ensure: bool) -> u32 {
    let _ = no_ensure;
    todo!("binary")
}

pub struct OpenConnection {
    pub link: Link,
    pub inbound: mpsc::Receiver<LinkEvent>,
    pub describe: RuntimeDescribeResult,
    pub profile_name: String,
}

pub struct ConnectionService;

impl ConnectionService {
    pub async fn open(
        launch: &Launch,
        home: &Path,
        recorder: Option<Recorder>,
    ) -> Result<OpenConnection, LaunchFailure> {
        let _ = (launch, home, recorder);
        todo!("binary")
    }
}

#[cfg(test)]
mod tests {
    use super::find_git_toplevel;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    fn install(path: &Path, body: &str) {
        fs::write(path, body).expect("script");
        let mut permissions = fs::metadata(path).expect("meta").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("mode");
    }

    fn run_case(case: &str, bin: &Path, folder: &Path) {
        let path = std::env::var("PATH").unwrap_or_default();
        let joined = format!("{}:{}", bin.display(), path);
        let output = Command::new(std::env::current_exe().expect("test exe"))
            .arg("find_git_toplevel_limits")
            .arg("--exact")
            .env("TALARIA_FIND_GIT_CASE", case)
            .env("TALARIA_FIND_GIT_FOLDER", folder)
            .env("PATH", joined)
            .output()
            .expect("re-exec");
        assert!(
            output.status.success(),
            "{case} child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn find_git_toplevel_limits() {
        if let Ok(case) = std::env::var("TALARIA_FIND_GIT_CASE") {
            let folder = PathBuf::from(std::env::var("TALARIA_FIND_GIT_FOLDER").expect("folder"));
            let started = Instant::now();
            let answer = find_git_toplevel(&folder);
            let elapsed = started.elapsed();
            assert!(answer.is_none(), "{case}");
            if case == "sleep" {
                assert!(
                    elapsed >= Duration::from_secs(5),
                    "git was killed before five seconds ({elapsed:?})"
                );
                assert!(
                    elapsed < Duration::from_secs(8),
                    "git was not killed within five seconds, waited {elapsed:?}"
                );
            }
            return;
        }

        let root = tempfile::tempdir().expect("temp dir");
        let outside = root.path().join("plain");
        fs::create_dir(&outside).expect("dir");
        // A directory that is not a repository. The sleeping and relative stand-ins run in a
        // child whose search path lists them first. The sleeping stand-in is the slow case.
        assert!(find_git_toplevel(&outside).is_none());

        let bin = root.path().join("bin");
        fs::create_dir(&bin).expect("bin");
        let git = bin.join("git");
        install(&git, "#!/bin/sh\nsleep 30\n");
        run_case("sleep", &bin, &outside);
        install(&git, "#!/bin/sh\nprintf 'proj\\n'\n");
        run_case("relative", &bin, &outside);
        install(&git, "#!/bin/sh\nprintf '\\n'\n");
        run_case("empty", &bin, &outside);
    }
}
