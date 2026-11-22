//! The files behind workflow routing: the saved per-session switch and the local setup read.
//! The application runs these off its update loop and posts the results as messages.
use crate::hivecode::{SetupFacts, SkillCheck, TierCheck};
use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use talaria_core::hivecode::{SavedChanges, SavedModes};
use talaria_workflow::{builtins, skill, staffing::StaffingTable};

/// The saved switch's file name in Talaria's home folder.
pub const MODE_FILE: &str = "workflow-mode.json";
/// The largest saved switch file read; a larger one is unreadable and replaced by the next save.
pub const MODE_FILE_LIMIT: u64 = 4 * 1024 * 1024;
/// The command an install from Talaria writes into the skill.
pub const INSTALL_COMMAND: &str = "talaria";

/// The saved per-session switch, shared by every Talaria window on this machine. A save holds a
/// lock beside the file, reads the file again, merges in this window's changes and replaces the
/// file atomically, so settings other windows saved are kept. Within one window, a session's
/// change older than one already written is dropped, so two saves that finish out of order
/// leave the newer setting.
#[derive(Debug, Clone)]
pub struct ModeFile {
    path: PathBuf,
    /// The save number last written for each stored session id.
    written: Arc<Mutex<HashMap<String, u64>>>,
}
impl ModeFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// The saved settings; a missing file has none. A file over [`MODE_FILE_LIMIT`] is
    /// unreadable.
    pub fn read(path: &Path) -> Result<SavedModes, String> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SavedModes::default());
            }
            Err(error) => return Err(error.kind().to_string()),
        };
        let mut text = String::new();
        io::Read::read_to_string(&mut io::Read::take(file, MODE_FILE_LIMIT + 1), &mut text)
            .map_err(|error| error.kind().to_string())?;
        if text.len() as u64 > MODE_FILE_LIMIT {
            return Err("the file is too large".into());
        }
        SavedModes::from_json(&text)
    }
    /// Merge `changes`, save number `generation`, into the file. An unreadable file is replaced.
    pub fn write(&self, generation: u64, changes: &SavedChanges) -> io::Result<()> {
        let mut written = self
            .written
            .lock()
            .map_err(|_| io::Error::other("an earlier save panicked"))?;
        let mut changes = changes.clone();
        changes.retain(|stored| written.get(stored).is_none_or(|last| generation > *last));
        if changes.is_empty() {
            return Ok(());
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut lock_path = self.path.clone().into_os_string();
        lock_path.push(".lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        lock.lock()?;
        let mut saved = Self::read(&self.path).unwrap_or_default();
        saved.merge(&changes);
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(saved.to_json().as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|error| error.error)?;
        for stored in changes.ids() {
            written.insert(stored.to_owned(), generation);
        }
        Ok(())
    }
}

/// Read the workflow setup of `profile` on this machine: the installed skill against its
/// install record and this Talaria's copy, the staffing tiers, and the built-in workflows.
pub fn read_setup(profile: &str, hermes_home: &Path, talaria_home: &Path) -> SetupFacts {
    SetupFacts {
        skill: read_skill(profile, hermes_home, talaria_home),
        tiers: match StaffingTable::load(&talaria_home.join("staffing.toml")) {
            Ok(table) if table.tiers.is_empty() => TierCheck::Missing,
            Ok(_) => TierCheck::Present,
            Err(error) => TierCheck::Unreadable(error.to_string()),
        },
        missing_builtins: builtins::BUILTINS
            .iter()
            .filter(|builtin| {
                builtins::installed_state(&talaria_home.join("workflows"), builtin)
                    == builtins::InstalledState::NotInstalled
            })
            .map(|builtin| builtin.name.to_owned())
            .collect(),
    }
}

fn read_skill(profile: &str, hermes_home: &Path, talaria_home: &Path) -> SkillCheck {
    let Ok(target) = skill::SkillTarget::resolve_local(profile, hermes_home) else {
        return SkillCheck::Unknown;
    };
    let Ok(records) = skill::InstallRecords::load(talaria_home) else {
        return SkillCheck::Unknown;
    };
    let Some(entry) = records.entry(&target.dir) else {
        return unrecorded(&target);
    };
    let Ok(rendered) = skill::render(&target.profile, &entry.command, &entry.hermes_root) else {
        return SkillCheck::Unknown;
    };
    let Ok(inspection) = skill::inspect(&target, &records, &rendered) else {
        return SkillCheck::Unknown;
    };
    let digest = || skill::installed_digest(&target.dir).unwrap_or_default();
    match inspection.state {
        skill::SkillState::Current => SkillCheck::Unchanged,
        skill::SkillState::Modified => SkillCheck::Modified {
            changes: inspection.changed_files,
            digest: digest(),
        },
        skill::SkillState::Outdated => SkillCheck::Outdated { digest: digest() },
        skill::SkillState::Missing | skill::SkillState::Foreign => SkillCheck::Unknown,
    }
}

/// A skill with no install record in Talaria's home folder, compared with the copy built into
/// this Talaria. A missing skill is the catalogue check's to report.
fn unrecorded(target: &skill::SkillTarget) -> SkillCheck {
    if std::fs::symlink_metadata(&target.dir).is_err() {
        return SkillCheck::Unknown;
    }
    let Ok(rendered) = skill::render(&target.profile, INSTALL_COMMAND, &target.hermes_root) else {
        return SkillCheck::Unknown;
    };
    match skill::same_files(&target.dir, &rendered) {
        Ok(true) => SkillCheck::Unchanged,
        Ok(false) => SkillCheck::Differs {
            digest: skill::installed_digest(&target.dir).unwrap_or_default(),
        },
        Err(_) => SkillCheck::Unknown,
    }
}
