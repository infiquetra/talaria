//! How a launch runs git: no prompts, closed standard input, a timeout, and a kill on cancel.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::bg::outcome::LaunchError;
use crate::bg::service::CancelToken;

/// How often a running git call looks at the cancellation token.
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// How long to wait for output to drain after git has exited.
const DRAIN_LIMIT: Duration = Duration::from_secs(5);

/// Longest git error text shown to the operator, in characters.
const ERROR_LIMIT: usize = 200;

/// Captured output of one git invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitOutput {
    /// The process status, or `-1` when git did not exit.
    pub status: i32,
    /// Standard output, lossy-decoded.
    pub stdout: String,
    /// Standard error, lossy-decoded.
    pub stderr: String,
}

/// Why a git invocation produced no output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitError {
    /// The program could not be started.
    NotFound,
    /// The timeout elapsed and the child was killed.
    TimedOut,
    /// The launch was cancelled and the child was killed.
    Cancelled,
    /// The child could not be started, or its output could not be read.
    Io(String),
}

/// The git program a launch uses, plus the environment added to every call.
#[derive(Clone, Debug)]
pub struct GitRunner {
    program: PathBuf,
    /// Whether ssh is configured, remembered per main worktree.
    ssh_configured: Arc<Mutex<BTreeMap<PathBuf, bool>>>,
}

impl GitRunner {
    /// Run `git` from `PATH`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::with_program(PathBuf::from("git"))
    }

    /// Run this program instead of `git`. Tests pass a wrapper script.
    #[must_use]
    pub fn with_program(program: PathBuf) -> Self {
        Self {
            program,
            ssh_configured: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// A call that may write (fetch, worktree add, config, branch).
    pub async fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        self.execute(cwd, args, timeout, false, None).await
    }

    /// A read-only call: it adds `GIT_OPTIONAL_LOCKS=0`.
    pub async fn run_read_only(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        self.execute(cwd, args, timeout, true, None).await
    }

    /// [`Self::run`], killed early with [`GitError::Cancelled`] when `cancel` is set.
    pub async fn run_until(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
        cancel: &CancelToken,
    ) -> Result<GitOutput, GitError> {
        self.execute(cwd, args, timeout, false, Some(cancel)).await
    }

    /// [`Self::run_read_only`], killed early with [`GitError::Cancelled`] when `cancel` is set.
    pub async fn run_read_only_until(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
        cancel: &CancelToken,
    ) -> Result<GitOutput, GitError> {
        self.execute(cwd, args, timeout, true, Some(cancel)).await
    }

    async fn execute(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
        read_only: bool,
        cancel: Option<&CancelToken>,
    ) -> Result<GitOutput, GitError> {
        let base = process_env();
        let prefix = if reaches_network(args) {
            git_prefix_args(self.ssh_is_configured(cwd, &base).await)
        } else {
            Vec::new()
        };
        self.spawn(cwd, &prefix, args, timeout, read_only, &base, cancel)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn(
        &self,
        cwd: &Path,
        prefix: &[String],
        args: &[&str],
        timeout: Duration,
        read_only: bool,
        base: &BTreeMap<String, String>,
        cancel: Option<&CancelToken>,
    ) -> Result<GitOutput, GitError> {
        if !cwd.is_dir() {
            return Err(GitError::Io("the folder does not exist".to_owned()));
        }
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Err(GitError::Cancelled);
        }
        let mut command = Command::new(&self.program);
        command
            .args(prefix)
            .args(args)
            .current_dir(cwd)
            .envs(git_env(base, read_only))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .process_group(0);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                GitError::NotFound
            } else {
                GitError::Io(error.to_string())
            }
        })?;
        let pid = child.id();
        let stdout = tokio::spawn(read_all(child.stdout.take()));
        let stderr = tokio::spawn(read_all(child.stderr.take()));
        let deadline = tokio::time::sleep(timeout);
        let cancelled = async {
            match cancel {
                Some(token) => {
                    while !token.is_cancelled() {
                        tokio::time::sleep(CANCEL_POLL).await;
                    }
                }
                None => std::future::pending::<()>().await,
            }
        };
        tokio::pin!(deadline, cancelled);
        let stopped = tokio::select! {
            status = child.wait() => Ok(status),
            () = &mut deadline => Err(GitError::TimedOut),
            () = &mut cancelled => Err(GitError::Cancelled),
        };
        let status = match stopped {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                stop(&mut child, pid, &stdout, &stderr).await;
                return Err(GitError::Io(error.to_string()));
            }
            Err(error) => {
                stop(&mut child, pid, &stdout, &stderr).await;
                return Err(error);
            }
        };
        let drained = tokio::time::timeout(DRAIN_LIMIT, async { tokio::join!(stdout, stderr) });
        let Ok((out, err)) = drained.await else {
            return Err(GitError::Io("git left its output open".to_owned()));
        };
        let (Ok(out), Ok(err)) = (out, err) else {
            return Err(GitError::Io("git output could not be read".to_owned()));
        };
        Ok(GitOutput {
            status: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }

    /// Whether a ssh command is already configured, so BatchMode is not forced over it.
    async fn ssh_is_configured(&self, cwd: &Path, base: &BTreeMap<String, String>) -> bool {
        let set = |key: &str| base.get(key).is_some_and(|value| !value.is_empty());
        if set("GIT_SSH_COMMAND") || set("GIT_SSH") {
            return true;
        }
        let cached = self
            .ssh_configured
            .lock()
            .ok()
            .and_then(|known| known.get(cwd).copied());
        if let Some(configured) = cached {
            return configured;
        }
        let found = self
            .spawn(
                cwd,
                &[],
                &["config", "--get", "core.sshCommand"],
                Duration::from_secs(10),
                true,
                base,
                None,
            )
            .await;
        let configured = matches!(
            found,
            Ok(ref output) if output.status == 0 && !output.stdout.trim().is_empty()
        );
        if let Ok(mut known) = self.ssh_configured.lock() {
            known.insert(cwd.to_path_buf(), configured);
        }
        configured
    }
}

/// The environment of one git call. `read_only` adds `GIT_OPTIONAL_LOCKS=0`.
#[must_use]
pub fn git_env(base: &BTreeMap<String, String>, read_only: bool) -> BTreeMap<String, String> {
    let mut env = base.clone();
    env.insert("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned());
    env.insert("GCM_INTERACTIVE".to_owned(), "Never".to_owned());
    if read_only {
        env.insert("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned());
    }
    env
}

/// The `-c core.sshCommand=...` prefix, or nothing when ssh is already configured.
#[must_use]
pub fn git_prefix_args(user_ssh_command_configured: bool) -> Vec<String> {
    if user_ssh_command_configured {
        Vec::new()
    } else {
        vec![
            "-c".to_owned(),
            "core.sshCommand=ssh -o BatchMode=yes".to_owned(),
        ]
    }
}

/// The text shown for a git failure: the first non-empty line of standard error, made safe
/// for the terminal and cut to 200 characters. Falls back to the exit status.
#[must_use]
pub fn error_line(output: &GitOutput) -> String {
    let line = output
        .stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty());
    match line {
        Some(line) => cut(&sanitize(line)),
        None => format!("git exited with status {}", output.status),
    }
}

/// Make arbitrary text safe to show, and cut it to 200 characters.
#[must_use]
pub fn safe_text(text: &str) -> String {
    cut(&sanitize(text))
}

/// The refusal for a machine without git.
pub(crate) fn missing_error() -> LaunchError {
    LaunchError {
        what: "git was not found on this machine.".to_owned(),
        next_step: "Install git, or run without --worktree.".to_owned(),
    }
}

/// The result of a cancelled preparation.
pub(crate) fn interrupted_error() -> LaunchError {
    LaunchError {
        what: "stopped before the prompt was sent; nothing was started.".to_owned(),
        next_step: "Run the command again when ready.".to_owned(),
    }
}

/// A refusal for a git call that produced no output. `doing` completes "git ... to <doing>".
pub(crate) fn run_failed(error: GitError, doing: &str) -> LaunchError {
    match error {
        GitError::NotFound => missing_error(),
        GitError::Cancelled => interrupted_error(),
        GitError::TimedOut => LaunchError {
            what: format!("git took too long to {doing}."),
            next_step: "Try again; nothing was started.".to_owned(),
        },
        GitError::Io(reason) => LaunchError {
            what: format!("git could not run to {doing}: {}.", safe_text(&reason)),
            next_step: "Fix what the system reports, then try again; nothing was started."
                .to_owned(),
        },
    }
}

/// `fetch` and `ls-remote` are the only calls that reach the network.
fn reaches_network(args: &[&str]) -> bool {
    matches!(args.first(), Some(&"fetch" | &"ls-remote"))
}

/// The process environment as text. A variable that is not valid UTF-8 is left to the child's
/// inherited copy.
fn process_env() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
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
    if let Some(pid) = pid {
        kill_group(pid);
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
    stdout.abort();
    stderr.abort();
}

/// Kill a whole process group, ignoring a group that is already gone.
fn kill_group(pid: u32) {
    let Ok(raw) = i32::try_from(pid) else {
        return;
    };
    let Some(process) = rustix::process::Pid::from_raw(raw) else {
        return;
    };
    // ESRCH means the group is gone; EPERM means the id belongs to another user.
    let _ = rustix::process::kill_process_group(process, rustix::process::Signal::KILL);
}

fn cut(text: &str) -> String {
    text.chars().take(ERROR_LIMIT).collect()
}

// Keep terminal text safe without crossing the application's presentation dependency boundary.
// The visible replacements match the terminal history sanitizer.
fn sanitize(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => sanitized.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    sanitized.push(picture);
                }
            }
            0x7f => sanitized.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => sanitized.push_str(&format!("‹U+{code:04X}›")),
            _ => sanitized.push(ch),
        }
    }
    sanitized
}
