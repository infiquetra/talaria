use talaria_core::capabilities::*;

#[test]
fn texts_are_exact() {
    let args = TextArgs {
        profile: "fixture".into(),
        name: "guide".into(),
        tab: CapTab::Skills,
        enabled: true,
        label: "Web".into(),
        key: "install-key".into(),
        message: "denied".into(),
        source: "official".into(),
        trust: "trusted".into(),
        verdict: "clean".into(),
        reason: "policy says no".into(),
        counts: [1, 2, 3, 4],
        number: 2,
        query: "zzz".into(),
        sources: vec!["github".into(), "gitlab".into()],
        repository: "https://example.org/plugin".into(),
        sha: Some("abcdef123456".into()),
        ..TextArgs::default()
    };
    for (kind, expected) in [
        (CapText::Loading, "Loading skills..."),
        (
            CapText::EmptySkills,
            "No skills are installed in profile fixture. Press Tab to open the skill hub.",
        ),
        (
            CapText::EmptyToolsets,
            "Hermes reported no configurable toolsets for profile fixture.",
        ),
        (
            CapText::EmptyPlugins,
            "You have not installed any plugins in profile fixture. Press a to show the 2 that ship with Hermes, or i to install one.",
        ),
        (
            CapText::EmptyHub,
            "The skill hub returned nothing. Check the network, then press r.",
        ),
        (CapText::NoSearchResults, "No hub skills match \"zzz\"."),
        (
            CapText::SlowSources,
            "Some hub sources did not answer in time: github, gitlab.",
        ),
        (
            CapText::SkillChanged,
            "Turned on skill guide in profile fixture. New sessions use this.",
        ),
        (
            CapText::ToolsetChanged,
            "Turned on toolset Web in profile fixture. Sessions started from now on use this; open sessions keep their toolsets.",
        ),
        (
            CapText::PostSetup,
            "Hermes is installing what Web needs (install-key) in the background.",
        ),
        (CapText::PluginChanged, "Turned on plugin guide."),
        (
            CapText::Mismatch,
            "Hermes did not keep the change to guide: it now reads on. Press r to refresh.",
        ),
        (CapText::Busy, "Wait for the current change to finish."),
        (CapText::FilterPrompt, "Filter: "),
        (CapText::SearchPrompt, "Search: "),
        (CapText::InstallPrompt, "Install: "),
        (
            CapText::LostAnswer,
            "Talaria lost the answer to this change. Refreshing to see what Hermes did.",
        ),
        (
            CapText::ConfirmSkill,
            "Install guide into profile fixture? Source: official (trusted). Scan: clean; 1 critical, 2 high, 3 medium, 4 low. Press y to install, any other key to cancel.",
        ),
        (
            CapText::DeniedSkill,
            "Hermes will not install guide from Talaria: policy says no",
        ),
        (
            CapText::InstallingSkill,
            "Installing guide... Hermes fetches and scans it; this can take a minute.",
        ),
        (
            CapText::InstalledSkill,
            "Installed guide into profile fixture.",
        ),
        (
            CapText::InstallFailed,
            "Hermes did not install guide: denied",
        ),
        (
            CapText::Unsupported,
            "This gateway does not answer install-key on Talaria's connection.",
        ),
        (CapText::Refused, "Hermes refused this request: denied"),
        (
            CapText::HubUnavailable,
            "The skill hub did not answer. Press r to try again.",
        ),
        (
            CapText::ConfirmCatalog,
            "Install plugin guide from the Hermes catalog (https://example.org/plugin, commit abcdef1) into profile fixture? Press y to install, any other key to cancel.",
        ),
        (
            CapText::ConfirmRepository,
            "Install the plugin at https://example.org/plugin into profile fixture? It is not from the Hermes catalog, so nobody has reviewed it. Press y to install, any other key to cancel.",
        ),
        (
            CapText::InvalidInput,
            "Type a catalog name (lowercase letters, digits, _ or -) or an https:// repository address.",
        ),
        (
            CapText::UnknownPlugin,
            "The Hermes plugin catalog has no plugin named guide.",
        ),
        (
            CapText::CatalogUnavailable,
            "Hermes could not read its plugin catalog. Try again later, or install from an https:// repository address.",
        ),
        (CapText::InstalledPlugin, "Installed plugin guide."),
        (
            CapText::ConfirmRemove,
            "Remove plugin guide from profile fixture? Press y to remove, any other key to cancel.",
        ),
        (CapText::RemovedPlugin, "Removed plugin guide."),
        (
            CapText::BundledRemove,
            "guide ships with Hermes and cannot be removed; turn it off instead.",
        ),
        (
            CapText::ReadOnly,
            "Talaria shows guide read-only: the gateway listed it without a key or with a status Talaria does not know.",
        ),
        (
            CapText::ProfileUnavailable,
            "Profile fixture is not served by the gateway, so its capabilities cannot be read or changed.",
        ),
        (CapText::Truncated, "(showing the first 256 KiB)"),
        (
            CapText::CatalogListUnavailable,
            "The plugin catalog list is not available from this gateway; type a catalog name instead.",
        ),
        (
            CapText::Unauthorized,
            "Hermes refused Talaria's sign-in for this request. Press r to retry.",
        ),
        (
            CapText::Unreachable,
            "Talaria could not reach the gateway. Press r to retry.",
        ),
        (
            CapText::MissingInstall,
            "Hermes reported guide installed, but profile fixture does not list it. Press r to refresh.",
        ),
        (
            CapText::ReadTimeout,
            "Hermes did not answer in time. Press r to try again.",
        ),
        (
            CapText::ReadBackFailed,
            "Hermes accepted the change to guide, but Talaria could not read it back. Press r to refresh.",
        ),
        (
            CapText::MissingContent,
            "Hermes has no skill guide with a SKILL.md in profile fixture. Press r to refresh.",
        ),
        (
            CapText::MissingHubItem,
            "The skill hub has no skill install-key.",
        ),
        (
            CapText::PackageRemove,
            "guide is a Python package; remove its package outside Talaria.",
        ),
        (CapText::UnchangedPlugin, "Plugin guide was already on."),
        (
            CapText::AmbiguousRemove,
            "More than one installed plugin is named guide, so Talaria cannot tell which one Hermes would remove.",
        ),
        (
            CapText::MissingChangedRow,
            "Hermes did not keep the change to guide: it is no longer installed. Press r to refresh.",
        ),
        (
            CapText::BadCommand,
            "Use /capabilities skills, hub, toolsets or plugins.",
        ),
        (CapText::TooSmall, "Capabilities needs at least 40 by 10."),
        (CapText::FindingsTail, "and 2 more"),
        (CapText::EmptyFilter, "Nothing matches \"zzz\"."),
        (CapText::EmptyLog, "(Hermes sent no install log)"),
    ] {
        assert_eq!(format_text(kind, &args), expected, "{kind:?}");
    }
    for (tab, expected) in [
        (CapTab::Hub, "Loading the skill hub..."),
        (CapTab::Toolsets, "Loading toolsets..."),
        (CapTab::Plugins, "Loading plugins..."),
    ] {
        assert_eq!(
            format_text(
                CapText::Loading,
                &TextArgs {
                    tab,
                    ..args.clone()
                }
            ),
            expected
        );
    }
    let off = TextArgs {
        enabled: false,
        ..args.clone()
    };
    assert_eq!(
        format_text(CapText::SkillChanged, &off),
        "Turned off skill guide in profile fixture. New sessions use this."
    );
    assert_eq!(
        format_text(CapText::ToolsetChanged, &off),
        "Turned off toolset Web in profile fixture. Sessions started from now on use this; open sessions keep their toolsets."
    );
    assert_eq!(
        format_text(CapText::PluginChanged, &off),
        "Turned off plugin guide."
    );
    assert_eq!(
        format_text(CapText::Mismatch, &off),
        "Hermes did not keep the change to guide: it now reads off. Press r to refresh."
    );
    assert_eq!(
        format_text(CapText::UnchangedPlugin, &off),
        "Plugin guide was already off."
    );
    assert_eq!(
        format_text(
            CapText::ConfirmCatalog,
            &TextArgs {
                sha: None,
                ..args.clone()
            }
        ),
        "Install plugin guide from the Hermes catalog (https://example.org/plugin, commit unknown) into profile fixture? Press y to install, any other key to cancel."
    );
    let activation = Activation {
        live_servers: vec!["fixture-mcp".into()],
        live_skills: vec!["guide".into()],
        deferred: vec![
            ("tools".into(), vec!["fixture_tool".into()]),
            ("other".into(), vec!["later".into()]),
        ],
    };
    let extra = "Live now: fixture-mcp, guide.\nFrom the next session: fixture_tool, later.\nRestart the gateway for this to take effect.";
    let detailed = TextArgs {
        activation,
        restart_required: true,
        ..args.clone()
    };
    assert_eq!(
        format_text(CapText::PluginChanged, &detailed),
        format!("Turned on plugin guide.\n{extra}")
    );
    let install = TextArgs {
        enabled: false,
        warnings: vec!["first".into(), "second".into()],
        missing_env: vec!["A_TOKEN".into(), "B_TOKEN".into()],
        known_issues: vec!["one".into(), "two".into()],
        ..detailed
    };
    assert_eq!(
        format_text(CapText::InstalledPlugin, &install),
        format!(
            "Installed plugin guide.\nWarning: first\nWarning: second\nIt needs these settings before it works: A_TOKEN, B_TOKEN.\nKnown issue: one\nKnown issue: two\n{extra}\nIt is turned off; press Space to turn it on."
        )
    );
    assert_eq!(
        format_text(
            CapText::RemovedPlugin,
            &TextArgs {
                cleared_memory_provider: true,
                ..args.clone()
            }
        ),
        "Removed plugin guide.\nIt was the memory provider; Hermes now uses its built-in memory."
    );
    assert_eq!(
        format_text(
            CapText::InstalledSkill,
            &TextArgs {
                session_line: Some(Ok("41 skill(s) available".into())),
                ..args.clone()
            }
        ),
        "Installed guide into profile fixture.\nThis session: 41 skill(s) available."
    );
    assert_eq!(
        format_text(
            CapText::InstalledSkill,
            &TextArgs {
                session_line: Some(Err("denied".into())),
                ..args.clone()
            }
        ),
        "Installed guide into profile fixture.\nThis session could not reload its skills: denied."
    );
    let long = "界".repeat(400);
    let expected = format!("Hermes did not install guide: {}…", "界".repeat(299));
    assert_eq!(
        format_text(
            CapText::InstallFailed,
            &TextArgs {
                message: long,
                ..args
            }
        ),
        expected
    );
    assert!(expected.chars().count() > 300);
}
