//! Locked, recoverable folder publication and the durable install record.
use super::{
    SKILL_NAME, SKILL_VERSION, SkillError, SkillInspection, SkillState, SkillTarget, io_error,
    render, render_hashes, scan,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallRecords {
    pub version: u32,
    pub installs: Vec<InstallEntry>,
    #[serde(skip)]
    raw: serde_json::Value,
}
impl Default for InstallRecords {
    fn default() -> Self {
        Self {
            version: 1,
            installs: vec![],
            raw: serde_json::Value::Null,
        }
    }
}
impl InstallRecords {
    pub fn load(home: &Path) -> Result<Self, SkillError> {
        let path = home.join("skills/installs.json");
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(record_error(&path, "read", e)),
        };
        let raw: serde_json::Value =
            serde_json::from_slice(&data).map_err(|e| record_error(&path, "read", e))?;
        let mut records: Self =
            serde_json::from_value(raw.clone()).map_err(|e| record_error(&path, "read", e))?;
        records.raw = raw;
        if records.version != 1
            || records.installs.iter().enumerate().any(|(i, e)| {
                records.installs[..i]
                    .iter()
                    .any(|old| old.target == e.target)
            })
        {
            return Err(record_error(
                &path,
                "read",
                "unsupported version or duplicate targets",
            ));
        }
        Ok(records)
    }
    pub fn save(&self, home: &Path) -> Result<(), SkillError> {
        let parent = home.join("skills");
        let path = parent.join("installs.json");
        private_dir(&parent).map_err(|e| record_error(&path, "save", e))?;
        let mut root = if self.raw.is_object() {
            self.raw.clone()
        } else {
            serde_json::json!({})
        };
        let mut installs = Vec::new();
        for entry in &self.installs {
            let encoded =
                serde_json::to_value(entry).map_err(|e| record_error(&path, "save", e))?;
            let mut value = self.raw["installs"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["target"] == encoded["target"]))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            if let (Some(previous), Some(fields)) = (value.as_object_mut(), encoded.as_object()) {
                previous.extend(fields.clone());
            }
            installs.push(value);
        }
        root["version"] = self.version.into();
        root["installs"] = installs.into();
        let bytes = serde_json::to_vec(&root).map_err(|e| record_error(&path, "save", e))?;
        atomic_write(&path, &bytes, 0o600).map_err(|e| record_error(&path, "save", e))
    }
    pub fn entry(&self, target: &Path) -> Option<&InstallEntry> {
        self.installs.iter().find(|e| e.target == target)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallEntry {
    pub target: PathBuf,
    pub profile: String,
    pub hermes_root: PathBuf,
    pub skill_version: String,
    pub command: String,
    pub files: BTreeMap<String, String>,
    pub installed_at: String,
}
pub struct InstallOptions {
    pub command: String,
    pub force: bool,
    pub dry_run: bool,
    pub talaria_home: PathBuf,
    pub now: chrono::DateTime<chrono::Utc>,
    pub lock_wait: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallAction {
    Write,
    Replace,
    None,
    Refuse,
    BackupAndReplace,
    Remove,
    BackupAndRemove,
}
#[derive(Debug)]
pub struct InstallReport {
    pub before: SkillInspection,
    pub action: InstallAction,
    pub backup: Option<PathBuf>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallStage {
    AfterInspection,
    AfterPublish,
    AfterRecord,
}
pub fn install(target: &SkillTarget, opts: &InstallOptions) -> Result<InstallReport, SkillError> {
    change(target, opts, false, None)
}
pub fn uninstall(target: &SkillTarget, opts: &InstallOptions) -> Result<InstallReport, SkillError> {
    change(target, opts, true, None)
}
#[doc(hidden)]
pub fn install_with_hook(
    target: &SkillTarget,
    opts: &InstallOptions,
    hook: &mut dyn FnMut(InstallStage),
) -> Result<InstallReport, SkillError> {
    change(target, opts, false, Some(hook))
}
pub fn recover(target: &SkillTarget, opts: &InstallOptions) -> Result<Vec<String>, SkillError> {
    let _locks = locks(target, opts)?;
    recover_locked(target, &opts.talaria_home)
}
fn record_error(path: &Path, verb: &str, error: impl std::fmt::Display) -> SkillError {
    SkillError::Record {
        path: path.into(),
        message: format!("cannot {verb} {}: {error}", path.display()),
    }
}
fn id() -> String {
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let bytes = Sha256::digest(
        format!(
            "{:?}:{}:{}",
            std::time::SystemTime::now(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    );
    hex::encode(&bytes[..8])
}
pub(crate) fn private_dir(path: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}
fn create_file(path: &Path, mode: u32) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let file = o.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    Ok(file)
}
fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent folder"))?;
    let temp = parent.join(format!(
        "{}.tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        id()
    ));
    let result = (|| {
        let mut f = create_file(&temp, mode)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
fn lock(path: &Path, target: &SkillTarget, wait: Duration) -> Result<File, SkillError> {
    let mut o = OpenOptions::new();
    o.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let file = o.open(path).map_err(|e| io_error(path, e))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) => {
                if start.elapsed() >= wait {
                    return Err(SkillError::LockTimeout {
                        profile: target.profile.clone(),
                    });
                }
                std::thread::sleep(
                    Duration::from_millis(100).min(wait.saturating_sub(start.elapsed())),
                );
            }
            Err(fs::TryLockError::Error(e)) => return Err(io_error(path, e)),
        }
    }
}
struct Locks {
    _record: File,
    _target: File,
}
fn locks(target: &SkillTarget, opts: &InstallOptions) -> Result<Locks, SkillError> {
    let p = target.home.join("skills/.locks");
    private_dir(&p).map_err(|e| io_error(&p, e))?;
    let target_lock = lock(&p.join("talaria-install.lock"), target, opts.lock_wait)?;
    let p = opts.talaria_home.join("skills");
    private_dir(&p).map_err(|e| io_error(&p, e))?;
    let record = lock(&p.join("installs.lock"), target, opts.lock_wait)?;
    Ok(Locks {
        _record: record,
        _target: target_lock,
    })
}
fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn action(state: &SkillState, force: bool, uninstall: bool) -> InstallAction {
    match (state, force, uninstall) {
        (SkillState::Missing, _, false) => InstallAction::Write,
        (SkillState::Missing, _, true) | (SkillState::Current, _, false) => InstallAction::None,
        (SkillState::Outdated, _, false) => InstallAction::Replace,
        (SkillState::Current | SkillState::Outdated, _, true) => InstallAction::Remove,
        (SkillState::Modified | SkillState::Foreign, false, _) => InstallAction::Refuse,
        (_, true, false) => InstallAction::BackupAndReplace,
        (_, true, true) => InstallAction::BackupAndRemove,
    }
}
#[derive(Serialize, Deserialize)]
struct Intent {
    kind: String,
    target: PathBuf,
    had_old: bool,
    new_files: BTreeMap<String, String>,
}
fn matches_files(path: &Path, files: &BTreeMap<String, String>) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
        && scan(path).is_ok_and(|(actual, special)| special.is_empty() && actual == *files)
}
fn recover_locked(target: &SkillTarget, talaria_home: &Path) -> Result<Vec<String>, SkillError> {
    let records = InstallRecords::load(talaria_home)?;
    let parent = target.home.join("skills/.locks");
    let mut warnings = vec![];
    for row in fs::read_dir(&parent).map_err(|e| io_error(&parent, e))? {
        let row = row.map_err(|e| io_error(&parent, e))?;
        if !row
            .file_name()
            .to_string_lossy()
            .starts_with("talaria-staging-")
        {
            continue;
        }
        let stage = row.path();
        if !fs::symlink_metadata(&stage)
            .map_err(|e| io_error(&stage, e))?
            .is_dir()
        {
            return Err(io_error(
                &stage,
                io::Error::other("staging entry is not a folder"),
            ));
        }
        let intent_path = stage.join("intent.json");
        let intent: Intent =
            serde_json::from_slice(&fs::read(&intent_path).map_err(|e| io_error(&intent_path, e))?)
                .map_err(|e| record_error(&intent_path, "read", e))?;
        if intent.target != target.dir {
            return Err(record_error(
                &intent_path,
                "recover",
                "target differs from this profile",
            ));
        }
        let committed = match records.entry(&target.dir) {
            Some(e) => matches_files(&target.dir, &e.files),
            None => !exists(&target.dir).map_err(|e| io_error(&target.dir, e))?,
        };
        if !committed {
            if exists(&stage.join("old")).map_err(|e| io_error(&stage, e))? {
                if exists(&target.dir).map_err(|e| io_error(&target.dir, e))? {
                    remove(&stage.join("discarded")).map_err(|e| io_error(&stage, e))?;
                    fs::rename(&target.dir, stage.join("discarded"))
                        .map_err(|e| io_error(&target.dir, e))?;
                }
                fs::rename(stage.join("old"), &target.dir).map_err(|e| io_error(&target.dir, e))?;
                warnings.push(format!(
                    "recovered an interrupted change in profile {}",
                    target.profile
                ));
            } else if intent.kind == "install"
                && !intent.had_old
                && matches_files(&target.dir, &intent.new_files)
            {
                remove(&target.dir).map_err(|e| io_error(&target.dir, e))?;
            }
        }
        remove(&stage).map_err(|e| io_error(&stage, e))?;
    }
    Ok(warnings)
}
fn change(
    target: &SkillTarget,
    opts: &InstallOptions,
    uninstall: bool,
    mut hook: Option<&mut dyn FnMut(InstallStage)>,
) -> Result<InstallReport, SkillError> {
    super::normalise_profile(&target.profile)?;
    super::validate_root(&target.hermes_root)?;
    super::validate_command(&opts.command)?;
    let mut warnings = vec![];
    let _locks = if opts.dry_run {
        None
    } else {
        let locks = locks(target, opts)?;
        warnings.extend(recover_locked(target, &opts.talaria_home)?);
        Some(locks)
    };
    let mut records = InstallRecords::load(&opts.talaria_home)?;
    let (command, root) = if uninstall {
        records
            .entry(&target.dir)
            .map(|e| (e.command.as_str(), e.hermes_root.as_path()))
            .unwrap_or((&opts.command, &target.hermes_root))
    } else {
        (&opts.command[..], target.hermes_root.as_path())
    };
    let rendered = render(&target.profile, command, root)?;
    let before =
        super::inspect(target, &records, &rendered).map_err(|e| io_error(&target.dir, e))?;
    let action = action(&before.state, opts.force, uninstall);
    if let Some(h) = hook.as_mut() {
        h(InstallStage::AfterInspection);
    }
    if !uninstall && !Path::new(&opts.command).is_absolute() && !on_path(&opts.command) {
        warnings.push(format!("'{}' is not on PATH for this process; the agent's terminal may not find it. Use --command <absolute path>.",opts.command));
    }
    let mut report = InstallReport {
        before,
        action,
        backup: None,
        warnings,
    };
    if opts.dry_run || matches!(action, InstallAction::None | InstallAction::Refuse) {
        return Ok(report);
    }
    if matches!(
        action,
        InstallAction::BackupAndReplace | InstallAction::BackupAndRemove
    ) {
        report.backup = Some(backup(target, opts)?);
    }
    let stage = target
        .home
        .join("skills/.locks")
        .join(format!("talaria-staging-{}", id()));
    private_dir(&stage).map_err(|e| io_error(&stage, e))?;
    let had_old = exists(&target.dir).map_err(|e| io_error(&target.dir, e))?;
    let hashes = if uninstall {
        BTreeMap::new()
    } else {
        render_hashes(&rendered)
    };
    let intent = Intent {
        kind: if uninstall { "uninstall" } else { "install" }.into(),
        target: target.dir.clone(),
        had_old,
        new_files: hashes.clone(),
    };
    let prepare = (|| -> io::Result<()> {
        let bytes = serde_json::to_vec(&intent).map_err(io::Error::other)?;
        atomic_write(&stage.join("intent.json"), &bytes, 0o600)?;
        if !uninstall {
            let next = stage.join("new").join(SKILL_NAME);
            for file in &rendered {
                let path = next.join(file.path);
                fs::create_dir_all(
                    path.parent()
                        .ok_or_else(|| io::Error::other("missing parent"))?,
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(parent) = path.parent() {
                        for dir in parent.ancestors().take_while(|dir| dir.starts_with(&next)) {
                            fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;
                        }
                    }
                }
                let mut f = create_file(&path, 0o644)?;
                f.write_all(&file.bytes)?;
                f.sync_all()?;
            }
        }
        Ok(())
    })();
    if let Err(e) = prepare {
        let _ = remove(&stage);
        return Err(io_error(&target.dir, e));
    }
    if had_old && let Err(e) = fs::rename(&target.dir, stage.join("old")) {
        let _ = remove(&stage);
        return Err(io_error(&target.dir, e));
    }
    if !uninstall && let Err(e) = fs::rename(stage.join("new").join(SKILL_NAME), &target.dir) {
        if had_old {
            fs::rename(stage.join("old"), &target.dir)
                .map_err(|rollback| io_error(&target.dir, rollback))?;
        }
        let _ = remove(&stage);
        return Err(io_error(&target.dir, e));
    }
    if let Some(h) = hook.as_mut() {
        h(InstallStage::AfterPublish);
    }
    let entry = InstallEntry {
        target: target.dir.clone(),
        profile: target.profile.clone(),
        hermes_root: target.hermes_root.clone(),
        skill_version: SKILL_VERSION.into(),
        command: opts.command.clone(),
        files: hashes,
        installed_at: opts.now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    };
    if let Some(index) = records.installs.iter().position(|e| e.target == target.dir) {
        if uninstall {
            records.installs.remove(index);
        } else {
            records.installs[index] = entry;
        }
    } else if !uninstall {
        records.installs.push(entry);
    }
    if let Err(e) = records.save(&opts.talaria_home) {
        if !uninstall {
            fs::rename(&target.dir, stage.join("discarded"))
                .map_err(|e| io_error(&target.dir, e))?;
        }
        if had_old {
            fs::rename(stage.join("old"), &target.dir).map_err(|e| io_error(&target.dir, e))?;
        }
        let _ = remove(&stage);
        return Err(e);
    }
    if let Some(h) = hook.as_mut() {
        h(InstallStage::AfterRecord);
    }
    if let Err(e) = remove(&stage) {
        report
            .warnings
            .push(format!("could not remove staging folder: {e}"));
    }
    Ok(report)
}
fn on_path(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|p| {
            let f = p.join(command);
            let Ok(meta) = fs::metadata(f) else {
                return false;
            };
            if !meta.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
    })
}
fn backup(target: &SkillTarget, opts: &InstallOptions) -> Result<PathBuf, SkillError> {
    let parent = opts.talaria_home.join("skills/backups");
    private_dir(&parent).map_err(|e| io_error(&parent, e))?;
    let stem = format!("{}-{}", opts.now.format("%Y%m%dT%H%M%SZ"), target.profile);
    let mut suffix = 1;
    let base = loop {
        let p = parent.join(if suffix == 1 {
            stem.clone()
        } else {
            format!("{stem}-{suffix}")
        });
        match fs::create_dir(&p) {
            Ok(()) => break p,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => suffix += 1,
            Err(e) => return Err(io_error(&p, e)),
        }
    };
    let path = base.join(SKILL_NAME);
    copy_entry(&target.dir, &path).map_err(|e| io_error(&target.dir, e))?;
    Ok(base)
}
fn copy_entry(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        let link = fs::read_link(from)?;
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(link, to)?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTypeExt;
            if meta.file_type().is_symlink_dir() {
                std::os::windows::fs::symlink_dir(link, to)?;
            } else {
                std::os::windows::fs::symlink_file(link, to)?;
            }
        }
        #[cfg(not(any(unix, windows)))]
        return Err(io::Error::other(
            "symbolic links are unsupported on this platform",
        ));
    } else if meta.is_dir() {
        fs::create_dir(to)?;
        for item in fs::read_dir(from)? {
            let item = item?;
            copy_entry(&item.path(), &to.join(item.file_name()))?;
        }
        fs::set_permissions(to, meta.permissions())?;
    } else if meta.is_file() {
        fs::copy(from, to)?;
        fs::set_permissions(to, meta.permissions())?;
    } else {
        return Err(io::Error::other(format!(
            "cannot back up {}: not a file, folder or link",
            from.display()
        )));
    }
    Ok(())
}
