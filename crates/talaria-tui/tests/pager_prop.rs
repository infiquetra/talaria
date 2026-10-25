//! Properties of search and movement. Issue 222.
//!
//! Every match the index returns is the same match a character scan finds.
//! Moves, resizes, appended cells and a new epoch keep the anchor inside the document.

#[path = "pager_support.rs"]
mod support;

use proptest::prelude::*;
use talaria_core::session::Cell;
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{
    Pager, PagerAction, Query, SearchIndex, cell_search_text, find_all, fold,
};

use support::{context, row_text, user};

fn alphabet() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('a'),
        Just('A'),
        Just('h'),
        Just(' '),
        Just('\u{023A}'),
        Just('\u{1E9E}'),
        Just('\u{0130}'),
        Just('\u{00C4}'),
        Just('x'),
    ]
}

fn sample_text() -> impl Strategy<Value = String> {
    prop::collection::vec(alphabet(), 0..=24).prop_map(|chars| chars.into_iter().collect())
}

fn naive_matches(cells: &[Cell], query: &Query) -> Vec<(usize, usize, usize, usize)> {
    let mut found = Vec::new();
    if query.text.is_empty() {
        return found;
    }
    let needle: Vec<char> = if query.case_sensitive {
        query.text.chars().collect()
    } else {
        query.text.chars().map(fold).collect()
    };
    for (cell_index, cell) in cells.iter().enumerate() {
        let text = cell_search_text(cell, &PlainCellLines);
        let hay: Vec<char> = if query.case_sensitive {
            text.chars().collect()
        } else {
            text.chars().map(fold).collect()
        };
        let mut start = 0;
        let mut ordinal = 0;
        while start + needle.len() <= hay.len() {
            if hay[start..start + needle.len()] == needle[..] {
                found.push((cell_index, ordinal, start, needle.len()));
                ordinal += 1;
                start += needle.len();
            } else {
                start += 1;
            }
        }
    }
    found
}

fn footer_cell(text: &str) -> Option<(usize, usize)> {
    let rest = text.split("cell ").nth(1)?;
    let (left, right) = rest.split_once('/')?;
    let index: usize = left.trim().parse().ok()?;
    let total: usize = right.split_whitespace().next()?.parse().ok()?;
    Some((index, total))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn prop_find_all_equals_naive_scan(
        texts in prop::collection::vec(sample_text(), 1..=6),
        query_text in sample_text(),
    ) {
        let mut cells = vec![user(1, &"a".repeat(100_000))];
        cells.extend(
            texts
                .iter()
                .enumerate()
                .map(|(index, text)| user((index as u64) + 2, text)),
        );
        let query = Query::new(&query_text);
        let mut index = SearchIndex::new();
        let found = find_all(&cells, 1, &mut index, &query, &PlainCellLines);
        let naive = naive_matches(&cells, &query);
        let got: Vec<_> = found
            .iter()
            .map(|item| (item.cell_index, item.ordinal_in_cell, item.char_start, item.char_len))
            .collect();
        prop_assert_eq!(got, naive);
    }

    #[test]
    fn prop_anchor_stays_in_document(
        count in 1usize..=12,
        steps in prop::collection::vec(0u8..=4, 1..=8),
        cols in 20u16..=80,
    ) {
        let cells: Vec<Cell> = (0..count)
            .map(|id| user(id as u64, &format!("row {id} stays")))
            .collect();
        let ctx = context(&cells);
        let mut index = SearchIndex::new();
        let mut pager = Pager::open(&ctx, 80, 24, &mut index);
        for step in steps {
            let action = match step {
                0 => PagerAction::LineUp,
                1 => PagerAction::LineDown,
                2 => PagerAction::Top,
                3 => PagerAction::Bottom,
                _ => PagerAction::PageDown,
            };
            let _ = pager.handle_action(action, &ctx, &mut index);
        }
        pager.resize(cols, 12);
        pager.content_changed(&ctx, &mut index);
        let mut buf = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, cols, 12));
        pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
        let footer = row_text(&buf, 11);
        let (shown, total) = footer_cell(&footer).unwrap_or_else(|| panic!("footer: {footer}"));
        prop_assert_eq!(total, count);
        prop_assert!(shown >= 1 && shown <= count, "cell {shown}/{total}");
    }
}
