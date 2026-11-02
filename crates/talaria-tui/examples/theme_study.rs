//! Offline screen study. No application startup, transport, or terminal modes.
use std::error::Error;

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

const CATALOGUE: &str = include_str!("../../../docs/design/theme-study/tokens.txt");

#[derive(Clone, Debug)]
struct Token<'a> {
    name: &'a str,
    role: &'a str,
    attributes: &'a str,
}

fn tokens() -> Vec<Token<'static>> {
    CATALOGUE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let parts: Vec<_> = line.split('|').collect();
            assert_eq!(parts.len(), 4);
            Token {
                name: parts[0],
                role: parts[1],
                attributes: parts[2],
            }
        })
        .collect()
}

const ROLES: [&str; 13] = [
    "background",
    "surface",
    "text",
    "muted",
    "disabled",
    "border",
    "primary",
    "accent",
    "success",
    "warning",
    "error",
    "attention",
    "selection",
];

#[derive(Clone, Copy)]
struct Theme {
    name: &'static str,
    colors: [Color; 12],
    ascii: bool,
}

fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn themes() -> [Theme; 3] {
    [
        Theme {
            name: "Ember / dark warm high contrast",
            colors: [
                0x171411, 0x25201b, 0xf5eadb, 0xc4b5a2, 0xa19383, 0x9c846b, 0xffc078, 0xffb86b,
                0xa8d08d, 0xffd166, 0xff8b82, 0xffd166,
            ]
            .map(rgb),
            ascii: false,
        },
        Theme {
            name: "Paper / light",
            colors: [
                0xfffcf5, 0xeee8dc, 0x25231f, 0x665d50, 0x766a58, 0x897a65, 0x714000, 0x005a80,
                0x285c2a, 0x704600, 0xa01824, 0x704600,
            ]
            .map(rgb),
            ascii: false,
        },
        Theme {
            name: "Terminal / ANSI",
            colors: [
                Color::Reset,
                Color::Reset,
                Color::Reset,
                Color::Gray,
                Color::DarkGray,
                Color::Gray,
                Color::Yellow,
                Color::Cyan,
                Color::Green,
                Color::Yellow,
                Color::Red,
                Color::Yellow,
            ],
            ascii: true,
        },
    ]
}

impl Theme {
    fn role(self, name: &str) -> Style {
        match name {
            "background" => Style::new().fg(self.colors[2]).bg(self.colors[0]),
            "surface" => Style::new().fg(self.colors[2]).bg(self.colors[1]),
            "selection" | "focus" if self.ascii => Style::new()
                .fg(Color::Reset)
                .bg(Color::Reset)
                .add_modifier(Modifier::REVERSED),
            "selection" | "focus" => Style::new().fg(self.colors[0]).bg(self.colors[7]),
            _ => {
                let index = ROLES
                    .iter()
                    .position(|role| *role == name)
                    .unwrap_or_else(|| panic!("Unknown role {name}"));
                Style::new().fg(self.colors[index]).bg(self.colors[0])
            }
        }
    }

    fn token(self, name: &str) -> Style {
        let token = tokens()
            .into_iter()
            .find(|token| token.name == name)
            .unwrap_or_else(|| panic!("Unknown token {name}"));
        self.style(&token)
    }

    fn style(self, token: &Token<'_>) -> Style {
        let on_surface = [
            "work.",
            "menu.",
            "dialog.",
            "picker.",
            "list.",
            "queue.",
            "code.",
            "fleet.",
            "agents.",
            "background.",
            "pager.",
            "review.",
            "rewind.",
            "workflow.",
            "cron.",
            "palette.",
            "profiles.",
            "launch.",
            "configure.",
            "doctor.",
            "mention.",
            "editor.",
            "providers.",
            "settings.",
            "context.",
            "alert.",
            "badge.",
        ]
        .iter()
        .any(|prefix| token.name.starts_with(prefix));
        let mut style = self.role(token.role);
        if on_surface && !matches!(token.role, "selection" | "focus" | "background") {
            style = style.bg(self.colors[1]);
        }
        match token.attributes {
            "bold" => style.add_modifier(Modifier::BOLD),
            "italic" => style.add_modifier(Modifier::ITALIC),
            "underline" => style.add_modifier(Modifier::UNDERLINED),
            "strikethrough" => style.add_modifier(Modifier::CROSSED_OUT),
            "" => style,
            other => panic!("Unknown attribute {other}"),
        }
    }
}

struct Screen {
    buffer: Buffer,
    marks: Vec<(String, u16, u16)>,
}

impl Screen {
    fn new(width: u16, height: u16, theme: Theme) -> Self {
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
        buffer.set_style(buffer.area, theme.token("workspace.background"));
        Self {
            buffer,
            marks: Vec::new(),
        }
    }

    fn text(&mut self, x: u16, y: u16, width: u16, value: &str, token: &str, theme: Theme) {
        if y >= self.buffer.area.height || x >= self.buffer.area.width {
            return;
        }
        self.buffer.set_stringn(
            x,
            y,
            value,
            usize::from(width.min(self.buffer.area.width - x)),
            theme.token(token),
        );
        self.marks.push((token.to_owned(), x, y));
    }

    fn fill(&mut self, area: Rect, token: &str, theme: Theme) {
        for y in area.y..area.bottom() {
            self.text(
                area.x,
                y,
                area.width,
                &" ".repeat(usize::from(area.width)),
                token,
                theme,
            );
        }
    }
}

fn workspace(theme: Theme, width: u16, height: u16) -> Screen {
    let mut screen = Screen::new(width, height, theme);
    screen.fill(Rect::new(0, 0, width, 1), "menu.background", theme);
    screen.text(0, 0, 7, "Talaria", "menu.brand", theme);
    screen.text(10, 0, 7, "Session", "menu.item", theme);
    screen.text(20, 0, 5, "Model", "menu.item", theme);
    screen.text(28, 0, 4, "View", "menu.item.focused", theme);
    screen.text(35, 0, 40, "Voice   Settings   Help", "menu.item", theme);
    let footer = height - 5;
    let body = [
        (
            "You: Keep the interface clean and light.",
            "transcript.user.label",
        ),
        ("Talaria", "transcript.assistant.label"),
        (
            "A few overrides can change the whole theme.",
            "transcript.text",
        ),
        ("Theme preview", "transcript.heading.1"),
        (
            "Reasoning: checking readable contrast.",
            "transcript.reasoning",
        ),
        ("Read docs/themes.md", "transcript.link.text"),
        ("rust / sample.rs", "code.label"),
        ("let theme = palette.resolve();", "code.syntax.keyword"),
        ("--- sample.rs", "diff.file"),
        ("- old_color", "diff.removed"),
        ("+ semantic_role", "diff.added"),
        ("ok render: six example workspace frames", "tool.success"),
        ("! Preview only; decision pending.", "alert.warning"),
    ];
    for (offset, (text, token)) in body.iter().enumerate() {
        screen.text(1, 2 + offset as u16, width - 2, text, token, theme);
    }
    let panel_x = width - 33;
    screen.fill(
        Rect::new(panel_x, 1, 33, footer - 1),
        "work.background",
        theme,
    );
    let divider = if theme.ascii { "|" } else { "│" };
    for y in 1..footer {
        screen.text(
            panel_x,
            y,
            1,
            divider,
            if width >= 112 && height >= 28 {
                "workspace.divider"
            } else {
                "workspace.overlay.border"
            },
            theme,
        );
    }
    let panel = [
        ("Work / 1 needs you", "work.attention"),
        ("Goal: keep the interface light", "work.goal"),
        ("Turn: running / compare themes", "work.turn"),
        ("Verdict: pending", "work.verdict.pending"),
        ("Tasks 1/3", "work.section"),
        ("[x] Read inputs", "work.task.done"),
        ("[>] Compare themes", "work.task.running"),
        ("[ ] Review screens", "work.task.pending"),
        ("Agents", "work.section"),
        ("! review: needs you", "work.agent.waiting"),
        ("  model: unknown / effort: ?", "work.agent.unknown"),
        ("> preview: rendering", "work.agent.running"),
        ("  model: unknown / effort: ?", "work.agent.unknown"),
        ("- next: queued", "work.agent.queued"),
        ("ok inventory: done", "work.agent.done"),
        ("+2 more", "work.overflow"),
        ("Alt+S details / Alt+A next", "work.help"),
    ];
    for (offset, (text, token)) in panel.iter().enumerate() {
        let y = 1 + offset as u16;
        if y < footer {
            screen.text(panel_x + 1, y, 32, text, token, theme);
        }
    }
    screen.fill(Rect::new(0, footer, width, 1), "queue.background", theme);
    screen.text(
        0,
        footer,
        width,
        " Queued 1: compare the light variant",
        "queue.text",
        theme,
    );
    let (tl, tr, bl, br, horizontal, vertical) = if theme.ascii {
        ("+", "+", "+", "+", "-", "|")
    } else {
        ("╭", "╮", "╰", "╯", "─", "│")
    };
    for (y, left, right) in [(footer + 1, tl, tr), (footer + 3, bl, br)] {
        let rule = format!("{left}{}{right}", horizontal.repeat(usize::from(width - 2)));
        screen.text(0, y, width, &rule, "composer.border.focused", theme);
    }
    screen.text(0, footer + 2, 1, vertical, "composer.border.focused", theme);
    screen.text(
        width - 1,
        footer + 2,
        1,
        vertical,
        "composer.border.focused",
        theme,
    );
    screen.text(2, footer + 2, 1, ">", "composer.marker", theme);
    screen.text(
        4,
        footer + 2,
        width - 6,
        "Keep the defaults quiet.",
        "composer.text",
        theme,
    );
    screen.text(
        0,
        height - 1,
        width,
        " Model unknown / effort ? | 1 needs you | IDLE +read off",
        "status.text",
        theme,
    );
    screen.text(28, height - 1, 11, "1 needs you", "status.attention", theme);
    screen.text(42, height - 1, 4, "IDLE", "status.voice.phase", theme);
    screen
}

fn atlas(theme: Theme, width: u16, height: u16) -> Screen {
    let mut screen = Screen::new(width, height, theme);
    let columns = usize::from(width / 5);
    assert!(tokens().len() <= columns * usize::from(height));
    for (index, token) in tokens().iter().enumerate() {
        screen.text(
            ((index % columns) * 5) as u16,
            (index / columns) as u16,
            4,
            &format!("{:04}", index + 1),
            token.name,
            theme,
        );
    }
    screen
}

fn capture(screen: &Screen) -> Result<Buffer, Box<dyn Error>> {
    let area = screen.buffer.area;
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))?;
    terminal.draw(|frame| frame.buffer_mut().clone_from(&screen.buffer))?;
    let result = terminal.backend().buffer().clone();
    assert_eq!(result, screen.buffer);
    Ok(result)
}

fn frame_markdown(buffer: &Buffer) -> String {
    let mut output = String::from("```text\n");
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        output.push_str(row.trim_end());
        output.push('\n');
    }
    output.push_str("```\n\n");
    output
}

fn color_label(color: Option<Color>) -> String {
    match color {
        Some(Color::Rgb(r, g, b)) => format!("#{r:02x}{g:02x}{b:02x}"),
        Some(Color::Reset) | None => "default".to_owned(),
        Some(other) => format!("{other:?}"),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut output = String::from("## Offline rendered screens and style callouts\n\n");
    output.push_str("All frames below come from `ratatui::backend::TestBackend`, not a terminal or gateway. Coordinates are zero-based (column,row). Blank trailing cells are omitted from text, but buffers retain their full dimensions. Plain Markdown cannot display cell colors: the callouts and swatch identifiers preserve their exact mapping.\n\n");
    for theme in themes() {
        output.push_str(&format!("### {}\n\n", theme.name));
        output.push_str("Role colors below are the actual preview colors. Element backgrounds inherit their area: background for transcript/composer/status/tool/diff; surface for menu/work/dialog/picker/lists/code/queue/alerts/badges and alternate views. Selection and focus use the accent background and background foreground, or terminal reverse in Terminal. Attributes come from the catalogue.\n\n| Role | Foreground | Base background |\n|---|---|---|\n");
        for role in ROLES.into_iter().chain(["focus"]) {
            let style = theme.role(role);
            output.push_str(&format!(
                "| {role} | {} | {} |\n",
                color_label(style.fg),
                color_label(style.bg)
            ));
        }
        output.push('\n');
        for (width, height) in [(80, 24), (120, 40)] {
            let screen = workspace(theme, width, height);
            output.push_str(&format!(
                "#### {width}x{height}: menu on, panel {}\n\n",
                if width < 112 {
                    "opened on demand as overlay"
                } else {
                    "docked"
                }
            ));
            output.push_str(&frame_markdown(&capture(&screen)?));
            output.push_str("Callouts: ");
            let mut seen = std::collections::BTreeSet::new();
            for (token, x, y) in &screen.marks {
                if [
                    "menu.brand",
                    "menu.item.focused",
                    "transcript.text",
                    "transcript.reasoning",
                    "diff.removed",
                    "diff.added",
                    "workspace.divider",
                    "workspace.overlay.border",
                    "work.attention",
                    "work.verdict.pending",
                    "queue.text",
                    "composer.marker",
                    "composer.border.focused",
                    "status.voice.phase",
                ]
                .contains(&token.as_str())
                    && seen.insert(token)
                {
                    output.push_str(&format!("`{token}` ({x},{y}); "));
                }
            }
            output.push_str("styles resolve through the catalogue and role table above.\n\n");
            output.push_str(&format!(
                "#### {width}x{height}: complete token swatch atlas\n\n"
            ));
            output.push_str(&frame_markdown(&capture(&atlas(theme, width, height))?));
            output.push_str(&format!("Every catalogue token is drawn with its resolved foreground, background and attributes. For token identifier n (one-based), its first cell is x = 5 × ((n − 1) modulo {}), y = floor((n − 1) / {}). Each identifier occupies four cells. This includes hidden dialogs, pickers and alternate views without pretending they appear simultaneously in the workspace.\n\n", width / 5, width / 5));
        }
    }
    std::fs::write(
        "docs/design/theme-study/screens.md",
        format!("{}\n", output.trim_end()),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_token_has_a_unique_name_and_reaches_a_styled_backend_cell()
    -> Result<(), Box<dyn Error>> {
        let catalogue = tokens();
        let names: std::collections::BTreeSet<_> = catalogue.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), catalogue.len());
        for theme in themes() {
            for (width, height) in [(80, 24), (120, 40)] {
                let screen = capture(&atlas(theme, width, height))?;
                for (index, token) in catalogue.iter().enumerate() {
                    let columns = usize::from(width / 5);
                    let cell = &screen[(((index % columns) * 5) as u16, (index / columns) as u16)];
                    let expected = theme.style(token);
                    assert_eq!(
                        cell.fg,
                        expected.fg.unwrap_or(Color::Reset),
                        "{}",
                        token.name
                    );
                    assert_eq!(
                        cell.bg,
                        expected.bg.unwrap_or(Color::Reset),
                        "{}",
                        token.name
                    );
                    assert_eq!(cell.modifier, expected.add_modifier, "{}", token.name);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn locked_layout_and_explicit_verdict_survive_both_sizes() -> Result<(), Box<dyn Error>> {
        for theme in themes() {
            for (width, height) in [(80, 24), (120, 40)] {
                let screen = capture(&workspace(theme, width, height))?;
                let x = width - 33;
                assert_eq!(screen[(x, 1)].symbol(), if theme.ascii { "|" } else { "│" });
                let row: String = (x + 1..width)
                    .map(|col| screen[(col, 4)].symbol())
                    .collect();
                assert!(row.starts_with("Verdict: pending"));
                let text = frame_markdown(&screen);
                assert_eq!(text.matches("Tasks 1/3").count(), 1);
                assert_eq!(text.lines().count(), usize::from(height) + 3);
                assert!(text.contains("+2 more"));
                assert!(text.contains("IDLE +read off"));
            }
        }
        Ok(())
    }
}
