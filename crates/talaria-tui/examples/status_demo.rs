//! Demo of the status line: `render`, `document`, `run-command` and `watch`.
//!
//! The fixtures are built from parts, never from the real home directory, so the program
//! prints the same line on every machine.

use std::time::{Duration, Instant};

use talaria_tui::history::sanitize;
use talaria_tui::status::{
    CommandLimits, CommandSpec, Connection, Decision, ExternalSegment, Failure, ParsedOutput,
    RunMode, SessionRef, StatusCommandService, StatusDocument, StatusInputs, Tone, TurnState, plan,
};

/// The external text of the `full` fixture.
const FULL_EXTERNAL: &str = "tests: 296 passing";

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = arguments.first().cloned() else {
        eprintln!("usage: status_demo <render|document|run-command|watch> [options]");
        std::process::exit(2);
    };
    let rest = &arguments[1..];
    match command.as_str() {
        "render" => render(rest),
        "document" => document(rest),
        "run-command" => run_command(rest),
        "watch" => watch(rest),
        other => {
            eprintln!("unknown sub-command {other}");
            std::process::exit(2);
        }
    }
}

/// The fake home directory, built from parts.
fn fake_home() -> String {
    ["", "h", "dev"].join("/")
}

/// The worked example of the specification.
fn full() -> StatusInputs {
    StatusInputs {
        ultracode: false,
        connection: Connection::Connected,
        model: Some("opus-5.5".to_string()),
        reasoning_effort: Some("high".to_string()),
        reasoning_effort_wire: Some("high".to_string()),
        working_dir: Some(["", "h", "dev", "proj"].join("/")),
        home_dir: Some(fake_home()),
        branch: Some("main".to_string()),
        context_percent: Some(34),
        context_estimated: false,
        cost_usd: Some(0.12),
        cost_status: Some("actual".to_string()),
        needs_you: 2,
        working: 3,
        mode: RunMode::Live,
        session: Some(SessionRef {
            id: "sess-1".to_string(),
            title: None,
        }),
        turn: TurnState::Idle,
        pending_prompts: 0,
        subagents_active: 0,
        subagents_terminal: 0,
        usage_tokens: None,
        failed: 0,
        done: 0,
        approvals: None,
        pull_request: None,
    }
}

/// A connected state with nothing else set: the row is blank.
fn empty() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        ..StatusInputs::default()
    }
}

/// The `full` fixture with a link that is being made again.
fn reconnecting() -> StatusInputs {
    StatusInputs {
        connection: Connection::Reconnecting,
        ..full()
    }
}

/// The `full` fixture with text that must never reach the terminal raw.
fn hostile() -> StatusInputs {
    StatusInputs {
        model: Some("evil\u{202e}name".to_string()),
        working_dir: Some(format!("/w/{}red", char::from(0x1b))),
        branch: Some("br\tanch".to_string()),
        ..full()
    }
}

/// The external text of a fixture. The empty fixture has none: its row is blank.
fn external_text(fixture: &str) -> Option<String> {
    match fixture {
        "empty" => None,
        "hostile" => Some(format!("hostile{}", char::from(0x7))),
        _ => Some(FULL_EXTERNAL.to_string()),
    }
}

/// The inputs of a fixture.
fn fixture_inputs(fixture: &str) -> StatusInputs {
    match fixture {
        "empty" => empty(),
        "reconnecting" => reconnecting(),
        "hostile" => hostile(),
        _ => full(),
    }
}

/// The value of an option, or the default.
fn option(arguments: &[String], name: &str) -> Option<String> {
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == name {
            return arguments.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

/// Every value of a repeated option.
fn options(arguments: &[String], name: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == name
            && let Some(value) = arguments.get(index + 1)
        {
            found.push(value.clone());
            index += 1;
        }
        index += 1;
    }
    found
}

/// The arguments after the `--` separator.
fn program(arguments: &[String]) -> Vec<String> {
    match arguments.iter().position(|argument| argument == "--") {
        Some(index) => arguments[index + 1..].to_vec(),
        None => Vec::new(),
    }
}

/// A number of an option, or the default.
fn number(arguments: &[String], name: &str, default: u64) -> u64 {
    option(arguments, name)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

/// The external segment of a fixture, when it has one.
fn external_segment(fixture: &str, none: bool) -> Option<ExternalSegment> {
    if none {
        return None;
    }
    Some(ExternalSegment {
        text: external_text(fixture)?,
        tone: Tone::Muted,
    })
}

/// Print the line of a fixture at a width.
fn render(arguments: &[String]) {
    let fixture = option(arguments, "--fixture").unwrap_or_else(|| "full".to_string());
    let cols = number(arguments, "--cols", 80) as u16;
    let none = arguments.iter().any(|argument| argument == "--no-external");
    let external = external_segment(&fixture, none);
    let inputs = fixture_inputs(&fixture);
    let line = plan(&inputs, external.as_ref(), cols);
    println!("{}", line.text());
}

/// Print the document of a fixture.
fn document(arguments: &[String]) {
    let fixture = option(arguments, "--fixture").unwrap_or_else(|| "full".to_string());
    let inputs = fixture_inputs(&fixture);
    let Some(document) = StatusDocument::from_inputs(&inputs) else {
        eprintln!("the fixture holds no session, so it has no document");
        std::process::exit(1);
    };
    print!("{}", String::from_utf8_lossy(&document.encode()));
}

/// The parent environment of the demo.
fn parent_env() -> Vec<(String, String)> {
    std::env::vars().collect()
}

/// Run the command once and print what it produced.
fn run_command(arguments: &[String]) {
    let allow = options(arguments, "--allow");
    let timeout = Duration::from_millis(number(arguments, "--timeout-ms", 2_000));
    let limits = CommandLimits {
        timeout,
        ..CommandLimits::default()
    };
    let argv = program(arguments);
    if argv.is_empty() {
        eprintln!("usage: status_demo run-command [options] -- <program> [arguments]");
        std::process::exit(2);
    }
    let spec = CommandSpec {
        argv,
        launch_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        env_allowlist: allow,
    };
    let inputs = full();
    let document = StatusDocument::from_inputs(&inputs).expect("the fixture holds a session");
    let env = talaria_tui::status::build_child_env(
        &parent_env(),
        &spec.env_allowlist,
        &hermes_client::redact::is_suspicious_key,
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let started = Instant::now();
    let result = runtime.block_on(talaria_tui::status::run_command(
        &spec,
        env,
        document.encode(),
        &limits,
    ));
    let elapsed = started.elapsed().as_millis();

    let (outcome, rows, truncated) = match &result.parsed {
        ParsedOutput::Rows { rows, truncated } => ("ok", rows.clone(), *truncated),
        ParsedOutput::Failure(failure) => (
            match failure {
                Failure::NoOutput => "no_output",
                Failure::InvalidUtf8 => "invalid_utf8",
                Failure::InvalidDocument(_) => "invalid_document",
                Failure::NonzeroExit { .. } => "command_failed",
                Failure::TimedOut => "timed_out",
                Failure::NotFound => "not_found",
                Failure::SpawnFailed(_) => "spawn_failed",
            },
            Vec::new(),
            false,
        ),
    };
    let marker = match &result.parsed {
        ParsedOutput::Rows { .. } => "-".to_string(),
        ParsedOutput::Failure(failure) => failure.marker(),
    };
    println!("outcome={outcome}");
    println!("elapsed_ms={elapsed}");
    println!("marker={marker}");
    println!("rows={}", rows.len());
    println!("truncated={truncated}");
    for (index, row) in rows.iter().enumerate() {
        println!("row[{index}]={}", sanitize(&row.text));
    }
    if let Some(cols) = arguments
        .iter()
        .position(|argument| argument == "--render-cols")
        .and_then(|index| arguments.get(index + 1))
        .and_then(|value| value.parse::<u16>().ok())
    {
        let external = talaria_tui::status::external_segment(&result);
        let line = plan(&inputs, external.as_ref(), cols);
        println!("line={}", sanitize(&line.text()));
    }
}

/// Run the command on a timer and print when it was started.
fn watch(arguments: &[String]) {
    let allow = options(arguments, "--allow");
    let seconds = number(arguments, "--seconds", 10);
    let interval = number(arguments, "--interval", 5);
    let change_every = number(arguments, "--change-every", 0);
    let argv = program(arguments);
    if argv.is_empty() {
        eprintln!("usage: status_demo watch [options] -- <program> [arguments]");
        std::process::exit(2);
    }
    let spec = CommandSpec {
        argv,
        launch_dir: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        env_allowlist: allow,
    };
    let inputs = full();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    runtime.block_on(async move {
        let mut service = StatusCommandService::new(
            spec,
            Duration::from_secs(interval),
            parent_env(),
            hermes_client::redact::is_suspicious_key,
            CommandLimits::default(),
        );
        let started = Instant::now();
        let mut changes = 0u64;
        let mut pending = 0u32;
        let mut spawns = 0u64;
        let ticks = seconds * 10;
        for tick in 0..ticks {
            // The ticks follow an absolute schedule, so the offsets do not drift.
            let target = started + Duration::from_millis(tick * 100);
            let now = Instant::now();
            if now < target {
                tokio::time::sleep(target - now).await;
            }
            if change_every > 0 {
                // The document changes on the tick that reaches the change interval.
                let due = tick / (change_every * 10);
                while changes < due {
                    changes += 1;
                    pending += 1;
                }
            }
            let mut inputs = inputs.clone();
            inputs.pending_prompts = pending;
            let document =
                StatusDocument::from_inputs(&inputs).expect("the fixture holds a session");
            if service.on_tick(Instant::now(), &document, |_| {}) == Decision::Run {
                println!("spawn offset_ms={}", tick * 100);
                spawns += 1;
            }
        }
        service.shutdown().await;
        println!("spawns={spawns}");
    });
}
