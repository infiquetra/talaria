//! Embedded workflow instructions and conservative profile-local installation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};
pub mod install;
pub use install::*;
pub const SKILL_NAME: &str = "talaria-workflows";
pub const SKILL_VERSION: &str = "1.0.0";
pub const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../skill/talaria-workflows/SKILL.md"),
    ),
    (
        "references/commands.md",
        include_str!("../../skill/talaria-workflows/references/commands.md"),
    ),
    (
        "references/examples.md",
        include_str!("../../skill/talaria-workflows/references/examples.md"),
    ),
    (
        "references/script-api.md",
        include_str!("../../skill/talaria-workflows/references/script-api.md"),
    ),
];
#[derive(Clone, Debug)]
pub struct RenderedFile {
    pub path: &'static str,
    pub bytes: Vec<u8>,
}
pub fn normalise_profile(raw: &str) -> Result<String, SkillError> {
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(SkillError::InvalidProfile(raw.into()));
    }
    Ok(name)
}
fn validate_command(command: &str) -> Result<(), SkillError> {
    let bare = !command.is_empty()
        && command.len() <= 64
        && command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    let path = Path::new(command).is_absolute()
        && command.len() <= 1024
        && command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b));
    if bare || path {
        Ok(())
    } else {
        Err(SkillError::InvalidCommand(command.into()))
    }
}
fn validate_root(path: &Path) -> Result<&str, SkillError> {
    match path.to_str() {
        Some(s)
            if path.is_absolute()
                && s.len() <= 1024
                && !s.chars().any(|c| c == '\'' || c.is_control()) =>
        {
            Ok(s)
        }
        _ => Err(SkillError::InvalidRoot(path.into())),
    }
}
pub fn render(
    profile: &str,
    command: &str,
    hermes_root: &Path,
) -> Result<Vec<RenderedFile>, SkillError> {
    let profile = normalise_profile(profile)?;
    validate_command(command)?;
    let root = validate_root(hermes_root)?;
    let prefix = format!("HERMES_HOME='{root}' {command}");
    let mut total = 0;
    SKILL_FILES
        .iter()
        .map(|(path, template)| {
            let text = template
                .replace("@@TALARIA_PROFILE@@", &profile)
                .replace("@@TALARIA_COMMAND@@", &prefix)
                .replace("\r\n", "\n");
            if text.contains("@@") {
                return Err(SkillError::Template(format!(
                    "unresolved template in {path}"
                )));
            }
            total += text.len();
            if text.len() > if *path == "SKILL.md" { 8192 } else { 24576 } || total > 65536 {
                return Err(SkillError::Template(format!(
                    "embedded skill size limit exceeded in {path}"
                )));
            }
            Ok(RenderedFile {
                path,
                bytes: text.into_bytes(),
            })
        })
        .collect()
}
pub fn hermes_root_of(home: &Path, native_home: &Path) -> PathBuf {
    if home.starts_with(native_home) {
        native_home.into()
    } else if home
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|s| s == "profiles")
    {
        home.parent().and_then(Path::parent).unwrap_or(home).into()
    } else {
        home.into()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    Missing,
    Current,
    Outdated,
    Modified,
    Foreign,
}
impl SkillState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Current => "current",
            Self::Outdated => "outdated",
            Self::Modified => "modified",
            Self::Foreign => "foreign",
        }
    }
}
#[derive(Clone, Debug)]
pub struct SkillTarget {
    pub profile: String,
    pub home: PathBuf,
    pub hermes_root: PathBuf,
    pub dir: PathBuf,
    pub via_gateway: bool,
}
impl SkillTarget {
    pub fn resolve_local(profile: &str, hermes_home: &Path) -> Result<Self, SkillError> {
        let profile = normalise_profile(profile)?;
        let native = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".hermes");
        let root = hermes_root_of(hermes_home, &native);
        let home = if profile == "default" {
            root.clone()
        } else {
            root.join("profiles").join(&profile)
        };
        Self::make(profile, home, root, false)
    }
    pub fn from_profile_row(profile: &str, row: &serde_json::Value) -> Result<Self, SkillError> {
        let profile = normalise_profile(profile)?;
        let served = row["name"].as_str().unwrap_or_default();
        if served != profile {
            return Err(SkillError::GatewayMismatch {
                served: served.into(),
                wanted: profile,
            });
        }
        let home = row["path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| SkillError::Template("profile row has no folder".into()))?;
        let root = if profile == "default" {
            home.clone()
        } else if home
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|s| s == "profiles")
        {
            home.parent()
                .and_then(Path::parent)
                .ok_or_else(|| SkillError::InvalidRoot(home.clone()))?
                .into()
        } else {
            return Err(SkillError::Template(format!(
                "unexpected profile folder {}",
                home.display()
            )));
        };
        Self::make(profile, home, root, true)
    }
    fn make(
        profile: String,
        home: PathBuf,
        root: PathBuf,
        via_gateway: bool,
    ) -> Result<Self, SkillError> {
        validate_root(&root)?;
        if !home.is_dir() {
            return Err(SkillError::ProfileNotFound { profile, home });
        }
        let home = home.canonicalize().map_err(|e| io_error(&home, e))?;
        let root = root.canonicalize().map_err(|e| io_error(&root, e))?;
        let skills = home.join("skills");
        let skills = if skills.exists() {
            skills.canonicalize().map_err(|e| io_error(&skills, e))?
        } else {
            skills
        };
        Ok(Self {
            profile,
            home,
            hermes_root: root,
            dir: skills.join(SKILL_NAME),
            via_gateway,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillInspection {
    pub state: SkillState,
    pub installed_version: Option<String>,
    pub changed_files: Vec<String>,
}
pub fn inspect(
    target: &SkillTarget,
    records: &InstallRecords,
    rendered: &[RenderedFile],
) -> io::Result<SkillInspection> {
    let entry = records.entry(&target.dir);
    let installed_version = entry.map(|e| e.skill_version.clone());
    let meta = match fs::symlink_metadata(&target.dir) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(SkillInspection {
                state: SkillState::Missing,
                installed_version,
                changed_files: vec![],
            });
        }
        Err(e) => return Err(e),
    };
    if meta.file_type().is_symlink() || entry.is_none() {
        return Ok(SkillInspection {
            state: SkillState::Foreign,
            installed_version,
            changed_files: if meta.file_type().is_symlink() {
                vec!["link:.".into()]
            } else {
                vec![]
            },
        });
    }
    let (files, mut changes) = scan(&target.dir)?;
    let Some(entry) = entry else {
        return Err(io::Error::other("record changed during inspection"));
    };
    for (path, hash) in &files {
        match entry.files.get(path) {
            None => changes.push(format!("added:{path}")),
            Some(old) if old != hash => changes.push(format!("changed:{path}")),
            _ => {}
        }
    }
    for path in entry.files.keys() {
        if !files.contains_key(path)
            && !changes
                .iter()
                .any(|c| c.split_once(':').is_some_and(|(_, p)| p == path))
        {
            changes.push(format!("removed:{path}"));
        }
    }
    changes.sort_by(|a, b| {
        a.split_once(':')
            .map(|(_, p)| p)
            .cmp(&b.split_once(':').map(|(_, p)| p))
    });
    let state = if !changes.is_empty() {
        SkillState::Modified
    } else if entry.files == render_hashes(rendered) {
        SkillState::Current
    } else {
        SkillState::Outdated
    };
    Ok(SkillInspection {
        state,
        installed_version,
        changed_files: changes,
    })
}
pub(crate) fn render_hashes(files: &[RenderedFile]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|f| (f.path.into(), hex::encode(Sha256::digest(&f.bytes))))
        .collect()
}
pub(crate) fn scan(path: &Path) -> io::Result<(BTreeMap<String, String>, Vec<String>)> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        special: &mut Vec<String>,
    ) -> io::Result<()> {
        for item in fs::read_dir(path)? {
            let p = item?.path();
            let meta = fs::symlink_metadata(&p)?;
            let name = p
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_str()
                .ok_or_else(|| io::Error::other("skill filename is not UTF-8"))?
                .replace('\\', "/");
            if meta.file_type().is_symlink() {
                special.push(format!("link:{name}"));
            } else if meta.is_dir() {
                visit(root, &p, files, special)?;
            } else if meta.is_file() {
                files.insert(name, hex::encode(Sha256::digest(fs::read(&p)?)));
            } else {
                special.push(format!("other:{name}"));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    let mut special = vec![];
    if fs::symlink_metadata(path)?.is_dir() {
        visit(path, path, &mut files, &mut special)?;
    } else {
        special.push("other:.".into());
    }
    Ok((files, special))
}
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("invalid profile name '{0}'")]
    InvalidProfile(String),
    #[error("invalid command '{0}'")]
    InvalidCommand(String),
    #[error("Hermes root '{0}' cannot be written into the skill")]
    InvalidRoot(PathBuf),
    #[error("profile '{profile}' not found ({home})")]
    ProfileNotFound { profile: String, home: PathBuf },
    #[error("gateway serves '{served}', not '{wanted}'")]
    GatewayMismatch { served: String, wanted: String },
    #[error("another talaria skill command is changing profile {profile}; try again")]
    LockTimeout { profile: String },
    #[error("cannot write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{message}")]
    Record { path: PathBuf, message: String },
    #[error("{0}")]
    Template(String),
}
impl SkillError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidProfile(_)
            | Self::InvalidCommand(_)
            | Self::InvalidRoot(_)
            | Self::ProfileNotFound { .. } => 2,
            _ => 1,
        }
    }
}
pub(crate) fn io_error(path: &Path, source: io::Error) -> SkillError {
    SkillError::Io {
        path: path.into(),
        source,
    }
}
