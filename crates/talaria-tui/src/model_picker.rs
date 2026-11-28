//! The model overlay and the confirmation card. Issue 230.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
#[cfg(test)]
use talaria_core::models::PickerRow;
use talaria_core::models::PickerView;
use talaria_core::mutation::Confirmation;

use crate::history::{sanitize, word_wrapped};
use crate::live::LiveSection;

/// Paint the model overlay into the alternate-screen buffer.
pub fn draw_model_picker(view: &PickerView, area: Rect, buf: &mut Buffer) {
    crate::picker_frame::draw_picker_frame(view, area, buf);
}

/// The live card that asks whether to switch anyway.
pub struct ModelConfirmSection<'a> {
    pub confirmation: &'a Confirmation,
    pub keep: &'a str,
    readiness: Readiness,
}

enum Readiness {
    Draft,
    Unfocused,
    Guarded,
    Ready,
}

impl<'a> ModelConfirmSection<'a> {
    pub fn new(confirmation: &'a Confirmation, keep: &'a str) -> Self {
        Self {
            confirmation,
            keep,
            readiness: Readiness::Ready,
        }
    }

    /// The composer still has a draft, so the card does not take Enter.
    pub fn waiting_on_draft(mut self) -> Self {
        self.readiness = Readiness::Draft;
        self
    }

    pub fn waiting_on_focus(mut self) -> Self {
        self.readiness = Readiness::Unfocused;
        self
    }

    pub fn guarded(mut self) -> Self {
        self.readiness = Readiness::Guarded;
        self
    }

    fn rows(&self, width: u16) -> Vec<Line<'static>> {
        let title = format!("Switch to {}?", sanitize(&self.confirmation.target_model));
        let message = if self.confirmation.message.is_empty() {
            "Hermes asks you to confirm this switch."
        } else {
            &self.confirmation.message
        };
        let keys = match self.readiness {
            Readiness::Draft => "Clear the draft to answer".into(),
            Readiness::Unfocused => "Return to the conversation to answer".into(),
            Readiness::Guarded => "Confirmation keys will be available in a moment".into(),
            Readiness::Ready => format!("y to confirm · Esc keep {}", sanitize(self.keep)),
        };
        let mut lines: Vec<_> = word_wrapped(&title, width)
            .into_iter()
            .map(|line| Line::styled(line, Style::default().bold()))
            .collect();
        lines.extend(word_wrapped(message, width).into_iter().map(Line::raw));
        lines.extend(word_wrapped(&keys, width).into_iter().map(Line::raw));
        lines
    }
}

impl LiveSection for ModelConfirmSection<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.rows(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        let rows = self.rows(area.width);
        let skip = rows.len().saturating_sub(usize::from(area.height));
        for (offset, row) in (0..area.height).zip(rows.iter().skip(skip)) {
            buf.set_line(area.x, area.y + offset, row, area.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_core::models::RowKey;

    #[test]
    fn a_row_refusal_is_visible_without_hiding_the_selected_row_or_footer() {
        let rows = [PickerRow {
            key: Some(RowKey::NotSetUp {
                provider: "other".into(),
            }),
            text: "Other · set up with hermes model".into(),
            badges: Vec::new(),
            price: None,
            dim: true,
            header: false,
        }];
        let text = "Other is not set up. Set it up with hermes model in a terminal, then press ctrl+r here.";
        let view = PickerView {
            browse: None,
            title: "Switch model · this session",
            filter: "",
            effort: "effort: medium",
            hints: "esc close",
            refreshing: false,
            rows: &rows,
            selected: Some(0),
            banner: Some(text),
        };
        let area = Rect::new(0, 0, 60, 12);
        let mut buffer = Buffer::empty(area);
        draw_model_picker(&view, area, &mut buffer);
        let rendered: Vec<String> = (0..area.height)
            .map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        assert!(rendered[2].contains("Other · set up with hermes model"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("Other is not set up."))
        );
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("then press ctrl+r here."))
        );
        assert!(rendered[11].contains("effort: medium"));
    }
}
