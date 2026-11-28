//! Built-in files use the same strict parser as user themes.
use super::{ThemeFile, parse};

struct Builtin {
    id: &'static str,
    description: &'static str,
    source: &'static str,
}

macro_rules! builtins {
    ($($id:literal => $description:literal),+ $(,)?) => {
        /// The built-in theme identifiers, in the order the `/theme` selector lists them.
        pub const BUILTIN_THEMES: [&str; BUILTINS.len()] = [$($id),+];

        const BUILTINS: &[Builtin] = &[
            $(Builtin {
                id: $id,
                description: $description,
                source: include_str!(concat!("../../themes/", $id, ".toml")),
            }),+
        ];
    };
}

builtins! {
    "terminal" => "Terminal defaults and named colours; follows the Hermes skin",
    "ember" => "A warm dark theme; ignores Hermes skins",
    "paper" => "A light theme with blue and brown accents; ignores Hermes skins",
    "homebrew" => "Green on black, cyan headings and gold warnings",
    "catppuccin-mocha" => "Pastel lavender and blue on a dark plum background",
    "catppuccin-latte" => "Mauve and blue on a cool, light background",
    "tokyo-night" => "Cool blue and violet on a midnight background",
    "dracula" => "Violet, hot pink and electric green on charcoal",
    "nord" => "Frost blue and teal on cool slate",
    "gruvbox-dark" => "Orange, ochre and olive on warm charcoal",
    "gruvbox-light" => "Rust, teal and olive on warm cream",
    "rose-pine" => "Dusty rose and iris on a dark plum background",
    "rose-pine-dawn" => "Rose and iris on a pale cream background",
    "solarized-dark" => "Blue and cyan on deep teal",
    "solarized-light" => "Blue and cyan on warm ivory",
}

fn entry(id: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|theme| theme.id == id)
}

/// One line describing a built-in theme for the `/theme` selector.
#[must_use]
pub fn builtin_description(id: &str) -> Option<&'static str> {
    entry(id).map(|theme| theme.description)
}

#[must_use]
pub fn builtin(id: &str) -> Option<ThemeFile> {
    parse(entry(id)?.source).ok()
}
