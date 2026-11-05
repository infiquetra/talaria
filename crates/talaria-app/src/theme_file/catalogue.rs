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
pub struct CatalogueEntry {
    pub token: &'static str,
    pub role: &'static str,
    pub attributes: &'static str,
    pub description: &'static str,
    pub reserved: bool,
    pub area: &'static str,
}
pub fn catalogue() -> &'static [CatalogueEntry] {
    &[]
}
pub fn area_background(_: &str) -> Option<&'static str> {
    None
}
