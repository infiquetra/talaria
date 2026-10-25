//! Appearance effects: skin commands, theme reports, and the start-up probe colours.
//!
//! The service holds the skin state; the terminal holder draws. Behavior contract: issue 227.
//!
//! Requests are built by hand with exactly the keys Talaria sends and go out through
//! `Client::call_raw`, as the session calls do. The describe result's `skin` is read as raw JSON,
//! so a colour that is not a string (an unquoted `#` in a skin file reads as `null`) is skipped
//! rather than failing the whole result.

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
    /// With a name, `config.set` writes the profile's Hermes setting, then `runtime.describe`
    /// shows what Hermes resolved it to. Without one, `config.get` reads the configured name and
    /// `runtime.describe` the resolved skin. Nothing is retried.
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
        let mut lines = vec![format!(
            "Skin set to \"{value}\" for profile {profile}. Hermes windows with a session open on \
             this profile change too."
        )];
        let payload = match describe(client).await {
            Ok(payload) => payload,
            Err(error) => {
                let why = match &error {
                    CallError::Rpc(error) => reason(error),
                    error if not_sent(error) => "not connected".to_owned(),
                    _ => "no answer".to_owned(),
                };
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
        match (self.skin(), change) {
            (
                _,
                AppearanceChange::UnknownSkin {
                    requested, using, ..
                },
            ) => lines.push(format!(
                "Hermes has no skin named \"{requested}\"; it is using \"{using}\". Run /skin \
                 {using} to clear the setting."
            )),
            (None, _) => lines.push(format!(
                "Hermes sent no skin for \"{value}\", so Talaria uses its own colours."
            )),
            (Some(skin), _) if skin.name != value => {
                lines.push(format!("Hermes calls this skin \"{}\".", skin.name));
            }
            _ => {}
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
        vec![line]
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
pub const THEME_USAGE: &str = "Usage: /theme [light|dark|auto]";

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
}

impl AppearanceService {
    /// The inputs the screen resolves the theme from.
    pub fn view(&self) -> AppearanceView<'_> {
        AppearanceView {
            skin: self.skin(),
            skipped: self.skipped(),
            polarity: self.polarity(),
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
