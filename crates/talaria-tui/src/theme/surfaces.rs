//! Bind the existing inline style fields to catalogue tokens.

use super::Theme;
use crate::status::{StatusPalette, Tone};
use ratatui::style::{Modifier, Style};
use std::collections::BTreeMap;

/// A catalogue token selected at a drawing site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token(&'static str);

/// Typed identifiers for the public theme catalogue.
pub mod tokens {
    use super::Token;
    pub const TRANSCRIPT_BACKGROUND: Token = Token("transcript.background");
    pub const TRANSCRIPT_TEXT: Token = Token("transcript.text");
    pub const TRANSCRIPT_USER_LABEL: Token = Token("transcript.user.label");
    pub const TRANSCRIPT_USER_TEXT: Token = Token("transcript.user.text");
    pub const TRANSCRIPT_ASSISTANT_LABEL: Token = Token("transcript.assistant.label");
    pub const TRANSCRIPT_SYSTEM: Token = Token("transcript.system");
    pub const TRANSCRIPT_TIMESTAMP: Token = Token("transcript.timestamp");
    pub const TRANSCRIPT_REASONING: Token = Token("transcript.reasoning");
    pub const TRANSCRIPT_REASONING_LABEL: Token = Token("transcript.reasoning.label");
    pub const TRANSCRIPT_OMISSION: Token = Token("transcript.omission");
    pub const TRANSCRIPT_SEARCH_MATCH: Token = Token("transcript.search.match");
    pub const TRANSCRIPT_SEARCH_CURRENT: Token = Token("transcript.search.current");
    pub const TRANSCRIPT_SELECTION: Token = Token("transcript.selection");
    pub const TRANSCRIPT_HEADING_1: Token = Token("transcript.heading.1");
    pub const TRANSCRIPT_HEADING_2: Token = Token("transcript.heading.2");
    pub const TRANSCRIPT_HEADING_3: Token = Token("transcript.heading.3");
    pub const TRANSCRIPT_STRONG: Token = Token("transcript.strong");
    pub const TRANSCRIPT_EMPHASIS: Token = Token("transcript.emphasis");
    pub const TRANSCRIPT_STRIKETHROUGH: Token = Token("transcript.strikethrough");
    pub const TRANSCRIPT_LINK_TEXT: Token = Token("transcript.link.text");
    pub const TRANSCRIPT_LINK_DESTINATION: Token = Token("transcript.link.destination");
    pub const TRANSCRIPT_IMAGE_ALT: Token = Token("transcript.image.alt");
    pub const TRANSCRIPT_QUOTE_BAR: Token = Token("transcript.quote.bar");
    pub const TRANSCRIPT_QUOTE_TEXT: Token = Token("transcript.quote.text");
    pub const TRANSCRIPT_RULE: Token = Token("transcript.rule");
    pub const TRANSCRIPT_LIST_MARKER: Token = Token("transcript.list.marker");
    pub const TRANSCRIPT_TABLE_BORDER: Token = Token("transcript.table.border");
    pub const TRANSCRIPT_TABLE_HEADER: Token = Token("transcript.table.header");
    pub const TRANSCRIPT_TABLE_CELL: Token = Token("transcript.table.cell");
    pub const COMPOSER_BACKGROUND: Token = Token("composer.background");
    pub const COMPOSER_TEXT: Token = Token("composer.text");
    pub const COMPOSER_PLACEHOLDER: Token = Token("composer.placeholder");
    pub const COMPOSER_MARKER: Token = Token("composer.marker");
    pub const COMPOSER_BORDER: Token = Token("composer.border");
    pub const COMPOSER_BORDER_FOCUSED: Token = Token("composer.border.focused");
    pub const COMPOSER_SELECTION: Token = Token("composer.selection");
    pub const COMPOSER_CURSOR: Token = Token("composer.cursor");
    pub const COMPOSER_ATTACHMENT: Token = Token("composer.attachment");
    pub const COMPOSER_ATTACHMENT_ERROR: Token = Token("composer.attachment.error");
    pub const COMPOSER_PASTE: Token = Token("composer.paste");
    pub const COMPOSER_COMPLETION_TEXT: Token = Token("composer.completion.text");
    pub const COMPOSER_COMPLETION_SELECTED: Token = Token("composer.completion.selected");
    pub const COMPOSER_COMPLETION_DESCRIPTION: Token = Token("composer.completion.description");
    pub const STATUS_BACKGROUND: Token = Token("status.background");
    pub const STATUS_TEXT: Token = Token("status.text");
    pub const STATUS_MUTED: Token = Token("status.muted");
    pub const STATUS_SEPARATOR: Token = Token("status.separator");
    pub const STATUS_ACCENT: Token = Token("status.accent");
    pub const STATUS_SUCCESS: Token = Token("status.success");
    pub const STATUS_WARNING: Token = Token("status.warning");
    pub const STATUS_ERROR: Token = Token("status.error");
    pub const STATUS_ATTENTION: Token = Token("status.attention");
    pub const STATUS_VOICE_PHASE: Token = Token("status.voice.phase");
    pub const STATUS_VOICE_READ_ALOUD: Token = Token("status.voice.read_aloud");
    pub const STATUS_VOICE_TARGET: Token = Token("status.voice.target");
    pub const STATUS_CONNECTION: Token = Token("status.connection");
    pub const STATUS_CONTEXT: Token = Token("status.context");
    pub const STATUS_COST: Token = Token("status.cost");
    pub const QUEUE_BACKGROUND: Token = Token("queue.background");
    pub const QUEUE_LABEL: Token = Token("queue.label");
    pub const QUEUE_TEXT: Token = Token("queue.text");
    pub const QUEUE_SELECTED: Token = Token("queue.selected");
    pub const QUEUE_INDEX: Token = Token("queue.index");
    pub const QUEUE_ACTION: Token = Token("queue.action");
    pub const QUEUE_OVERFLOW: Token = Token("queue.overflow");
    pub const DIALOG_BACKGROUND: Token = Token("dialog.background");
    pub const DIALOG_BORDER: Token = Token("dialog.border");
    pub const DIALOG_TITLE: Token = Token("dialog.title");
    pub const DIALOG_TEXT: Token = Token("dialog.text");
    pub const DIALOG_MUTED: Token = Token("dialog.muted");
    pub const DIALOG_INPUT: Token = Token("dialog.input");
    pub const DIALOG_PLACEHOLDER: Token = Token("dialog.placeholder");
    pub const DIALOG_SELECTION: Token = Token("dialog.selection");
    pub const DIALOG_BUTTON: Token = Token("dialog.button");
    pub const DIALOG_BUTTON_FOCUSED: Token = Token("dialog.button.focused");
    pub const DIALOG_BUTTON_DISABLED: Token = Token("dialog.button.disabled");
    pub const DIALOG_ERROR: Token = Token("dialog.error");
    pub const DIALOG_WARNING: Token = Token("dialog.warning");
    pub const DIALOG_KEY: Token = Token("dialog.key");
    pub const DIALOG_BACKDROP: Token = Token("dialog.backdrop");
    pub const DIALOG_APPROVAL_COMMAND: Token = Token("dialog.approval.command");
    pub const DIALOG_APPROVAL_PATH: Token = Token("dialog.approval.path");
    pub const DIALOG_CLARIFICATION_OPTION: Token = Token("dialog.clarification.option");
    pub const DIALOG_CLARIFICATION_SELECTED: Token = Token("dialog.clarification.selected");
    pub const DIALOG_SECRET_MASK: Token = Token("dialog.secret.mask");
    pub const DIALOG_CONFIRM_DESTRUCTIVE: Token = Token("dialog.confirm.destructive");
    pub const PICKER_BACKGROUND: Token = Token("picker.background");
    pub const PICKER_BORDER: Token = Token("picker.border");
    pub const PICKER_TITLE: Token = Token("picker.title");
    pub const PICKER_FILTER_BACKGROUND: Token = Token("picker.filter.background");
    pub const PICKER_FILTER_TEXT: Token = Token("picker.filter.text");
    pub const PICKER_FILTER_PLACEHOLDER: Token = Token("picker.filter.placeholder");
    pub const PICKER_FILTER_MATCH: Token = Token("picker.filter.match");
    pub const PICKER_GROUP_TAB: Token = Token("picker.group.tab");
    pub const PICKER_GROUP_TAB_SELECTED: Token = Token("picker.group.tab.selected");
    pub const PICKER_GROUP_HEADER: Token = Token("picker.group.header");
    pub const PICKER_ROW_TEXT: Token = Token("picker.row.text");
    pub const PICKER_ROW_SELECTED: Token = Token("picker.row.selected");
    pub const PICKER_ROW_DISABLED: Token = Token("picker.row.disabled");
    pub const PICKER_ROW_DESCRIPTION: Token = Token("picker.row.description");
    pub const PICKER_ROW_MATCH: Token = Token("picker.row.match");
    pub const PICKER_BADGE: Token = Token("picker.badge");
    pub const PICKER_BADGE_CURRENT: Token = Token("picker.badge.current");
    pub const PICKER_BADGE_PROVIDER: Token = Token("picker.badge.provider");
    pub const PICKER_BADGE_EFFORT: Token = Token("picker.badge.effort");
    pub const PICKER_BADGE_UNKNOWN: Token = Token("picker.badge.unknown");
    pub const PICKER_BADGE_UNAVAILABLE: Token = Token("picker.badge.unavailable");
    pub const PICKER_DETAIL_BACKGROUND: Token = Token("picker.detail.background");
    pub const PICKER_DETAIL_DIVIDER: Token = Token("picker.detail.divider");
    pub const PICKER_DETAIL_LABEL: Token = Token("picker.detail.label");
    pub const PICKER_DETAIL_VALUE: Token = Token("picker.detail.value");
    pub const PICKER_DETAIL_HELP: Token = Token("picker.detail.help");
    pub const PICKER_DETAIL_WARNING: Token = Token("picker.detail.warning");
    pub const PICKER_EMPTY: Token = Token("picker.empty");
    pub const PICKER_SCROLLBAR: Token = Token("picker.scrollbar");
    pub const PICKER_FOOTER: Token = Token("picker.footer");
    pub const MENU_BACKGROUND: Token = Token("menu.background");
    pub const MENU_BRAND: Token = Token("menu.brand");
    pub const MENU_ITEM: Token = Token("menu.item");
    pub const MENU_ITEM_FOCUSED: Token = Token("menu.item.focused");
    pub const MENU_ITEM_DISABLED: Token = Token("menu.item.disabled");
    pub const MENU_DROPDOWN_BACKGROUND: Token = Token("menu.dropdown.background");
    pub const MENU_DROPDOWN_BORDER: Token = Token("menu.dropdown.border");
    pub const MENU_DROPDOWN_ITEM: Token = Token("menu.dropdown.item");
    pub const MENU_DROPDOWN_SELECTED: Token = Token("menu.dropdown.selected");
    pub const MENU_DROPDOWN_DISABLED: Token = Token("menu.dropdown.disabled");
    pub const MENU_DROPDOWN_KEY: Token = Token("menu.dropdown.key");
    pub const MENU_DROPDOWN_SEPARATOR: Token = Token("menu.dropdown.separator");
    pub const WORK_BACKGROUND: Token = Token("work.background");
    pub const WORK_TITLE: Token = Token("work.title");
    pub const WORK_ATTENTION: Token = Token("work.attention");
    pub const WORK_LABEL: Token = Token("work.label");
    pub const WORK_GOAL: Token = Token("work.goal");
    pub const WORK_TURN: Token = Token("work.turn");
    pub const WORK_VERDICT_PENDING: Token = Token("work.verdict.pending");
    pub const WORK_VERDICT_SUCCESS: Token = Token("work.verdict.success");
    pub const WORK_VERDICT_FAILURE: Token = Token("work.verdict.failure");
    pub const WORK_SECTION: Token = Token("work.section");
    pub const WORK_DIVIDER: Token = Token("work.divider");
    pub const WORK_TASK_TEXT: Token = Token("work.task.text");
    pub const WORK_TASK_PENDING: Token = Token("work.task.pending");
    pub const WORK_TASK_RUNNING: Token = Token("work.task.running");
    pub const WORK_TASK_DONE: Token = Token("work.task.done");
    pub const WORK_TASK_CANCELLED: Token = Token("work.task.cancelled");
    pub const WORK_TASK_SELECTED: Token = Token("work.task.selected");
    pub const WORK_AGENT_WAITING: Token = Token("work.agent.waiting");
    pub const WORK_AGENT_RUNNING: Token = Token("work.agent.running");
    pub const WORK_AGENT_QUEUED: Token = Token("work.agent.queued");
    pub const WORK_AGENT_DONE: Token = Token("work.agent.done");
    pub const WORK_AGENT_FAILED: Token = Token("work.agent.failed");
    pub const WORK_AGENT_NAME: Token = Token("work.agent.name");
    pub const WORK_AGENT_METADATA: Token = Token("work.agent.metadata");
    pub const WORK_AGENT_UNKNOWN: Token = Token("work.agent.unknown");
    pub const WORK_OVERFLOW: Token = Token("work.overflow");
    pub const WORK_HELP: Token = Token("work.help");
    pub const WORKSPACE_BACKGROUND: Token = Token("workspace.background");
    pub const WORKSPACE_DIVIDER: Token = Token("workspace.divider");
    pub const WORKSPACE_OVERLAY_BORDER: Token = Token("workspace.overlay.border");
    pub const WORKSPACE_EMPTY: Token = Token("workspace.empty");
    pub const BORDER_NORMAL: Token = Token("border.normal");
    pub const BORDER_FOCUSED: Token = Token("border.focused");
    pub const BORDER_DISABLED: Token = Token("border.disabled");
    pub const BORDER_DIVIDER: Token = Token("border.divider");
    pub const CODE_BACKGROUND: Token = Token("code.background");
    pub const CODE_TEXT: Token = Token("code.text");
    pub const CODE_INLINE: Token = Token("code.inline");
    pub const CODE_LABEL: Token = Token("code.label");
    pub const CODE_LINE_NUMBER: Token = Token("code.line_number");
    pub const CODE_SYNTAX_KEYWORD: Token = Token("code.syntax.keyword");
    pub const CODE_SYNTAX_STRING: Token = Token("code.syntax.string");
    pub const CODE_SYNTAX_NUMBER: Token = Token("code.syntax.number");
    pub const CODE_SYNTAX_COMMENT: Token = Token("code.syntax.comment");
    pub const CODE_SYNTAX_FUNCTION: Token = Token("code.syntax.function");
    pub const CODE_SYNTAX_TYPE: Token = Token("code.syntax.type");
    pub const CODE_SYNTAX_OPERATOR: Token = Token("code.syntax.operator");
    pub const CODE_SYNTAX_PUNCTUATION: Token = Token("code.syntax.punctuation");
    pub const CODE_SYNTAX_VARIABLE: Token = Token("code.syntax.variable");
    pub const CODE_SYNTAX_CONSTANT: Token = Token("code.syntax.constant");
    pub const CODE_SYNTAX_INVALID: Token = Token("code.syntax.invalid");
    pub const DIFF_BACKGROUND: Token = Token("diff.background");
    pub const DIFF_ADDED: Token = Token("diff.added");
    pub const DIFF_REMOVED: Token = Token("diff.removed");
    pub const DIFF_CONTEXT: Token = Token("diff.context");
    pub const DIFF_HUNK: Token = Token("diff.hunk");
    pub const DIFF_FILE: Token = Token("diff.file");
    pub const DIFF_NOTE: Token = Token("diff.note");
    pub const DIFF_LINE_NUMBER: Token = Token("diff.line_number");
    pub const DIFF_ADDED_WORD: Token = Token("diff.added.word");
    pub const DIFF_REMOVED_WORD: Token = Token("diff.removed.word");
    pub const DIFF_SELECTION: Token = Token("diff.selection");
    pub const TOOL_BACKGROUND: Token = Token("tool.background");
    pub const TOOL_RUNNING: Token = Token("tool.running");
    pub const TOOL_SUCCESS: Token = Token("tool.success");
    pub const TOOL_FAILED: Token = Token("tool.failed");
    pub const TOOL_MUTED: Token = Token("tool.muted");
    pub const TOOL_TITLE: Token = Token("tool.title");
    pub const TOOL_SUBJECT: Token = Token("tool.subject");
    pub const TOOL_METADATA: Token = Token("tool.metadata");
    pub const TOOL_OUTPUT: Token = Token("tool.output");
    pub const TOOL_ERROR: Token = Token("tool.error");
    pub const ALERT_INFO: Token = Token("alert.info");
    pub const ALERT_SUCCESS: Token = Token("alert.success");
    pub const ALERT_WARNING: Token = Token("alert.warning");
    pub const ALERT_ERROR: Token = Token("alert.error");
    pub const ALERT_ATTENTION: Token = Token("alert.attention");
    pub const ALERT_BACKGROUND: Token = Token("alert.background");
    pub const BADGE_TEXT: Token = Token("badge.text");
    pub const BADGE_BACKGROUND: Token = Token("badge.background");
    pub const BADGE_SUCCESS: Token = Token("badge.success");
    pub const BADGE_WARNING: Token = Token("badge.warning");
    pub const BADGE_ERROR: Token = Token("badge.error");
    pub const BADGE_ATTENTION: Token = Token("badge.attention");
    pub const BADGE_DISABLED: Token = Token("badge.disabled");
    pub const LIST_BACKGROUND: Token = Token("list.background");
    pub const LIST_HEADER: Token = Token("list.header");
    pub const LIST_ROW: Token = Token("list.row");
    pub const LIST_SELECTED: Token = Token("list.selected");
    pub const LIST_MUTED: Token = Token("list.muted");
    pub const LIST_DISABLED: Token = Token("list.disabled");
    pub const LIST_SUCCESS: Token = Token("list.success");
    pub const LIST_WARNING: Token = Token("list.warning");
    pub const LIST_ERROR: Token = Token("list.error");
    pub const LIST_ATTENTION: Token = Token("list.attention");
    pub const LIST_RUNNING: Token = Token("list.running");
    pub const LIST_DIVIDER: Token = Token("list.divider");
    pub const LIST_SCROLLBAR: Token = Token("list.scrollbar");
    pub const LIST_EMPTY: Token = Token("list.empty");
    pub const TRANSCRIPT_BANNER: Token = Token("transcript.banner");
    pub const TRANSCRIPT_COMMAND: Token = Token("transcript.command");
    pub const TRANSCRIPT_NOTICE: Token = Token("transcript.notice");
    pub const AUTOMATION_HEADING: Token = Token("automation.heading");
    pub const AUTOMATION_ACCENT: Token = Token("automation.accent");
    pub const AUTOMATION_WARNING: Token = Token("automation.warning");
    pub const AUTOMATION_METADATA: Token = Token("automation.metadata");
    pub const SIDE_WARNING: Token = Token("side.warning");
    pub const SIDE_ERROR: Token = Token("side.error");
    pub const SIDE_METADATA: Token = Token("side.metadata");
    pub const CONTEXT_HEADER: Token = Token("context.header");
    pub const CONTEXT_TEXT: Token = Token("context.text");
    pub const CONTEXT_FREE: Token = Token("context.free");
    pub const CONTEXT_ERROR: Token = Token("context.error");
    pub const CONTEXT_TICK: Token = Token("context.tick");
    pub const CONTEXT_SECTION_SYSTEM_PROMPT: Token = Token("context.section.system_prompt");
    pub const CONTEXT_SECTION_TOOL_DEFINITIONS: Token = Token("context.section.tool_definitions");
    pub const CONTEXT_SECTION_RULES: Token = Token("context.section.rules");
    pub const CONTEXT_SECTION_SKILLS: Token = Token("context.section.skills");
    pub const CONTEXT_SECTION_MCP: Token = Token("context.section.mcp");
    pub const CONTEXT_SECTION_SUBAGENT_DEFINITIONS: Token =
        Token("context.section.subagent_definitions");
    pub const CONTEXT_SECTION_MEMORY: Token = Token("context.section.memory");
    pub const CONTEXT_SECTION_CONVERSATION: Token = Token("context.section.conversation");
    pub const CONTEXT_SECTION_OTHER: Token = Token("context.section.other");
    pub const FLEET_HEADER: Token = Token("fleet.header");
    pub const FLEET_GROUP: Token = Token("fleet.group");
    pub const FLEET_CURSOR: Token = Token("fleet.cursor");
    pub const FLEET_STATE_ATTENTION: Token = Token("fleet.state.attention");
    pub const FLEET_STATE_WORKING: Token = Token("fleet.state.working");
    pub const FLEET_STATE_FAILED: Token = Token("fleet.state.failed");
    pub const FLEET_STATE_DONE: Token = Token("fleet.state.done");
    pub const FLEET_STATE_IDLE: Token = Token("fleet.state.idle");
    pub const FLEET_LABEL_SUCCESS: Token = Token("fleet.label.success");
    pub const FLEET_LABEL_ERROR: Token = Token("fleet.label.error");
    pub const FLEET_LABEL_WARNING: Token = Token("fleet.label.warning");
    pub const FLEET_LABEL_MUTED: Token = Token("fleet.label.muted");
    pub const FLEET_LABEL_ACCENT: Token = Token("fleet.label.accent");
    pub const FLEET_SECONDARY: Token = Token("fleet.secondary");
    pub const FLEET_PEEK: Token = Token("fleet.peek");
    pub const FLEET_INPUT: Token = Token("fleet.input");
    pub const AGENTS_HEADER: Token = Token("agents.header");
    pub const AGENTS_RUNNING: Token = Token("agents.running");
    pub const AGENTS_QUEUED: Token = Token("agents.queued");
    pub const AGENTS_DONE: Token = Token("agents.done");
    pub const AGENTS_FAILED: Token = Token("agents.failed");
    pub const AGENTS_INTERRUPTED: Token = Token("agents.interrupted");
    pub const AGENTS_GONE: Token = Token("agents.gone");
    pub const AGENTS_HELP: Token = Token("agents.help");
    pub const AGENTS_INPUT: Token = Token("agents.input");
    pub const BACKGROUND_TEXT: Token = Token("background.text");
    pub const BACKGROUND_SELECTED: Token = Token("background.selected");
    pub const BACKGROUND_METADATA: Token = Token("background.metadata");
    pub const BACKGROUND_WARNING: Token = Token("background.warning");
    pub const BACKGROUND_ERROR: Token = Token("background.error");
    pub const BACKGROUND_SUCCESS: Token = Token("background.success");
    pub const PAGER_HEADER: Token = Token("pager.header");
    pub const PAGER_FOOTER: Token = Token("pager.footer");
    pub const PAGER_SEARCH: Token = Token("pager.search");
    pub const PAGER_GUTTER_MARK: Token = Token("pager.gutter_mark");
    pub const REVIEW_HEADER: Token = Token("review.header");
    pub const REVIEW_SELECTION: Token = Token("review.selection");
    pub const REVIEW_INPUT: Token = Token("review.input");
    pub const REVIEW_LABEL_SUCCESS: Token = Token("review.label.success");
    pub const REVIEW_LABEL_ERROR: Token = Token("review.label.error");
    pub const REVIEW_LABEL_WARNING: Token = Token("review.label.warning");
    pub const REVIEW_LABEL_OTHER: Token = Token("review.label.other");
    pub const REVIEW_LABEL_MUTED: Token = Token("review.label.muted");
    pub const REWIND_SELECTED: Token = Token("rewind.selected");
    pub const REWIND_METADATA: Token = Token("rewind.metadata");
    pub const REWIND_WARNING: Token = Token("rewind.warning");
    pub const REWIND_ACCENT: Token = Token("rewind.accent");
    pub const WORKFLOW_TEXT: Token = Token("workflow.text");
    pub const WORKFLOW_SELECTED: Token = Token("workflow.selected");
    pub const WORKFLOW_METADATA: Token = Token("workflow.metadata");
    pub const WORKFLOW_WARNING: Token = Token("workflow.warning");
    pub const CRON_HEADING: Token = Token("cron.heading");
    pub const CRON_ACCENT: Token = Token("cron.accent");
    pub const CRON_WARNING: Token = Token("cron.warning");
    pub const CRON_ERROR: Token = Token("cron.error");
    pub const CRON_HINT: Token = Token("cron.hint");
    pub const CRON_SELECTED: Token = Token("cron.selected");
    pub const CRON_INPUT: Token = Token("cron.input");
    pub const PALETTE_ORIGIN: Token = Token("palette.origin");
    pub const PROFILES_WARNING: Token = Token("profiles.warning");
    pub const PROFILES_ATTENTION: Token = Token("profiles.attention");
    pub const LAUNCH_ERROR: Token = Token("launch.error");
    pub const LAUNCH_MARKER: Token = Token("launch.marker");
    pub const LAUNCH_SECONDARY: Token = Token("launch.secondary");
    pub const CONFIGURE_SELECTION: Token = Token("configure.selection");
    pub const DOCTOR_SELECTION: Token = Token("doctor.selection");
    pub const DOCTOR_HEADER: Token = Token("doctor.header");
    pub const MENTION_SELECTION: Token = Token("mention.selection");
    pub const MENTION_METADATA: Token = Token("mention.metadata");
    pub const EDITOR_SELECTION: Token = Token("editor.selection");
    pub const PROVIDERS_TITLE: Token = Token("providers.title");
    pub const SETTINGS_SELECTION: Token = Token("settings.selection");
}

/// Resolved view styles. An empty context preserves the original view palettes.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewStyles {
    elements: std::sync::Arc<BTreeMap<String, Style>>,
    color: bool,
    border: super::BorderPreset,
    divider: talaria_app::theme_file::DividerPreset,
}

impl Default for ViewStyles {
    fn default() -> Self {
        Self {
            elements: Default::default(),
            color: true,
            border: Default::default(),
            divider: Default::default(),
        }
    }
}

impl ViewStyles {
    pub(super) fn new(
        elements: BTreeMap<String, Style>,
        color: bool,
        border: super::BorderPreset,
        divider: talaria_app::theme_file::DividerPreset,
    ) -> Self {
        Self {
            elements: std::sync::Arc::new(elements),
            color,
            border,
            divider,
        }
    }

    pub fn active(&self) -> bool {
        !self.elements.is_empty()
    }
    pub fn color(&self) -> bool {
        self.color
    }
    pub fn border_preset(&self) -> super::BorderPreset {
        self.border
    }

    /// Markdown surface fields for explicit view renderers.
    pub fn markdown_theme(&self) -> crate::markdown::MarkdownTheme {
        let mut theme = crate::markdown::MarkdownTheme::default();
        macro_rules! bind {
            ($($field:ident => $token:ident),* $(,)?) => { $(
                theme.$field = self.inline(tokens::$token, theme.$field);
            )* };
        }
        bind!(
            heading1 => TRANSCRIPT_HEADING_1, heading2 => TRANSCRIPT_HEADING_2,
            heading3 => TRANSCRIPT_HEADING_3, strong => TRANSCRIPT_STRONG,
            emphasis => TRANSCRIPT_EMPHASIS, strikethrough => TRANSCRIPT_STRIKETHROUGH,
            inline_code => CODE_INLINE, link_text => TRANSCRIPT_LINK_TEXT,
            link_destination => TRANSCRIPT_LINK_DESTINATION, image_alt => TRANSCRIPT_IMAGE_ALT,
            quote_bar => TRANSCRIPT_QUOTE_BAR, rule => TRANSCRIPT_RULE, code_label => CODE_LABEL,
            table_border => TRANSCRIPT_TABLE_BORDER, table_header => TRANSCRIPT_TABLE_HEADER,
        );
        theme
    }

    pub fn style(&self, token: Token, fallback: Style) -> Style {
        self.elements.get(token.0).copied().unwrap_or(fallback)
    }

    pub fn inline(&self, token: Token, fallback: Style) -> Style {
        let style = self.style(token, fallback);
        if self.active() {
            Style { bg: None, ..style }
        } else {
            style
        }
    }

    /// Supply a view's ordinary text style to unstyled spans on an inline surface.
    pub fn inline_lines(
        &self,
        mut lines: Vec<ratatui::text::Line<'static>>,
        token: Token,
    ) -> Vec<ratatui::text::Line<'static>> {
        if !self.active() {
            return lines;
        }
        let base = self.inline(token, Style::default());
        let constrain = |mut style: Style| {
            style.bg = None;
            if !self.color {
                style.fg = None;
                style.underline_color = None;
            }
            style
        };
        for line in &mut lines {
            line.style = constrain(base.patch(line.style));
            for span in &mut line.spans {
                span.style = constrain(line.style.patch(span.style));
            }
        }
        lines
    }

    /// Complete the region owned by a view, or remove backgrounds from an inline region.
    pub fn finish(
        &self,
        area: ratatui::layout::Rect,
        buffer: &mut ratatui::buffer::Buffer,
        token: Token,
        owned_screen: bool,
    ) {
        if !self.active() {
            return;
        }
        let base = self.style(token, Style::default());
        let area = area.intersection(buffer.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buffer[(x, y)];
                if !self.color {
                    cell.fg = ratatui::style::Color::Reset;
                    cell.bg = ratatui::style::Color::Reset;
                    cell.underline_color = ratatui::style::Color::Reset;
                } else {
                    if cell.fg == ratatui::style::Color::Reset {
                        cell.fg = base.fg.unwrap_or(ratatui::style::Color::Reset);
                    }
                    if owned_screen {
                        if cell.bg == ratatui::style::Color::Reset {
                            cell.bg = base.bg.unwrap_or(ratatui::style::Color::Reset);
                        }
                    } else {
                        cell.bg = ratatui::style::Color::Reset;
                    }
                }
            }
        }
    }

    pub fn border_symbol<'a>(&self, original: &'a str) -> &'a str {
        use super::BorderPreset;
        if !self.active() {
            return original;
        }
        let single = match original {
            "╭" | "┌" | "╔" => "┌",
            "╮" | "┐" | "╗" => "┐",
            "╰" | "└" | "╚" => "└",
            "╯" | "┘" | "╝" => "┘",
            "─" | "═" => "─",
            "│" | "║" => "│",
            "├" | "╠" => "├",
            "┤" | "╣" => "┤",
            "┬" | "╦" => "┬",
            "┴" | "╩" => "┴",
            "┼" | "╬" => "┼",
            _ => return original,
        };
        match self.border {
            BorderPreset::Single => single,
            BorderPreset::Rounded => match single {
                "┌" => "╭",
                "┐" => "╮",
                "└" => "╰",
                "┘" => "╯",
                other => other,
            },
            BorderPreset::Double => match single {
                "┌" => "╔",
                "┐" => "╗",
                "└" => "╚",
                "┘" => "╝",
                "─" => "═",
                "│" => "║",
                "├" => "╠",
                "┤" => "╣",
                "┬" => "╦",
                "┴" => "╩",
                "┼" => "╬",
                other => other,
            },
            BorderPreset::Ascii => match single {
                "─" => "-",
                "│" => "|",
                _ => "+",
            },
            BorderPreset::None => " ",
        }
    }

    pub fn divider_symbol<'a>(&self, original: &'a str) -> &'a str {
        use talaria_app::theme_file::DividerPreset;
        if !self.active() {
            return original;
        }
        match original {
            "─" | "═" | "│" | "║" => match self.divider {
                DividerPreset::Single => {
                    if matches!(original, "│" | "║") {
                        "│"
                    } else {
                        "─"
                    }
                }
                DividerPreset::Double => {
                    if matches!(original, "│" | "║") {
                        "║"
                    } else {
                        "═"
                    }
                }
                DividerPreset::Ascii => {
                    if matches!(original, "│" | "║") {
                        "|"
                    } else {
                        "-"
                    }
                }
                DividerPreset::None => " ",
            },
            _ => original,
        }
    }

    pub fn frame_text(&self, text: &str) -> String {
        text.chars()
            .map(|ch| self.border_symbol(ch.encode_utf8(&mut [0; 4])).to_owned())
            .collect()
    }

    pub fn divider_text(&self, text: &str) -> String {
        text.chars()
            .map(|ch| self.divider_symbol(ch.encode_utf8(&mut [0; 4])).to_owned())
            .collect()
    }
}

/// Concrete style field and its catalogue token.
pub const SURFACE_BINDINGS: &[(&str, &str)] = &[
    ("markdown.heading1", "transcript.heading.1"),
    ("markdown.heading2", "transcript.heading.2"),
    ("markdown.heading3", "transcript.heading.3"),
    ("markdown.strong", "transcript.strong"),
    ("markdown.emphasis", "transcript.emphasis"),
    ("markdown.strikethrough", "transcript.strikethrough"),
    ("markdown.inline_code", "code.inline"),
    ("markdown.link_text", "transcript.link.text"),
    ("markdown.link_destination", "transcript.link.destination"),
    ("markdown.image_alt", "transcript.image.alt"),
    ("markdown.quote_bar", "transcript.quote.bar"),
    ("markdown.rule", "transcript.rule"),
    ("markdown.code_label", "code.label"),
    ("markdown.table_border", "transcript.table.border"),
    ("markdown.table_header", "transcript.table.header"),
    ("status.text", "status.text"),
    ("status.muted", "status.muted"),
    ("status.separator", "status.separator"),
    ("status.success", "status.success"),
    ("status.warning", "status.warning"),
    ("status.error", "status.error"),
    ("status.attention", "status.attention"),
    ("status.accent", "status.accent"),
    ("composer.border", "composer.border.focused"),
    ("composer.marker", "composer.marker"),
    ("reasoning", "transcript.reasoning"),
    ("tool.glyph_running", "tool.running"),
    ("tool.glyph_ok", "tool.success"),
    ("tool.glyph_failed", "tool.failed"),
    ("tool.glyph_muted", "tool.muted"),
    ("tool.title", "tool.title"),
    ("tool.subject", "tool.subject"),
    ("tool.meta", "tool.metadata"),
    ("tool.output", "tool.output"),
    ("tool.error", "tool.error"),
    ("tool.diff_added", "diff.added"),
    ("tool.diff_removed", "diff.removed"),
    ("tool.diff_context", "diff.context"),
    ("tool.diff_hunk", "diff.hunk"),
    ("tool.diff_file", "diff.file"),
    ("tool.diff_note", "diff.note"),
    ("tool.line_number", "diff.line_number"),
];

const TONES: [(Tone, &str); 8] = [
    (Tone::Text, "status.text"),
    (Tone::Muted, "status.muted"),
    (Tone::Separator, "status.separator"),
    (Tone::Success, "status.success"),
    (Tone::Warning, "status.warning"),
    (Tone::Error, "status.error"),
    (Tone::Attention, "status.attention"),
    (Tone::Accent, "status.accent"),
];

macro_rules! style_fields {
    ($theme:ident, $field:ident, $style:ident, $($($path:ident).+),+ $(,)?) => {
        match $field {
            $(stringify!($($path).+) => { $theme.$($path).+ = $style; },)+
            _ => {}
        }
    };
}

macro_rules! read_fields {
    ($theme:ident, $field:ident, $($($path:ident).+),+ $(,)?) => {
        match $field {
            $(stringify!($($path).+) => Some($theme.$($path).+),)+
            _ => None
        }
    };
}

fn read(theme: &Theme, field: &str) -> Option<Style> {
    read_fields!(
        theme,
        field,
        markdown.heading1,
        markdown.heading2,
        markdown.heading3,
        markdown.strong,
        markdown.emphasis,
        markdown.strikethrough,
        markdown.inline_code,
        markdown.link_text,
        markdown.link_destination,
        markdown.image_alt,
        markdown.quote_bar,
        markdown.rule,
        markdown.code_label,
        markdown.table_border,
        markdown.table_header,
        composer.border,
        composer.marker,
        reasoning,
        tool.glyph_running,
        tool.glyph_ok,
        tool.glyph_failed,
        tool.glyph_muted,
        tool.title,
        tool.subject,
        tool.meta,
        tool.output,
        tool.error,
        tool.diff_added,
        tool.diff_removed,
        tool.diff_context,
        tool.diff_hunk,
        tool.diff_file,
        tool.diff_note,
        tool.line_number
    )
    .or_else(|| {
        TONES
            .iter()
            .find(|(_, name)| *name == field)
            .map(|(tone, _)| theme.status.style(*tone))
    })
}

/// Inspect every bound style without relying on the resolver's binding application.
pub fn bound_styles(theme: &Theme) -> Vec<(&'static str, Style)> {
    SURFACE_BINDINGS
        .iter()
        .filter_map(|(field, _)| read(theme, field).map(|style| (*field, style)))
        .collect()
}

pub(super) fn apply(
    theme: &mut Theme,
    elements: &BTreeMap<String, Style>,
    sources: &BTreeMap<String, BTreeMap<String, String>>,
) {
    let mut status = TONES.map(|(tone, _)| theme.status.style(tone));
    for &(field, token) in SURFACE_BINDINGS {
        let Some(&style) = elements.get(token) else {
            continue;
        };
        // Inline cells never own the terminal background, including retained scrollback.
        let mut style = Style { bg: None, ..style };
        if !style.add_modifier.contains(Modifier::UNDERLINED)
            && sources
                .get(token)
                .and_then(|fields| fields.get("underline_color"))
                .is_some_and(|source| source == "skin")
        {
            // An untouched skin foreground need not introduce a dormant underline emission.
            // The complete catalogue keeps its effective-foreground fallback independently.
            style.underline_color = read(theme, field).and_then(|legacy| legacy.underline_color);
        }
        style_fields!(
            theme,
            field,
            style,
            markdown.heading1,
            markdown.heading2,
            markdown.heading3,
            markdown.strong,
            markdown.emphasis,
            markdown.strikethrough,
            markdown.inline_code,
            markdown.link_text,
            markdown.link_destination,
            markdown.image_alt,
            markdown.quote_bar,
            markdown.rule,
            markdown.code_label,
            markdown.table_border,
            markdown.table_header,
            composer.border,
            composer.marker,
            reasoning,
            tool.glyph_running,
            tool.glyph_ok,
            tool.glyph_failed,
            tool.glyph_muted,
            tool.title,
            tool.subject,
            tool.meta,
            tool.output,
            tool.error,
            tool.diff_added,
            tool.diff_removed,
            tool.diff_context,
            tool.diff_hunk,
            tool.diff_file,
            tool.diff_note,
            tool.line_number
        );
        if let Some(index) = TONES.iter().position(|(_, name)| *name == field) {
            status[index] = style;
        }
    }
    theme.status = StatusPalette::from_styles([
        status[0], status[1], status[2], status[3], status[4], status[5], status[6],
    ])
    .with_accent(status[7]);
}
