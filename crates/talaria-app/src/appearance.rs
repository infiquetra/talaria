//! Appearance effects: skin commands, theme reports, and the start-up probe colours.
//!
//! The service holds the skin state; the terminal holder draws. Behavior contract: issue 227.
//!
//! Requests are built by hand with exactly the keys Talaria sends and go out through
//! `Client::call_raw`, as the session calls do. The describe result's `skin` is read as raw JSON,
//! so a colour that is not a string (an unquoted `#` in a skin file reads as `null`) is skipped
//! rather than failing the whole result.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::Client;
use hermes_client::rpc::{CallError, CallOptions, RpcError};
use serde_json::{Map, Value, json};
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, AppearanceState, ColorDepth, NotAppliedReason, Polarity,
    PolaritySource, Skin, SkippedValue, TerminalColors, ThemeReport,
};

use crate::theme_file::selection::{self, Background};

/// How long the reconnect describe waits before its one retry.
const DESCRIBE_RETRY: Duration = Duration::from_secs(1);

/// The usage line for a bad skin name.
pub const SKIN_USAGE: &str = "Usage: /skin [name]. A skin name is letters, digits, dot, dash or \
                              underscore, up to 64 characters.";

/// Shown when `config.set` never reached Hermes.
const NOT_REACHED: &str = "Talaria could not reach Hermes, so nothing was changed. Try again \
                           when the status line shows it is connected.";

/// The appearance state for one run.
#[derive(Debug, Clone)]
pub struct AppearanceService {
    state: AppearanceState,
    startup_colors: TerminalColors,
    startup_polarity: (Polarity, PolaritySource),
    override_: Option<Polarity>,
    /// The newest connection generation, shared by clones so an older reconnect describe drops
    /// its retry when a newer connection is up.
    generation: Arc<AtomicU64>,
    files: Option<ThemeFileSession>,
    file_generation: u64,
    file_loading: bool,
    file_commands: VecDeque<(String, ThemeFileWork)>,
    /// What the `/theme` selector shows in place of the file or background, while it is open.
    preview: Option<ThemePreview>,
}

/// A look the `/theme` selector shows before it is chosen. Nothing else changes while it shows.
#[derive(Clone, Debug)]
pub enum ThemePreview {
    /// This file, or no file, in place of the active one.
    File(Option<(String, crate::theme_file::ThemeFile)>),
    /// This background, or the start-up decision for `None`, in place of the one in force.
    Background(Option<Polarity>),
}

/// Local theme work serialized by the application loop.
#[derive(Clone, Debug)]
pub(crate) enum ThemeFileWork {
    Command(String),
    ProfileChanged,
}

impl AppearanceService {
    /// A service with no skin, capturing the start-up probe colours and the
    /// start-up polarity once. `/theme auto` restores that polarity.
    pub fn new(
        startup_colors: TerminalColors,
        startup_polarity: (Polarity, PolaritySource),
    ) -> Self {
        Self {
            state: AppearanceState::default(),
            startup_colors,
            startup_polarity,
            override_: None,
            generation: Arc::new(AtomicU64::new(0)),
            files: None,
            file_generation: 0,
            file_loading: false,
            file_commands: VecDeque::new(),
            preview: None,
        }
    }

    /// Show this look until it is replaced or cleared; the session's choices stay as they are.
    pub fn set_theme_preview(&mut self, preview: Option<ThemePreview>) {
        self.preview = preview;
    }

    /// The captured start-up probe colours. Nothing else writes them.
    pub fn startup_colors(&self) -> TerminalColors {
        self.startup_colors
    }

    /// Fold one input into the skin state.
    pub fn update(&mut self, input: AppearanceInput) -> AppearanceChange {
        self.state.update(input)
    }

    /// The current skin, if any.
    pub fn skin(&self) -> Option<&Skin> {
        self.state.skin()
    }

    /// The current skin's skipped values.
    pub fn skipped(&self) -> &[SkippedValue] {
        self.state.skipped()
    }

    /// The skin state, to hand back from a command run on a clone.
    pub fn skin_state(&self) -> &AppearanceState {
        &self.state
    }

    /// Take the skin state a command produced on a clone. A command's describe is at least as
    /// new as anything applied meanwhile.
    pub fn adopt(&mut self, state: AppearanceState) -> AppearanceChange {
        if state == self.state {
            return AppearanceChange::Unchanged;
        }
        self.state = state;
        AppearanceChange::Changed
    }

    /// The polarity in force, including the active profile's saved preference.
    pub fn polarity(&self) -> (Polarity, PolaritySource) {
        match self.override_ {
            Some(polarity) => (polarity, PolaritySource::Command),
            None => self.files.as_ref().map_or(self.startup_polarity, |files| {
                files.polarity(self.startup_polarity)
            }),
        }
    }

    pub(crate) fn startup_polarity(&self) -> (Polarity, PolaritySource) {
        self.startup_polarity
    }

    /// Set or clear the session polarity override. True when the theme changes.
    pub fn set_polarity_override(&mut self, override_: Option<Polarity>) -> bool {
        let before = self.polarity();
        self.override_ = override_;
        before != self.polarity()
    }

    /// The chosen background, if any.
    pub fn polarity_override(&self) -> Option<Polarity> {
        self.override_.or_else(|| {
            self.files
                .as_ref()
                .and_then(|files| files.background.map(Polarity::from))
        })
    }

    /// Adopt the selection prepared before the first frame.
    pub fn set_theme_files(&mut self, files: ThemeFileSession) {
        self.files = Some(files);
    }

    pub fn theme_files(&self) -> Option<&ThemeFileSession> {
        self.files.as_ref()
    }

    /// Update profile selection without reading a file on the loop.
    pub fn set_file_profile(&mut self, profile: &str) -> bool {
        self.files
            .as_mut()
            .is_some_and(|files| files.set_profile(profile))
    }

    pub(crate) fn queue_file_work(&mut self, work: ThemeFileWork) {
        if let Some(files) = &self.files {
            self.file_commands.push_back((files.profile.clone(), work));
        }
    }

    /// Start one queued command from the last adopted selection.
    pub(crate) fn begin_file_request(&mut self) -> Option<(u64, ThemeFileSession, ThemeFileWork)> {
        if self.file_loading {
            return None;
        }
        let mut files = self.files.clone()?;
        let (profile, work) = self.file_commands.pop_front()?;
        files.set_profile(&profile);
        self.file_loading = true;
        self.file_generation = self.file_generation.wrapping_add(1);
        Some((self.file_generation, files, work))
    }

    pub(crate) fn file_loading(&self) -> bool {
        self.file_loading
    }

    pub fn file_generation(&self) -> u64 {
        self.file_generation
    }

    pub fn finish_file_request(&mut self, generation: u64) -> bool {
        if generation != self.file_generation {
            return false;
        }
        self.file_loading = false;
        true
    }

    pub fn adopt_file_result(
        &mut self,
        generation: u64,
        files: ThemeFileSession,
    ) -> Option<AppearanceChange> {
        let current = self.files.as_ref()?;
        if generation != self.file_generation || files.profile != current.profile {
            return None;
        }
        let changed = current.active != files.active || current.background != files.background;
        self.files = Some(files);
        Some(if changed {
            AppearanceChange::Changed
        } else {
            AppearanceChange::Unchanged
        })
    }

    /// The newest connection generation a reconnect describe was started for.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Run `/theme <arg>`: set or clear the session polarity override. Returns the
    /// notice lines and whether the theme changed; sends nothing to Hermes.
    ///
    /// A bare `/theme` returns no lines: its report needs the resolved theme, which the screen
    /// holds.
    pub fn theme_command(&mut self, arg: &str) -> (Vec<String>, AppearanceChange) {
        let changed = |changed: bool| {
            if changed {
                AppearanceChange::Changed
            } else {
                AppearanceChange::Unchanged
            }
        };
        match arg.trim() {
            "" => (Vec::new(), AppearanceChange::Unchanged),
            "light" | "dark" => {
                let polarity = if arg.trim() == "light" {
                    Polarity::Light
                } else {
                    Polarity::Dark
                };
                let change = changed(self.set_polarity_override(Some(polarity)));
                (vec![theme_set_line(polarity)], change)
            }
            "auto" => {
                let change = changed(self.set_polarity_override(None));
                let (polarity, source) = self.startup_polarity;
                (
                    vec![background_restored_line(polarity, &source.label())],
                    change,
                )
            }
            _ => (vec![THEME_USAGE.to_owned()], AppearanceChange::Unchanged),
        }
    }

    /// Run `/skin` or `/skin <name>`, returning the notice lines.
    ///
    /// With a name, check Hermes's completions before saving and describing the skin. If the
    /// completion is inconclusive, preserve the previous setting for a possible rollback. Without
    /// a name, read and describe the current setting and list the available skins.
    pub async fn skin_command(
        &mut self,
        client: &Client,
        name: Option<&str>,
        session: Option<&str>,
        profile: &str,
    ) -> Vec<String> {
        match name {
            Some(name) => self.set_skin(client, name.trim(), session, profile).await,
            None => self.show_skin(client, session, profile).await,
        }
    }

    async fn set_skin(
        &mut self,
        client: &Client,
        name: &str,
        session: Option<&str>,
        profile: &str,
    ) -> Vec<String> {
        if !valid_skin_name(name) {
            return vec![SKIN_USAGE.to_owned()];
        }
        let exact = skin_names(client, &format!("/skin {name}"), session).await;
        let validation = match exact {
            Ok(names) if names.iter().any(|item| item == name) => Some(true),
            Ok(_) => {
                // Hermes omits an exact completion, so ask for the preceding prefix.
                let prefix = &name[..name.len() - 1];
                match skin_names(client, &format!("/skin {prefix}"), session).await {
                    Ok(names) if names.iter().any(|item| item == name) => Some(true),
                    Ok(names) if names.len() < 30 => Some(false),
                    Err(CallError::NotConnected) => return vec![NOT_REACHED.to_owned()],
                    _ => None,
                }
            }
            Err(CallError::NotConnected) => return vec![NOT_REACHED.to_owned()],
            Err(_) => None,
        };
        if validation == Some(false) {
            let mut lines = vec![format!("Hermes has no skin named \"{name}\".")];
            match skin_names(client, "/skin ", session).await {
                Ok(names) => lines.push(skins_line(&names, None)),
                Err(_) => lines.push("The list of skins could not be fetched.".to_owned()),
            }
            return lines;
        }
        if validation.is_none() {
            return vec!["The skin could not be checked, so nothing was changed.".to_owned()];
        }
        let mut params = Map::new();
        if let Some(session) = session {
            params.insert("session_id".to_owned(), json!(session));
        }
        params.insert("key".to_owned(), json!("skin"));
        params.insert("value".to_owned(), json!(name));
        let value = match call(client, "config.set", Value::Object(params)).await {
            Ok(result) => result
                .get("value")
                .and_then(Value::as_str)
                .map_or_else(|| name.to_owned(), str::to_owned),
            Err(CallError::Rpc(error)) => {
                let reason = reason(&error);
                let mut lines = vec![format!("Hermes refused to change the skin ({reason}).")];
                if reason == "permission_denied" {
                    lines.push("This connection may not change settings.".to_owned());
                }
                return lines;
            }
            Err(error) if not_sent(&error) => return vec![NOT_REACHED.to_owned()],
            Err(_) => {
                return vec![
                    "The skin change may or may not have been saved. Run /skin to check."
                        .to_owned(),
                ];
            }
        };
        let success = format!(
            "Skin set to \"{value}\" for profile {profile}. Hermes windows with a session open on \
             this profile change too."
        );
        let mut lines = Vec::new();
        let payload = match describe(client).await {
            Ok(payload) => payload,
            Err(error) => {
                let why = match &error {
                    CallError::Rpc(error) => reason(error),
                    error if not_sent(error) => "not connected".to_owned(),
                    _ => "no answer".to_owned(),
                };
                lines.push(success);
                lines.push(format!(
                    "Talaria could not fetch the new skin ({why}). Run /skin to refresh."
                ));
                return lines;
            }
        };
        let change = self.update(AppearanceInput::Described {
            payload,
            requested: Some(value.clone()),
        });
        let unknown = matches!(change, AppearanceChange::UnknownSkin { .. });
        match (self.skin(), change) {
            (
                _,
                AppearanceChange::UnknownSkin {
                    requested, using, ..
                },
            ) => lines.push(format!(
                "Hermes has no skin named \"{requested}\"; it is using \"{using}\"."
            )),
            (None, _) => lines.push(format!(
                "Hermes sent no skin for \"{value}\", so Talaria uses its own colours."
            )),
            (Some(skin), _) if skin.name != value => {
                lines.push(format!("Hermes calls this skin \"{}\".", skin.name));
            }
            _ => {}
        }
        if !unknown {
            lines.insert(0, success);
        }
        lines
    }

    async fn show_skin(
        &mut self,
        client: &Client,
        session: Option<&str>,
        profile: &str,
    ) -> Vec<String> {
        let read_failed = |error: CallError| match error {
            CallError::Rpc(error) => format!(
                "Talaria could not read the skin from Hermes ({}).",
                reason(&error)
            ),
            error if not_sent(&error) => {
                "Talaria could not read the skin from Hermes (not connected).".to_owned()
            }
            _ => "Hermes did not answer. Run /skin again.".to_owned(),
        };
        let mut params = Map::new();
        params.insert("key".to_owned(), json!("skin"));
        if let Some(session) = session {
            params.insert("session_id".to_owned(), json!(session));
        }
        let configured = match call(client, "config.get", Value::Object(params)).await {
            Ok(result) => result
                .get("value")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("default")
                .to_owned(),
            Err(error) => return vec![read_failed(error)],
        };
        let payload = match describe(client).await {
            Ok(payload) => payload,
            Err(error) => return vec![read_failed(error)],
        };
        self.update(AppearanceInput::Described {
            payload,
            requested: None,
        });
        let line = match self.skin() {
            None => "Skin: none. Hermes sent no skin, so Talaria uses its own colours.".to_owned(),
            Some(skin) if skin.name == "default" && configured != "default" => format!(
                "Skin: default. The profile asks for \"{configured}\", which Hermes could not \
                 find, so it uses default."
            ),
            Some(skin) if skin.name != configured => format!(
                "Skin: {} (Hermes profile {profile}, set as \"{configured}\")",
                skin.name
            ),
            Some(skin) => format!("Skin: {} (Hermes profile {profile})", skin.name),
        };
        let mut lines = vec![line];
        match skin_names(client, "/skin ", session).await {
            Ok(names) => lines.push(skins_line(
                &names,
                self.skin().map(|skin| skin.name.as_str()),
            )),
            Err(_) => lines.push("The list of skins could not be fetched.".to_owned()),
        }
        lines
    }

    /// Describe again after a reconnect, retrying once unless superseded.
    ///
    /// Any failure waits one second and tries once more, unless a newer connection has come up
    /// meanwhile (it runs its own describe). A second failure keeps the current skin quietly.
    pub async fn connection_opened(&mut self, client: &Client, generation: u64) {
        self.generation.fetch_max(generation, Ordering::SeqCst);
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(DESCRIBE_RETRY).await;
                if self.generation() > generation {
                    return;
                }
            }
            if let Ok(payload) = describe(client).await {
                self.update(AppearanceInput::Handshake {
                    ready: None,
                    describe: Some(payload).filter(|payload| !payload.is_null()),
                });
                return;
            }
        }
    }

    /// Write `name` as the profile's skin and describe it, without the checks `/skin <name>`
    /// makes: the `/skin` selector offers only names Hermes listed.
    pub async fn preview_skin(
        &mut self,
        client: &Client,
        name: &str,
        session: Option<&str>,
    ) -> Result<(), SkinPreviewError> {
        let mut params = Map::new();
        if let Some(session) = session {
            params.insert("session_id".to_owned(), json!(session));
        }
        params.insert("key".to_owned(), json!("skin"));
        params.insert("value".to_owned(), json!(name));
        match call(client, "config.set", Value::Object(params)).await {
            Ok(_) => {}
            Err(CallError::Rpc(error)) => {
                return Err(SkinPreviewError::Refused(format!(
                    "Hermes refused to change the skin ({})",
                    reason(&error)
                )));
            }
            Err(error) if not_sent(&error) => {
                return Err(SkinPreviewError::Refused(
                    "Talaria could not reach Hermes".to_owned(),
                ));
            }
            Err(_) => {
                return Err(SkinPreviewError::Unknown(
                    "Hermes did not answer the skin change".to_owned(),
                ));
            }
        }
        if let Ok(payload) = describe(client).await {
            self.update(AppearanceInput::Described {
                payload,
                requested: Some(name.to_owned()),
            });
        }
        Ok(())
    }
}

/// Why a skin preview stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkinPreviewError {
    /// Nothing was written.
    Refused(String),
    /// The write may have happened.
    Unknown(String),
}

/// What the `/skin` selector lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkinChoices {
    /// The profile's skin setting, `default` when it is empty.
    pub configured: String,
    /// Each skin Hermes offers, with its description.
    pub items: Vec<(String, String)>,
}

/// Read the profile's skin setting and the skins Hermes offers, with their descriptions
/// (`complete.slash` gives each item's description as `meta`).
pub async fn skin_choices(client: &Client, session: Option<&str>) -> Result<SkinChoices, String> {
    let mut params = Map::new();
    params.insert("key".to_owned(), json!("skin"));
    if let Some(session) = session {
        params.insert("session_id".to_owned(), json!(session));
    }
    let configured = call(client, "config.get", Value::Object(params))
        .await
        .map_err(|_| "Talaria could not read the skin from Hermes.".to_owned())?
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("default")
        .to_owned();
    let mut params = Map::new();
    params.insert("text".to_owned(), json!("/skin "));
    if let Some(session) = session {
        params.insert("session_id".to_owned(), json!(session));
    }
    let result = call(client, "complete.slash", Value::Object(params))
        .await
        .map_err(|_| "The list of skins could not be fetched.".to_owned())?;
    let items = result
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("text").and_then(Value::as_str)?.trim();
                    let meta = item.get("meta").and_then(Value::as_str).unwrap_or("");
                    valid_skin_name(name).then(|| (name.to_owned(), meta.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SkinChoices { configured, items })
}

/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.
fn valid_skin_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && name.len() <= 64
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// Nothing reached Hermes.
fn not_sent(error: &CallError) -> bool {
    matches!(
        error,
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout
    )
}

/// The refusal reason: `data.reason`, else the message, else `no reason given`.
fn reason(error: &RpcError) -> String {
    error
        .reason
        .clone()
        .or_else(|| Some(error.message.clone()).filter(|message| !message.is_empty()))
        .unwrap_or_else(|| "no reason given".to_owned())
}

async fn call(client: &Client, method: &str, params: Value) -> Result<Value, CallError> {
    client
        .call_raw(method, params, CallOptions { timeout: None })
        .await
}

async fn skin_names(
    client: &Client,
    text: &str,
    session: Option<&str>,
) -> Result<Vec<String>, CallError> {
    let mut params = Map::new();
    params.insert("text".to_owned(), json!(text));
    if let Some(session) = session {
        params.insert("session_id".to_owned(), json!(session));
    }
    let result = call(client, "complete.slash", Value::Object(params)).await?;
    let items = result
        .get("items")
        .and_then(Value::as_array)
        .ok_or(CallError::Encode)?;
    items
        .iter()
        .map(|item| {
            item.get("text")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(CallError::Encode)
        })
        .collect()
}

fn skins_line(names: &[String], current: Option<&str>) -> String {
    let body = names
        .iter()
        .map(|name| {
            if current == Some(name.as_str()) {
                format!("{name} (current)")
            } else {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let partial = if names.len() == 30 {
        " The list may be partial."
    } else {
        ""
    };
    format!("Skins Hermes has: {body}.{partial}")
}

/// `runtime.describe {}` and its `skin`, raw; `null` when the result has none.
async fn describe(client: &Client) -> Result<Value, CallError> {
    let result = call(client, "runtime.describe", json!({})).await?;
    Ok(result.get("skin").cloned().unwrap_or(Value::Null))
}

/// The `/theme` report lines for a resolved theme.
pub fn theme_lines(report: &ThemeReport) -> Vec<String> {
    let skin = match (&report.skin, not_applied(report)) {
        (None, _) => "none".to_owned(),
        (Some(name), Some(reason)) => {
            let why = match reason {
                NotAppliedReason::ColoursOff => "colours off",
                NotAppliedReason::FollowOff => "follow_hermes_skin = false",
                NotAppliedReason::SixteenColours => "16-colour terminal",
            };
            format!("{name} (not applied: {why})")
        }
        (Some(name), None) => format!("{name} from Hermes"),
    };
    let (polarity, polarity_source) = report.polarity;
    let (depth, depth_source) = report.depth;
    let depth = match depth {
        ColorDepth::TrueColor => "24-bit",
        ColorDepth::Ansi256 => "256",
        ColorDepth::Ansi16 => "16",
        ColorDepth::None => "off",
    };
    let (motion, motion_source) = report.motion;
    let motion = match motion {
        talaria_core::appearance::Motion::Full => "full",
        talaria_core::appearance::Motion::Reduced => "reduced",
    };
    let mut lines = vec![
        "Theme".to_owned(),
        format!("  Skin: {skin}"),
        format!(
            "  Background: {} ({})",
            polarity_word(polarity),
            polarity_source.label()
        ),
        format!("  Colours: {depth} ({})", depth_source.label()),
        format!("  Motion: {motion} ({})", motion_source.label()),
    ];
    if !report.ignored.is_empty() {
        let shown: Vec<&str> = report.ignored.iter().take(3).map(String::as_str).collect();
        lines.push(format!(
            "  Ignored skin values: {} ({})",
            report.ignored.len(),
            shown.join(", ")
        ));
    }
    lines
}

fn polarity_word(polarity: Polarity) -> &'static str {
    match polarity {
        Polarity::Light => "light",
        Polarity::Dark => "dark",
    }
}

/// The `/theme light|dark` confirmation line.
pub fn theme_set_line(polarity: Polarity) -> String {
    format!(
        "Background set to {} for this session. Lines already in scrollback keep their colours.",
        polarity_word(polarity)
    )
}

/// The `/theme auto` line for the restored polarity and its source label.
pub fn background_restored_line(polarity: Polarity, source_label: &str) -> String {
    format!(
        "Background detection restored: {} ({source_label}).",
        polarity_word(polarity)
    )
}

/// The `/theme` usage line.
pub const THEME_USAGE: &str = "Usage: /theme [report|light|dark|auto|use [<id>]|new <id>|reload]";

/// The theme configuration and saved preferences for the active profile.
#[derive(Clone, Debug)]
pub struct ThemeFileSession {
    config_path: PathBuf,
    theme: crate::config::ThemeConfig,
    profile: String,
    saved_file: Option<String>,
    background: Option<Background>,
    active: Option<(String, crate::theme_file::ThemeFile)>,
}

impl ThemeFileSession {
    pub fn new(config: PathBuf, theme: crate::config::ThemeConfig, profile: &str) -> Self {
        Self {
            config_path: config,
            theme,
            profile: profile.to_owned(),
            saved_file: None,
            background: None,
            active: None,
        }
    }

    /// The file chosen for this profile with `/theme use <id>`, if any.
    pub fn saved_choice(&self) -> Option<&str> {
        self.saved_file.as_deref()
    }

    /// The file the configuration selects for this profile, if any.
    pub fn configured_id(&self) -> Option<&str> {
        self.theme
            .profiles
            .get(&self.profile)
            .map(String::as_str)
            .or(self.theme.file.as_deref())
    }

    /// Resolve a saved background over the startup decision, honoring `TALARIA_THEME`.
    pub fn polarity(&self, startup: (Polarity, PolaritySource)) -> (Polarity, PolaritySource) {
        match self.background {
            Some(background) if startup.1 != PolaritySource::Environment => {
                (background.into(), PolaritySource::Command)
            }
            _ => startup,
        }
    }

    /// Talaria's `config.toml`, beside which the `themes` folder lives.
    pub fn config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.saved_choice().or_else(|| self.configured_id())
    }
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_ref().map(|(id, _)| id.as_str())
    }
    pub fn file(&self) -> Option<&crate::theme_file::ThemeFile> {
        self.active.as_ref().map(|(_, file)| file)
    }
    pub fn startup(&mut self) -> Vec<String> {
        self.active = None;
        self.saved_file = None;
        let mut notices = Vec::new();
        let saved = selection::read(&self.config_path, &self.profile).unwrap_or_else(|error| {
            notices.push(error);
            selection::Selection::default()
        });
        self.background = saved.background;
        if let Err(error) = self.load_choice(saved.file.as_deref()) {
            notices.push(error);
            if saved.file.is_some()
                && saved.file.as_deref() != self.configured_id()
                && let Err(error) = self.load_choice(None)
            {
                notices.push(error);
            }
        } else {
            self.saved_file = saved.file;
        }
        notices.extend(self.active_id().and_then(|id| self.reserved_notice(id)));
        notices
    }
    fn reserved_notice(&self, id: &str) -> Option<String> {
        if !crate::theme_file::reserved_id(id) {
            return None;
        }
        let parent = self
            .config_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        std::fs::symlink_metadata(parent.join("themes").join(format!("{id}.toml"))).ok()?;
        Some(format!(
            "themes/{id}.toml: reserved name; using the built-in theme."
        ))
    }
    pub fn set_profile(&mut self, profile: &str) -> bool {
        if self.profile == profile {
            return false;
        }
        self.profile = profile.to_owned();
        true
    }
    pub fn command(&mut self, argument: &str) -> (Vec<String>, AppearanceChange) {
        let words: Vec<_> = argument.split_whitespace().collect();
        let before = self.active.clone();
        let result = match words.as_slice() {
            ["use", id] if crate::theme_file::valid_id(id) => self.save_file(Some(id)),
            ["use"] => self.save_file(None),
            ["new", id] if crate::theme_file::valid_id(id) => {
                // A new file is written but not selected, and config.toml is not touched.
                let line = match crate::theme_file::write_scaffold(&self.config_path, id) {
                    Ok(_) => format!("Wrote themes/{id}.toml. Run /theme use {id}."),
                    Err(error) => error,
                };
                return (vec![line], AppearanceChange::Unchanged);
            }
            ["reload"] => self.reload(),
            _ => return (vec![THEME_USAGE.to_owned()], AppearanceChange::Unchanged),
        };
        if let Err(error) = result {
            return (vec![error], AppearanceChange::Unchanged);
        }
        let change = if self.active != before {
            AppearanceChange::Changed
        } else {
            AppearanceChange::Unchanged
        };
        let line = self.active_id().map_or_else(
            || "Theme file selection cleared; using the configured skin behaviour.".to_owned(),
            |id| format!("Theme file: themes/{id}.toml."),
        );
        let mut lines = vec![line];
        if let ["use", _] = words.as_slice() {
            lines.push("Theme choice saved for this profile.".to_owned());
        }
        if let Some(notice) = self.active_id().and_then(|id| self.reserved_notice(id)) {
            lines.push(notice);
        }
        (lines, change)
    }

    /// Apply a queued command or load a different profile's preferences.
    pub(crate) fn run_work(
        &mut self,
        work: ThemeFileWork,
        startup: (Polarity, PolaritySource),
    ) -> Vec<String> {
        let argument = match work {
            ThemeFileWork::ProfileChanged => return self.startup(),
            ThemeFileWork::Command(argument) => argument,
        };
        let background = match argument.as_str() {
            "light" => Some(Background::Light),
            "dark" => Some(Background::Dark),
            "auto" => None,
            _ => return self.command(&argument).0,
        };
        if let Err(error) = selection::save(&self.config_path, &self.profile, |saved| {
            saved.background = background;
        }) {
            return vec![error];
        }
        self.background = background;
        match background {
            Some(background) => {
                let polarity = polarity_word(background.into());
                let mut lines = vec![format!("{polarity} background saved for this profile.")];
                if startup.1 == PolaritySource::Environment {
                    lines.push("TALARIA_THEME controls the background in this window.".to_owned());
                } else {
                    lines.push("Lines already in scrollback keep their colours.".to_owned());
                }
                lines
            }
            None => vec![background_restored_line(startup.0, &startup.1.label())],
        }
    }

    fn save_file(&mut self, id: Option<&str>) -> Result<(), String> {
        let active = self.resolve_choice(id)?;
        let id = id.map(str::to_owned);
        selection::save(&self.config_path, &self.profile, |saved| {
            saved.file.clone_from(&id);
        })?;
        self.saved_file = id;
        self.active = active;
        Ok(())
    }

    fn reload(&mut self) -> Result<(), String> {
        let saved = selection::read(&self.config_path, &self.profile)?;
        self.load_choice(saved.file.as_deref())?;
        self.saved_file = saved.file;
        self.background = saved.background;
        Ok(())
    }

    fn load_choice(&mut self, saved: Option<&str>) -> Result<(), String> {
        self.active = self.resolve_choice(saved)?;
        Ok(())
    }

    fn resolve_choice(
        &self,
        saved: Option<&str>,
    ) -> Result<Option<(String, crate::theme_file::ThemeFile)>, String> {
        let Some(id) = saved.or_else(|| self.configured_id()) else {
            return Ok(None);
        };
        let file = crate::theme_file::load_selection(&self.config_path, id)?;
        Ok(Some((id.to_owned(), file)))
    }
}

/// Additional report lines for an explicitly selected file.
pub fn theme_file_lines(report: &talaria_core::appearance::ThemeFileReport) -> Vec<String> {
    let mut lines = vec![
        format!("  File: themes/{}.toml", report.file),
        format!("  Base: {}", report.base),
        format!(
            "  Hermes skin: {}",
            if report.skin_participates {
                "participates"
            } else {
                "not applied"
            }
        ),
        "  Sources:".to_owned(),
    ];
    for (token, fields) in &report.sources {
        let fields = fields
            .iter()
            .map(|(field, source)| format!("{field}={source}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("    {token}: {fields}"));
    }
    lines.extend(
        report
            .warnings
            .iter()
            .map(|warning| format!("  Warning: {warning}")),
    );
    if !report.unknown_contrast.is_empty() {
        lines.push(format!(
            "  Contrast unknown: {}",
            report.unknown_contrast.join(", ")
        ));
    }
    lines
}

/// The reason a skin is not applied, if any, from a resolved report.
pub fn not_applied(report: &ThemeReport) -> Option<NotAppliedReason> {
    report.not_applied
}

/// What the screen needs to resolve the theme: the skin and the polarity in force.
#[derive(Debug, Clone, Copy)]
pub struct AppearanceView<'a> {
    /// The current skin, if any.
    pub skin: Option<&'a Skin>,
    /// The current skin's skipped values.
    pub skipped: &'a [SkippedValue],
    /// The session override, else the start-up decision.
    pub polarity: (Polarity, PolaritySource),
    /// The last valid selected file, if any.
    pub file: Option<&'a crate::theme_file::ThemeFile>,
    /// The identifier of the last valid selected file.
    pub file_id: Option<&'a str>,
    /// The `/theme` selector is showing a look that is not in force. Rows committed meanwhile
    /// keep the look in force.
    pub previewing: bool,
}

impl AppearanceService {
    /// The inputs the screen resolves the theme from.
    pub fn view(&self) -> AppearanceView<'_> {
        let (file, file_id) = match &self.preview {
            Some(ThemePreview::File(file)) => (
                file.as_ref().map(|(_, file)| file),
                file.as_ref().map(|(id, _)| id.as_str()),
            ),
            _ => (
                self.files.as_ref().and_then(ThemeFileSession::file),
                self.files.as_ref().and_then(ThemeFileSession::active_id),
            ),
        };
        let polarity = match self.preview {
            Some(ThemePreview::Background(Some(polarity))) => (polarity, PolaritySource::Command),
            Some(ThemePreview::Background(None)) => self.startup_polarity,
            _ => self.polarity(),
        };
        AppearanceView {
            skin: self.skin(),
            skipped: self.skipped(),
            polarity,
            file,
            file_id,
            previewing: self.preview.is_some(),
        }
    }
}

/// `/skin` before the connection is up: the usage line for a bad name, otherwise the line for a
/// call that never reached Hermes.
pub fn offline_skin_lines(name: Option<&str>) -> Vec<String> {
    match name.map(str::trim) {
        Some(name) if !valid_skin_name(name) => vec![SKIN_USAGE.to_owned()],
        Some(_) => vec![NOT_REACHED.to_owned()],
        None => vec!["Talaria could not read the skin from Hermes (not connected).".to_owned()],
    }
}
