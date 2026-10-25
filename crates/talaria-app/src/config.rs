//! Loader for `~/.talaria/config.toml`.
//!
//! The file is walked by hand, so one bad key costs only that key: each problem becomes one
//! plain notice and the setting falls back to its default. Notices name the file by the literal
//! text `~/.talaria/config.toml`, never by its real path.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// The shared file for favourites and defaults on this machine.
#[must_use]
pub fn models_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".talaria/models.json"))
}

/// Match Hermes's public label for a served profile home.
#[must_use]
pub fn served_model_profile(profile_id: &str) -> &str {
    let home = Path::new(profile_id);
    if home
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        == Some("profiles")
        && let Some(name) = home
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|name| !name.starts_with('.'))
    {
        name
    } else {
        "default"
    }
}

/// The local offset is supplied to pure catalogue formatting as data.
#[must_use]
pub fn model_display_zone() -> talaria_core::models::DisplayZone {
    let offset = std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| {
            let value = value.trim();
            let bytes = value.as_bytes();
            if bytes.len() != 5
                || !matches!(bytes[0], b'+' | b'-')
                || !bytes[1..].iter().all(u8::is_ascii_digit)
            {
                return None;
            }
            let hours = value[1..3].parse::<i32>().ok()?;
            let minutes = value[3..5].parse::<i32>().ok()?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            Some((hours * 60 + minutes) * if bytes[0] == b'-' { -1 } else { 1 })
        })
        .unwrap_or(0);
    talaria_core::models::DisplayZone {
        minutes_east: offset,
    }
}

/// Read model choices before the application begins handling input.
pub fn load_models() -> Result<talaria_core::models::SavedModels, String> {
    use crate::models_file::SavedModelsFile;
    match models_path() {
        Some(path) => talaria_core::models::SavedModels::read(&path),
        None => Err("~/.talaria/models.json could not be read (the home folder is unavailable); favourites and saved defaults are off until it is fixed or removed.".into()),
    }
}

/// Raw settings read from the configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Key table entries, still strings, for the key map to interpret.
    pub keys: BTreeMap<String, Vec<String>>,
    /// Status command settings. The command is not run here.
    pub status: StatusConfig,
    /// Names the status command may see. Filtering happens elsewhere.
    pub environment: EnvironmentConfig,
}

/// Status command text and how often it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusConfig {
    /// The command line, still one string. Empty means off.
    pub command: Option<String>,
    /// Seconds between runs. The default is 5.
    pub interval_seconds: u64,
}

/// Environment names forwarded to the status command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentConfig {
    /// Variable names. The default is an empty list.
    pub allowlist: Vec<String>,
}

/// A configuration plus the notices produced while reading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    /// The settings, with defaults filled in for anything unusable.
    pub config: Config,
    /// Plain sentences. Never a real path or user name.
    pub notices: Vec<String>,
}

/// Default seconds between status command runs.
pub const DEFAULT_STATUS_INTERVAL_SECONDS: u64 = 5;

/// The accepted range of `status.interval_seconds`.
const INTERVAL_RANGE: std::ops::RangeInclusive<i64> = 1..=3600;

/// How the file is named in every notice.
const FILE: &str = "~/.talaria/config.toml";

/// Longest parser message quoted in a notice, in characters.
const MESSAGE_LIMIT: usize = 120;

impl Default for Config {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            status: StatusConfig {
                command: None,
                interval_seconds: DEFAULT_STATUS_INTERVAL_SECONDS,
            },
            environment: EnvironmentConfig {
                allowlist: Vec::new(),
            },
        }
    }
}

/// Parse configuration text. A syntax error returns defaults and one notice.
#[must_use]
pub fn parse(text: &str) -> LoadedConfig {
    let table = match toml::from_str::<Table>(text) {
        Ok(table) => table,
        Err(error) => {
            let message: String = error.message().chars().take(MESSAGE_LIMIT).collect();
            let notice = match error.span() {
                Some(span) => {
                    let (line, column) = line_column(text, span.start);
                    format!(
                        "Talaria could not read {FILE} (line {line}, column {column}): {message}. \
                         Using the default settings."
                    )
                }
                None => {
                    format!("Talaria could not read {FILE}: {message}. Using the default settings.")
                }
            };
            return LoadedConfig {
                config: Config::default(),
                notices: vec![notice],
            };
        }
    };

    let mut config = Config::default();
    let mut notices = Vec::new();
    for name in ["keys", "status", "environment"] {
        let Some(value) = table.get(name) else {
            continue;
        };
        let Some(section) = value.as_table() else {
            notices.push(format!(
                "{name} in {FILE} must be a table; using the defaults for it."
            ));
            continue;
        };
        match name {
            "keys" => config.keys = read_keys(section, &mut notices),
            "status" => config.status = read_status(section, &mut notices),
            _ => config.environment = read_environment(section, &mut notices),
        }
    }
    for (name, value) in &table {
        if matches!(name.as_str(), "keys" | "status" | "environment") {
            continue;
        }
        let shown = if value.is_table() {
            format!("[{name}]")
        } else {
            name.clone()
        };
        notices.push(format!(
            "Ignoring {shown} in {FILE}: Talaria does not read it yet."
        ));
    }
    LoadedConfig { config, notices }
}

/// One-based line and column, in characters, of a byte offset. An offset at the end of the
/// text reports the last character.
fn line_column(text: &str, byte_offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (1, 1);
    }
    let mut offset = byte_offset.min(text.len());
    if offset == text.len() {
        offset = text
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let head = &text[..offset];
    let line = head.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = head.chars().rev().take_while(|c| *c != '\n').count() + 1;
    (line, column)
}

fn read_keys(section: &Table, notices: &mut Vec<String>) -> BTreeMap<String, Vec<String>> {
    let mut keys = BTreeMap::new();
    for (name, value) in section {
        match value {
            Value::String(chord) => {
                keys.insert(name.clone(), vec![chord.clone()]);
            }
            Value::Array(items) => {
                let mut chords = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    match item.as_str() {
                        Some(chord) => chords.push(chord.to_string()),
                        None => notices.push(format!(
                            "[keys] {name} in {FILE}: item {} is not a key name; ignoring it.",
                            index + 1
                        )),
                    }
                }
                if items.is_empty() || !chords.is_empty() {
                    keys.insert(name.clone(), chords);
                }
            }
            _ => notices.push(format!(
                "[keys] {name} in {FILE} must be a key name or a list of key names; using the \
                 default."
            )),
        }
    }
    keys
}

fn read_status(section: &Table, notices: &mut Vec<String>) -> StatusConfig {
    let mut status = Config::default().status;
    for (name, value) in section {
        match name.as_str() {
            "command" => match value.as_str() {
                Some(command) if command.trim().is_empty() => notices.push(format!(
                    "status.command in {FILE} is empty; the status command is off."
                )),
                Some(command) => status.command = Some(command.to_string()),
                None => notices.push(format!(
                    "status.command in {FILE} must be a text string; the status command is off."
                )),
            },
            "interval_seconds" => match value.as_integer() {
                Some(seconds) if INTERVAL_RANGE.contains(&seconds) => {
                    status.interval_seconds = seconds.unsigned_abs();
                }
                _ => notices.push(format!(
                    "status.interval_seconds in {FILE} must be a whole number from 1 to 3600; \
                     using {DEFAULT_STATUS_INTERVAL_SECONDS}."
                )),
            },
            _ => notices.push(unknown_setting("status", name)),
        }
    }
    status
}

fn read_environment(section: &Table, notices: &mut Vec<String>) -> EnvironmentConfig {
    let mut environment = Config::default().environment;
    for (name, value) in section {
        if name != "allowlist" {
            notices.push(unknown_setting("environment", name));
            continue;
        }
        let names: Option<Vec<String>> = value.as_array().and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect()
        });
        match names {
            Some(names) => environment.allowlist = names,
            None => notices.push(format!(
                "environment.allowlist in {FILE} must be a list of variable names; using an \
                 empty list."
            )),
        }
    }
    environment
}

fn unknown_setting(table: &str, key: &str) -> String {
    format!("Ignoring {table}.{key} in {FILE}: it is not a known setting.")
}

/// Read a configuration file. A missing file is defaults and no notice; any other read error
/// is one notice naming the kind of error, never the path.
#[must_use]
pub fn load_file(path: &Path) -> LoadedConfig {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedConfig {
            config: Config::default(),
            notices: Vec::new(),
        },
        Err(error) => LoadedConfig {
            config: Config::default(),
            notices: vec![format!(
                "Talaria could not read {FILE} ({}). Using the default settings.",
                error.kind()
            )],
        },
    }
}

/// Path of the user configuration file under `home`.
#[must_use]
pub fn user_config_path(home: &Path) -> PathBuf {
    home.join(".talaria").join("config.toml")
}

/// Read `<home>/.talaria/config.toml`.
#[must_use]
pub fn load_user_config(home: &Path) -> LoadedConfig {
    load_file(&user_config_path(home))
}

#[cfg(test)]
mod tests {
    use super::{
        Config, DEFAULT_STATUS_INTERVAL_SECONDS, load_file, load_user_config, parse,
        user_config_path,
    };

    fn line_column(text: &str, byte_offset: usize) -> (usize, usize) {
        if text.is_empty() {
            return (1, 1);
        }
        let mut offset = byte_offset.min(text.len());
        if offset == text.len() {
            offset = text
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
        while offset > 0 && !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let head = &text[..offset];
        let line = head.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = head.chars().rev().take_while(|ch| *ch != '\n').count() + 1;
        (line, column)
    }

    fn syntax_notice(text: &str) -> String {
        let error = toml::from_str::<toml::Table>(text).expect_err("this text is not valid toml");
        let message: String = error.message().chars().take(120).collect();
        match error.span() {
            Some(span) => {
                let (line, column) = line_column(text, span.start);
                format!(
                    "Talaria could not read ~/.talaria/config.toml (line {line}, column {column}): \
                     {message}. Using the default settings."
                )
            }
            None => format!(
                "Talaria could not read ~/.talaria/config.toml: {message}. Using the default \
                 settings."
            ),
        }
    }

    #[test]
    fn missing_file_gives_defaults() {
        let home = tempfile::tempdir().unwrap();
        let loaded = load_user_config(home.path());
        assert!(loaded.notices.is_empty());
        assert_eq!(loaded.config, Config::default());
        assert!(loaded.config.keys.is_empty());
        assert!(loaded.config.status.command.is_none());
        assert_eq!(
            loaded.config.status.interval_seconds,
            DEFAULT_STATUS_INTERVAL_SECONDS
        );
        assert!(loaded.config.environment.allowlist.is_empty());

        let path = user_config_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[keys]\nsubmit = \"enter\"\n[status]\ninterval_seconds = 5\n[environment]\nallowlist = []\n",
        )
        .unwrap();
        let loaded = load_file(&path);
        assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
        assert_eq!(
            loaded.config.keys.get("submit").map(Vec::as_slice),
            Some(["enter".to_string()].as_slice())
        );
        assert_eq!(loaded.config.status.interval_seconds, 5);
        assert!(loaded.config.environment.allowlist.is_empty());
    }

    #[test]
    fn bad_file_gives_defaults_and_one_notice() {
        let text = "x = 1\ny = = 2\n";
        let loaded = parse(text);
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.notices, vec![syntax_notice(text)]);
        assert!(
            loaded.notices[0]
                .starts_with("Talaria could not read ~/.talaria/config.toml (line 2, column ")
        );

        let dir = tempfile::tempdir().unwrap();
        let as_dir = dir.path().join("config.toml");
        std::fs::create_dir(&as_dir).unwrap();
        let loaded = load_file(&as_dir);
        assert_eq!(loaded.config, Config::default());
        assert_eq!(
            loaded.notices,
            vec![
                "Talaria could not read ~/.talaria/config.toml (is a directory). Using the \
                 default settings."
                    .to_string()
            ]
        );
        assert!(!loaded.notices[0].contains(&as_dir.display().to_string()));

        let bad_bytes = dir.path().join("bad.toml");
        std::fs::write(&bad_bytes, [0xFFu8, 0xFE, 0x00]).unwrap();
        let loaded = load_file(&bad_bytes);
        assert_eq!(loaded.config, Config::default());
        assert_eq!(
            loaded.notices,
            vec![
                "Talaria could not read ~/.talaria/config.toml (invalid data). Using the default \
                 settings."
                    .to_string()
            ]
        );
        assert!(!loaded.notices[0].contains(&bad_bytes.display().to_string()));
    }

    #[test]
    fn unknown_names_reported_once() {
        let loaded = parse("[theme]\n\n[ui]\n\n[status]\ncolour = 1\n\n[environment]\nx = 1\n");
        assert_eq!(
            loaded.notices,
            vec![
                "Ignoring status.colour in ~/.talaria/config.toml: it is not a known setting."
                    .to_string(),
                "Ignoring environment.x in ~/.talaria/config.toml: it is not a known setting."
                    .to_string(),
                "Ignoring [theme] in ~/.talaria/config.toml: Talaria does not read it yet."
                    .to_string(),
                "Ignoring [ui] in ~/.talaria/config.toml: Talaria does not read it yet."
                    .to_string(),
            ]
        );
        assert_eq!(loaded.config.status.interval_seconds, 5);
        assert!(loaded.config.environment.allowlist.is_empty());
    }

    #[test]
    fn wrong_values_fall_back_with_a_notice() {
        let interval = "status.interval_seconds in ~/.talaria/config.toml must be a whole number \
                        from 1 to 3600; using 5.";
        for text in [
            "[status]\ninterval_seconds = \"x\"\n",
            "[status]\ninterval_seconds = 0\n",
            "[status]\ninterval_seconds = 3601\n",
        ] {
            let loaded = parse(text);
            assert_eq!(loaded.notices, vec![interval.to_string()], "{text}");
            assert_eq!(loaded.config.status.interval_seconds, 5);
        }

        let loaded = parse("[status]\ncommand = 5\n");
        assert_eq!(
            loaded.notices,
            vec![
                "status.command in ~/.talaria/config.toml must be a text string; the status \
                 command is off."
                    .to_string()
            ]
        );
        assert!(loaded.config.status.command.is_none());

        let loaded = parse("[status]\ncommand = \"   \"\n");
        assert_eq!(
            loaded.notices,
            vec![
                "status.command in ~/.talaria/config.toml is empty; the status command is off."
                    .to_string()
            ]
        );
        assert!(loaded.config.status.command.is_none());

        let loaded = parse("[environment]\nallowlist = \"EDITOR\"\n");
        assert_eq!(
            loaded.notices,
            vec![
                "environment.allowlist in ~/.talaria/config.toml must be a list of variable \
                 names; using an empty list."
                    .to_string()
            ]
        );
        assert!(loaded.config.environment.allowlist.is_empty());

        let loaded = parse("status = 5\n");
        assert_eq!(
            loaded.notices,
            vec![
                "status in ~/.talaria/config.toml must be a table; using the defaults for it."
                    .to_string()
            ]
        );
        assert_eq!(loaded.config.status.interval_seconds, 5);

        let loaded = parse("[keys]\nsubmit = 3\n");
        assert_eq!(
            loaded.notices,
            vec![
                "[keys] submit in ~/.talaria/config.toml must be a key name or a list of key \
                 names; using the default."
                    .to_string()
            ]
        );
        assert!(!loaded.config.keys.contains_key("submit"));

        let loaded = parse("[keys]\nyank = [\"ctrl+y\", 3]\n");
        assert_eq!(
            loaded.notices,
            vec![
                "[keys] yank in ~/.talaria/config.toml: item 2 is not a key name; ignoring it."
                    .to_string()
            ]
        );
        assert_eq!(
            loaded.config.keys.get("yank").map(Vec::as_slice),
            Some(["ctrl+y".to_string()].as_slice())
        );
    }
}
