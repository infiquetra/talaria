use ratatui::style::Style;

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
        crate::theme::legacy::markdown_theme()
    }
}
