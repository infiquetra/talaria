//! Pure readers for capability and integration screens.

#[derive(Debug, PartialEq, Eq)]
pub struct McpRow {
    pub name: String,
    pub state: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CatalogueRow {
    pub name: String,
    pub transport: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CapabilityRow {
    pub name: String,
    pub enabled: Option<bool>,
    pub source: Option<String>,
    pub tools: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CronRow {
    pub name: String,
    pub schedule: String,
    pub state: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PlatformRow {
    pub name: String,
    pub state: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FieldState {
    Set,
    NotSet,
    Stored(String),
}

pub fn overlay_title_found(_text: &str, _title_fmt: &str, _profile: &str) -> bool {
    panic!("not implemented")
}

pub fn mcp_rows(_text: &str) -> Vec<McpRow> {
    panic!("not implemented")
}

pub fn catalogue_rows(_text: &str) -> Vec<CatalogueRow> {
    panic!("not implemented")
}

pub fn catalogue_transport_found(_text: &str) -> bool {
    panic!("not implemented")
}

pub fn capabilities_tab_rows(_text: &str) -> Vec<CapabilityRow> {
    panic!("not implemented")
}

pub fn capabilities_categories(_text: &str) -> Vec<String> {
    panic!("not implemented")
}

pub fn memory_sections(_text: &str) -> Vec<String> {
    panic!("not implemented")
}

pub fn cron_rows(_text: &str) -> Vec<CronRow> {
    panic!("not implemented")
}

pub fn cron_empty_found(_text: &str, _profile: &str) -> bool {
    panic!("not implemented")
}

pub fn platform_rows(_text: &str) -> Vec<PlatformRow> {
    panic!("not implemented")
}

pub fn field_state(_text: &str, _field: &str) -> Option<FieldState> {
    panic!("not implemented")
}

pub fn refusal_found(_text: &str, _exact: &str) -> bool {
    panic!("not implemented")
}

pub fn write_banner_found(_text: &str) -> bool {
    panic!("not implemented")
}

pub fn consent_found(_text: &str, _profile: &str) -> bool {
    panic!("not implemented")
}

pub fn idle_screen(_text: &str) -> bool {
    panic!("not implemented")
}
