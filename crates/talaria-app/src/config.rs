//! Loader for `~/.talaria/config.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
pub fn parse(_text: &str) -> LoadedConfig {
    let _table = std::any::type_name::<toml::Table>();
    todo!()
}

/// Read a configuration file. A missing file is defaults and no notice.
#[must_use]
pub fn load_file(_path: &Path) -> LoadedConfig {
    todo!()
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
