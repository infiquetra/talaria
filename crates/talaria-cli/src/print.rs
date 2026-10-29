//! The headless entry point: print one answer and return its exit code.

use std::io::{self, IsTerminal, Read, Write};
use std::time::{Duration, Instant};
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::headless::input::{MAX_STDIN_BYTES, PromptError, assemble_prompt};
use talaria_app::headless::output::{
    HeadlessResult, render_json, render_stderr, render_text_stdout,
};
use talaria_app::headless::run::{StopCause, failed_before_open, stopped_before_submit};
use talaria_app::headless::service::{HeadlessTimings, run_headless};
use talaria_app::headless::stream_input::{INPUT_QUEUE, spawn_input_reader};
use talaria_app::headless::stream_service::{StreamSource, run_stream};
use talaria_app::headless::{OutputFormat, PrintOptions};

use crate::args::Cli;

/// Run one headless turn, write its output, and return the exit code.
pub fn run_print(cli: &Cli, started: Instant) -> u8 {
    if let Err(error) = crate::args::validate_print_options(cli) {
        return usage(&error.what, &error.next_step);
    }
    let options = PrintOptions {
        prompt_arg: cli.prompt.clone(),
        format: match cli.output_format.as_deref() {
            Some("json") => OutputFormat::Json,
            Some("stream-json") => OutputFormat::StreamJson,
            _ => OutputFormat::Text,
        },
        timeout: cli
            .timeout
            .as_ref()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs),
    };
    if cli.input_format.as_deref() == Some("stream-json") && io::stdin().is_terminal() {
        return usage(
            "--input-format stream-json reads standard input, which is a terminal.",
            "pipe JSON lines into talaria, or use --input-format text.",
        );
    }
    let launch = match cli.clone().into_launch() {
        Ok(launch) => launch,
        Err(error) => {
            let failure = LaunchFailure::from(error);
            return usage(&failure.what, &failure.next_step);
        }
    };
    let preflight = if !launch.folder.is_dir() {
        Err(failure::folder_unreadable())
    } else {
        launch
            .record
            .as_deref()
            .map(talaria_app::app::check_record_path)
            .transpose()
            .map(|_| ())
    };
    if let Err(error) = preflight {
        return write_result(&failed_before_open(error), options.format);
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            return write_result(
                &failed_before_open(LaunchFailure {
                    what: "could not start the event loop.".into(),
                    next_step: "Run the command again.".into(),
                    exit_code: 4,
                }),
                options.format,
            );
        }
    };
    let code = runtime.block_on(async {
        let (tx, mut stops) = tokio::sync::mpsc::channel(8);
        for (kind, number) in [
            (tokio::signal::unix::SignalKind::interrupt(), 2),
            (tokio::signal::unix::SignalKind::terminate(), 15),
            (tokio::signal::unix::SignalKind::hangup(), 1),
        ] {
            let mut signal = match tokio::signal::unix::signal(kind) {
                Ok(signal) => signal,
                Err(_) => return write_result(&failed_before_open(LaunchFailure { what: "could not listen for shutdown signals.".into(), next_step: "Run the command again.".into(), exit_code: 4 }), options.format),
            };
            let sender = tx.clone();
            tokio::spawn(async move {
                while signal.recv().await.is_some() {
                    if sender.send(StopCause::Signal(number)).await.is_err() { break; }
                }
            });
        }
        if let Some(timeout) = options.timeout {
            let sender = tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep_until(tokio::time::Instant::from_std(started + timeout)).await;
                let _ = sender.send(StopCause::Timeout).await;
            });
        }
        drop(tx);
        let home = std::env::var_os("HERMES_HOME").map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".hermes")))
            .unwrap_or_default();
        if cli.input_format.as_deref() == Some("stream-json") {
            let source = StreamSource::Lines(spawn_input_reader(io::stdin(), INPUT_QUEUE));
            return run_stream(&launch, &options, source, &home, started, stops, HeadlessTimings::default(), io::stdout()).await;
        }
        let bytes = if io::stdin().is_terminal() { None } else {
            let read = tokio::task::spawn_blocking(|| {
                let mut bytes = Vec::new();
                io::stdin().lock().take((MAX_STDIN_BYTES + 1) as u64).read_to_end(&mut bytes).map(|_| bytes)
            });
            tokio::select! {
                biased;
                Some(cause) = stops.recv() => return write_result(&stopped_before_submit(cause), options.format),
                result = read => match result {
                    Ok(Ok(bytes)) => Some(bytes),
                    _ => return usage("standard input could not be read.", "Send UTF-8 text on standard input, or give the prompt after -p."),
                }
            }
        };
        let text = match assemble_prompt(options.prompt_arg.as_deref(), bytes.as_deref()) {
            Ok(text) => text,
            Err(error) => return prompt_error(error),
        };
        if options.format == OutputFormat::StreamJson {
            return run_stream(&launch, &options, StreamSource::Prompt(text), &home, started, stops, HeadlessTimings::default(), io::stdout()).await;
        }
        let result = run_headless(&launch, &options, text, &home, started, stops, HeadlessTimings::default()).await;
        write_result(&result, options.format)
    });
    runtime.shutdown_background();
    code
}

fn usage(what: &str, next_step: &str) -> u8 {
    let _ = io::stderr()
        .lock()
        .write_all(format!("talaria: {what}\nNext step: {next_step}\n").as_bytes());
    2
}

fn prompt_error(error: PromptError) -> u8 {
    let (what, next) = match error {
        PromptError::Empty => (
            "no prompt: give it after -p or on standard input.",
            "Run talaria -p \"your prompt\", or pipe text into talaria -p.",
        ),
        PromptError::StdinTooLarge => (
            "standard input is larger than 1 MiB.",
            "Send a shorter text, or reference the file in the prompt instead.",
        ),
        PromptError::PromptTooLarge => (
            "the prompt text is larger than 1 MiB.",
            "Send a shorter text, or reference the file in the prompt instead.",
        ),
        PromptError::NotUtf8 => (
            "standard input is not UTF-8 text.",
            "Convert it to UTF-8, or reference the file in the prompt instead.",
        ),
    };
    usage(what, next)
}

fn write_result(result: &HeadlessResult, format: OutputFormat) -> u8 {
    let bytes = match format {
        OutputFormat::Text => render_text_stdout(result, io::stdout().is_terminal()),
        OutputFormat::Json => render_json(result).into_bytes(),
        OutputFormat::StreamJson => render_json(result).into_bytes(),
    };
    let _ = io::stdout().lock().write_all(&bytes);
    let _ = io::stderr().lock().write_all(&render_stderr(result));
    result.exit_code
}
