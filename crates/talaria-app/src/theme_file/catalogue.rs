use std::sync::OnceLock;
pub const ROLES: [&str; 14] = [
    "background",
    "surface",
    "text",
    "muted",
    "disabled",
    "border",
    "primary",
    "accent",
    "success",
    "warning",
    "error",
    "attention",
    "selection",
    "focus",
];
pub const PALETTE_SLOTS: [&str; 12] = [
    "background",
    "surface",
    "foreground",
    "muted",
    "disabled",
    "border",
    "primary",
    "accent",
    "success",
    "warning",
    "error",
    "attention",
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueEntry {
    pub token: &'static str,
    pub role: &'static str,
    pub attributes: &'static str,
    pub description: &'static str,
    pub reserved: bool,
    pub area: &'static str,
}
fn family_area(token: &str) -> &'static str {
    match token.split('.').next().unwrap_or("") {
        "transcript" | "composer" | "status" | "tool" | "diff" | "workspace" => "background",
        _ => "surface",
    }
}
pub fn catalogue() -> &'static [CatalogueEntry] {
    static ENTRIES: OnceLock<Vec<CatalogueEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        include_str!("catalogue.txt")
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let mut parts = line.split('|');
                // Each embedded row has four fields; catalogue tests pin all 323 records.
                let token = parts.next().expect("catalogue token");
                CatalogueEntry {
                    token,
                    role: parts.next().expect("catalogue role"),
                    attributes: parts.next().expect("catalogue attributes"),
                    description: parts.next().expect("catalogue description"),
                    reserved: token.starts_with("picker."),
                    area: family_area(token),
                }
            })
            .collect()
    })
}
pub fn area_background(token: &str) -> Option<&'static str> {
    catalogue()
        .iter()
        .find(|entry| entry.token == token)
        .map(|entry| entry.area)
}
