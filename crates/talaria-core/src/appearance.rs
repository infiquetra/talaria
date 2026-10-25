//! The Hermes skin as domain state: what it is, where it came from, what changed.
//!
//! Pure. Behavior contract: issue 227.

use std::collections::BTreeMap;

use hermes_contract::schemas::SkinPayload;
use serde_json::Value;

/// One skin colour map: only string values are kept.
pub type ColorMap = BTreeMap<String, String>;

/// Which map of a skin payload a skipped value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinMap {
    /// `colors`.
    Colors,
    /// `light_colors`.
    LightColors,
    /// `dark_colors`.
    DarkColors,
}

/// A colour value that was not a JSON string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedValue {
    /// Which map held it.
    pub map: SkinMap,
    /// Its key.
    pub key: String,
}

/// A Hermes skin: the name and the three colour maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skin {
    /// The skin name, empty when the payload had none.
    pub name: String,
    /// String colours.
    pub colors: ColorMap,
    /// String light-polarity colours.
    pub light_colors: ColorMap,
    /// String dark-polarity colours.
    pub dark_colors: ColorMap,
}

impl Skin {
    /// None for an empty payload or one with no name and no colours.
    pub fn from_payload(payload: &SkinPayload) -> Option<Skin> {
        serde_json::to_value(payload)
            .ok()
            .and_then(|value| Skin::from_value(&value).0)
    }

    /// Same rules over a raw JSON value; every non-string colour value is skipped
    /// and returned, in map order then key order.
    ///
    /// A colour can arrive as JSON `null` when a skin file writes an unquoted `#`, which YAML
    /// reads as a comment, so a skipped value never fails the skin.
    pub fn from_value(value: &Value) -> (Option<Skin>, Vec<SkippedValue>) {
        let mut skipped = Vec::new();
        let Some(object) = value.as_object() else {
            return (None, skipped);
        };
        let mut read = |map: SkinMap, key: &str| {
            let mut colors = ColorMap::new();
            if let Some(entries) = object.get(key).and_then(Value::as_object) {
                let mut keys: Vec<&String> = entries.keys().collect();
                keys.sort();
                for name in keys {
                    match &entries[name] {
                        Value::String(color) => {
                            colors.insert(name.clone(), color.clone());
                        }
                        _ => skipped.push(SkippedValue {
                            map,
                            key: name.clone(),
                        }),
                    }
                }
            }
            colors
        };
        let colors = read(SkinMap::Colors, "colors");
        let light_colors = read(SkinMap::LightColors, "light_colors");
        let dark_colors = read(SkinMap::DarkColors, "dark_colors");
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if name.is_empty() && colors.is_empty() && light_colors.is_empty() && dark_colors.is_empty()
        {
            return (None, skipped);
        }
        let skin = Skin {
            name,
            colors,
            light_colors,
            dark_colors,
        };
        (Some(skin), skipped)
    }
}

/// Where the current skin came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinSource {
    /// No skin.
    None,
    /// The `gateway.ready` skin.
    GatewayReady,
    /// A `runtime.describe` skin.
    RuntimeDescribe,
    /// A `skin.changed` event.
    Changed,
}

/// What reached the appearance state.
#[derive(Clone, Debug)]
pub enum AppearanceInput {
    /// A new connection: the ready skin when the app has it, the describe skin value.
    Handshake {
        /// The ready skin, if any.
        ready: Option<Value>,
        /// The describe result's skin value.
        describe: Option<Value>,
    },
    /// A `skin.changed` event with its replay flag.
    SkinChanged {
        /// The event payload.
        payload: Value,
        /// True when a replay window produced it.
        replayed: bool,
    },
    /// A describe result outside a handshake.
    Described {
        /// The describe result's skin value.
        payload: Value,
        /// The name `/skin <name>` asked for, if any.
        requested: Option<String>,
    },
}

/// What an input changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppearanceChange {
    /// Nothing changed.
    Unchanged,
    /// The skin changed.
    Changed,
    /// Hermes fell back to `default` for a requested name.
    UnknownSkin {
        /// The requested name.
        requested: String,
        /// The skin Hermes uses.
        using: String,
        /// Whether the colours changed.
        redraw: bool,
    },
}

/// The current skin and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceState {
    skin: Option<Skin>,
    source: SkinSource,
    skipped: Vec<SkippedValue>,
}

impl Default for AppearanceState {
    fn default() -> Self {
        Self {
            skin: None,
            source: SkinSource::None,
            skipped: Vec::new(),
        }
    }
}

/// An exact RGB colour.
///
/// This and the decision types below live in core (rather than `talaria-tui::theme`,
/// which re-exports them) so the application services can name them: domain crates
/// must never reach the terminal libraries behind the presentation crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Light or dark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Polarity {
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// `[theme] mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the terminal.
    #[default]
    Auto,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

/// `[theme] colors`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorsSetting {
    /// Follow the terminal.
    #[default]
    Auto,
    /// 24-bit colour.
    TrueColor,
    /// 256 colours.
    Ansi256,
    /// 16 colours.
    Ansi16,
    /// No colour.
    None,
}

/// What decided the polarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolaritySource {
    /// `/theme light` or `/theme dark` this session.
    Command,
    /// `TALARIA_THEME`.
    Environment,
    /// `[theme] mode`.
    Config,
    /// The probe background, trusted.
    TerminalBackground(Rgb),
    /// The probe foreground.
    TerminalForeground(Rgb),
    /// `COLORFGBG`.
    Colorfgbg,
    /// `TERM_PROGRAM`.
    TermProgram,
    /// The default.
    Default,
}

impl Rgb {
    /// Lower-case `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

impl PolaritySource {
    /// The `/theme` source label.
    pub fn label(self) -> String {
        match self {
            Self::Command => "set with /theme".to_owned(),
            Self::Environment => "TALARIA_THEME".to_owned(),
            Self::Config => "config.toml".to_owned(),
            Self::TerminalBackground(color) => format!("terminal background {}", color.hex()),
            Self::TerminalForeground(color) => format!("terminal text colour {}", color.hex()),
            Self::Colorfgbg => "COLORFGBG".to_owned(),
            Self::TermProgram => "TERM_PROGRAM".to_owned(),
            Self::Default => "default (no signal)".to_owned(),
        }
    }
}

/// How much colour the terminal takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    /// No colour.
    None,
    /// 16 colours.
    Ansi16,
    /// 256 colours.
    Ansi256,
    /// 24-bit colour.
    TrueColor,
}

/// What decided the colour depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthSource {
    /// `NO_COLOR`.
    NoColorEnv,
    /// `[theme] colors`.
    Config,
    /// `TERM=dumb`.
    TermDumb,
    /// `COLORTERM`.
    Colorterm,
    /// `TERM`.
    Term,
    /// The default.
    Default,
}

impl DepthSource {
    /// The `/theme` source label.
    pub fn label(self) -> String {
        match self {
            Self::NoColorEnv => "NO_COLOR",
            Self::Config => "config.toml",
            Self::TermDumb => "TERM=dumb",
            Self::Colorterm => "COLORTERM",
            Self::Term => "TERM",
            Self::Default => "default",
        }
        .to_owned()
    }
}

/// Full or reduced motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// Full.
    Full,
    /// Reduced.
    Reduced,
}

/// What decided the motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionSource {
    /// `TALARIA_REDUCED_MOTION`.
    Environment,
    /// `[ui] reduced_motion`.
    Config,
    /// The default.
    Default,
}

impl MotionSource {
    /// The `/theme` source label.
    pub fn label(self) -> String {
        match self {
            Self::Environment => "TALARIA_REDUCED_MOTION",
            Self::Config => "config.toml",
            Self::Default => "default",
        }
        .to_owned()
    }
}

/// The start-up probe's colour answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalColors {
    /// The OSC 11 background answer.
    pub background: Option<Rgb>,
    /// The OSC 10 foreground answer.
    pub foreground: Option<Rgb>,
}

/// Why a skin is not applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotAppliedReason {
    /// Colours are off.
    ColoursOff,
    /// `follow_hermes_skin = false`.
    FollowOff,
    /// A 16-colour terminal.
    SixteenColours,
}

/// What `/theme` prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeReport {
    /// The skin name, if any.
    pub skin: Option<String>,
    /// Why it is not applied, if so.
    pub not_applied: Option<NotAppliedReason>,
    /// Decided polarity and its source.
    pub polarity: (Polarity, PolaritySource),
    /// Decided depth and its source.
    pub depth: (ColorDepth, DepthSource),
    /// Decided motion and its source.
    pub motion: (Motion, MotionSource),
    /// Ignored chain keys in byte order.
    pub ignored: Vec<String>,
}

impl AppearanceState {
    /// The current skin, if any.
    pub fn skin(&self) -> Option<&Skin> {
        self.skin.as_ref()
    }

    /// Where the current skin came from.
    pub fn source(&self) -> SkinSource {
        self.source
    }

    /// The non-string values of the payload that produced the current skin.
    pub fn skipped(&self) -> &[SkippedValue] {
        &self.skipped
    }

    /// Fold one input into the state.
    ///
    /// A handshake takes the describe skin over the ready one, because only describe is resolved
    /// for this connection's profile; with neither, the current skin stays. A replayed
    /// `skin.changed` is older than the describe that follows a reconnect, so it changes nothing.
    /// A requested name is reported unknown only when Hermes fell back to `default`: Hermes finds
    /// a user skin by file name but reports the name inside the file, so any other difference is
    /// a renamed skin, not a missing one.
    pub fn update(&mut self, input: AppearanceInput) -> AppearanceChange {
        match input {
            AppearanceInput::Handshake { ready, describe } => {
                let present = |value: Option<Value>| value.filter(|value| !value.is_null());
                match (present(describe), present(ready)) {
                    (Some(describe), _) => self.apply(&describe, SkinSource::RuntimeDescribe),
                    (None, Some(ready)) => self.apply(&ready, SkinSource::GatewayReady),
                    (None, None) => AppearanceChange::Unchanged,
                }
            }
            AppearanceInput::SkinChanged { replayed: true, .. } => AppearanceChange::Unchanged,
            AppearanceInput::SkinChanged { payload, .. } => {
                self.apply(&payload, SkinSource::Changed)
            }
            AppearanceInput::Described { payload, requested } => {
                let change = self.apply(&payload, SkinSource::RuntimeDescribe);
                let fell_back = self
                    .skin
                    .as_ref()
                    .is_some_and(|skin| skin.name == "default");
                match requested {
                    Some(requested) if fell_back && requested != "default" => {
                        AppearanceChange::UnknownSkin {
                            requested,
                            using: "default".to_owned(),
                            redraw: change == AppearanceChange::Changed,
                        }
                    }
                    _ => change,
                }
            }
        }
    }

    /// Replace the skin with the one in `value`; equal field by field means unchanged.
    fn apply(&mut self, value: &Value, source: SkinSource) -> AppearanceChange {
        let (skin, skipped) = Skin::from_value(value);
        if skin == self.skin {
            return AppearanceChange::Unchanged;
        }
        self.source = if skin.is_some() {
            source
        } else {
            SkinSource::None
        };
        self.skin = skin;
        self.skipped = skipped;
        AppearanceChange::Changed
    }
}
