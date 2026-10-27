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
#[derive(Clone, PartialEq, Eq)]
pub struct AccountLine {
    pub name: String,
    pub status: String,
    pub connection_id: String,
}

impl std::fmt::Debug for AccountLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountLine")
            .field("name_len", &self.name.len())
            .field("connection_len", &self.connection_id.len())
            .finish()
    }
}

/// Why a typed alias cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasProblem {
    Invalid,
    Taken,
}

/// Live accounts, then retired ones when requested.
pub fn account_view(rows: &[AccountRow], show_retired: bool) -> AccountView {
    let retired = rows.iter().filter(|row| row.disabled).count();
    let mut sorted: Vec<_> = rows
        .iter()
        .filter(|row| show_retired || !row.disabled)
        .collect();
    let name = |row: &AccountRow| {
        row.alias
            .clone()
            .unwrap_or_else(|| format!("{} (unnamed)", row.label))
    };
    sorted.sort_by(|a, b| {
        a.disabled
            .cmp(&b.disabled)
            .then_with(|| name(a).to_lowercase().cmp(&name(b).to_lowercase()))
            .then_with(|| a.connection_id.cmp(&b.connection_id))
    });
    AccountView {
        rows: sorted
            .into_iter()
            .map(|row| AccountLine {
                name: super::text::sanitize(&name(row)),
                status: if row.disabled {
                    "retired".into()
                } else {
                    super::text::sanitize(&row.status)
                },
                connection_id: row.connection_id.clone(),
            })
            .collect(),
        retired,
    }
}

/// Alias shape and uniqueness for one connector.
pub fn alias_problem(
    name: &str,
    connector: &str,
    accounts: &[AccountRow],
    own: Option<&str>,
) -> Option<AliasProblem> {
    let mut bytes = name.bytes();
    if name.len() > 32
        || !bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        || !bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Some(AliasProblem::Invalid);
    }
    accounts
        .iter()
        .any(|row| {
            row.connector == connector
                && !row.disabled
                && row.alias.as_deref() == Some(name)
                && own != Some(row.connection_id.as_str())
        })
        .then_some(AliasProblem::Taken)
}
