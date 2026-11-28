//! Resolve a selected theme file without changing the compatibility resolver.

use super::{ColorDepth, Polarity, Theme, ThemeInputs, color, contrast, surfaces};
use ratatui::style::{Color, Modifier, Style};
use std::collections::BTreeMap;
pub use talaria_app::theme_file::BorderPreset;
use talaria_app::theme_file::{
    Base, CatalogueEntry, ColorValue, PALETTE_SLOTS, StyleFields, ThemeFile, catalogue,
};
use talaria_core::appearance::ThemeFileReport;

type Sources = BTreeMap<String, String>;
const FIELDS: [&str; 9] = [
    "fg",
    "bg",
    "underline_color",
    "bold",
    "dim",
    "italic",
    "underline",
    "reverse",
    "strikethrough",
];
const ATTRIBUTES: [(&str, Modifier); 6] = [
    ("bold", Modifier::BOLD),
    ("dim", Modifier::DIM),
    ("italic", Modifier::ITALIC),
    ("underline", Modifier::UNDERLINED),
    ("reverse", Modifier::REVERSED),
    ("strikethrough", Modifier::CROSSED_OUT),
];

/// Resolved catalogue and the styles used by the current inline surfaces.
pub struct ThemeFileResolution {
    pub theme: Theme,
    pub elements: BTreeMap<String, Style>,
    pub report: Option<ThemeFileReport>,
    pub border: BorderPreset,
}

impl ThemeFileResolution {
    /// The live viewport needs its own surface when a theme and the physical terminal have
    /// opposite backgrounds. Finished lines still belong to the terminal's native history.
    pub fn inline_canvas(&self, terminal: super::TerminalColors) -> Option<(Style, super::Rgb)> {
        let report = self.report.as_ref()?;
        if !self.theme.color {
            return None;
        }
        let (observed, source) = super::detect::polarity(
            None,
            &super::detect::EnvView::default(),
            super::ThemeMode::Auto,
            &terminal,
        );
        let polarity = if source == super::PolaritySource::Default {
            self.theme.report.polarity.0
        } else {
            observed
        };
        if !matches!(
            (report.base.as_str(), polarity),
            ("light", Polarity::Dark) | ("dark", Polarity::Light)
        ) {
            return None;
        }
        let background = match source {
            super::PolaritySource::TerminalBackground(rgb) => rgb,
            _ if polarity == Polarity::Light => super::LIGHT_REFERENCE,
            _ => super::DARK_REFERENCE,
        };
        let style = *self.elements.get("transcript.background")?;
        Some((style, background))
    }
}

/// Resolve both the automatic appearance and named files through the same catalogue.
pub fn resolve_with_theme_file(
    inputs: &ThemeInputs<'_>,
    file: &ThemeFile,
    report_file: bool,
) -> ThemeFileResolution {
    let skin_participates = inputs.skin.is_some()
        && inputs.follow_hermes_skin
        && file.follow_hermes_skin
        && matches!(inputs.depth.0, ColorDepth::TrueColor | ColorDepth::Ansi256);
    let mut theme = super::resolve(inputs);
    if !file.follow_hermes_skin && inputs.skin.is_some() && theme.report.not_applied.is_none() {
        theme.report.not_applied = Some(super::NotAppliedReason::FollowOff);
    }
    let legacy = surfaces::bound_styles(&theme);
    let palette = palette(file, inputs.polarity.0);
    let canvases = [palette["background"], palette["surface"]];
    let mut report = ThemeFileReport {
        file: file.name.clone().unwrap_or_default(),
        base: file.extends.as_str().into(),
        skin_participates,
        sources: BTreeMap::new(),
        warnings: Vec::new(),
        unknown_contrast: Vec::new(),
    };
    let mut elements = BTreeMap::new();
    for entry in catalogue() {
        let (mut style, mut sources) = element_base(entry, file, &palette, report_file);
        if skin_participates && !file.palette.contains_key(role_slot(entry.role)) {
            let foreground = surfaces::SURFACE_BINDINGS
                .iter()
                .find(|(_, token)| *token == entry.token)
                .filter(|(field, _)| skin_field_present(inputs, field))
                .and_then(|(field, _)| legacy.iter().find(|(name, _)| name == field))
                .and_then(|(_, style)| style.fg);
            if let Some(foreground) = foreground {
                // The existing bridge explicitly clears dim when it supplies a foreground.
                style = style.fg(foreground).remove_modifier(Modifier::DIM);
                if report_file {
                    sources.insert("fg".into(), "skin".into());
                }
            }
        }
        if let Some(fields) = file.roles.get(entry.role) {
            patch(
                &mut style,
                &mut sources,
                fields,
                &format!("roles.{}", entry.role),
                &palette,
            );
        }
        if let Some(fields) = file.elements.get(entry.token) {
            patch(
                &mut style,
                &mut sources,
                fields,
                &format!("elements.{}", entry.token),
                &palette,
            );
            if entry.reserved {
                report.warnings.push(format!(
                    "{} is reserved; its drawing is not implemented",
                    entry.token
                ));
            }
        }
        let authored_underline = file
            .roles
            .get(entry.role)
            .is_some_and(|fields| fields.underline_color.is_some())
            || file
                .elements
                .get(entry.token)
                .is_some_and(|fields| fields.underline_color.is_some());
        if !authored_underline {
            style.underline_color = style.fg;
            if report_file {
                let source = sources.get("fg").cloned().unwrap_or_else(|| "base".into());
                sources.insert("underline_color".into(), source);
            }
        }
        constrain(
            &mut style,
            &mut sources,
            inputs.depth.0,
            entry.role,
            canvases,
        );
        elements.insert(entry.token.into(), style);
        if report_file {
            report.sources.insert(entry.token.into(), sources);
        }
    }
    if report_file {
        contrast::diagnose(&elements, inputs, &mut report);
        surfaces::apply(&mut theme, &elements, &report.sources);
        let inline_override = |token| {
            file.elements.contains_key(token).then(|| Style {
                bg: None,
                ..elements[token]
            })
        };
        theme.status.model = inline_override("status.model");
        theme.tool.shell_operator = inline_override("tool.shell.operator");
    }
    ThemeFileResolution {
        theme,
        elements,
        report: report_file.then_some(report),
        border: file.chrome.border,
    }
}

fn palette(file: &ThemeFile, polarity: Polarity) -> BTreeMap<String, Color> {
    let values = match file.extends {
        Base::Dark => [
            0x101014, 0x202024, 0xf4f4f4, 0xc4c4c4, 0xa8a8a8, 0xa0a0a0, 0xaaccff, 0x80dcff,
            0xa0e0a0, 0xffe080, 0xffa0a0, 0xffe080,
        ],
        Base::Light => [
            0xffffff, 0xf0f0f0, 0x161616, 0x484848, 0x606060, 0x707070, 0x174a8b, 0x005b70,
            0x236624, 0x714b00, 0x9b2424, 0x714b00,
        ],
        Base::Terminal => [0; 12],
    };
    // On native surfaces the terminal owns the actual values of these names.
    // Choose the dark or bright bank according to the detected background.
    use Color::{
        Black, Blue, DarkGray, Gray, Green, LightCyan, LightGreen, LightRed, Magenta, Red, Reset,
        Yellow,
    };
    let terminal = match polarity {
        Polarity::Dark => [
            Reset, Reset, Reset, Gray, Gray, Gray, Yellow, LightCyan, LightGreen, Yellow, LightRed,
            Yellow,
        ],
        Polarity::Light => [
            Reset, Reset, Reset, Black, Black, DarkGray, Blue, Magenta, Green, Red, Red, Blue,
        ],
    };
    let mut palette: BTreeMap<String, Color> = PALETTE_SLOTS
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let value = values[index];
            let color = if file.extends == Base::Terminal {
                terminal[index]
            } else {
                Color::Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
            };
            ((*slot).into(), color)
        })
        .collect();
    for (slot, value) in &file.palette {
        // File validation forbids references in the palette itself.
        palette.insert(slot.clone(), value_color(value, &BTreeMap::new()));
    }
    palette
}

fn role_slot(role: &str) -> &str {
    match role {
        "background" | "surface" | "text" => "foreground",
        "selection" | "focus" => "background",
        role => role,
    }
}

fn palette_source(file: &ThemeFile, slot: &str) -> String {
    if file.palette.contains_key(slot) {
        format!("palette.{slot}")
    } else {
        "base".into()
    }
}

fn element_base(
    entry: &CatalogueEntry,
    file: &ThemeFile,
    palette: &BTreeMap<String, Color>,
    report_file: bool,
) -> (Style, Sources) {
    let slot = role_slot(entry.role);
    let foreground = palette.get(slot).copied().unwrap_or(Color::Reset);
    let background_slot = match entry.role {
        "selection" | "focus" => "accent",
        "background" => "background",
        "surface" => "surface",
        _ => entry.area,
    };
    let background = palette
        .get(background_slot)
        .copied()
        .unwrap_or(Color::Reset);
    let mut style = Style::default().fg(foreground).bg(background);
    for (field, modifier) in ATTRIBUTES {
        if entry
            .attributes
            .split(',')
            .any(|attribute| attribute == field)
        {
            style = style.add_modifier(modifier);
        }
    }
    if !report_file {
        return (style, Sources::new());
    }
    let mut sources: Sources = FIELDS.map(|field| (field.into(), "base".into())).into();
    sources.insert("fg".into(), palette_source(file, slot));
    sources.insert("bg".into(), palette_source(file, background_slot));
    if !matches!(entry.role, "selection" | "focus")
        && let Some(background) = file
            .roles
            .get(entry.area)
            .and_then(|fields| fields.bg.as_ref())
    {
        style.bg = Some(value_color(background, palette));
        sources.insert("bg".into(), format!("roles.{}.bg", entry.area));
    }
    (style, sources)
}

fn value_color(value: &ColorValue, palette: &BTreeMap<String, Color>) -> Color {
    match value {
        ColorValue::Default => Color::Reset,
        ColorValue::Rgb(r, g, b) => Color::Rgb(*r, *g, *b),
        ColorValue::Ansi(index) => color::ansi_color(*index),
        ColorValue::Index(index) => Color::Indexed(*index),
        ColorValue::Palette(slot) => palette.get(slot).copied().unwrap_or(Color::Reset),
    }
}

fn patch(
    style: &mut Style,
    sources: &mut Sources,
    fields: &StyleFields,
    prefix: &str,
    palette: &BTreeMap<String, Color>,
) {
    for (name, value, target) in [
        ("fg", fields.fg.as_ref(), &mut style.fg),
        ("bg", fields.bg.as_ref(), &mut style.bg),
        (
            "underline_color",
            fields.underline_color.as_ref(),
            &mut style.underline_color,
        ),
    ] {
        if let Some(value) = value {
            *target = Some(value_color(value, palette));
            sources.insert(name.into(), format!("{prefix}.{name}"));
        }
    }
    let values = [
        fields.bold,
        fields.dim,
        fields.italic,
        fields.underline,
        fields.reverse,
        fields.strikethrough,
    ];
    for ((name, modifier), value) in ATTRIBUTES.into_iter().zip(values) {
        if let Some(value) = value {
            *style = if value {
                style.add_modifier(modifier)
            } else {
                style.remove_modifier(modifier)
            };
            sources.insert(name.into(), format!("{prefix}.{name}"));
        }
    }
}

fn constrain(
    style: &mut Style,
    sources: &mut Sources,
    depth: ColorDepth,
    role: &str,
    canvases: [Color; 2],
) {
    let authored = *style;
    for (name, target) in [
        ("fg", &mut style.fg),
        ("bg", &mut style.bg),
        ("underline_color", &mut style.underline_color),
    ] {
        let converted = target.and_then(|color| color::constrain_color(color, depth));
        if *target != converted {
            sources.insert(name.into(), "constraints".into());
            *target = converted;
        }
    }
    if depth == ColorDepth::Ansi16
        && let Some(ink) = contrast::ansi16_ink(authored, *style, role, canvases)
    {
        style.fg = Some(ink);
        if authored.underline_color == authored.fg {
            style.underline_color = style.fg;
        }
    }
    if matches!(depth, ColorDepth::Ansi16 | ColorDepth::None)
        && matches!(role, "selection" | "focus")
    {
        *style = style.add_modifier(Modifier::REVERSED);
        sources.insert("reverse".into(), "constraints".into());
    }
}

fn skin_field_present(inputs: &ThemeInputs<'_>, field: &str) -> bool {
    let role = match field {
        "markdown.heading1" | "markdown.heading2" | "markdown.heading3" => "primary",
        "markdown.inline_code" | "markdown.link_text" | "status.accent" => "accent",
        "markdown.quote_bar"
        | "markdown.rule"
        | "markdown.table_border"
        | "markdown.code_label"
        | "composer.border" => "border",
        "composer.marker" => "prompt",
        "reasoning" => "thinking",
        "tool.glyph_running" => "tool",
        "tool.glyph_ok" => "ok",
        "tool.glyph_failed" | "tool.error" => "error",
        "tool.glyph_muted" | "tool.meta" | "tool.line_number" | "tool.diff_note" => "muted",
        "tool.diff_added" => "diff added",
        "tool.diff_removed" => "diff removed",
        "tool.diff_hunk" => "diff hunk",
        "tool.diff_file" => "diff file",
        "status.text" => "status text",
        "status.muted" | "status.separator" => "status muted",
        "status.success" => "status success",
        "status.warning" => "status warning",
        "status.error" => "status error",
        "status.attention" => "status attention",
        _ => return false,
    };
    let Some(skin) = inputs.skin else {
        return false;
    };
    let overlay = if inputs.polarity.0 == Polarity::Light {
        &skin.light_colors
    } else {
        &skin.dark_colors
    };
    super::CHAINS
        .iter()
        .find(|chain| chain.role == role)
        .is_some_and(|chain| {
            chain.keys.iter().any(|key| {
                overlay
                    .get(*key)
                    .or_else(|| skin.colors.get(*key))
                    .is_some_and(|value| color::parse_color(value).is_some())
            })
        })
}
