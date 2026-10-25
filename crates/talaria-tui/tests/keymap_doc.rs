//! The configuration document's key table matches the built-in bindings.

use std::fs;
use std::path::PathBuf;

use talaria_app::config::parse;
use talaria_tui::keys::{Action, KeyMap};

#[test]
fn doc_keys_equal_defaults() {
    let blocks = toml_blocks(&read_document());
    assert_eq!(blocks.len(), 1, "found {} toml blocks", blocks.len());

    let loaded = parse(&blocks[0]);
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    let (map, notices) = KeyMap::from_entries(&loaded.config.keys);
    assert!(notices.is_empty(), "{notices:?}");
    let defaults = KeyMap::defaults();
    for action in Action::ALL {
        assert_eq!(
            labels(&map, action),
            labels(&defaults, action),
            "{}",
            action.config_name()
        );
        assert_eq!(
            labels(&map, action),
            expected(action),
            "{}",
            action.config_name()
        );
    }
}

fn labels(map: &KeyMap, action: Action) -> Vec<String> {
    map.chords(action)
        .iter()
        .map(|chord| chord.label())
        .collect()
}

fn expected(action: Action) -> Vec<String> {
    let labels: &[&str] = match action {
        Action::Submit => &["Enter"],
        Action::Newline => &["Shift+Enter", "Alt+Enter", "Ctrl+J"],
        Action::Interrupt => &["Esc"],
        Action::Cancel => &["Ctrl+C"],
        Action::MoveLeft => &["Left"],
        Action::MoveRight => &["Right"],
        Action::MoveUp => &["Up"],
        Action::MoveDown => &["Down"],
        Action::LineStart => &["Home", "Ctrl+A"],
        Action::LineEnd => &["End", "Ctrl+E"],
        Action::BufferStart => &["Ctrl+Home"],
        Action::BufferEnd => &["Ctrl+End"],
        Action::WordLeft => &["Alt+Left", "Ctrl+Left", "Alt+B"],
        Action::WordRight => &["Alt+Right", "Ctrl+Right", "Alt+F"],
        Action::DeleteBackward => &["Backspace", "Ctrl+H"],
        Action::DeleteForward => &["Delete"],
        Action::DeleteWordBackward => &["Ctrl+W", "Alt+Backspace"],
        Action::KillToLineEnd => &["Ctrl+K"],
        Action::KillToLineStart => &["Ctrl+U"],
        Action::Yank => &["Ctrl+Y"],
        Action::CompletionNext => &["Down", "Ctrl+N"],
        Action::CompletionPrev => &["Up", "Ctrl+P"],
        Action::CompletionAccept => &["Tab"],
        Action::CompletionClose => &["Esc"],
        Action::OpenEditor => &["Ctrl+G", "Alt+G"],
        Action::HistorySearch => &["Ctrl+R"],
        Action::Suspend => &["Ctrl+Z"],
    };
    labels.iter().map(|label| (*label).to_string()).collect()
}

fn read_document() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/configuration.md");
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn toml_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```toml") {
        let after = rest[start + "```toml".len()..]
            .strip_prefix('\n')
            .unwrap_or("");
        let end = after.find("\n```").unwrap_or(after.len());
        blocks.push(after[..end].to_string());
        rest = &after[end..];
    }
    blocks
}
