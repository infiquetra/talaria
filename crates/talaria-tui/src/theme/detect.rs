//! Terminal detection: polarity, colour depth and motion from the environment.
//!
//! Pure; the environment is passed in. Behavior contract: issue 227.

use std::collections::HashMap;

use talaria_core::appearance::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, TerminalColors,
};

use super::color::{luma709, relative_luminance};
pub use talaria_core::appearance::{ColorsSetting, ThemeMode};

/// The environment, read once.
#[derive(Clone, Debug, Default)]
pub struct EnvView {
    values: HashMap<String, String>,
}

impl EnvView {
    /// Read the process environment.
    pub fn from_process() -> EnvView {
        let mut values = HashMap::new();
        for name in [
            "NO_COLOR",
            "TERM",
            "COLORTERM",
            "COLORFGBG",
            "TERM_PROGRAM",
            "TALARIA_THEME",
            "TALARIA_REDUCED_MOTION",
        ] {
            if let Ok(value) = std::env::var(name) {
                values.insert(name.to_owned(), value);
            }
        }
        EnvView { values }
    }

    /// Build a view from pairs, for tests.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> EnvView {
        EnvView {
            values: pairs
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    /// One variable, if set.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

/// The escape byte, written as a number: only the terminal module writes escape sequences.
const ESC: u8 = 0x1b;
/// The bell byte that may end an OSC reply.
const BEL: u8 = 0x07;

/// A probe background of exactly black is distrusted: terminals built on xterm.js, and tmux,
/// report it when they do not know.
const UNKNOWN_BACKGROUND: Rgb = Rgb(0, 0, 0);

/// One environment value, trimmed and lower-cased; `None` when unset or blank.
fn setting(env: &EnvView, name: &str) -> Option<String> {
    env.get(name)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn theme_env(env: &EnvView) -> Option<Polarity> {
    match setting(env, "TALARIA_THEME")?.as_str() {
        "light" => Some(Polarity::Light),
        "dark" => Some(Polarity::Dark),
        _ => None,
    }
}

fn motion_env(env: &EnvView) -> Option<Motion> {
    match setting(env, "TALARIA_REDUCED_MOTION")?.as_str() {
        "1" | "true" | "yes" | "on" => Some(Motion::Reduced),
        "0" | "false" | "no" | "off" => Some(Motion::Full),
        _ => None,
    }
}

/// `COLORFGBG`'s last field: 7 or 15 is a light background, any other 0 to 15 dark.
fn colorfgbg(env: &EnvView) -> Option<Polarity> {
    let last = env.get("COLORFGBG")?.rsplit(';').next()?.trim();
    if last.is_empty() || !last.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match last.parse::<u8>().ok()? {
        7 | 15 => Some(Polarity::Light),
        0..=15 => Some(Polarity::Dark),
        _ => None,
    }
}

/// Decide light or dark, first match wins.
///
/// The session override, `TALARIA_THEME`, then `[theme] mode`; then, as Hermes's own terminal
/// interface does, a trusted probe background decides both ways, the probe foreground only when
/// there is no trusted background, then `COLORFGBG`, then `TERM_PROGRAM`, then dark.
pub fn polarity(
    override_: Option<Polarity>,
    env: &EnvView,
    config_mode: ThemeMode,
    terminal: &TerminalColors,
) -> (Polarity, PolaritySource) {
    if let Some(polarity) = override_ {
        return (polarity, PolaritySource::Command);
    }
    if let Some(polarity) = theme_env(env) {
        return (polarity, PolaritySource::Environment);
    }
    match config_mode {
        ThemeMode::Light => return (Polarity::Light, PolaritySource::Config),
        ThemeMode::Dark => return (Polarity::Dark, PolaritySource::Config),
        ThemeMode::Auto => {}
    }
    if let Some(background) = terminal
        .background
        .filter(|background| *background != UNKNOWN_BACKGROUND)
    {
        let polarity = if luma709(background) >= 0.6 {
            Polarity::Light
        } else {
            Polarity::Dark
        };
        return (polarity, PolaritySource::TerminalBackground(background));
    }
    if let Some(foreground) = terminal
        .foreground
        .filter(|color| *color != Rgb(0, 0, 0) && *color != Rgb(255, 255, 255))
    {
        let luminance = relative_luminance(foreground);
        if luminance >= 0.45 {
            return (
                Polarity::Dark,
                PolaritySource::TerminalForeground(foreground),
            );
        }
        if luminance <= 0.2 {
            return (
                Polarity::Light,
                PolaritySource::TerminalForeground(foreground),
            );
        }
    }
    if let Some(polarity) = colorfgbg(env) {
        return (polarity, PolaritySource::Colorfgbg);
    }
    if env.get("TERM_PROGRAM") == Some("Apple_Terminal") {
        return (Polarity::Light, PolaritySource::TermProgram);
    }
    (Polarity::Dark, PolaritySource::Default)
}

/// Decide the colour depth, first match wins.
///
/// A non-empty `NO_COLOR`, then `[theme] colors`, then `TERM=dumb`, `COLORTERM` and `TERM`.
pub fn depth(env: &EnvView, config: ColorsSetting) -> (ColorDepth, DepthSource) {
    if env.get("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        return (ColorDepth::None, DepthSource::NoColorEnv);
    }
    match config {
        ColorsSetting::None => return (ColorDepth::None, DepthSource::Config),
        ColorsSetting::TrueColor => return (ColorDepth::TrueColor, DepthSource::Config),
        ColorsSetting::Ansi256 => return (ColorDepth::Ansi256, DepthSource::Config),
        ColorsSetting::Ansi16 => return (ColorDepth::Ansi16, DepthSource::Config),
        ColorsSetting::Auto => {}
    }
    let term = env.get("TERM").unwrap_or_default();
    if term == "dumb" {
        return (ColorDepth::None, DepthSource::TermDumb);
    }
    if matches!(
        setting(env, "COLORTERM").as_deref(),
        Some("truecolor" | "24bit")
    ) {
        return (ColorDepth::TrueColor, DepthSource::Colorterm);
    }
    if term.contains("256color") {
        return (ColorDepth::Ansi256, DepthSource::Term);
    }
    (ColorDepth::Ansi16, DepthSource::Default)
}

/// Decide full or reduced motion: `TALARIA_REDUCED_MOTION`, then `[ui] reduced_motion`, then
/// full. `config_reduced` is `None` when the configuration does not set it.
pub fn motion(env: &EnvView, config_reduced: Option<bool>) -> (Motion, MotionSource) {
    if let Some(motion) = motion_env(env) {
        return (motion, MotionSource::Environment);
    }
    match config_reduced {
        Some(true) => (Motion::Reduced, MotionSource::Config),
        Some(false) => (Motion::Full, MotionSource::Config),
        None => (Motion::Full, MotionSource::Default),
    }
}

/// One `rgb:` channel of 1 to 4 hex digits, scaled to 0..=255 and rounded to nearest.
fn osc_channel(text: &str) -> Option<u8> {
    if !(1..=4).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    let max = (1u32 << (4 * text.len())) - 1;
    u8::try_from((value * 255 + max / 2) / max).ok()
}

/// Parse one OSC 10 or 11 reply into its index and colour.
///
/// `ESC ] 10|11 ; <spec>` ended by BEL or `ESC \`, where the spec is `rgb:r/g/b` with 1 to 4 hex
/// digits per channel or `#rrggbb`. Anything else, `rgba:` included, is unknown.
pub fn parse_osc_color(reply: &[u8]) -> Option<(u8, Rgb)> {
    let body = reply.strip_prefix(&[ESC, b']'])?;
    let body = body
        .strip_suffix(&[BEL])
        .or_else(|| body.strip_suffix(&[ESC, b'\\']))?;
    let text = std::str::from_utf8(body).ok()?;
    let (index, spec) = text.split_once(';')?;
    let index = match index {
        "10" => 10,
        "11" => 11,
        _ => return None,
    };
    let color = if let Some(channels) = spec.strip_prefix("rgb:") {
        let mut parts = channels.split('/');
        let color = Rgb(
            osc_channel(parts.next()?)?,
            osc_channel(parts.next()?)?,
            osc_channel(parts.next()?)?,
        );
        if parts.next().is_some() {
            return None;
        }
        color
    } else {
        let hex = spec.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let channel = |at: usize| osc_channel(hex.get(at..at + 2)?);
        Rgb(channel(0)?, channel(2)?, channel(4)?)
    };
    Some((index, color))
}

/// The notices for invalid environment values, in table order.
pub fn env_notices(env: &EnvView) -> Vec<String> {
    let mut notices = Vec::new();
    if setting(env, "TALARIA_THEME").is_some_and(|value| value != "auto")
        && theme_env(env).is_none()
    {
        notices.push("TALARIA_THEME must be light, dark or auto; ignoring it.".to_owned());
    }
    if setting(env, "TALARIA_REDUCED_MOTION").is_some() && motion_env(env).is_none() {
        notices.push(
            "TALARIA_REDUCED_MOTION must be 1, true, yes, on, 0, false, no or off; ignoring it."
                .to_owned(),
        );
    }
    notices
}
