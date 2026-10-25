//! Every recorded session can be searched from the person's own words. Issue 222.

#[path = "pager_support.rs"]
mod support;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::session::{Cell, SessionState};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{Pager, PagerAction, Query, SearchIndex, SearchInput, find_all};

use support::{context, row_text};

fn first_word(text: &str) -> Option<&str> {
    text.split_whitespace()
        .next()
        .filter(|word| !word.is_empty())
}

fn user_words(state: &SessionState) -> Vec<(usize, String)> {
    state
        .cells()
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| match cell {
            Cell::User(user) => first_word(&user.text).map(|word| (index, word.to_string())),
            _ => None,
        })
        .collect()
}

fn footer_cell(text: &str) -> Option<usize> {
    let rest = text.split("cell ").nth(1)?;
    let (left, _) = rest.split_once('/')?;
    left.trim().parse().ok()
}

#[test]
fn every_fixture_prompt_is_findable() {
    let mut sessions = Vec::new();
    for file in support::FIXTURES {
        let state = support::replay_fixture(file);
        let words = user_words(&state);
        assert!(!words.is_empty(), "{file} has a user cell");
        sessions.push((*file, state, words));
    }

    for (file, state, words) in &sessions {
        let cells = state.cells();
        let ctx = context(cells);
        let mut index = SearchIndex::new();
        let mut pager = Pager::open(&ctx, 80, 24, &mut index);
        for (cell_index, word) in words {
            let query = Query::new(word);
            let found = find_all(cells, ctx.source_epoch, &mut index, &query, &PlainCellLines);
            assert!(
                found.iter().any(|item| item.cell_index == *cell_index),
                "{file} word {word} is in cell {cell_index}"
            );
            let _ = pager.handle_action(PagerAction::StartSearch, &ctx, &mut index);
            for ch in word.chars() {
                let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
            }
            let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
            let mut reached = false;
            for _ in 0..found.len() {
                let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
                pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
                if footer_cell(&row_text(&buf, 23)) == Some(cell_index + 1) {
                    reached = true;
                    break;
                }
                let _ = pager.handle_action(PagerAction::NextMatch, &ctx, &mut index);
            }
            assert!(reached, "{file} reaches cell {} for {word}", cell_index + 1);
        }
    }
}
