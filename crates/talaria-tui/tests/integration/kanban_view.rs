//! Board overlay drawing at a fixed size.

use std::io::Write;
use std::sync::{Arc, Mutex};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use ratatui::{TerminalOptions, Viewport};
use talaria_core::kanban::{
    BoardCard, BoardKey, BoardMode, DetailMotion, FrameCard, FrameColumn, KanbanBoardState,
    KanbanEffect, KanbanFrame, KanbanInput, update,
};
use talaria_tui::kanban_view::{
    KanbanOverlay, KanbanView, detail_viewport, draw_kanban, draw_kanban_with_theme,
};
use talaria_tui::theme::{ColorDepth, DepthSource, resolve_with_theme_file};

use crate::view_colours_support as theme_support;

#[test]
fn board_renders_columns_and_cards() {
    let parser = render(&frame());
    let text = parser.screen().contents();
    assert!(
        text.contains("Kanban · mock · 2 cards · Live"),
        "view missing"
    );
    assert!(text.contains("Esc close"), "view missing");
    assert!(text.contains("> fix the parser"), "view missing");
    assert!(text.contains("! waiting input"), "view missing");
    let mut overlay = KanbanOverlay::open();
    assert!(overlay.is_open(), "view missing");
    overlay.close();
    assert!(overlay.showing_inline(), "view missing");
    assert!(!overlay.is_open(), "view missing");
}

#[test]
fn kanban_uses_list_colours_and_keeps_selection_in_monochrome() {
    let file = talaria_app::theme_file::parse(
        r##"
schema = 1
extends = "dark"
[elements."list.background"]
bg = "#212223"
[elements."list.header"]
fg = "#aabbcc"
[elements."list.selected"]
bg = "#1155aa"
[elements."list.warning"]
fg = "#ffbd00"
[elements."list.muted"]
fg = "#98a0aa"
"##,
    )
    .unwrap();
    let area = Rect::new(0, 0, 81, 12);
    let mut buffer = Buffer::empty(area);
    let frame = frame();
    let style = theme_support::resolve(Some(&file)).theme.views;
    draw_kanban_with_theme(&frame, area, &mut buffer, &style);
    assert_eq!(buffer[(80, 1)].bg, Color::Rgb(0x21, 0x22, 0x23));
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(0xaa, 0xbb, 0xcc));
    assert_eq!(buffer[(0, 2)].bg, Color::Rgb(0x11, 0x55, 0xaa));
    assert_eq!(buffer[(42, 2)].fg, Color::Rgb(0xff, 0xbd, 0));
    assert_eq!(buffer[(0, 11)].fg, Color::Rgb(0x98, 0xa0, 0xaa));

    let inputs = theme_support::inputs((ColorDepth::None, DepthSource::Config));
    let style = resolve_with_theme_file(&inputs, Some(&file)).theme.views;
    draw_kanban_with_theme(&frame, area, &mut buffer, &style);
    assert!(theme_support::no_colour(&buffer).is_none());
    assert!(buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
    assert_eq!(buffer[(0, 2)].symbol(), ">");
    assert_eq!(buffer[(42, 2)].symbol(), "!");
}

#[test]
fn clipped_card_titles_keep_wide_and_combining_graphemes_whole() {
    let mut frame = frame();
    let card = &mut frame.columns[0].cards[0];
    card.title = format!("界e\u{301}{}", "界".repeat(200));
    card.assignee = "123456789012345678901234567890".into();
    let area = Rect::new(0, 0, 80, 5);
    let mut buffer = Buffer::empty(area);
    draw_kanban(&frame, area, &mut buffer);
    assert_eq!(buffer[(2, 2)].symbol(), "界");
    assert_eq!(buffer[(4, 2)].symbol(), "e\u{301}");
    let text: String = (0..39).map(|x| buffer[(x, 2)].symbol()).collect();
    assert!(text.contains("123456…"));
    assert!(text.trim_end().ends_with("1d"));
}

#[test]
fn narrow_board_summary_and_card_hit_targets_match_the_rendered_cells() {
    let mut state = KanbanBoardState::blank();
    let mut selected = frame().detail.unwrap();
    selected.status = "triage".into();
    selected.body = "A description that should stay in the full card.".into();
    selected.latest_summary = Some("The schema check passed.".into());
    let mut other = selected.clone();
    other.id = "other".into();
    other.title = "Read the fixture".into();
    other.assignee = "test owner".into();
    other.status = "todo".into();
    state.load_cards(vec![selected, other]);
    let area = Rect::new(4, 5, 60, 24);
    let mut buffer = Buffer::empty(area);
    KanbanView::draw(&state, area, &mut buffer);
    let rows = (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let screen = rows.join("\n");
    assert!(
        screen.contains("Off-screen: Scheduled 0") && screen.contains("Done 0"),
        "{screen}"
    );
    assert!(
        screen.contains("Latest result  The schema check passed."),
        "{screen}"
    );
    assert!(!screen.contains("A description that should stay in the full card."));
    let y = |text: &str| area.y + rows.iter().position(|line| line.contains(text)).unwrap() as u16;
    let x = area.x + 34;
    let header = y("Todo (1)");
    let card = y("test owner");
    let preview = y("Selected card");
    assert_eq!(
        KanbanView::hit_test(&state, area, x, header),
        Some(BoardKey::SelectColumn(1))
    );
    assert_eq!(
        KanbanView::hit_test(&state, area, area.x + 29, header),
        None,
        "divider"
    );
    assert_eq!(KanbanView::hit_test(&state, area, x, preview), None);
    let hit =
        KanbanView::hit_test(&state, area, x, card).expect("the metadata row selects its card");
    assert_eq!(hit, BoardKey::SelectCard("other".into()));
    assert!(update(&mut state, KanbanInput::Key(hit)).is_empty());
    assert_eq!(state.selected().unwrap().id, "other");
    state.mode = BoardMode::Filter;
    assert_eq!(KanbanView::hit_test(&state, area, x, card), None);
}

#[test]
fn click_escape_matches_the_board_mode_and_preserves_text_until_cancel() {
    let mut state = KanbanBoardState::blank();
    state.live = true;
    state.cursor = 25534;
    let area = Rect::new(4, 5, 40, 16);
    for (mode, hint) in [
        (BoardMode::Browse, "Esc close"),
        (BoardMode::Filter, "Esc cancel"),
        (
            BoardMode::Comment {
                text: "a draft".into(),
            },
            "Esc cancel",
        ),
    ] {
        state.mode = mode;
        let mut buffer = Buffer::empty(area);
        KanbanView::draw(&state, area, &mut buffer);
        let y = (area.y..area.bottom())
            .find(|y| {
                let text: String = (area.x..area.right())
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect();
                text.contains(hint)
            })
            .expect("the Escape hint is drawn");
        let text: String = (area.x..area.right())
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let x = area.x + ratatui::text::Line::raw(&text[..text.find(hint).unwrap()]).width() as u16;
        let action = KanbanView::hit_test(&state, area, x, y).unwrap();
        assert_eq!(action, BoardKey::Esc);
        assert_eq!(
            KanbanView::hit_test(&state, area, x + hint.len() as u16, y),
            None
        );
        let effects = update(&mut state, KanbanInput::Key(action));
        if hint == "Esc close" {
            assert!(effects.contains(&KanbanEffect::Close));
        } else {
            assert!(effects.is_empty());
            assert_eq!(state.mode, BoardMode::Browse);
        }
    }
}

#[test]
fn visible_footer_keys_act_like_the_keyboard_on_the_board_and_in_detail() {
    let mut state = KanbanBoardState::blank();
    state.live = true;
    state.cursor = 25534;
    let area = Rect::new(4, 5, 100, 18);
    let mut buffer = Buffer::empty(area);
    KanbanView::draw(&state, area, &mut buffer);
    let row = |buffer: &Buffer, y| {
        (area.x..area.right())
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    assert!(row(&buffer, area.y).contains("Last event ID: 25534"));
    let y = (area.y..area.bottom())
        .find(|y| row(&buffer, *y).contains("Enter details"))
        .unwrap();
    let text = row(&buffer, y);
    for (label, key) in [
        ("h/l", BoardKey::Char('h')),
        ("l columns", BoardKey::Char('l')),
        ("j/k", BoardKey::Char('j')),
        ("k cards", BoardKey::Char('k')),
        ("Enter details", BoardKey::Enter),
        ("/ filter", BoardKey::Char('/')),
        ("b boards", BoardKey::Char('b')),
    ] {
        let prefix = &text[..text.find(label).unwrap()];
        let x = area.x + ratatui::text::Line::raw(prefix).width() as u16;
        assert_eq!(
            KanbanView::hit_test(&state, area, x, y),
            Some(key),
            "{label}"
        );
    }

    state.detail_id = Some("card".into());
    KanbanView::draw(&state, area, &mut buffer);
    let y = area.bottom() - 1;
    let text = row(&buffer, y);
    let viewport = detail_viewport(&state, area).unwrap();
    for (label, motion) in [
        ("↑", DetailMotion::Up),
        ("↓", DetailMotion::Down),
        ("PgUp", DetailMotion::PageUp),
        ("PgDn", DetailMotion::PageDown),
        ("Home", DetailMotion::Home),
        ("End", DetailMotion::End),
    ] {
        let prefix = &text[..text.find(label).unwrap()];
        let x = area.x + ratatui::text::Line::raw(prefix).width() as u16;
        assert_eq!(
            KanbanView::hit_test(&state, area, x, y),
            Some(BoardKey::ScrollDetail { motion, viewport }),
            "{label}"
        );
    }
}

#[test]
fn selected_card_detail_wraps_original_title_and_description() {
    let mut state = KanbanBoardState::blank();
    let mut card = frame().detail.unwrap();
    card.title =
        "Repair the parser and document the real parser fallback for the next maintainer".into();
    card.latest_summary = Some("Regression test reproduced the issue".into());
    card.body = format!(
        "{}Keep the final recovery step visible.",
        "The parser previously rejected multiline input. ".repeat(6)
    );
    state.detail_id = Some(card.id.clone());
    state.detail_fields = vec![
        "Title: shortened title".into(),
        "Body: shortened body".into(),
        "Workspace: scratch".into(),
    ];
    state.load_cards(vec![card]);
    let area = Rect::new(0, 0, 60, 24);
    let mut buffer = Buffer::empty(area);
    KanbanView::draw(&state, area, &mut buffer);
    let rendered = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("next maintainer"), "{rendered}");
    assert!(
        rendered.contains("Latest result")
            && rendered.contains("Regression test reproduced the issue")
    );
    let wrapped = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        wrapped.contains("Keep the final recovery step visible.")
            && wrapped.contains("Workspace: scratch"),
        "{rendered}"
    );
    assert!(rendered.contains("Esc back to board"));
    assert!(!rendered.contains("shortened"));
}

#[test]
fn detail_scroll_reaches_the_last_word_and_returns_to_the_selected_card() {
    let mut state = KanbanBoardState::blank();
    let mut first = frame().detail.unwrap();
    first.status = "triage".into();
    first.body = (0..150)
        .map(|i| {
            format!("Checkpoint {i:03}: Preserve the full explanation of this task's behavior.\n")
        })
        .collect::<String>();
    first.body.push_str("Last detail word: finishing-token");
    let mut second = first.clone();
    second.id = "second".into();
    second.title = "Next task".into();
    second.body = "A short card".into();
    state.load_cards(vec![first.clone(), second.clone()]);
    let key = |state: &mut KanbanBoardState, key| update(state, KanbanInput::Key(key));
    assert_eq!(
        key(&mut state, BoardKey::Enter),
        [KanbanEffect::Detail(first.id.clone())]
    );
    assert!(
        update(
            &mut state,
            KanbanInput::Detail {
                card: first.clone(),
                fields: vec!["Workspace: scratch".into()]
            }
        )
        .is_empty()
    );

    for width in [36, 60, 100] {
        let area = Rect::new(3, 2, width, 16);
        let viewport = detail_viewport(&state, area).unwrap();
        let scroll = |state: &mut KanbanBoardState, motion| {
            assert!(key(state, BoardKey::ScrollDetail { motion, viewport }).is_empty());
        };
        let render = |state: &KanbanBoardState| {
            let mut buffer = Buffer::empty(area);
            KanbanView::draw(state, area, &mut buffer);
            (area.y..area.bottom())
                .map(|y| {
                    (area.x..area.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        };
        scroll(&mut state, DetailMotion::Home);
        assert!(render(&state).join("\n").contains("Checkpoint 000"));
        assert!(!render(&state).join("\n").contains("finishing-token"));
        scroll(&mut state, DetailMotion::Down);
        assert_eq!(state.detail_scroll(), 1);
        scroll(&mut state, DetailMotion::PageDown);
        assert_eq!(state.detail_scroll(), 1 + viewport.page_rows);
        scroll(&mut state, DetailMotion::PageUp);
        assert_eq!(state.detail_scroll(), 1);
        scroll(&mut state, DetailMotion::Up);
        assert_eq!(state.detail_scroll(), 0);
        scroll(&mut state, DetailMotion::End);
        let end = render(&state);
        assert!(
            end[..=viewport.page_rows]
                .join("\n")
                .contains("finishing-token"),
            "{end:?}"
        );
        assert!(
            end[viewport.page_rows + 1..]
                .join("\n")
                .contains("Esc back to board")
        );
        scroll(&mut state, DetailMotion::Down);
        assert_eq!(state.detail_scroll(), viewport.max_offset);
    }

    assert!(key(&mut state, BoardKey::Char('l')).is_empty());
    assert!(key(&mut state, BoardKey::Esc).is_empty());
    assert_eq!(state.selected().unwrap().id, first.id);
    assert_eq!(state.detail_scroll(), 0);
    assert!(key(&mut state, BoardKey::Char('j')).is_empty());
    assert_eq!(
        key(&mut state, BoardKey::Enter),
        [KanbanEffect::Detail(second.id.clone())]
    );
    assert_eq!(state.detail_scroll(), 0);
    assert!(
        update(
            &mut state,
            KanbanInput::Detail {
                card: first,
                fields: vec!["Late reply".into()]
            }
        )
        .is_empty()
    );
    assert!(state.detail_fields.is_empty());
}

#[test]
fn a_clipped_board_menu_does_not_cover_the_footer_or_write_outside_its_buffer() {
    let style = theme_support::resolve(talaria_app::theme_file::builtin("ember").as_ref())
        .theme
        .views;
    let mut state = KanbanBoardState::blank();
    state.live = true;
    state.notice = Some("There are live updates for this board".into());
    state.mode = BoardMode::Boards {
        slugs: vec!["first".into(), "selected".into()],
        selected: 1,
    };
    let area = Rect::new(4, 3, 25, 8);
    let mut buffer = Buffer::empty(area);
    KanbanView::draw_with_theme(&state, Rect::new(0, 0, 60, 30), &mut buffer, &style);
    let selected = (area.top()..area.bottom())
        .find(|y| buffer[(area.x, *y)].symbol() == ">")
        .expect("selection remains visible");
    assert_eq!(buffer[(area.x, selected)].bg, Color::Rgb(0xff, 0xb8, 0x6b));
    assert!((selected + 1..area.bottom()).any(|y| {
        (area.x..area.x + 10)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            == "Esc cancel"
    }));
    KanbanView::draw_with_theme(&state, Rect::new(0, 0, 0, 15), &mut buffer, &style);
    KanbanView::draw_with_theme(&state, Rect::new(50, 30, 5, 5), &mut buffer, &style);
    assert_eq!(buffer[(area.x, selected)].symbol(), ">");
}

fn render(frame: &KanbanFrame) -> vt100::Parser {
    let sink = ByteSink::default();
    let backend = CrosstermBackend::new(sink.clone());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 120, 36)),
        },
    )
    .expect("terminal");
    terminal
        .draw(|ui| draw_kanban(frame, ui.area(), ui.buffer_mut()))
        .expect("draw");
    let mut parser = vt100::Parser::new(36, 120, 0);
    parser.process(&sink.take());
    parser
}

fn frame() -> KanbanFrame {
    KanbanFrame {
        board: "mock".into(),
        shown: 2,
        total: 2,
        live: "12".into(),
        columns: vec![
            FrameColumn {
                label: "Ready".into(),
                cards: vec![FrameCard {
                    id: "t_ready".into(),
                    title: "fix the parser".into(),
                    assignee: "fixture".into(),
                    status: "ready".into(),
                    age: "1d".into(),
                    selected: true,
                }],
            },
            FrameColumn {
                label: "Blocked".into(),
                cards: vec![FrameCard {
                    id: "t_blocked".into(),
                    title: "waiting input".into(),
                    assignee: "fixture".into(),
                    status: "blocked".into(),
                    age: "1d".into(),
                    selected: false,
                }],
            },
        ],
        status_line: String::new(),
        detail: Some(BoardCard {
            id: "t_ready".into(),
            title: "fix the parser".into(),
            assignee: "fixture".into(),
            status: "ready".into(),
            created_at_ms: 1_792_993_000,
            started_at_ms: None,
            parent_titles: vec![("sort the inbox".into(), "triage".into())],
            child_titles: Vec::new(),
            body: "Repair the parser and leave a summary.".into(),
            latest_summary: None,
            comments: Vec::new(),
        }),
    }
}

#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
