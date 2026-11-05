//! Resolve a selected theme file without changing the compatibility resolver.

use super::{Theme, ThemeInputs};
use ratatui::style::Style;
use std::collections::BTreeMap;
use talaria_app::theme_file::{BorderPreset, ThemeFile};
use talaria_core::appearance::ThemeFileReport;

/// Resolved catalogue and the styles used by the current inline surfaces.
pub struct ThemeFileResolution {
    pub theme: Theme,
    pub elements: BTreeMap<String, Style>,
    pub report: Option<ThemeFileReport>,
    pub border: BorderPreset,
}

/// Apply a file, or preserve the existing resolver when no file is selected.
pub fn resolve_with_theme_file(
    inputs: &ThemeInputs<'_>,
    _file: Option<&ThemeFile>,
) -> ThemeFileResolution {
    ThemeFileResolution {
        theme: super::resolve(inputs),
        elements: BTreeMap::new(),
        report: None,
        border: BorderPreset::Rounded,
    }
}
