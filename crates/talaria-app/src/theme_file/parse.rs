use super::{
    Base, BorderPreset, Chrome, ColorValue, DividerPreset, PALETTE_SLOTS, ROLES, StyleFields,
    ThemeFile, ThemeFileError, catalogue,
};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;
use toml::Spanned;

type Map = BTreeMap<Spanned<String>, Spanned<Node>>;
#[derive(Debug)]
enum Node {
    Map(Map),
    String(String),
    Integer(i64),
    Bool(bool),
    Other,
}
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NodeVisitor;
        impl<'de> Visitor<'de> for NodeVisitor {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a TOML value")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Node, M::Error> {
                Map::deserialize(de::value::MapAccessDeserializer::new(map)).map(Node::Map)
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Node, S::Error> {
                while seq.next_element::<de::IgnoredAny>()?.is_some() {}
                Ok(Node::Other)
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Node, E> {
                Ok(Node::String(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Node, E> {
                Ok(Node::String(value))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Node, E> {
                Ok(Node::Integer(value))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Node, E> {
                Ok(i64::try_from(value).map_or(Node::Other, Node::Integer))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Node, E> {
                Ok(Node::Other)
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Node, E> {
                Ok(Node::Bool(value))
            }
        }
        deserializer.deserialize_any(NodeVisitor)
    }
}

fn error(input: &str, key: &str, offset: usize, message: &str) -> ThemeFileError {
    let offset = offset.min(input.len());
    let prefix = input.get(..offset).unwrap_or("");
    ThemeFileError {
        key: key.to_owned(),
        line: prefix.bytes().filter(|b| *b == b'\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
        message: message.to_owned(),
    }
}
fn table<'a>(input: &str, key: &str, at: usize, node: &'a Node) -> Result<&'a Map, ThemeFileError> {
    match node {
        Node::Map(map) => Ok(map),
        _ => Err(error(input, key, at, "expected a table")),
    }
}
fn color(
    input: &str,
    key: &str,
    at: usize,
    node: &Node,
    references: bool,
) -> Result<ColorValue, ThemeFileError> {
    let Node::String(value) = node else {
        return Err(error(input, key, at, "expected a colour string"));
    };
    if value == "default" {
        return Ok(ColorValue::Default);
    }
    if let Some(hex) = value.strip_prefix('#')
        && hex.len() == 6
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
    {
        let n = u32::from_str_radix(hex, 16)
            .map_err(|_| error(input, key, at, "invalid hexadecimal colour"))?;
        return Ok(ColorValue::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8));
    }
    for (prefix, max, ansi) in [("ansi:", 15, true), ("index:", 255, false)] {
        if let Some(number) = value.strip_prefix(prefix)
            && let Ok(n) = number.parse::<u8>()
            && n <= max
            && number == n.to_string()
        {
            return Ok(if ansi {
                ColorValue::Ansi(n)
            } else {
                ColorValue::Index(n)
            });
        }
    }
    if references
        && let Some(slot) = value.strip_prefix("$palette.")
        && PALETTE_SLOTS.contains(&slot)
    {
        return Ok(ColorValue::Palette(slot.to_owned()));
    }
    Err(error(input, key, at, "invalid colour"))
}
fn style(input: &str, path: &str, at: usize, node: &Node) -> Result<StyleFields, ThemeFileError> {
    let mut result = StyleFields::default();
    for (key, value) in table(input, path, at, node)? {
        let name = key.get_ref();
        let full = format!("{path}.{name}");
        let pos = key.span().start;
        match name.as_str() {
            "fg" | "bg" | "underline_color" => {
                let c = Some(color(input, &full, pos, value.get_ref(), true)?);
                match name.as_str() {
                    "fg" => result.fg = c,
                    "bg" => result.bg = c,
                    _ => result.underline_color = c,
                }
            }
            "bold" | "dim" | "italic" | "underline" | "reverse" | "strikethrough" => {
                let Node::Bool(b) = value.get_ref() else {
                    return Err(error(input, &full, pos, "expected a boolean"));
                };
                match name.as_str() {
                    "bold" => result.bold = Some(*b),
                    "dim" => result.dim = Some(*b),
                    "italic" => result.italic = Some(*b),
                    "underline" => result.underline = Some(*b),
                    "reverse" => result.reverse = Some(*b),
                    _ => result.strikethrough = Some(*b),
                }
            }
            _ => return Err(error(input, &full, pos, "unknown style field")),
        }
    }
    Ok(result)
}

pub fn parse(input: &str) -> Result<ThemeFile, ThemeFileError> {
    let root: Map = toml::from_str(input).map_err(|e: toml::de::Error| {
        error(
            input,
            "syntax",
            e.span().map_or(0, |span| span.start),
            &e.to_string(),
        )
    })?;
    let mut result = ThemeFile {
        name: None,
        extends: Base::Terminal,
        follow_hermes_skin: true,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        elements: BTreeMap::new(),
        chrome: Chrome::default(),
    };
    let mut schema = false;
    for (key, value) in &root {
        let name = key.get_ref().as_str();
        let pos = key.span().start;
        let node = value.get_ref();
        match name {
            "schema" => {
                if !matches!(node, Node::Integer(1)) {
                    return Err(error(input, name, pos, "schema must be 1"));
                }
                schema = true;
            }
            "name" => {
                let Node::String(s) = node else {
                    return Err(error(input, name, pos, "expected a string"));
                };
                result.name = Some(s.clone());
            }
            "extends" => {
                result.extends = match node {
                    Node::String(s) if s == "terminal" => Base::Terminal,
                    Node::String(s) if s == "dark" => Base::Dark,
                    Node::String(s) if s == "light" => Base::Light,
                    _ => return Err(error(input, name, pos, "expected terminal, dark or light")),
                };
            }
            "follow_hermes_skin" => {
                let Node::Bool(b) = node else {
                    return Err(error(input, name, pos, "expected a boolean"));
                };
                result.follow_hermes_skin = *b;
            }
            "palette" => {
                for (slot, value) in table(input, name, pos, node)? {
                    let path = format!("palette.{}", slot.get_ref());
                    let at = slot.span().start;
                    if !PALETTE_SLOTS.contains(&slot.get_ref().as_str()) {
                        return Err(error(input, &path, at, "unknown palette slot"));
                    }
                    result.palette.insert(
                        slot.get_ref().clone(),
                        color(input, &path, at, value.get_ref(), false)?,
                    );
                }
            }
            "roles" | "elements" => {
                for (entry, value) in table(input, name, pos, node)? {
                    let entry_name = entry.get_ref();
                    let path = format!("{name}.{entry_name}");
                    let at = entry.span().start;
                    let known = if name == "roles" {
                        ROLES.contains(&entry_name.as_str())
                    } else {
                        catalogue().iter().any(|item| item.token == entry_name)
                    };
                    if !known {
                        return Err(error(input, &path, at, "unknown name"));
                    }
                    let fields = style(input, &path, at, value.get_ref())?;
                    if name == "roles" {
                        result.roles.insert(entry_name.clone(), fields);
                    } else {
                        result.elements.insert(entry_name.clone(), fields);
                    }
                }
            }
            "chrome" => {
                for (entry, value) in table(input, name, pos, node)? {
                    let path = format!("chrome.{}", entry.get_ref());
                    let at = entry.span().start;
                    match (entry.get_ref().as_str(), value.get_ref()) {
                        ("border", Node::String(s)) => {
                            result.chrome.border = match s.as_str() {
                                "rounded" => BorderPreset::Rounded,
                                "single" => BorderPreset::Single,
                                "double" => BorderPreset::Double,
                                "ascii" => BorderPreset::Ascii,
                                "none" => BorderPreset::None,
                                _ => return Err(error(input, &path, at, "invalid border")),
                            }
                        }
                        ("section_divider", Node::String(s)) => {
                            result.chrome.section_divider = match s.as_str() {
                                "single" => DividerPreset::Single,
                                "double" => DividerPreset::Double,
                                "ascii" => DividerPreset::Ascii,
                                "none" => DividerPreset::None,
                                _ => return Err(error(input, &path, at, "invalid divider")),
                            }
                        }
                        _ => {
                            return Err(error(
                                input,
                                &path,
                                at,
                                "unknown or invalid chrome setting",
                            ));
                        }
                    }
                }
            }
            _ => return Err(error(input, name, pos, "unknown key")),
        }
    }
    if !schema {
        return Err(error(input, "schema", 0, "schema is required"));
    }
    Ok(result)
}
