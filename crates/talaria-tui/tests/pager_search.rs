//! Search text, case and the index. Issue 222.
//!
//! Lowercase query ignores case. Uppercase query is case-sensitive. Matches stay ordered and
//! do not overlap. Search text covers every cell kind. Fold keeps one character per character.
//! The index is reused within one epoch and dropped across epochs.

#[path = "pager_support.rs"]
mod support;

use support::{assistant, notice, prompt, reasoning, tool, user};
use talaria_core::session::{Cell, PromptKind, PromptState, ToolStatus};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{Query, SearchIndex, cell_search_text, find_all, fold};

fn matches(cells: &[Cell], query: &str) -> Vec<talaria_tui::pager::Match> {
    let mut index = SearchIndex::new();
    find_all(cells, 1, &mut index, &Query::new(query), &PlainCellLines)
}

#[test]
fn lowercase_query_ignores_case() {
    let cells = vec![
        user(1, "Heron"),
        user(2, "a heron"),
        user(3, "HERON."),
        user(4, "a  b"),
        user(5, "ab"),
    ];
    let found = matches(&cells, "heron");
    assert_eq!(found.len(), 3, "heron");
    assert_eq!(found[0].cell_index, 0);
    assert_eq!(found[0].char_start, 0);
    assert_eq!(found[1].cell_index, 1);
    assert_eq!(found[1].char_start, 2);
    assert_eq!(found[2].cell_index, 2);
    assert_eq!(found[2].char_start, 0);
    assert!(found.iter().all(|item| item.char_len == 5));

    let spaces = matches(&cells, "  ");
    assert_eq!(spaces.len(), 1, "two spaces are literal");
    assert_eq!(spaces[0].cell_index, 3);
    assert_eq!(spaces[0].char_start, 1);
    assert_eq!(spaces[0].char_len, 2);
}

#[test]
fn uppercase_query_is_case_sensitive() {
    let cells = vec![user(1, "Heron"), user(2, "a heron"), user(3, "HERON.")];
    let found = matches(&cells, "HERON");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].cell_index, 2);
    assert_eq!(found[0].char_start, 0);
    assert_eq!(found[0].char_len, 5);
    assert!(Query::new("HERON").case_sensitive);
    assert!(!Query::new("heron").case_sensitive);
}

#[test]
fn matches_ordered_and_non_overlapping() {
    let cells = vec![
        user(1, "aa"),
        user(2, "aaaa"),
        user(3, "xaa"),
        user(4, "no"),
        user(5, "aa aa"),
    ];
    let found = matches(&cells, "aa");
    let places: Vec<_> = found
        .iter()
        .map(|item| (item.cell_index, item.char_start, item.ordinal_in_cell))
        .collect();
    assert_eq!(
        places,
        vec![
            (0, 0, 0),
            (1, 0, 0),
            (1, 2, 1),
            (2, 1, 0),
            (4, 0, 0),
            (4, 3, 1)
        ]
    );
    let overlap = found.iter().filter(|item| item.cell_index == 1).count();
    assert_eq!(overlap, 2, "aa in aaaa");
}

#[test]
fn search_text_covers_every_cell_kind() {
    let mut call = tool(4, "patch", ToolStatus::Succeeded);
    call.context = Some("<tmp>/notes.txt".to_string());
    call.args = Some(serde_json::json!({"path": "notes.txt", "count": 2}));
    call.summary = Some("edited the file".to_string());
    call.result = Some("beta is now gamma".to_string());
    call.diff = Some("--- a/notes.txt\n+++ b/notes.txt".to_string());

    let mut approval = prompt(
        6,
        PromptKind::Approval,
        PromptState::Answering("once".to_string()),
    );
    approval.command = Some("rm -rf cache".to_string());
    approval.description = Some("remove it".to_string());
    approval.question = Some("what now".to_string());
    approval.choices = vec!["once".to_string(), "deny".to_string()];

    let cells = [
        user(1, "Heron"),
        assistant(2, "**heron**", true),
        reasoning(3, "thinking about heron"),
        Cell::Tool(call),
        notice(5, "heron failed"),
        Cell::Prompt(approval),
    ];
    let plain = PlainCellLines;
    let expected = [
        "Heron",
        "**heron**",
        "thinking about heron",
        // The object keys come out in the order `serde_json` keeps them, which is sorted
        // because the workspace does not turn on the `preserve_order` feature.
        "patch\n<tmp>/notes.txt\n{\n  \"count\": 2,\n  \"path\": \"notes.txt\"\n}\nedited the file\nbeta is now gamma\n--- a/notes.txt\n+++ b/notes.txt",
        "heron failed",
        "rm -rf cache\nremove it\nwhat now\nonce\ndeny\nonce",
    ];
    for (cell, want) in cells.iter().zip(expected) {
        let text = cell_search_text(cell, &plain);
        assert_eq!(text, want);
        assert!(!text.contains("args:"), "{text}");
        assert!(!text.contains("output:"), "{text}");
        assert!(!text.contains('\u{2234}'), "{text}");
        assert!(!text.contains('\u{2691}'), "{text}");
    }
}

#[test]
fn fold_keeps_one_char_per_char() {
    assert_eq!(fold('\u{0130}'), '\u{0130}', "I with dot above stays");
    assert_eq!(fold('\u{00c4}'), '\u{00e4}');
    assert_eq!(fold('\u{023a}'), '\u{2c65}');
    assert_eq!(fold('\u{1e9e}'), '\u{00df}');

    let cells = vec![user(1, "x\u{023a}y\u{1e9e}z")];
    let lower_a = matches(&cells, "\u{2c65}");
    assert_eq!(lower_a.len(), 1);
    assert_eq!(lower_a[0].char_start, 1);
    assert_eq!(lower_a[0].char_len, 1);
    let sharp = matches(&cells, "\u{00df}");
    assert_eq!(sharp.len(), 1);
    assert_eq!(sharp[0].char_start, 3);
}

#[test]
fn index_reused_within_epoch_dropped_across() {
    let cells = vec![assistant(1, "heron heron", true)];
    let query = Query::new("heron");
    let plain = PlainCellLines;
    let mut index = SearchIndex::new();
    let _ = find_all(&cells, 7, &mut index, &query, &plain);
    let built = index.build_count();
    let _ = find_all(&cells, 7, &mut index, &query, &plain);
    assert_eq!(
        index.build_count(),
        built,
        "a final cell is not built twice"
    );
    let _ = find_all(&cells, 8, &mut index, &query, &plain);
    assert!(
        index.build_count() > built,
        "a new epoch drops the old entry and builds again"
    );
}
