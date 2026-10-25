//! Hermes skins resolved into Talaria's styles. Pure except the probe.
//!
//! Behavior contract: issue 227.

pub mod color;
pub mod detect;
pub mod roles;

use ratatui::style::{Modifier, Style};

pub use color::{
    contrast_ratio, ensure_contrast, luma709, parse_color, relative_luminance, to_ansi256,
};
pub use roles::{RoleChain, chain};
pub use talaria_core::appearance::{
    ColorDepth, ColorsSetting, DepthSource, Motion, MotionSource, NotAppliedReason, Polarity,
    PolaritySource, Rgb, Skin, SkippedValue, TerminalColors, ThemeMode, ThemeReport,
};

use crate::markdown::MarkdownTheme;
use crate::status::StatusPalette;

/// Everything `resolve` needs. No environment reads.
#[derive(Debug)]
pub struct ThemeInputs<'a> {
    /// The current skin, if any.
    pub skin: Option<&'a Skin>,
    /// The current skin's skipped values.
    pub skipped: &'a [SkippedValue],
    /// `[theme] follow_hermes_skin`.
    pub follow_hermes_skin: bool,
    /// Decided polarity and its source.
    pub polarity: (Polarity, PolaritySource),
    /// Decided depth and its source.
    pub depth: (ColorDepth, DepthSource),
    /// Decided motion and its source.
    pub motion: (Motion, MotionSource),
    /// The start-up probe colours.
    pub terminal: TerminalColors,
}

/// The composer border and prompt marker styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposerStyle {
    /// The border style.
    pub border: Style,
    /// The prompt marker style.
    pub marker: Style,
}

impl Default for ComposerStyle {
    fn default() -> Self {
        let dim = Style::default().add_modifier(Modifier::DIM);
        Self {
            border: dim,
            marker: dim,
        }
    }
}

/// Every themed surface, resolved once per change.
#[derive(Clone, Debug)]
pub struct Theme {
    /// Markdown styles.
    pub markdown: MarkdownTheme,
    /// Status line styles.
    pub status: StatusPalette,
    /// Composer styles.
    pub composer: ComposerStyle,
    /// Reasoning base style.
    pub reasoning: Style,
    /// False when depth is none.
    pub color: bool,
    /// Full or reduced motion.
    pub motion: Motion,
    /// What `/theme` prints.
    pub report: ThemeReport,
}

/// Resolve a skin and the detected terminal into every themed surface.
pub fn resolve(inputs: &ThemeInputs<'_>) -> Theme {
    let _ = inputs;
    todo!("#227")
}
