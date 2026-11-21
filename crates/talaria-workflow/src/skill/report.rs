//! Profile resolution and installation report formatting.
use super::{
    self as skill, InstallAction, InstallReport, SKILL_NAME, SKILL_VERSION, SkillError, SkillTarget,
};
use std::path::Path;

const LOCAL_FOLDER_NOTE: &str = "using the local profile folder";

pub async fn resolve_target(profile: &str) -> Result<(SkillTarget, Vec<String>), SkillError> {
    resolve_target_in(profile, &hermes_client::default_home()).await
}

pub async fn resolve_target_in(
    profile: &str,
    hermes_home: &Path,
) -> Result<(SkillTarget, Vec<String>), SkillError> {
    let profile = skill::normalise_profile(profile)?;
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..Default::default()
    };
    match hermes_client::discover_with(hermes_home, &options).await {
        Ok(endpoint) => match skill::target_from_endpoint(&profile, &endpoint, hermes_home)? {
            Some(target) => Ok((target, vec![])),
            None => Ok((
                SkillTarget::resolve_local(&profile, hermes_home)?,
                vec![format!(
                    "the gateway does not serve profile {profile}; {LOCAL_FOLDER_NOTE}"
                )],
            )),
        },
        Err(
            hermes_client::DiscoveryError::NoGateway
            | hermes_client::DiscoveryError::NamedProfileHome
            | hermes_client::DiscoveryError::NotReady { .. },
        ) => Ok((
            SkillTarget::resolve_local(&profile, hermes_home)?,
            vec!["gateway not running; using the local profile folder".into()],
        )),
        Err(error) => Err(SkillError::Template(format!(
            "could not find the folder for profile {profile}: {error}"
        ))),
    }
}
pub fn report_text(
    target: &SkillTarget,
    report: &InstallReport,
    dry_run: bool,
    uninstall: bool,
) -> String {
    let mut lines = vec![];
    for warning in &report.warnings {
        if warning.ends_with(LOCAL_FOLDER_NOTE) {
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
