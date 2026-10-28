//! Read a profile-bound roster without changing Hermes configuration.
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::{
    hermes::{LinkWireFactory, PortConfig},
    staffing::*,
};
#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .init();
    let code = match run().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    };
    std::process::exit(code);
}
async fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut options = BTreeMap::new();
    let mut json = false;
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "roster [--profile PROFILE] [--staffing FILE] [--budget-tokens N] [--budget-usd USD] [--json]"
            );
            return Ok(0);
        }
        if key == "--json" {
            json = true;
            continue;
        }
        if !["--profile", "--staffing", "--budget-tokens", "--budget-usd"].contains(&key.as_str()) {
            return Err(format!("unknown option {key}").into());
        }
        let value = args.next().ok_or("missing option value")?;
        if options.insert(key, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    let profile = options.get("--profile").map_or("default", String::as_str);
    let path = options
        .get("--staffing")
        .map(PathBuf::from)
        .or_else(|| talaria_home().map(|h| StaffingTable::default_path(&h)))
        .ok_or("no Talaria home; use --staffing")?;
    let table = StaffingTable::load(&path)?;
    let tokens = options
        .get("--budget-tokens")
        .map(|s| s.parse::<u64>())
        .transpose()?;
    let money = options
        .get("--budget-usd")
        .map(|s| s.parse::<f64>())
        .transpose()?;
    if tokens == Some(0) || money.is_some_and(|m| !m.is_finite() || m <= 0.0) {
        return Err("budgets must be positive and finite".into());
    }
    let budget = RunBudget::resolve(tokens, money, &table);
    let clock = Arc::new(SystemClock);
    let cache = RosterCache::new(
        Arc::new(LinkRosterSource::new(Arc::new(LinkWireFactory::new(
            PortConfig::default().home,
        )))),
        clock.clone(),
        Duration::from_secs(30),
    );
    let roster = cache.load(profile).await?;
    let report = RosterReport::build(&table, &roster, &budget, clock.now());
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.to_text());
    }
    for problem in &report.check.problems {
        eprintln!("{problem}");
    }
    for warning in &report.check.warnings {
        eprintln!("{warning}");
    }
    Ok(if report.check.is_ok() { 0 } else { 2 })
}
