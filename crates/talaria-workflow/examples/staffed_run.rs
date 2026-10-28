//! Compose staffing, durable execution and journal-backed scheduling.
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use talaria_workflow::{
    engine::*,
    hermes::{
        ExplicitLaunch, HermesPort, LaunchDefaults, LinkWireFactory, PortConfig, WireFactory,
    },
    journal::*,
    schedule::*,
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
    let mut o = BTreeMap::new();
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "staffed_run [--profile PROFILE] [--staffing FILE] [--budget-tokens N] [--budget-usd USD] [--json] (--script FILE --run-dir PARENT | --resume RUN_FOLDER)"
            );
            return Ok(0);
        }
        if key == "--json" {
            continue;
        }
        if ![
            "--profile",
            "--staffing",
            "--budget-tokens",
            "--budget-usd",
            "--script",
            "--run-dir",
            "--resume",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("unknown option {key}").into());
        }
        let value = args.next().ok_or("missing option value")?;
        if o.insert(key, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    if o.contains_key("--resume") && (o.contains_key("--script") || o.contains_key("--run-dir")) {
        return Err("choose --resume or --script with --run-dir".into());
    }
    let old = o
        .get("--resume")
        .map(|p| RunDir::open(Path::new(p)))
        .transpose()?;
    let profile = o
        .get("--profile")
        .map(String::as_str)
        .or_else(|| old.as_ref().map(|d| d.manifest().profile.as_str()))
        .unwrap_or("default")
        .to_owned();
    if old
        .as_ref()
        .is_some_and(|d| d.manifest().profile != profile)
    {
        return Err("resume must use the run's profile".into());
    }
    let path = o
        .get("--staffing")
        .map(PathBuf::from)
        .or_else(|| talaria_home().map(|h| StaffingTable::default_path(&h)))
        .ok_or("no Talaria home; use --staffing")?;
    let table = StaffingTable::load(&path)?;
    let tokens = o
        .get("--budget-tokens")
        .map(|s| s.parse::<u64>())
        .transpose()?;
    let money = o
        .get("--budget-usd")
        .map(|s| s.parse::<f64>())
        .transpose()?;
    if tokens == Some(0) || money.is_some_and(|m| !m.is_finite() || m <= 0.0) {
        return Err("budgets must be positive and finite".into());
    }
    let budget = RunBudget::resolve(tokens, money, &table);
    let clock = Arc::new(SystemClock);
    let cfg = PortConfig::default();
    let wires: Arc<dyn WireFactory> = Arc::new(LinkWireFactory::new(cfg.home.clone()));
    let cache = RosterCache::new(
        Arc::new(LinkRosterSource::new(wires.clone())),
        clock.clone(),
        Duration::from_secs(30),
    );
    let roster = cache.load(&profile).await?;
    let check = check_tiers(&table, &roster, &budget, clock.now());
    for w in &check.warnings {
        eprintln!("{w}");
    }
    if !check.is_ok() {
        for p in &check.problems {
            eprintln!("{p}");
        }
        return Ok(2);
    }
    if let Some(d) = &old {
        for drift in staffing_drift(&d.manifest().roster, &table) {
            eprintln!("{drift}");
        }
    }
    let source = if let Some(d) = &old {
        std::fs::read_to_string(
            d.path()
                .join("scripts")
                .join(format!("{}.js", d.manifest().workflow.script_sha256)),
        )?
    } else {
        std::fs::read_to_string(o.get("--script").ok_or("missing --script")?)?
    };
    let parsed = parse_script(&source)?;
    let snapshot = roster.snapshot_with(&table, &budget);
    let dir = if let Some(d) = old {
        d
    } else {
        let parent = Path::new(o.get("--run-dir").ok_or("missing --run-dir")?);
        std::fs::create_dir_all(parent)?;
        RunDir::create(
            parent,
            NewManifest {
                workflow_name: parsed.meta.name.clone(),
                args: None,
                started_at_ms: clock.now().timestamp_millis(),
                seed: 42,
                profile: profile.clone(),
                on_unknown: OnUnknown::Wait,
                talaria_version: env!("CARGO_PKG_VERSION").into(),
                hermes: json!({}),
                roster: snapshot,
                approval_policy: json!({"finite":true}),
            },
            &source,
        )?
    };
    let prices = PriceBook::from_snapshot(&dir.manifest().roster);
    let (journal, _, warnings) = RunJournal::open(dir.clone(), &source).await?;
    for w in warnings {
        eprintln!("warning: {w:?}");
    }
    let journal = Arc::new(journal);
    let (scheduler, mut events) = Scheduler::start(
        SchedulerConfig::from_table(&table, budget, &profile, prices),
        cache,
        dir.path().join("journal.jsonl"),
        clock.clone(),
    )
    .await?;
    let launch = StaffedLaunch::new(
        StaffingResolver::new(Arc::new(table), roster).with_path(path),
        ExplicitLaunch::new(LaunchDefaults {
            profile: profile.clone(),
            cwd: std::env::current_dir()?.canonicalize()?,
            toolsets: Some(vec!["file".into()]),
            worktrees: None,
        }),
    );
    let config = journal.engine_config(&launch)?;
    let (port, feed) = HermesPort::with_wires(cfg, wires, &profile).await?;
    let feeds = tokio::spawn(async move {
        let mut feed = feed;
        while feed.recv().await.is_some() {}
    });
    let backend = Arc::new(staffed_backend(
        journal.clone(),
        port,
        launch,
        scheduler.clone(),
        RetryPolicy::default(),
    ));
    let handle = spawn(parsed, config, backend, Arc::new(MapResolver::new()));
    let forward = journal.forward_events(handle.events);
    let mut outcome = handle.outcome;
    let result = loop {
        tokio::select! {e=events.0.recv()=>{if let Some(e)=e{println!("{}",serde_json::to_string(&e)?);}},r=&mut outcome=>break r?}
    };
    forward.await??;
    let script_result = match &result {
        RunOutcome::Completed { result } => result.clone(),
        RunOutcome::Failed { error } => json!({"error":error}),
        RunOutcome::Aborted { reason } => json!({"reason":reason}),
    };
    let kind = match result {
        RunOutcome::Completed { .. } => RunOutcomeKind::Completed,
        RunOutcome::Failed { .. } => RunOutcomeKind::Failed,
        RunOutcome::Aborted { .. } => RunOutcomeKind::Aborted,
    };
    journal.finish(kind.clone()).await?;
    let _ = scheduler.budget();
    while let Ok(e) = events.0.try_recv() {
        println!("{}", serde_json::to_string(&e)?);
    }
    let spend = scheduler.status().spend;
    println!(
        "{}",
        json!({"event":"finished","at":clock.now().to_rfc3339_opts(chrono::SecondsFormat::AutoSi,true),"outcome":kind,"spend":spend,"money_text":spend.money_text(),"result":script_result,"run_dir":dir.path()})
    );
    feeds.abort();
    Ok(if kind == RunOutcomeKind::Completed {
        0
    } else {
        1
    })
}
