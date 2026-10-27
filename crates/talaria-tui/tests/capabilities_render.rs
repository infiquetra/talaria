mod support;

use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect};
use serde_json::json;
use support::{ByteSink, Vt, live_view_sections, probe};
use talaria_core::capabilities::{
    CapData, CapEffect, CapInput, CapListRow, CapTab, CapView, CapabilitiesState, KeyAction,
    Provenance, SkillRow, redact_paths,
};
use talaria_tui::capabilities::draw_capabilities;
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const SKILL_HINTS: &str = "Space on/off  Enter read  / filter  Tab next tab  r refresh  Esc close";

fn sample() -> CapView {
    let skill = |name: &str, description: &str, source: &str, enabled: bool, usage| CapListRow {
        name: name.into(),
        label: source.into(),
        source: source.into(),
        description: description.into(),
        marker: if enabled { "[x]" } else { "[ ]" }.into(),
        usage,
        ..Default::default()
    };
    let category = |name: &str| CapListRow {
        name: name.into(),
        is_header: true,
        ..Default::default()
    };
    CapView {
        profile: "fixture".into(),
        title: "Capabilities · profile fixture".into(),
        tabs: vec![
            "Skills 3/4 on".into(),
            "Skill hub".into(),
            "Toolsets".into(),
            "Plugins".into(),
        ],
        rows: vec![
            category("creative"),
            skill(
                "ascii-art",
                "Draw text banners in the terminal.",
                "bundled",
                true,
                3,
            ),
            skill(
                "audiocraft-audio-generation",
                "Generate music and sound effects from a text prompt.",
                "hub",
                false,
                0,
            ),
            category("software-development"),
            skill(
                "code-review",
                "Review a diff for correctness and style.",
                "bundled",
                true,
                41,
            ),
            category("(no category)"),
            skill(
                "fixture-skill-c",
                "A top-level skill with no category folder.",
                "local",
                true,
                0,
            ),
        ],
        selected: 1,
        detail: vec![
            "ascii-art · creative · bundled · used 3".into(),
            "Draw text banners in the terminal.".into(),
        ],
        hints: SKILL_HINTS.into(),
        open: true,
        ..Default::default()
    }
}

fn rig(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut vt = Vt::new(cols, rows);
    vt.absorb(&sink);
    (term, sink, vt)
}

fn screen(view: &CapView, cols: u16, rows: u16) -> Vec<String> {
    let (mut term, sink, mut vt) = rig(cols, rows);
    term.open_overlay().unwrap();
    term.draw_overlay(|area, buf| draw_capabilities(view, area, buf))
        .unwrap();
    vt.absorb(&sink);
    vt.rows()
}

#[test]
fn snapshots_at_four_sizes() {
    other_tabs_catalog_dialog_log_and_filter_have_fixed_rows();

    for (cols, rows, golden) in [
        (120, 40, include_str!("fixtures/capabilities/120x40.txt")),
        (99, 30, include_str!("fixtures/capabilities/99x30.txt")),
        (80, 24, include_str!("fixtures/capabilities/80x24.txt")),
        (39, 9, include_str!("fixtures/capabilities/39x9.txt")),
    ] {
        let mut expected = golden.lines().map(str::to_owned).collect::<Vec<_>>();
        assert!(
            expected.len() <= usize::from(rows),
            "golden height at {cols}x{rows}"
        );
        expected.resize(usize::from(rows), String::new());
        assert_eq!(
            expected.len(),
            usize::from(rows),
            "golden height at {cols}x{rows}"
        );
        assert_eq!(screen(&sample(), cols, rows), expected, "{cols}x{rows}");
    }
}

fn other_tabs_catalog_dialog_log_and_filter_have_fixed_rows() {
    let mut view = sample();
    view.tab = CapTab::Hub;
    view.tabs = vec![
        "Skills 3/4 on".into(),
        "Skill hub".into(),
        "Toolsets".into(),
        "Plugins".into(),
    ];
    view.rows = vec![CapListRow {
        name: "fixture-hub".into(),
        trust: "official".into(),
        source: "official".into(),
        description: "A shared skill.".into(),
        installed: true,
        ..Default::default()
    }];
    view.selected = 0;
    view.footer = "Page 1 of 2 · 25 skills".into();
    view.hints = "Enter preview  i install  / search  n/p page  r refresh  Esc close".into();
    let rows = screen(&view, 120, 40);
    assert_eq!(rows[1], " Skills 3/4 on  [Skill hub]  Toolsets  Plugins");
    assert_eq!(
        rows[3],
        " > fixture-hub                   official   official    A shared skill.  installed"
    );
    assert_eq!(rows[38], " Page 1 of 2 · 25 skills");
    assert_eq!(rows[39], format!(" {}", view.hints));

    view.tab = CapTab::Toolsets;
    view.footer.clear();
    view.rows = vec![CapListRow {
        name: "image_gen".into(),
        label: "Image generation".into(),
        marker: "[ ]".into(),
        tool_count: 3,
        platform: "telegram".into(),
        platform_label: "Telegram".into(),
        ..Default::default()
    }];
    view.hints = "Space on/off  Enter detail  / filter  Tab next tab  r refresh  Esc close".into();
    let rows = screen(&view, 120, 40);
    assert_eq!(
        rows[3],
        " > [ ] Image generation (image_gen)      3 tools    needs setup  · Telegram"
    );

    view.tab = CapTab::Plugins;
    view.rows = vec![CapListRow {
        name: "fixture-plugin".into(),
        marker: "[-]".into(),
        version: "1.2.0".into(),
        source: "yours".into(),
        server_count: 1,
        update_available: true,
        ..Default::default()
    }];
    view.hints = "Space on/off  i install  c catalog  x remove  a all  r refresh  Esc close".into();
    assert_eq!(
        screen(&view, 120, 40)[3],
        " > [-] fixture-plugin                1.2.0     yours    1 servers   update available"
    );
    view.catalog = true;
    view.rows = vec![CapListRow {
        name: "fixture-plugin".into(),
        label: "official".into(),
        installed: true,
        description: "A catalog plugin.".into(),
        ..Default::default()
    }];
    assert_eq!(
        screen(&view, 120, 40)[3],
        " > fixture-plugin                official    installed  A catalog plugin."
    );

    view.catalog = false;
    view.message = vec![
        "Remove fixture-plugin from profile fixture? This deletes its plugin folder. [y/N]".into(),
    ];
    view.hints = "y confirm  any other key cancels".into();
    assert_eq!(screen(&view, 120, 40)[38], format!(" {}", view.message[0]));
    view.message = (1..=10).map(|i| format!("message line {i}")).collect();
    let rows = screen(&view, 120, 40);
    assert_eq!(rows[31], " message line 1");
    assert_eq!(rows[38], " message line 8…");
    assert!(!rows.join("\n").contains("message line 9"));

    view.message.clear();
    view.detail_focus = true;
    view.detail = vec![
        "install failed".into(),
        "$HERMES_HOME/plugins/fixture-plugin".into(),
        "(Hermes sent no install log)".into(),
    ];
    view.detail_scroll = 1;
    assert_eq!(
        screen(&view, 80, 24)[3],
        " $HERMES_HOME/plugins/fixture-plugin"
    );
    view.detail_focus = false;
    view.input = Some("Filter: fixture".into());
    view.text_focus = true;
    view.hints = "Enter submit  Esc cancel".into();
    let rows = screen(&view, 80, 24);
    assert_eq!(rows[22], " Filter: fixture");
    assert_eq!(rows[23], " Enter submit  Esc cancel");
}

#[test]
fn hostile_and_secret_text() {
    let raw = json!({"plugins": [{
        "name":"fixture\u{001b}[2J\u{202e}plugin", "key":"fixture-plugin", "version":"1", "source":"user", "status":"enabled",
        "description":"/synthetic/hermes/plugins/fixture-plugin in /synthetic/user",
        "servers":[], "settings_schema":[{"key":"token", "label":"Access token", "type":"secret", "has_value":true, "value":"fixture-secret-value"}]
    }]}).to_string();
    let safe = redact_paths(&raw, "/synthetic/hermes", "/synthetic/user");
    let data = CapData::from_json(CapTab::Plugins, &serde_json::from_str(&safe).unwrap()).unwrap();
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Plugins);
    let effects = state.update(CapInput::Open(CapTab::Plugins), 0);
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            CapEffect::Fetch { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    state.update(
        CapInput::Loaded {
            tab: CapTab::Plugins,
            generation,
            result: Ok(data),
        },
        1,
    );
    state.update(CapInput::Key(KeyAction::Enter), 2);
    let rows = screen(&state.view(), 120, 40);
    let text = rows.join("\n");
    assert!(text.contains("Access token: set"), "{text}");
    assert!(!text.contains("fixture-secret-value"));
    assert!(!text.contains("/synthetic/hermes"));
    assert!(!text.contains("/synthetic/user"));
    assert!(!text.contains('\u{001b}'));
    assert!(!text.contains('\u{202e}'));
    assert!(text.contains("␛[2J"), "escape must be visible text: {text}");
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn five_hundred_rows_inside_budget() {
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
    let effects = state.update(CapInput::Open(CapTab::Skills), 0);
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            CapEffect::Fetch { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    let skills = (0..500)
        .map(|i| SkillRow {
            name: format!("fixture-skill-{i:03}"),
            description: "A synthetic skill.".into(),
            category: Some(format!("category-{:02}", i % 60)),
            enabled: i % 2 == 0,
            usage: i,
            provenance: Provenance::Bundled,
        })
        .collect();
    state.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation,
            result: Ok(CapData::Skills(skills)),
        },
        1,
    );
    let area = Rect::new(0, 0, 120, 40);
    let mut buf = Buffer::empty(area);
    let mut samples = Vec::new();
    for i in 0..200 {
        let started = Instant::now();
        state.update(CapInput::Key(KeyAction::Down), i + 2);
        draw_capabilities(&state.view(), area, &mut buf);
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    assert!(
        samples[100] <= Duration::from_millis(2),
        "median {:?}",
        samples[100]
    );
    assert!(
        samples[199] <= Duration::from_millis(16),
        "worst {:?}",
        samples[199]
    );
    for now_ms in 202..=2202 {
        assert!(
            state.update(CapInput::Tick, now_ms).is_empty(),
            "idle effect at {now_ms}"
        );
    }
}

#[test]
fn closing_restores_the_inline_view() {
    let (mut term, sink, mut vt) = rig(80, 24);
    let sections = live_view_sections(2);
    let live = LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    };
    term.draw_live(&live).unwrap();
    vt.absorb(&sink);
    let before = vt.rows();
    let cursor_before = vt.cursor();
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
    state.update(CapInput::Open(CapTab::Skills), 0);
    term.open_overlay().unwrap();
    term.draw_overlay(|area, buf| draw_capabilities(&state.view(), area, buf))
        .unwrap();
    vt.absorb(&sink);
    assert!(vt.alternate());
    let effects = state.update(CapInput::Key(KeyAction::Cancel), 1);
    assert!(effects.contains(&CapEffect::Dispose));
    assert!(!state.view().open);
    term.close_overlay().unwrap();
    term.draw_live(&live).unwrap();
    vt.absorb(&sink);
    assert_eq!(vt.rows(), before);
    assert_eq!(vt.cursor(), cursor_before);
    assert!(!vt.alternate());
}
