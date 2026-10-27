//! Read-only diagnostics and the explicitly confirmed gateway restart command.

use std::io::{self, IsTerminal, Write};
use std::time::Instant;

use clap::Parser;
use talaria_app::doctor::DoctorService;
use talaria_core::doctor::{
    PROBE_INTERVAL, RestartInput, RestartOutcome, RestartState, ask_input, confirmation_lines,
    diagnose, exit_code, outcome_fix, outcome_text, render_json, render_text, restart_update,
};

/// Options for the diagnostics command, separate from interactive launch options.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "talaria doctor",
    about = "Check the link to the Hermes gateway"
)]
pub struct DoctorArgs {
    #[arg(long, value_name = "NAME", value_parser = crate::args::required_text)]
    pub profile: Option<String>,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub restart: bool,
    #[arg(long, requires = "restart")]
    pub yes: bool,
}

/// Parse the command arguments, including their leading `doctor` word.
pub fn parse(args: &[String]) -> Result<DoctorArgs, clap::Error> {
    DoctorArgs::try_parse_from(
        std::iter::once("talaria doctor".to_owned()).chain(args.iter().skip(1).cloned()),
    )
}

/// Dispatch before the interactive terminal check.
pub fn run(args: &[String]) -> u8 {
    let args = match parse(args) {
        Ok(args) => args,
        Err(error) => {
            if error.kind() == clap::error::ErrorKind::DisplayHelp {
                print!("{error}");
                return 0;
            }
            eprintln!("talaria doctor: invalid arguments. Run talaria doctor --help.");
            return 2;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("talaria doctor: the async runtime could not start.");
            return 1;
        }
    };
    u8::try_from(runtime.block_on(run_doctor(args))).unwrap_or(1)
}

/// Print the diagnostics and optionally drive the confirmed restart state machine.
pub async fn run_doctor(args: DoctorArgs) -> i32 {
    let profile = args.profile.as_deref().unwrap_or("default").trim();
    if profile.is_empty() || (args.yes && !args.restart) {
        eprintln!("talaria doctor: invalid arguments. Run talaria doctor --help.");
        return 2;
    }
    let service = DoctorService::new(hermes_client::default_home());
    let mut facts = service.gather(profile).await;
    let mut report = diagnose(&facts);
    if !args.json {
        print!("{}", render_text(&report));
    }
    let mut outcome = None;
    if args.restart {
        let result = if !args.yes && (!io::stdin().is_terminal() || !io::stderr().is_terminal()) {
            RestartOutcome::NeedsConfirmation
        } else {
            confirmed_restart(&service, &facts, profile, args.yes).await
        };
        if !args.json {
            println!("Restart: {}", outcome_text(&result));
            if let Some(fix) = outcome_fix(&result) {
                println!("Fix: {fix}");
            }
        }
        if matches!(result, RestartOutcome::Restarted { .. }) {
            facts = service.gather(profile).await;
            report = diagnose(&facts);
            if !args.json {
                let text = render_text(&report);
                let (header, rows) = text.split_once('\n').unwrap_or((&text, ""));
                println!("{header}, after the restart");
                print!("{rows}");
            }
        }
        outcome = Some(result);
    }
    if args.json {
        let text = render_json(
            &report,
            outcome.as_ref(),
            &facts.pin,
            &facts.talaria_version,
        );
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    }
    exit_code(&report, outcome.as_ref())
}

async fn confirmed_restart(
    service: &DoctorService,
    facts: &talaria_core::doctor::DoctorFacts,
    profile: &str,
    scripted: bool,
) -> RestartOutcome {
    let (mut state, _) = restart_update(RestartState::Idle, ask_input(facts));
    if matches!(state, RestartState::Asking { .. }) {
        (state, _) = restart_update(state, RestartInput::ActiveWork(service.active_work().await));
    }
    if let RestartState::Confirming { profiles, work, .. } = &state {
        for line in confirmation_lines(profiles, work.as_ref()) {
            eprintln!("{line}");
        }
        let accepted = if scripted {
            true
        } else {
            eprint!("Type yes to restart: ");
            let _ = io::stderr().flush();
            let mut answer = String::new();
            io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("yes")
        };
        let input = if accepted {
            RestartInput::Confirm
        } else {
            RestartInput::Cancel
        };
        (state, _) = restart_update(state, input);
    }
    if let RestartState::Requesting { old_instance } = &state {
        let posted = service.post_restart(old_instance, profile).await;
        let answered_at = Instant::now();
        (state, _) = restart_update(state, RestartInput::Posted(posted));
        while matches!(state, RestartState::Waiting { .. }) {
            tokio::time::sleep(PROBE_INTERVAL).await;
            let probe = service.probe().await;
            (state, _) = restart_update(state, RestartInput::Tick(answered_at.elapsed()));
            (state, _) = restart_update(state, RestartInput::Probe(probe));
        }
    }
    match state {
        RestartState::Done(outcome) => outcome,
        _ => RestartOutcome::Refused {
            http_status: None,
            detail: "the restart could not be confirmed".into(),
        },
    }
}
