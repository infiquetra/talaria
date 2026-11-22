//! The built-in table of setting choices, and its parity with the proof file generated from the
//! pinned Hermes source by `tools/contract-gen/contract_gen.py config-choices`.
use serde_json::Value;
use std::collections::BTreeSet;
use talaria_core::settings::{SaveAs, SchemaList, choice_spec, choice_specs};

const PROOF: &str = include_str!("../../../../tools/contract-gen/config-choices.json");
const PINNED: &str = include_str!("../../../hermes-contract/PINNED");

fn proof() -> Value {
    serde_json::from_str(PROOF).expect("config-choices.json is JSON")
}

fn values(key: &str) -> Vec<&'static str> {
    choice_spec(key)
        .unwrap_or_else(|| panic!("no spec for {key}"))
        .items
        .iter()
        .map(|item| item.value)
        .collect()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn spec_set(key: &str) -> BTreeSet<String> {
    values(key)
        .into_iter()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_spec_value_is_unique_within_its_key() {
    let mut keys = BTreeSet::new();
    for spec in choice_specs() {
        assert!(keys.insert(spec.key), "{} appears twice", spec.key);
        let mut seen = BTreeSet::new();
        for item in spec.items {
            assert!(
                seen.insert(item.value),
                "{}: {:?} twice",
                spec.key,
                item.value
            );
            for alias in item.aliases {
                assert!(seen.insert(alias), "{}: alias {alias:?} twice", spec.key);
            }
        }
    }
}

#[test]
fn every_spec_row_has_a_one_line_description() {
    for spec in choice_specs() {
        assert!(!spec.items.is_empty(), "{} has no rows", spec.key);
        for item in spec.items {
            let text = item.description;
            assert!(
                !text.is_empty(),
                "{}: {:?} has no description",
                spec.key,
                item.value
            );
            assert!(!text.contains('\n'), "{}: {:?}", spec.key, item.value);
            assert!(
                !text.to_lowercase().contains("(default"),
                "{}: {:?} says default; the chooser marks the live default",
                spec.key,
                item.value
            );
        }
        if spec.items.iter().any(|item| item.value.is_empty()) {
            let blank = spec
                .items
                .iter()
                .find(|item| item.value.is_empty())
                .unwrap();
            assert!(
                blank.label.is_some(),
                "{}: the blank row needs a label",
                spec.key
            );
        }
    }
}

#[test]
fn replace_specs_cover_modal_mode_context_engine_human_delay() {
    assert_eq!(values("terminal.modal_mode"), ["auto", "managed", "direct"]);
    assert_eq!(values("context.engine"), ["compressor"]);
    assert_eq!(values("human_delay.mode"), ["off", "natural", "custom"]);
    for key in ["terminal.modal_mode", "context.engine", "human_delay.mode"] {
        assert_eq!(
            choice_spec(key).unwrap().schema,
            SchemaList::Replace,
            "{key}"
        );
    }
    assert!(choice_spec("context.engine").unwrap().custom.is_some());
    assert!(choice_spec("terminal.modal_mode").unwrap().custom.is_none());
}

#[test]
fn stt_xai_model_offers_the_blank_default() {
    let spec = choice_spec("stt.xai.model").unwrap();
    assert_eq!(spec.items[0].value, "");
    assert!(spec.items[0].label.is_some());
    assert_eq!(spec.schema, SchemaList::Merge);
}

#[test]
fn null_default_keys_save_json() {
    for key in [
        "computer_use.no_overlay",
        "gateway.service_install_choice",
        "onboarding.run",
        "x_search.reasoning_effort",
    ] {
        assert_eq!(choice_spec(key).unwrap().save, SaveAs::Json, "{key}");
    }
    assert_eq!(choice_spec("logging.level").unwrap().save, SaveAs::Text);
}

#[test]
fn keys_left_for_later_have_no_spec_yet() {
    // Run-time lists, the language flag and keys present only after the pin come later.
    for key in [
        "display.personality",
        "display.language",
        "kanban.default_assignee",
        "gateway.sessiondb_loop_guard",
        "voice.auto_tts_mode",
        "bedrock.region",
    ] {
        assert!(choice_spec(key).is_none(), "{key}");
    }
}

#[test]
fn proof_file_is_from_the_pinned_commit() {
    let commit = PINNED
        .lines()
        .find_map(|line| line.strip_prefix("commit="))
        .expect("PINNED commit");
    assert_eq!(proof()["hermes"]["commit"], commit);
}

#[test]
fn every_spec_has_a_proof_entry() {
    let proof = proof();
    let mut proven = BTreeSet::new();
    for entry in proof["choices"].as_array().unwrap() {
        let key = entry["key"].as_str().unwrap();
        assert!(choice_spec(key).is_some(), "proof entry {key} has no spec");
        proven.insert(key.to_owned());
    }
    for entry in proof["schema"].as_array().unwrap() {
        proven.insert(entry["key"].as_str().unwrap().to_owned());
    }
    for spec in choice_specs() {
        assert!(proven.contains(spec.key), "{} has no proof", spec.key);
    }
}

#[test]
fn spec_values_match_the_pinned_hermes_source() {
    let proof = proof();
    for spec in choice_specs() {
        let ours = spec_set(spec.key);
        let entry = proof["choices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == spec.key)
            .or_else(|| {
                proof["schema"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|entry| entry["key"] == spec.key)
            })
            .unwrap();
        let theirs = strings(entry.get("values").unwrap_or(&entry["options"]));
        if entry["match"] == "subset" {
            assert!(
                ours.is_subset(&theirs),
                "{}: {:?} not in the pinned source {:?}",
                spec.key,
                ours.difference(&theirs).collect::<Vec<_>>(),
                theirs
            );
        } else {
            assert_eq!(ours, theirs, "{} differs from the pinned source", spec.key);
        }
    }
}

#[test]
fn replace_specs_still_differ_from_the_schema() {
    let proof = proof();
    for spec in choice_specs()
        .iter()
        .filter(|spec| spec.schema == SchemaList::Replace)
    {
        let schema = proof["schema"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == spec.key)
            .unwrap_or_else(|| panic!("{} replaces a schema list that is not there", spec.key));
        assert_ne!(
            strings(&schema["options"]),
            spec_set(spec.key),
            "Hermes fixed its list for {}; drop the replacement",
            spec.key
        );
    }
}
