//! Terminal detection: polarity, colour depth and motion from the environment.
//!
//! Pure; the environment is passed in. Behavior contract: issue 227.

use std::collections::HashMap;

use talaria_core::appearance::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, TerminalColors,
};
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

/// Decide light or dark, first match wins.
pub fn polarity(
    override_: Option<Polarity>,
    env: &EnvView,
    config_mode: ThemeMode,
    terminal: &TerminalColors,
) -> (Polarity, PolaritySource) {
    let _ = (override_, env, config_mode, terminal);
    todo!("#227")
}

/// Decide the colour depth, first match wins.
pub fn depth(env: &EnvView, config: ColorsSetting) -> (ColorDepth, DepthSource) {
    let _ = (env, config);
    todo!("#227")
}

/// Decide full or reduced motion.
pub fn motion(env: &EnvView, config_reduced: Option<bool>) -> (Motion, MotionSource) {
    let _ = (env, config_reduced);
    todo!("#227")
}

/// Parse one OSC 10 or 11 reply into its index and colour.
pub fn parse_osc_color(reply: &[u8]) -> Option<(u8, Rgb)> {
    let _ = reply;
    todo!("#227")
}

/// The section 10 notices for invalid environment values, in table order.
pub fn env_notices(env: &EnvView) -> Vec<String> {
    let _ = env;
    todo!("#227")
}
