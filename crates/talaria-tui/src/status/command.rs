//! Running the external command and deciding when to run it.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::Tone;
use super::document::StatusDocument;
use super::env::build_child_env;
use super::output::{ExternalRow, Failure, ParsedOutput, parse_output};
use super::segments::ExternalSegment;

/// The shortest gap between two runs when the document changed.
pub const MIN_RERUN_GAP: Duration = Duration::from_secs(1);
/// The longest and shortest interval the configuration may ask for.
const MIN_INTERVAL_SECONDS: u64 = 1;
const MAX_INTERVAL_SECONDS: u64 = 3600;
/// How long the pipes are given to reach their end after the group was killed.
const DRAIN_GRACE: Duration = Duration::from_millis(250);

/// Split with POSIX quoting, never through a shell.
///
/// The reason never repeats the command, so a configured command holding a secret cannot reach
/// a log through the notice.
pub fn parse_command(command: &str) -> Result<Vec<String>, String> {
    if command.trim().is_empty() {
        return Err("the status command must not be empty; it is disabled".to_string());
    }
    match shlex::split(command) {
        Some(parts) if !parts.is_empty() => Ok(parts),
        _ => Err("the status command has invalid quoting; it is disabled".to_string()),
    }
}

/// Clamp to 1 through 3600 seconds, with no notice.
pub fn interval_from_config(seconds: u64) -> Duration {
    Duration::from_secs(seconds.clamp(MIN_INTERVAL_SECONDS, MAX_INTERVAL_SECONDS))
}

/// The limits of one run.
#[derive(Clone, Debug)]
pub struct CommandLimits {
    /// How long the command may run.
    pub timeout: Duration,
    /// How many bytes of standard output are read.
    pub stdout_bytes: usize,
    /// How many bytes of standard error are read.
    pub stderr_bytes: usize,
    /// How many rows are kept.
    pub rows: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            stdout_bytes: 16_384,
            stderr_bytes: 4_096,
            rows: 8,
        }
    }
}

/// What to run, and where.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    /// The program and its arguments, split without a shell.
    pub argv: Vec<String>,
    /// The directory the command starts in.
    pub launch_dir: PathBuf,
    /// The names the operator allows beside the contract's own.
    pub env_allowlist: Vec<String>,
}

/// What one run produced.
#[derive(Clone, Debug)]
pub struct CommandResult {
    /// The rows, or the failure.
    pub parsed: ParsedOutput,
}

/// The segment a result shows, without any history: a failure gives its marker alone.
pub fn external_segment(result: &CommandResult) -> Option<ExternalSegment> {
    match &result.parsed {
        ParsedOutput::Rows { rows, .. } => first_row(rows).map(|row| ExternalSegment {
            text: row.text.trim().to_string(),
            tone: row.tone,
        }),
        ParsedOutput::Failure(failure) => Some(ExternalSegment {
            text: failure.marker(),
            tone: Tone::Warning,
        }),
    }
}

/// The first row that holds something.
fn first_row(rows: &[ExternalRow]) -> Option<&ExternalRow> {
    rows.iter().find(|row| !row.text.trim().is_empty())
}

/// Keeps the last good text beside a failure marker, so output problems never blank the line.
#[derive(Default)]
pub struct ExternalDisplay {
    last_good: Option<ExternalSegment>,
    shown: Option<ExternalSegment>,
}

impl ExternalDisplay {
    /// Take the result of a run into the display.
    pub fn apply(&mut self, result: &CommandResult) {
        match &result.parsed {
            ParsedOutput::Rows { rows, .. } => {
                let segment = first_row(rows).map(|row| ExternalSegment {
                    text: row.text.trim().to_string(),
                    tone: row.tone,
                });
                self.last_good = segment.clone();
                self.shown = segment;
            }
            ParsedOutput::Failure(failure) => {
                let marker = failure.marker();
                let text = match &self.last_good {
                    Some(good) => format!("{} · {marker}", good.text),
                    None => marker,
                };
                self.shown = Some(ExternalSegment {
                    text,
                    tone: Tone::Warning,
                });
            }
        }
    }

    /// The segment to draw, when there is one.
    pub fn segment(&self) -> Option<&ExternalSegment> {
        self.shown.as_ref()
    }
}

/// Run one command under the process contract and parse its output.
#[cfg(unix)]
pub async fn run_command(
    spec: &CommandSpec,
    env: Vec<(String, String)>,
    stdin: Vec<u8>,
    limits: &CommandLimits,
) -> CommandResult {
    let shared = Arc::new(Shared::default());
    run_command_shared(spec, env, stdin, limits, &shared).await
}

/// The process contract is Unix-only: elsewhere no command can be started.
#[cfg(not(unix))]
pub async fn run_command(
    _spec: &CommandSpec,
    _env: Vec<(String, String)>,
    _stdin: Vec<u8>,
    _limits: &CommandLimits,
) -> CommandResult {
    result_of(Failure::SpawnFailed(
        "the status command runner is Unix-only".to_string(),
    ))
}

/// The state the service and the running command share.
#[derive(Default)]
struct Shared {
    stopping: AtomicBool,
    pid: AtomicU32,
}

impl Shared {
    fn record(&self, pid: u32) {
        self.pid.store(pid, Ordering::SeqCst);
    }

    fn clear(&self) {
        self.pid.store(0, Ordering::SeqCst);
    }

    fn take(&self) -> u32 {
        self.pid.swap(0, Ordering::SeqCst)
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }
}

/// Run one command, recording its process group in `shared` so a shutdown can find it.
#[cfg(unix)]
async fn run_command_shared(
    spec: &CommandSpec,
    env: Vec<(String, String)>,
    stdin: Vec<u8>,
    limits: &CommandLimits,
    shared: &Arc<Shared>,
) -> CommandResult {
    let Some((program, arguments)) = spec.argv.split_first() else {
        return result_of(Failure::SpawnFailed(
            "the command holds no program".to_string(),
        ));
    };
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(&spec.launch_dir)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .process_group(0);

    let mut attempts = 0u8;
    let mut child = loop {
        match command.spawn() {
            Ok(child) => break child,
            Err(error) if error.raw_os_error() == Some(26) && attempts < 10 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return result_of(Failure::NotFound);
            }
            Err(error) => {
                return result_of(Failure::SpawnFailed(error.to_string()));
            }
        }
    };

    // The process id is recorded before the first await, so a shutdown always finds the child.
    let pid = child.id().unwrap_or(0);
    shared.record(pid);

    let stdin_pipe = child.stdin.take();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let writer = tokio::spawn(write_stdin(stdin_pipe, stdin));
    let stdout_reader = tokio::spawn(read_capped(stdout_pipe, limits.stdout_bytes, pid));
    let stderr_reader = tokio::spawn(read_capped(stderr_pipe, limits.stderr_bytes, pid));

    // A shutdown that arrived while the child was starting kills the group now.
    if shared.stopping() {
        kill_group(pid);
    }

    // One deadline for the whole run.
    let deadline = tokio::time::Instant::now() + limits.timeout;
    let waited = tokio::time::timeout_at(deadline, child.wait()).await;
    // The leader is gone: the group dies with it, so a helper holding the pipes cannot keep
    // the run waiting for its end.
    kill_group(pid);
    let _ = child.start_kill();
    shared.clear();

    let status = match waited {
        Ok(Ok(status)) => Some(status),
        Ok(Err(error)) => {
            // The wait itself failed: the child is gone and its status is unknown.
            return result_of(Failure::SpawnFailed(error.to_string()));
        }
        Err(_) => None,
    };
    // The pipes reach their end now that the group is gone.
    let (stdout, stdout_overflow) = drain(stdout_reader).await;
    let (stderr, stderr_overflow) = drain(stderr_reader).await;
    let _ = drain(writer).await;

    let Some(status) = status else {
        return result_of(Failure::TimedOut);
    };
    if !stdout_overflow
        && !stderr_overflow
        && let Err(code) = exit_outcome(&status)
    {
        return result_of(Failure::NonzeroExit {
            code,
            stderr_line: first_stderr_line(&stderr),
        });
    }
    CommandResult {
        parsed: parse_output(&stdout, limits.stdout_bytes, limits.rows),
    }
}

/// The process contract is Unix-only: elsewhere no command can be started.
#[cfg(not(unix))]
async fn run_command_shared(
    _spec: &CommandSpec,
    _env: Vec<(String, String)>,
    _stdin: Vec<u8>,
    _limits: &CommandLimits,
    _shared: &Arc<Shared>,
) -> CommandResult {
    result_of(Failure::SpawnFailed(
        "the status command runner is Unix-only".to_string(),
    ))
}

/// Write the document to the command's standard input and close it.
///
/// A command that never reads its input is fine: the broken pipe is ignored.
#[cfg(unix)]
async fn write_stdin(pipe: Option<tokio::process::ChildStdin>, stdin: Vec<u8>) {
    let Some(mut pipe) = pipe else {
        return;
    };
    let _ = pipe.write_all(&stdin).await;
    let _ = pipe.shutdown().await;
}

/// Read a pipe up to `cap` bytes, and say whether the cap was passed.
#[cfg(unix)]
async fn read_capped<R>(pipe: Option<R>, cap: usize, pid: u32) -> (Vec<u8>, bool)
where
    R: AsyncRead + Unpin,
{
    let Some(mut pipe) = pipe else {
        return (Vec::new(), false);
    };
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let wanted = (cap + 1).saturating_sub(buffer.len());
        if wanted == 0 {
            break;
        }
        let limit = wanted.min(chunk.len());
        let read = match pipe.read(&mut chunk[..limit]).await {
            Ok(read) => read,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    if buffer.len() > cap {
        // The cap is passed: the group dies now, so the other pipe reaches its end.
        kill_group(pid);
        return (buffer, true);
    }
    (buffer, false)
}

/// Wait for a task to end, and give up on it after a short grace.
#[cfg(unix)]
async fn drain<T>(handle: tokio::task::JoinHandle<T>) -> T
where
    T: Default,
{
    match tokio::time::timeout(DRAIN_GRACE, handle).await {
        Ok(Ok(value)) => value,
        _ => T::default(),
    }
}

/// The exit status: `Ok` for a clean exit, `Err` with the code or `None` for a signal.
#[cfg(unix)]
fn exit_outcome(status: &std::process::ExitStatus) -> Result<(), Option<i32>> {
    if status.success() {
        return Ok(());
    }
    Err(status.code())
}

/// The first non-blank line of the standard error.
#[cfg(unix)]
fn first_stderr_line(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Kill a whole process group, ignoring a group that is already gone.
#[cfg(unix)]
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

/// Kill a whole process group: nothing to kill away from Unix.
#[cfg(not(unix))]
fn kill_group(_pid: u32) {}

/// The result of a run that failed.
fn result_of(failure: Failure) -> CommandResult {
    CommandResult {
        parsed: ParsedOutput::Failure(failure),
    }
}

/// Why a tick did not start a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// A command is running.
    Running,
    /// The document did not change and the interval has not passed.
    Unchanged,
    /// The document changed too soon after the last run.
    TooSoon,
    /// The service was shut down.
    Stopped,
}

/// What a tick decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Start the command.
    Run,
    /// Do not start it.
    Skip(SkipReason),
}

/// When the command runs again: a pure function of time and the document.
pub struct RefreshPolicy {
    interval: Duration,
    last_start: Option<Instant>,
    last_document: Option<Vec<u8>>,
    change_pending: bool,
}

impl RefreshPolicy {
    /// A policy that runs at most once per interval.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_start: None,
            last_document: None,
            change_pending: false,
        }
    }

    /// Decide whether this tick starts the command.
    pub fn decide(&mut self, now: Instant, document: &[u8], running: bool) -> Decision {
        // The change is recomputed at every call, never latched: a change that is undone
        // leaves nothing pending.
        let changed = self.last_document.as_deref() != Some(document);
        self.change_pending = changed;
        if running {
            return Decision::Skip(SkipReason::Running);
        }
        let Some(start) = self.last_start else {
            self.start(now, document);
            return Decision::Run;
        };
        let elapsed = now.saturating_duration_since(start);
        if elapsed >= self.interval {
            self.start(now, document);
            Decision::Run
        } else if changed {
            if elapsed >= MIN_RERUN_GAP {
                self.start(now, document);
                Decision::Run
            } else {
                Decision::Skip(SkipReason::TooSoon)
            }
        } else {
            Decision::Skip(SkipReason::Unchanged)
        }
    }

    /// The earliest instant a tick could start the command.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        match self.last_start {
            None => Some(now),
            Some(start) => {
                if self.change_pending {
                    Some(start + MIN_RERUN_GAP.min(self.interval))
                } else {
                    Some(start + self.interval)
                }
            }
        }
    }

    fn start(&mut self, now: Instant, document: &[u8]) {
        self.last_start = Some(now);
        self.last_document = Some(document.to_vec());
        self.change_pending = false;
    }
}

/// Clears the running flag when the task ends, however it ends.
struct ClearOnDrop(Arc<AtomicBool>);

impl Drop for ClearOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Owns the policy, the running command and its process group.
pub struct StatusCommandService {
    policy: RefreshPolicy,
    spec: CommandSpec,
    limits: CommandLimits,
    parent_env: Vec<(String, String)>,
    is_credential_name: fn(&str) -> bool,
    shared: Arc<Shared>,
    running: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
    stopped: bool,
}

impl StatusCommandService {
    /// A service for `spec`, running at most once per `interval`.
    pub fn new(
        spec: CommandSpec,
        interval: Duration,
        parent_env: Vec<(String, String)>,
        is_credential_name: fn(&str) -> bool,
        limits: CommandLimits,
    ) -> Self {
        Self {
            policy: RefreshPolicy::new(interval),
            spec,
            limits,
            parent_env,
            is_credential_name,
            shared: Arc::new(Shared::default()),
            running: Arc::new(AtomicBool::new(false)),
            task: None,
            stopped: false,
        }
    }

    /// Ask the service to start the command. Must be called inside a Tokio runtime.
    pub fn on_tick(
        &mut self,
        now: Instant,
        document: &StatusDocument,
        done: impl FnOnce(CommandResult) + Send + 'static,
    ) -> Decision {
        if self.stopped {
            return Decision::Skip(SkipReason::Stopped);
        }
        let encoded = document.encode();
        let running = self.running.load(Ordering::SeqCst);
        if let Decision::Skip(reason) = self.policy.decide(now, &encoded, running) {
            return Decision::Skip(reason);
        }

        let spec = self.spec.clone();
        let limits = self.limits.clone();
        let env = build_child_env(
            &self.parent_env,
            &spec.env_allowlist,
            &self.is_credential_name,
        );
        let shared = self.shared.clone();
        let running = self.running.clone();
        self.running.store(true, Ordering::SeqCst);
        self.task = Some(tokio::spawn(async move {
            let _clear = ClearOnDrop(running.clone());
            let result = run_command_shared(&spec, env, encoded, &limits, &shared).await;
            running.store(false, Ordering::SeqCst);
            done(result);
        }));
        Decision::Run
    }

    /// The instant the next tick should come, or `None` while a command runs.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        if self.running.load(Ordering::SeqCst) {
            return None;
        }
        self.policy.next_wake(now)
    }

    /// Kill a command in flight and return when it is dead.
    pub async fn shutdown(&mut self) {
        self.stopped = true;
        self.shared.stopping.store(true, Ordering::SeqCst);
        let pid = self.shared.take();
        if pid != 0 {
            kill_group(pid);
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for StatusCommandService {
    fn drop(&mut self) {
        let pid = self.shared.take();
        if pid != 0 {
            kill_group(pid);
        }
    }
}
