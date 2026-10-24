//! `smoke run`: argument parsing, preflight, signal flag, exit codes and the result line.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use clap::{Parser, Subcommand};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;

use smoke::clock::SystemClock;
use smoke::config::{SmokeConfig, launch_line};
use smoke::report::StepOutcome;
use smoke::scrub::Scrubber;
use smoke::steps::run_smoke;
use smoke::terminal::{HerdrCli, Terminal};

/// The longest launch line the command line takes.
const MAX_LAUNCH_LINE: usize = 600;

/// The message of a run outside a Herdr pane.
const NOT_IN_HERDR: &str =
    "This test must run inside a Herdr pane (HERDR_ENV=1). Start it from a Herdr tab.";

#[derive(Parser)]
#[command(
    name = "smoke",
    about = "Live smoke test: drives the built binary through the Herdr command line."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// The binary under test; the default is the workspace build.
    #[arg(long, global = true)]
    talaria: Option<PathBuf>,
    /// The command line of the terminal multiplexer.
    #[arg(long, global = true, default_value = "herdr")]
    herdr: PathBuf,
    /// The profile the session runs in.
    #[arg(long, global = true, default_value = "fixture")]
    profile: String,
    /// The model the session uses.
    #[arg(long, global = true, default_value = "minimax-m3.1-flash")]
    model: String,
    /// The provider the model comes from.
    #[arg(long, global = true, default_value = "ollama-cloud")]
    provider: String,
    /// The whole run, in seconds.
    #[arg(long, global = true, default_value_t = 300)]
    deadline_secs: u64,
    /// The scale of the per-step limits; the deadline and the quiet periods stay.
    #[arg(long, global = true, default_value_t = 1.0)]
    step_scale: f32,
    /// Where the failure report is written.
    #[arg(long, global = true)]
    report_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the smoke test.
    Run,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Run => match run(&cli) {
            Ok(code) => ExitCode::from(code),
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        },
    }
}

/// The preflight, the run and the printing.
fn run(cli: &Cli) -> Result<u8, String> {
    if std::env::var("HERDR_ENV").as_deref() != Ok("1") {
        return Err(NOT_IN_HERDR.to_owned());
    }
    let workspace = std::env::var("HERDR_WORKSPACE_ID").unwrap_or_default();
    if workspace.is_empty() {
        return Err(NOT_IN_HERDR.to_owned());
    }
    if !cli.step_scale.is_finite() || cli.step_scale <= 0.0 {
        return Err("--step-scale must be a finite number above 0.".to_owned());
    }
    if cli.deadline_secs == 0 {
        return Err("--deadline-secs must be above 0.".to_owned());
    }
    if let Some(dir) = &cli.report_dir
        && !dir.is_dir()
    {
        return Err(format!(
            "--report-dir {} is not an existing directory.",
            dir.display()
        ));
    }
    let login = match std::env::var("USER")
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            std::env::var("LOGNAME")
                .ok()
                .filter(|name| !name.is_empty())
        }) {
        Some(name) => name,
        None => {
            return Err(
                "Neither USER nor LOGNAME is set; the smoke test cannot scrub the login name from \
                 pane text."
                    .to_owned(),
            );
        }
    };
    if login.chars().count() < 4 {
        return Err(
            "Your login name is too short to scrub safely from pane text; run the smoke test from \
             an account whose login name has four or more characters."
                .to_owned(),
        );
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let hermes_home = std::env::var("HERMES_HOME").ok();
    let temp_root = std::env::temp_dir();
    // The scratch project does not exist yet, so it is no needle here.
    let scrubber = Scrubber::new(
        &home,
        &login,
        std::path::Path::new(""),
        hermes_home.as_deref(),
        &temp_root,
    );

    let herdr = HerdrCli::new(&cli.herdr, &workspace);
    if let Err(error) = herdr.check() {
        return Err(scrubber.scrub(&format!(
            "the command `herdr tab list --workspace {workspace}` failed: {error}"
        )));
    }

    let talaria = match cli
        .talaria
        .clone()
        .or_else(|| std::env::var_os("TALARIA_SMOKE_BIN").map(PathBuf::from))
    {
        Some(path) => path,
        None => default_talaria(),
    };
    let resolved = talaria.canonicalize().ok();
    let binary = match &resolved {
        Some(path) if path.is_file() && is_executable(path) => path.clone(),
        _ => {
            return Err(scrubber.scrub(&format!(
                "No talaria binary at {}. Run `cargo build -p talaria-cli` or pass --talaria.",
                talaria.display()
            )));
        }
    };

    let start_line = launch_line(
        &binary,
        &[
            "--no-ensure",
            "--profile",
            &cli.profile,
            "--model",
            &cli.model,
            "--provider",
            &cli.provider,
        ],
    );
    let resume_line = launch_line(&binary, &["--no-ensure", "-c", "--profile", &cli.profile]);
    if start_line.chars().count() >= MAX_LAUNCH_LINE
        || resume_line.chars().count() >= MAX_LAUNCH_LINE
    {
        return Err(
            "the launch line is longer than 600 characters; shorten --model or --provider."
                .to_owned(),
        );
    }

    let report_dir = match &cli.report_dir {
        Some(dir) => dir.clone(),
        None => temp_root.join(format!("smoke-report-{}", run_id())),
    };
    let _ = std::fs::create_dir_all(&report_dir);

    let config = SmokeConfig {
        talaria: binary,
        profile: cli.profile.clone(),
        model: cli.model.clone(),
        provider: cli.provider.clone(),
        deadline: Duration::from_secs(cli.deadline_secs),
        step_scale: cli.step_scale,
        report_dir: Some(report_dir.clone()),
        require: std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1"),
        temp_root,
        seed: None,
    };

    let stop = Arc::new(AtomicBool::new(false));
    flag::register(SIGINT, Arc::clone(&stop)).map_err(|error| error.to_string())?;
    flag::register(SIGTERM, Arc::clone(&stop)).map_err(|error| error.to_string())?;

    println!(
        "talaria: {}",
        scrubber.scrub(&config.talaria.display().to_string())
    );
    println!("herdr: {}", scrubber.scrub(&herdr.version()));

    let clock = SystemClock::new();
    let report = run_smoke(&herdr, &clock, &config, &stop);

    // The scratch project is known now, so the pane text can name it as well.
    let late = Scrubber::new(
        &home,
        &login,
        &report.scratch,
        hermes_home.as_deref(),
        &std::env::temp_dir(),
    );
    for line in report.step_lines().lines() {
        println!("{}", late.scrub(line));
    }
    if let Some(tab) = &report.tab {
        println!("tab: {}", tab.0);
    }
    if let Some((step, reason)) = failing_step(&report) {
        let block = pane_block(&report, step);
        for line in &block {
            println!("{}", late.scrub(line));
        }
        let path = report_dir.join("smoke-report.txt");
        let mut text = String::new();
        text.push_str(&format!("step: {step}\nreason: {reason}\n"));
        text.push_str(&format!("herdr: {}\n", report.version));
        text.push_str(&format!(
            "talaria: {}\n",
            late.scrub(&config.talaria.display().to_string())
        ));
        for line in &block {
            text.push_str(&late.scrub(line));
            text.push('\n');
        }
        let _ = std::fs::write(&path, text);
        println!("report: {}", late.scrub(&path.display().to_string()));
    }
    if let Some(tab) = &report.unclosed_tab {
        println!("cleanup: tab {} was not closed", tab.0);
    }
    println!("{}", report.result_line());
    Ok(u8::try_from(report.exit_code()).unwrap_or(1))
}

/// The step that failed, with its reason.
fn failing_step(report: &smoke::SmokeReport) -> Option<(&'static str, String)> {
    report.outcomes.iter().find_map(|outcome| match outcome {
        StepOutcome::Fail { step, reason, .. } => Some((*step, reason.clone())),
        _ => None,
    })
}

/// The pane text of a failure, ready to print.
fn pane_block(report: &smoke::SmokeReport, step: &str) -> Vec<String> {
    let Some(outcome) = report.outcomes.iter().find_map(|outcome| match outcome {
        StepOutcome::Fail { capture, .. } => Some(capture),
        _ => None,
    }) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for (index, capture) in outcome.iter().enumerate() {
        let marker = format!(
            "----- pane text (step: {step}, pane: {}) -----",
            capture.pane
        );
        lines.push(marker.clone());
        lines.push(capture.visible.trim_end().to_owned());
        // The recent text of the pane in use is the last one.
        if index + 1 == outcome.len() {
            lines.push(capture.recent.trim_end().to_owned());
        }
        lines.push(marker);
    }
    lines
}

/// The binary under test, when no path was given.
fn default_talaria() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_default();
    root.join("target").join("debug").join("talaria")
}

/// The name of the report folder.
fn run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let value = nanos as u64;
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut id = String::with_capacity(8);
    let mut rest = value;
    for _ in 0..8 {
        id.push(alphabet[(rest % alphabet.len() as u64) as usize] as char);
        rest /= alphabet.len() as u64;
        rest = rest.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    }
    id
}

/// Whether the file may be run.
fn is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
