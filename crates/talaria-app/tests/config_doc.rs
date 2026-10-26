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
    assert_eq!(loaded.config.composer.paste_collapse_lines, 5);
    assert_eq!(loaded.config.composer.paste_collapse_chars, 2000);
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
        ("open_editor", &["ctrl+g", "alt+g"][..]),
        ("history_search", &["ctrl+r"][..]),
        ("suspend", &["ctrl+z"][..]),
        ("completion_next", &["down", "ctrl+n"][..]),
        ("completion_prev", &["up", "ctrl+p"][..]),
        ("completion_accept", &["tab"][..]),
        ("completion_close", &["esc"][..]),
        ("attach_clipboard_image", &["ctrl+v", "alt+v"][..]),
        // Issue 222 adds the transcript keys to the documented file.
        ("transcript_open", &["ctrl+o"][..]),
        ("pager_line_up", &["up", "k"][..]),
        ("pager_line_down", &["down", "j"][..]),
        ("pager_page_up", &["pageup", "b"][..]),
        ("pager_page_down", &["pagedown", "space"][..]),
        ("pager_half_up", &["ctrl+u"][..]),
        ("pager_half_down", &["ctrl+d"][..]),
        ("pager_top", &["g", "home"][..]),
        ("pager_bottom", &["shift+g", "end"][..]),
        ("pager_prev_prompt", &["{"][..]),
        ("pager_next_prompt", &["}"][..]),
        ("pager_search", &["/"][..]),
        ("pager_next_match", &["n"][..]),
        ("pager_prev_match", &["shift+n"][..]),
        ("pager_details", &["t"][..]),
        ("pager_close", &["q"][..]),
        ("toggle_tasks", &["ctrl+t"][..]),
        ("approvals_toggle", &["shift+tab"][..]),
        ("composer_send_now", &["ctrl+enter"][..]),
        ("composer_steer", &[][..]),
        ("queue_edit", &["enter", "e"][..]),
        ("queue_remove", &["d", "delete", "ctrl+x"][..]),
        ("queue_steer", &["s"][..]),
        ("queue_send_now", &["n"][..]),
        ("queue_move_up", &["K", "shift+up"][..]),
        ("queue_move_down", &["J", "shift+down"][..]),
        ("queue_leave", &["esc"][..]),
        ("session_picker_open", &[][..]),
        ("picker_up", &["up", "k"][..]),
        ("picker_down", &["down", "j"][..]),
        ("picker_page_up", &["pageup"][..]),
        ("picker_page_down", &["pagedown"][..]),
        ("picker_top", &["home", "g"][..]),
        ("picker_bottom", &["end", "G"][..]),
        ("picker_open", &["enter"][..]),
        ("picker_rename", &["r"][..]),
        ("picker_branch", &["b"][..]),
        ("picker_export", &["e"][..]),
        ("picker_search", &["/"][..]),
        ("picker_profile_next", &["tab"][..]),
        ("picker_profile_prev", &["shift+tab"][..]),
        ("picker_refresh", &["ctrl+r"][..]),
        ("picker_close", &["q"][..]),
        ("picker_format_markdown", &["m"][..]),
        ("picker_format_json", &["j"][..]),
        ("picker_format_html", &["h"][..]),
        ("picker_toggle_redact", &["x"][..]),
        ("fleet_view_open", &[][..]),
        ("fleet_up", &["up", "k"][..]),
        ("fleet_down", &["down", "j"][..]),
        ("fleet_page_up", &["pageup"][..]),
        ("fleet_page_down", &["pagedown"][..]),
        ("fleet_top", &["home", "g"][..]),
        ("fleet_bottom", &["end", "shift+g"][..]),
        ("fleet_open", &["enter"][..]),
        ("fleet_peek", &["space"][..]),
        ("fleet_reply", &["r"][..]),
        ("fleet_stop", &["x"][..]),
        ("fleet_answer", &["a"][..]),
        ("fleet_filter", &["/"][..]),
        ("fleet_profile_next", &["tab"][..]),
        ("fleet_profile_prev", &["shift+tab"][..]),
<<<<<<< HEAD
        ("agents_panel", &["alt+s"][..]),
        ("agents_up", &["up", "k"][..]),
        ("agents_down", &["down", "j"][..]),
        ("agents_tail", &["enter"][..]),
        ("agents_detail", &["d", "right"][..]),
        ("agents_back", &["esc", "left"][..]),
        ("agents_close", &["q"][..]),
        ("agents_steer", &["e"][..]),
        ("agents_stop", &["x"][..]),
        ("agents_stop_all", &["shift+x"][..]),
        ("agents_pause", &["p"][..]),
        ("agents_next_session", &["tab"][..]),
        ("agents_page_up", &["pageup"][..]),
        ("agents_page_down", &["pagedown"][..]),
        ("agents_top", &["g", "home"][..]),
        ("agents_bottom", &["shift+g", "end"][..]),
        ("attention_next", &["alt+a"][..]),
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
