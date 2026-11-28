//! Profile preferences changed from `/theme`, beside the user-authored configuration.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

use rustix::fs::{Mode, OFlags, open};
use rustix::io::Errno;
use serde::{Deserialize, Serialize};
use talaria_core::appearance::Polarity;

const FILE: &str = "theme-selection.json";
const LOCK: &str = "theme-selection.lock";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Background {
    Light,
    Dark,
}

impl From<Background> for Polarity {
    fn from(background: Background) -> Self {
        match background {
            Background::Light => Self::Light,
            Background::Dark => Self::Dark,
        }
    }
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Selection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
}

type Profiles = BTreeMap<String, Selection>;

pub(crate) fn read(config: &Path, profile: &str) -> Result<Selection, String> {
    read_profiles(directory(config))
        .map(|mut profiles| profiles.remove(profile).unwrap_or_default())
        .map_err(|error| format!("Talaria could not read {FILE} ({error})."))
}

/// Hold a lock across the read, change, and atomic replacement so windows changing different
/// profiles or settings keep each other's choices.
pub(crate) fn save(
    config: &Path,
    profile: &str,
    change: impl FnOnce(&mut Selection),
) -> Result<(), String> {
    update_profiles(directory(config), profile, change)
        .map_err(|error| format!("Talaria could not save the theme choice in {FILE} ({error})."))
}

fn directory(config: &Path) -> &Path {
    config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn read_profiles(dir: &Path) -> io::Result<Profiles> {
    const MAX_BYTES: u64 = 256 * 1024;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    let file = match open(dir.join(FILE), flags, Mode::empty()) {
        Ok(file) => File::from(file),
        Err(Errno::NOENT) => return Ok(Profiles::new()),
        Err(error) => return Err(error.into()),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("expected a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(io::Error::other("file exceeds 256 KiB"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::from)
}

fn update_profiles(
    dir: &Path,
    profile: &str,
    change: impl FnOnce(&mut Selection),
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let flags =
        OFlags::CREATE | OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    let lock = File::from(open(dir.join(LOCK), flags, Mode::RUSR | Mode::WUSR)?);
    lock.lock()?;
    let mut profiles = read_profiles(dir)?;
    let selected = profiles.entry(profile.to_owned()).or_default();
    change(selected);
    if *selected == Selection::default() {
        profiles.remove(profile);
    }
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut file, &profiles)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(dir.join(FILE)).map_err(|error| error.error)?;
    Ok(())
}
