//! Colour parsing, contrast and palette mapping. Behavior contract: issue 227.

use talaria_tui::theme::color::{Rgb, ensure_contrast, parse_color, to_ansi256};

#[test]
fn command_reply_respects_the_resolved_color_policy_and_palette() {
    use ratatui::style::{Color, Style};
    use talaria_core::automation::HermesCommandCell;
    use talaria_core::session::Cell;
    use talaria_tui::history::CellLines;
    use talaria_tui::theme::cells::ThemedCells;
    use talaria_tui::theme::{
        ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
        ThemeInputs, resolve,
    };

    let command = Cell::HermesCommand(HermesCommandCell {
        name: "test".into(),
        typed: "/test".into(),
        body: "Warning `code` and [link](https://example.com)".into(),
        warning: true,
    });
    for source in [DepthSource::NoColorEnv, DepthSource::Config] {
        let theme = resolve(&ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: false,
            polarity: (Polarity::Dark, PolaritySource::Config),
            depth: (ColorDepth::None, source),
            motion: (Motion::Full, MotionSource::Default),
            terminal: TerminalColors::default(),
        });
        let lines = ThemedCells { theme: &theme }.lines(&command, 80);
        for line in &lines {
            assert!(
                line.style.fg.is_none()
                    && line.style.bg.is_none()
                    && line.style.underline_color.is_none()
            );
            for span in &line.spans {
                assert!(
                    span.style.fg.is_none()
                        && span.style.bg.is_none()
                        && span.style.underline_color.is_none(),
                    "{:?}",
                    span
                );
            }
        }
    }
    let mut theme = resolve(&ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: false,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    });
    theme.markdown.inline_code = Style::default().fg(Color::Magenta);
    let lines = ThemedCells { theme: &theme }.lines(&command, 80);
    assert!(
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| span.content.contains("code") && span.style.fg == Some(Color::Magenta))
    );
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

#[test]
fn parse_color_accepts_hermes_forms_only() {
    let pale = rgb(0xaa, 0xbb, 0xcc);
    assert_eq!(parse_color("#abc"), Some(pale));
    assert_eq!(parse_color("abc"), Some(pale));
    assert_eq!(parse_color("#A1B2C3"), Some(rgb(0xa1, 0xb2, 0xc3)));
    assert_eq!(parse_color("FFD700"), Some(rgb(0xff, 0xd7, 0x00)));
    assert_eq!(parse_color(" #abcdef "), Some(rgb(0xab, 0xcd, 0xef)));

    let dark = rgb(1, 2, 3);
    assert_eq!(parse_color("rgb(1, 2, 3)"), Some(dark));
    assert_eq!(parse_color("RGB(1,2,3)"), Some(dark));
    assert_eq!(parse_color("rgb( 1 ,2 , 3 )"), Some(dark));
    assert_eq!(parse_color("rgba(1, 2, 3, 0.5)"), Some(dark));
    assert_eq!(parse_color("rgb(1,2,3) trailing"), Some(dark));
    assert_eq!(parse_color("rgb(1, 2, 3"), Some(dark));
    assert_eq!(parse_color("rgb(300, 0, 0)"), Some(rgb(255, 0, 0)));

    for text in [
        "red",
        "ansi256(3)",
        "#abcd",
        "",
        "rgb(1,2)",
        "#ggg",
        "rgb (1,2,3)",
    ] {
        assert_eq!(parse_color(text), None, "{text:?} is not a colour");
    }
}

#[test]
fn ensure_contrast_lifts_only_failing_colours() {
    assert_eq!(
        ensure_contrast(rgb(0xff, 0xf8, 0xdc), rgb(0xff, 0xff, 0xff), 1.18),
        rgb(0xe5, 0xdf, 0xc6)
    );
    assert_eq!(
        ensure_contrast(rgb(0x20, 0x20, 0x20), rgb(0x10, 0x10, 0x14), 1.45),
        rgb(0x37, 0x37, 0x37)
    );
    let kept = rgb(0xc0, 0xc0, 0xc0);
    assert_eq!(
        ensure_contrast(kept, rgb(0x1e, 0x1e, 0x1e), 1.45),
        kept,
        "above the floor stays byte-identical"
    );
}

#[test]
fn ansi256_picks_nearest_index() {
    assert_eq!(to_ansi256(rgb(0, 0, 0)), 16);
    assert_eq!(to_ansi256(rgb(255, 255, 255)), 231);
    assert_eq!(to_ansi256(rgb(0x5f, 0x87, 0xaf)), 67);
    assert_eq!(to_ansi256(rgb(0x80, 0x80, 0x80)), 244);
}

#[test]
fn inline_user_and_notice_use_theme_colours() {
    use ratatui::style::{Color, Style};
    use std::collections::{BTreeMap, BTreeSet};
    use talaria_app::theme_file::{Base, Chrome, ColorValue, StyleFields, ThemeFile};
    use talaria_core::session::{Cell, CellId, NoticeCell, NoticeKind, ReasoningCell, UserCell};
    use talaria_tui::history::CellLines;
    use talaria_tui::theme::cells::ThemedCells;
    use talaria_tui::theme::{
        ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
        ThemeInputs, resolve_with_theme_file,
    };

    let inputs = ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: false,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    };

    let element_override = |fg: [u8; 3], bg: [u8; 3]| StyleFields {
        fg: Some(ColorValue::Rgb(fg[0], fg[1], fg[2])),
        bg: Some(ColorValue::Rgb(bg[0], bg[1], bg[2])),
        underline_color: None,
        bold: None,
        dim: None,
        italic: None,
        underline: None,
        reverse: None,
        strikethrough: None,
    };

    let mut elements = BTreeMap::new();
    elements.insert(
        "transcript.user.text".to_owned(),
        element_override([17, 34, 51], [1, 2, 3]),
    );
    elements.insert(
        "transcript.notice".to_owned(),
        element_override([68, 85, 102], [4, 5, 6]),
    );
    elements.insert(
        "alert.error".to_owned(),
        element_override([170, 0, 17], [7, 8, 9]),
    );
    elements.insert(
        "transcript.reasoning.label".to_owned(),
        element_override([16, 192, 32], [9, 8, 7]),
    );
    elements.insert(
        "transcript.user.label".to_owned(),
        element_override([255, 0, 255], [3, 2, 1]),
    );
    elements.insert(
        "transcript.assistant.label".to_owned(),
        element_override([0, 255, 255], [2, 3, 4]),
    );
    elements.insert(
        "transcript.system".to_owned(),
        element_override([255, 0, 128], [5, 4, 3]),
    );

    let file = ThemeFile {
        name: None,
        extends: Base::Dark,
        follow_hermes_skin: false,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        elements,
        chrome: Chrome::default(),
    };

    let mut resolution = resolve_with_theme_file(&inputs, Some(&file));
    let reasoning_fg = Color::Rgb(196, 163, 90);
    resolution.theme.reasoning = Style::default().fg(reasoning_fg);

    let user_text_style = resolution.elements["transcript.user.text"];
    let notice_style = resolution.elements["transcript.notice"];
    let error_style = resolution.elements["alert.error"];
    let reasoning_label_style = resolution.elements["transcript.reasoning.label"];
    let reserved_fgs = [
        resolution.elements["transcript.user.label"].fg.unwrap(),
        resolution.elements["transcript.assistant.label"]
            .fg
            .unwrap(),
        resolution.elements["transcript.system"].fg.unwrap(),
    ];

    let all_fgs = [
        user_text_style.fg.unwrap(),
        notice_style.fg.unwrap(),
        error_style.fg.unwrap(),
        reasoning_label_style.fg.unwrap(),
        reserved_fgs[0],
        reserved_fgs[1],
        reserved_fgs[2],
    ];
    let distinct: BTreeSet<_> = all_fgs.iter().map(|c| format!("{c:?}")).collect();
    assert_eq!(distinct.len(), 7, "all seven foregrounds must be distinct");
    assert!(!all_fgs.contains(&reasoning_fg));

    let drawer = ThemedCells {
        theme: &resolution.theme,
    };

    let painted_styles = |cell: &Cell| -> Vec<(String, Vec<Style>)> {
        drawer
            .lines(cell, 80)
            .into_iter()
            .map(|line| {
                let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                let styles = line
                    .spans
                    .iter()
                    .filter(|s| !s.content.is_empty())
                    .map(|s| {
                        let painted = line.style.patch(s.style);
                        assert_eq!(painted.bg, None, "inline cells must paint no background");
                        assert!(
                            !painted.fg.is_some_and(|fg| reserved_fgs.contains(&fg)),
                            "reserved element foreground must not be painted: {painted:?}"
                        );
                        painted
                    })
                    .collect();
                (text, styles)
            })
            .collect()
    };

    let user = Cell::User(UserCell {
        id: CellId::new(1),
        text: "hello".to_owned(),
    });
    let user_lines = painted_styles(&user);
    assert_eq!(user_lines.len(), 1);
    assert_eq!(user_lines[0].0, "> hello");
    for style in &user_lines[0].1 {
        assert_eq!(style.fg, user_text_style.fg);
        assert_eq!(style.add_modifier, user_text_style.add_modifier);
    }

    let non_error_notices = [
        (2, NoticeKind::Warning, "warning"),
        (3, NoticeKind::Interrupted, "interrupted"),
        (4, NoticeKind::Automation, "automation"),
        (5, NoticeKind::Diagnostic, "diagnostic"),
        (6, NoticeKind::System, "system"),
    ];
    for (id, kind, text) in non_error_notices {
        let cell = Cell::Notice(NoticeCell {
            id: CellId::new(id),
            kind,
            text: text.to_owned(),
            detail: None,
        });
        let lines = painted_styles(&cell);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, format!("\u{b7} {text}"));
        for style in &lines[0].1 {
            assert_eq!(style.fg, notice_style.fg);
            assert_eq!(style.add_modifier, notice_style.add_modifier);
            assert_ne!(style.fg, error_style.fg);
        }
    }

    let error_cell = Cell::Notice(NoticeCell {
        id: CellId::new(7),
        kind: NoticeKind::Error,
        text: "broke".to_owned(),
        detail: None,
    });
    let error_lines = painted_styles(&error_cell);
    assert_eq!(error_lines.len(), 1);
    assert_eq!(error_lines[0].0, "\u{b7} broke");
    for style in &error_lines[0].1 {
        assert_eq!(style.fg, error_style.fg);
        assert_eq!(style.add_modifier, error_style.add_modifier);
        assert_ne!(style.fg, notice_style.fg);
    }

    let reasoning = Cell::Reasoning(ReasoningCell {
        id: CellId::new(8),
        text: "the route".to_owned(),
        sealed: true,
    });
    let reasoning_lines = painted_styles(&reasoning);
    assert!(reasoning_lines.len() >= 2);
    assert_eq!(reasoning_lines[0].0, "∴ thinking");
    for style in &reasoning_lines[0].1 {
        assert_eq!(style.fg, reasoning_label_style.fg);
        assert_eq!(style.add_modifier, reasoning_label_style.add_modifier);
        assert_ne!(style.fg, Some(reasoning_fg));
    }
    let body_line = reasoning_lines[1..]
        .iter()
        .find(|(text, _)| text.contains("the route"))
        .expect("reasoning body must contain 'the route'");
    for style in &body_line.1 {
        assert_eq!(style.fg, Some(reasoning_fg));
    }
}
