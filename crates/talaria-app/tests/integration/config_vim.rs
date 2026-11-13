//! The composer vim setting: on, off, and a wrong type.

use std::fs;
use std::path::PathBuf;

use talaria_app::config::parse;

const NOTICE: &str = "composer.vim in ~/.talaria/config.toml must be true or false; using false.";

#[test]
fn vim_setting_parses_falls_back_and_defaults_off() {
    let absent = parse("");
    assert!(absent.notices.is_empty(), "{:?}", absent.notices);
    assert!(!absent.config.composer.vim, "an absent vim key stays off");

    let wrong = parse("[composer]\nvim = \"yes\"\n");
    assert!(
        !wrong.config.composer.vim,
        "a non-boolean vim value stays off"
    );
    assert_eq!(wrong.notices, vec![NOTICE.to_string()]);

    let blocks = toml_blocks(&read_document());
    assert_eq!(blocks.len(), 1, "found {} toml blocks", blocks.len());
    let document = parse(&blocks[0]);
    assert!(document.notices.is_empty(), "{:?}", document.notices);
    assert!(
        !document.config.composer.vim,
        "the document block leaves vim off"
    );

    let enabled = parse("[composer]\nvim = true\n");
    assert!(enabled.notices.is_empty(), "{:?}", enabled.notices);
    assert!(enabled.config.composer.vim, "vim = true loads on");
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
