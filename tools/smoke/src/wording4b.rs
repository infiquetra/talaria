//! Exact texts used when reading capability and integration screens.

// Server management: profile title, empty list, and catalogue transport labels.
pub const MCP_TITLE_FMT: &str = "MCP servers · profile <name>";
pub const MCP_EMPTY_FMT: &str = "No MCP servers in profile <name> yet.";
pub const MCP_REMOTE: &str = "remote";
pub const MCP_BAD_TRANSPORT: &str = "TransportSpec(";

// Capabilities: profile title, enabled boxes, tool count, and bundled source.
pub const CAPABILITIES_TITLE_FMT: &str = "Capabilities · profile <name>";
pub const CAPABILITIES_ON: &str = "[x]";
pub const CAPABILITIES_OFF: &str = "[ ]";
pub const CAPABILITIES_TOOLS_RE: &str = r"([0-9]+) tools";
pub const CAPABILITIES_BUNDLED: &str = "bundled";
pub const CAPABILITIES_PLUGINS_EMPTY_FMT: &str = "You have not installed any plugins in profile <p>. Press a to show the <n> that ship with Hermes, or i to install one.";
// Capabilities: skills are grouped by named categories, then an uncategorized group.
pub const CAPABILITIES_NO_CATEGORY: &str = "(no category)";
// Capabilities commands: text header and invalid list refusal.
pub const CAPABILITIES_CLI_HEADER: &str = "NAME  STATE  SOURCE  CATEGORY";
pub const CAPABILITIES_UNKNOWN_FMT: &str =
    "talaria capabilities: unknown list <name>; use skills, toolsets or plugins";
// Gateway discovery: missing gateway and the next action.
pub const NO_GATEWAY_FMT: &str = "talaria: no Hermes gateway is running for this home";
pub const NEXT_STEP: &str = "Next step:";

// Memory: profile title and the ordered section headings.
pub const MEMORY_TITLE_FMT: &str = "Memory · profile <name>";
pub const MEMORY_SECTIONS: [&str; 3] = ["MEMORY PROVIDER", "BUILT-IN MEMORY", "SKILL CURATOR"];
// Memory: built-in provider, empty files, file labels, and curator states.
pub const MEMORY_BUILT_IN: &str = "built-in only";
pub const MEMORY_EMPTY: &str = "empty";
pub const MEMORY_FILE_ROWS: [&str; 2] = ["MEMORY.md", "USER.md"];
pub const MEMORY_CURATOR_STATES: [&str; 3] = ["active", "paused", "off"];
// Memory commands: gateway replies and the unknown-command refusal.
pub const MEMORY_PENDING_NONE: &str = "No pending memory writes.";
pub const MEMORY_PENDING_SOME: &str = "Pending memory writes (";
pub const CURATOR_STATUS_RE: &str = r"curator: (ENABLED|PAUSED|DISABLED)";
pub const UNKNOWN_COMMAND: &str = "it is not a command this gateway runs.";
pub const UNKNOWN_COMMAND_PREFIX: &str = "Hermes refused /";

// Scheduled jobs: profile title, empty list, and visible state words.
pub const CRON_TITLE_FMT: &str = "Scheduled jobs · profile <name>";
pub const CRON_EMPTY_FMT: &str = "No scheduled jobs in profile <name>. ctrl+n creates one.";
pub const CRON_STATES: [&str; 2] = ["next in", "paused"];

// Messaging: profile title, platform name, field name, and masked state.
pub const MESSAGING_TITLE_FMT: &str = "Connectors and messaging · profile <name>";
pub const SIGNAL_ROW: &str = "Signal";
pub const SIGNAL_URL_FIELD: &str = "SIGNAL_HTTP_URL";
pub const FIELD_SET: &str = "set";
pub const FIELD_NOT_SET: &str = "not set";

// Connectors: opening consent explains the hosted service and names the profile.
pub const CONNECTORS_CONSENT_FMT: &str = "Connectors run through Nous Research's hosted tool gateway. If profile <p> has no Nous sign-in, Hermes creates a free guest identity for it when you continue.";
pub const CONNECTORS_CONSENT_HINT: &str = "enter continue · esc back";

// Configure: requests containing secret-shaped input are refused before opening a view.
pub const CONFIGURE_SECRET_REFUSAL: &str = "That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.";

// Inline chat: an empty composer.
pub const COMPOSER: &str = ">";

// Integration screens: notices which mean a write occurred.
pub const WRITE_BANNERS: [&str; 15] = [
    "Saved.",
    "Saved the key",
    "MCP servers reloaded.",
    "Installed ",
    "Removed ",
    "Deleted ",
    "Cron job '",
    "Curator paused",
    "Curator resumed",
    "Curator running",
    "Configured profile",
    "Pairing approved",
    "Renamed to",
    "Signed in to",
    "Applying ",
];
