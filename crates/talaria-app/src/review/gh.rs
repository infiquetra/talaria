//! The `gh` a review runs: closed standard input, a timeout, a kill on timeout, and the
//! fixed environment. Issue #245.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use talaria_core::review::{GhReady, PrStatus, parse_pr_reply, pr_chunks, pr_query};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::worktree::git::safe_text;
use crate::worktree::{GitError, GitOutput};

/// The program a review runs when `TALARIA_GH_BIN` is not set.
const DEFAULT_PROGRAM: &str = "gh";

/// How long an ordinary `gh` call may run.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// How long `gh pr create` may run.
const CREATE_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for output to drain after `gh` has exited.
const DRAIN_LIMIT: Duration = Duration::from_secs(5);

/// The `gh` program a review uses.
#[derive(Clone, Debug)]
pub struct GhRunner {
    program: PathBuf,
    /// One lock per folder: at most one `gh` process runs at a time in a repository.
    locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
}

impl GhRunner {
    /// Run `gh` from `PATH`. `TALARIA_GH_BIN` overrides it, which tests use.
    #[must_use]
    pub fn from_env() -> GhRunner {
        let program = std::env::var_os("TALARIA_GH_BIN")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_PROGRAM), PathBuf::from);
        Self::with_binary(program)
    }

    /// Run this program instead of `gh`. Tests pass a stub script.
    #[must_use]
    pub fn with_binary(path: PathBuf) -> GhRunner {
        Self {
            program: path,
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Run `gh` with standard input closed. The child and everything it started are killed
    /// when `timeout` passes.
    pub async fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        if !cwd.is_dir() {
            return Err(GitError::Io("the folder does not exist".to_owned()));
        }
        let lock = self.lock_for(cwd);
        let _turn = lock.lock().await;
        let base: BTreeMap<String, String> = std::env::vars_os()
            .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(cwd)
            .envs(gh_env(&base))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0);
        let mut child = spawn_retrying(&mut command).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitError::NotFound
            } else {
                GitError::Io(error.to_string())
            }
        })?;
        let pid = child.id();
        let stdout = tokio::spawn(read_all(child.stdout.take()));
        let stderr = tokio::spawn(read_all(child.stderr.take()));
        let waited = tokio::time::timeout(timeout, child.wait()).await;
        let status = match waited {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                stop(&mut child, pid, &stdout, &stderr).await;
                return Err(GitError::Io(error.to_string()));
            }
            Err(_) => {
                stop(&mut child, pid, &stdout, &stderr).await;
                return Err(GitError::TimedOut);
            }
        };
        let drained = tokio::time::timeout(DRAIN_LIMIT, async { tokio::join!(stdout, stderr) });
        let Ok((Ok(out), Ok(err))) = drained.await else {
            return Err(GitError::Io("gh output could not be read".to_owned()));
        };
        Ok(GitOutput {
            status: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Whether `gh` can open pull requests here.
    pub async fn readiness(&self, cwd: &Path) -> GhReady {
        let signed_in = match self.run(cwd, &["auth", "status"], CALL_TIMEOUT).await {
            Ok(output) => output,
            Err(GitError::NotFound) => return GhReady::NotFound,
            Err(error) => return GhReady::NotSignedIn(run_text(&error)),
        };
        if signed_in.status != 0 {
            return GhReady::NotSignedIn(last_line(&signed_in));
        }
        let args = [
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ];
        let viewed = match self.run(cwd, &args, CALL_TIMEOUT).await {
            Ok(output) => output,
            Err(GitError::NotFound) => return GhReady::NotFound,
            Err(error) => return GhReady::NotGitHub(run_text(&error)),
        };
        if viewed.status != 0 {
            return GhReady::NotGitHub(last_line(&viewed));
        }
        let repo = viewed.stdout.trim();
        if is_owner_name(repo) {
            GhReady::Ready {
                repo: repo.to_owned(),
            }
        } else {
            GhReady::NotGitHub(format!(
                "gh named the repository {}",
                safe_text(&format!("{repo:?}"))
            ))
        }
    }

    /// The pull-request statuses of `branches` in `repo`. Requests of 50 branches run one
    /// after another. A request that fails drops its branches; the call fails only when
    /// every request did.
    pub async fn pr_statuses(
        &self,
        cwd: &Path,
        repo: &str,
        branches: &[String],
    ) -> Result<Vec<PrStatus>, String> {
        let Some((owner, name)) = repo.split_once('/') else {
            return Err(format!("{} is not owner/name", safe_text(repo)));
        };
        let chunks = pr_chunks(branches);
        let mut statuses = Vec::new();
        let mut failure: Option<String> = None;
        let mut answered = chunks.is_empty();
        for chunk in &chunks {
            let query = format!("query={}", pr_query(owner, name, chunk));
            match self.statuses_of(cwd, &query, chunk).await {
                Ok(found) => {
                    answered = true;
                    statuses.extend(found);
                }
                Err(reason) => failure = Some(reason),
            }
        }
        match (answered, failure) {
            (false, Some(reason)) => Err(reason),
            _ => Ok(statuses),
        }
    }

    async fn statuses_of(
        &self,
        cwd: &Path,
        query: &str,
        chunk: &[String],
    ) -> Result<Vec<PrStatus>, String> {
        let output = self
            .run(cwd, &["api", "graphql", "-f", query], CALL_TIMEOUT)
            .await
            .map_err(|error| run_text(&error))?;
        if output.status != 0 {
            return Err(last_line(&output));
        }
        parse_pr_reply(&output.stdout, chunk).map_err(|reason| safe_text(&reason))
    }

    /// Open a draft pull request. The URL it printed.
    pub async fn create_draft(
        &self,
        cwd: &Path,
        base: &str,
        branch: &str,
    ) -> Result<String, String> {
        let args = [
            "pr", "create", "--draft", "--base", base, "--head", branch, "--fill",
        ];
        let output = self
            .run(cwd, &args, CREATE_TIMEOUT)
            .await
            .map_err(|error| format!("gh pr create failed: {}", run_text(&error)))?;
        if output.status != 0 {
            return Err(format!("gh pr create failed: {}", last_line(&output)));
        }
        output
            .stdout
            .lines()
            .map(str::trim)
            .rev()
            .find(|line| line.starts_with("https://"))
            .map(safe_text)
            .ok_or_else(|| "gh pr create printed no URL.".to_owned())
    }

    /// Open a pull request in the browser.
    pub async fn open_in_browser(&self, cwd: &Path, number: u64) -> Result<(), String> {
        let number = number.to_string();
        let output = self
            .run(cwd, &["pr", "view", &number, "--web"], CALL_TIMEOUT)
            .await
            .map_err(|error| format!("Could not open the browser: {}", run_text(&error)))?;
        if output.status == 0 {
            Ok(())
        } else {
            Err(format!(
                "Could not open the browser: {}",
                last_line(&output)
            ))
        }
    }

    fn lock_for(&self, cwd: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        locks.entry(cwd.to_path_buf()).or_default().clone()
    }
}

/// The environment of one `gh` call: the process environment plus exactly the
/// five fixed keys.
pub fn gh_env(base: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut env = base.clone();
    for (key, value) in [
        ("GH_PROMPT_DISABLED", "1"),
        ("GH_NO_UPDATE_NOTIFIER", "1"),
        ("NO_COLOR", "1"),
        ("CLICOLOR", "0"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        env.insert(key.to_owned(), value.to_owned());
    }
    env
}

/// The last non-empty line of standard error, made safe and cut to 200 characters. Falls
/// back to the exit status.
fn last_line(output: &GitOutput) -> String {
    output
        .stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or_else(
            || format!("gh exited with status {}", output.status),
            safe_text,
        )
}

/// The text for a `gh` call that produced no output.
fn run_text(error: &GitError) -> String {
    match error {
        GitError::NotFound => "gh was not found".to_owned(),
        GitError::TimedOut => "gh took too long to answer".to_owned(),
        GitError::Cancelled => "gh was stopped".to_owned(),
        GitError::Io(reason) => safe_text(reason),
    }
}

/// `owner/name`: two non-empty parts and nothing that is not part of a name.
fn is_owner_name(text: &str) -> bool {
    let Some((owner, name)) = text.split_once('/') else {
        return false;
    };
    let part = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    };
    part(owner) && part(name)
}

/// Linux refuses to run a file another thread still holds open for writing (`ETXTBSY`),
/// which happens when a parallel test writes its stub script while this one forks.
const TEXT_FILE_BUSY: i32 = 26;

/// Spawn, trying again briefly while the program file is busy.
async fn spawn_retrying(command: &mut Command) -> std::io::Result<tokio::process::Child> {
    let mut tries = 0;
    loop {
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(TEXT_FILE_BUSY) && tries < 20 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            spawned => return spawned,
        }
    }
}

async fn read_all<R: AsyncRead + Unpin>(reader: Option<R>) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(mut reader) = reader {
        let _ = reader.read_to_end(&mut bytes).await;
    }
    bytes
}

/// Kill the child and everything it started, then stop reading its output.
async fn stop(
    child: &mut tokio::process::Child,
    pid: Option<u32>,
    stdout: &tokio::task::JoinHandle<Vec<u8>>,
    stderr: &tokio::task::JoinHandle<Vec<u8>>,
) {
    if let Some(process) = pid
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(rustix::process::Pid::from_raw)
    {
        // ESRCH means the group is gone; EPERM means the id belongs to another user.
        let _ = rustix::process::kill_process_group(process, rustix::process::Signal::KILL);
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    stdout.abort();
    stderr.abort();
}
