//! Command line grammar. Bench flags are listed for help; the binary handles them first.

use std::path::PathBuf;

use clap::{ColorChoice, CommandFactory, Parser};

use talaria_app::launch::{Launch, StartMode, UsageError, new_session_options, parse_effort};

fn required_text(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("the value is empty".into())
    } else {
        Ok(value.to_string())
    }
}

fn required_path(value: &str) -> Result<PathBuf, String> {
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

#[derive(Debug, Parser)]
#[command(
    name = "talaria",
    version,
    about = "Talk to a Hermes session from the terminal.",
    color = ColorChoice::Never
)]
pub struct Cli {
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
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
    #[arg(long = "output-format")]
    pub output_format: Option<String>,
    #[arg(long = "timeout")]
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

fn neutralise(raw: &str) -> String {
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

/// Check the print options by hand after clap, with or without `-p`.
pub fn validate_print_options(cli: &Cli) -> Result<(), PrintUsageError> {
    let _ = cli;
    todo!("#235")
}
