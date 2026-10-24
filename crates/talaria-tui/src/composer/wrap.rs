//! Display-width wrapping. The caller has already reserved the caret cell.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// One visual row. `end` does not include a following line break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Byte index where the row starts.
    pub start: usize,
    /// Byte index where the row ends, exclusive.
    pub end: usize,
    /// A `\n` sits between this row and the next.
    pub hard_break_after: bool,
}

/// Wrap `text` to `width` display cells. A width below 1 is treated as 1.
///
/// Each logical line is filled greedily. When the next cluster does not fit, the row breaks
/// after its last space if it holds one, else before that cluster. A cluster wider than the
/// width gets a row of its own.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<Row> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut line_start = 0;
    loop {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |at| line_start + at);
        wrap_line(text, line_start, line_end, width, &mut rows);
        if line_end == text.len() {
            break;
        }
        if let Some(last) = rows.last_mut() {
            last.hard_break_after = true;
        }
        line_start = line_end + 1;
    }
    rows
}

fn wrap_line(text: &str, start: usize, end: usize, width: usize, rows: &mut Vec<Row>) {
    let mut row_start = start;
    let mut used = 0;
    // Byte offset just after the last space in the current row, if any.
    let mut after_space: Option<usize> = None;
    for (offset, cluster) in text[start..end].grapheme_indices(true) {
        let at = start + offset;
        let cells = cluster_width(cluster);
        if used > 0 && used + cells > width {
            let break_at = match after_space {
                Some(split) if split > row_start => split,
                _ => at,
            };
            rows.push(Row {
                start: row_start,
                end: break_at,
                hard_break_after: false,
            });
            used = display_width(&text[break_at..at]);
            row_start = break_at;
            after_space = None;
            if used > 0 && used + cells > width {
                rows.push(Row {
                    start: row_start,
                    end: at,
                    hard_break_after: false,
                });
                row_start = at;
                used = 0;
            }
        }
        used += cells;
        if cluster == " " {
            after_space = Some(at + cluster.len());
        }
    }
    rows.push(Row {
        start: row_start,
        end,
        hard_break_after: false,
    });
}

/// Display columns of a cluster. A tab is four cells.
#[must_use]
pub fn cluster_width(cluster: &str) -> usize {
    if cluster == "\t" {
        4
    } else {
        UnicodeWidthStr::width(cluster)
    }
}

/// Display columns of `text`, with each tab counting as four.
#[must_use]
pub fn display_width(text: &str) -> usize {
    text.graphemes(true).map(cluster_width).sum()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{Row, display_width, wrap};

    fn row(start: usize, end: usize, hard: bool) -> Row {
        Row {
            start,
            end,
            hard_break_after: hard,
        }
    }

    #[test]
    fn wrap_examples() {
        let hello = "hello world";
        assert_eq!(wrap(hello, 10), vec![row(0, 6, false), row(6, 11, false)]);
        assert_eq!(wrap(hello, 20), vec![row(0, 11, false)]);

        let packed = "abcdefghijk";
        assert_eq!(
            wrap(packed, 10),
            vec![row(0, 10, false), row(10, 11, false)]
        );
        assert_eq!(wrap(packed, 20), vec![row(0, 11, false)]);

        let blank = "ab\n\ncd";
        let blank_rows = vec![row(0, 2, true), row(3, 3, true), row(4, 6, false)];
        assert_eq!(wrap(blank, 10), blank_rows);
        assert_eq!(wrap(blank, 20), blank_rows);

        let wide = "aaaaaaaaa你";
        assert_eq!(wrap(wide, 10), vec![row(0, 9, false), row(9, 12, false)]);
        assert_eq!(wrap(wide, 20), vec![row(0, 12, false)]);

        let tabs = "a\tb\tc\td\te";
        assert_eq!(
            wrap(tabs, 10),
            vec![row(0, 4, false), row(4, 8, false), row(8, 9, false)]
        );
        assert_eq!(wrap(tabs, 20), vec![row(0, 8, false), row(8, 9, false)]);

        let spaces = "hello     world";
        assert_eq!(
            wrap(spaces, 10),
            vec![row(0, 10, false), row(10, 15, false)]
        );
        assert_eq!(wrap(spaces, 20), vec![row(0, 15, false)]);

        assert_eq!(wrap("ab", 0), vec![row(0, 1, false), row(1, 2, false)]);
        assert_eq!(wrap("你", 0), vec![row(0, "你".len(), false)]);
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 32,
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn wrap_properties(
            chars in proptest::collection::vec(
                proptest::sample::select(
                    &['a', 'b', ' ', '\n', '\t', '你', '\u{0301}', 'é', 'x'][..],
                ),
                0..60usize,
            ),
            width in 4usize..=40,
        ) {
            let text: String = chars.into_iter().collect();
            let rows = wrap(&text, width);
            let mut rebuilt = String::new();
            let mut cursor = 0usize;
            let bounds: Vec<usize> = unicode_segmentation::UnicodeSegmentation::grapheme_indices(
                text.as_str(),
                true,
            )
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .collect();
            for wrapped in &rows {
                assert_eq!(wrapped.start, cursor);
                assert!(bounds.contains(&wrapped.start));
                assert!(bounds.contains(&wrapped.end));
                rebuilt.push_str(&text[wrapped.start..wrapped.end]);
                let clusters: Vec<&str> =
                    unicode_segmentation::UnicodeSegmentation::graphemes(
                        &text[wrapped.start..wrapped.end],
                        true,
                    )
                    .collect();
                let width_of = display_width(&text[wrapped.start..wrapped.end]);
                if clusters.len() > 1 {
                    assert!(width_of <= width, "{width_of} > {width}");
                }
                if wrapped.hard_break_after {
                    rebuilt.push('\n');
                    cursor = wrapped.end + 1;
                } else {
                    cursor = wrapped.end;
                }
            }
            assert_eq!(rebuilt, text);
            assert_eq!(cursor, text.len());
        }
    }
}
