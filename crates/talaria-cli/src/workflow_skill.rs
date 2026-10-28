//! Explicit workflow-skill installation commands; discovery never starts a gateway.
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::time::Duration;
use talaria_workflow::{
    hermes::{LinkWireFactory, WireError, WireFactory},
    skill::{
        self, InstallAction, InstallOptions, InstallRecords, InstallReport, SKILL_NAME,
        SKILL_VERSION, SkillError, SkillTarget,
    },
};
#[derive(Clone, Debug, Args)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: SkillCommand,
}
#[derive(Clone, Debug, Subcommand)]
pub enum SkillCommand {
    Install(InstallArgs),
    Status(StatusArgs),
    Uninstall(UninstallArgs),
}
#[derive(Clone, Debug, Args)]
pub struct InstallArgs {
    #[arg(long, default_value = "default")]
    pub profile: String,
    #[arg(long, default_value = "talaria")]
    pub command: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub json: bool,
}
#[derive(Clone, Debug, Args)]
pub struct StatusArgs {
    #[arg(long, default_value = "default")]
    pub profile: String,
    #[arg(long)]
    pub json: bool,
}
#[derive(Clone, Debug, Args)]
pub struct UninstallArgs {
    #[arg(long, default_value = "default")]
    pub profile: String,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub json: bool,
}
pub async fn resolve_target(profile: &str) -> Result<(SkillTarget, Vec<String>), SkillError> {
    let profile = skill::normalise_profile(profile)?;
    let home = hermes_client::default_home();
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..Default::default()
    };
    match hermes_client::discover_with(&home, &options).await {
        Ok(_) => {
            let factory = LinkWireFactory::new(home.clone());
            match factory.open(&profile).await {
                Ok(wire) => {
                    let result = wire
                        .call(
                            "profiles.list",
                            serde_json::Map::from_iter([(
                                "include_sessions".into(),
                                Value::Bool(false),
                            )]),
                            Duration::from_secs(5),
                        )
                        .await
                        .map_err(|e| SkillError::Template(e.to_string()))?;
                    let row = result["profiles"]
                        .as_array()
                        .and_then(|p| p.first())
                        .ok_or_else(|| {
                            SkillError::Template("gateway returned no profile row".into())
                        })?;
                    return Ok((SkillTarget::from_profile_row(&profile, row)?, vec![]));
                }
                Err(WireError::NotServed(_)) => {}
                Err(e) => return Err(SkillError::Template(e.to_string())),
            }
        }
        Err(
            hermes_client::DiscoveryError::NoGateway
            | hermes_client::DiscoveryError::NamedProfileHome
            | hermes_client::DiscoveryError::NotReady { .. },
        ) => {}
        Err(e) => return Err(SkillError::Template(e.to_string())),
    }
    Ok((
        SkillTarget::resolve_local(&profile, &home)?,
        vec!["gateway not running; using the local profile folder".into()],
    ))
}
pub fn report_json(target: &SkillTarget, report: &InstallReport) -> Value {
    json!({"version":1,"profile":target.profile,"state":report.before.state,"skill_version_installed":report.before.installed_version,"skill_version_embedded":SKILL_VERSION,"target":target.dir,"action":report.action,"changed_files":report.before.changed_files,"backup":report.backup,"warnings":report.warnings})
}
pub fn report_text(
    target: &SkillTarget,
    report: &InstallReport,
    dry_run: bool,
    uninstall: bool,
) -> String {
    let mut lines = vec![];
    for warning in &report.warnings {
        if warning == "gateway not running; using the local profile folder" {
            lines.push(warning.clone());
        } else {
            lines.push(format!("warning: {warning}"));
        }
    }
    let state = report.before.state.as_str();
    if dry_run {
        lines.extend([
            format!("state: {state}"),
            format!("target: {}", target.dir.display()),
            format!(
                "action: {}",
                serde_json::to_value(report.action)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            ),
        ]);
    } else {
        match report.action {
            InstallAction::Refuse => {
                let verb = if uninstall { "removing" } else { "replacing" };
                lines.push(format!("{SKILL_NAME} in profile {} was changed outside Talaria ({state}); not {verb} it.",target.profile));
                if state == "foreign" {
                    lines.push("  no install record".into());
                } else {
                    for change in &report.before.changed_files {
                        let (kind, path) = change.split_once(':').unwrap_or(("changed", change));
                        lines.push(format!("  {kind}: {path}"));
                    }
                }
                lines.push(format!(
                    "Use --force to back it up and {} it.",
                    if uninstall { "remove" } else { "replace" }
                ));
            }
            InstallAction::None => lines.push(if uninstall {
                "not installed".into()
            } else {
                format!(
                    "{SKILL_NAME} {SKILL_VERSION} is already current in profile {}",
                    target.profile
                )
            }),
            InstallAction::Remove | InstallAction::BackupAndRemove => {
                if let Some(backup) = &report.backup {
                    lines.push(format!(
                        "backed up the old {SKILL_NAME} to {}",
                        backup.display()
                    ));
                }
                lines.push(format!(
                    "removed {SKILL_NAME} from profile {}",
                    target.profile
                ));
            }
            _ => {
                if let Some(backup) = &report.backup {
                    lines.push(format!(
                        "backed up the old {SKILL_NAME} to {}",
                        backup.display()
                    ));
                }
                if report.action == InstallAction::Replace {
                    lines.push(format!(
                        "updated {SKILL_NAME} {} -> {SKILL_VERSION} in profile {}",
                        report
                            .before
                            .installed_version
                            .as_deref()
                            .unwrap_or("unknown"),
                        target.profile
                    ));
                } else {
                    lines.push(format!(
                        "installed {SKILL_NAME} {SKILL_VERSION} into profile {}",
                        target.profile
                    ));
                }
                lines.push(format!("  {}", target.dir.display()));
            }
        }
    }
    lines.join("\n") + "\n"
}
pub async fn execute(args: SkillArgs) -> u8 {
    let (profile, command, force, dry_run, json_mode, uninstall, status) = match args.command {
        SkillCommand::Install(a) => (
            a.profile, a.command, a.force, a.dry_run, a.json, false, false,
        ),
        SkillCommand::Status(a) => (
            a.profile,
            "talaria".into(),
            false,
            true,
            a.json,
            false,
            true,
        ),
        SkillCommand::Uninstall(a) => (
            a.profile,
            "talaria".into(),
            a.force,
            false,
            a.json,
            true,
            false,
        ),
    };
    let run = async {
        let profile = skill::normalise_profile(&profile)?;
        skill::render(&profile, &command, std::path::Path::new("/"))?;
        let (target, warnings) = resolve_target(&profile).await?;
        let home = talaria_workflow::staffing::talaria_home()
            .ok_or_else(|| SkillError::Template("no home folder for Talaria".into()))?;
        let opts = InstallOptions {
            command,
            force,
            dry_run,
            talaria_home: home.clone(),
            now: chrono::Utc::now(),
            lock_wait: Duration::from_secs(10),
        };
        let mut report = if status {
            let records = InstallRecords::load(&home)?;
            let (command, root) = records
                .entry(&target.dir)
                .map(|e| (e.command.as_str(), e.hermes_root.as_path()))
                .unwrap_or(("talaria", &target.hermes_root));
            let rendered = skill::render(&profile, command, root)?;
            let before =
                skill::inspect(&target, &records, &rendered).map_err(|source| SkillError::Io {
                    path: target.dir.clone(),
                    source,
                })?;
            InstallReport {
                before,
                action: InstallAction::None,
                backup: None,
                warnings: vec![],
            }
        } else {
            let owned_target = target.clone();
            tokio::task::spawn_blocking(move || {
                if uninstall {
                    skill::uninstall(&owned_target, &opts)
                } else {
                    skill::install(&owned_target, &opts)
                }
            })
            .await
            .map_err(|e| SkillError::Template(e.to_string()))??
        };
        report.warnings.splice(0..0, warnings);
        if json_mode {
            println!("{}", report_json(&target, &report));
        } else if status {
            for warning in &report.warnings {
                eprintln!("{}", super::args::neutralise(warning).trim_end());
            }
            println!(
                "state: {}\ninstalled version: {}\nembedded version: {SKILL_VERSION}\ntarget: {}",
                report.before.state.as_str(),
                report.before.installed_version.as_deref().unwrap_or("-"),
                target.dir.display()
            );
            for change in &report.before.changed_files {
                println!("  {change}");
            }
        } else {
            for line in report_text(&target, &report, dry_run, uninstall).lines() {
                print!("{}", super::args::neutralise(line));
            }
        }
        Ok::<u8, SkillError>(if report.action == InstallAction::Refuse {
            1
        } else {
            0
        })
    }
    .await;
    match run {
        Ok(code) => code,
        Err(e) => {
            eprint!("{}", super::args::neutralise(&e.to_string()));
            e.exit_code() as u8
        }
    }
}
