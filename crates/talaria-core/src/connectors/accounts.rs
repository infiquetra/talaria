//! Account rows, the alias check, and the card's account list.

use serde::Deserialize;

/// One connector account.
#[derive(Clone, PartialEq, Deserialize)]
pub struct AccountRow {
    pub connection_id: String,
    pub connector: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub status_reason: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub disabled: bool,
}

impl std::fmt::Debug for AccountRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountRow")
            .field("connector", &self.connector)
            .field("status", &self.status)
            .field("disabled", &self.disabled)
            .field("label_len", &self.label.len())
            .field("alias_len", &self.alias.as_ref().map(String::len))
            .field("connection_len", &self.connection_id.len())
            .finish()
    }
}

/// Accounts as the card lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountView {
    pub rows: Vec<AccountLine>,
    pub retired: usize,
}

/// One account line. `name` is the alias, or the label with an unnamed marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountLine {
    pub name: String,
    pub status: String,
    pub connection_id: String,
}

/// Why a typed alias cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasProblem {
    Invalid,
    Taken,
}

/// Live accounts, then retired ones when requested.
pub fn account_view(rows: &[AccountRow], show_retired: bool) -> AccountView {
    let _ = (rows, show_retired);
    todo!("connectors stub")
}

/// Alias shape and uniqueness for one connector.
pub fn alias_problem(
    name: &str,
    connector: &str,
    accounts: &[AccountRow],
    own: Option<&str>,
) -> Option<AliasProblem> {
    let _ = (name, connector, accounts, own);
    todo!("connectors stub")
}
