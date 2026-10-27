use crate::commands::local::LocalAction;
#[derive(Clone, Copy, Debug)]
pub struct SettingsLink {
    pub label: &'static str,
    pub action: LocalAction,
}
pub const SETTINGS_LINKS: &[SettingsLink] = &[];
