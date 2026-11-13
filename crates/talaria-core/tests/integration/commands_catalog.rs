//! Catalogue rows, provenance, and warnings. Issue #223.

use std::path::PathBuf;

use serde_json::{Value, json};
use talaria_core::commands::{CatalogResult, CommandCatalog, CommandSource, SkillOrigin};

fn fixture_catalog() -> CatalogResult {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/slash-palette.jsonl");
    let text = std::fs::read_to_string(&path).expect("slash-palette fixture");
    for line in text.lines() {
        let record: Value = serde_json::from_str(line).expect("fixture line");
        if record["dir"] != "in" {
            continue;
        }
        let skills = &record["frame"]["result"]["skills"];
        if skills.as_object().is_some_and(|map| !map.is_empty()) {
            return serde_json::from_value(record["frame"]["result"].clone())
                .expect("catalogue reply decodes");
        }
    }
    panic!("slash-palette fixture has no catalogue reply");
}

fn placeholder(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('/') else {
        return false;
    };
    let Some((kind, number)) = rest.rsplit_once('-') else {
        return false;
    };
    matches!(
        kind,
        "fixture-skill" | "fixture-plugin" | "fixture-user" | "fixture-bundle"
    ) && !number.is_empty()
        && number.chars().all(|ch| ch.is_ascii_digit())
}

fn decode(value: Value) -> CatalogResult {
    serde_json::from_value(value).expect("catalogue result decodes")
}

#[test]
fn catalog_from_recorded_result() {
    let result = fixture_catalog();
    assert!(
        !result.skills.is_empty(),
        "the fixture catalogue has a skill"
    );
    for name in result.skills.keys() {
        assert!(placeholder(name), "skill {name}");
    }
    for name in result.plugin_commands.keys() {
        assert!(placeholder(&format!("/{name}")), "plugin {name}");
    }
    for category in &result.categories {
        if category.name == "User commands" {
            for pair in &category.pairs {
                let name = pair.first().map(String::as_str).unwrap_or("");
                assert!(placeholder(name), "user command {name}");
            }
        }
    }
    assert!(
        result.warning.is_empty() || result.warning == "fixture warning",
        "a non-empty warning must be the fixture warning"
    );
    assert!(
        result.plugin_discovery_error.is_empty()
            || result.plugin_discovery_error == "fixture warning",
        "a non-empty plugin error must be the fixture warning"
    );

    let catalog = CommandCatalog::from_result(&result);
    assert!(!catalog.categories().iter().any(|name| name == "TUI"));
    assert!(catalog.categories().windows(2).all(|pair| {
        let left = result
            .categories
            .iter()
            .position(|category| category.name == *pair[0]);
        let right = result
            .categories
            .iter()
            .position(|category| category.name == *pair[1]);
        left < right
    }));
}

fn provenance_result() -> CatalogResult {
    decode(json!({
        "pairs": [
            ["/new", "Start a new session"],
            ["/shared", "A registry command"],
            ["/fixture-user-1", "Fixture user command 1"],
            ["/both", "Listed as yours"],
            ["/fixture-plugin-1", "Fixture plugin 1"],
            ["/fixture-skill-1", "Fixture skill 1"],
            ["/density", "Ink extra"],
            ["/details", "Ink extra"]
        ],
        "categories": [
            {"name": "Session", "pairs": [
                ["/new", "Start a new session"],
                ["/shared", "A registry command"]
            ]},
            {"name": "User commands", "pairs": [
                ["/fixture-user-1", "Fixture user command 1"],
                ["/both", "Listed as yours"]
            ]},
            {"name": "Plugin commands", "pairs": [
                ["/fixture-plugin-1", "Fixture plugin 1"]
            ]},
            {"name": "TUI", "pairs": [
                ["/density", "Ink extra"],
                ["/details", "Ink extra"]
            ]}
        ],
        "commands": {
            "/new": {"argument_mode": "text"},
            "/shared": {"argument_mode": "text"}
        },
        "plugin_commands": {
            "fixture-plugin-1": "Fixture plugin 1",
            "shared": "hidden behind the registry name"
        },
        "skills": {
            "/fixture-skill-1": {"usage": 4, "origin": "hub"},
            "/both": {"usage": 1, "origin": "local"},
            "/fixture-skill-2": {"usage": 0, "origin": "bundled"}
        },
        "canon": {"/reset": "/new"}
    }))
}

#[test]
fn provenance_rules_in_order() {
    let catalog = CommandCatalog::from_result(&provenance_result());
    let source = |name: &str| catalog.lookup(name).map(|entry| entry.source.clone());
    assert_eq!(
        source("/new"),
        Some(CommandSource::Hermes {
            category: "Session".into()
        })
    );
    assert_eq!(
        source("/shared"),
        Some(CommandSource::Hermes {
            category: "Session".into()
        })
    );
    assert_eq!(source("/fixture-user-1"), Some(CommandSource::UserCommand));
    assert_eq!(source("/both"), Some(CommandSource::UserCommand));
    assert_eq!(source("/fixture-plugin-1"), Some(CommandSource::Plugin));
    assert_eq!(
        source("/fixture-skill-1"),
        Some(CommandSource::Skill {
            origin: SkillOrigin::Hub
        })
    );
    assert_eq!(
        source("/fixture-skill-2"),
        Some(CommandSource::Skill {
            origin: SkillOrigin::Bundled
        })
    );
    assert!(source("/density").is_none());
    assert!(source("/details").is_none());
    assert!(
        catalog
            .entries()
            .iter()
            .all(|entry| entry.name != "/density")
    );
    assert!(
        catalog
            .entries()
            .iter()
            .all(|entry| entry.name != "/details")
    );
}

#[test]
fn canon_resolves_aliases_case_insensitively() {
    let catalog = CommandCatalog::from_result(&provenance_result());
    assert_eq!(catalog.canonical("/RESET"), Some("/new"));
    assert_eq!(catalog.canonical("reset"), Some("/new"));
    assert!(
        catalog
            .lookup("/RESET")
            .is_some_and(|entry| entry.name == "/new")
    );
}

#[test]
fn warning_and_plugin_error_surface() {
    let present = CommandCatalog::from_result(&decode(json!({
        "warning": "fixture warning",
        "plugin_discovery_error": "fixture warning"
    })));
    assert_eq!(present.warning(), Some("fixture warning"));
    assert_eq!(present.plugin_discovery_error(), Some("fixture warning"));

    let absent = CommandCatalog::from_result(&decode(json!({
        "warning": "",
        "plugin_discovery_error": ""
    })));
    assert_eq!(absent.warning(), None);
    assert_eq!(absent.plugin_discovery_error(), None);
}

#[test]
fn decodes_with_unknown_fields_and_defaults() {
    let value = json!({
        "future_field": {"kept": false},
        "pairs": [["/help", "Show available commands"]]
    });
    let result: CatalogResult = serde_json::from_value(value).expect("unknown fields are ignored");
    assert_eq!(result.pairs.len(), 1);
    assert!(result.warning.is_empty());
    assert!(result.plugin_discovery_error.is_empty());
    assert!(result.skills.is_empty());
    assert_eq!(result.skill_count, 0);

    let catalog = CommandCatalog::from_result(&result);
    assert!(catalog.lookup("/help").is_some());
}
