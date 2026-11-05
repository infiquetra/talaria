//! Original view palettes, retained for the no-file path.
use crate::{
    automation::AutomationTheme, background::BackgroundTheme, checklist::ChecklistStyle,
    context_view::ContextStyle, markdown::MarkdownTheme, pager::PagerTheme, side::SideTheme,
    status::StatusPalette, subagents::dock::SubagentStyle, tool_cell::ToolTheme,
    workflow_view::WorkflowTheme,
};
use ratatui::style::{Color, Modifier, Style};

pub(crate) fn automation_theme() -> AutomationTheme {
    AutomationTheme {
        accent: Style::default().fg(Color::Cyan),
        warning: Style::default().fg(Color::Yellow),
        dim: Style::default().add_modifier(Modifier::DIM),
    }
}

pub(crate) fn background_theme() -> BackgroundTheme {
    BackgroundTheme {
        dim: Style::default().add_modifier(Modifier::DIM),
        warning: Style::default().fg(Color::Yellow),
        error: Style::default().fg(Color::Red),
        ok: Style::default().fg(Color::Green),
        selected: Style::default().add_modifier(Modifier::REVERSED),
    }
}

pub(crate) fn checklist_style() -> ChecklistStyle {
    ChecklistStyle {
        header: Style::default().add_modifier(Modifier::BOLD),
        hint: Style::default().add_modifier(Modifier::DIM),
        in_progress: Style::default().add_modifier(Modifier::BOLD),
        pending: Style::default(),
        completed: Style::default().add_modifier(Modifier::DIM),
        cancelled: Style::default().add_modifier(Modifier::DIM),
        glyph_in_progress: Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        glyph_completed: Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::DIM),
    }
}

pub(crate) fn context_style() -> ContextStyle {
    ContextStyle {
        header: Style::default().add_modifier(Modifier::BOLD),
        dim: Style::default().add_modifier(Modifier::DIM),
        error: Style::default().fg(Color::Red),
        tick: Style::default().fg(Color::Green),
        categories: [
            Style::default().fg(Color::Blue),
            Style::default().fg(Color::Magenta),
            Style::default().fg(Color::Yellow),
            Style::default().fg(Color::Cyan),
            Style::default().fg(Color::LightMagenta),
            Style::default().fg(Color::LightBlue),
            Style::default().fg(Color::Green),
            Style::default().fg(Color::LightCyan),
            Style::default().fg(Color::White),
        ],
        free: Style::default().add_modifier(Modifier::DIM),
    }
}

pub(crate) fn pager_theme() -> PagerTheme {
    let reversed = Style::default().add_modifier(Modifier::REVERSED);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    PagerTheme {
        header: reversed,
        footer: reversed,
        banner: bold,
        user: bold,
        dim: Style::default().add_modifier(Modifier::DIM),
        tool_ok: Style::default().fg(Color::Green),
        tool_failed: Style::default().fg(Color::Red),
        diff_add: Style::default().fg(Color::Green),
        diff_del: Style::default().fg(Color::Red),
        search_match: reversed,
        search_current: reversed.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        gutter_mark: bold,
    }
}

pub(crate) fn side_theme() -> SideTheme {
    SideTheme {
        warning: Style::default().fg(Color::Yellow),
        error: Style::default().fg(Color::Red),
        dim: Style::default().add_modifier(Modifier::DIM),
    }
}

pub(crate) fn status_palette() -> StatusPalette {
    StatusPalette {
        border: None,
        accent: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        styles: [
            Style::new(),
            Style::new().add_modifier(Modifier::DIM),
            Style::new().add_modifier(Modifier::DIM),
            Style::new().fg(Color::Green),
            Style::new().fg(Color::Yellow),
            Style::new().fg(Color::Red),
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ],
    }
}

pub(crate) fn subagent_style() -> SubagentStyle {
    SubagentStyle {
        header: Style::new().bold(),
        hint: Style::new().dim(),
        running: Style::new().fg(Color::Cyan),
        queued: Style::new().dim(),
        completed: Style::new().fg(Color::Green),
        failed: Style::new().fg(Color::Red),
        interrupted: Style::new().fg(Color::Yellow),
        gone: Style::new().dim(),
    }
}

pub(crate) fn tool_theme() -> ToolTheme {
    let green = Style::default().fg(Color::Green);
    let red = Style::default().fg(Color::Red);
    let dim = Style::default().add_modifier(Modifier::DIM);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    ToolTheme {
        glyph_running: dim,
        glyph_ok: green,
        glyph_failed: red,
        glyph_muted: dim,
        title: bold,
        subject: Style::default(),
        meta: dim,
        output: Style::default(),
        error: red,
        diff_added: green,
        diff_removed: red,
        diff_context: Style::default(),
        diff_hunk: Style::default().fg(Color::Cyan),
        diff_file: bold,
        diff_note: dim,
        line_number: dim,
    }
}

pub(crate) fn workflow_theme() -> WorkflowTheme {
    WorkflowTheme {
        normal: Style::default(),
        selected: Style::default().bg(Color::DarkGray),
        muted: Style::default().fg(Color::Gray),
        warning: Style::default().fg(Color::Yellow),
    }
}

pub(crate) fn markdown_theme() -> MarkdownTheme {
    MarkdownTheme {
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

use crate::theme::surfaces::tokens;
use talaria_core::review::LabelTone;

pub(crate) fn rewind_theme(color: bool) -> crate::rewind::RewindTheme {
    crate::rewind::RewindTheme {
        selected: Style::default().add_modifier(Modifier::REVERSED),
        dim: Style::default().add_modifier(Modifier::DIM),
        warning: if color {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        },
        accent: if color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        },
    }
}

pub(crate) fn pr_label_style(tone: LabelTone) -> Style {
    match tone {
        LabelTone::Success => Style::default().fg(Color::Green),
        LabelTone::Error => Style::default().fg(Color::Red),
        LabelTone::Warning => Style::default().fg(Color::Yellow),
        LabelTone::Muted => Style::default().add_modifier(Modifier::DIM),
        LabelTone::Accent => Style::default().fg(Color::Magenta),
    }
}
pub(crate) fn cron_accent() -> Style {
    Style::default().fg(Color::Cyan)
}
pub(crate) fn cron_warning() -> Style {
    Style::default().fg(Color::Yellow)
}
pub(crate) fn cron_error() -> Style {
    Style::default().fg(Color::Red)
}
pub(crate) fn cron_hint() -> Style {
    Style::default().fg(Color::DarkGray)
}
pub(crate) fn cron_selected() -> Style {
    Style::default().bg(Color::DarkGray)
}
pub(crate) fn palette_origin() -> Style {
    Style::new().fg(Color::Cyan)
}
pub(crate) fn profiles_warning() -> Style {
    Style::default().fg(Color::Yellow)
}
pub(crate) fn launch_error() -> Style {
    Style::default().fg(Color::Red)
}

const SYNTAX_PALETTE: [(syntect::highlighting::Color, ratatui::style::Color); 7] = [
    (syntax_rgb(1), ratatui::style::Color::DarkGray),
    (syntax_rgb(2), ratatui::style::Color::Green),
    (syntax_rgb(3), ratatui::style::Color::Yellow),
    (syntax_rgb(4), ratatui::style::Color::Magenta),
    (syntax_rgb(5), ratatui::style::Color::Cyan),
    (syntax_rgb(6), ratatui::style::Color::Blue),
    (syntax_rgb(7), ratatui::style::Color::Red),
];

const fn syntax_rgb(value: u8) -> syntect::highlighting::Color {
    syntect::highlighting::Color {
        r: value,
        g: value,
        b: value,
        a: 255,
    }
}

pub(crate) fn syntax_style(syntax_style: syntect::highlighting::Style) -> ratatui::style::Style {
    let mut style = ratatui::style::Style::default();
    if let Some((_, color)) = SYNTAX_PALETTE
        .iter()
        .find(|(syntax_rgb, _)| *syntax_rgb == syntax_style.foreground)
    {
        style = style.fg(*color);
    }
    for (syntax_modifier, modifier) in [
        (
            syntect::highlighting::FontStyle::BOLD,
            ratatui::style::Modifier::BOLD,
        ),
        (
            syntect::highlighting::FontStyle::ITALIC,
            ratatui::style::Modifier::ITALIC,
        ),
        (
            syntect::highlighting::FontStyle::UNDERLINE,
            ratatui::style::Modifier::UNDERLINED,
        ),
    ] {
        if syntax_style.font_style.contains(syntax_modifier) {
            style = style.add_modifier(modifier);
        }
    }
    style
}

pub(crate) fn syntax_theme() -> syntect::highlighting::Theme {
    let mut theme = syntect::highlighting::Theme {
        name: Some("Terminal palette".into()),
        author: None,
        settings: syntect::highlighting::ThemeSettings::default(),
        scopes: Vec::new(),
    };
    for (selector, colour, font_style) in [
        ("comment", Some(0), None),
        ("string", Some(1), None),
        ("constant.numeric", Some(2), None),
        ("constant.language, constant.character", Some(2), None),
        ("keyword, storage.modifier", Some(3), None),
        ("storage.type", Some(4), None),
        ("entity.name.function, support.function", Some(5), None),
        (
            "entity.name.type, entity.name.class, support.type, support.class",
            Some(4),
            None,
        ),
        (
            "markup.heading",
            None,
            Some(syntect::highlighting::FontStyle::BOLD),
        ),
        ("markup.inserted", Some(1), None),
        ("markup.deleted, invalid", Some(6), None),
    ] {
        if let Ok(scope) = selector.parse() {
            theme.scopes.push(syntect::highlighting::ThemeItem {
                scope,
                style: syntect::highlighting::StyleModifier {
                    foreground: colour.map(|index| SYNTAX_PALETTE[index].0),
                    background: None,
                    font_style,
                },
            });
        }
    }
    theme
}

impl crate::theme::surfaces::ViewStyles {
    pub fn attachment_accent(&self) -> Style {
        self.inline(tokens::COMPOSER_MARKER, Style::default().fg(Color::Cyan))
    }
    pub fn automation_theme(&self) -> AutomationTheme {
        let mut palette = automation_theme();
        palette.accent = self.inline(tokens::AUTOMATION_ACCENT, palette.accent);
        palette.warning = self.inline(tokens::AUTOMATION_WARNING, palette.warning);
        palette.dim = self.inline(tokens::AUTOMATION_METADATA, palette.dim);
        palette
    }
    pub fn background_theme(&self) -> BackgroundTheme {
        let mut palette = background_theme();
        palette.dim = self.inline(tokens::BACKGROUND_METADATA, palette.dim);
        palette.warning = self.inline(tokens::BACKGROUND_WARNING, palette.warning);
        palette.error = self.inline(tokens::BACKGROUND_ERROR, palette.error);
        palette.ok = self.inline(tokens::BACKGROUND_SUCCESS, palette.ok);
        palette.selected = self.inline(tokens::BACKGROUND_SELECTED, palette.selected);
        palette
    }
    pub fn checklist_style(&self) -> ChecklistStyle {
        let mut palette = checklist_style();
        palette.header = self.inline(tokens::WORK_TITLE, palette.header);
        palette.hint = self.inline(tokens::WORK_TASK_TEXT, palette.hint);
        palette.in_progress = self.inline(tokens::WORK_TASK_RUNNING, palette.in_progress);
        palette.pending = self.inline(tokens::WORK_TASK_PENDING, palette.pending);
        palette.completed = self.inline(tokens::WORK_TASK_DONE, palette.completed);
        palette.cancelled = self.inline(tokens::WORK_TASK_CANCELLED, palette.cancelled);
        palette.glyph_in_progress =
            self.inline(tokens::WORK_TASK_RUNNING, palette.glyph_in_progress);
        palette.glyph_completed = self.inline(tokens::WORK_TASK_DONE, palette.glyph_completed);
        palette
    }
    pub fn context_style(&self) -> ContextStyle {
        let mut palette = context_style();
        palette.header = self.inline(tokens::CONTEXT_HEADER, palette.header);
        palette.dim = self.inline(tokens::CONTEXT_TEXT, palette.dim);
        palette.error = self.inline(tokens::CONTEXT_ERROR, palette.error);
        palette.tick = self.inline(tokens::CONTEXT_TICK, palette.tick);
        palette.free = self.inline(tokens::CONTEXT_FREE, palette.free);
        palette.categories[0] =
            self.inline(tokens::CONTEXT_SECTION_SYSTEM_PROMPT, palette.categories[0]);
        palette.categories[1] = self.inline(
            tokens::CONTEXT_SECTION_TOOL_DEFINITIONS,
            palette.categories[1],
        );
        palette.categories[2] = self.inline(tokens::CONTEXT_SECTION_RULES, palette.categories[2]);
        palette.categories[3] = self.inline(tokens::CONTEXT_SECTION_SKILLS, palette.categories[3]);
        palette.categories[4] = self.inline(tokens::CONTEXT_SECTION_MCP, palette.categories[4]);
        palette.categories[5] = self.inline(
            tokens::CONTEXT_SECTION_SUBAGENT_DEFINITIONS,
            palette.categories[5],
        );
        palette.categories[6] = self.inline(tokens::CONTEXT_SECTION_MEMORY, palette.categories[6]);
        palette.categories[7] =
            self.inline(tokens::CONTEXT_SECTION_CONVERSATION, palette.categories[7]);
        palette.categories[8] = self.inline(tokens::CONTEXT_SECTION_OTHER, palette.categories[8]);
        palette
    }
    pub fn pager_theme(&self) -> PagerTheme {
        let mut palette = pager_theme();
        palette.header = self.style(tokens::PAGER_HEADER, palette.header);
        palette.footer = self.style(tokens::PAGER_FOOTER, palette.footer);
        palette.banner = self.style(tokens::TRANSCRIPT_BANNER, palette.banner);
        palette.user = self.style(tokens::TRANSCRIPT_USER_TEXT, palette.user);
        palette.dim = self.style(tokens::TRANSCRIPT_NOTICE, palette.dim);
        palette.tool_ok = self.style(tokens::TOOL_SUCCESS, palette.tool_ok);
        palette.tool_failed = self.style(tokens::TOOL_FAILED, palette.tool_failed);
        palette.diff_add = self.style(tokens::DIFF_ADDED, palette.diff_add);
        palette.diff_del = self.style(tokens::DIFF_REMOVED, palette.diff_del);
        palette.search_match = self.style(tokens::TRANSCRIPT_SEARCH_MATCH, palette.search_match);
        palette.search_current =
            self.style(tokens::TRANSCRIPT_SEARCH_CURRENT, palette.search_current);
        palette.gutter_mark = self.style(tokens::PAGER_GUTTER_MARK, palette.gutter_mark);
        palette
    }
    pub fn side_theme(&self) -> SideTheme {
        let mut palette = side_theme();
        palette.warning = self.inline(tokens::SIDE_WARNING, palette.warning);
        palette.error = self.inline(tokens::SIDE_ERROR, palette.error);
        palette.dim = self.inline(tokens::SIDE_METADATA, palette.dim);
        palette
    }
    pub fn subagent_style(&self) -> SubagentStyle {
        let mut palette = subagent_style();
        palette.header = self.inline(tokens::AGENTS_HEADER, palette.header);
        palette.hint = self.inline(tokens::AGENTS_HELP, palette.hint);
        palette.running = self.inline(tokens::AGENTS_RUNNING, palette.running);
        palette.queued = self.inline(tokens::AGENTS_QUEUED, palette.queued);
        palette.completed = self.inline(tokens::AGENTS_DONE, palette.completed);
        palette.failed = self.inline(tokens::AGENTS_FAILED, palette.failed);
        palette.interrupted = self.inline(tokens::AGENTS_INTERRUPTED, palette.interrupted);
        palette.gone = self.inline(tokens::AGENTS_GONE, palette.gone);
        palette
    }
    pub fn workflow_theme(&self) -> WorkflowTheme {
        let mut palette = workflow_theme();
        palette.normal = self.style(tokens::WORKFLOW_TEXT, palette.normal);
        palette.selected = self.style(tokens::WORKFLOW_SELECTED, palette.selected);
        palette.muted = self.style(tokens::WORKFLOW_METADATA, palette.muted);
        palette.warning = self.style(tokens::WORKFLOW_WARNING, palette.warning);
        palette
    }
    pub fn rewind_theme(&self, legacy_color: bool) -> crate::rewind::RewindTheme {
        let palette = rewind_theme(legacy_color);
        crate::rewind::RewindTheme {
            selected: self.style(tokens::REWIND_SELECTED, palette.selected),
            dim: self.style(tokens::REWIND_METADATA, palette.dim),
            warning: self.style(tokens::REWIND_WARNING, palette.warning),
            accent: self.style(tokens::REWIND_ACCENT, palette.accent),
        }
    }

    pub fn pr_label_style(&self, tone: talaria_core::review::LabelTone) -> Style {
        use talaria_core::review::LabelTone;
        let token = match tone {
            LabelTone::Success => tokens::REVIEW_LABEL_SUCCESS,
            LabelTone::Error => tokens::REVIEW_LABEL_ERROR,
            LabelTone::Warning => tokens::REVIEW_LABEL_WARNING,
            LabelTone::Muted => tokens::REVIEW_LABEL_MUTED,
            LabelTone::Accent => tokens::REVIEW_LABEL_OTHER,
        };
        self.inline(token, pr_label_style(tone))
    }
}
