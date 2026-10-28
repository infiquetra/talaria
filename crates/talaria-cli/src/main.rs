use std::process::ExitCode;

use clap::Parser;

mod bench_replay;
mod workflow_builtins;

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if args.first().is_some_and(|arg| arg == "workflow")
        && args.get(1).is_some_and(|arg| arg == "builtins")
    {
        return workflow_builtins::run(&args[2..]);
    }
    if talaria_cli::args::is_workflow_command(&args) {
        return ExitCode::from(talaria_cli::workflow::entry(&args));
    }
    if talaria_cli::args::is_doctor_command(&args) {
        return ExitCode::from(talaria_cli::doctor_cmd::run(&args));
    }
    if args.first().is_some_and(|arg| arg == "capabilities") {
        let parsed = match talaria_cli::capabilities_cmd::parse(&args) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        };
        return match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(talaria_cli::capabilities_cmd::run(parsed)),
            Err(_) => {
                eprintln!("talaria capabilities: could not start the runtime");
                ExitCode::from(1)
            }
        };
    }
    if talaria_cli::configure_cmd::is_command(&args) {
        return talaria_cli::configure_cmd::run(&args);
    }
    if talaria_cli::args::is_attach_command(&args) {
        return talaria_cli::attach_cmd::run(&args);
    }
    if talaria_cli::args::is_profiles_command(&args) {
        return talaria_cli::profiles_cmd::run(&args);
    }
    if args.iter().any(|arg| arg == "--bench-replay") {
        match bench_replay::parse_args(&args) {
            Ok(Some(parsed)) => return bench_replay::run(parsed),
            Ok(None) => {}
            Err(error) => {
                eprintln!("talaria: {error}");
                return ExitCode::from(2);
            }
        }
    }
    if talaria_cli::args::is_fleet_command(&args) {
        return ExitCode::from(talaria_cli::fleet_cmd::run(&args));
    }
    match talaria_cli::args::Cli::try_parse() {
        Ok(cli) => {
            if cli.bg || cli.worktree || cli.name.is_some() {
                return ExitCode::from(talaria_cli::bg::run_bg(&cli, started));
            }
            if let Err(error) = talaria_cli::args::validate_print_options(&cli) {
                eprint!("talaria: {}\nNext step: {}\n", error.what, error.next_step);
                return ExitCode::from(2);
            }
            if cli.print {
                return ExitCode::from(talaria_cli::print::run_print(&cli, started));
            }
            match cli.into_launch() {
                Ok(launch) => talaria_cli::run::run(launch),
                Err(error) => {
                    let failure = talaria_app::failure::LaunchFailure::from(error);
                    eprint!("{}", failure.render());
                    ExitCode::from(failure.exit_code)
                }
            }
        }
        Err(error) => ExitCode::from(talaria_cli::args::write_usage(&error)),
    }
}
