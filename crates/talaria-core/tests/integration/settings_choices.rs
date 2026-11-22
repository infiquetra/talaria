//! The built-in table of setting choices, and its parity with the proof file generated from the
//! pinned Hermes source by `tools/contract-gen/contract_gen.py config-choices`.
use serde_json::Value;
use std::collections::BTreeSet;
use talaria_core::settings::{
    ManyStore, OwnKind, Pick, RuntimeList, SaveAs, SchemaList, choice_spec, choice_specs,
    number_spec, number_specs,
};

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
        // A key whose names all come from a fetched list may have no built-in rows.
        assert!(
            !spec.items.is_empty() || spec.runtime.is_some(),
            "{} has no rows",
            spec.key
        );
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
fn keys_whose_values_differ_per_profile_fetch_their_list() {
    for (key, list) in [
        ("display.skin", RuntimeList::Skins),
        ("display.personality", RuntimeList::Personalities),
        ("terminal.backend", RuntimeList::TerminalBackends),
        ("dashboard.theme", RuntimeList::DashboardThemes),
        ("context.engine", RuntimeList::ContextEngines),
        ("kanban.default_assignee", RuntimeList::Profiles),
        ("kanban.orchestrator_profile", RuntimeList::Profiles),
        ("toolsets", RuntimeList::Toolsets),
        ("agent.disabled_toolsets", RuntimeList::Toolsets),
        ("skills.auto_load", RuntimeList::Skills),
    ] {
        assert_eq!(choice_spec(key).unwrap().runtime, Some(list), "{key}");
    }
    assert_eq!(choice_spec("logging.level").unwrap().runtime, None);
    let personality = choice_spec("display.personality").unwrap();
    assert_eq!(personality.items[0].value, "");
    assert_eq!(personality.items[0].label, Some("No personality"));
    assert!(personality.items[0].aliases.contains(&"none"));
    assert!(personality.custom.is_some());
}

#[test]
fn keys_left_for_later_have_no_spec_yet() {
    // Keys present only after the pin come later.
    for key in [
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
fn x_search_effort_is_proven_by_the_list_its_reader_checks() {
    // The general effort list is wider than what the x_search tool accepts.
    let proof = proof();
    let entry = proof["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "x_search.reasoning_effort")
        .unwrap();
    assert_eq!(entry["file"], "tools/x_search_tool.py");
    assert_eq!(entry["name"], "X_SEARCH_REASONING_EFFORTS");
    assert_eq!(
        values("x_search.reasoning_effort"),
        ["", "low", "medium", "high", "xhigh"]
    );
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

#[test]
fn multiple_choice_keys_list_every_value_their_comment_names() {
    let status = choice_spec("display.status_bar.fields").unwrap();
    assert_eq!(
        status.pick,
        Pick::Many {
            ordered: false,
            empty_label: Some("Default set (empty list)"),
            store: ManyStore::List
        }
    );
    assert_eq!(
        values("display.status_bar.fields"),
        [
            "model",
            "context_detail",
            "context_pct",
            "cache_hit",
            "latency",
            "tps",
            "compressions",
            "bg_tasks",
            "bg_processes",
            "bg_subagents",
            "goal",
            "git_branch",
            "duration",
            "prompt_elapsed",
            "idle_since",
            "focus",
            "yolo",
            "stash",
            "battery",
            "title",
            "total_tokens"
        ]
    );
    // The footer's order is the order shown, so it keeps the order picked.
    assert_eq!(
        choice_spec("display.runtime_footer.fields").unwrap().pick,
        Pick::Many {
            ordered: true,
            empty_label: None,
            store: ManyStore::List
        }
    );
    assert_eq!(
        values("display.runtime_footer.fields"),
        ["model", "context_pct", "cwd", "latency", "served_model"]
    );
    // Hermes types this key as a string, so the picks save as one comma string.
    assert_eq!(
        choice_spec("discord.server_actions").unwrap().pick,
        Pick::Many {
            ordered: false,
            empty_label: Some("All actions"),
            store: ManyStore::CommaString
        }
    );
    assert_eq!(values("discord.server_actions").len(), 14);
}

#[test]
fn word_or_own_entry_keys_offer_their_words_and_say_what_an_own_entry_is() {
    for key in [
        "agent.tool_use_enforcement",
        "agent.execution_guidance",
        "agent.intent_ack_continuation",
    ] {
        let spec = choice_spec(key).unwrap();
        assert_eq!(
            spec.pick,
            Pick::Mixed {
                own_label: "Own list…",
                own: OwnKind::List
            },
            "{key}"
        );
        assert_eq!(values(key), ["auto", "true", "false"], "{key}");
        assert_eq!(spec.save, SaveAs::Json, "{key}");
        assert!(spec.custom.is_some(), "{key}");
    }
    let memory = choice_spec("agent.agent_cache.memory_high_mb").unwrap();
    assert_eq!(
        memory.pick,
        Pick::Mixed {
            own_label: "Number of MB…",
            own: OwnKind::Number
        }
    );
    assert_eq!(values("agent.agent_cache.memory_high_mb"), ["auto", "off"]);
    let approvers = choice_spec("approvals.approvers").unwrap();
    assert_eq!(approvers.save, SaveAs::List);
    assert_eq!(approvers.items[0].value, "*");
    assert_eq!(approvers.items[1].value, "");
    assert_eq!(approvers.items[1].label, Some("Requester and admins only"));
    assert_eq!(approvers.saved("*"), serde_json::json!(["*"]));
    assert_eq!(approvers.saved(""), serde_json::json!([]));
}

#[test]
fn delegation_gateway_mode_lists_its_three_modes() {
    assert_eq!(
        values("delegation.gateway_mode"),
        ["inline", "background", "auto"]
    );
}

#[test]
fn number_specs_state_a_range_a_step_and_a_unit() {
    let mut seen = BTreeSet::new();
    for spec in number_specs() {
        assert!(seen.insert(spec.key), "{} twice", spec.key);
        assert!(spec.step > 0.0, "{}", spec.key);
        assert!(!spec.unit.is_empty(), "{} has no unit", spec.key);
        if let (Some(min), Some(max)) = (spec.min, spec.max) {
            assert!(min <= max, "{}", spec.key);
        }
        assert!(
            choice_spec(spec.key).is_none(),
            "{} is both a list and a number",
            spec.key
        );
    }
    let threshold = number_spec("compression.threshold").unwrap();
    assert_eq!((threshold.min, threshold.max), (Some(0.0), Some(1.0)));
    assert!((threshold.step - 0.05).abs() < f64::EPSILON);
    let ratio = number_spec("agent.budget_warning_ratio").unwrap();
    assert!(ratio.exclusive);
    assert_eq!(ratio.blank, Some("off"));
    let attempts = number_spec("compression.max_attempts").unwrap();
    assert_eq!((attempts.min, attempts.max), (Some(1.0), Some(10.0)));
    // A number Hermes states no range for has no spec, so nothing is invented.
    assert!(number_spec("agent.agent_cache.max_size").is_none());
    let blank_keys: Vec<_> = number_specs()
        .iter()
        .filter(|spec| spec.blank.is_some())
        .map(|spec| spec.key)
        .collect();
    assert_eq!(blank_keys.len(), 13, "{blank_keys:?}");
}

#[test]
fn display_language_lists_the_languages_hermes_ships() {
    let spec = choice_spec("display.language").expect("display.language has a chooser");
    assert_eq!(spec.pick, Pick::One);
    assert_eq!(spec.save, SaveAs::Text);
    assert_eq!(spec.items[0].value, "en");
    assert_eq!(spec.items.len(), 17);
    // A language pack adds languages the bundled list lacks.
    assert!(spec.custom.is_some());
    // Hermes reads its aliases, such as a region tag, as the bundled language.
    let pt = spec.items.iter().find(|item| item.value == "pt").unwrap();
    assert!(pt.aliases.contains(&"pt-br"), "{:?}", pt.aliases);
    let proof = proof();
    let entry = proof["choices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "display.language")
        .expect("a proof entry for display.language");
    assert_eq!(entry["file"], "agent/i18n.py");
    assert_eq!(entry["name"], "SUPPORTED_LANGUAGES");
}

#[test]
fn toolset_and_skill_keys_tick_fetched_names_and_take_own_entries() {
    for (key, empty) in [
        ("toolsets", None),
        ("agent.disabled_toolsets", Some("None disabled")),
        ("skills.auto_load", Some("No skills")),
    ] {
        let spec = choice_spec(key).unwrap();
        assert_eq!(
            spec.pick,
            Pick::Many {
                ordered: false,
                empty_label: empty,
                store: ManyStore::List
            },
            "{key}"
        );
        assert!(spec.custom.is_some(), "{key}");
    }
    assert_eq!(values("toolsets"), ["hermes-cli"]);
}
