//! Synthetic theme captures using the application's renderers.
use crate::{
    checklist::ChecklistStyle,
    composer::Composer,
    diff_view::{DiffRenderOptions, diff_lines},
    keys::KeyMap,
    live::LiveSection,
    markdown::{self, MarkdownOptions},
    picker_frame::draw_picker_frame_with_theme,
    status::{StatusInputs, StatusSection},
    theme::{
        self, ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource,
        TerminalColors, ThemeInputs,
    },
    tool_cell::{Detail, ToolRenderOptions, tool_lines},
    workspace::panel,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
};
use std::collections::HashSet;
use talaria_core::{
    appearance::Rgb,
    models::PickerView,
    session::{CellId, ToolCell, ToolStatus},
};

/// One synthetic capture, including its true-colour terminal recording.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeScreenFrame {
    pub theme_id: &'static str,
    pub width: u16,
    pub height: u16,
    pub inline: bool,
    pub terminal_background: Option<Rgb>,
    pub text: String,
    pub code_colours: usize,
    pub paints_background: bool,
    pub ansi: String,
}

/// Draw every requested synthetic screen.
#[must_use]
pub fn frames() -> Vec<ThemeScreenFrame> {
    markdown::warm_up();
    for _ in 0..500 {
        if markdown::highlight::is_loaded() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut frames = Vec::new();
    for id in ["terminal", "ember", "paper"] {
        for (width, height) in [(80, 24), (120, 40)] {
            frames.push(render(id, width, height, false, None));
        }
    }
    for id in ["ember", "paper"] {
        for background in [Rgb(255, 255, 255), Rgb(16, 16, 20)] {
            frames.push(render(id, 80, 24, true, Some(background)));
        }
    }
    frames
}

fn render(
    id: &'static str,
    width: u16,
    height: u16,
    inline: bool,
    background: Option<Rgb>,
) -> ThemeScreenFrame {
    let file = talaria_app::theme_file::builtin(id).expect("built-in theme parses");
    let inputs = ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors {
            background,
            foreground: None,
        },
    };
    let resolved = theme::resolve_with_theme_file(&inputs, Some(&file));
    let theme = &resolved.theme;
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    let left = if inline { width } else { width - 40 };
    let options = MarkdownOptions {
        width: left,
        theme: theme.markdown.clone(),
        color: true,
        base_style: Style::default(),
    };
    let source = "# Theme preview\n\n- one plain item\n\n[readable notes](guide)\n\n> quoted remark\n\n| swatch | sample |\n|---|---|\n| swatch cell | sample cell |\n\n```rust\nlet colour = 1;\n```";
    let lines = markdown::render_markdown_with_theme(source, &options, &theme.views);
    let mut y = 0u16;
    let mut code_colours = HashSet::new();
    for line in lines.iter().filter(|line| {
        line.spans
            .iter()
            .any(|span| !span.content.trim().is_empty())
    }) {
        if y >= height.saturating_sub(7) {
            break;
        }
        if line
            .spans
            .iter()
            .any(|span| span.content.contains("colour"))
        {
            for span in &line.spans {
                if !span.content.trim().is_empty() {
                    code_colours.insert(format!("{:?}", span.style.fg));
                }
            }
        }
        buffer.set_line(0, y, line, left);
        y += 1;
    }
    buffer.set_line(
        0,
        y,
        &Line::styled("checking contrast", theme.reasoning),
        left,
    );
    y += 1;
    for (index, name, status) in [
        (1, "running lookup", ToolStatus::Running),
        (2, "finished lookup", ToolStatus::Succeeded),
    ] {
        let cell = ToolCell {
            id: CellId::new(index),
            tool_id: format!("call-{index}"),
            name: name.into(),
            context: None,
            args: None,
            labels: vec![serde_json::json!({"text": name})],
            status,
            progress: None,
            summary: None,
            result: None,
            diff: None,
            duration_s: None,
        };
        let options = ToolRenderOptions {
            width: left,
            theme: theme.tool.clone(),
            color: true,
            detail: Detail::Compact,
            expand_hint: None,
            elapsed: None,
        };
        for line in tool_lines(&cell, &options).iter().take(1) {
            buffer.set_line(0, y, line, left);
            y += 1;
        }
    }
    let diff = talaria_core::diff::parse_diff(
        "--- old\n+++ new\n@@ -1 +1 @@\n-dropped line\n+kept line\n",
    );
    let (lines, _) = diff_lines(
        &diff,
        &DiffRenderOptions {
            width: left,
            indent: 0,
            max_rows: None,
            theme: theme.tool.clone(),
            color: true,
        },
    );
    for line in lines.iter().filter(|line| {
        line.spans
            .iter()
            .any(|span| span.content.contains("dropped line") || span.content.contains("kept line"))
    }) {
        buffer.set_line(0, y, line, left);
        y += 1;
    }
    buffer.set_string(
        0,
        y,
        "queued prompt",
        theme
            .views
            .inline(crate::theme::surfaces::tokens::QUEUE_TEXT, Style::default()),
    );
    let mut composer = Composer::new(KeyMap::defaults());
    composer.load("quiet composer".into());
    composer.set_style(theme.composer);
    composer
        .section(left, 3)
        .render(Rect::new(0, height - 4, left, 3), &mut buffer);
    let status = StatusInputs {
        model: Some("synthetic status".into()),
        ..StatusInputs::default()
    };
    StatusSection::new(&status, None, &theme.status)
        .render_into(Rect::new(0, height - 1, left, 1), &mut buffer);
    if !inline {
        panel::draw_with_theme(
            Rect::new(left, 0, 40, height - 10),
            &mut buffer,
            None,
            false,
            &ChecklistStyle::default(),
            None,
            &theme.views,
        );
        let picker = PickerView {
            browse: None,
            title: "Choose a model",
            filter: "",
            effort: "",
            hints: "Esc closes",
            refreshing: false,
            rows: &[],
            selected: None,
            banner: None,
        };
        draw_picker_frame_with_theme(
            &picker,
            Rect::new(left, height - 10, 40, 10),
            &mut buffer,
            &theme.views,
        );
    }
    theme.views.finish(
        Rect::new(0, 0, left, height),
        &mut buffer,
        crate::theme::surfaces::tokens::TRANSCRIPT_BACKGROUND,
        !inline,
    );
    let mut text = String::new();
    let mut ansi = String::new();
    for row in 0..height {
        for col in 0..width {
            let cell = &buffer[(col, row)];
            text.push_str(cell.symbol());
            crate::terminal::push_sgr(&mut ansi, "0");
            if let Some(bg) = background {
                crate::terminal::push_sgr(&mut ansi, &format!("48;2;{};{};{}", bg.0, bg.1, bg.2));
            }
            sgr(&mut ansi, cell.fg, false);
            sgr(&mut ansi, cell.bg, true);
            if cell.modifier.contains(Modifier::BOLD) {
                crate::terminal::push_sgr(&mut ansi, "1");
            }
            if cell.modifier.contains(Modifier::ITALIC) {
                crate::terminal::push_sgr(&mut ansi, "3");
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                crate::terminal::push_sgr(&mut ansi, "4");
            }
            if cell.modifier.contains(Modifier::REVERSED) {
                crate::terminal::push_sgr(&mut ansi, "7");
            }
            ansi.push_str(cell.symbol());
        }
        text.push('\n');
        crate::terminal::push_sgr(&mut ansi, "0");
        ansi.push('\n');
    }
    ThemeScreenFrame {
        theme_id: id,
        width,
        height,
        inline,
        terminal_background: background,
        text,
        code_colours: code_colours.len(),
        paints_background: buffer[(0, 0)].bg != Color::Reset,
        ansi,
    }
}

fn sgr(out: &mut String, color: Color, background: bool) {
    let prefix = if background { 48 } else { 38 };
    match color {
        Color::Rgb(r, g, b) => {
            crate::terminal::push_sgr(out, &format!("{prefix};2;{r};{g};{b}"));
        }
        Color::Indexed(n) => {
            crate::terminal::push_sgr(out, &format!("{prefix};5;{n}"));
        }
        Color::Reset => {}
        named => {
            let n = match named {
                Color::Black => 0,
                Color::Red => 1,
                Color::Green => 2,
                Color::Yellow => 3,
                Color::Blue => 4,
                Color::Magenta => 5,
                Color::Cyan => 6,
                Color::Gray => 7,
                Color::DarkGray => 8,
                Color::LightRed => 9,
                Color::LightGreen => 10,
                Color::LightYellow => 11,
                Color::LightBlue => 12,
                Color::LightMagenta => 13,
                Color::LightCyan => 14,
                Color::White => 15,
                _ => return,
            };
            crate::terminal::push_sgr(out, &format!("{prefix};5;{n}"));
        }
    }
}
