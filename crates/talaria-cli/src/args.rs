//! Command line grammar. Bench flags are listed for help; the binary handles them first.

use std::path::PathBuf;

use clap::{ColorChoice, CommandFactory, Parser};

use talaria_app::bg::explicit_slug;
use talaria_app::launch::{Launch, StartMode, UsageError, new_session_options, parse_effort};

pub(crate) fn required_text(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("the value is empty".into())
    } else {
        Ok(value.to_string())
    }
}

pub(crate) fn required_path(value: &str) -> Result<PathBuf, String> {
    if value.trim().is_empty() {
        Err("the value is empty".into())
    } else {
        Ok(PathBuf::from(value))
    }
}

fn reject_bench(value: &str) -> Result<String, String> {
    let _ = value;
    Err("see --help".into())
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "talaria",
    version,
    about = "Talk to a Hermes session from the terminal.",
    color = ColorChoice::Never,
    after_long_help = "Print mode runs one finite prompt without a screen. -p takes a prompt, not a profile name.\nPass < /dev/null when standard input is an open pipe you do not mean to send.\nWithout --timeout, talaria -p waits as long as the turn runs; set --timeout in automated jobs.\nPrint exits: 0 completed, 1 failed or unknown, 2 usage, 3 input required, 4 not started,\n124 timed out, 129 hangup, 130 interrupted, 143 terminated.\nJSON format: docs/formats/print-result-v1.md\n\n--bg starts a new session in the background and returns once Hermes has accepted the prompt.\nGive the prompt as an argument or on standard input, as for -p.\n--worktree (-w) first creates a git worktree for the session: a second checkout on a new\nbranch talaria/<name>, under .worktrees/ in the main checkout, cut from a freshly fetched\norigin. Folders listed in .worktreeinclude are shared with the main checkout by a link, so\nedits inside them reach the main checkout. Uncommitted changes in the main checkout are not\ncarried into the tree. --name sets the worktree's name and needs --worktree.\nBackground exits: 0 launched, 1 unknown (the prompt may have started), 2 usage, 4 not started,\n130 interrupted before the prompt was sent.\nJSON format: docs/formats/bg-result-v1.md"
)]
pub struct Cli {
    /// Route substantive interactive prompts through workflow agents.
    #[arg(long, global = true)]
    pub ultracode: bool,
    #[arg(short = 'c', long = "continue", conflicts_with = "resume")]
    pub continue_session: bool,
    #[arg(long, num_args = 0..=1, value_name = "ID", value_parser = required_text)]
    pub resume: Option<Option<String>>,
    #[arg(long, value_parser = required_text)]
    pub model: Option<String>,
    #[arg(long, value_parser = required_text)]
    pub provider: Option<String>,
    #[arg(long, value_parser = required_text)]
    pub effort: Option<String>,
    #[arg(long, value_parser = required_text, default_value = "default")]
    pub profile: String,
    #[arg(long, value_name = "PATH", value_parser = required_path)]
    pub record: Option<PathBuf>,
    #[arg(long)]
    pub no_ensure: bool,
    #[arg(short = 'p', long = "print")]
    pub print: bool,
    /// Start a session in the background and return.
    #[arg(long)]
    pub bg: bool,
    /// Create a git worktree for the background session.
    #[arg(short = 'w', long)]
    pub worktree: bool,
    /// Name for the worktree. Needs `--worktree`.
    #[arg(long)]
    pub name: Option<String>,
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
    #[arg(long = "output-format")]
    pub output_format: Option<String>,
    #[arg(long = "timeout", allow_hyphen_values = true)]
    pub timeout: Option<String>,
    #[arg(long = "bench-replay", value_name = "FIXTURE", value_parser = reject_bench)]
    pub bench_replay: Option<String>,
    #[arg(long = "bench-speed", value_name = "SPEED", value_parser = reject_bench)]
    pub bench_speed: Option<String>,
    #[arg(long = "bench-report", value_name = "PATH", value_parser = reject_bench)]
    pub bench_report: Option<String>,
}

impl Cli {
    /// The launch this command line asks for. New-session options with `-c` or `--resume` and
    /// an unknown `--effort` are refused here. The folder is the current one, made canonical
    /// when it can be; the binary refuses one that cannot be read.
    pub fn into_launch(self) -> Result<Launch, UsageError> {
        let mode = if self.continue_session {
            StartMode::Continue
        } else if let Some(resume) = self.resume {
            StartMode::Resume(resume.map(|id| id.trim().to_string()))
        } else {
            StartMode::New
        };
        let folder = std::env::current_dir()
            .map(|folder| folder.canonicalize().unwrap_or(folder))
            .unwrap_or_default();
        let launch = Launch {
            mode,
            model: self.model,
            provider: self.provider,
            effort: self.effort,
            profile: self.profile.trim().to_string(),
            folder,
            record: self.record,
            bench_replay: self.bench_replay.map(PathBuf::from),
            no_ensure: self.no_ensure,
        };
        let given = new_session_options(&launch);
        if launch.mode != StartMode::New && !given.is_empty() {
            return Err(UsageError::NewSessionOptionsWithResume { given });
        }
        if let Some(effort) = &launch.effort {
            parse_effort(effort)?;
        }
        Ok(launch)
    }
}

pub fn render_usage(error: &clap::Error) -> (String, u8) {
    use clap::error::ErrorKind;
    match error.kind() {
        ErrorKind::DisplayVersion => (format!("talaria {}\n", env!("CARGO_PKG_VERSION")), 0),
        ErrorKind::DisplayHelp => {
            let mut command = Cli::command();
            let mut text = Vec::new();
            command.write_long_help(&mut text).expect("help");
            if !text.ends_with(b"\n") {
                text.push(b'\n');
            }
            (String::from_utf8_lossy(&text).into_owned(), 0)
        }
        _ => (neutralise(&error.render().ansi().to_string()), 2),
    }
}

/// True when the first argument is the fleet command.
pub fn is_fleet_command(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "fleet")
}

/// True when the first argument is the attach command.
pub fn is_attach_command(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "attach")
}

/// True when the first argument is the diagnostics command.
pub fn is_doctor_command(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "doctor")
}

/// True when the first argument is the profiles command.
pub fn is_profiles_command(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "profiles")
}

pub fn is_workflow_command(args: &[String]) -> bool {
    args.first().is_some_and(|arg| arg == "workflow")
}

pub fn write_usage(error: &clap::Error) -> u8 {
    let (text, code) = render_usage(error);
    match code {
        0 => print!("{text}"),
        _ => eprint!("{text}"),
    }
    code
}

fn is_bidi(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

pub(crate) fn neutralise(raw: &str) -> String {
    let mut line = String::new();
    for character in raw.chars() {
        if character == '\n' || character == '\r' {
            if !line.ends_with(' ') && !line.is_empty() {
                line.push(' ');
            }
            continue;
        }
        if character.is_control() || is_bidi(character) {
            line.push('?');
        } else {
            line.push(character);
        }
    }
    let mut line = cut_quoted(&line);
    while line.ends_with(' ') {
        line.pop();
    }
    line.push('\n');
    line
}

fn cut_quoted(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '`' || character == '\'' {
            let mut inner = String::new();
            for next in chars.by_ref() {
                if next == character {
                    break;
                }
                inner.push(next);
            }
            if inner.chars().count() > 64 {
                inner = inner.chars().take(64).collect();
            }
            out.push(character);
            out.push_str(&inner);
            out.push(character);
        } else {
            out.push(character);
        }
    }
    out
}

/// Why the print options are unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintUsageError {
    pub what: String,
    pub next_step: String,
}

/// Check the print and background options by hand after clap, with or without `-p`.
pub fn validate_print_options(cli: &Cli) -> Result<(), PrintUsageError> {
    let error = |what: String, next_step: String| PrintUsageError { what, next_step };
    let help = |what: &str| error(what.into(), "Run talaria --help.".into());
    if cli.bg && cli.print {
        return Err(help("talaria --bg cannot be combined with -p."));
    }
    if cli.bg && (cli.continue_session || cli.resume.is_some()) {
        return Err(help(
            "talaria --bg starts a new session; it cannot be combined with -c or --resume.",
        ));
    }
    if cli.worktree && !cli.bg {
        return Err(help("talaria --worktree needs --bg."));
    }
    if let Some(name) = &cli.name {
        if !cli.worktree {
            return Err(help("talaria --name needs --worktree."));
        }
        if explicit_slug(name).is_err() {
            return Err(help(
                "talaria --name needs at least one ASCII letter or digit.",
            ));
        }
    }
    if cli.bg && cli.timeout.is_some() {
        return Err(help("--timeout only works with -p."));
    }
    if !cli.print && !cli.bg {
        let option = if cli.prompt.is_some() {
            Some("a prompt on the command line")
        } else if cli.output_format.is_some() {
            Some("--output-format")
        } else if cli.timeout.is_some() {
            Some("--timeout")
        } else {
            None
        };
        if let Some(option) = option {
            return Err(error(
                format!("{option} only works with -p."),
                format!("Add -p, or drop {option}."),
            ));
        }
        return Ok(());
    }
    if let Some(format) = &cli.output_format {
        match format.as_str() {
            "text" | "json" => {}
            "stream-json" => {
                return Err(error(
                    "streaming JSON output is not available yet.".into(),
                    "Use --output-format json.".into(),
                ));
            }
            _ => {
                return Err(error(
                    format!(
                        "--output-format must be text or json (got {}).",
                        talaria_app::headless::output::neutralise_line(format)
                    ),
                    "Pick one of those values.".into(),
                ));
            }
        }
    }
    if let Some(timeout) = &cli.timeout {
        let valid = !timeout.is_empty()
            && timeout.bytes().all(|b| b.is_ascii_digit())
            && timeout
                .parse::<u64>()
                .is_ok_and(|value| (1..=86400).contains(&value));
        if !valid {
            return Err(error(
                format!(
                    "--timeout must be a whole number of seconds from 1 to 86400 (got {}).",
                    talaria_app::headless::output::neutralise_line(timeout)
                ),
                "Pick a value in that range.".into(),
            ));
        }
    }
    if matches!(cli.resume, Some(None)) {
        return Err(error(
            "talaria -p needs a session id after --resume.".into(),
            "Find one with talaria --resume, then pass it.".into(),
        ));
    }
    Ok(())
}
