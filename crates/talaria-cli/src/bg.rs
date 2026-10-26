//! `talaria --bg`: start a session in the background, confirm Hermes took the prompt, and return.

use std::io::{self, IsTerminal, Read, Write};
use std::time::{Duration, Instant};

use talaria_app::bg::{
    CancelToken, LaunchConn, LaunchError, LaunchIds, LaunchOrigin, LaunchOutcome, LaunchRequest,
    WorktreeWanted, exit_code, render_json, render_text, run_launch,
};
use talaria_app::commands::fresh_submission_id;
use talaria_app::connection::{ConnectionService, OpenConnection};
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::headless::input::{MAX_STDIN_BYTES, PromptError, assemble_prompt};
use talaria_app::launch::{Launch, new_request_id};
use talaria_app::models::{FlagsGiven, OfferedCreateKeys, apply_saved_default};
use talaria_app::worktree::GitRunner;
use tokio::sync::watch;

use crate::args::Cli;

/// How long closing the connection may take once the launch has finished.
const CLOSE_LIMIT: Duration = Duration::from_secs(2);

/// Run one background launch, write its output, and return the exit code.
pub fn run_bg(cli: &Cli, started: Instant) -> u8 {
    let _ = started;
    if let Err(error) = crate::args::validate_print_options(cli) {
        return usage(&error.what, &error.next_step);
    }
    let json = cli.output_format.as_deref() == Some("json");
    let launch = match cli.clone().into_launch() {
        Ok(launch) => launch,
        Err(error) => {
            let failure = LaunchFailure::from(error);
            return usage(&failure.what, &failure.next_step);
        }
    };
    let preflight = if launch.folder.is_dir() {
        launch
            .record
            .as_deref()
            .map(talaria_app::app::check_record_path)
            .transpose()
            .map(|_| ())
    } else {
        Err(failure::folder_unreadable())
    };
    if let Err(failure) = preflight {
        return refuse(&failure);
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            return finish(
                &not_started(&failed(
                    "could not start the event loop.",
                    "Run the command again.",
                )),
                json,
            );
        }
    };
    let code = runtime.block_on(background(cli, launch, json));
    runtime.shutdown_background();
    code
}

async fn background(cli: &Cli, launch: Launch, json: bool) -> u8 {
    let cancel = CancelToken::new();
    let (stopped_tx, mut stopped) = watch::channel(false);
    for kind in [
        tokio::signal::unix::SignalKind::interrupt(),
        tokio::signal::unix::SignalKind::terminate(),
        tokio::signal::unix::SignalKind::hangup(),
    ] {
        let Ok(mut signal) = tokio::signal::unix::signal(kind) else {
            return finish(
                &not_started(&failed(
                    "could not listen for shutdown signals.",
                    "Run the command again.",
                )),
                json,
            );
        };
        let cancel = cancel.clone();
        let stopped_tx = stopped_tx.clone();
        tokio::spawn(async move {
            while signal.recv().await.is_some() {
                cancel.cancel();
                let _ = stopped_tx.send(true);
            }
        });
    }
    drop(stopped_tx);
    let stop = async move {
        let _ = stopped.wait_for(|stopped| *stopped).await;
    };
    tokio::pin!(stop);
    let prompt = tokio::select! {
        biased;
        () = &mut stop => return finish(&interrupted(), json),
        read = read_prompt(cli.prompt.clone()) => match read {
            Ok(prompt) => prompt,
            Err(code) => return code,
        },
    };
    let recorder = match launch
        .record
        .as_deref()
        .map(talaria_app::app::create_recorder)
        .transpose()
    {
        Ok(recorder) => recorder,
        Err(failure) => return refuse(&failure),
    };
    let home = hermes_client::default_home();
    let open = tokio::select! {
        biased;
        () = &mut stop => return finish(&interrupted(), json),
        open = ConnectionService::open(&launch, &home, recorder) => match open {
            Ok(open) => open,
            Err(failure) => return finish(&not_started(&failure_text(&failure)), json),
        },
    };
    let (request, warnings) = request_for(cli, &launch, prompt, &open);
    for warning in warnings {
        let _ = writeln!(io::stderr().lock(), "talaria: warning: {warning}");
    }
    let conn = LaunchConn::opened(open);
    let ids = LaunchIds {
        request_id: new_request_id(),
        submission_id: fresh_submission_id(),
    };
    let outcome = run_launch(&conn, &GitRunner::from_env(), request, ids, None, cancel).await;
    let _ = tokio::time::timeout(CLOSE_LIMIT, conn.close()).await;
    finish(&outcome, json)
}

/// The launch the command line asks for. Saved defaults fill only what the flags left empty.
fn request_for(
    cli: &Cli,
    launch: &Launch,
    prompt: String,
    open: &OpenConnection,
) -> (LaunchRequest, Vec<String>) {
    let mut request = LaunchRequest {
        prompt,
        profile: launch.profile.clone(),
        folder: launch.folder.clone(),
        model: launch.model.clone(),
        provider: launch.provider.clone(),
        effort: launch.effort.clone(),
        worktree: cli.worktree.then(|| WorktreeWanted {
            name: cli.name.clone(),
        }),
        origin: LaunchOrigin::CommandLine,
    };
    let mut warnings = Vec::new();
    let saved = match talaria_app::config::load_models() {
        Ok(saved) => saved,
        Err(notice) => {
            warnings.push(notice);
            return (request, warnings);
        }
    };
    let key = talaria_app::config::served_model_profile(&open.describe.profile_id);
    let Some(default) = saved.defaults.get(key) else {
        return (request, warnings);
    };
    let flags = FlagsGiven {
        model_or_provider: request.model.is_some() || request.provider.is_some(),
        effort: request.effort.is_some(),
    };
    let offered = OfferedCreateKeys(
        open.describe
            .session_create
            .parameters
            .iter()
            .cloned()
            .collect(),
    );
    let (filled, applied) = apply_saved_default(
        talaria_app::launch::NewSession {
            request_id: String::new(),
            source: String::new(),
            cwd: launch.folder.clone(),
            model: request.model.take(),
            provider: request.provider.take(),
            reasoning: request.effort.take(),
        },
        Some(default),
        flags,
        &offered,
    );
    request.model = filled.model;
    request.provider = filled.provider;
    request.effort = filled.reasoning;
    warnings.extend(
        applied
            .dropped_not_offered
            .iter()
            .map(|key| talaria_core::models::notice_not_offered(key)),
    );
    (request, warnings)
}

/// The prompt: the argument and standard input, assembled as print mode does.
///
/// A usage error is written here and comes back as the exit code.
async fn read_prompt(argument: Option<String>) -> Result<String, u8> {
    let bytes = if io::stdin().is_terminal() {
        None
    } else {
        let read = tokio::task::spawn_blocking(|| {
            let mut bytes = Vec::new();
            io::stdin()
                .lock()
                .take((MAX_STDIN_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
        .await;
        match read {
            Ok(Ok(bytes)) => Some(bytes),
            _ => {
                return Err(usage(
                    "talaria --bg: standard input could not be read.",
                    "Run talaria --help.",
                ));
            }
        }
    };
    assemble_prompt(argument.as_deref(), bytes.as_deref()).map_err(|error| {
        let what = match error {
            PromptError::Empty => {
                "talaria --bg needs a prompt: pass it as an argument or on standard input."
            }
            PromptError::StdinTooLarge | PromptError::PromptTooLarge => {
                "talaria --bg: the prompt is larger than 1 MiB."
            }
            PromptError::NotUtf8 => "talaria --bg: standard input is not valid UTF-8.",
        };
        usage(what, "Run talaria --help.")
    })
}

fn failed(what: &str, next_step: &str) -> LaunchError {
    LaunchError {
        what: what.into(),
        next_step: next_step.into(),
    }
}

fn failure_text(failure: &LaunchFailure) -> LaunchError {
    LaunchError {
        what: failure.what.clone(),
        next_step: failure.next_step.clone(),
    }
}

fn not_started(error: &LaunchError) -> LaunchOutcome {
    LaunchOutcome::NotStarted {
        error: error.clone(),
        closed_session: None,
        tree_removed: false,
        tree_kept: None,
    }
}

fn interrupted() -> LaunchOutcome {
    LaunchOutcome::Interrupted {
        tree_removed: false,
        tree_kept: None,
    }
}

fn usage(what: &str, next_step: &str) -> u8 {
    let _ = io::stderr()
        .lock()
        .write_all(format!("talaria: {what}\nNext step: {next_step}\n").as_bytes());
    2
}

/// A start-up failure that keeps its own exit code and writes nothing to standard output.
fn refuse(failure: &LaunchFailure) -> u8 {
    let _ = io::stderr().lock().write_all(failure.render().as_bytes());
    failure.exit_code
}

/// Print the result and return its exit code. Standard error comes first, so warnings precede
/// the success text.
fn finish(outcome: &LaunchOutcome, json: bool) -> u8 {
    let (text, errors) = render_text(outcome);
    let _ = io::stderr().lock().write_all(errors.as_bytes());
    let out = if json {
        format!("{}\n", render_json(outcome))
    } else {
        text
    };
    let _ = io::stdout().lock().write_all(out.as_bytes());
    exit_code(outcome)
}
