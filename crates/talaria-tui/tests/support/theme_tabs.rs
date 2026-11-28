//! Additional populated tabs and detail states for palette review.
use crate::view_colours_support::Fixture;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};
use std::collections::BTreeMap;
use talaria_tui::theme::ThemeFileResolution;

fn draw(
    name: &'static str,
    family: &'static str,
    render: impl FnOnce(Rect, &mut Buffer),
) -> Fixture {
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    render(area, &mut buffer);
    assert!(
        buffer.content.iter().any(|cell| cell.symbol() != " "),
        "empty fixture {name}"
    );
    Fixture {
        name,
        buffer,
        owned_screen: true,
        family,
    }
}

pub fn fixtures(resolved: &ThemeFileResolution) -> Vec<Fixture> {
    let mut out = Vec::new();
    providers(&mut out, resolved);
    capabilities(&mut out, resolved);
    connectors(&mut out, resolved);
    mcp(&mut out, resolved);
    settings(&mut out, resolved);
    subagents(&mut out, resolved);
    pager(&mut out, resolved);
    composer_and_picker(&mut out, resolved);
    out
}

fn composer_and_picker(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::models::{BrowseCounts, BrowseView, ListScope, PickerRow, PickerView};
    let rows = [PickerRow {
        key: None,
        text: "No models match this filter".into(),
        badges: vec![],
        price: None,
        dim: true,
        header: false,
    }];
    let view = PickerView {
        title: "Switch model · this session",
        filter: "unknown",
        effort: "effort: medium",
        hints: "Ctrl+U clear · Esc close",
        refreshing: false,
        rows: &rows,
        selected: None,
        banner: None,
        browse: Some(BrowseView {
            scope: ListScope::All,
            counts: BrowseCounts {
                models: 0,
                providers: 1,
                starred: 0,
                configured: 1,
                setup: 0,
            },
            details: &[],
            details_open: false,
            details_scroll: 0,
            current: None,
        }),
    };
    out.push(draw("picker-no-results", "picker", |area, buf| {
        resolved.draw_picker_frame(&view, area, buf)
    }));
    use talaria_tui::live::LiveSection;
    let mut composer = talaria_tui::composer::Composer::new(talaria_tui::keys::KeyMap::defaults());
    composer.load("Review @src".into());
    let section = composer
        .section(80, 10)
        .in_conversation("Fixture conversation", true);
    let mention = talaria_core::attachments::MentionView {
        rows: vec![
            ("src/main.rs".into(), "file".into()),
            ("src/lib.rs".into(), "file".into()),
        ],
        selected: Some(0),
        status: None,
        footer: "tab complete · enter insert · esc close",
    };
    let mention = talaria_tui::mention::MentionSection::new(&mention)
        .with_theme(resolved.theme.views.clone());
    out.push(draw("composer-completion", "composer", |area, buf| {
        let mh = mention.height(area.width);
        mention.render(Rect::new(0, 0, area.width, mh), buf);
        section.render_with_theme(
            Rect::new(0, mh, area.width, section.height(area.width)),
            buf,
            &resolved.theme.views,
        );
    }));
}

fn providers(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::providers::{
        ProvidersTab, ProvidersView,
        browse::{BrowseCounts, BrowseKey, BrowseRow, BrowseView, ProviderStatus, SetupMethod},
    };
    for (name, tab) in [
        ("providers-accounts", ProvidersTab::Accounts),
        ("providers-keys", ProvidersTab::Keys),
        ("providers-endpoints", ProvidersTab::Endpoints),
    ] {
        let view = ProvidersView {
            title: "Providers · profile fixture".into(),
            tab,
            rows: vec![
                "Example provider · key saved".into(),
                "Example endpoint · ready".into(),
            ],
            selected: Some(0),
            filter: String::new(),
            card: None,
            notice: None,
            banner: None,
            secret_focus: false,
            footer: "Enter details · Esc close".into(),
            browse: Some(BrowseView {
                rows: vec![
                    BrowseRow {
                        key: Some(BrowseKey::Provider("example".into())),
                        label: "Example provider".into(),
                        status: Some(ProviderStatus::KeySaved),
                        methods: vec![SetupMethod::Key],
                        details: vec!["Key saved; reachability unknown".into()],
                    },
                    BrowseRow {
                        key: Some(BrowseKey::Endpoint("local".into())),
                        label: "Local endpoint".into(),
                        status: Some(ProviderStatus::ListedReady),
                        methods: vec![SetupMethod::Endpoint],
                        details: vec!["Custom endpoint".into()],
                    },
                ],
                selected: Some(0),
                counts: BrowseCounts {
                    providers: 1,
                    complete: true,
                },
                ..Default::default()
            }),
        };
        out.push(draw(name, "providers", |area, buf| {
            resolved.draw_providers(&view, 0, area, buf)
        }));
    }
}

fn capabilities(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::capabilities::{CapListRow, CapTab, CapView};
    for (name, tab, row, detail) in [
        (
            "capabilities-hub",
            CapTab::Hub,
            "review-helper",
            "Available from the skill hub",
        ),
        (
            "capabilities-toolsets",
            CapTab::Toolsets,
            "terminal",
            "Run local commands",
        ),
        (
            "capabilities-plugins",
            CapTab::Plugins,
            "example-plugin",
            "Installed plugin",
        ),
    ] {
        let view = CapView {
            profile: "fixture".into(),
            title: "Capabilities · profile fixture".into(),
            tab,
            tabs: vec![
                "Skills 2 installed".into(),
                "Skill hub".into(),
                "Toolsets 1/2 on".into(),
                "Plugins 1 installed".into(),
            ],
            rows: vec![CapListRow {
                name: row.into(),
                label: "available".into(),
                source: "local".into(),
                description: detail.into(),
                marker: "[x]".into(),
                usage: 3,
                ..Default::default()
            }],
            selected: 0,
            detail: vec![row.into(), detail.into(), "Enter opens details".into()],
            hints: "Enter details · / filter · Tab next · Esc close".into(),
            open: true,
            ..Default::default()
        };
        out.push(draw(name, "capabilities", |area, buf| {
            resolved.draw_capabilities(&view, area, buf)
        }));
    }
}

fn connectors(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::connectors::{ConnectorsTab, ConnectorsView, Line, Row, Screen, Tone};
    let row = || Row {
        label: "Example account".into(),
        state: "connected".into(),
        marker: None,
    };
    let line = || Line {
        text: "Connected · tools available".into(),
        tone: Tone::Normal,
    };
    for (name, tab, screen) in [
        (
            "connectors-messaging",
            ConnectorsTab::Messaging,
            Screen::PlatformList {
                rows: vec![row()],
                selected: 0,
                restart_line: true,
                other_pairing: vec![],
            },
        ),
        (
            "connectors-detail",
            ConnectorsTab::Connectors,
            Screen::ConnectorCard {
                title: "Example connector".into(),
                lines: vec![line()],
                accounts: vec![row()],
                retired: 0,
                selected: 0,
            },
        ),
        (
            "connectors-edit",
            ConnectorsTab::Connectors,
            Screen::Prompt {
                title: "Edit connector".into(),
                text: "Example account".into(),
                help: Some("Enter a name".into()),
                problem: None,
            },
        ),
    ] {
        let view = ConnectorsView {
            title: "Connectors and messaging · profile fixture".into(),
            tab,
            screen,
            notice: None,
            footer: vec![("esc".into(), "back".into())],
            review_scroll: 0,
        };
        out.push(draw(name, "connectors", |area, buf| {
            resolved.draw_connectors(&view, &BTreeMap::new(), area, buf)
        }));
    }
}

fn mcp(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::mcp::{CardView, McpTab, McpView, RowView};
    let mut view = McpView {
        title: "MCP servers · profile fixture".into(),
        tab: McpTab::Catalogue,
        banner: None,
        filter: String::new(),
        placeholder: None,
        rows: vec![RowView {
            name: "Example server".into(),
            kind: "remote".into(),
            label: "available · 3 tools".into(),
            dim: false,
        }],
        selected: Some(0),
        status_line: None,
        card: None,
        pending_line: None,
        notice: None,
        footer: "enter details · esc close".into(),
        secret_focus: None,
    };
    out.push(draw("mcp-catalogue", "mcp", |area, buf| {
        resolved.draw_mcp(&view, &|_| 0, area, buf)
    }));
    view.card = Some(CardView {
        title: "Example server".into(),
        lines: vec![
            "Remote server · 3 tools".into(),
            "Search the documentation".into(),
        ],
        fields: vec![],
        footer: "enter install · esc back".into(),
    });
    out.push(draw("mcp-catalogue-detail", "mcp", |area, buf| {
        resolved.draw_mcp(&view, &|_| 0, area, buf)
    }));
}

fn settings(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::settings::{FieldHelp, Screen, SettingsRow, SettingsView};
    let base = SettingsView {
        screen: Screen::Browsing,
        profile: "fixture".into(),
        categories: vec!["display".into(), "general".into()],
        category: "display".into(),
        rows: vec![SettingsRow {
            key: "display.compact".into(),
            value: "true".into(),
            origin: "set".into(),
            marker: "*".into(),
            effect_label: "applies now".into(),
        }],
        detail: "Compact display".into(),
        field_help: Some(FieldHelp {
            description: "Control the display density".into(),
            default_value: "false".into(),
            note: "Applies immediately".into(),
        }),
        footer: "Enter edit · / filter · ? help · Esc close".into(),
        ..Default::default()
    };
    let mut help = base.clone();
    help.screen = Screen::Help;
    help.help = vec![
        ("Enter".into(), "Edit the selected setting".into()),
        ("/".into(), "Filter settings".into()),
    ];
    let mut filter = base.clone();
    filter.filter = "display".into();
    filter.filter_cursor = Some(7);
    let mut discard = base;
    discard.screen = Screen::ConfirmDiscard;
    discard.footer = "Discard 1 unapplied change(s)? y discard · n keep editing".into();
    for (name, view) in [
        ("settings-help", help),
        ("settings-filter", filter),
        ("settings-discard", discard),
    ] {
        out.push(draw(name, "settings", |area, buf| {
            talaria_tui::settings::draw_settings_with_theme(&view, area, buf, &resolved.theme.views)
        }));
    }
}

fn subagents(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use hermes_contract::events::EventFrame;
    use serde_json::json;
    use talaria_core::{
        directory::rows::RowKey,
        subagents::{Clock, Spawning, SubagentEvent, SubagentTree},
    };
    use talaria_tui::subagents::{AgentsPanel, PanelHeader};
    let mut tree = SubagentTree::new();
    for (id, goal) in [
        ("child-1", "Inspect the changes"),
        ("child-2", "Review the tests"),
    ] {
        let frame = EventFrame::from_params(&json!({"type":"subagent.start", "session_id":"fixture", "seq":1, "replay_epoch":"fixture", "payload":{"subagent_id":id,"goal":goal,"task_index":0,"task_count":2,"model":"example-model"}})).expect("subagent event");
        tree.apply(
            &SubagentEvent::from_notification(&frame.notification).expect("start event"),
            Clock {
                mono_ms: 1000,
                wall_s: 1.0,
            },
        );
    }
    let mut panel = AgentsPanel::new(RowKey {
        profile: "fixture".into(),
        session_id: "fixture".into(),
    });
    panel.set_style(resolved.subagent_style());
    let header = PanelHeader {
        title: "Review the changes".into(),
        spawning: Spawning::Unknown,
        key_label: None,
    };
    out.push(draw("subagents-explorer", "subagents", |area, buf| {
        Paragraph::new(panel.render(
            &tree,
            &header,
            None,
            area.width,
            area.height,
            Clock {
                mono_ms: 13_000,
                wall_s: 13.0,
            },
        ))
        .render(area, buf)
    }));
}

fn pager(out: &mut Vec<Fixture>, resolved: &ThemeFileResolution) {
    use talaria_core::session::{Cell, CellId, UserCell};
    use talaria_tui::pager::{
        ConnectionBanner, Pager, PagerAction, PagerContext, SearchIndex, SearchInput,
    };
    let cells = vec![
        Cell::User(UserCell {
            id: CellId::new(1),
            text: "Find the needle in this conversation".into(),
            expanded_paste: None,
        }),
        Cell::User(UserCell {
            id: CellId::new(2),
            text: "The needle appears again".into(),
            expanded_paste: None,
        }),
    ];
    let context = PagerContext {
        cells: &cells,
        source_epoch: 1,
        title: "Fixture conversation",
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    };
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&context, 80, 24, &mut index);
    pager.handle_action(PagerAction::StartSearch, &context, &mut index);
    pager.handle_search_input(SearchInput::Paste("needle".into()), &context, &mut index);
    pager.handle_search_input(SearchInput::Enter, &context, &mut index);
    out.push(draw("pager-search-hit", "pager", |area, buf| {
        resolved.draw_pager(
            &mut pager,
            area,
            buf,
            &context,
            &talaria_tui::history::PlainCellLines,
        )
    }));
}
