//! The 7.2 role table: skin keys per Talaria role, in fallback order.
//!
//! Data, transcribed from the binding table. Behavior contract: issue 227.

/// One role: its fallback key chain and whether it takes the semantic floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleChain {
    /// The role name.
    pub role: &'static str,
    /// Skin keys, first parseable value wins.
    pub keys: &'static [&'static str],
    /// True for the semantic (higher-floor) roles.
    pub semantic: bool,
}

/// Every role chain of the binding table.
pub static CHAINS: &[RoleChain] = &[
    RoleChain {
        role: "primary",
        keys: &["ui_primary", "banner_title"],
        semantic: false,
    },
    RoleChain {
        role: "accent",
        keys: &["ui_accent", "banner_accent"],
        semantic: false,
    },
    RoleChain {
        role: "border",
        keys: &["input_rule", "ui_border", "banner_border"],
        semantic: false,
    },
    RoleChain {
        role: "prompt",
        keys: &["prompt", "banner_text"],
        semantic: false,
    },
    RoleChain {
        role: "muted",
        keys: &["banner_dim"],
        semantic: false,
    },
    RoleChain {
        role: "thinking",
        keys: &["ui_thinking", "banner_dim"],
        semantic: false,
    },
    RoleChain {
        role: "tool",
        keys: &["ui_tool", "ui_accent", "banner_accent"],
        semantic: false,
    },
    RoleChain {
        role: "ok",
        keys: &["ui_ok"],
        semantic: true,
    },
    RoleChain {
        role: "error",
        keys: &["ui_error"],
        semantic: true,
    },
    RoleChain {
        role: "diff added",
        keys: &["diff_added_word", "ui_ok"],
        semantic: true,
    },
    RoleChain {
        role: "diff removed",
        keys: &["diff_removed_word", "ui_error"],
        semantic: true,
    },
    RoleChain {
        role: "diff hunk",
        keys: &["session_border"],
        semantic: false,
    },
    RoleChain {
        role: "diff file",
        keys: &["session_label"],
        semantic: false,
    },
    RoleChain {
        role: "status text",
        keys: &["status_bar_text"],
        semantic: false,
    },
    RoleChain {
        role: "status muted",
        keys: &["status_bar_dim"],
        semantic: false,
    },
    RoleChain {
        role: "status success",
        keys: &["status_bar_good", "ui_ok"],
        semantic: true,
    },
    RoleChain {
        role: "status warning",
        keys: &["status_bar_warn", "ui_warn"],
        semantic: true,
    },
    RoleChain {
        role: "status error",
        keys: &["status_bar_critical", "status_bar_bad", "ui_error"],
        semantic: true,
    },
    RoleChain {
        role: "status attention",
        keys: &["status_bar_strong", "ui_accent"],
        semantic: true,
    },
];

/// The chain for one role, if it has one.
pub fn chain(role: &str) -> Option<&'static [&'static str]> {
    CHAINS
        .iter()
        .find(|entry| entry.role == role)
        .map(|entry| entry.keys)
}
