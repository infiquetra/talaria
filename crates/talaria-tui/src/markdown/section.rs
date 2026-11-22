use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::live::{LiveSection, tail_hidden};

/// Builds the first row of a cut tail from the count of rows not drawn.
type CutMarker = Box<dyn Fn(usize) -> Line<'static>>;

pub struct MarkdownTailSection {
    tail: Vec<Line<'static>>,
    cut: Option<(usize, CutMarker)>,
}

impl MarkdownTailSection {
    /// Draws the last rows that fit, with no sign of the rows left out.
    #[must_use]
    pub fn new(tail: Vec<Line<'static>>) -> Self {
        Self { tail, cut: None }
    }

    /// Asks for at most `max` rows. Whenever rows are left out, because the tail is longer
    /// than `max` or because the layout gives it fewer rows, the first row drawn gives way to
    /// `marker(hidden)`, where `hidden` counts every row not drawn, so the cut is never silent.
    #[must_use]
    pub fn cut_with(
        tail: Vec<Line<'static>>,
        max: usize,
        marker: impl Fn(usize) -> Line<'static> + 'static,
    ) -> Self {
        Self {
            tail,
            cut: Some((max, Box::new(marker))),
        }
    }
}

fn draw_line(line: &Line<'static>, x: u16, y: u16, width: u16, buf: &mut Buffer) {
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
    buf.set_line(x, y, &drawn, width);
}

impl LiveSection for MarkdownTailSection {
    fn height(&self, _width: u16) -> u16 {
        let len = match &self.cut {
            Some((max, _)) => self.tail.len().min(*max),
            None => self.tail.len(),
        };
        u16::try_from(len).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut take = usize::from(area.height);
        let mut y = area.y;
        if let Some((max, marker)) = &self.cut {
            take = take.min(*max);
            if let Some(hidden) = tail_hidden(self.tail.len(), take) {
                draw_line(&marker(hidden), area.x, y, area.width, buf);
                y += 1;
                take -= 1;
            }
        }
        let skip = self.tail.len().saturating_sub(take);
        for line in self.tail.iter().skip(skip).take(take) {
            draw_line(line, area.x, y, area.width, buf);
            y += 1;
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
