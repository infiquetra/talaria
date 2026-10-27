//! Screen readers for the configuration screens suite.

#[derive(Debug, PartialEq, Eq)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub flags: Vec<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ProviderRow {
    pub name: String,
    pub status: String,
    pub key_tail: Option<String>,
}
pub fn settings_title_found(_text: &str, _profile: &str) -> bool {
    panic!("not implemented")
}
pub fn settings_categories(_text: &str) -> Vec<String> {
    panic!("not implemented")
}
pub fn setting_row(_text: &str, _key: &str) -> Option<SettingRow> {
    panic!("not implemented")
}
pub fn providers_row(_text: &str, _name: &str) -> Option<ProviderRow> {
    panic!("not implemented")
}
pub fn providers_tabs(_text: &str) -> Vec<String> {
    panic!("not implemented")
}
pub fn models_sections(_text: &str) -> Vec<String> {
    panic!("not implemented")
}
pub fn models_failed(_text: &str) -> bool {
    panic!("not implemented")
}
pub fn profiles_rows(_text: &str) -> Vec<String> {
    panic!("not implemented")
}
pub fn refusal_found(_text: &str, _exact: &str) -> bool {
    panic!("not implemented")
}
pub fn write_banner_found(_text: &str) -> bool {
    panic!("not implemented")
}
pub fn doctor_rows(_text: &str) -> Vec<String> {
    panic!("not implemented")
}
pub fn doctor_json(_text: &str) -> Option<(String, usize)> {
    panic!("not implemented")
}
pub fn doctor_summary(_text: &str) -> Option<(usize, usize, usize)> {
    panic!("not implemented")
}
pub fn idle_screen(_text: &str) -> bool {
    panic!("not implemented")
}
