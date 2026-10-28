//! Roster output preserves local context when the gateway is unavailable.
use clap::Args;
use std::{path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::{
    hermes::{LinkWireFactory, WireFactory},
    roster_envelope::{self, LocalParts, RosterEnvelope, RosterErrorKind},
    runner,
    skill::{self, InstallRecords, SkillTarget},
    staffing::{
        self, Clock, LinkRosterSource, RosterCache, RosterError, RosterReport, RunBudget,
        StaffingTable, SystemClock,
    },
};
#[derive(Clone, Debug, Args)]
pub struct RosterArgs {
    #[arg(long, default_value = "default")]
    pub profile: String,
    #[arg(long)]
    pub staffing: Option<PathBuf>,
    #[arg(long,value_parser=clap::value_parser!(u64).range(1..))]
    pub budget_tokens: Option<u64>,
    #[arg(long,value_parser=money)]
    pub budget_usd: Option<f64>,
    #[arg(long)]
    pub json: bool,
}
fn money(raw: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "money budget must be finite and positive".into())
}
pub fn local_parts(
    home: Option<PathBuf>,
    hermes_home: &std::path::Path,
) -> (LocalParts, Option<(RosterErrorKind, String)>) {
    let mut local = LocalParts {
        talaria_home: home.clone(),
        runs_dir: None,
        workflows_dir: None,
        drafts_dir: None,
        workflows: vec![],
        skill: None,
        warnings: vec![],
    };
    let Some(home) = home else {
        return (
            local,
            Some((
                RosterErrorKind::HomeUnavailable,
                "no home folder for Talaria".into(),
            )),
        );
    };
    local.runs_dir = Some(runner::runs_dir(None));
    local.workflows_dir = Some(runner::workflows_dir(None));
    let error = match roster_envelope::drafts_dir(&home) {
        Ok(path) => {
            local.drafts_dir = Some(path);
            None
        }
        Err(e) => Some((
            RosterErrorKind::Filesystem,
            format!("cannot create {}: {e}", home.join("drafts").display()),
        )),
    };
    if let Some(dir) = &local.workflows_dir {
        let (workflows, warnings) = roster_envelope::list_workflows(dir);
        local.workflows = workflows;
        local.warnings = warnings;
    }
    let _ = hermes_home;
    (local, error)
}
fn inspect_skill(
    profile: &str,
    home: &std::path::Path,
    hermes_home: &std::path::Path,
) -> Result<skill::SkillInspection, String> {
    let target = SkillTarget::resolve_local(profile, hermes_home).map_err(|e| e.to_string())?;
    let records = InstallRecords::load(home).map_err(|e| e.to_string())?;
    let (command, root) = records
        .entry(&target.dir)
        .map(|e| (e.command.as_str(), e.hermes_root.as_path()))
        .unwrap_or(("talaria", &target.hermes_root));
    let rendered = skill::render(profile, command, root).map_err(|e| e.to_string())?;
    skill::inspect(&target, &records, &rendered).map_err(|e| e.to_string())
}
pub async fn envelope(args: &RosterArgs) -> (RosterEnvelope, i32) {
    let profile = match skill::normalise_profile(&args.profile) {
        Ok(p) => p,
        Err(e) => {
            let (local, _) = local_parts(None, &hermes_client::default_home());
            return roster_envelope::build_envelope(
                &args.profile,
                local,
                Err((RosterErrorKind::ProfileUnserved, e.to_string())),
            );
        }
    };
    let hermes_home = hermes_client::default_home();
    let (mut local, local_error) = local_parts(staffing::talaria_home(), &hermes_home);
    if let Some(home) = &local.talaria_home {
        match inspect_skill(&profile, home, &hermes_home) {
            Ok(s) => local.skill = Some(s),
            Err(e) => local.warnings.push(e),
        }
    }
    if let Some(error) = local_error {
        return roster_envelope::build_envelope(&profile, local, Err(error));
    }
    let result = async {
        let Some(home) = local.talaria_home.as_ref() else {
            return Err((
                RosterErrorKind::HomeUnavailable,
                "no home folder for Talaria".into(),
            ));
        };
        let table = StaffingTable::load(
            &args
                .staffing
                .clone()
                .unwrap_or_else(|| StaffingTable::default_path(home)),
        )
        .map_err(|e| (RosterErrorKind::StaffingInvalid, e.to_string()))?;
        let budget = RunBudget::resolve(args.budget_tokens, args.budget_usd, &table);
        let options = hermes_client::DiscoverOptions {
            ensure: hermes_client::EnsurePolicy::Never,
            ..Default::default()
        };
        hermes_client::discover_with(&hermes_home, &options)
            .await
            .map_err(|e| match e {
                hermes_client::DiscoveryError::NoGateway
                | hermes_client::DiscoveryError::NamedProfileHome
                | hermes_client::DiscoveryError::NotReady { .. } => (
                    RosterErrorKind::GatewayUnavailable,
                    format!("no Hermes gateway is running for profile {profile}"),
                ),
                _ => (RosterErrorKind::RosterFailed, e.to_string()),
            })?;
        let wires: Arc<dyn WireFactory> = Arc::new(LinkWireFactory::new(hermes_home));
        let clock = Arc::new(SystemClock);
        let cache = RosterCache::new(
            Arc::new(LinkRosterSource::new(wires)),
            clock.clone(),
            Duration::from_secs(30),
        );
        let roster = cache.load(&profile).await.map_err(|e| {
            let kind = if matches!(e, RosterError::NotServed(_)) {
                RosterErrorKind::ProfileUnserved
            } else {
                RosterErrorKind::RosterFailed
            };
            (kind, e.to_string())
        })?;
        Ok(RosterReport::build(&table, &roster, &budget, clock.now()))
    }
    .await;
    roster_envelope::build_envelope(&profile, local, result)
}
pub async fn execute(args: RosterArgs) -> u8 {
    if let Err(error) = skill::normalise_profile(&args.profile) {
        eprint!("{}", super::args::neutralise(&error.to_string()));
        return 2;
    }
    let (envelope, code) = envelope(&args).await;
    if args.json {
        match serde_json::to_string(&envelope) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprint!("{}", super::args::neutralise(&e.to_string()));
                return 1;
            }
        }
    } else {
        if let Some(report) = &envelope.report {
            print!("{}", report.to_text());
        }
        println!("Workflows:");
        for item in &envelope.workflows {
            println!(
                "  {}  {}",
                super::args::neutralise(&item.name).trim_end(),
                super::args::neutralise(&item.path.to_string_lossy()).trim_end()
            );
        }
        println!(
            "Skill: {}",
            envelope
                .skill
                .as_ref()
                .map_or("unknown", |s| s.state.as_str())
        );
        for warning in &envelope.warnings {
            eprint!("warning: {}", super::args::neutralise(warning));
        }
        if let Some(error) = envelope.error {
            eprint!("{}", super::args::neutralise(&error.message));
        }
    }
    code as u8
}
