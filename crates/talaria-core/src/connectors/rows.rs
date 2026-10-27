//! Connector list rows and the label chosen for each one.

use serde::Deserialize;

use super::accounts::AccountRow;

/// One connector from `connectors.list`.
#[derive(Clone, PartialEq, Deserialize)]
pub struct ConnectorRow {
    pub connector: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub connection_status: Option<String>,
    #[serde(default)]
    pub status_reason: Option<String>,
    #[serde(default)]
    pub gateway_disabled_tools: Vec<String>,
    /// Set by the joiner when an organisation or role layer locks the slug.
    #[serde(default)]
    pub org_locked: bool,
    /// False when the row exists only in the catalogue.
    #[serde(default = "listed_by_default")]
    pub listed: bool,
}

fn listed_by_default() -> bool {
    true
}

impl std::fmt::Debug for ConnectorRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectorRow")
            .field("connector", &self.connector)
            .field("enabled", &self.enabled)
            .field("connected", &self.connected)
            .field("connection_status", &self.connection_status)
            .field("org_locked", &self.org_locked)
            .field("listed", &self.listed)
            .field("tools", &self.gateway_disabled_tools.len())
            .finish()
    }
}

/// One catalogue entry.
#[derive(Clone, PartialEq, Deserialize)]
pub struct CatalogRow {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

impl std::fmt::Debug for CatalogRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CatalogRow")
            .field("slug", &self.slug)
            .field("name_len", &self.name.len())
            .finish()
    }
}

/// The list reply.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ListReply {
    pub available: bool,
    #[serde(default)]
    pub connectors: Vec<ConnectorRow>,
}

/// The catalogue reply.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CatalogReply {
    #[serde(default)]
    pub connectors: Vec<CatalogRow>,
}

/// First matching label for a connector row.
pub fn connector_label(row: &ConnectorRow, accounts: &[AccountRow]) -> String {
    if row.org_locked {
        return "off by your organization".into();
    }
    if !row.enabled {
        return "off".into();
    }
    let count = accounts
        .iter()
        .filter(|account| {
            account.connector == row.connector && account.status == "active" && !account.disabled
        })
        .count();
    if count > 0 {
        return format!(
            "connected · {count} account{}",
            if count == 1 { "" } else { "s" }
        );
    }
    if row.connected {
        return "connected".into();
    }
    match row.connection_status.as_deref() {
        Some("pending") => "connecting…".into(),
        Some(status @ ("failed" | "expired" | "revoked" | "inactive")) => {
            format!("needs repair ({status})")
        }
        _ if !row.listed => "not offered".into(),
        _ => "not connected".into(),
    }
}
