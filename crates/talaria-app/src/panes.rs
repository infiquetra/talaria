//! Plan and open a session in a new multiplexer pane.

use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use talaria_core::directory::rows::RowKey;
use tokio::io::{AsyncRead, AsyncReadExt};

/// The process environment a multiplexer detection reads: five variables, nothing else.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneEnv {
    pub herdr_env: Option<String>,
    pub herdr_pane_id: Option<String>,
    pub herdr_bin_path: Option<String>,
    pub tmux: Option<String>,
    pub tmux_pane: Option<String>,
}

impl PaneEnv {
    /// Read the five variables from the process environment.
    #[must_use]
    pub fn from_process() -> PaneEnv {
        PaneEnv {
            herdr_env: std::env::var("HERDR_ENV").ok(),
            herdr_pane_id: std::env::var("HERDR_PANE_ID").ok(),
            herdr_bin_path: std::env::var("HERDR_BIN_PATH").ok(),
            tmux: std::env::var("TMUX").ok(),
            tmux_pane: std::env::var("TMUX_PANE").ok(),
        }
    }
}

/// Which multiplexer the operator configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiplexerSetting {
    Auto,
    Herdr,
    Tmux,
    Off,
}

/// Which way a new pane splits off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionSetting {
    Auto,
    Right,
    Down,
}

/// The `[panes]` table: multiplexer, direction, focus. Defaults are Auto, Auto, false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneSettings {
    pub multiplexer: MultiplexerSetting,
    pub direction: DirectionSetting,
    pub focus: bool,
}

impl Default for PaneSettings {
    fn default() -> Self {
        Self {
            multiplexer: MultiplexerSetting::Auto,
            direction: DirectionSetting::Auto,
            focus: false,
        }
    }
}

/// The detected multiplexer: its binary and the calling pane.
#[derive(Clone, Debug, PartialEq)]
pub enum Multiplexer {
    Herdr { bin: PathBuf, pane: String },
    Tmux { bin: PathBuf, pane: String },
}

impl Multiplexer {
    /// `Herdr` or `tmux`, exactly as the outcome texts spell it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Multiplexer::Herdr { .. } => "Herdr",
            Multiplexer::Tmux { .. } => "tmux",
        }
    }
}

/// Detect the multiplexer from the environment and the setting.
#[must_use]
pub fn detect(env: &PaneEnv, setting: MultiplexerSetting) -> Option<Multiplexer> {
    fn present(s: &Option<String>) -> Option<&str> {
        s.as_deref().filter(|s| !s.is_empty())
    }
    if matches!(
        setting,
        MultiplexerSetting::Auto | MultiplexerSetting::Herdr
    ) && env.herdr_env.as_deref() == Some("1")
        && let (Some(pane), Some(bin)) = (present(&env.herdr_pane_id), present(&env.herdr_bin_path))
    {
        return Some(Multiplexer::Herdr {
            bin: bin.into(),
            pane: pane.into(),
        });
    }
    if matches!(setting, MultiplexerSetting::Auto | MultiplexerSetting::Tmux)
        && present(&env.tmux).is_some()
        && let Some(pane) = present(&env.tmux_pane)
    {
        return Some(Multiplexer::Tmux {
            bin: "tmux".into(),
            pane: pane.into(),
        });
    }
    None
}

/// Which way a new pane splits off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Right,
    Down,
}

/// Pick the split direction from the setting and the terminal size.
#[must_use]
pub fn choose_direction(setting: DirectionSetting, cols: u16, rows: u16) -> Direction {
    match setting {
        DirectionSetting::Right => Direction::Right,
        DirectionSetting::Down => Direction::Down,
        DirectionSetting::Auto if cols >= 120 && u32::from(cols) >= 2 * u32::from(rows) => {
            Direction::Right
        }
        DirectionSetting::Auto => Direction::Down,
    }
}

/// Why the typed command line was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum PaneRefusal {
    UnsafeArgument,
    UnsafeExePath,
    TooLong { bytes: usize },
    BinaryGone,
}

/// Pure (string checks only; never touches the file system):
/// `'<exe>' attach '<session_id>' --profile '<profile>'`. Never returns BinaryGone.
pub fn attach_command_line(exe: &Path, key: &RowKey) -> Result<String, PaneRefusal> {
    let id_ok = |b: u8| b.is_ascii_alphanumeric() || b"._:@".contains(&b);
    let profile_ok = |b: u8| b.is_ascii_alphanumeric() || b"_-".contains(&b);
    let id = key.session_id.as_bytes();
    let profile = key.profile.as_bytes();
    if !(1..=200).contains(&id.len())
        || !id_ok(id[0])
        || !id.iter().all(|&b| id_ok(b) || b == b'-')
        || !(1..=64).contains(&profile.len())
        || !profile_ok(profile[0])
        || !profile.iter().all(|&b| profile_ok(b) || b == b'.')
    {
        return Err(PaneRefusal::UnsafeArgument);
    }
    let exe = exe.to_str().ok_or(PaneRefusal::UnsafeExePath)?;
    if exe
        .chars()
        .any(|c| c.is_control() || matches!(c, '\'' | '\\'))
    {
        return Err(PaneRefusal::UnsafeExePath);
    }
    let line = format!(
        "'{exe}' attach '{}' --profile '{}'",
        key.session_id, key.profile
    );
    if line.len() > 1_000 {
        return Err(PaneRefusal::TooLong { bytes: line.len() });
    }
    Ok(line)
}

/// One multiplexer call: the program and its arguments, run with no shell.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

/// The split call for the multiplexer, direction, folder and focus flag.
#[must_use]
pub fn split_command(
    m: &Multiplexer,
    direction: Direction,
    cwd: Option<&Path>,
    focus: bool,
) -> CommandSpec {
    let (program, mut args): (_, Vec<OsString>) = match m {
        Multiplexer::Herdr { bin, .. } => (
            bin.clone(),
            [
                "pane",
                "split",
                "--current",
                "--direction",
                match direction {
                    Direction::Right => "right",
                    Direction::Down => "down",
                },
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
        ),
        Multiplexer::Tmux { bin, pane } => {
            let mut args: Vec<OsString> = vec![
                "split-window".into(),
                match direction {
                    Direction::Right => "-h",
                    Direction::Down => "-v",
                }
                .into(),
            ];
            if !focus {
                args.push("-d".into());
            }
            args.extend(["-t".into(), pane.into()]);
            (bin.clone(), args)
        }
    };
    if let Some(cwd) = cwd {
        args.push(
            if matches!(m, Multiplexer::Herdr { .. }) {
                "--cwd"
            } else {
                "-c"
            }
            .into(),
        );
        args.push(cwd.as_os_str().into());
    }
    match m {
        Multiplexer::Herdr { .. } => args.push(if focus { "--focus" } else { "--no-focus" }.into()),
        Multiplexer::Tmux { .. } => args.extend(["-P", "-F", "#{pane_id}"].map(Into::into)),
    }
    CommandSpec { program, args }
}

/// Read the new pane id from split output: Herdr JSON or the tmux `%<n>` line.
#[must_use]
pub fn parse_new_pane(m: &Multiplexer, stdout: &str) -> Option<String> {
    match m {
        Multiplexer::Herdr { .. } => {
            let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
            value
                .pointer("/result/pane/pane_id")?
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        }
        Multiplexer::Tmux { .. } => {
            let id = stdout.trim();
            let digits = id.strip_prefix('%')?;
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| id.to_owned())
        }
    }
}

/// The typing calls for the new pane: one for Herdr, two for tmux.
#[must_use]
pub fn type_commands(m: &Multiplexer, new_pane: &str, line: &str) -> Vec<CommandSpec> {
    let (bin, lists) = match m {
        Multiplexer::Herdr { bin, .. } => (bin, vec![vec!["pane", "run", new_pane, line]]),
        Multiplexer::Tmux { bin, .. } => (
            bin,
            vec![
                vec!["send-keys", "-t", new_pane, "-l", line],
                vec!["send-keys", "-t", new_pane, "Enter"],
            ],
        ),
    };
    lists
        .into_iter()
        .map(|args| CommandSpec {
            program: bin.clone(),
            args: args.into_iter().map(Into::into).collect(),
        })
        .collect()
}

/// What one multiplexer call produced.
#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// Runs one multiplexer call. A boxed future keeps the trait object-safe.
pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        spec: &CommandSpec,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = RunResult> + Send + '_>>;
}

/// The real runner: `tokio::process`, stdin null, output piped, env inherited.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(
        &self,
        spec: &CommandSpec,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = RunResult> + Send + '_>> {
        let spec = spec.clone();
        Box::pin(async move {
            let mut attempts = 0_u8;
            let mut child = loop {
                match tokio::process::Command::new(&spec.program)
                    .args(&spec.args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()
                {
                    Ok(child) => break child,
                    Err(error) if error.raw_os_error() == Some(26) && attempts < 20 => {
                        attempts += 1;
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Err(error) => {
                        return RunResult {
                            code: None,
                            stdout: String::new(),
                            stderr: error.to_string(),
                            timed_out: false,
                        };
                    }
                }
            };
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let mut out = Vec::new();
            let mut err = Vec::new();
            // The timed future owns the pipes, so cancelling it closes them even
            // when another process inherited a writer and never sends EOF.
            let completed = tokio::time::timeout(timeout, async {
                let (status, (), ()) = tokio::join!(
                    child.wait(),
                    drain(stdout, &mut out),
                    drain(stderr, &mut err)
                );
                status
            })
            .await;
            let (code, timed_out) = match completed {
                Ok(Ok(status)) => (status.code(), false),
                Ok(Err(error)) => {
                    err = error.to_string().into_bytes();
                    (None, false)
                }
                Err(_) => {
                    let _ = child.start_kill();
                    let status = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
                    (
                        status
                            .ok()
                            .and_then(Result::ok)
                            .and_then(|status| status.code()),
                        true,
                    )
                }
            };
            RunResult {
                code,
                stdout: String::from_utf8_lossy(&out).into_owned(),
                stderr: String::from_utf8_lossy(&err).into_owned(),
                timed_out,
            }
        })
    }
}

async fn drain<R: AsyncRead + Unpin>(pipe: Option<R>, kept: &mut Vec<u8>) {
    let Some(mut pipe) = pipe else { return };
    let mut buffer = [0; 8_192];
    loop {
        match pipe.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let remaining = 65_536_usize.saturating_sub(kept.len());
                kept.extend_from_slice(&buffer[..n.min(remaining)]);
            }
        }
    }
}

/// How long one split or typing step may take.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything `open_in_pane` needs.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneRequest {
    pub key: RowKey,
    pub title: String,
    pub exe: PathBuf,
    pub cwd: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// What opening in a pane produced.
#[derive(Clone, Debug, PartialEq)]
pub enum PaneOutcome {
    Opened {
        multiplexer: &'static str,
    },
    NoMultiplexer {
        command: String,
        off: bool,
    },
    Refused(PaneRefusal),
    SplitFailed {
        multiplexer: &'static str,
        reason: String,
    },
    TypeFailed {
        multiplexer: &'static str,
        reason: String,
        command: String,
    },
}

/// Split the current pane and type the attach line into the new shell.
pub async fn open_in_pane(
    runner: &dyn CommandRunner,
    env: &PaneEnv,
    settings: &PaneSettings,
    request: &PaneRequest,
) -> PaneOutcome {
    if std::fs::metadata(&request.exe).is_err() {
        return PaneOutcome::Refused(PaneRefusal::BinaryGone);
    }
    let command = match attach_command_line(&request.exe, &request.key) {
        Ok(command) => command,
        Err(reason) => return PaneOutcome::Refused(reason),
    };
    let Some(mux) = detect(env, settings.multiplexer) else {
        return PaneOutcome::NoMultiplexer {
            command,
            off: settings.multiplexer == MultiplexerSetting::Off,
        };
    };
    let cwd = request
        .cwd
        .as_deref()
        .filter(|cwd| std::fs::metadata(cwd).is_ok_and(|m| m.is_dir()));
    let split = split_command(
        &mux,
        choose_direction(settings.direction, request.cols, request.rows),
        cwd,
        settings.focus,
    );
    let result = runner.run(&split, STEP_TIMEOUT).await;
    let pane = if result.code == Some(0) && !result.timed_out {
        parse_new_pane(&mux, &result.stdout)
    } else {
        None
    };
    let Some(pane) = pane else {
        return PaneOutcome::SplitFailed {
            multiplexer: mux.name(),
            reason: failure_reason(&result),
        };
    };
    for spec in type_commands(&mux, &pane, &command) {
        let result = runner.run(&spec, STEP_TIMEOUT).await;
        if result.timed_out || result.code != Some(0) {
            return PaneOutcome::TypeFailed {
                multiplexer: mux.name(),
                reason: failure_reason(&result),
                command,
            };
        }
    }
    PaneOutcome::Opened {
        multiplexer: mux.name(),
    }
}

fn failure_reason(result: &RunResult) -> String {
    if result.timed_out {
        return "timed out after 5 s".into();
    }
    if result.code != Some(0) {
        if let Some(line) = result.stderr.lines().find(|line| !line.trim().is_empty()) {
            return sanitize(line).chars().take(200).collect();
        }
        return result.code.map_or_else(
            || "ended without an exit code".into(),
            |code| format!("exit code {code}"),
        );
    }
    format!(
        "unexpected reply: {}",
        sanitize(&result.stdout.chars().take(80).collect::<String>())
    )
}

/// The T1 notice, or None without a multiplexer.
#[must_use]
pub fn opening_text(title: &str, multiplexer: Option<&'static str>) -> Option<String> {
    multiplexer.map(|mux| format!("Opening \"{}\" in a new {mux} pane…", display_title(title)))
}

/// The outcome notice for the fleet overlay or the main screen.
#[must_use]
pub fn outcome_text(title: &str, outcome: &PaneOutcome) -> String {
    match outcome {
        PaneOutcome::Opened { multiplexer } => format!("Opened \"{}\" in a new {multiplexer} pane.", display_title(title)),
        PaneOutcome::NoMultiplexer { command, off: false } => format!("Not inside Herdr or tmux. To watch \"{}\" side by side, run this in another terminal: {command}", display_title(title)),
        PaneOutcome::NoMultiplexer { command, off: true } => format!("Opening in a new pane is turned off ([panes] multiplexer = \"off\"). Run this in another terminal: {command}"),
        PaneOutcome::SplitFailed { multiplexer, reason } => format!("{multiplexer} could not open a pane: {reason}."),
        PaneOutcome::TypeFailed { multiplexer, reason, command } => format!("Opened a new {multiplexer} pane, but could not start Talaria in it: {reason}. Run this there: {command}"),
        PaneOutcome::Refused(reason) => match reason {
            PaneRefusal::UnsafeArgument => "Cannot open this session in a pane: its id or profile has characters Talaria will not put on a command line.",
            PaneRefusal::UnsafeExePath => "Cannot open this session in a pane: the path of this Talaria binary has characters Talaria will not put on a command line.",
            PaneRefusal::TooLong { .. } => "Cannot open this session in a pane: the command would be longer than 1,000 bytes.",
            PaneRefusal::BinaryGone => "This Talaria binary is no longer on disk (it was rebuilt or moved). Restart Talaria, then try again.",
        }.into(),
    }
}

fn display_title(title: &str) -> String {
    sanitize(if title.is_empty() {
        "(untitled)"
    } else {
        title
    })
    .chars()
    .take(40)
    .collect()
}

// Match the visible replacements used by terminal history, without importing
// terminal presentation into the application crate.
fn sanitize(text: &str) -> String {
    let mut clean = String::new();
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => clean.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    clean.push(picture);
                }
            }
            0x7f => clean.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => clean.push_str(&format!("‹U+{code:04X}›")),
            _ => clean.push(ch),
        }
    }
    clean
}
