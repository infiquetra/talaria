use crate::support;

use ratatui::{buffer::Buffer, layout::Rect};
use support::{ByteSink, Vt, probe};
use talaria_core::models::{BrowseCounts, BrowseView, ListScope, PickerRow, PickerView, RowKey};
use talaria_tui::picker_frame::{PickerLayout, PickerMode, draw_picker_frame, list_height};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn model(text: &str) -> PickerRow {
    PickerRow {
        key: Some(RowKey::Model {
            provider: "example".into(),
            model: text.into(),
        }),
        text: text.into(),
        badges: vec!["effort ?".into()],
        price: None,
        dim: false,
        header: false,
    }
}

fn fixture_rows() -> Vec<PickerRow> {
    let mut rows = vec![PickerRow {
        key: None,
        text: "Example provider".into(),
        badges: vec![],
        price: None,
        dim: false,
        header: true,
    }];
    rows.push(model("★ ● reasoner-large"));
    rows.push(model("reasoner-small"));
    rows
}

fn fixture_details() -> Vec<String> {
    [
        "Selected: Example provider / reasoner-large",
        "Requested: high",
        "Support: unverified for this route",
        "Applied: unknown",
        "Context: unknown",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn view<'a>(rows: &'a [PickerRow], details: &'a [String]) -> PickerView<'a> {
    PickerView {
        title: "Switch model · this session",
        filter: "",
        effort: "effort: high",
        hints: "Enter switch · Esc clear,close",
        refreshing: false,
        rows,
        selected: Some(1),
        banner: None,
        browse: Some(BrowseView {
            scope: ListScope::All,
            counts: BrowseCounts {
                models: 2,
                providers: 1,
                starred: 1,
                configured: 1,
                setup: 0,
            },
            details,
            details_open: false,
            details_scroll: 0,
        }),
    }
}

fn lines(buffer: &Buffer) -> Vec<String> {
    (buffer.area.y..buffer.area.bottom())
        .map(|y| {
            (buffer.area.x..buffer.area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn rendered(view: &PickerView<'_>, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    draw_picker_frame(view, area, &mut buffer);
    buffer
}

#[test]
fn threshold_edges_and_reference_geometry() {
    for (width, height, mode) in [
        (39, 24, PickerMode::SizeMessage),
        (80, 9, PickerMode::SizeMessage),
        (40, 10, PickerMode::Compact),
        (59, 24, PickerMode::Compact),
        (80, 15, PickerMode::Compact),
        (60, 16, PickerMode::Below),
        (111, 40, PickerMode::Below),
        (120, 27, PickerMode::Below),
        (112, 28, PickerMode::Side),
    ] {
        assert_eq!(
            PickerLayout::new(Rect::new(0, 0, width, height)).mode,
            mode,
            "{width}x{height}"
        );
    }
    let layout = PickerLayout::new(Rect::new(0, 0, 80, 24));
    assert_eq!(layout.content, Rect::new(1, 1, 78, 22));
    assert_eq!(layout.filter, Rect::new(1, 1, 78, 1));
    assert_eq!(layout.chips, Rect::new(1, 2, 78, 1));
    assert_eq!(layout.count, Rect::new(1, 3, 78, 1));
    assert_eq!(layout.list, Rect::new(1, 4, 78, 10));
    assert_eq!(layout.details, Rect::new(1, 14, 78, 7));
    assert_eq!(layout.footer, Rect::new(1, 21, 78, 2));
    let side = PickerLayout::new(Rect::new(0, 0, 120, 40));
    assert_eq!(side.list.width, 70);
    assert_eq!(side.details.width, 48);
    assert_eq!(side.list.y, side.details.y);
    assert_eq!(side.list.height, side.details.height);
    assert_eq!(side.details.right(), side.content.right());
}

#[test]
fn snapshots_at_both_sizes() {
    let rows = fixture_rows();
    let details = fixture_details();
    let view = view(&rows, &details);
    for (width, height, expected) in [
        (80, 24, include_str!("../snapshots/picker/models-80x24.txt")),
        (
            120,
            40,
            include_str!("../snapshots/picker/models-120x40.txt"),
        ),
    ] {
        let actual = lines(&rendered(&view, width, height)).join("\n") + "\n";
        assert_eq!(actual, expected, "{width}x{height}");
    }
}

#[test]
fn viewport_height_matches_the_rows_drawn() {
    let rows: Vec<_> = (0..100).map(|i| model(&format!("model-{i:03}"))).collect();
    for (width, height) in [(40, 10), (60, 12), (60, 16), (80, 24), (112, 28), (120, 40)] {
        let mut view = view(&rows, &[]);
        view.selected = Some(0);
        let buffer = rendered(&view, width, height);
        let layout = PickerLayout::new(buffer.area);
        let painted = lines(&buffer)
            .iter()
            .filter(|line| line.contains("model-"))
            .count();
        assert_eq!(painted, list_height(buffer.area), "{width}x{height}");
        assert_eq!(painted, usize::from(layout.list.height.saturating_sub(1)));
    }
    assert_eq!(list_height(Rect::new(0, 0, 80, 24)), 9);
}

#[test]
fn large_catalogue_at_both_sizes() {
    let mut rows = Vec::new();
    for provider in 0..20 {
        rows.push(PickerRow {
            key: None,
            text: format!("Example provider {provider:02}"),
            badges: vec![],
            price: None,
            dim: false,
            header: true,
        });
        for index in 0..30 {
            rows.push(model(&format!("model-{provider:02}-{index:02}")));
        }
    }
    for (width, height) in [(80, 24), (120, 40)] {
        let mut view = view(&rows, &[]);
        view.browse.as_mut().unwrap().counts = BrowseCounts {
            models: 600,
            providers: 20,
            ..BrowseCounts::default()
        };
        for selected in [1, 311, 619] {
            view.selected = Some(selected);
            let buffer = rendered(&view, width, height);
            let layout = PickerLayout::new(buffer.area);
            let y = (layout.list.y..layout.list.bottom() - 1)
                .find(|&y| {
                    let text: String = (layout.list.x..layout.list.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect();
                    text.contains(&rows[selected].text)
                })
                .expect("selected model remains in the list viewport");
            let line: String = (layout.list.x..layout.list.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            assert!(line.starts_with('>'), "selection pointer: {line}");
            assert!(
                lines(&buffer)
                    .join("\n")
                    .contains("600 models · 20 providers")
            );
        }
    }
}

#[test]
fn truncation_keeps_the_badge() {
    for label in [
        "identifier-".repeat(20),
        "会話".repeat(40),
        "e\u{301}".repeat(200),
    ] {
        let rows = vec![model(&label)];
        let mut view = view(&rows, &[]);
        view.selected = Some(0);
        for width in [40, 60, 80, 120] {
            let buffer = rendered(&view, width, 24);
            let layout = PickerLayout::new(buffer.area);
            let text: String = (layout.list.x..layout.list.right())
                .map(|x| buffer[(x, layout.list.y)].symbol())
                .collect();
            assert!(text.contains('…'), "{width}: {text}");
            assert!(
                text.contains("effort ?"),
                "badge must not be clipped: {text}"
            );
            assert!(
                !text.contains("e…"),
                "must not separate a combining mark: {text}"
            );
            let badge_x = layout.list.right() - 8;
            for (offset, character) in "effort ?".chars().enumerate() {
                assert_eq!(
                    buffer[(badge_x + offset as u16, layout.list.y)].symbol(),
                    character.to_string(),
                    "badge owns its reserved terminal cells"
                );
            }
            if label.contains('\u{301}') {
                for x in layout.list.x..badge_x {
                    assert_ne!(
                        buffer[(x, layout.list.y)].symbol(),
                        "e",
                        "the combining mark must remain attached to its base character"
                    );
                }
            }
        }
    }
}

#[test]
fn details_layer_scrolls_at_both_sizes() {
    let rows = fixture_rows();
    let mut details = vec![format!("Selected: {}", "long-identifier-".repeat(40))];
    details.extend((0..80).map(|i| format!("Warning line {i:02}: {}", "long warning ".repeat(12))));
    for (width, height) in [(80, 24), (120, 40)] {
        let mut view = view(&rows, &details);
        view.browse.as_mut().unwrap().details_open = true;
        let first = lines(&rendered(&view, width, height)).join("\n");
        assert!(first.contains("Selected:"));
        assert!(!first.contains("Warning line 79:"));
        assert!(first.contains("Esc back"));
        view.browse.as_mut().unwrap().details_scroll = 40;
        let next = lines(&rendered(&view, width, height)).join("\n");
        assert_ne!(first, next);
        assert!(!next.contains("Selected:"));
        assert!(next.contains("Warning line"));
        assert_eq!(view.selected, Some(1));
    }
}

#[test]
fn hostile_text_in_every_area() {
    let hostile = "\u{1b}\u{7}\u{202e}literal";
    let rows = vec![model(hostile)];
    let details = vec![hostile.into()];
    let mut view = view(&rows, &details);
    view.title = hostile;
    view.filter = hostile;
    view.effort = hostile;
    view.hints = hostile;
    view.banner = Some(hostile);
    view.selected = Some(0);
    for full in [false, true] {
        view.browse.as_mut().unwrap().details_open = full;
        let screen = lines(&rendered(&view, 120, 40)).join("\n");
        for raw in ['\u{1b}', '\u{7}', '\u{202e}'] {
            assert!(!screen.contains(raw));
        }
        for visible in ["␛", "␇", "‹U+202E›"] {
            assert!(screen.contains(visible), "{screen}");
        }
    }
}

#[test]
fn arbitrary_area_is_respected_and_tiny_sizes_do_not_panic() {
    let rows = fixture_rows();
    let details = fixture_details();
    let view = view(&rows, &details);
    let full = Rect::new(0, 0, 140, 50);
    let area = Rect::new(7, 3, 80, 24);
    let mut buffer = Buffer::empty(full);
    for cell in &mut buffer.content {
        cell.set_symbol("x");
    }
    draw_picker_frame(&view, area, &mut buffer);
    for y in 0..full.height {
        for x in 0..full.width {
            if !area.contains((x, y).into()) {
                assert_eq!(buffer[(x, y)].symbol(), "x");
            }
        }
    }
    assert_eq!(buffer[(area.x, area.y)].symbol(), "┌");
    for width in 0..41 {
        for height in 0..11 {
            let _ = rendered(&view, width, height);
        }
    }
    assert!(lines(&rendered(&view, 39, 9)).join("\n").contains("40×10"));
}

#[test]
fn idle_picker_writes_nothing() {
    let rows = fixture_rows();
    let details = fixture_details();
    let view = view(&rows, &details);
    let sink = ByteSink::default();
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .unwrap();
    terminal.open_overlay().unwrap();
    sink.take();
    terminal
        .draw_overlay(|area, buffer| draw_picker_frame(&view, area, buffer))
        .unwrap();
    let mut vt = Vt::new(80, 24);
    vt.absorb(&sink);
    assert!(vt.rows().join("\n").contains("reasoner-large"));
    let again = terminal
        .draw_overlay(|area, buffer| draw_picker_frame(&view, area, buffer))
        .unwrap();
    assert!(!again.drew);
    assert_eq!(again.bytes, 0);
    assert!(sink.is_empty());
}
mod independent {
    use talaria_tui::picker_frame::PickerMode as FrameMode;

    use super::support;
    mod catalogue {
        pub fn parse(value: serde_json::Value) -> hermes_contract::schemas::ModelOptionsResult {
            serde_json::from_value(value).expect("synthetic model options")
        }
    }
    mod drive {
        use talaria_core::models::{
            ModelsEffect, ModelsInput, ModelsState, PickerKey, SavedModels,
        };
        pub fn open_loaded(
            options: hermes_contract::schemas::ModelOptionsResult,
            effort: Option<&str>,
            saved: SavedModels,
        ) -> ModelsState {
            let mut state = ModelsState::new("independent", Ok(saved));
            state.set_list_height(10);
            state.update(ModelsInput::SessionOpened {
                session_id: "independent-session".into(),
                revision: Some(12),
                execution_generation: Some(4),
            });
            state.update(ModelsInput::ForegroundChanged {
                session_id: Some("independent-session".into()),
            });
            state.update(ModelsInput::OpenPicker {
                session_id: Some("independent-session".into()),
                profile: "fixture".into(),
            });
            state.update(ModelsInput::OptionsLoaded {
                session_id: "independent-session".into(),
                result: options,
            });
            state.update(ModelsInput::EffortRead {
                session_id: "independent-session".into(),
                value: effort.map(str::to_owned),
            });
            state
        }
        pub fn press(state: &mut ModelsState, key: PickerKey) -> Vec<ModelsEffect> {
            state.update(ModelsInput::Picker(key))
        }
    }
    struct FrameGeometry {
        mode: FrameMode,
        filter: Rect,
        chips: Rect,
        count: Rect,
        list: Rect,
        scrollbar: Rect,
        details: Rect,
        footer: [Rect; 2],
    }
    fn layout(area: Rect) -> FrameGeometry {
        let source = talaria_tui::picker_frame::PickerLayout::new(area);
        FrameGeometry {
            mode: source.mode,
            filter: source.filter,
            chips: source.chips,
            count: source.count,
            list: Rect {
                height: source.list.height.saturating_sub(1),
                ..source.list
            },
            scrollbar: Rect::new(
                source.list.x,
                source.list.bottom().saturating_sub(1),
                source.list.width,
                u16::from(source.list.height > 0),
            ),
            details: source.details,
            footer: [
                Rect::new(source.footer.x, source.footer.y, source.footer.width, 1),
                Rect::new(source.footer.x, source.footer.y + 1, source.footer.width, 1),
            ],
        }
    }
    // Exercise truncation through the public frame, reconstructing wide graphemes once.
    fn truncate_label(label: &str, badge: &str, width: u16) -> String {
        use talaria_core::models::{PickerRow, RowKey};
        let rows = [PickerRow {
            key: Some(RowKey::Model {
                provider: "example-00".into(),
                model: label.into(),
            }),
            text: label.into(),
            badges: vec![badge.into()],
            price: None,
            dim: false,
            header: false,
        }];
        let view = PickerView {
            title: "Model picker",
            filter: "",
            effort: "",
            hints: "",
            refreshing: false,
            rows: &rows,
            selected: None,
            banner: None,
            browse: None,
        };
        let buffer = paint(&view, width, 12);
        let mut row = String::new();
        let mut x = 2_u16;
        while x < width {
            let symbol = buffer[(x, 2)].symbol();
            row.push_str(symbol);
            x += u16::try_from(symbol.width().max(1)).unwrap();
        }
        row
    }

    use ratatui::{buffer::Buffer, layout::Rect};
    use serde_json::json;
    use talaria_core::models::{PickerKey, PickerView, SavedModels};
    use talaria_tui::model_picker::draw_model_picker;
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;

    fn state() -> talaria_core::models::ModelsState {
        drive::open_loaded(
            catalogue::parse(
                json!({"provider":"example-00","model":"reasoner-large","providers":[{"slug":"example-00","name":"Example provider 00","authenticated":true,"models":["reasoner-large"]}]}),
            ),
            Some("high"),
            SavedModels::empty(),
        )
    }
    fn paint(view: &PickerView<'_>, width: u16, height: u16) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        draw_model_picker(view, buffer.area, &mut buffer);
        buffer
    }
    fn text(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn thresholds_at_exact_edges() {
        for (w, h, mode) in [
            (112, 28, FrameMode::Side),
            (111, 28, FrameMode::Below),
            (112, 27, FrameMode::Below),
            (60, 16, FrameMode::Below),
            (59, 16, FrameMode::Compact),
            (60, 15, FrameMode::Compact),
            (40, 10, FrameMode::Compact),
            (39, 10, FrameMode::SizeMessage),
            (40, 9, FrameMode::SizeMessage),
        ] {
            assert_eq!(layout(Rect::new(0, 0, w, h)).mode, mode, "{w}x{h}");
        }
    }

    #[test]
    fn viewport_height_matches_the_rows_drawn() {
        let narrow = layout(Rect::new(0, 0, 80, 24));
        assert_eq!(narrow.filter, Rect::new(1, 1, 78, 1));
        assert_eq!(narrow.chips, Rect::new(1, 2, 78, 1));
        assert_eq!(narrow.count, Rect::new(1, 3, 78, 1));
        assert_eq!(narrow.list, Rect::new(1, 4, 78, 9));
        assert_eq!(narrow.scrollbar, Rect::new(1, 13, 78, 1));
        assert_eq!(narrow.details, Rect::new(1, 14, 78, 7));
        assert_eq!(
            narrow.footer,
            [Rect::new(1, 21, 78, 1), Rect::new(1, 22, 78, 1)]
        );
        let wide = layout(Rect::new(0, 0, 120, 40));
        assert_eq!(wide.list, Rect::new(1, 4, 70, 32));
        assert_eq!(wide.scrollbar, Rect::new(1, 36, 70, 1));
        assert_eq!(wide.details, Rect::new(71, 4, 48, 33));
        assert_eq!(
            wide.footer,
            [Rect::new(1, 37, 118, 1), Rect::new(1, 38, 118, 1)]
        );
        let offset = layout(Rect::new(5, 7, 80, 24));
        assert_eq!(offset.list, Rect::new(6, 11, 78, 9));
    }

    #[test]
    fn snapshots_of_visible_picker_elements_at_both_sizes() {
        let state = state();
        for (w, h, expected) in [
            (
                80,
                24,
                "All\nStarred\nConfigured\nSet up\nreasoner-large\neffort ?\nRequested: high\nSupport: unverified for this route\nApplied: unknown\nContext: unknown\nF2/Ctrl+E details\n",
            ),
            (
                120,
                40,
                "All\nStarred\nConfigured\nSet up\nreasoner-large\neffort ?\nRequested: high\nSupport: unverified for this route\nApplied: unknown\nContext: unknown\nF2/Ctrl+E details\n",
            ),
        ] {
            let actual = text(&paint(&state.view().unwrap(), w, h));
            // Snapshot semantic text; rectangle tests independently pin where those elements live.
            let tokens = [
                "All",
                "Starred",
                "Configured",
                "Set up",
                "reasoner-large",
                "effort ?",
                "Requested: high",
                "Support: unverified for this route",
                "Applied: unknown",
                "Context: unknown",
                "F2/Ctrl+E details",
            ];
            let snapshot = tokens
                .into_iter()
                .filter(|s| actual.contains(s))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert_eq!(snapshot, expected, "{w}x{h}: {actual}");
        }
    }

    #[test]
    fn exact_model_state_frames_at_both_sizes() {
        let state = state();
        for (width, height, expected) in [
            (
                80,
                24,
                include_str!("../snapshots/picker/models-state-80x24.txt"),
            ),
            (
                120,
                40,
                include_str!("../snapshots/picker/models-state-120x40.txt"),
            ),
        ] {
            assert_eq!(
                text(&paint(&state.view().unwrap(), width, height)) + "\n",
                expected
            );
        }
    }

    #[test]
    fn large_catalogue_at_both_sizes() {
        let providers=(0..20).map(|p|json!({"slug":format!("example-{p:02}"),"name":format!("Example provider {p:02}"),"authenticated":true,"models":(0..30).map(|m|format!("model-{m:02}")).collect::<Vec<_>>()})).collect::<Vec<_>>();
        for (w, h) in [(80, 24), (120, 40)] {
            let mut state = drive::open_loaded(
                catalogue::parse(
                    json!({"provider":"example-00","model":"model-00","providers":providers}),
                ),
                Some("high"),
                SavedModels::empty(),
            );
            state.set_list_height(usize::from(layout(Rect::new(0, 0, w, h)).list.height));
            let first = text(&paint(&state.view().unwrap(), w, h));
            assert!(
                first.contains("600")
                    && first.contains("20")
                    && first.to_lowercase().contains("below"),
                "{first}"
            );
            drive::press(&mut state, PickerKey::End);
            let last = text(&paint(&state.view().unwrap(), w, h));
            assert!(
                last.contains("model-29") && last.to_lowercase().contains("above"),
                "{last}"
            );
        }
    }

    #[test]
    fn truncation_keeps_the_badge() {
        for label in [
            "organization/team/reasoner-with-a-long-name".repeat(4),
            "模型e\u{301}👩‍💻".repeat(20),
        ] {
            for width in [40, 60, 70] {
                let row = truncate_label(&label, "effort ?", width);
                assert!(row.width() <= usize::from(width));
                assert!(row.ends_with("effort ?"), "{row}");
                let prefix = row.strip_suffix("effort ?").unwrap().trim_end();
                if label.width() + 1 + "effort ?".width() > usize::from(width) {
                    assert!(prefix.ends_with('…'), "{row}");
                }
                let kept = prefix.trim_end_matches('…');
                assert!(
                    label
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .chain([label.len()])
                        .any(|i| &label[..i] == kept),
                    "split a grapheme: {row}"
                );
            }
        }
    }

    #[test]
    fn details_layer_scrolls_at_both_sizes() {
        for (w, h) in [(80, 24), (120, 40)] {
            let mut state = drive::open_loaded(
                catalogue::parse(
                    json!({"provider":"example-00","model":"long-model-name".repeat(30),"providers":[{"slug":"example-00","name":"Example provider 00","authenticated":true,"models":["long-model-name".repeat(30)],"warning":(0..80).map(|n|format!("warning line {n}")).collect::<Vec<_>>().join("\n")}]}),
                ),
                Some("high"),
                SavedModels::empty(),
            );
            drive::press(&mut state, PickerKey::Details);
            let before = text(&paint(&state.view().unwrap(), w, h));
            drive::press(&mut state, PickerKey::PageDown);
            let view = state.view().unwrap();
            assert!(view.browse.unwrap().details_open && view.browse.unwrap().details_scroll > 0);
            assert_ne!(text(&paint(&view, w, h)), before);
            drive::press(&mut state, PickerKey::Escape);
            assert!(!state.view().unwrap().browse.unwrap().details_open);
        }
    }

    #[test]
    fn hostile_text_in_every_area() {
        let state = state();
        let mut view = state.view().unwrap();
        let hostile = "literal\u{1b}[2J\u{7}\u{202e}end";
        let details = vec![hostile.to_owned()];
        let mut rows = view.rows.to_vec();
        for row in &mut rows {
            row.text = hostile.into();
        }
        view.title = hostile;
        view.filter = hostile;
        view.banner = Some(hostile);
        view.rows = &rows;
        view.browse.as_mut().unwrap().details = &details;
        let actual = text(&paint(&view, 120, 40));
        assert!(!actual.contains(['\u{1b}', '\u{7}', '\u{202e}']));
        assert!(actual.contains('␛') && actual.contains('␇') && actual.contains("‹U+202E›"));
        assert!(
            actual.matches("literal").count() >= 5,
            "title, filter, header, row and details: {actual}"
        );
    }

    #[test]
    fn idle_picker_writes_nothing() {
        let state = state();
        let sink = support::ByteSink::default();
        let mut terminal = talaria_tui::terminal::InlineTerminal::new(
            sink.clone(),
            (80, 24),
            2,
            support::probe(false, false, Some((0, 23))),
            talaria_tui::terminal::RestoreLink::detached(),
        )
        .unwrap();
        terminal.open_overlay().unwrap();
        terminal
            .draw_overlay(|a, b| draw_model_picker(&state.view().unwrap(), a, b))
            .unwrap();
        sink.take();
        let idle = terminal
            .draw_overlay(|a, b| draw_model_picker(&state.view().unwrap(), a, b))
            .unwrap();
        assert!(!idle.drew && idle.bytes == 0 && sink.is_empty());
    }
}
