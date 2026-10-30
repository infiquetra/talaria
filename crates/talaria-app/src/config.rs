//! Loader for `~/.talaria/config.toml`.
//!
//! The file is walked by hand, so one bad key costs only that key: each problem becomes one
//! plain notice and the setting falls back to its default. Notices name the file by the literal
//! text `~/.talaria/config.toml`, never by its real path.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub use talaria_core::appearance::{ColorsSetting, ThemeMode};
use talaria_core::attention::AlertKind;
use talaria_core::fleet_attention::FleetScope;
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
    /// External editor settings.
    pub editor: EditorConfig,
    /// Prompt history settings.
    pub history: HistoryConfig,
    /// Notification settings.
    pub notifications: NotificationsConfig,
    /// Herdr reporting settings.
    pub herdr: HerdrConfig,
    /// Theme settings from `[theme]`.
    pub theme: ThemeConfig,
    /// Interface settings from `[ui]`.
    pub ui: UiConfig,
    /// When a paste becomes a label.
    pub composer: ComposerConfig,
    /// Fleet poll timings from `[fleet]`.
    pub fleet: crate::fleet::FleetConfig,
    /// Configured external agent executables.
    pub acp: crate::acp::AcpConfig,
    /// How fleet sessions open in another multiplexer pane.
    pub panes: crate::panes::PaneSettings,
    /// Subagent poll and dock settings from `[subagents]`. Issue #242.
    pub subagents: crate::subagents::SubagentsConfig,
    /// Optional provider and model for the configuration assistant.
    pub configure: Option<(String, String)>,
    /// Local speech preferences; microphone and playback remain on the gateway.
    pub voice: VoiceConfig,
}

/// Voice preferences for newly attached sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceConfig {
    pub read_aloud: bool,
    pub speak_max_chars: usize,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            read_aloud: false,
            speak_max_chars: 1_500,
        }
    }
}

/// Paste collapse thresholds. Zero turns that rule off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerConfig {
    /// Line count that collapses a paste. The default is 5.
    pub paste_collapse_lines: usize,
    /// Character count that collapses a paste. The default is 2,000.
    pub paste_collapse_chars: usize,
    /// Start the composer with vim editing. The default is off.
    pub vim: bool,
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

/// External editor command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorConfig {
    /// The command line, still one string. Blank means the environment, then a fallback.
    pub command: String,
}

/// Prompt history file use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryConfig {
    /// Read and write the history file. The default is true.
    pub save: bool,
}

/// Bell and desktop notification channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationsConfig {
    /// Kinds that ring the terminal bell.
    pub bell: Vec<AlertKind>,
    /// Kinds that send a desktop notification.
    pub desktop: Vec<AlertKind>,
    /// A finished turn alerts only at or above these seconds.
    pub done_after_seconds: u64,
    /// Which background requests raise an alert. The default is every background row.
    pub fleet_needs_you: FleetScope,
    /// Which background finishes raise an alert. The default is watched rows.
    pub fleet_finished: FleetScope,
}

/// Herdr pane state reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HerdrConfig {
    /// Report working, blocked and idle to Herdr when inside Herdr. The default is true.
    pub report_state: bool,
    /// Discover live terminal agents in the fleet.
    pub enabled: bool,
    /// Terminal manager executable.
    pub binary: PathBuf,
    /// Discovery period in milliseconds.
    pub poll_interval_ms: u64,
    /// Optional workspace identifier.
    pub workspace_filter: Option<String>,
}

/// Theme settings from `[theme]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeConfig {
    /// Light or dark preference. The default is auto.
    pub mode: ThemeMode,
    /// Follow the Hermes skin. The default is true.
    pub follow_hermes_skin: bool,
    /// Colour depth preference. The default is auto.
    pub colors: ColorsSetting,
}

/// Interface settings from `[ui]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiConfig {
    /// Reduced motion. The default is false.
    pub reduced_motion: bool,
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

/// Default seconds a finished turn must have run to alert.
pub const DEFAULT_DONE_AFTER_SECONDS: u64 = 20;
/// Default line count for paste collapse.
pub const DEFAULT_PASTE_COLLAPSE_LINES: usize = 5;

/// Default character count for paste collapse.
pub const DEFAULT_PASTE_COLLAPSE_CHARS: usize = 2000;

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
            editor: EditorConfig {
                command: String::new(),
            },
            history: HistoryConfig { save: true },
            notifications: NotificationsConfig {
                bell: vec![AlertKind::NeedsYou, AlertKind::Failed],
                desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
                done_after_seconds: DEFAULT_DONE_AFTER_SECONDS,
                fleet_needs_you: FleetScope::All,
                fleet_finished: FleetScope::Watched,
            },
            herdr: HerdrConfig {
                report_state: true,
                enabled: false,
                binary: PathBuf::from("herdr"),
                poll_interval_ms: 2_000,
                workspace_filter: None,
            },
            theme: ThemeConfig {
                mode: ThemeMode::Auto,
                follow_hermes_skin: true,
                colors: ColorsSetting::Auto,
            },
            ui: UiConfig {
                reduced_motion: false,
            },
            composer: ComposerConfig::default(),
            fleet: crate::fleet::FleetConfig::default(),
            acp: crate::acp::AcpConfig::default(),
            panes: crate::panes::PaneSettings::default(),
            subagents: crate::subagents::SubagentsConfig::default(),
            configure: None,
            voice: VoiceConfig::default(),
        }
    }
}

impl Default for ComposerConfig {
    fn default() -> Self {
        Self {
            paste_collapse_lines: DEFAULT_PASTE_COLLAPSE_LINES,
            paste_collapse_chars: DEFAULT_PASTE_COLLAPSE_CHARS,
            vim: false,
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
    for name in [
        "keys",
        "status",
        "environment",
        "editor",
        "history",
        "notifications",
        "herdr",
        "theme",
        "ui",
        "composer",
        "fleet",
        "acp",
        "panes",
        "subagents",
        "configure",
        "voice",
    ] {
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
            "environment" => config.environment = read_environment(section, &mut notices),
            "theme" => config.theme = read_theme(section, &mut notices),
            "ui" => config.ui = read_ui(section, &mut notices),
            "composer" => config.composer = read_composer(section, &mut notices),
            "fleet" => config.fleet = read_fleet(section, &mut notices),
            "acp" => config.acp = crate::acp::parse_section(section, &mut notices),
            "panes" => config.panes = read_panes(section, &mut notices),
            "subagents" => config.subagents = read_subagents(section, &mut notices),
            "configure" => config.configure = read_configure(section, &mut notices),
            "voice" => config.voice = read_voice(section, &mut notices),
            _ => read_comforts(name, section, &mut config, &mut notices),
        }
    }
    for (name, value) in &table {
        if matches!(
            name.as_str(),
            "keys"
                | "status"
                | "environment"
                | "editor"
                | "history"
                | "notifications"
                | "herdr"
                | "theme"
                | "ui"
                | "composer"
                | "fleet"
                | "acp"
                | "panes"
                | "subagents"
                | "configure"
                | "voice"
        ) {
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

fn read_voice(section: &Table, notices: &mut Vec<String>) -> VoiceConfig {
    let mut voice = VoiceConfig::default();
    for (name, value) in section {
        match name.as_str() {
            "read_aloud" => match value.as_bool() {
                Some(enabled) => voice.read_aloud = enabled,
                None => {
                    notices.push("[voice] read_aloud must be true or false; using false.".into())
                }
            },
            "speak_max_chars" => match value.as_integer() {
                Some(count @ 100..=10_000) => {
                    voice.speak_max_chars = usize::try_from(count).unwrap_or(1_500)
                }
                _ => notices.push(
                    "[voice] speak_max_chars must be a whole number from 100 to 10000; using 1500."
                        .into(),
                ),
            },
            _ => notices.push(unknown_setting("voice", name)),
        }
    }
    voice
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

fn read_configure(section: &Table, notices: &mut Vec<String>) -> Option<(String, String)> {
    for name in section.keys() {
        if name != "provider" && name != "model" {
            notices.push(unknown_setting("configure", name));
        }
    }
    let provider = section.get("provider");
    let model = section.get("model");
    match (provider, model) {
        (None, None) => None,
        (Some(Value::String(provider)), Some(Value::String(model)))
            if !provider.trim().is_empty() && !model.trim().is_empty() =>
        {
            Some((provider.clone(), model.clone()))
        }
        _ => {
            notices.push("[configure] provider and model must both be non-empty strings; using the next available model.".into());
            None
        }
    }
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

fn read_theme(section: &Table, notices: &mut Vec<String>) -> ThemeConfig {
    let mut theme = Config::default().theme;
    for (name, value) in section {
        match name.as_str() {
            "mode" => match value.as_str() {
                Some("auto") => theme.mode = ThemeMode::Auto,
                Some("light") => theme.mode = ThemeMode::Light,
                Some("dark") => theme.mode = ThemeMode::Dark,
                _ => notices.push("[theme] mode must be auto, light or dark; using auto.".into()),
            },
            "follow_hermes_skin" => match value.as_bool() {
                Some(follow) => theme.follow_hermes_skin = follow,
                None => notices
                    .push("[theme] follow_hermes_skin must be true or false; using true.".into()),
            },
            "colors" => match value.as_str() {
                Some("auto") => theme.colors = ColorsSetting::Auto,
                Some("truecolor") => theme.colors = ColorsSetting::TrueColor,
                Some("256") => theme.colors = ColorsSetting::Ansi256,
                Some("16") => theme.colors = ColorsSetting::Ansi16,
                Some("none") => theme.colors = ColorsSetting::None,
                _ => notices.push(
                    "[theme] colors must be auto, truecolor, 256, 16 or none; using auto.".into(),
                ),
            },
            "name" => notices.push(
                "theme.name is no longer used: Talaria follows the Hermes skin (see /skin and \
                 /theme)."
                    .into(),
            ),
            _ => notices.push(unknown_setting("theme", name)),
        }
    }
    theme
}

fn read_ui(section: &Table, notices: &mut Vec<String>) -> UiConfig {
    let mut ui = Config::default().ui;
    for (name, value) in section {
        match name.as_str() {
            "reduced_motion" => match value.as_bool() {
                Some(reduced) => ui.reduced_motion = reduced,
                None => {
                    notices.push("[ui] reduced_motion must be true or false; using false.".into());
                }
            },
            _ => notices.push(unknown_setting("ui", name)),
        }
    }
    ui
}

fn read_composer(section: &Table, notices: &mut Vec<String>) -> ComposerConfig {
    let mut composer = ComposerConfig::default();
    for (name, value) in section {
        if name == "vim" {
            if let Some(enabled) = value.as_bool() {
                composer.vim = enabled;
            } else {
                notices.push(
                    "composer.vim in ~/.talaria/config.toml must be true or false; using false."
                        .into(),
                );
            }
            continue;
        }
        let fallback = match name.as_str() {
            "paste_collapse_lines" => DEFAULT_PASTE_COLLAPSE_LINES,
            "paste_collapse_chars" => DEFAULT_PASTE_COLLAPSE_CHARS,
            _ => {
                notices.push(unknown_setting("composer", name));
                continue;
            }
        };
        match value.as_integer() {
            Some(number) if number >= 0 => {
                let count = usize::try_from(number).unwrap_or(fallback);
                if name == "paste_collapse_lines" {
                    composer.paste_collapse_lines = count;
                } else {
                    composer.paste_collapse_chars = count;
                }
            }
            _ => notices.push(format!(
                "composer.{name} in {FILE} must be a non-negative whole number; using {fallback}."
            )),
        }
    }
    composer
}

/// Whether startup should open the composer with vim on.
#[must_use]
pub fn composer_starts_with_vim(config: &Config) -> bool {
    config.composer.vim
}

/// Whether a history-search key is redo for a focused vim composer.
///
#[must_use]
pub fn vim_redo_claim(
    history_search: bool,
    composer_focused: bool,
    vim_normal_or_visual: bool,
) -> bool {
    history_search && composer_focused && vim_normal_or_visual
}

fn read_subagents(section: &Table, notices: &mut Vec<String>) -> crate::subagents::SubagentsConfig {
    let mut config = crate::subagents::SubagentsConfig::default();
    for key in ["poll_s", "tail_poll_ms", "dock_rows"] {
        let Some(value) = section.get(key) else {
            continue;
        };
        let (min, max, fallback) = match key {
            "poll_s" => (2, 60, 5),
            "tail_poll_ms" => (1000, 10000, 2000),
            _ => (0, 10, 3),
        };
        let number = match value.as_integer() {
            Some(number) if (min..=max).contains(&number) => number as u64,
            _ => {
                notices.push(format!(
                    "[subagents] {key} must be an integer from {min} to {max}; using {fallback}."
                ));
                fallback as u64
            }
        };
        match key {
            "poll_s" => config.poll_s = number,
            "tail_poll_ms" => config.tail_poll_ms = number,
            _ => config.dock_rows = number as u16,
        }
    }
    for key in section
        .keys()
        .filter(|key| !matches!(key.as_str(), "poll_s" | "tail_poll_ms" | "dock_rows"))
    {
        notices.push(unknown_setting("subagents", key));
    }
    config
}

fn read_panes(section: &Table, notices: &mut Vec<String>) -> crate::panes::PaneSettings {
    use crate::panes::{DirectionSetting, MultiplexerSetting, PaneSettings};
    let mut settings = PaneSettings::default();
    for (name, value) in section {
        match name.as_str() {
            "multiplexer" => {
                settings.multiplexer = match value.as_str() {
                    Some("auto") => MultiplexerSetting::Auto,
                    Some("herdr") => MultiplexerSetting::Herdr,
                    Some("tmux") => MultiplexerSetting::Tmux,
                    Some("off") => MultiplexerSetting::Off,
                    _ => {
                        notices.push(
                            "[panes] multiplexer must be auto, herdr, tmux or off; using auto."
                                .into(),
                        );
                        MultiplexerSetting::Auto
                    }
                }
            }
            "direction" => {
                settings.direction = match value.as_str() {
                    Some("auto") => DirectionSetting::Auto,
                    Some("right") => DirectionSetting::Right,
                    Some("down") => DirectionSetting::Down,
                    _ => {
                        notices.push(
                            "[panes] direction must be auto, right or down; using auto.".into(),
                        );
                        DirectionSetting::Auto
                    }
                }
            }
            "focus" => match value.as_bool() {
                Some(focus) => settings.focus = focus,
                None => notices.push("[panes] focus must be true or false; using false.".into()),
            },
            _ => notices.push(unknown_setting("panes", name)),
        }
    }
    settings
}

fn read_fleet(section: &Table, notices: &mut Vec<String>) -> crate::fleet::FleetConfig {
    let mut fleet = crate::fleet::FleetConfig::default();
    for (name, value) in section {
        match name.as_str() {
            "enabled" => match value.as_bool() {
                Some(enabled) => fleet.enabled = enabled,
                None => {
                    notices.push("[fleet] enabled must be true or false; using true.".into());
                }
            },
            "hot_poll_ms" => {
                fleet.hot_poll = std::time::Duration::from_millis(
                    u64::try_from(read_fleet_whole(
                        value,
                        1000,
                        10_000,
                        2000,
                        "hot_poll_ms",
                        notices,
                    ))
                    .unwrap_or(2000),
                );
            }
            "cold_poll_ms" => {
                fleet.cold_poll = std::time::Duration::from_millis(
                    u64::try_from(read_fleet_whole(
                        value,
                        2000,
                        30_000,
                        4000,
                        "cold_poll_ms",
                        notices,
                    ))
                    .unwrap_or(4000),
                );
            }
            "backstop_s" => {
                fleet.backstop = std::time::Duration::from_secs(
                    u64::try_from(read_fleet_whole(value, 10, 300, 30, "backstop_s", notices))
                        .unwrap_or(30),
                );
            }
            _ => notices.push(unknown_setting("fleet", name)),
        }
    }
    fleet
}

/// A whole number in `min..=max`. Out of range is clamped. Anything else uses `fallback`.
fn read_fleet_whole(
    value: &Value,
    min: i64,
    max: i64,
    fallback: i64,
    key: &str,
    notices: &mut Vec<String>,
) -> i64 {
    match value.as_integer() {
        Some(number) if (min..=max).contains(&number) => number,
        Some(number) => {
            let clamped = number.clamp(min, max);
            notices.push(format!(
                "[fleet] {key} must be a whole number from {min} to {max}; using {clamped}."
            ));
            clamped
        }
        None => {
            notices.push(format!(
                "[fleet] {key} must be a whole number from {min} to {max}; using {fallback}."
            ));
            fallback
        }
    }
}

fn unknown_setting(table: &str, key: &str) -> String {
    format!("Ignoring {table}.{key} in {FILE}: it is not a known setting.")
}

fn read_comforts(table: &str, section: &Table, config: &mut Config, notices: &mut Vec<String>) {
    for (key, value) in section {
        match (table,key.as_str()) {
            ("editor","command") => match value.as_str() {
                Some(text) => config.editor.command=text.into(),
                None => notices.push(format!("editor.command in {FILE} must be a text string; using the default.")),
            },
            ("history","save") | ("herdr","report_state") => match value.as_bool() {
                Some(enabled) => if table=="history" { config.history.save=enabled; } else { config.herdr.report_state=enabled; },
                None => notices.push(format!("{table}.{key} in {FILE} must be true or false; using true.")),
            },
            ("herdr", "enabled") => match value.as_bool() {
                Some(enabled) => config.herdr.enabled = enabled,
                None => notices.push("[herdr] enabled must be true or false; using false.".into()),
            },
            ("herdr", "binary") => match value.as_str().filter(|text| !text.trim().is_empty()) {
                Some(text) => config.herdr.binary = PathBuf::from(text),
                None => notices.push("[herdr] binary must be nonempty text; using herdr.".into()),
            },
            ("herdr", "poll_interval_ms") => match value.as_integer() {
                Some(value) if (50..=300_000).contains(&value) => config.herdr.poll_interval_ms = value as u64,
                _ => notices.push("[herdr] poll_interval_ms must be a whole number from 50 to 300000; using 2000.".into()),
            },
            ("herdr", "workspace_filter") => match value.as_str().filter(|text| !text.trim().is_empty()) {
                Some(text) => config.herdr.workspace_filter = Some(text.to_owned()),
                None => notices.push("[herdr] workspace_filter must be nonempty text; using all workspaces.".into()),
            },
            ("notifications","done_after_seconds") => match value.as_integer() {
                Some(n) if (0..=86400).contains(&n) => config.notifications.done_after_seconds=n as u64,
                _ => notices.push("[notifications] done_after_seconds must be a whole number from 0 to 86400; using 20.".into()),
            },
            ("notifications", "bell" | "desktop") => {
                let Some(items)=value.as_array().filter(|items| items.iter().all(|v|v.as_str().is_some())) else {
                    notices.push(format!("[notifications] {key} must be a list of notification kinds; using the default.")); continue;
                };
                let mut kinds=Vec::new();
                for item in items {
                    let text=item.as_str().unwrap_or_default();
                    let kind=match text { "needs_you"=>AlertKind::NeedsYou,"done"=>AlertKind::Done,"failed"=>AlertKind::Failed,_=>{notices.push(format!("Unknown notification kind \"{text}\" in [notifications] {key}; it is ignored."));continue;} };
                    if !kinds.contains(&kind) { kinds.push(kind); }
                }
                if key=="bell" { config.notifications.bell=kinds; } else { config.notifications.desktop=kinds; }
            }
            ("notifications", "fleet_needs_you" | "fleet_finished") => {
                let scope = match value.as_str() {
                    Some("all") => Some(FleetScope::All),
                    Some("watched") => Some(FleetScope::Watched),
                    Some("off") => Some(FleetScope::Off),
                    _ => None,
                };
                match (key.as_str(), scope) {
                    ("fleet_needs_you", Some(scope)) => config.notifications.fleet_needs_you = scope,
                    ("fleet_needs_you", None) => notices.push(
                        "[notifications] fleet_needs_you must be \"all\", \"watched\" or \"off\"; using \"all\"."
                            .into(),
                    ),
                    ("fleet_finished", Some(scope)) => config.notifications.fleet_finished = scope,
                    ("fleet_finished", None) => notices.push(
                        "[notifications] fleet_finished must be \"all\", \"watched\" or \"off\"; using \"watched\"."
                            .into(),
                    ),
                    _ => {}
                }
            }
            _ => notices.push(unknown_setting(table,key)),
        }
    }
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

impl HerdrConfig {
    /// Discovery settings for the terminal-agent service.
    pub fn fleet_config(&self) -> crate::herdr::HerdrConfig {
        crate::herdr::HerdrConfig {
            enabled: self.enabled,
            binary: self.binary.clone(),
            poll_interval_ms: self.poll_interval_ms,
            workspace_filter: self.workspace_filter.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ColorsSetting, Config, DEFAULT_STATUS_INTERVAL_SECONDS, ThemeMode, load_file,
        load_user_config, parse, user_config_path,
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
    fn configure_model_requires_a_valid_pair() {
        let loaded = parse("[configure]\nprovider = 'example'\nmodel = 'model'\n");
        assert!(loaded.notices.is_empty());
        assert_eq!(
            loaded.config.configure,
            Some(("example".into(), "model".into()))
        );
        for text in [
            "[configure]\nprovider = 'example'\n",
            "[configure]\nprovider = 4\nmodel = 'model'\n",
            "[configure]\nprovider = 'example'\nmodel = '  '\n",
        ] {
            let loaded = parse(text);
            assert!(loaded.config.configure.is_none());
            assert_eq!(loaded.notices.len(), 1);
        }
        assert!(parse("[configure]\n").notices.is_empty());
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
        // #227: [theme] and [ui] are known tables now, so two typo tables stand in.
        let loaded =
            parse("[theme_typo]\n\n[ui_typo]\n\n[status]\ncolour = 1\n\n[environment]\nx = 1\n");
        assert_eq!(
            loaded.notices,
            vec![
                "Ignoring status.colour in ~/.talaria/config.toml: it is not a known setting."
                    .to_string(),
                "Ignoring environment.x in ~/.talaria/config.toml: it is not a known setting."
                    .to_string(),
                "Ignoring [theme_typo] in ~/.talaria/config.toml: Talaria does not read it yet."
                    .to_string(),
                "Ignoring [ui_typo] in ~/.talaria/config.toml: Talaria does not read it yet."
                    .to_string(),
            ]
        );
        assert_eq!(loaded.config.status.interval_seconds, 5);
        assert!(loaded.config.environment.allowlist.is_empty());
    }

    #[test]
    fn composer_bad_value_uses_the_default() {
        let lines = "composer.paste_collapse_lines in ~/.talaria/config.toml must be a \
                     non-negative whole number; using 5.";
        let chars = "composer.paste_collapse_chars in ~/.talaria/config.toml must be a \
                     non-negative whole number; using 2000.";
        for text in [
            "[composer]\npaste_collapse_lines = -1\n",
            "[composer]\npaste_collapse_lines = 1.5\n",
            "[composer]\npaste_collapse_lines = \"x\"\n",
        ] {
            let loaded = parse(text);
            assert_eq!(loaded.notices, vec![lines.to_string()], "{text}");
            assert_eq!(loaded.config.composer.paste_collapse_lines, 5);
        }
        let loaded = parse("[composer]\npaste_collapse_chars = -1\ncolour = 1\n");
        assert_eq!(
            loaded.notices,
            vec![
                "Ignoring composer.colour in ~/.talaria/config.toml: it is not a known setting."
                    .to_string(),
                chars.to_string(),
            ]
        );
        assert_eq!(loaded.config.composer.paste_collapse_chars, 2000);
        let loaded = parse("[composer]\npaste_collapse_lines = 0\npaste_collapse_chars = 0\n");
        assert!(loaded.notices.is_empty());
        assert_eq!(loaded.config.composer.paste_collapse_lines, 0);
        assert_eq!(loaded.config.composer.paste_collapse_chars, 0);
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

    #[test]
    fn theme_and_ui_tables() {
        let loaded = parse("");
        assert!(loaded.notices.is_empty());
        assert_eq!(loaded.config.theme.mode, ThemeMode::Auto);
        assert!(loaded.config.theme.follow_hermes_skin);
        assert_eq!(loaded.config.theme.colors, ColorsSetting::Auto);
        assert!(!loaded.config.ui.reduced_motion);

        let loaded = parse(
            "[theme]\nmode = \"dark\"\nfollow_hermes_skin = false\ncolors = \"256\"\n\n\
             [ui]\nreduced_motion = true\n",
        );
        assert!(loaded.notices.is_empty());
        assert_eq!(loaded.config.theme.mode, ThemeMode::Dark);
        assert!(!loaded.config.theme.follow_hermes_skin);
        assert_eq!(loaded.config.theme.colors, ColorsSetting::Ansi256);
        assert!(loaded.config.ui.reduced_motion);

        let loaded = parse("[theme]\nmode = \"purple\"\n");
        assert_eq!(
            loaded.notices,
            vec!["[theme] mode must be auto, light or dark; using auto.".to_string()]
        );
        assert_eq!(loaded.config.theme.mode, ThemeMode::Auto);

        let loaded = parse("[theme]\nfollow_hermes_skin = 7\n");
        assert_eq!(
            loaded.notices,
            vec!["[theme] follow_hermes_skin must be true or false; using true.".to_string()]
        );
        assert!(loaded.config.theme.follow_hermes_skin);

        let loaded = parse("[theme]\ncolors = \"rainbow\"\n");
        assert_eq!(
            loaded.notices,
            vec![
                "[theme] colors must be auto, truecolor, 256, 16 or none; using auto.".to_string()
            ]
        );
        assert_eq!(loaded.config.theme.colors, ColorsSetting::Auto);

        let loaded = parse("[ui]\nreduced_motion = \"sometimes\"\n");
        assert_eq!(
            loaded.notices,
            vec!["[ui] reduced_motion must be true or false; using false.".to_string()]
        );
        assert!(!loaded.config.ui.reduced_motion);

        let loaded = parse("[theme]\nname = \"ares\"\n");
        assert_eq!(
            loaded.notices,
            vec![
                "theme.name is no longer used: Talaria follows the Hermes skin (see /skin and /theme)."
                    .to_string()
            ]
        );

        let loaded = parse("[theme]\nunknown_key = 1\n");
        assert_eq!(
            loaded.notices,
            vec![
                "Ignoring theme.unknown_key in ~/.talaria/config.toml: it is not a known setting."
                    .to_string()
            ]
        );
    }
}
