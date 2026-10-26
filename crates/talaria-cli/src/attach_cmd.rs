//! Resolve an existing session, then use the ordinary terminal resume path.

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{ColorChoice, Parser};
use hermes_client::discovery::{DiscoverOptions, EnsurePolicy, discover_with};
use talaria_app::attach::{
    AttachFailure, AttachRequest, ReferenceError, fast_path, find_session, validate_reference,
};
use talaria_app::failure::{self, LaunchFailure};
use talaria_app::launch::{Launch, StartMode};
use talaria_core::directory::rows::RowKey;

#[derive(Parser)]
#[command(
    name = "talaria attach",
    bin_name = "talaria attach",
    about = "Resume a Hermes session by its id, id prefix, or title.",
    color = ColorChoice::Never
)]
struct Options {
    #[arg(long, value_name = "name", value_parser = crate::args::required_text)]
    profile: Option<String>,
    #[arg(long)]
    no_ensure: bool,
    #[arg(long, value_name = "path", value_parser = crate::args::required_path)]
    record: Option<PathBuf>,
    #[arg(value_name = "SESSION")]
    session: String,
}

fn fail(failure: &LaunchFailure) -> ExitCode {
    eprint!("{}", failure.render());
    ExitCode::from(failure.exit_code)
}

fn reference_failure(error: ReferenceError) -> LaunchFailure {
    LaunchFailure {
        what: match error {
            ReferenceError::Empty => "the session to attach to is empty.",
            ReferenceError::TooLong { .. } => {
                "the session reference is longer than 200 characters."
            }
            ReferenceError::ControlCharacter => {
                "the session reference contains a control character."
            }
        }
        .to_owned(),
        next_step: "pass a session id, the start of one, or a title.".to_owned(),
        exit_code: 2,
    }
}

fn launch(key: RowKey, folder: PathBuf, record: Option<PathBuf>, no_ensure: bool) -> Launch {
    Launch {
        mode: StartMode::Resume(Some(key.session_id)),
        profile: key.profile,
        folder,
        record,
        no_ensure,
        model: None,
        provider: None,
        effort: None,
        bench_replay: None,
    }
}

/// Run the arguments after the program name, including `attach` itself.
pub fn run(args: &[String]) -> ExitCode {
    let options = match Options::try_parse_from(
        std::iter::once("talaria attach".to_owned()).chain(args.iter().skip(1).cloned()),
    ) {
        Ok(options) => options,
        Err(error) => {
            if error.kind() == clap::error::ErrorKind::DisplayHelp {
                print!("{}", error.render());
                return ExitCode::SUCCESS;
            }
            eprint!("{}", crate::args::neutralise(&error.render().to_string()));
            return ExitCode::from(2);
        }
    };
    let reference = match validate_reference(&options.session) {
        Ok(reference) => reference,
        Err(error) => return fail(&reference_failure(error)),
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return fail(&failure::no_terminal());
    }
    let folder = match std::env::current_dir() {
        Ok(folder) => folder.canonicalize().unwrap_or(folder),
        Err(_) => return fail(&failure::folder_unreadable()),
    };
    let request = AttachRequest {
        reference,
        profile: options.profile.map(|name| name.trim().to_owned()),
        no_ensure: options.no_ensure,
    };
    if let Some(key) = fast_path(&request) {
        return crate::run::run(launch(key, folder, options.record, options.no_ensure));
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return fail(&failure::link_failure("the async runtime could not start")),
    };
    let home = hermes_client::default_home();
    let target = runtime.block_on(async {
        let mut discovery = DiscoverOptions::default();
        if request.no_ensure {
            discovery.ensure = EnsurePolicy::Never;
        }
        let endpoint = discover_with(&home, &discovery)
            .await
            .map_err(|error| AttachFailure {
                notes: Vec::new(),
                failure: error.into(),
            })?;
        let served: Vec<String> = endpoint.profiles().iter().map(|p| p.name.clone()).collect();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64());
        find_session(&home, &request, &served, now).await
    });
    // The resolver has closed its side connection. Release its runtime before the screen
    // starts the ordinary resume runtime.
    drop(runtime);
    let target = match target {
        Ok(target) => target,
        Err(error) => {
            for note in error.notes {
                eprintln!("{note}");
            }
            return fail(&error.failure);
        }
    };
    for note in target.notes {
        eprintln!("{note}");
    }
    crate::run::run(launch(target.key, folder, options.record, true))
}
