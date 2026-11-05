use super::{ThemeFile, parse};
use rustix::fs::{Mode, OFlags, open, openat};
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

/// The built-in theme for `id`, already parsed by the strict parser.
///
/// `terminal`, `ember` and `paper` are the only built-ins. A user file of the
/// same name is never returned here.
#[must_use]
pub fn builtin(_id: &str) -> Option<ThemeFile> {
    None
}

pub fn load(config: &Path, id: &str) -> Result<ThemeFile, String> {
    if !valid_id(id) {
        return Err("invalid theme identifier".into());
    }
    let label = format!("themes/{id}.toml");
    if reserved_id(id) {
        return Err(format!("{label}: reserved theme identifier"));
    }
    let parent = config
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let base = open(parent, flags, Mode::empty())
        .map_err(|_| format!("{label}: cannot open configuration directory"))?;
    let themes = openat(&base, "themes", flags | OFlags::NOFOLLOW, Mode::empty())
        .map_err(|_| format!("{label}: cannot open themes directory"))?;
    let fd = openat(
        &themes,
        format!("{id}.toml"),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| format!("{label}: cannot open theme file"))?;
    let file = std::fs::File::from(fd);
    let meta = file
        .metadata()
        .map_err(|_| format!("{label}: cannot inspect theme file"))?;
    if !meta.is_file() {
        return Err(format!("{label}: expected a regular file"));
    }
    const LIMIT: u64 = 256 * 1024;
    if meta.len() > LIMIT {
        return Err(format!("{label}: file exceeds 256 KiB"));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{label}: cannot read theme file"))?;
    if bytes.len() as u64 > LIMIT {
        return Err(format!("{label}: file exceeds 256 KiB"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{label}: file is not UTF-8"))?;
    parse(text).map_err(|error| format!("{label}: {error}"))
}
