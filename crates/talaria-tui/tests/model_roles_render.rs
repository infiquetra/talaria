//! Drawn model roles page.

#![allow(clippy::too_many_lines)]

use hermes_contract::schemas::ModelOptionsResult;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use serde_json::json;
use talaria_core::model_roles::{
    PickerStep, RolesView, RouteCatalog, RoutePickerView, RouteRow, RowView, Section, TabView,
    filter_routes,
};
use talaria_tui::history::sanitize;
use talaria_tui::model_roles::{draw_model_roles, draw_route_picker};

fn row(text: &str, selected: bool) -> RowView {
    RowView {
        text: text.to_owned(),
        selected,
        dimmed: false,
        marker: None,
    }
}

fn tabs(current: Section) -> Vec<TabView> {
    [
        (Section::Main, "1 Main"),
        (Section::Fallbacks, "2 Fallbacks"),
        (Section::Helpers, "3 Helpers"),
        (Section::Subagents, "4 Subagents"),
        (Section::Moa, "5 Mixture of Agents"),
    ]
    .into_iter()
    .map(|(section, label)| TabView {
        label: label.to_owned(),
        current: section == current,
        stale: false,
    })
    .collect()
}

fn page(current: Section, rows: Vec<&str>, notice: Vec<&str>, footer: Vec<&str>) -> RolesView {
    RolesView {
        title: "Model roles - profile fixture".to_owned(),
        tabs: tabs(current),
        rows: rows
            .into_iter()
            .enumerate()
            .map(|(index, text)| row(text, index == 0))
            .collect(),
        detail: None,
        focus_detail: false,
        notice: notice.into_iter().map(str::to_owned).collect(),
        footer: footer.into_iter().map(str::to_owned).collect(),
        modal: None,
        picker: None,
        raw_keys: false,
    }
}

fn lines_of(buf: &Buffer, width: u16, height: u16) -> Vec<String> {
    (0..height)
        .map(|y| {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buf[(x, y)].symbol());
            }
            line.trim_end().to_owned()
        })
        .collect()
}

fn draw(view: &RolesView, cols: u16, rows: u16) -> Vec<String> {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    draw_model_roles(view, area, &mut buf);
    let lines = lines_of(&buf, cols, rows);
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(lines.join("\n").as_bytes());
    let _ = parser.screen().contents();
    lines
}

fn screen(view: &RolesView, cols: u16, rows: u16) -> String {
    draw(view, cols, rows).join("\n")
}

#[test]
fn sections_snapshot() {
    let main = page(
        Section::Main,
        vec![
            "Model: minimax-m3.1-flash (via ollama-cloud)",
            "Context window: 128,000 tokens (auto-detected)",
            "Can use: tools, images, reasoning",
            "Max output: 8,192 tokens",
            "Family: minimax",
            "Default effort: high",
            "Applies to new sessions. Running sessions keep their model; change a session with /model.",
        ],
        vec!["No main model set. Choose one with Enter."],
        vec!["1 Main  2 Fallbacks  3 Helpers  4 Subagents  5 Mixture of Agents"],
    );
    for (cols, rows) in [(100, 30), (160, 48)] {
        let text = screen(&main, cols, rows);
        assert!(text.contains("Model roles - profile fixture"));
        assert!(text.contains("1 Main"));
        assert!(text.contains("5 Mixture of Agents"));
        assert!(text.contains("Model: minimax-m3.1-flash (via ollama-cloud)"));
        assert!(text.contains("Context window: 128,000 tokens (auto-detected)"));
        assert!(text.contains("Can use: tools, images, reasoning"));
        assert!(text.contains("Applies to new sessions."));
    }
    let fallbacks = page(
        Section::Fallbacks,
        vec![
            "1. minimax-m3.1-flash (via ollama-cloud)",
            "No fallback models. When the main model fails, the turn fails.",
        ],
        Vec::new(),
        Vec::new(),
    );
    assert!(
        screen(&fallbacks, 100, 30)
            .contains("No fallback models. When the main model fails, the turn fails.")
    );
    let helpers = page(
        Section::Helpers,
        vec![
            "Title gen - Session titles",
            "follows main model",
            "effort: inherited",
        ],
        Vec::new(),
        Vec::new(),
    );
    assert!(screen(&helpers, 160, 48).contains("Title gen - Session titles"));
    assert!(screen(&helpers, 160, 48).contains("follows main model"));
    let subagents = page(
        Section::Subagents,
        vec![
            "Route: follows main model",
            "Effort: inherited",
            "Fallback: inherits the main chain",
            "Other subagent limits are in Settings.",
        ],
        Vec::new(),
        Vec::new(),
    );
    let text = screen(&subagents, 100, 30);
    assert!(text.contains("Route: follows main model"));
    assert!(text.contains("Other subagent limits are in Settings."));
    let mut moa = page(
        Section::Moa,
        vec!["* alpha", "beta (off)"],
        vec![
            "No presets of your own. Hermes shows its example preset below; saving writes it into this profile, and enabled presets then appear as models under \"Mixture of Agents\".",
        ],
        Vec::new(),
    );
    moa.detail = Some(vec![
        row("Enabled: yes", false),
        row("Aggregator: minimax-m3.1-flash (via ollama-cloud)", false),
        row("Advisors run: once per user turn", false),
        row("If an advisor fails: say so (loud)", false),
        row("Active preset: none", false),
        row("Privacy filter: off", false),
    ]);
    let text = screen(&moa, 160, 48);
    assert!(text.contains("No presets of your own."));
    assert!(text.contains("Advisors run: once per user turn"));
    assert!(text.contains("Privacy filter: off"));
}

#[test]
fn loading_and_error_states() {
    let loading = page(Section::Main, vec!["Loading"], Vec::new(), Vec::new());
    let text = screen(&loading, 100, 30);
    assert!(
        text.lines().position(|line| line.contains("Loading"))
            < text
                .lines()
                .position(|line| line.contains("Usage") || line.contains("Enter"))
    );
    let mut failed = page(
        Section::Moa,
        vec!["This Hermes has no mixture of agents."],
        Vec::new(),
        Vec::new(),
    );
    failed.tabs[4].stale = true;
    failed.tabs[4].label = "5 Mixture of Agents (stale)".to_owned();
    let text = screen(&failed, 100, 30);
    assert!(text.contains("This Hermes has no"));
    assert!(text.contains("(stale)"));
}

#[test]
fn saved_default_line_and_footer() {
    let view = page(
        Section::Main,
        vec!["Model: minimax-m3.1-flash (via ollama-cloud)"],
        vec![
            "Talaria starts new sessions with ollama-cloud/minimax-m3.1-flash (your Talaria default), which overrides this.",
        ],
        vec![
            "Applies to new sessions.",
            "Sessions already running may keep their current setting.",
        ],
    );
    let text = screen(&view, 100, 30);
    assert!(text.contains("Talaria starts new sessions with ollama-cloud/minimax-m3.1-flash (your Talaria default), which overrides this."));
    assert!(text.contains("Sessions already running may keep their current setting."));
    assert!(text.contains("Applies to new sessions."));
}

#[test]
fn route_picker_snapshot() {
    let result: ModelOptionsResult = serde_json::from_value(json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [
            {"slug": "ollama-cloud", "name": "Ollama Cloud", "models": ["minimax-m3.1-flash", "missing-from-row"], "authenticated": true},
            {"slug": "openrouter", "name": "OpenRouter", "models": ["alpha"], "authenticated": true},
            {"slug": "skeleton", "name": "Skeleton", "models": [], "authenticated": false},
            {"slug": "my-lab", "name": "My Lab", "models": ["local"], "is_user_defined": true, "api_url": "https://endpoint.invalid"},
            {"slug": "moa", "name": "Mixture of Agents", "models": ["preset"], "source": "virtual"}
        ]
    }))
    .expect("catalogue");
    let catalog = RouteCatalog::from_result(&result);
    assert!(
        catalog
            .providers
            .iter()
            .all(|provider| provider.slug != "moa")
    );
    let rows = filter_routes(&catalog, &PickerStep::Providers, "sk");
    assert!(rows.iter().any(
        |row| matches!(row, RouteRow::Provider { slug, choosable: false, .. } if slug == "skeleton")
    ));
    assert!(rows.iter().all(|row| match row {
        RouteRow::Provider { slug, .. } => slug != "moa",
        RouteRow::Model { .. } => true,
    }));
    let view = RoutePickerView {
        title: "Choose a provider".to_owned(),
        filter: "sk".to_owned(),
        rows: vec![
            RowView {
                text: "Skeleton  not set up: add it under Providers".to_owned(),
                selected: true,
                dimmed: true,
                marker: None,
            },
            RowView {
                text: "stored-model (not in the model list)".to_owned(),
                selected: false,
                dimmed: false,
                marker: Some('!'),
            },
        ],
        status: Some("loading models...".to_owned()),
    };
    let area = Rect::new(0, 0, 100, 30);
    let mut buf = Buffer::empty(area);
    draw_route_picker(&view, area, &mut buf);
    let text = lines_of(&buf, 100, 30).join("\n");
    assert!(text.contains("not set up: add it under Providers"));
    assert!(text.contains("(not in the model list)"));
    assert!(text.contains("loading models..."));
    assert!(!text.contains("moa"));
    let dimmed = (0..100).any(|x| {
        buf[(x, 1)].style().add_modifier.contains(Modifier::DIM)
            || buf[(x, 2)].style().add_modifier.contains(Modifier::DIM)
    });
    assert!(dimmed, "a provider that is not set up is dimmed");
    let _ = sanitize("kept");
}

#[test]
fn untrusted_text_sanitised() {
    let hostile = format!("model \u{202E}evil\u{1b}name");
    let view = page(
        Section::Main,
        vec![&format!("Model: {hostile} (via provider)")],
        Vec::new(),
        Vec::new(),
    );
    let text = screen(&view, 100, 30);
    assert!(!text.contains('\u{202E}'));
    assert!(!text.contains('\u{1b}'));
    assert!(text.contains('\u{241b}'));
    assert!(text.contains("\u{2039}U+202E\u{203a}"));
    let mut parser = vt100::Parser::new(30, 100, 0);
    parser.process(text.as_bytes());
    let again = parser.screen().contents();
    assert!(again.contains('\u{241b}') || again.contains("U+202E"));
}

#[test]
fn narrow_80x24() {
    let labels = [
        "Vision - Image analysis",
        "Compression - Context compaction",
        "Skills hub - Skill search",
        "Approval - Smart auto-approve",
        "MCP - MCP tool routing",
        "Title gen - Session titles",
        "Review - /review reviewer subagent",
        "Voice chat - Spoken voice-mode replies",
        "Triage specifier - Kanban spec fleshing",
        "Kanban decomposer - Task decomposition",
        "Profile describer - Auto profile descriptions",
        "Curator - Skill-usage review",
        "Plugin task 1 - Plugin hint 1",
        "Plugin task 2 - Plugin hint 2",
    ];
    let mut rows: Vec<RowView> = labels
        .iter()
        .enumerate()
        .map(|(index, text)| row(text, index + 1 == labels.len()))
        .collect();
    rows.insert(0, row("↑ 2 more", false));
    let mut view = page(
        Section::Helpers,
        Vec::new(),
        Vec::new(),
        vec![
            "Enter open  e effort  i inherit  Ctrl+S review  u discard  Ctrl+R reload  Esc close  ? keys",
            "Sessions already running may keep their current setting.",
        ],
    );
    view.rows = rows;
    let lines = draw(&view, 80, 24);
    let text = lines.join("\n");
    assert!(text.contains("Plugin task 2 - Plugin hint 2"));
    assert!(text.contains("↑ 2 more"));
    assert!(
        lines.iter().any(|line| line.contains("Enter open"))
            && lines
                .iter()
                .any(|line| line.contains("Sessions already running"))
    );
    assert!(
        lines
            .iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 80)
    );
    let mut detail = Vec::new();
    for index in 1..=6 {
        detail.push(row(
            &format!("reference {index}: model (via provider)"),
            index == 6,
        ));
    }
    detail.insert(0, row("↑ 1 more", false));
    view.detail = Some(detail);
    view.focus_detail = true;
    let text = screen(&view, 80, 24);
    assert!(text.contains("reference 6: model (via provider)"));
}
