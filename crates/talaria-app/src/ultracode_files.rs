//! The files behind workflow routing: the saved per-session switch and the local setup read.
//! The application runs these off its update loop and posts the results as messages.
use crate::ultracode::{SetupFacts, SkillCheck, TierCheck};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use talaria_core::ultracode::SavedModes;
use talaria_workflow::{builtins, skill, staffing::StaffingTable};

/// The saved switch's file name in Talaria's home folder.
pub const MODE_FILE: &str = "workflow-mode.json";

/// The saved per-session switch. Writes are atomic, and a write older than one already made is
/// dropped, so two saves that finish out of order leave the newer text.
#[derive(Debug, Clone)]
pub struct ModeFile {
    path: PathBuf,
    written: Arc<Mutex<u64>>,
}
impl ModeFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Arc::new(Mutex::new(0)),
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// The saved settings; a missing file has none.
    pub fn read(path: &Path) -> Result<SavedModes, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => SavedModes::from_json(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SavedModes::default()),
            Err(error) => Err(error.kind().to_string()),
        }
    }
    /// Replace the file with `text`, save number `generation`.
    pub fn write(&self, generation: u64, text: &str) -> io::Result<()> {
        let mut written = self
            .written
            .lock()
            .map_err(|_| io::Error::other("an earlier save panicked"))?;
        if generation <= *written {
            return Ok(());
        }
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(text.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|error| error.error)?;
        *written = generation;
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
        return SkillCheck::Unknown;
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
