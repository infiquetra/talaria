//! Compatibility recordings made from the integrated, unmodified renderers.
use crate::view_colours_support as support;

use ratatui::style::{Color, Modifier, Style};

#[test]
fn every_default_view_matches_its_recorded_cells() {
    support::warm_syntax();
    let resolved = support::resolve(None);
    let fixtures = support::fixtures(&resolved);
    let expected = [
        "automation",
        "automation-warning",
        "background",
        "background-processes",
        "capabilities",
        "checklist",
        "composer",
        "connectors",
        "context",
        "cron-detail",
        "cron-form",
        "cron-history",
        "cron-list",
        "diff",
        "fleet-peek",
        "history-search",
        "launch",
        "markdown",
        "mcp",
        "mcp-card",
        "menu",
        "menu-dropdown",
        "model-roles",
        "model-roles-modal",
        "pager",
        "palette",
        "picker",
        "picker-details",
        "profiles",
        "profiles-confirm",
        "profiles-detail",
        "providers",
        "providers-card",
        "review",
        "review-labels",
        "rewind",
        "rewind-actions",
        "rewind-actions-off",
        "rewind-off",
        "side",
        "status",
        "subagent-dock",
        "syntax",
        "tool",
        "work-panel",
        "workflow",
        "workflow-picker",
        "workflow-selected",
        "workspace",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        fixtures
            .iter()
            .map(|fixture| fixture.name)
            .collect::<std::collections::BTreeSet<_>>(),
        expected,
        "the migration fixture inventory is incomplete"
    );
    let mut names = std::collections::BTreeSet::new();
    for fixture in fixtures {
        assert!(
            names.insert(fixture.name),
            "duplicate fixture {}",
            fixture.name
        );
        assert!(
            fixture
                .buffer
                .content
                .iter()
                .any(|cell| !cell.symbol().trim().is_empty()),
            "{} is blank",
            fixture.name
        );
        support::assert_recording(fixture.name, &fixture.buffer);
    }
    let recorded: std::collections::BTreeSet<_> = std::fs::read_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/view_colours"),
    )
    .unwrap()
    .map(|entry| {
        entry
            .unwrap()
            .path()
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    })
    .collect();
    assert_eq!(
        recorded,
        names.into_iter().map(str::to_owned).collect(),
        "recording inventory differs from real draw fixtures"
    );
}

#[test]
fn rewind_palette_follows_the_legacy_no_color_rule() {
    let resolved = support::resolve(None);
    for no_color in [None, Some(""), Some("1")] {
        // The CLI's compatibility rule is presence, including an empty value.
        let palette = resolved.rewind_theme(no_color.is_none());
        assert_eq!(
            palette.selected,
            Style::default().add_modifier(Modifier::REVERSED)
        );
        assert_eq!(palette.dim, Style::default().add_modifier(Modifier::DIM));
        if no_color.is_none() {
            assert_eq!(palette.warning, Style::default().fg(Color::Yellow));
            assert_eq!(palette.accent, Style::default().fg(Color::Cyan));
        } else {
            assert_eq!(
                palette.warning,
                Style::default().add_modifier(Modifier::BOLD)
            );
            assert_eq!(palette.accent, Style::default());
        }
    }
}
