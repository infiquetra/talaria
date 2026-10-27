mod support;

use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::mcp::{CardView, FieldView, McpTab, McpView, SecretSlot};
use talaria_tui::mcp::draw_mcp;

fn blank_view() -> McpView {
    McpView {
        title: "MCP servers · profile fixture".into(),
        tab: McpTab::Servers,
        banner: None,
        filter: String::new(),
        placeholder: None,
        rows: vec![],
        selected: None,
        status_line: None,
        card: None,
        pending_line: None,
        notice: None,
        footer: "enter details · esc close".into(),
        secret_focus: None,
    }
}

fn screen(view: &McpView, width: u16, height: u16) -> Vec<String> {
    use support::{ByteSink, Vt, probe};
    use talaria_tui::terminal::{InlineTerminal, RestoreLink};
    let sink = ByteSink::default();
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (width, height),
        2,
        probe(false, false, Some((0, height - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut vt = Vt::new(width, height);
    vt.absorb(&sink);
    terminal.open_overlay().unwrap();
    vt.absorb(&sink);
    terminal
        .draw_overlay(|area, buf| draw_mcp(view, &|_| 0, area, buf))
        .unwrap();
    vt.absorb(&sink);
    vt.rows()
}

#[test]
fn tabs_snapshots() {
    for (width, height) in [(120, 40), (80, 24)] {
        for placeholder in [
            "Loading…",
            "No MCP servers in profile fixture yet. ctrl+n adds one; tab opens Hermes's catalogue.",
            "Could not load servers.",
        ] {
            let mut view = blank_view();
            view.placeholder = Some(placeholder.into());
            let rows = screen(&view, width, height);
            assert!(rows[0].starts_with("MCP servers · profile fixture"));
            assert!(rows[0].contains("[Servers]"));
            assert!(
                rows[3].contains(placeholder)
                    || (width == 80
                        && rows[3].starts_with(&placeholder[..placeholder.len().min(20)]))
            );
            view.tab = McpTab::Catalogue;
            view.placeholder = Some("Loading…".into());
            assert!(screen(&view, width, height)[0].contains("[Catalogue]"));
        }
    }
}

#[test]
fn cards_snapshots() {
    for (title, footer) in [
        ("example", "c copy command · esc back"),
        (
            "Add an MCP server",
            "tab next field · ←→ choose · ctrl+s save · esc discard",
        ),
        ("Install example?", "enter install · esc cancel"),
        ("Remove example?", "y remove · n cancel"),
        ("Apply changes?", "y apply · n cancel"),
        ("API key for example", "enter save · esc cancel"),
        (
            "Sign in to example",
            "o open link · c copy link · p paste the browser's address · esc cancel",
        ),
        ("Paste the browser's address", "enter send · esc back"),
    ] {
        let mut view = blank_view();
        view.card = Some(CardView {
            title: title.into(),
            lines: vec!["Example text".into()],
            fields: vec![],
            footer: footer.into(),
        });
        let rows = screen(&view, 50, 24).join("\n");
        assert!(rows.contains(title));
        assert!(rows.contains("Example text"));
        assert!(rows.contains(footer.split_whitespace().next().unwrap()));
    }
}

#[test]
fn layout_matches_specification() {
    let mut view = blank_view();
    view.rows = vec![talaria_core::mcp::RowView {
        name: "deepwiki".into(),
        kind: "remote".into(),
        label: "connected · 3 tools · test ok, 3 tools".into(),
        dim: false,
    }];
    view.selected = Some(0);
    view.pending_line = Some(
        "Saved for new sessions; this session still uses the old servers · ctrl+y apply".into(),
    );
    view.notice = Some("deepwiki works: 3 tools, 0 prompts, 0 resources.".into());
    let rows = screen(&view, 120, 40);
    let title = "MCP servers · profile fixture";
    let tabs = "[Servers]  Catalogue";
    assert_eq!(
        rows[0],
        format!(
            "{title}{}{tabs}",
            " ".repeat(120 - title.chars().count() - tabs.chars().count())
        )
    );
    assert_eq!(rows[1], "─".repeat(120));
    assert_eq!(rows[2], "type to filter");
    assert!(rows[3].starts_with("› deepwiki"));
    assert!(rows[3].contains("remote"));
    assert_eq!(rows[37], view.pending_line.unwrap());
    assert_eq!(rows[38], view.notice.unwrap());
    assert_eq!(rows[39], view.footer);
    let mut catalogue = blank_view();
    catalogue.tab = McpTab::Catalogue;
    catalogue.filter = "deep".into();
    catalogue.status_line = Some("66 servers in Hermes's catalogue".into());
    catalogue.footer = "enter install · tab servers · ctrl+r reload · esc close".into();
    let rows = screen(&catalogue, 120, 40);
    let tabs = "Servers  [Catalogue]";
    assert_eq!(
        rows[0],
        format!(
            "{title}{}{tabs}",
            " ".repeat(120 - title.chars().count() - tabs.chars().count())
        )
    );
    assert_eq!(rows[1], "─".repeat(120));
    assert_eq!(rows[2], "filter: deep");
    assert_eq!(rows[36], "66 servers in Hermes's catalogue");
    assert_eq!(rows[37], "");
    assert_eq!(rows[38], "");
    assert_eq!(rows[39], catalogue.footer);
}

#[test]
fn opens_without_waiting_and_idles() {
    use std::time::{Duration, Instant};
    use talaria_core::mcp::{McpInput, McpOpen, McpState, next_deadline, update, view};
    let now = Instant::now();
    let mut state = McpState::default();
    update(
        &mut state,
        McpInput::Open {
            open: McpOpen::Tab(McpTab::Servers),
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: false,
            foreground: None,
        },
        now,
    );
    let current = view(&state, now);
    assert!(
        current
            .text_lines()
            .iter()
            .any(|line| line.contains("Loading…"))
    );
    assert!(next_deadline(&state).is_none());
    let area = Rect::new(0, 0, 120, 40);
    let mut buffer = Buffer::empty(area);
    draw_mcp(&current, &|_| 0, area, &mut buffer);
    let mut elapsed = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        draw_mcp(&current, &|_| 0, area, &mut buffer);
        elapsed.push(started.elapsed());
    }
    elapsed.sort();
    assert!(elapsed[10] < Duration::from_millis(50));
}

#[test]
fn secret_fields_draw_bullets_only() {
    let view = McpView {
        title: "MCP servers · profile fixture".into(),
        tab: McpTab::Servers,
        banner: None,
        filter: String::new(),
        placeholder: None,
        rows: vec![],
        selected: None,
        status_line: None,
        card: Some(CardView {
            title: "API key".into(),
            lines: vec![],
            fields: vec![FieldView {
                label: "Key".into(),
                text: String::new(),
                secret: Some(SecretSlot::KeyValue),
                error: None,
                warning: None,
                focused: true,
            }],
            footer: String::new(),
        }),
        pending_line: None,
        notice: None,
        footer: String::new(),
        secret_focus: Some(SecretSlot::KeyValue),
    };
    let area = Rect::new(0, 0, 120, 40);
    let mut buffer = Buffer::empty(area);
    draw_mcp(&view, &|_| 45, area, &mut buffer);
    let rendered: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
    assert!(rendered.contains(&"•".repeat(40)));
    assert!(rendered.contains('…'));
    assert!(!rendered.contains(&"•".repeat(41)));
}

#[test]
fn keymap_defaults_for_mcp() {
    use std::collections::BTreeMap;
    use talaria_tui::keys::{Action, KeyMap};
    let (map, _) = KeyMap::from_entries(&BTreeMap::new());
    for (name, chord) in [
        ("mcp_next_tab", "Tab"),
        ("mcp_prev_tab", "Shift+Tab"),
        ("mcp_reload", "Ctrl+R"),
        ("mcp_add", "Ctrl+N"),
        ("mcp_test", "Ctrl+T"),
        ("mcp_toggle", "Ctrl+E"),
        ("mcp_set_key", "Ctrl+K"),
        ("mcp_sign_in", "Ctrl+O"),
        ("mcp_remove", "Ctrl+D"),
        ("mcp_apply", "Ctrl+Y"),
        ("mcp_save", "Ctrl+S"),
        ("mcp_open_link", "o"),
        ("mcp_copy", "c"),
        ("mcp_paste_redirect", "p"),
    ] {
        let action =
            Action::from_config_name(name).unwrap_or_else(|| panic!("missing action {name}"));
        assert!(
            map.chords(action)
                .iter()
                .any(|key| key.label().eq_ignore_ascii_case(chord)),
            "{name}"
        );
    }
}
