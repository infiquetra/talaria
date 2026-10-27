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
    SettingsLink {
        label: "Model roles",
        action: LocalAction::ModelRoles,
    },
    SettingsLink {
        label: "MCP servers",
        action: LocalAction::Mcp(crate::mcp::McpOpen::Tab(crate::mcp::McpTab::Servers)),
    },
    SettingsLink {
        label: "Skills, toolsets and plugins",
        action: LocalAction::Capabilities,
    },
];
/// The labels of the settings page's link rows, in order.
pub const PAGE_ROWS: &[&str] = &[
    "Profiles",
    "Providers",
    "Model roles",
    "MCP servers",
    "Skills, toolsets and plugins",
];
