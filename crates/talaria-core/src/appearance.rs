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
        let _ = payload;
        todo!("#227")
    }

    /// Same rules over a raw JSON value; every non-string colour value is skipped
    /// and returned, in map order then key order.
    pub fn from_value(value: &Value) -> (Option<Skin>, Vec<SkippedValue>) {
        let _ = value;
        todo!("#227")
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
#[derive(Clone, Debug, Default)]
pub struct AppearanceState {
    _private: (),
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

impl PolaritySource {
    /// The `/theme` source label (issue 227; stub).
    pub fn label(self) -> String {
        todo!("#227")
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
    /// The `/theme` source label (issue 227; stub).
    pub fn label(self) -> String {
        todo!("#227")
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
    /// The `/theme` source label (issue 227; stub).
    pub fn label(self) -> String {
        todo!("#227")
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
        todo!("#227")
    }

    /// Where the current skin came from.
    pub fn source(&self) -> SkinSource {
        todo!("#227")
    }

    /// The non-string values of the payload that produced the current skin.
    pub fn skipped(&self) -> &[SkippedValue] {
        todo!("#227")
    }

    /// Fold one input into the state.
    pub fn update(&mut self, input: AppearanceInput) -> AppearanceChange {
        let _ = input;
        todo!("#227")
    }
}
