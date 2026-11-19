use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::live::LiveSection;

pub struct MarkdownTailSection {
    tail: Vec<Line<'static>>,
}

impl MarkdownTailSection {
    #[must_use]
    pub fn new(tail: Vec<Line<'static>>) -> Self {
        Self { tail }
    }
}

impl LiveSection for MarkdownTailSection {
    fn height(&self, _width: u16) -> u16 {
        u16::try_from(self.tail.len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let take = usize::from(area.height);
        let skip = self.tail.len().saturating_sub(take);
        for (index, line) in self.tail.iter().skip(skip).take(take).enumerate() {
            let spans: Vec<Span<'static>> = line
                .spans
                .iter()
                .map(|span| {
                    Span::styled(
                        talaria_core::text::sanitize(span.content.as_ref()).into_owned(),
                        span.style,
                    )
                })
                .collect();
            let mut drawn = Line::from(spans);
            drawn.style = line.style;
            buf.set_line(area.x, area.y + index as u16, &drawn, area.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MarkdownTailSection;
    use crate::live::LiveSection;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Line;

    #[test]
    fn tail_draw_sanitizes_decoded_characters() {
        let section = MarkdownTailSection::new(vec![Line::from("a\u{202e}b \u{00a0} \u{200d}")]);
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("test terminal");
        terminal
            .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let mut screen = String::new();
        for x in 0..buffer.area.width {
            screen.push_str(buffer[(x, 0)].symbol());
        }
        for (raw, stand_in) in [
            ('\u{202e}', "\u{2039}U+202E\u{203a}"),
            ('\u{00a0}', "\u{2039}U+00A0\u{203a}"),
            ('\u{200d}', "\u{2039}U+200D\u{203a}"),
        ] {
            assert!(screen.contains(stand_in), "{screen:?}");
            assert!(!screen.contains(raw), "{screen:?}");
        }
    }
}
