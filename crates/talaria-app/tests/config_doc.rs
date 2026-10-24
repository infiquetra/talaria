//! The configuration document has one toml block, and that block loads cleanly.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use talaria_app::config::{Config, parse};

#[test]
fn doc_default_shape_loads_cleanly() {
    let blocks = toml_blocks(&read_document());
    assert_eq!(blocks.len(), 1, "found {} toml blocks", blocks.len());

    let loaded = parse(&blocks[0]);
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    assert_eq!(loaded.config.status.interval_seconds, 5);
    assert_eq!(loaded.config.status.command, None);
    assert!(loaded.config.environment.allowlist.is_empty());
    assert_eq!(loaded.config.keys, default_keys());
    assert_eq!(
        loaded.config,
        Config {
            keys: default_keys(),
            ..Config::default()
        }
    );
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

fn default_keys() -> BTreeMap<String, Vec<String>> {
    let rows = [
        ("submit", &["enter"][..]),
        ("newline", &["shift+enter", "alt+enter", "ctrl+j"][..]),
        ("interrupt", &["esc"][..]),
        ("cancel", &["ctrl+c"][..]),
        ("move_left", &["left"][..]),
        ("move_right", &["right"][..]),
        ("move_up", &["up"][..]),
        ("move_down", &["down"][..]),
        ("line_start", &["home", "ctrl+a"][..]),
        ("line_end", &["end", "ctrl+e"][..]),
        ("buffer_start", &["ctrl+home"][..]),
        ("buffer_end", &["ctrl+end"][..]),
        ("word_left", &["alt+left", "ctrl+left", "alt+b"][..]),
        ("word_right", &["alt+right", "ctrl+right", "alt+f"][..]),
        ("delete_backward", &["backspace", "ctrl+h"][..]),
        ("delete_forward", &["delete"][..]),
        ("delete_word_backward", &["ctrl+w", "alt+backspace"][..]),
        ("kill_to_line_end", &["ctrl+k"][..]),
        ("kill_to_line_start", &["ctrl+u"][..]),
        ("yank", &["ctrl+y"][..]),
    ];
    rows.into_iter()
        .map(|(name, chords)| {
            (
                name.to_string(),
                chords.iter().map(|chord| (*chord).to_string()).collect(),
            )
        })
        .collect()
}
