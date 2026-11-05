//! Bind the existing inline style fields to catalogue tokens.

use super::Theme;
use ratatui::style::Style;

/// Concrete style field and its catalogue token.
pub const SURFACE_BINDINGS: &[(&str, &str)] = &[];

/// Inspect every bound style without relying on the resolver's binding application.
pub fn bound_styles(_theme: &Theme) -> Vec<(&'static str, Style)> {
    Vec::new()
}
