//! Screen readers against synthetic capability and integration examples.

use smoke::screen4b::*;
use smoke::wording4b::*;

fn screen(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/screens/phase4b")
            .join(name),
    )
    .expect("synthetic screen")
}

#[test]
fn overlay_title_found_with_profile() {
    for (file, title, exact) in [
        (
            "mcp-browse-r1.txt",
            MCP_TITLE_FMT,
            "MCP servers · profile fixture",
        ),
        (
            "capabilities-browse-r1.txt",
            CAPABILITIES_TITLE_FMT,
            "Capabilities · profile fixture",
        ),
        (
            "memory-open-r1.txt",
            MEMORY_TITLE_FMT,
            "Memory · profile fixture",
        ),
        (
            "cron-browse-r1.txt",
            CRON_TITLE_FMT,
            "Scheduled jobs · profile fixture",
        ),
        (
            "messaging-browse-r1.txt",
            MESSAGING_TITLE_FMT,
            "Connectors and messaging · profile fixture",
        ),
    ] {
        let text = screen(file);
        assert_eq!(title.replace("<name>", "fixture"), exact);
        assert!(overlay_title_found(&text, title, "fixture"));
        assert!(!overlay_title_found(&text, title, "default"));
        assert!(!overlay_title_found(&screen("idle.txt"), title, "fixture"));
        assert!(!overlay_title_found(
            &format!("notice: {exact}"),
            title,
            "fixture"
        ));
    }
}

#[test]
fn mcp_rows_parsed_with_state() {
    assert_eq!(
        mcp_rows(&screen("mcp-rows.txt")),
        [
            McpRow {
                name: "fixture-docs".into(),
                state: "connected · 3 tools".into()
            },
            McpRow {
                name: "fixture-local".into(),
                state: "off".into()
            },
        ]
    );
    assert!(mcp_rows(&screen("mcp-browse-r1.txt")).is_empty());
    assert!(mcp_rows(&screen("mcp-rows-no-state.txt")).is_empty());
    assert!(mcp_rows("Servers  Catalogue\nNAME  KIND  STATE\nEsc close").is_empty());
}

#[test]
fn catalogue_remote_rows_counted() {
    let rows = catalogue_rows(&screen("mcp-browse-r2.txt"));
    assert_eq!(MCP_REMOTE, "remote");
    assert_eq!(rows.len(), 10);
    assert!(rows.iter().all(|row| row.transport == MCP_REMOTE));
    assert_eq!(rows.iter().filter(|row| row.name == "deepwiki").count(), 1);
    assert_eq!(
        catalogue_rows(&screen("mcp-browse-r3.txt")),
        [CatalogueRow {
            name: "deepwiki".into(),
            transport: "remote".into()
        }]
    );
    assert!(catalogue_rows("Catalogue\n10 servers in Hermes's catalogue\nremote").is_empty());
}

#[test]
fn catalogue_transport_text_detected() {
    assert_eq!(MCP_BAD_TRANSPORT, "TransportSpec(");
    assert!(catalogue_transport_found(&screen(
        "catalogue-transport.txt"
    )));
    assert!(!catalogue_transport_found(&screen("mcp-browse-r2.txt")));
    assert!(catalogue_transport_found(
        "deepwiki  TransportSpec(type='http')"
    ));
    assert!(!catalogue_transport_found("deepwiki  remote"));
}

#[test]
fn capabilities_tabs_rows_parsed() {
    let skills = screen("capabilities-browse-r1.txt");
    assert_eq!(CAPABILITIES_ON, "[x]");
    assert_eq!(CAPABILITIES_OFF, "[ ]");
    assert_eq!(CAPABILITIES_BUNDLED, "bundled");
    assert_eq!(CAPABILITIES_NO_CATEGORY, "(no category)");
    assert_eq!(
        capabilities_categories(&skills),
        ["creative", "software-development", "(no category)"]
    );
    assert_eq!(
        capabilities_tab_rows(&skills),
        [
            CapabilityRow {
                name: "fixture-art".into(),
                enabled: Some(true),
                source: Some("bundled".into()),
                tools: None
            },
            CapabilityRow {
                name: "fixture-code".into(),
                enabled: Some(false),
                source: Some("hub".into()),
                tools: None
            },
            CapabilityRow {
                name: "fixture-guide".into(),
                enabled: Some(true),
                source: Some("local".into()),
                tools: None
            },
        ]
    );
    assert_eq!(
        capabilities_tab_rows(&screen("capabilities-browse-r4.txt")),
        [
            CapabilityRow {
                name: "fixture-alpha".into(),
                enabled: Some(true),
                source: Some("bundled".into()),
                tools: None
            },
            CapabilityRow {
                name: "fixture-beta".into(),
                enabled: Some(false),
                source: Some("bundled".into()),
                tools: None
            },
        ]
    );
    let empty = CAPABILITIES_PLUGINS_EMPTY_FMT
        .replace("<p>", "fixture")
        .replace("<n>", "2");
    assert!(screen("capabilities-browse-r3.txt").contains(&empty));
    assert!(capabilities_tab_rows(&screen("capabilities-browse-r3.txt")).is_empty());
    assert!(capabilities_tab_rows("Skills  Toolsets  Plugins\n[x]\nSpace on/off").is_empty());
}

#[test]
fn capabilities_tools_count_found() {
    let rows = capabilities_tab_rows(&screen("capabilities-browse-r2.txt"));
    assert_eq!(CAPABILITIES_TOOLS_RE, r"([0-9]+) tools");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Web (web)");
    assert_eq!(rows[0].tools, Some(3));
    assert_eq!(rows[1].name, "Image generation");
    assert_eq!(rows[1].tools, Some(2));
    assert_eq!(
        capabilities_tab_rows(" > [x] Empty toolset                 0 tools")[0].tools,
        Some(0)
    );
    assert!(capabilities_tab_rows("Toolsets\n3 tools\nEnter open").is_empty());
}

#[test]
fn memory_sections_found_in_order() {
    assert_eq!(
        memory_sections(&screen("memory-open-r1.txt")),
        MEMORY_SECTIONS
    );
    let text = screen("memory-open-r1.txt");
    assert_eq!(MEMORY_BUILT_IN, "built-in only");
    assert_eq!(MEMORY_EMPTY, "empty");
    assert_eq!(MEMORY_FILE_ROWS, ["MEMORY.md", "USER.md"]);
    assert_eq!(MEMORY_CURATOR_STATES, ["active", "paused", "off"]);
    assert!(text.contains(MEMORY_BUILT_IN));
    assert!(text.contains("MEMORY.md)     1.2 KB"));
    assert!(text.contains("USER.md)       empty"));
    assert_eq!(
        memory_sections("SKILL CURATOR\nBUILT-IN MEMORY\nMEMORY PROVIDER"),
        ["SKILL CURATOR", "BUILT-IN MEMORY", "MEMORY PROVIDER"]
    );
    assert!(memory_sections("notice: MEMORY PROVIDER\n> MEMORY PROVIDER").is_empty());
}

#[test]
fn cron_rows_parsed_with_state() {
    assert_eq!(
        cron_rows(&screen("cron-browse-r1.txt")),
        [
            CronRow {
                name: "fixture-daily".into(),
                schedule: "0 8 * * *".into(),
                state: "next in 3h 12m".into()
            },
            CronRow {
                name: "fixture-weekly".into(),
                schedule: "0 9 * * 1".into(),
                state: "paused".into()
            },
        ]
    );
    assert_eq!(CRON_STATES, ["next in", "paused"]);
    assert!(cron_rows(&screen("cron-no-state.txt")).is_empty());
    assert!(cron_rows("NAME  SCHEDULE  STATE\nctrl+r reload · esc close").is_empty());
}

#[test]
fn cron_empty_text_found() {
    assert_eq!(
        CRON_EMPTY_FMT.replace("<name>", "fixture"),
        "No scheduled jobs in profile fixture. ctrl+n creates one."
    );
    assert!(cron_empty_found(&screen("cron-empty.txt"), "fixture"));
    assert!(!cron_empty_found(&screen("cron-empty.txt"), "default"));
    assert!(!cron_empty_found(&screen("cron-browse-r1.txt"), "fixture"));
    assert!(!cron_empty_found("No scheduled jobs.", "fixture"));
}

#[test]
fn platform_rows_counted() {
    let rows = platform_rows(&screen("messaging-browse-r1.txt"));
    assert_eq!(rows.len(), 10);
    assert_eq!(rows.iter().filter(|row| row.name == SIGNAL_ROW).count(), 1);
    assert_eq!(
        platform_rows(&screen("messaging-browse-r2.txt")),
        [PlatformRow {
            name: "Signal".into(),
            state: "off".into()
        }]
    );
    assert!(platform_rows("Messaging\nSignal\nState: off\nFields\nWaiting to pair (0)").is_empty());
    assert!(platform_rows("Enter open · / filter · Esc close").is_empty());
}

#[test]
fn field_state_set_or_not_set() {
    assert_eq!(SIGNAL_URL_FIELD, "SIGNAL_HTTP_URL");
    assert_eq!(FIELD_SET, "set");
    assert_eq!(FIELD_NOT_SET, "not set");
    assert_eq!(
        field_state(&screen("messaging-browse-r3.txt"), SIGNAL_URL_FIELD),
        Some(FieldState::NotSet)
    );
    assert_eq!(
        field_state(&screen("signal-set.txt"), SIGNAL_URL_FIELD),
        Some(FieldState::Set)
    );
    assert_eq!(
        field_state(&screen("signal-stored.txt"), SIGNAL_URL_FIELD),
        Some(FieldState::Stored("unexpected-value".into()))
    );
    assert_eq!(field_state("Signal\nState: off", SIGNAL_URL_FIELD), None);
    assert_eq!(
        field_state("SIGNAL_HTTP_URL_BACKUP  set", SIGNAL_URL_FIELD),
        None
    );
    assert_eq!(
        field_state("SIGNAL_HTTP_URL  unset", SIGNAL_URL_FIELD),
        None
    );
}

#[test]
fn consent_found_with_profile() {
    let text = screen("connectors-consent-r1.txt");
    assert!(consent_found(&text, "fixture"));
    assert!(!consent_found(&text, "default"));
    let exact = CONNECTORS_CONSENT_FMT.replace("<p>", "fixture");
    assert_eq!(
        exact,
        "Connectors run through Nous Research's hosted tool gateway. If profile fixture has no Nous sign-in, Hermes creates a free guest identity for it when you continue."
    );
    assert!(text.contains(&exact));
    assert_eq!(CONNECTORS_CONSENT_HINT, "enter continue · esc back");
    assert!(!consent_found(
        &text.replace(CONNECTORS_CONSENT_HINT, ""),
        "fixture"
    ));
    assert!(!consent_found(
        &text.replace("free guest identity", "account"),
        "fixture"
    ));
}

#[test]
fn write_banner_detected() {
    assert_eq!(
        WRITE_BANNERS,
        [
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
        ]
    );
    for banner in WRITE_BANNERS {
        assert!(write_banner_found(&format!("notice: {banner}fixture")));
    }
    assert!(!write_banner_found(&screen("idle.txt")));
    for file in [
        "mcp-browse-r1.txt",
        "mcp-browse-r2.txt",
        "capabilities-browse-r1.txt",
        "capabilities-browse-r2.txt",
        "capabilities-browse-r3.txt",
        "capabilities-browse-r4.txt",
        "memory-open-r1.txt",
        "cron-browse-r1.txt",
        "messaging-browse-r1.txt",
        "messaging-browse-r3.txt",
        "connectors-consent-r1.txt",
    ] {
        assert!(!write_banner_found(&screen(file)), "{file}");
    }
    assert!(refusal_found(
        &screen("configure-secret-refused-r1.txt"),
        CONFIGURE_SECRET_REFUSAL
    ));
    assert!(!refusal_found(
        "That request contains a secret.",
        CONFIGURE_SECRET_REFUSAL
    ));
}

#[test]
fn idle_screen_requires_composer_and_no_title() {
    assert!(idle_screen(&screen("idle.txt")));
    assert!(!idle_screen("Talaria · profile fixture"));
    assert!(!idle_screen(&screen("draft.txt")));
    for title in [
        MCP_TITLE_FMT,
        CAPABILITIES_TITLE_FMT,
        MEMORY_TITLE_FMT,
        CRON_TITLE_FMT,
        MESSAGING_TITLE_FMT,
    ] {
        assert!(!idle_screen(&format!(
            "{}\n>",
            title.replace("<name>", "fixture")
        )));
    }
    assert!(!idle_screen(&format!(
        "{}\n>",
        screen("connectors-consent-r1.txt")
    )));
    assert!(idle_screen(&screen("memory-commands-r2.txt")));
    assert!(idle_screen(&screen("configure-secret-refused-r3.txt")));
}

#[test]
fn combined_binary_recordings_preserve_screen_identity_and_read_only_states() {
    let recorded = |name| screen(&format!("recorded/{name}.txt"));
    for (name, title) in [
        ("mcp-servers", MCP_TITLE_FMT),
        ("mcp-catalogue", MCP_TITLE_FMT),
        ("mcp-deepwiki", MCP_TITLE_FMT),
        ("capabilities-skills", CAPABILITIES_TITLE_FMT),
        ("capabilities-toolsets", CAPABILITIES_TITLE_FMT),
        ("capabilities-plugins", CAPABILITIES_TITLE_FMT),
        ("capabilities-bundled", CAPABILITIES_TITLE_FMT),
        ("memory", MEMORY_TITLE_FMT),
        ("cron", CRON_TITLE_FMT),
        ("messaging", MESSAGING_TITLE_FMT),
        ("messaging-signal", MESSAGING_TITLE_FMT),
        ("signal-card", MESSAGING_TITLE_FMT),
    ] {
        let text = recorded(name);
        assert!(overlay_title_found(&text, title, "fixture"), "{name}");
        assert!(!overlay_title_found(&text, title, "default"), "{name}");
        assert!(!idle_screen(&text), "{name}");
        assert!(!write_banner_found(&text), "{name}");
    }
    assert!(catalogue_rows(&recorded("mcp-catalogue")).len() >= 10);
    assert_eq!(
        catalogue_rows(&recorded("mcp-deepwiki"))[0].name,
        MCP_FILTER
    );
    assert_eq!(catalogue_rows(&recorded("mcp-deepwiki")).len(), 1);
    assert!(!capabilities_categories(&recorded("capabilities-skills")).is_empty());
    assert!(
        capabilities_tab_rows(&recorded("capabilities-toolsets"))
            .iter()
            .any(|r| r.tools.is_some())
    );
    assert!(
        capabilities_tab_rows(&recorded("capabilities-bundled"))
            .iter()
            .any(|r| r.source.as_deref() == Some(CAPABILITIES_BUNDLED))
    );
    assert_eq!(memory_sections(&recorded("memory")), MEMORY_SECTIONS);
    assert!(cron_empty_found(&recorded("cron"), "fixture"));
    assert!(platform_rows(&recorded("messaging")).len() >= 10);
    assert_eq!(platform_rows(&recorded("messaging-signal")).len(), 1);
    assert_eq!(
        platform_rows(&recorded("messaging-signal"))[0].name,
        SIGNAL_ROW
    );
    assert_eq!(
        field_state(&recorded("signal-card"), SIGNAL_URL_FIELD),
        Some(FieldState::NotSet)
    );
    assert!(consent_found(&recorded("connectors-consent"), "fixture"));
    assert!(!consent_found(&recorded("connectors-consent"), "default"));
    for name in ["idle", "memory-pending", "curator-status"] {
        assert!(idle_screen(&recorded(name)), "{name}");
        assert!(!write_banner_found(&recorded(name)), "{name}");
    }
    assert!(recorded("memory-pending").contains(MEMORY_PENDING_NONE));
    assert!(
        regex::Regex::new(CURATOR_STATUS_RE)
            .unwrap()
            .is_match(&recorded("curator-status"))
    );
    let refused = recorded("configure-refusal");
    assert!(refusal_found(&refused, CONFIGURE_SECRET_REFUSAL));
    assert!(idle_screen(&refused));
    assert!(!write_banner_found(&refused));
}
