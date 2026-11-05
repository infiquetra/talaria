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
    file_commands: VecDeque<String>,
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
        }
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

    /// The polarity in force: the session override, else the start-up decision.
    pub fn polarity(&self) -> (Polarity, PolaritySource) {
        match self.override_ {
            Some(polarity) => (polarity, PolaritySource::Command),
            None => self.startup_polarity,
        }
    }

    /// Set or clear the session polarity override. True when the theme changes.
    pub fn set_polarity_override(&mut self, override_: Option<Polarity>) -> bool {
        let before = self.polarity();
        self.override_ = override_;
        before != self.polarity()
    }

    /// The session polarity override, if any.
    pub fn polarity_override(&self) -> Option<Polarity> {
        self.override_
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
        let Some(files) = &mut self.files else {
            return false;
        };
        let before = files.selected_id().map(str::to_owned);
        if !files.set_profile(profile) {
            return false;
        }
        before.as_deref() != files.selected_id()
    }

    pub fn queue_file_command(&mut self, argument: String) {
        self.file_commands.push_back(argument);
    }

    /// Start one queued command from the last adopted selection.
    pub fn begin_file_request(&mut self) -> Option<(u64, ThemeFileSession, String)> {
        if self.file_loading {
            return None;
        }
        let files = self.files.clone()?;
        let argument = self.file_commands.pop_front()?;
        self.file_loading = true;
        self.file_generation = self.file_generation.wrapping_add(1);
        Some((self.file_generation, files, argument))
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
        mut files: ThemeFileSession,
    ) -> Option<AppearanceChange> {
        if generation != self.file_generation {
            return None;
        }
        let changed = self.files.as_ref().map(|old| &old.active) != Some(&files.active);
        // A connection may change profiles while the file is read. Keep that newer identity;
        // a queued profile refresh then inherits any session choice this command produced.
        if let Some(current) = &self.files {
            files.profile.clone_from(&current.profile);
        }
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
                    _ => None,
                }
            }
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
        let previous = if validation.is_none() {
            match configured_skin(client, session).await {
                Ok(value) => Some(value),
                Err(CallError::NotConnected) => return vec![NOT_REACHED.to_owned()],
                Err(_) => {
                    return vec![
                        "Talaria could not read the previous skin, so nothing was changed."
                            .to_owned(),
                    ];
                }
            }
        } else {
            None
        };
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
                if validation == Some(true) {
                    lines.push(success);
                    lines.push(format!(
                        "Talaria could not fetch the new skin ({why}). Run /skin to refresh."
                    ));
                } else {
                    lines.push(format!("The skin may have been saved, but Talaria could not verify it ({why}). Run /skin to refresh."));
                }
                return lines;
            }
        };
        let prior_state = self.state.clone();
        let change = self.update(AppearanceInput::Described {
            payload,
            requested: Some(value.clone()),
        });
        let unknown = matches!(change, AppearanceChange::UnknownSkin { .. });
        let unverified = validation.is_none() && self.skin().is_none();
        match (self.skin(), change) {
            (_, AppearanceChange::UnknownSkin { requested, .. }) if previous.is_some() => {
                let previous = previous.as_deref().unwrap_or("default");
                let mut params = Map::new();
                if let Some(session) = session {
                    params.insert("session_id".to_owned(), json!(session));
                }
                params.insert("key".to_owned(), json!("skin"));
                params.insert("value".to_owned(), json!(previous));
                match call(client, "config.set", Value::Object(params)).await {
                    Ok(result) if result.get("value").and_then(Value::as_str) == Some(previous) => {
                        self.state = prior_state;
                        lines.push(format!("The skin was not kept. Hermes has no skin named \"{requested}\", and the list of skins could not be fetched. The profile is still \"{previous}\"."));
                    }
                    _ => lines.push(format!("Hermes has no skin named \"{requested}\". Talaria could not confirm restoration of \"{previous}\"; run /skin to check.")),
                }
            }
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
        if unverified {
            lines.insert(0, "The skin may have been saved, but Talaria could not verify it. Run /skin to check.".to_owned());
        } else if !unknown {
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

async fn configured_skin(client: &Client, session: Option<&str>) -> Result<String, CallError> {
    let mut params = Map::new();
    params.insert("key".to_owned(), json!("skin"));
    if let Some(session) = session {
        params.insert("session_id".to_owned(), json!(session));
    }
    let result = call(client, "config.get", Value::Object(params)).await?;
    result
        .get("value")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .ok_or(CallError::Encode)
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
pub const THEME_USAGE: &str = "Usage: /theme [light|dark|auto|use [<id>]|reload]";

/// The selected user theme file for one application run.
#[derive(Clone, Debug)]
pub struct ThemeFileSession {
    config_path: PathBuf,
    theme: crate::config::ThemeConfig,
    profile: String,
    session_file: Option<String>,
    active: Option<(String, crate::theme_file::ThemeFile)>,
}

impl ThemeFileSession {
    pub fn new(config: PathBuf, theme: crate::config::ThemeConfig, profile: &str) -> Self {
        Self {
            config_path: config,
            theme,
            profile: profile.to_owned(),
            session_file: None,
            active: None,
        }
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.session_file
            .as_deref()
            .or_else(|| self.theme.profiles.get(&self.profile).map(String::as_str))
            .or(self.theme.file.as_deref())
    }
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_ref().map(|(id, _)| id.as_str())
    }
    pub fn file(&self) -> Option<&crate::theme_file::ThemeFile> {
        self.active.as_ref().map(|(_, file)| file)
    }
    pub fn startup(&mut self) -> Vec<String> {
        if let Err(error) = self.refresh() {
            return vec![error];
        }
        self.active_id()
            .and_then(|id| self.reserved_notice(id))
            .into_iter()
            .collect()
    }
    fn reserved_notice(&self, id: &str) -> Option<String> {
        crate::theme_file::builtin(id)?;
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
            ["use", id] if crate::theme_file::valid_id(id) => match self.load_selection(id) {
                Ok(file) => {
                    self.session_file = Some((*id).to_owned());
                    self.active = Some(((*id).to_owned(), file));
                    Ok(())
                }
                Err(error) => Err(error),
            },
            ["use"] => {
                self.session_file = None;
                self.refresh()
            }
            ["reload"] => self.refresh(),
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
        if let Some(notice) = self.active_id().and_then(|id| self.reserved_notice(id)) {
            lines.push(notice);
        }
        (lines, change)
    }

    fn refresh(&mut self) -> Result<(), String> {
        self.active = match self.selected_id() {
            Some(id) => Some((id.to_owned(), self.load_selection(id)?)),
            None => None,
        };
        Ok(())
    }

    fn load_selection(&self, id: &str) -> Result<crate::theme_file::ThemeFile, String> {
        if let Some(file) = crate::theme_file::builtin(id) {
            return Ok(file);
        }
        if matches!(id, "dark" | "light") {
            return Err(format!("{id} is a base for extends, not a theme."));
        }
        crate::theme_file::load(&self.config_path, id)
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
}

impl AppearanceService {
    /// The inputs the screen resolves the theme from.
    pub fn view(&self) -> AppearanceView<'_> {
        AppearanceView {
            skin: self.skin(),
            skipped: self.skipped(),
            polarity: self.polarity(),
            file: self.files.as_ref().and_then(ThemeFileSession::file),
            file_id: self.files.as_ref().and_then(ThemeFileSession::active_id),
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
