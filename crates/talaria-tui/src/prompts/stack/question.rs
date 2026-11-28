//! The live answer pane. The heading and controls stay in place as the choice list scrolls.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Widget},
};
use talaria_core::session::{PromptCell, PromptKey};

use super::{
    Card, MAX_RENDERED_CHOICES, PromptStack, add, add_capped, add_capped_prefixed, add_field,
    card_header,
};
use crate::history::sanitize;
use crate::theme::surfaces::{ViewStyles, tokens as t};

const FRAME_WIDTH: u16 = 24;

/// The option rows actually drawn, including continuations of wrapped labels.
pub struct QuestionRows {
    pub(super) key: PromptKey,
    area: Rect,
    indices: Vec<usize>,
}

impl QuestionRows {
    pub fn key(&self) -> &PromptKey {
        &self.key
    }

    pub fn offset_y(&mut self, top: u16) {
        self.area.y = self.area.y.saturating_add(top);
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        self.area.contains((col, row).into())
    }

    pub(super) fn index_at(&self, col: u16, row: u16) -> Option<usize> {
        self.contains(col, row)
            .then(|| self.indices[usize::from(row - self.area.y)])
    }
}

pub(super) struct QuestionPane {
    key: PromptKey,
    title: String,
    mode: &'static str,
    question: Vec<Line<'static>>,
    choices: Vec<(usize, Line<'static>)>,
    selected: usize,
    notes: Vec<Line<'static>>,
    field: Option<Line<'static>>,
    pub(super) hint: String,
}

impl QuestionPane {
    pub(super) fn new(stack: &PromptStack, card: &Card, cell: &PromptCell, width: u16) -> Self {
        let inner = if width >= FRAME_WIDTH {
            width - 4
        } else {
            width.max(1)
        };
        let mode = if cell.multi_select {
            "Select all that apply"
        } else {
            "Choose one"
        };
        let mut title = card_header(&format!("Question · {mode}"), card, stack);
        if let Some(seconds) = cell.timeout_s {
            title.push_str(&format!(" · {}s timeout", seconds.floor()));
        }
        let mut question = Vec::new();
        if let Some(text) = &cell.question {
            add_capped(&mut question, text, inner);
        }
        let total = cell.choices.len() + 1; // Includes the custom answer.
        let start = card
            .selected
            .saturating_sub(MAX_RENDERED_CHOICES / 2)
            .min(total.saturating_sub(MAX_RENDERED_CHOICES));
        let end = (start + MAX_RENDERED_CHOICES).min(total);
        let mut choices = Vec::new();
        for index in start..end {
            let marker = if index == card.selected { "›" } else { " " };
            let tick = if !cell.multi_select || index == cell.choices.len() {
                ""
            } else if card.ticks.contains(&index) {
                "[x] "
            } else {
                "[ ] "
            };
            let prefix = if width < FRAME_WIDTH {
                format!("{marker} {tick}")
            } else {
                format!("{marker} {}. {tick}", index + 1)
            };
            let label = cell
                .choices
                .get(index)
                .map_or("Other (type an answer)", String::as_str);
            let mut wrapped = Vec::new();
            add_capped_prefixed(&mut wrapped, &prefix, label, inner);
            choices.extend(wrapped.into_iter().map(|line| (index, line)));
        }
        let mut notes = Vec::new();
        if end - start < total {
            add(
                &mut notes,
                &format!(
                    "{} more choices · {} of {total}",
                    total - (end - start),
                    card.selected + 1
                ),
                inner,
            );
        }
        let field = card.field.then(|| {
            let mut lines = Vec::new();
            let prefix = if inner >= 12 { "answer: " } else { "› " };
            add_field(
                &mut lines,
                prefix,
                &card.text,
                inner.saturating_sub(1).max(1),
            );
            let mut line = lines.pop().unwrap_or_default();
            line.push_span(ratatui::text::Span::styled(
                " ",
                Style::default().reversed(),
            ));
            line
        });
        if let Some(length) = card.too_long {
            add(
                &mut notes,
                &format!("answer too long ({length} of 16,384 characters)"),
                inner,
            );
        }
        let hint: String = if stack.stopping.contains(&card.session().map(str::to_owned)) {
            "stopping…".into()
        } else if !stack.draft_empty {
            "Send or clear your draft to answer".into()
        } else if card.answering() {
            "Sending answer…".into()
        } else if card.field {
            "Enter send   Esc back".into()
        } else if cell.multi_select {
            if inner >= 46 {
                "↑↓ move   Space toggle   Enter send   Esc skip"
            } else {
                "Space toggle  Enter send  Esc skip"
            }
            .into()
        } else if inner >= 53 && total <= 9 {
            format!("↑↓ move   Enter answer   1–{total} choose   Esc skip")
        } else if inner >= 33 {
            "↑↓ move   Enter answer   Esc skip".into()
        } else {
            "Enter answer  Esc skip".into()
        };
        Self {
            key: cell.key.clone(),
            title,
            mode,
            question,
            choices,
            selected: card.selected,
            notes,
            field,
            hint,
        }
    }

    pub(super) fn height(&self) -> u16 {
        u16::try_from(
            2 + self.question.len()
                + self.choices.len()
                + self.notes.len()
                + usize::from(self.field.is_some()),
        )
        .unwrap_or(u16::MAX)
    }

    pub(super) fn draw(
        &self,
        area: Rect,
        buf: &mut Buffer,
        views: &ViewStyles,
        key_hints: bool,
    ) -> QuestionRows {
        let mut hits = QuestionRows {
            key: self.key.clone(),
            area: Rect::default(),
            indices: Vec::new(),
        };
        if area.is_empty() {
            return hits;
        }
        let fits_controls = area.height >= 4 + u16::from(self.field.is_some());
        let room = if fits_controls && area.width >= FRAME_WIDTH {
            let mut frame = Block::bordered()
                .border_set(views.border_set())
                .border_style(views.inline(t::DIALOG_BORDER, Style::default()))
                .title_top(Line::styled(
                    format!(" {} ", sanitize(&self.title)),
                    views.inline(t::DIALOG_TITLE, Style::default()).bold(),
                ));
            if key_hints {
                frame = frame.title_bottom(Line::styled(
                    format!(" {} ", self.hint),
                    views.inline(t::DIALOG_KEY, Style::default()),
                ));
            }
            let inner = frame.inner(area);
            frame.render(area, buf);
            Rect::new(
                inner.x + 1,
                inner.y,
                inner.width.saturating_sub(2),
                inner.height,
            )
        } else if fits_controls {
            buf.set_stringn(
                area.x,
                area.y,
                self.mode,
                usize::from(area.width),
                views.inline(t::DIALOG_TITLE, Style::default()).bold(),
            );
            if key_hints {
                buf.set_stringn(
                    area.x,
                    area.bottom() - 1,
                    &self.hint,
                    usize::from(area.width),
                    views.inline(t::DIALOG_KEY, Style::default()),
                );
            }
            Rect::new(area.x, area.y + 1, area.width, area.height - 2)
        } else {
            area
        };
        // Even when the terminal gives this pane only a few rows, reserve one for the
        // current option. Use the remainder for the question and any active answer field.
        let height = usize::from(room.height).saturating_sub(usize::from(self.field.is_some()));
        let minimum_question = usize::from(
            !self.question.is_empty() && (height >= 2 || (self.field.is_some() && height > 0)),
        );
        let minimum_choice = usize::from(height > minimum_question);
        let note_count = self
            .notes
            .len()
            .min(height.saturating_sub(minimum_question + minimum_choice));
        let question_count = self
            .question
            .len()
            .min(height.saturating_sub(note_count + minimum_choice));
        let option_count = height
            .saturating_sub(note_count + question_count)
            .min(self.choices.len());
        let first_selected = self
            .choices
            .partition_point(|(index, _)| *index < self.selected);
        let selected_end = self
            .choices
            .partition_point(|(index, _)| *index <= self.selected);
        let start = selected_end
            .saturating_sub(option_count)
            .min(first_selected)
            .min(self.choices.len().saturating_sub(option_count));
        let mut y = room.y;
        let mut put = |line: &Line<'_>, style: Style, clipped: bool| {
            buf.set_style(Rect::new(room.x, y, room.width, 1), style);
            let width = room.width.saturating_sub(u16::from(clipped));
            let (end, _) = buf.set_line(room.x, y, line, width);
            if clipped {
                buf.set_stringn(end, y, "…", 1, style);
            }
            y += 1;
        };
        for (i, line) in self.question.iter().take(question_count).enumerate() {
            let style = views.inline(t::DIALOG_TEXT, Style::default()).bold();
            put(
                line,
                style,
                i + 1 == question_count && question_count < self.question.len(),
            );
        }
        hits.area = Rect::new(
            room.x,
            room.y + question_count as u16,
            room.width,
            option_count as u16,
        );
        for (index, line) in &self.choices[start..start + option_count] {
            let style = if *index == self.selected {
                views
                    .inline_selection(t::DIALOG_CLARIFICATION_SELECTED, Style::default().bold())
                    .bold()
            } else {
                views.inline(t::DIALOG_CLARIFICATION_OPTION, Style::default())
            };
            put(line, style, false);
            hits.indices.push(*index);
        }
        for line in self.notes.iter().skip(self.notes.len() - note_count) {
            put(line, views.inline(t::DIALOG_TEXT, Style::default()), false);
        }
        if let Some(field) = &self.field {
            put(
                field,
                views.inline(t::DIALOG_INPUT, Style::default()),
                false,
            );
        }
        hits
    }
}
