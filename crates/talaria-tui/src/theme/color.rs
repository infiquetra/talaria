//! Colour parsing, contrast and palette mapping. Pure. Behavior contract: issue 227.

pub use talaria_core::appearance::Rgb;

/// Parse a colour the way Hermes's `parseColor` does.
pub fn parse_color(text: &str) -> Option<Rgb> {
    let _ = text;
    todo!("#227")
}

/// WCAG relative luminance with the 0.03928 threshold.
pub fn relative_luminance(color: Rgb) -> f64 {
    let _ = color;
    todo!("#227")
}

/// WCAG contrast ratio, 1.0 to 21.0.
pub fn contrast_ratio(first: Rgb, second: Rgb) -> f64 {
    let _ = (first, second);
    todo!("#227")
}

/// Rec. 709 luma, 0.0 to 1.0.
pub fn luma709(color: Rgb) -> f64 {
    let _ = color;
    todo!("#227")
}

/// Lift a foreground toward black or white until it reaches the floor.
pub fn ensure_contrast(fg: Rgb, bg: Rgb, min: f64) -> Rgb {
    let _ = (fg, bg, min);
    todo!("#227")
}

/// The nearest xterm index, 16 to 255.
pub fn to_ansi256(color: Rgb) -> u8 {
    let _ = color;
    todo!("#227")
}
