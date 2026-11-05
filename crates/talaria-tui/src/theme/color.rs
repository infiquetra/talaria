//! Colour parsing, contrast and palette mapping. Pure. Behavior contract: issue 227.
//!
//! `parse_color`, the luminance and contrast measures, and `ensure_contrast` copy Hermes's
//! `parseColor` (`apps/shared/src/color.ts`) and `liftForContrast` (`ui-tui/src/lib/color.ts`), so
//! a skin Hermes draws comes out the same colour here and nothing Hermes accepts is ignored.

pub use talaria_core::appearance::Rgb;

/// Parse a colour the way Hermes's `parseColor` does.
///
/// Trimmed, then `#rrggbb` or `#rgb` with the `#` optional and any case, or `rgb(` / `rgba(`
/// followed by three 1-to-3-digit channels, each clamped to 255; anything after the third
/// channel is ignored.
pub fn parse_color(text: &str) -> Option<Rgb> {
    let value = text.trim();
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let digit = |index: usize| u8::from_str_radix(&hex[index..=index], 16).ok();
        match hex.len() {
            6 => {
                let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
                return Some(Rgb(channel(0)?, channel(2)?, channel(4)?));
            }
            3 => {
                let doubled = |index: usize| digit(index).map(|nibble| nibble * 17);
                return Some(Rgb(doubled(0)?, doubled(1)?, doubled(2)?));
            }
            _ => {}
        }
    }
    parse_rgb_function(value)
}

/// `^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})`, case-insensitive.
fn parse_rgb_function(value: &str) -> Option<Rgb> {
    let lower = value.get(..4)?.to_ascii_lowercase();
    let rest = if lower == "rgb(" {
        &value[4..]
    } else if lower == "rgba" && value.get(4..5) == Some("(") {
        &value[5..]
    } else {
        return None;
    };
    let mut rest = rest;
    let mut channels = [0u8; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        rest = rest.trim_start();
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=3).contains(&digits) {
            return None;
        }
        let number: u16 = rest[..digits].parse().ok()?;
        *channel = u8::try_from(number.min(255)).ok()?;
        rest = &rest[digits..];
        if index < 2 {
            rest = rest.trim_start().strip_prefix(',')?;
        }
    }
    Some(Rgb(channels[0], channels[1], channels[2]))
}

fn channel_luminance(value: u8) -> f64 {
    let normalized = f64::from(value) / 255.0;
    if normalized <= 0.03928 {
        normalized / 12.92
    } else {
        ((normalized + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance with the 0.03928 threshold.
pub fn relative_luminance(color: Rgb) -> f64 {
    0.2126 * channel_luminance(color.0)
        + 0.7152 * channel_luminance(color.1)
        + 0.0722 * channel_luminance(color.2)
}

/// WCAG contrast ratio, 1.0 to 21.0.
pub fn contrast_ratio(first: Rgb, second: Rgb) -> f64 {
    let (a, b) = (relative_luminance(first), relative_luminance(second));
    let (high, low) = if a >= b { (a, b) } else { (b, a) };
    (high + 0.05) / (low + 0.05)
}

/// Rec. 709 luma, 0.0 to 1.0.
pub fn luma709(color: Rgb) -> f64 {
    (0.2126 * f64::from(color.0) + 0.7152 * f64::from(color.1) + 0.0722 * f64::from(color.2))
        / 255.0
}

/// Lift a foreground toward black or white until it reaches the floor.
///
/// A colour already at the floor is returned unchanged. Otherwise every channel steps 10% toward
/// black when the background is light (relative luminance above 0.5), else 10% of the remaining
/// headroom toward white, until the floor, black or white is reached.
pub fn ensure_contrast(fg: Rgb, bg: Rgb, min: f64) -> Rgb {
    if contrast_ratio(fg, bg) >= min {
        return fg;
    }
    let mut color = fg;
    let darken = relative_luminance(bg) > 0.5;
    let step = |channel: u8| -> u8 {
        if darken {
            channel - channel.div_ceil(10)
        } else {
            channel.saturating_add((255 - channel).div_ceil(10))
        }
    };
    while contrast_ratio(color, bg) < min {
        let done = if darken {
            color == Rgb(0, 0, 0)
        } else {
            color == Rgb(255, 255, 255)
        };
        if done {
            break;
        }
        color = Rgb(step(color.0), step(color.1), step(color.2));
    }
    color
}

/// The six levels of each channel of the xterm colour cube (indexes 16 to 231).
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The nearest xterm index, 16 to 255.
///
/// Nearest by squared RGB distance over the colour cube and the grey ramp (232 to 255 at
/// `8 + 10 n`); a tie goes to the lower index.
pub fn to_ansi256(color: Rgb) -> u8 {
    let distance = |candidate: Rgb| -> u32 {
        let delta = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
        delta(color.0, candidate.0) + delta(color.1, candidate.1) + delta(color.2, candidate.2)
    };
    let mut best = (16u8, u32::MAX);
    for index in 16u8..=255 {
        let candidate = if index < 232 {
            let cube = index - 16;
            Rgb(
                CUBE[usize::from(cube / 36)],
                CUBE[usize::from(cube / 6 % 6)],
                CUBE[usize::from(cube % 6)],
            )
        } else {
            let level = 8 + 10 * (index - 232);
            Rgb(level, level, level)
        };
        let gap = distance(candidate);
        if gap < best.1 {
            best = (index, gap);
        }
    }
    best.0
}

/// Conventional reference colours used only to choose a 16-colour approximation.
pub const ANSI16: [Rgb; 16] = [
    Rgb(0, 0, 0),
    Rgb(128, 0, 0),
    Rgb(0, 128, 0),
    Rgb(128, 128, 0),
    Rgb(0, 0, 128),
    Rgb(128, 0, 128),
    Rgb(0, 128, 128),
    Rgb(192, 192, 192),
    Rgb(128, 128, 128),
    Rgb(255, 0, 0),
    Rgb(0, 255, 0),
    Rgb(255, 255, 0),
    Rgb(0, 0, 255),
    Rgb(255, 0, 255),
    Rgb(0, 255, 255),
    Rgb(255, 255, 255),
];

/// A named ANSI colour keeps the terminal's configured palette effective.
pub fn ansi_color(index: u8) -> ratatui::style::Color {
    use ratatui::style::Color;
    match index {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        15 => Color::White,
        _ => Color::Indexed(index),
    }
}

/// The xterm reference value for an index. ANSI entries remain user configurable in practice.
pub fn indexed_rgb(index: u8) -> Rgb {
    match index {
        0..=15 => ANSI16[usize::from(index)],
        16..=231 => {
            let cube = index - 16;
            Rgb(
                CUBE[usize::from(cube / 36)],
                CUBE[usize::from(cube / 6 % 6)],
                CUBE[usize::from(cube % 6)],
            )
        }
        _ => {
            let level = 8 + 10 * (index - 232);
            Rgb(level, level, level)
        }
    }
}

/// Nearest conventional ANSI colour by squared RGB distance; ties choose the lower index.
pub fn to_ansi16(color: Rgb) -> u8 {
    ANSI16
        .iter()
        .enumerate()
        .min_by_key(|(_, candidate)| {
            let delta = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
            delta(color.0, candidate.0) + delta(color.1, candidate.1) + delta(color.2, candidate.2)
        })
        .map_or(0, |(index, _)| index as u8)
}

/// Apply the terminal's depth after file layering, to any style colour field.
pub fn constrain_color(
    color: ratatui::style::Color,
    depth: talaria_core::appearance::ColorDepth,
) -> Option<ratatui::style::Color> {
    use ratatui::style::Color;
    use talaria_core::appearance::ColorDepth;
    match (depth, color) {
        (ColorDepth::None, _) => None,
        (ColorDepth::Ansi16, Color::Rgb(r, g, b)) => Some(ansi_color(to_ansi16(Rgb(r, g, b)))),
        (ColorDepth::Ansi16, Color::Indexed(index)) => Some(ansi_color(if index < 16 {
            index
        } else {
            to_ansi16(indexed_rgb(index))
        })),
        (ColorDepth::Ansi256, Color::Rgb(r, g, b)) => {
            Some(Color::Indexed(to_ansi256(Rgb(r, g, b))))
        }
        (_, color) => Some(color),
    }
}
