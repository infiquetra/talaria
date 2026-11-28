//! Read the final cells after overlays, inlined styles and selection fills have been applied.
use crate::view_colours_support as support;
use ratatui::style::Color;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use talaria_app::theme_file::{BUILTIN_THEMES, Base, builtin};
use talaria_tui::theme::{self, ColorDepth, DepthSource, Polarity, PolaritySource, Rgb, color};

#[test]
fn homebrew_uses_the_reference_greens_for_the_draft_and_distinct_inks_for_focus() {
    let resolved = support::resolve(Some(&builtin("homebrew").unwrap()));
    let frames = support::fixtures(&resolved);
    for (frame, text, expected) in [
        ("composer", "Inspect the fixture", Color::Rgb(0, 255, 0)),
        ("composer", "╭", Color::Rgb(0, 153, 0)),
        ("tool", "Shell", Color::Rgb(125, 211, 252)),
        ("picker", "> Example model", Color::Rgb(74, 222, 128)),
    ] {
        let frame = frames.iter().find(|f| f.name == frame).unwrap();
        assert_eq!(support::at_text(&frame.buffer, text).fg, expected);
    }
}

#[test]
fn homebrew_uses_the_reference_purples_in_inline_and_workspace_cells() {
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        widgets::{Paragraph, Widget},
    };
    use serde_json::json;
    use talaria_core::{
        layout::PanelVisibility,
        session::{AssistantCell, Cell, CellId, ToolCell, ToolStatus},
    };
    use talaria_tui::{
        history::{CellLines, PlainCellLines},
        live::LiveView,
        pager::{Details, TranscriptEntry},
        status::{Connection, StatusInputs, StatusSection},
        theme::cells::ThemedCells,
        tool_cell::{Detail, ToolCellLines, ToolRenderOptions},
        workspace::{self, WorkspaceView},
    };

    let file = builtin("homebrew").unwrap();
    let resolved = support::resolve(Some(&file));
    let theme = &resolved.theme;
    let status_inputs = StatusInputs {
        connection: Connection::Connected,
        model: Some("provider/example-model".into()),
        reasoning_effort: Some("medium".into()),
        hivecode: true,
        ..StatusInputs::default()
    };
    let status = StatusSection::new(&status_inputs, None, &theme.status).in_conversation();
    let footer = LiveView {
        sections: vec![&status],
    };
    let assistant = Cell::Assistant(AssistantCell {
        id: CellId::new(1),
        text: "Ordinary **strong** and *emphasis* beside `code`.".into(),
        sealed: true,
        interim: false,
        transformed: false,
    });
    let shell = Cell::Tool(ToolCell {
        id: CellId::new(2),
        tool_id: "fixture".into(),
        name: "terminal".into(),
        context: None,
        args: Some(json!({"command": r"printf 'left|right' | head && echo \; > out # |comment"})),
        labels: vec![],
        status: ToolStatus::Succeeded,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    });
    let tool_renderer = |detail| {
        ToolCellLines::new(
            Box::new(PlainCellLines),
            ToolRenderOptions {
                width: 120,
                theme: theme.tool.clone(),
                color: theme.color,
                detail,
                expand_hint: None,
                elapsed: None,
            },
        )
        .in_conversation()
    };
    let area = Rect::new(0, 0, 120, 24);
    let mut inline = Buffer::empty(area);
    let cells = ThemedCells {
        theme,
        transcript_key: None,
    };
    let mut lines = cells.lines(&assistant, area.width);
    lines.extend(tool_renderer(Detail::Compact).lines(&shell, area.width));
    Paragraph::new(lines).render(area, &mut inline);
    status.render_into(Rect::new(0, 23, area.width, 1), &mut inline);

    let mut workspace_buffer = Buffer::empty(area);
    workspace::draw_with_theme(
        &WorkspaceView {
            entries: &[
                TranscriptEntry::Cell(&assistant),
                TranscriptEntry::Cell(&shell),
            ],
            details: Details::Expanded,
            panel: PanelVisibility::Hidden,
            checklist: None,
            checklist_expanded: false,
            menu: workspace::menu::MenuState::hidden(),
            footer: &footer,
            work: None,
        },
        area,
        &mut workspace_buffer,
        &tool_renderer(Detail::Full),
        &theme.views,
    );
    let goal_purple = Color::Rgb(178, 0, 178);
    let text_purple = Color::Rgb(214, 92, 255);
    let black = Color::Rgb(0, 0, 0);
    for (buffer, background) in [(&inline, Color::Reset), (&workspace_buffer, black)] {
        for text in ["example-model", "medium"] {
            let cell = support::at_text(buffer, text);
            assert_eq!((cell.fg, cell.bg), (goal_purple, background), "{text}");
            assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
        }
        for text in ["strong", "emphasis", "| head", "&&", "> out"] {
            assert_eq!(support::at_text(buffer, text).fg, text_purple, "{text}");
        }
        for text in ["|right", "; >", "|comment"] {
            assert_ne!(support::at_text(buffer, text).fg, text_purple, "{text}");
        }
        assert_eq!(
            support::at_text(buffer, "hivecode").fg,
            Color::Rgb(125, 211, 252)
        );
    }
    // The sampled status ink is intentionally darker than the transcript's purple on black.
    assert!(color::contrast_ratio(Rgb(178, 0, 178), Rgb(0, 0, 0)) >= 3.5);
}

#[test]
fn homebrew_status_purple_respects_terminal_color_depth() {
    use ratatui::{buffer::Buffer, layout::Rect};
    use talaria_tui::status::{Connection, StatusInputs, StatusSection};

    let file = builtin("homebrew").unwrap();
    let inputs = StatusInputs {
        connection: Connection::Connected,
        model: Some("example-model".into()),
        reasoning_effort: Some("medium".into()),
        ..StatusInputs::default()
    };
    for depth in [ColorDepth::Ansi256, ColorDepth::Ansi16, ColorDepth::None] {
        let resolved = theme::resolve_with_theme_file(
            &support::inputs((depth, DepthSource::Config)),
            Some(&file),
        );
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);
        StatusSection::new(&inputs, None, &resolved.theme.status)
            .in_conversation()
            .render_into(area, &mut buffer);
        let expected = resolved.elements["status.model"].fg.unwrap_or(Color::Reset);
        assert_eq!(support::at_text(&buffer, "example-model").fg, expected);
        assert_eq!(support::at_text(&buffer, "medium").fg, expected);
    }
}

#[test]
fn builtins_keep_visible_text_and_structure_legible() {
    support::warm_syntax();
    let mut failures = BTreeSet::new();
    let mut original_text = BTreeMap::new();
    for id in BUILTIN_THEMES {
        let file = builtin(id).unwrap();
        let dark = file.extends != Base::Light;
        for (depth, dark) in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
            ColorDepth::None,
        ]
        .into_iter()
        .map(|depth| (depth, dark))
        .chain((file.extends == Base::Terminal).then_some((ColorDepth::TrueColor, false)))
        {
            let mut inputs = support::inputs((depth, DepthSource::Config));
            inputs.polarity = (
                if dark {
                    Polarity::Dark
                } else {
                    Polarity::Light
                },
                PolaritySource::Config,
            );
            let resolved = theme::resolve_with_theme_file(&inputs, Some(&file));
            let canvas = resolved.elements["transcript.background"];
            // Terminal delegates its named colours to the emulator. The other themes own
            // their canvas. Inline frames here run on a terminal matching that canvas.
            let default_fg = canvas.fg.and_then(color::rgb_of).unwrap_or(if dark {
                Rgb(229, 229, 229)
            } else {
                Rgb(0, 0, 0)
            });
            let default_bg = canvas.bg.and_then(color::rgb_of).unwrap_or(if dark {
                Rgb(0, 0, 0)
            } else {
                Rgb(255, 255, 255)
            });
            let mut fixtures = support::fixtures(&resolved);
            fixtures.extend(crate::theme_scenarios::fixtures(&resolved));
            fixtures.extend(crate::theme_tabs::fixtures(&resolved));
            let mut lowest = [f64::INFINITY; 2];
            for frame in fixtures {
                assert!(
                    frame
                        .buffer
                        .content
                        .iter()
                        .any(|cell| !cell.symbol().trim().is_empty()),
                    "{} blank",
                    frame.name
                );
                if depth == ColorDepth::TrueColor {
                    let visible = support::text(&frame.buffer);
                    let original = original_text
                        .entry(frame.name)
                        .or_insert_with(|| visible.clone());
                    assert_eq!(
                        original, &visible,
                        "{id}: {} changed screen content",
                        frame.name
                    );
                }
                let mut checked = HashSet::new();
                for cell in &frame.buffer.content {
                    let glyph = cell.symbol();
                    if glyph.trim().is_empty() {
                        continue;
                    }
                    let structural = support::is_chrome(glyph) || glyph == "▎";
                    if !checked.insert((cell.fg, cell.bg, cell.underline_color, structural)) {
                        continue;
                    }
                    let fg = color::rgb_of(cell.fg).unwrap_or(default_fg);
                    let bg = color::rgb_of(cell.bg).unwrap_or(default_bg);
                    let ratio = color::contrast_ratio(fg, bg);
                    let minimum = &mut lowest[usize::from(structural)];
                    *minimum = minimum.min(ratio);
                    if depth == ColorDepth::None {
                        assert_eq!(
                            (cell.fg, cell.bg, cell.underline_color),
                            (Color::Reset, Color::Reset, Color::Reset),
                            "{id} {} {glyph}",
                            frame.name
                        );
                    } else if ratio < if structural { 3.0 } else { 4.5 } {
                        failures.insert(format!(
                            "{id} {depth:?} {} {glyph:?} {} on {} {ratio:.2}",
                            frame.name,
                            fg.hex(),
                            bg.hex()
                        ));
                    }
                }
            }
            if depth != ColorDepth::None {
                println!(
                    "{id} {depth:?} {:?}: text {:.2}:1, structure {:.2}:1",
                    inputs.polarity.0, lowest[0], lowest[1]
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.into_iter().collect::<Vec<_>>().join("\n")
    );
}
