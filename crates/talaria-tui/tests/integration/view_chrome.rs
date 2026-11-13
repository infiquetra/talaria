//! Theme chrome affects Talaria's frames and dividers while content stays literal.
use crate::view_colours_support as support;

use talaria_app::theme_file::{BorderPreset, DividerPreset};

fn fixtures(border: BorderPreset, divider: DividerPreset) -> Vec<support::Fixture> {
    support::warm_syntax();
    let mut file = support::full_file();
    file.chrome.border = border;
    file.chrome.section_divider = divider;
    support::fixtures(&support::resolve(Some(&file)))
}

fn content(name: &str) -> bool {
    matches!(name, "markdown" | "syntax")
}

fn single(symbol: &str, rounded: bool) -> &str {
    match symbol {
        "╭" | "┌" | "╔" => {
            if rounded {
                "╭"
            } else {
                "┌"
            }
        }
        "╮" | "┐" | "╗" => {
            if rounded {
                "╮"
            } else {
                "┐"
            }
        }
        "╰" | "└" | "╚" => {
            if rounded {
                "╰"
            } else {
                "└"
            }
        }
        "╯" | "┘" | "╝" => {
            if rounded {
                "╯"
            } else {
                "┘"
            }
        }
        "─" | "═" => "─",
        "│" | "║" => "│",
        "├" | "╠" => "├",
        "┤" | "╣" => "┤",
        "┬" | "╦" => "┬",
        "┴" | "╩" => "┴",
        "┼" | "╬" => "┼",
        other => other,
    }
}

fn double(symbol: &str) -> &str {
    match single(symbol, false) {
        "┌" => "╔",
        "┐" => "╗",
        "└" => "╚",
        "┘" => "╝",
        "─" => "═",
        "│" => "║",
        "├" => "╠",
        "┤" => "╣",
        "┬" => "╦",
        "┴" => "╩",
        "┼" => "╬",
        other => other,
    }
}

#[test]
fn border_and_section_divider_presets_reach_each_recorded_site() {
    support::warm_syntax();
    let original = support::fixtures(&support::resolve(None));
    let mut failures = Vec::new();
    let mut sites = std::collections::BTreeSet::new();
    for (border, divider) in [
        (BorderPreset::Rounded, DividerPreset::Single),
        (BorderPreset::Single, DividerPreset::Single),
        (BorderPreset::Double, DividerPreset::Double),
    ] {
        for (before, after) in original.iter().zip(fixtures(border, divider)) {
            assert_eq!(before.name, after.name);
            if content(before.name) {
                continue;
            }
            for (index, (old, new)) in before
                .buffer
                .content
                .iter()
                .zip(&after.buffer.content)
                .enumerate()
            {
                if !support::is_chrome(old.symbol()) {
                    continue;
                }
                sites.insert(before.name);
                let expected = match border {
                    BorderPreset::Rounded => single(old.symbol(), true),
                    BorderPreset::Single => single(old.symbol(), false),
                    BorderPreset::Double => double(old.symbol()),
                    _ => unreachable!(),
                };
                if new.symbol() != expected {
                    failures.push(format!(
                        "{} ({}, {}) {border:?}/{divider:?}: expected {expected:?}, got {:?}",
                        before.name,
                        index % 80,
                        index / 80,
                        new.symbol()
                    ));
                    break;
                }
            }
        }
    }
    for required in [
        "background-processes",
        "capabilities",
        "history-search",
        "launch",
        "mcp",
        "mcp-card",
        "providers-card",
        "picker",
        "picker-details",
        "work-panel",
        "workflow",
        "model-roles",
        "model-roles-modal",
        "fleet-peek",
        "review",
        "status",
        "composer",
    ] {
        assert!(
            sites.contains(required),
            "missing chrome fixture for {required}"
        );
    }
    assert!(
        failures.is_empty(),
        "unmigrated frame/divider sites:\n{}",
        failures.join("\n")
    );
}

#[test]
fn wide_profiles_divider_uses_the_section_preset() {
    let original = support::wide_profile_divider(&support::resolve(None));
    let sites: Vec<_> = original
        .content
        .iter()
        .enumerate()
        .filter(|(_, cell)| support::is_chrome(cell.symbol()))
        .map(|(index, _)| index)
        .collect();
    assert!(
        !sites.is_empty(),
        "the wide profile branch must draw its divider"
    );
    for (preset, expected) in [
        (DividerPreset::Single, "─"),
        (DividerPreset::Double, "═"),
        (DividerPreset::Ascii, "-"),
        (DividerPreset::None, " "),
    ] {
        let mut file = support::full_file();
        file.chrome.section_divider = preset;
        let actual = support::wide_profile_divider(&support::resolve(Some(&file)));
        for &index in &sites {
            assert_eq!(
                actual.content[index].symbol(),
                expected,
                "profiles {preset:?} at ({}, {})",
                index % 120,
                index / 120
            );
        }
        for (old, new) in original.content.iter().zip(&actual.content) {
            if !support::is_chrome(old.symbol()) {
                assert_eq!(old.symbol(), new.symbol(), "wide profile content moved");
            }
        }
    }
}

#[test]
fn none_removes_glyphs_without_moving_any_content() {
    support::warm_syntax();
    let original = support::fixtures(&support::resolve(None));
    let mut failures = Vec::new();
    for (before, after) in original
        .iter()
        .zip(fixtures(BorderPreset::None, DividerPreset::None))
    {
        assert_eq!(before.name, after.name);
        if content(before.name) {
            continue;
        }
        for (index, (old, new)) in before
            .buffer
            .content
            .iter()
            .zip(&after.buffer.content)
            .enumerate()
        {
            let expected = if support::is_chrome(old.symbol()) {
                " "
            } else {
                old.symbol()
            };
            if new.symbol() != expected {
                failures.push(format!(
                    "{} ({}, {}): expected {expected:?}, got {:?}",
                    before.name,
                    index % 80,
                    index / 80,
                    new.symbol()
                ));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "none must keep the original spacing:\n{}",
        failures.join("\n")
    );
}

#[test]
fn ascii_writes_only_ascii_chrome_and_preserves_semantic_symbols() {
    support::warm_syntax();
    let original = support::fixtures(&support::resolve(None));
    let mut failures = Vec::new();
    for (before, after) in original
        .iter()
        .zip(fixtures(BorderPreset::Ascii, DividerPreset::Ascii))
    {
        assert_eq!(before.name, after.name);
        if content(before.name) {
            continue;
        }
        for (index, (old, new)) in before
            .buffer
            .content
            .iter()
            .zip(&after.buffer.content)
            .enumerate()
        {
            let good = if support::is_chrome(old.symbol()) {
                matches!(new.symbol(), "+" | "-" | "|") && new.symbol().is_ascii()
            } else {
                new.symbol() == old.symbol()
            };
            if !good {
                failures.push(format!(
                    "{} ({}, {}): {:?} became {:?}",
                    before.name,
                    index % 80,
                    index / 80,
                    old.symbol(),
                    new.symbol()
                ));
                break;
            }
        }
        assert!(
            !after
                .buffer
                .content
                .iter()
                .any(|cell| support::is_chrome(cell.symbol())),
            "{} retains non-ASCII chrome",
            after.name
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn border_and_section_divider_are_independent_choices() {
    let views = fixtures(BorderPreset::Double, DividerPreset::Ascii);
    let launch = views.iter().find(|view| view.name == "launch").unwrap();
    assert_eq!(launch.buffer[(0, 0)].symbol(), "╔", "launch border preset");
    let panel = views.iter().find(|view| view.name == "work-panel").unwrap();
    assert_eq!(
        panel.buffer[(0, 0)].symbol(),
        "|",
        "panel section divider preset"
    );
    let views = fixtures(BorderPreset::Ascii, DividerPreset::Double);
    let launch = views.iter().find(|view| view.name == "launch").unwrap();
    assert_eq!(launch.buffer[(0, 0)].symbol(), "+");
    let panel = views.iter().find(|view| view.name == "work-panel").unwrap();
    assert_eq!(panel.buffer[(0, 0)].symbol(), "║");
}

#[test]
fn markdown_rules_and_tables_keep_their_content_glyphs() {
    let baseline = fixtures(BorderPreset::Rounded, DividerPreset::Single);
    let markdown = baseline
        .iter()
        .find(|view| view.name == "markdown")
        .unwrap();
    assert!(
        markdown
            .buffer
            .content
            .iter()
            .any(|cell| support::is_chrome(cell.symbol())),
        "fixture must contain a rule/table grid"
    );
    for (border, divider) in [
        (BorderPreset::None, DividerPreset::None),
        (BorderPreset::Ascii, DividerPreset::Ascii),
        (BorderPreset::Double, DividerPreset::Double),
    ] {
        let changed = fixtures(border, divider);
        let actual = changed.iter().find(|view| view.name == "markdown").unwrap();
        assert_eq!(
            support::text(&actual.buffer),
            support::text(&markdown.buffer)
        );
    }
}
