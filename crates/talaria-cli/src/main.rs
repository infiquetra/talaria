use std::process::ExitCode;

use clap::Parser;

mod bench_replay;

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
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
    match talaria_cli::args::Cli::try_parse() {
        Ok(cli) => {
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
