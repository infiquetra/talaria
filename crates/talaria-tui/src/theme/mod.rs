//! Hermes skins resolved into Talaria's styles. Pure except the probe.
//!
//! Behavior contract: issue 227.

pub mod cells;
pub mod color;
pub mod detect;
pub mod roles;

use std::collections::{BTreeMap, BTreeSet};

use ratatui::style::{Color, Modifier, Style};

pub use color::{
    contrast_ratio, ensure_contrast, luma709, parse_color, relative_luminance, to_ansi256,
};
pub use roles::{CHAINS, RoleChain, chain};
pub use talaria_core::appearance::{
    ColorDepth, ColorsSetting, DepthSource, Motion, MotionSource, NotAppliedReason, Polarity,
    PolaritySource, Rgb, Skin, SkinMap, SkippedValue, TerminalColors, ThemeMode, ThemeReport,
};

use crate::markdown::MarkdownTheme;
use crate::status::{StatusPalette, Tone};
use crate::tool_cell::ToolTheme;

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

/// The phase 1 look: a plain frame and a bold `>` marker.
impl Default for ComposerStyle {
    fn default() -> Self {
        Self {
            border: Style::default(),
            marker: Style::default().add_modifier(Modifier::BOLD),
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
    /// Tool cell and diff styles.
    pub tool: ToolTheme,
    /// False when depth is none.
    pub color: bool,
    /// Full or reduced motion.
    pub motion: Motion,
    /// What `/theme` prints.
    pub report: ThemeReport,
}

/// The background assumed for a light terminal when the probe did not decide polarity.
const LIGHT_REFERENCE: Rgb = Rgb(0xff, 0xff, 0xff);
/// The background assumed for a dark terminal when the probe did not decide polarity.
const DARK_REFERENCE: Rgb = Rgb(0x10, 0x10, 0x14);

/// Resolve a skin and the detected terminal into every themed surface.
///
/// Pure. With colours off every themed surface keeps its default with the colours removed; with
/// no skin, `follow_hermes_skin = false` or a 16-colour terminal each surface is its own
/// `Default`, the phase 1 look. Otherwise each role takes the first parseable colour of its key
/// chain from the skin's colours with the polarity's overlay merged over them, lifted to Hermes's
/// contrast floor against the real or assumed background. Only foreground colours are set:
/// Talaria never paints a background, so body text keeps the terminal's own colour.
pub fn resolve(inputs: &ThemeInputs<'_>) -> Theme {
    let (depth, _) = inputs.depth;
    let not_applied = inputs.skin.and(if depth == ColorDepth::None {
        Some(NotAppliedReason::ColoursOff)
    } else if !inputs.follow_hermes_skin {
        Some(NotAppliedReason::FollowOff)
    } else if depth == ColorDepth::Ansi16 {
        Some(NotAppliedReason::SixteenColours)
    } else {
        None
    });
    let mut report = ThemeReport {
        skin: inputs.skin.map(|skin| skin.name.clone()),
        not_applied,
        polarity: inputs.polarity,
        depth: inputs.depth,
        motion: inputs.motion,
        ignored: Vec::new(),
    };
    let mut theme = Theme {
        markdown: MarkdownTheme::default(),
        status: StatusPalette::default(),
        composer: ComposerStyle::default(),
        reasoning: Style::default(),
        tool: ToolTheme::default(),
        color: depth != ColorDepth::None,
        motion: inputs.motion.0,
        report: report.clone(),
    };
    if depth == ColorDepth::None {
        strip_colours(&mut theme);
        return theme;
    }
    let Some(skin) = inputs.skin.filter(|_| not_applied.is_none()) else {
        return theme;
    };

    let polarity = inputs.polarity.0;
    let (overlay, overlay_map) = match polarity {
        Polarity::Light => (&skin.light_colors, SkinMap::LightColors),
        Polarity::Dark => (&skin.dark_colors, SkinMap::DarkColors),
    };
    let mut effective: BTreeMap<&str, &str> = skin
        .colors
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    effective.extend(
        overlay
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    report.ignored = ignored_keys(&effective, overlay, overlay_map, inputs.skipped);

    let reference = match inputs.polarity.1 {
        PolaritySource::TerminalBackground(background) => background,
        _ if polarity == Polarity::Light => LIGHT_REFERENCE,
        _ => DARK_REFERENCE,
    };
    let floor = |semantic: bool| match (polarity, semantic) {
        (Polarity::Dark, false) => 1.45,
        (Polarity::Light, false) => 1.18,
        (Polarity::Dark, true) => 2.2,
        (Polarity::Light, true) => 1.6,
    };
    let role = |name: &str| -> Option<Color> {
        let chain = CHAINS.iter().find(|entry| entry.role == name)?;
        let color = chain.keys.iter().find_map(|key| {
            effective
                .get(key)
                .and_then(|value| color::parse_color(value))
        })?;
        let lifted = color::ensure_contrast(color, reference, floor(chain.semantic));
        Some(match depth {
            ColorDepth::Ansi256 => Color::Indexed(color::to_ansi256(lifted)),
            _ => Color::Rgb(lifted.0, lifted.1, lifted.2),
        })
    };

    let markdown = &mut theme.markdown;
    if let Some(color) = role("primary") {
        for style in [
            &mut markdown.heading1,
            &mut markdown.heading2,
            &mut markdown.heading3,
        ] {
            *style = colored(*style, color);
        }
    }
    if let Some(color) = role("accent") {
        markdown.inline_code = colored(markdown.inline_code, color);
        markdown.link_text = colored(markdown.link_text, color);
    }
    if let Some(color) = role("border") {
        for style in [
            &mut markdown.quote_bar,
            &mut markdown.rule,
            &mut markdown.table_border,
            &mut markdown.code_label,
        ] {
            *style = colored(*style, color);
        }
        theme.composer.border = colored(theme.composer.border, color);
    }
    if let Some(color) = role("prompt") {
        theme.composer.marker = colored(theme.composer.marker, color);
    }
    if let Some(color) = role("thinking") {
        theme.reasoning = colored(theme.reasoning, color);
    }
    apply_tool_roles(&mut theme.tool, &role);
    let palette = StatusPalette::default();
    let tone = |tone: Tone, name: &str| match role(name) {
        Some(color) => colored(palette.style(tone), color),
        None => palette.style(tone),
    };
    theme.status = StatusPalette::from_styles([
        tone(Tone::Text, "status text"),
        tone(Tone::Muted, "status muted"),
        tone(Tone::Separator, "status muted"),
        tone(Tone::Success, "status success"),
        tone(Tone::Warning, "status warning"),
        tone(Tone::Error, "status error"),
        tone(Tone::Attention, "status attention"),
    ]);
    theme.report = report;
    theme
}

/// Tool cell styles from the skin: each row of the role table that names a tool field.
fn apply_tool_roles(tool: &mut ToolTheme, role: &dyn Fn(&str) -> Option<Color>) {
    let paint = |name: &str, styles: &mut [&mut Style]| {
        if let Some(color) = role(name) {
            for style in styles {
                **style = colored(**style, color);
            }
        }
    };
    paint("tool", &mut [&mut tool.glyph_running]);
    paint("ok", &mut [&mut tool.glyph_ok]);
    paint("error", &mut [&mut tool.glyph_failed, &mut tool.error]);
    paint(
        "muted",
        &mut [
            &mut tool.glyph_muted,
            &mut tool.meta,
            &mut tool.line_number,
            &mut tool.diff_note,
        ],
    );
    paint("diff added", &mut [&mut tool.diff_added]);
    paint("diff removed", &mut [&mut tool.diff_removed]);
    paint("diff hunk", &mut [&mut tool.diff_hunk]);
    paint("diff file", &mut [&mut tool.diff_file]);
}

/// A default style with a skin colour: its modifiers stay, except that the colour replaces dim.
fn colored(style: Style, color: Color) -> Style {
    style.fg(color).remove_modifier(Modifier::DIM)
}

/// Keys in a role chain that hold an unparseable string in the effective map, or that were not
/// strings in `colors` or the overlay in use, unless that overlay supplies a string for the key.
/// In byte order.
fn ignored_keys(
    effective: &BTreeMap<&str, &str>,
    overlay: &talaria_core::appearance::ColorMap,
    overlay_map: SkinMap,
    skipped: &[SkippedValue],
) -> Vec<String> {
    let chained: BTreeSet<&str> = CHAINS
        .iter()
        .flat_map(|entry| entry.keys.iter().copied())
        .collect();
    let mut ignored: BTreeSet<String> = BTreeSet::new();
    for key in &chained {
        if effective
            .get(key)
            .is_some_and(|value| color::parse_color(value).is_none())
        {
            ignored.insert((*key).to_owned());
        }
    }
    for value in skipped {
        let in_use = value.map == SkinMap::Colors || value.map == overlay_map;
        if in_use && chained.contains(value.key.as_str()) && !overlay.contains_key(&value.key) {
            ignored.insert(value.key.clone());
        }
    }
    ignored.into_iter().collect()
}

/// Remove every colour from every themed surface; modifiers stay.
fn strip_colours(theme: &mut Theme) {
    let bare = |style: Style| Style {
        fg: None,
        bg: None,
        underline_color: None,
        ..style
    };
    let markdown = &mut theme.markdown;
    for style in [
        &mut markdown.heading1,
        &mut markdown.heading2,
        &mut markdown.heading3,
        &mut markdown.strong,
        &mut markdown.emphasis,
        &mut markdown.strikethrough,
        &mut markdown.inline_code,
        &mut markdown.link_text,
        &mut markdown.link_destination,
        &mut markdown.image_alt,
        &mut markdown.quote_bar,
        &mut markdown.rule,
        &mut markdown.code_label,
        &mut markdown.table_border,
        &mut markdown.table_header,
    ] {
        *style = bare(*style);
    }
    let palette = &theme.status;
    theme.status = StatusPalette::from_styles(
        [
            Tone::Text,
            Tone::Muted,
            Tone::Separator,
            Tone::Success,
            Tone::Warning,
            Tone::Error,
            Tone::Attention,
        ]
        .map(|tone| bare(palette.style(tone))),
    );
    theme.composer = ComposerStyle {
        border: bare(theme.composer.border),
        marker: bare(theme.composer.marker),
    };
    theme.reasoning = bare(theme.reasoning);
    let tool = &mut theme.tool;
    for style in [
        &mut tool.glyph_running,
        &mut tool.glyph_ok,
        &mut tool.glyph_failed,
        &mut tool.glyph_muted,
        &mut tool.title,
        &mut tool.subject,
        &mut tool.meta,
        &mut tool.output,
        &mut tool.error,
        &mut tool.diff_added,
        &mut tool.diff_removed,
        &mut tool.diff_context,
        &mut tool.diff_hunk,
        &mut tool.diff_file,
        &mut tool.diff_note,
        &mut tool.line_number,
    ] {
        *style = bare(*style);
    }
}
