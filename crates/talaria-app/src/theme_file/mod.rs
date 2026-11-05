use std::collections::BTreeMap;

mod catalogue;
mod load;
mod parse;
pub use catalogue::{CatalogueEntry, PALETTE_SLOTS, ROLES, area_background, catalogue};
pub use load::{load, reserved_id, valid_id};
pub use parse::parse;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFile {
    pub name: Option<String>,
    pub extends: Base,
    pub follow_hermes_skin: bool,
    pub palette: BTreeMap<String, ColorValue>,
    pub roles: BTreeMap<String, StyleFields>,
    pub elements: BTreeMap<String, StyleFields>,
    pub chrome: Chrome,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Terminal,
    Dark,
    Light,
}
impl Base {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorValue {
    Default,
    Rgb(u8, u8, u8),
    Ansi(u8),
    Index(u8),
    Palette(String),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleFields {
    pub fg: Option<ColorValue>,
    pub bg: Option<ColorValue>,
    pub underline_color: Option<ColorValue>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub reverse: Option<bool>,
    pub strikethrough: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub border: BorderPreset,
    pub section_divider: DividerPreset,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderPreset {
    #[default]
    Rounded,
    Single,
    Double,
    Ascii,
    None,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DividerPreset {
    #[default]
    Single,
    Double,
    Ascii,
    None,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFileError {
    pub key: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}
impl std::fmt::Display for ThemeFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{}: {}",
            self.key, self.line, self.column, self.message
        )
    }
}
impl std::error::Error for ThemeFileError {}
