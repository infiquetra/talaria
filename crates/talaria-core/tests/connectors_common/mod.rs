//! Shared fixture loads for the connectors tests.

#![allow(dead_code)]

use serde::Deserialize;
use talaria_core::connectors::{
    AccountRow, CatalogReply, ConnectorsInput, ConnectorsOpen, ConnectorsTab, ListReply, Pairing,
    PlatformsReply, Policy, ToolsReply,
};

/// Open the overlay after consent. Post-consent flows start here.
pub fn open_consented(tab: ConnectorsTab) -> ConnectorsInput {
    ConnectorsInput::Open {
        open: ConnectorsOpen::Tab(tab),
        profile: PROFILE.into(),
        consented: true,
    }
}

pub const PROFILE: &str = "fixture";
pub const REVISION: &str = "01J00000000000000000000000";

pub fn fixture(name: &str) -> &'static str {
    match name {
        "list" => include_str!("../fixtures/connectors/list.json"),
        "catalog" => include_str!("../fixtures/connectors/catalog.json"),
        "accounts" => include_str!("../fixtures/connectors/accounts.json"),
        "policy-deny" => include_str!("../fixtures/connectors/policy-deny.json"),
        "policy-modes" => include_str!("../fixtures/connectors/policy-modes.json"),
        "operation-open" => include_str!("../fixtures/connectors/operation-open.json"),
        "connection-update" => include_str!("../fixtures/connectors/connection-update.json"),
        "platforms" => include_str!("../fixtures/connectors/platforms.json"),
        "pairing" => include_str!("../fixtures/connectors/pairing.json"),
        "tools" => include_str!("../fixtures/connectors/tools.json"),
        other => panic!("unknown fixture {other}"),
    }
}

pub fn scrub_fixtures() {
    for name in [
        "list",
        "catalog",
        "accounts",
        "policy-deny",
        "policy-modes",
        "operation-open",
        "connection-update",
        "platforms",
        "pairing",
        "tools",
    ] {
        scrub(fixture(name));
    }
}

pub fn scrub(text: &str) {
    for needle in [
        "/Users/",
        "/home/",
        &hidden("un", "attended"),
        &hidden("f", "inite"),
    ] {
        assert!(!text.contains(needle), "fixture holds a forbidden token");
    }
    for needle in planning_needles() {
        assert!(!text.contains(&needle), "fixture holds a planning token");
    }
    if let Ok(name) = std::env::var("USER")
        && name.len() >= 3
    {
        assert!(!text.contains(&name));
    }
}

fn hidden(prefix: &str, rest: &str) -> String {
    format!("{prefix}{rest}")
}

fn planning_needles() -> Vec<String> {
    ["P", "PH", "UA", "RE", "C", "D", "L"]
        .into_iter()
        .zip(["4-", "4", "T-", "Q-", "-", "-", "-"])
        .map(|(prefix, rest)| format!("{prefix}{rest}"))
        .collect()
}

pub fn list_reply() -> ListReply {
    serde_json::from_str(fixture("list")).expect("list fixture")
}

pub fn catalog_reply() -> CatalogReply {
    serde_json::from_str(fixture("catalog")).expect("catalog fixture")
}

#[derive(Deserialize)]
struct AccountsFile {
    accounts: Vec<AccountRow>,
}

pub fn accounts() -> Vec<AccountRow> {
    serde_json::from_str::<AccountsFile>(fixture("accounts"))
        .expect("accounts fixture")
        .accounts
}

pub fn policy_deny() -> Policy {
    serde_json::from_str(fixture("policy-deny")).expect("policy fixture")
}

pub fn platforms() -> PlatformsReply {
    serde_json::from_str(fixture("platforms")).expect("platforms fixture")
}

pub fn pairing() -> Pairing {
    serde_json::from_str(fixture("pairing")).expect("pairing fixture")
}

pub fn tools() -> ToolsReply {
    serde_json::from_str(fixture("tools")).expect("tools fixture")
}
