//! Gateway-independent roster context, with an explicit failure envelope.
use crate::{engine::parse_script, skill::SkillInspection, staffing::RosterReport};
use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize)]
pub struct WorkflowEntry {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
}
pub fn list_workflows(dir: &Path) -> (Vec<WorkflowEntry>, Vec<String>) {
    let rows = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (vec![], vec![]),
        Err(e) => return (vec![], vec![format!("cannot read workflows: {e}")]),
    };
    let mut paths = vec![];
    let mut warnings = vec![];
    for row in rows {
        match row {
            Ok(row) if row.path().extension().is_some_and(|e| e == "js") => paths.push(row.path()),
            Ok(_) => {}
            Err(e) => warnings.push(format!("cannot read workflow entry: {e}")),
        }
    }
    paths.sort();
    let mut workflows = vec![];
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            warnings.push("workflow filename is not UTF-8".into());
            continue;
        };
        let name = stem.to_owned();
        let description = (|| -> Result<String, String> {
            let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
            if !meta.is_file() {
                return Err("not a regular script file".into());
            }
            if meta.len() > 1_048_576 {
                return Err("script exceeds 1 MiB".into());
            }
            let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            parse_script(&source)
                .map(|s| s.meta.description)
                .map_err(|e| e.to_string())
        })();
        let description = match description {
            Ok(s) => Some(s),
            Err(e) => {
                warnings.push(format!(
                    "workflows/{}: {e}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                None
            }
        };
        workflows.push(WorkflowEntry {
            name,
            path,
            description,
        });
    }
    (workflows, warnings)
}
pub fn drafts_dir(home: &Path) -> io::Result<PathBuf> {
    let path = home.join("drafts");
    super::skill::install::private_dir(&path)?;
    Ok(path)
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RosterErrorKind {
    HomeUnavailable,
    Filesystem,
    StaffingInvalid,
    GatewayUnavailable,
    ProfileUnserved,
    RosterFailed,
}
pub struct LocalParts {
    pub talaria_home: Option<PathBuf>,
    pub runs_dir: Option<PathBuf>,
    pub workflows_dir: Option<PathBuf>,
    pub drafts_dir: Option<PathBuf>,
    pub workflows: Vec<WorkflowEntry>,
    pub skill: Option<SkillInspection>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct EnvelopePaths {
    pub talaria_home: Option<PathBuf>,
    pub runs_dir: Option<PathBuf>,
    pub workflows_dir: Option<PathBuf>,
    pub drafts_dir: Option<PathBuf>,
}
#[derive(Debug, Serialize)]
pub struct EnvelopeSkill {
    pub state: String,
    pub skill_version_installed: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct EnvelopeError {
    pub kind: RosterErrorKind,
    pub message: String,
}
#[derive(Serialize)]
pub struct RosterEnvelope {
    pub version: u32,
    pub profile: String,
    pub report: Option<RosterReport>,
    pub paths: EnvelopePaths,
    pub workflows: Vec<WorkflowEntry>,
    pub skill: Option<EnvelopeSkill>,
    pub warnings: Vec<String>,
    pub error: Option<EnvelopeError>,
}
pub fn build_envelope(
    profile: &str,
    local: LocalParts,
    report: Result<RosterReport, (RosterErrorKind, String)>,
) -> (RosterEnvelope, i32) {
    let skill = local
        .skill
        .map(|s| EnvelopeSkill {
            state: s.state.as_str().into(),
            skill_version_installed: s.installed_version,
        })
        .or_else(|| {
            local.talaria_home.as_ref().map(|_| EnvelopeSkill {
                state: "unknown".into(),
                skill_version_installed: None,
            })
        });
    let (report, error, code) = match report {
        Ok(report) => {
            let code = if report.check.is_ok() { 0 } else { 2 };
            (Some(report), None, code)
        }
        Err((kind, message)) => (None, Some(EnvelopeError { kind, message }), 1),
    };
    (
        RosterEnvelope {
            version: 1,
            profile: profile.into(),
            report,
            paths: EnvelopePaths {
                talaria_home: local.talaria_home,
                runs_dir: local.runs_dir,
                workflows_dir: local.workflows_dir,
                drafts_dir: local.drafts_dir,
            },
            workflows: local.workflows,
            skill,
            warnings: local.warnings,
            error,
        },
        code,
    )
}
