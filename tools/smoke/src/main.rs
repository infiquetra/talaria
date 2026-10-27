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
use smoke::config::{Phase2aConfig, SmokeConfig, launch_line, launch_line_env};
use smoke::report::StepOutcome;
use smoke::scrub::Scrubber;
use smoke::steps::run_smoke;
use smoke::steps2a::{run_smoke_2a, scratch_for};
use smoke::steps2b::run_smoke_2b;
use smoke::steps3::{run_smoke_3, shell_lines};
use smoke::suite::Suite;
use smoke::suite4a::{Phase4aConfig, run_smoke_4a};
use smoke::terminal::{HerdrCli, Terminal};
use smoke::tmux::{Paster, TmuxCli};

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
    /// The whole run, in seconds [default: 300 for phase1, 1800 for phase2a, 1500 for phase2b,
    /// 1200 for phase3, 900 for phase4a].
    #[arg(long, global = true)]
    deadline_secs: Option<u64>,
    /// The scale of the per-step limits; the deadline and the quiet periods stay.
    #[arg(long, global = true, default_value_t = 1.0)]
    step_scale: f32,
    /// Where the failure report is written.
    #[arg(long, global = true)]
    report_dir: Option<PathBuf>,
    /// Which suite runs: phase1, phase2a, phase2b, phase3, or phase4a.
    #[arg(long, global = true, default_value = "phase1")]
    suite: Suite,
    /// The tmux command line the phase2a suite pastes through.
    #[arg(long, global = true, default_value = "tmux")]
    tmux: PathBuf,
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
    let suite = cli.suite;
    if suite == Suite::Phase4a {
        let jq_found = std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| {
                let jq = dir.join("jq");
                jq.is_file() && is_executable(&jq)
            })
        });
        if !jq_found {
            return Err("the phase4a suite needs jq on the PATH to parse the doctor and profiles JSON; install jq and run it again".to_owned());
        }
        if cli.profile != "fixture"
            || cli.model != "minimax-m3.1-flash"
            || cli.provider != "ollama-cloud"
        {
            return Err("the phase4a suite checks texts that name profile fixture, model minimax-m3.1-flash and provider ollama-cloud; run it without --profile, --model and --provider".to_owned());
        }
    }
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
    if cli.deadline_secs == Some(0) {
        return Err("--deadline-secs must be above 0.".to_owned());
    }
    if suite == Suite::Phase2b
        && (cli.model != "minimax-m3.1-flash" || cli.provider != "ollama-cloud")
    {
        return Err("phase2b needs the default provider and model (ollama-cloud, minimax-m3.1-flash): its expected texts name them".to_owned());
    }
    if suite == Suite::Phase3
        && (cli.profile != "fixture"
            || cli.model != "minimax-m3.1-flash"
            || cli.provider != "ollama-cloud")
    {
        return Err("phase3 needs the default profile, provider and model (fixture, ollama-cloud, minimax-m3.1-flash): its expected texts name them".to_owned());
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

    if cli.suite == Suite::Phase2a {
        preflight_2a(
            cli,
            &scrubber,
            &home,
            &login,
            hermes_home.as_deref(),
            &temp_root,
        )?;
    }
    if suite == Suite::Phase2b || suite == Suite::Phase3 {
        let root = temp_root.to_string_lossy();
        if root.contains(' ') || root.contains('\'') {
            return Err(scrubber.scrub(&format!(
                "The scratch folder would be under {root}, which holds a space or a single quote; set TMPDIR to a folder without either."
            )));
        }
    }

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
    if suite == Suite::Phase3 {
        // The run id is not drawn yet; a placeholder of its length gives the same lengths.
        let absolute = std::path::absolute(&talaria).unwrap_or_else(|_| talaria.clone());
        let scratch = temp_root.join("talaria-smoke-00000000");
        let cfg = (
            cli.profile.as_str(),
            cli.model.as_str(),
            cli.provider.as_str(),
        );
        let home = hermes_home
            .as_deref()
            .filter(|home| !home.is_empty())
            .map(std::path::Path::new);
        if shell_lines(&absolute, &scratch, "00000000", cfg, home)
            .iter()
            .any(|line| line.chars().count() >= MAX_LAUNCH_LINE)
        {
            return Err(
                "a phase 3 shell line is longer than 600 characters; use a shorter temporary directory or binary path"
                    .to_owned(),
            );
        }
    }
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
    // The phase 2a run fixes its seed, so its scratch project, and with it the editor script
    // every launch line names, is known before it starts.
    let seed = match cli.suite {
        Suite::Phase1 | Suite::Phase3 | Suite::Phase4a | Suite::Phase4b => None,
        Suite::Phase2a | Suite::Phase2b => Some(rand::random::<u64>()),
    };
    let (start_line, resume_line) = match (cli.suite, seed) {
        (Suite::Phase2a, Some(seed)) => {
            let editor = scratch_for(&temp_root, seed).join("smoke-editor.sh");
            let editor = editor.to_string_lossy();
            let env = [("EDITOR", editor.as_ref())];
            (
                launch_line_env(
                    &env,
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
                ),
                launch_line_env(
                    &env,
                    &binary,
                    &["--no-ensure", "-c", "--profile", &cli.profile],
                ),
            )
        }
        _ => (start_line, resume_line),
    };
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
    if suite != Suite::Phase4a {
        let _ = std::fs::create_dir_all(&report_dir);
    }

    let config = SmokeConfig {
        talaria: binary,
        profile: cli.profile.clone(),
        model: cli.model.clone(),
        provider: cli.provider.clone(),
        deadline: Duration::from_secs(
            cli.deadline_secs
                .unwrap_or_else(|| cli.suite.default_deadline_secs()),
        ),
        step_scale: cli.step_scale,
        // Phase 3 keeps its scratch only when --report-dir asks for the captures.
        report_dir: if suite == Suite::Phase3 || suite == Suite::Phase4a {
            cli.report_dir.clone()
        } else {
            Some(report_dir.clone())
        },
        require: std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1"),
        temp_root,
        seed,
    };

    let stop = Arc::new(AtomicBool::new(false));
    flag::register(SIGINT, Arc::clone(&stop)).map_err(|error| error.to_string())?;
    flag::register(SIGTERM, Arc::clone(&stop)).map_err(|error| error.to_string())?;

    let binary_label = if suite == Suite::Phase4a {
        "<binary>".to_owned()
    } else {
        scrubber.scrub(&config.talaria.display().to_string())
    };
    println!("talaria: {binary_label}");
    println!("herdr: {}", scrubber.scrub(&herdr.version()));

    let clock = SystemClock::new();
    let report = match suite {
        Suite::Phase1 => run_smoke(&herdr, &clock, &config, &stop),
        Suite::Phase2a => {
            let seed = seed.unwrap_or(0);
            let scratch = scratch_for(&config.temp_root, seed);
            let tmux = TmuxCli::new(&cli.tmux, &scratch.join("tmux-exit.txt"), &scratch);
            println!("tmux: {}", scrubber.scrub(&tmux.version()));
            let cfg2 = Phase2aConfig {
                tmux: cli.tmux.clone(),
            };
            run_smoke_2a(&herdr, &tmux, &clock, &config, &cfg2, &stop)
        }
        Suite::Phase2b => run_smoke_2b(&herdr, &clock, &config, &stop),
        Suite::Phase3 => run_smoke_3(&herdr, &clock, &config, &stop),
        Suite::Phase4a => run_smoke_4a(
            &herdr,
            &clock,
            &config,
            &Phase4aConfig {
                empty_home_parent: config.temp_root.clone(),
            },
            &stop,
        ),
        Suite::Phase4b => smoke::suite4b::run_smoke_4b(
            &herdr,
            &clock,
            &config,
            &smoke::suite4b::Phase4bConfig {
                empty_home_parent: config.temp_root.clone(),
            },
            &stop,
        ),
    };

    // The scratch project is known now, so the pane text can name it as well.
    let late = Scrubber::new(
        &home,
        &login,
        &report.scratch,
        hermes_home.as_deref(),
        &std::env::temp_dir(),
    );
    let public_text = |text: &str| {
        if suite == Suite::Phase4a {
            late.scrub(&text.replace(&config.talaria.to_string_lossy().to_string(), "<binary>"))
        } else {
            late.scrub(text)
        }
    };
    for line in report.step_lines().lines() {
        println!("{}", public_text(line));
    }
    if let Some(tab) = &report.tab {
        println!("tab: {}", tab.0);
    }
    if let Some((step, reason)) = failing_step(&report) {
        let block = pane_block(&report, step);
        for line in &block {
            println!("{}", public_text(line));
        }
        let path = report_dir.join("smoke-report.txt");
        let mut text = String::new();
        text.push_str(&public_text(&format!("step: {step}\nreason: {reason}\n")));
        text.push_str(&format!("herdr: {}\n", report.version));
        text.push_str(&format!("talaria: {binary_label}\n"));
        for line in &block {
            text.push_str(&public_text(line));
            text.push('\n');
        }
        if suite != Suite::Phase4a || !report.interrupted {
            let _ = std::fs::create_dir_all(&report_dir);
            let _ = std::fs::write(&path, text);
            println!("report: {}", late.scrub(&path.display().to_string()));
        }
    }
    if let Some(tab) = &report.unclosed_tab {
        println!("cleanup: tab {} was not closed", tab.0);
    }
    if let Some(session) = &report.unclosed_session {
        println!("cleanup: tmux session {session} was not closed");
    }
    println!("{}", report.result_line());
    Ok(u8::try_from(report.exit_code()).unwrap_or(1))
}

/// The phase 2a checks before anything is created: tmux runs, and the scratch path can be
/// written into the single-quoted scripts.
fn preflight_2a(
    cli: &Cli,
    scrubber: &Scrubber,
    home: &str,
    login: &str,
    hermes_home: Option<&str>,
    temp_root: &std::path::Path,
) -> Result<(), String> {
    let runs = std::process::Command::new(&cli.tmux)
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success());
    if !runs {
        return Err(scrubber.scrub(&format!(
            "tmux was not found or did not run: {}",
            cli.tmux.display()
        )));
    }
    let root = temp_root.to_string_lossy();
    if root.contains(' ') || root.contains('\'') {
        // The temporary folder is what the sentence names, so only the home folder and the
        // login name are scrubbed from it.
        let named = Scrubber::new(
            home,
            login,
            std::path::Path::new(""),
            hermes_home,
            std::path::Path::new(""),
        );
        return Err(named.scrub(&format!(
            "The scratch folder would be under {root}, which holds a space or a single quote; set \
             TMPDIR to a folder without either."
        )));
    }
    Ok(())
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
        let recent = capture.recent.trim_end();
        let visible = capture.visible.trim_end();
        let include_recent = index + 1 == outcome.len() && !recent.is_empty();
        if !include_recent || !recent.contains(visible) {
            lines.push(visible.to_owned());
        }
        // The recent text of the pane in use is the last one.
        if include_recent {
            lines.push(recent.to_owned());
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
