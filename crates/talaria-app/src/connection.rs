//! Opening one gateway link for a launch, and the git workspace lookup.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hermes_client::discovery::{DiscoverOptions, Endpoint, EnsurePolicy, ProfileId};
use hermes_client::liveness::{Dial, DialFailure, Dialed, Link, LinkConfig, LinkEvent};
use hermes_client::record::Recorder;
use hermes_client::ticket::{Purpose, Scope};
use hermes_contract::schemas::RuntimeDescribeResult;
use tokio::sync::mpsc;

use crate::failure::LaunchFailure;
use crate::launch::Launch;

/// How long `git rev-parse` may take before it is killed and the folder is used instead.
pub const GIT_LIMIT: Duration = Duration::from_secs(5);

/// `git rev-parse --show-toplevel` in `folder`: an absolute path, or `None` when the folder is
/// not in a repository, git is missing, it answers something else, or it takes longer than
/// [`GIT_LIMIT`] (it is killed then).
#[must_use]
pub fn find_git_toplevel(folder: &Path) -> Option<PathBuf> {
    let mut child = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < GIT_LIMIT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            // Too slow, or the wait failed. A grandchild may still hold the pipe, so it is not
            // read.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut answer = String::new();
    child.stdout.take()?.read_to_string(&mut answer).ok()?;
    let answer = PathBuf::from(answer.trim());
    (answer.is_absolute()).then_some(answer)
}

/// How many times one launch may run `hermes gateway ensure`: once, or never with
/// `--no-ensure`. Discovery runs it only when no gateway answered; reconnects never run it.
#[must_use]
pub fn ensure_invocations(no_ensure: bool) -> u32 {
    u32::from(!no_ensure)
}

fn ensure_policy(no_ensure: bool) -> EnsurePolicy {
    if no_ensure {
        EnsurePolicy::Never
    } else {
        DiscoverOptions::default().ensure
    }
}

pub struct OpenConnection {
    pub link: Link,
    pub inbound: mpsc::Receiver<LinkEvent>,
    pub describe: RuntimeDescribeResult,
    pub profile_name: String,
}

/// The dial for one launch. The first connection reuses the endpoint the launch discovered
/// (so `ensure` ran at most once); later ones discover again without `ensure`.
struct LaunchDial {
    home: PathBuf,
    profile: String,
    first: Option<(Endpoint, ProfileId)>,
    recorder: Option<Recorder>,
    describe: Arc<Mutex<Option<RuntimeDescribeResult>>>,
}

impl Dial for LaunchDial {
    async fn dial(&mut self, generation: u64) -> Result<Dialed, DialFailure> {
        let (endpoint, profile) = match self.first.take() {
            Some(first) => first,
            None => {
                let options = DiscoverOptions {
                    ensure: EnsurePolicy::Never,
                    ..DiscoverOptions::default()
                };
                let endpoint = hermes_client::discover_with(&self.home, &options).await?;
                let profile = endpoint.resolve_profile(&self.profile)?;
                (endpoint, profile)
            }
        };
        let ticket = hermes_client::mint_ticket(
            &self.home,
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await?;
        let mut connection = hermes_client::connect(&endpoint, ticket).await?;
        if let Some(recorder) = &self.recorder {
            connection.set_recorder(recorder.clone(), generation);
        }
        let handshake = hermes_client::handshake(&mut connection).await?;
        *self.describe.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(handshake.describe.clone());
        Ok(Dialed {
            parts: connection.into_parts(),
            process_epoch: handshake.ready.replay_epoch,
            heartbeat: handshake.ready.heartbeat == Some(true),
            capabilities: handshake.describe.capabilities,
        })
    }
}

pub struct ConnectionService;

impl ConnectionService {
    /// Open a connection using a caller-supplied dial.
    pub async fn open_with_dial<D: hermes_client::liveness::Dial>(
        _launch: &Launch,
        _dial: D,
    ) -> Result<OpenConnection, LaunchFailure> {
        todo!("add remote gateway implementation")
    }

    /// Discover (running `ensure` at most once, never with `--no-ensure`), resolve the profile,
    /// and start the link on an interactive, profile-scoped ticket.
    pub async fn open(
        launch: &Launch,
        home: &Path,
        recorder: Option<Recorder>,
    ) -> Result<OpenConnection, LaunchFailure> {
        let options = DiscoverOptions {
            ensure: ensure_policy(launch.no_ensure),
            ..DiscoverOptions::default()
        };
        let endpoint = hermes_client::discover_with(home, &options).await?;
        let profile = endpoint.resolve_profile(&launch.profile)?;
        let describe = Arc::new(Mutex::new(None));
        let dial = LaunchDial {
            home: home.to_path_buf(),
            profile: launch.profile.clone(),
            first: Some((endpoint, profile)),
            recorder,
            describe: Arc::clone(&describe),
        };
        let (link, inbound) = Link::start(dial, LinkConfig::default()).await?;
        let describe = describe
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| {
                LaunchFailure::from(hermes_client::connect::HandshakeError::BadDescribe)
            })?;
        Ok(OpenConnection {
            link,
            inbound,
            describe,
            profile_name: launch.profile.trim().to_lowercase(),
        })
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
            .arg("connection::tests::find_git_toplevel_limits")
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
