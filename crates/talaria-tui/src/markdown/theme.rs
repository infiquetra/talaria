use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Debug)]
pub struct MarkdownTheme {
    pub heading1: Style,
    pub heading2: Style,
    pub heading3: Style,
    pub strong: Style,
    pub emphasis: Style,
    pub strikethrough: Style,
    pub inline_code: Style,
    pub link_text: Style,
    pub link_destination: Style,
    pub image_alt: Style,
    pub quote_bar: Style,
    pub rule: Style,
    pub code_label: Style,
    pub table_border: Style,
    pub table_header: Style,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            heading1: Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            heading2: Style::default().add_modifier(Modifier::BOLD),
            heading3: Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
            strong: Style::default().add_modifier(Modifier::BOLD),
            emphasis: Style::default().add_modifier(Modifier::ITALIC),
            strikethrough: Style::default().add_modifier(Modifier::CROSSED_OUT),
            inline_code: Style::default().fg(Color::Cyan),
            link_text: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            link_destination: Style::default().add_modifier(Modifier::DIM),
            image_alt: Style::default().add_modifier(Modifier::ITALIC),
            quote_bar: Style::default().add_modifier(Modifier::DIM),
            rule: Style::default().add_modifier(Modifier::DIM),
            code_label: Style::default().add_modifier(Modifier::DIM),
            table_border: Style::default().add_modifier(Modifier::DIM),
            table_header: Style::default().add_modifier(Modifier::BOLD),
        }
    }
}
