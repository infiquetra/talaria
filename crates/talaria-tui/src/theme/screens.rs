//! Synthetic theme screens drawn through the resolver and the views.

use talaria_core::appearance::Rgb;

/// One captured screen of a built-in theme.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeScreenFrame {
    /// Built-in identifier: `terminal`, `ember` or `paper`.
    pub theme_id: &'static str,
    /// Columns of the captured terminal.
    pub width: u16,
    /// Rows of the captured terminal.
    pub height: u16,
    /// The inline view, which paints no theme background of its own.
    pub inline: bool,
    /// The terminal background behind an inline view.
    pub terminal_background: Option<Rgb>,
    /// Visible text in the frame.
    pub text: String,
    /// Distinct foregrounds inside the code block.
    pub code_colours: usize,
    /// Whether the frame paints the theme's own background.
    pub paints_background: bool,
}

/// The screens the example prints and the screen tests inspect.
#[must_use]
pub fn frames() -> Vec<ThemeScreenFrame> {
    let light = Some(Rgb(255, 255, 255));
    let dark = Some(Rgb(16, 16, 20));
    let mut frames = Vec::new();
    for theme_id in ["terminal", "ember", "paper"] {
        for (width, height) in [(80, 24), (120, 40)] {
            frames.push(blank(theme_id, width, height, false, None));
        }
    }
    for theme_id in ["ember", "paper"] {
        for background in [light, dark] {
            frames.push(blank(theme_id, 80, 24, true, background));
        }
    }
    frames
}

fn blank(
    theme_id: &'static str,
    width: u16,
    height: u16,
    inline: bool,
    terminal_background: Option<Rgb>,
) -> ThemeScreenFrame {
    ThemeScreenFrame {
        theme_id,
        width,
        height,
        inline,
        terminal_background,
        text: String::new(),
        code_colours: 0,
        paints_background: false,
    }
}
