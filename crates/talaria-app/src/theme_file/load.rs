use super::{ThemeFile, builtin, builtin_description, parse};
use rustix::fs::{Mode, OFlags, open, openat};
use rustix::io::Errno;
use std::io::Read;
use std::path::Path;

pub fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-')
}
pub fn reserved_id(id: &str) -> bool {
    matches!(id, "terminal" | "dark" | "light" | "ember" | "paper")
}

pub(crate) fn load_selection(config: &Path, id: &str) -> Result<ThemeFile, String> {
    if matches!(id, "dark" | "light") {
        return Err(format!("{id} is a base for extends, not a theme."));
    }
    if reserved_id(id)
        && let Some(file) = builtin(id)
    {
        return Ok(file);
    }
    match load_from_disk(config, id) {
        Ok(file) => Ok(file),
        Err(error) if error.absent && builtin_description(id).is_some() => {
            builtin(id).ok_or(error.message)
        }
        Err(error) => Err(error.message),
    }
}

/// Every user theme file beside `config`, sorted by identifier, each loaded or with the reason
/// it cannot be. Reserved and invalid identifiers, other files and symbolic links are left out,
/// and a missing or linked `themes` folder lists nothing.
pub fn list_themes(config: &Path) -> Vec<(String, Result<ThemeFile, String>)> {
    let parent = config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let folder = parent.join("themes");
    if !std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.is_dir()) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(".toml")?;
            (valid_id(id) && !reserved_id(id)).then(|| id.to_owned())
        })
        .collect();
    ids.sort();
    ids.into_iter()
        .map(|id| {
            let file = load(config, &id);
            (id, file)
        })
        .collect()
}

pub fn load(config: &Path, id: &str) -> Result<ThemeFile, String> {
    load_from_disk(config, id).map_err(|error| error.message)
}

struct LoadError {
    message: String,
    absent: bool,
}

impl From<String> for LoadError {
    fn from(message: String) -> Self {
        Self {
            message,
            absent: false,
        }
    }
}

impl LoadError {
    fn open(error: Errno, message: String) -> Self {
        Self {
            message,
            absent: error == Errno::NOENT,
        }
    }
}

fn load_from_disk(config: &Path, id: &str) -> Result<ThemeFile, LoadError> {
    if !valid_id(id) {
        return Err(String::from("invalid theme identifier").into());
    }
    let label = format!("themes/{id}.toml");
    if reserved_id(id) {
        return Err(format!("{label}: reserved theme identifier").into());
    }
    let parent = config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let base = open(parent, flags, Mode::empty()).map_err(|error| {
        LoadError::open(
            error,
            format!("{label}: cannot open configuration directory"),
        )
    })?;
    let themes =
        openat(&base, "themes", flags | OFlags::NOFOLLOW, Mode::empty()).map_err(|error| {
            LoadError::open(error, format!("{label}: cannot open themes directory"))
        })?;
    let fd = openat(
        &themes,
        format!("{id}.toml"),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|error| LoadError::open(error, format!("{label}: cannot open theme file")))?;
    let file = std::fs::File::from(fd);
    let meta = file
        .metadata()
        .map_err(|_| format!("{label}: cannot inspect theme file"))?;
    if !meta.is_file() {
        return Err(format!("{label}: expected a regular file").into());
    }
    const LIMIT: u64 = 256 * 1024;
    if meta.len() > LIMIT {
        return Err(format!("{label}: file exceeds 256 KiB").into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{label}: cannot read theme file"))?;
    if bytes.len() as u64 > LIMIT {
        return Err(format!("{label}: file exceeds 256 KiB").into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{label}: file is not UTF-8"))?;
    parse(text).map_err(|error| format!("{label}: {error}").into())
}
