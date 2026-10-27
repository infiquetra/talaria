use crate::commands::local::LocalAction;
#[derive(Clone, Debug)]
pub struct SettingsLink {
    pub label: &'static str,
    pub action: LocalAction,
}
pub const SETTINGS_LINKS: &[SettingsLink] = &[
    SettingsLink {
        label: "Profiles",
        action: LocalAction::Profiles,
    },
    SettingsLink {
        label: "Providers",
        action: LocalAction::Providers(crate::providers::OpenTarget::Tab(
            crate::providers::ProvidersTab::Keys,
        )),
    },
];
