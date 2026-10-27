use std::process::ExitCode;

use clap::Parser;

mod bench_replay;

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if talaria_cli::args::is_doctor_command(&args) {
        let values = std::iter::once("talaria doctor").chain(args[1..].iter().map(String::as_str));
        let cli = match talaria_cli::args::DoctorCli::try_parse_from(values) {
            Ok(cli) => cli,
            Err(error) => {
                let code = if error.use_stderr() { 2 } else { 0 };
                let _ = error.print();
                return ExitCode::from(code);
            }
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let code = runtime.block_on(talaria_cli::doctor_cmd::run_doctor(
            cli.profile.as_deref(),
            cli.doctor,
        ));
        return ExitCode::from(u8::try_from(code).unwrap_or(99));
    }
    if talaria_cli::args::is_attach_command(&args) {
        return talaria_cli::attach_cmd::run(&args);
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
