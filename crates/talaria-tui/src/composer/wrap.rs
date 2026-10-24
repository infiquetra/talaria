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
#[must_use]
pub fn wrap(_text: &str, _width: usize) -> Vec<Row> {
    todo!()
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
