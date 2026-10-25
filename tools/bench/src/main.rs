//! `talaria-bench generate | generate-queue | run`.

use std::path::PathBuf;
use std::process::ExitCode;
use talaria_bench::{BenchError, Budgets, RunOptions};

fn command() -> Result<u8, BenchError> {
    let mut args = std::env::args().skip(1);
    let verb = args.next().unwrap_or_default();
    if !["run", "generate", "generate-queue"].contains(&verb.as_str()) {
        return Err(BenchError::BudgetsInvalid(
            "expected generate, generate-queue or run".into(),
        ));
    }
    let mut binary = PathBuf::from("target/release/talaria");
    let mut fixtures = PathBuf::from("fixtures/frames");
    let mut budgets = PathBuf::from("tools/bench/budgets.toml");
    let mut out = None;
    let mut report = None;
    let mut attempts = 3;
    let mut idle = None;
    let mut grace = 10;
    let mut binary_args = Vec::new();
    let mut extra = Vec::new();
    let mut scenarios = None;
    let mut only = Vec::new();
    while let Some(key) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| BenchError::BudgetsInvalid("missing option value".into()))?;
        let number = || {
            value
                .parse::<u32>()
                .map_err(|_| BenchError::BudgetsInvalid("invalid option number".into()))
        };
        match key.as_str() {
            "--binary" => binary = value.into(),
            "--fixtures" => fixtures = value.into(),
            "--budgets" => budgets = value.into(),
            "--out" => out = Some(PathBuf::from(value)),
            "--report" => report = Some(PathBuf::from(value)),
            "--attempts" => attempts = number()?,
            "--idle-seconds" => idle = Some(number()?),
            "--report-grace-seconds" => grace = number()?,
            "--binary-arg" => binary_args.push(value),
            "--extra-fixture" => extra.push(PathBuf::from(value)),
            "--scenario-dir" => scenarios = Some(PathBuf::from(value)),
            "--only" => only.push(value),
            _ => return Err(BenchError::BudgetsInvalid("unknown option".into())),
        }
    }
    let budgets = Budgets::load(&budgets)?;
    if verb == "generate" {
        talaria_bench::generate(
            &fixtures,
            &out.ok_or_else(|| BenchError::BudgetsInvalid("generate needs --out".into()))?,
            &budgets,
        )?;
        return Ok(0);
    }
    if verb == "generate-queue" {
        talaria_bench::generate_queue(
            &fixtures,
            &out.ok_or_else(|| BenchError::BudgetsInvalid("generate-queue needs --out".into()))?,
            &budgets,
        )?;
        return Ok(0);
    }
    let mut options = RunOptions::new(binary, budgets);
    options.fixtures = fixtures;
    options.report = report;
    options.attempts = attempts;
    options.idle_seconds = idle;
    options.report_grace_seconds = grace;
    options.binary_args = binary_args;
    options.extra_fixtures = extra;
    options.scenario_dir = scenarios;
    options.only = only;
    let report = talaria_bench::run(&options)?;
    print!("{}", report.render());
    Ok(talaria_bench::exit_code(&report))
}
fn main() -> ExitCode {
    match command() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
