//! Embedded workflow instructions and conservative profile-local installation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};
pub mod install;
pub mod report;
pub use install::*;
pub use report::{report_text, resolve_target, resolve_target_in};
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
const PROFILE_TOKEN: &str = "@@TALARIA_PROFILE@@";
const COMMAND_TOKEN: &str = "@@TALARIA_COMMAND@@";
/// What the size budget counts for each `@@TALARIA_COMMAND@@`: room for
/// `HERMES_HOME='<folder>' <command>` with a folder of about 45 bytes. A real folder path is
/// not measured, so the budget gives the same answer on every machine.
pub const COMMAND_ALLOWANCE: usize = 64;
/// What the size budget counts for each `@@TALARIA_PROFILE@@`.
pub const PROFILE_ALLOWANCE: usize = 24;
/// The budget for `SKILL.md`, counted on its template with the allowances above.
pub const SKILL_BUDGET: usize = 8192;
/// The budget for each reference file, counted the same way.
pub const REFERENCE_BUDGET: usize = 24576;
/// The budget for all files together, counted the same way.
pub const TOTAL_BUDGET: usize = 65536;
/// How far the rendered text may pass a budget before `render` refuses. It is what a long Hermes
/// folder path may add, so that path length alone rarely decides whether a skill installs.
pub const RENDER_HEADROOM: usize = 4096;
/// The budget for the file at `path`.
pub fn file_budget(path: &str) -> usize {
    if path == "SKILL.md" {
        SKILL_BUDGET
    } else {
        REFERENCE_BUDGET
    }
}
/// The size a template counts toward its budget: its own length, with every placeholder counted
/// at its fixed allowance instead of the text it will be replaced by.
pub fn budgeted_size(template: &str) -> usize {
    let template = template.replace("\r\n", "\n");
    let commands = template.matches(COMMAND_TOKEN).count();
    let profiles = template.matches(PROFILE_TOKEN).count();
    template.len() - commands * COMMAND_TOKEN.len() - profiles * PROFILE_TOKEN.len()
        + commands * COMMAND_ALLOWANCE
        + profiles * PROFILE_ALLOWANCE
}
/// Render the skill files.
///
/// Two size rules apply. First, each template must fit its budget when counted by
/// [`budgeted_size`]; that depends on the embedded text only, so it cannot pass on one machine
/// and fail on another. Second, the rendered text may exceed its budget by at most
/// [`RENDER_HEADROOM`] bytes; a Hermes folder path long enough to pass that is refused with an
/// error that names the path length.
pub fn render(
    profile: &str,
    command: &str,
    hermes_root: &Path,
) -> Result<Vec<RenderedFile>, SkillError> {
    let profile = normalise_profile(profile)?;
    validate_command(command)?;
    let root = validate_root(hermes_root)?;
    let prefix = format!("HERMES_HOME='{root}' {command}");
    for (path, template) in SKILL_FILES {
        let counted = budgeted_size(template);
        if counted > file_budget(path) {
            return Err(SkillError::Template(format!(
                "embedded skill size limit exceeded in {path}: {counted} bytes counting each placeholder at its allowance, limit {}",
                file_budget(path)
            )));
        }
    }
    let counted_total: usize = SKILL_FILES.iter().map(|(_, t)| budgeted_size(t)).sum();
    if counted_total > TOTAL_BUDGET {
        return Err(SkillError::Template(format!(
            "embedded skill size limit exceeded: {counted_total} bytes in all files counting each placeholder at its allowance, limit {TOTAL_BUDGET}"
        )));
    }
    let too_long = |what: &str, size: usize, limit: usize| {
        SkillError::Template(format!(
            "{what} renders to {size} bytes, over the limit of {limit}; the cause is the Hermes folder path ({} bytes) and command ({} bytes) written into every command line, so use a shorter Hermes folder path",
            root.len(),
            command.len()
        ))
    };
    let mut total = 0;
    SKILL_FILES
        .iter()
        .map(|(path, template)| {
            let text = template
                .replace(PROFILE_TOKEN, &profile)
                .replace(COMMAND_TOKEN, &prefix)
                .replace("\r\n", "\n");
            if text.contains("@@") {
                return Err(SkillError::Template(format!(
                    "unresolved template in {path}"
                )));
            }
            total += text.len();
            let limit = file_budget(path) + RENDER_HEADROOM;
            if text.len() > limit {
                return Err(too_long(path, text.len(), limit));
            }
            if total > TOTAL_BUDGET + RENDER_HEADROOM {
                return Err(too_long(
                    "the skill files together",
                    total,
                    TOTAL_BUDGET + RENDER_HEADROOM,
                ));
            }
            Ok(RenderedFile {
                path,
                bytes: text.into_bytes(),
            })
        })
        .collect()
}
/// Confirm with the gateway's served-profile entry that `profile` is served from the folder the
/// local rule gives under `hermes_home`. `Ok(None)` means the gateway does not serve the profile.
pub fn target_from_endpoint(
    profile: &str,
    endpoint: &hermes_client::Endpoint,
    hermes_home: &Path,
) -> Result<Option<SkillTarget>, SkillError> {
    let profile = normalise_profile(profile)?;
    let Some(reported) = endpoint.profile_home(&profile) else {
        return Ok(None);
    };
    let expected = SkillTarget::resolve_local(&profile, hermes_home)?;
    let reported = reported.as_path();
    let same_folder = reported.is_absolute()
        && reported
            .canonicalize()
            .is_ok_and(|reported| reported == expected.home);
    if !same_folder {
        return Err(SkillError::GatewayFolderMismatch { profile });
    }
    Ok(Some(SkillTarget {
        via_gateway: true,
        ..expected
    }))
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
/// One digest over every file in an installed skill folder, so one change can be told from the
/// next.
pub fn installed_digest(dir: &Path) -> io::Result<String> {
    let (files, special) = scan(dir)?;
    let mut hasher = Sha256::new();
    for (path, hash) in &files {
        hasher.update(format!("{path}\0{hash}\n"));
    }
    for entry in &special {
        hasher.update(format!("{entry}\n"));
    }
    Ok(hex::encode(hasher.finalize()))
}
/// True when the folder holds exactly the `rendered` files, compared by hash.
pub fn same_files(dir: &Path, rendered: &[RenderedFile]) -> io::Result<bool> {
    let (files, special) = scan(dir)?;
    Ok(special.is_empty() && files == render_hashes(rendered))
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
    #[error(
        "the gateway serves profile {profile} from a different folder than this Hermes home gives; restart the gateway, then try again"
    )]
    GatewayFolderMismatch { profile: String },
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
