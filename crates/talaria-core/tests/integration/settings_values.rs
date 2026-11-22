use serde_json::{Value, json};
use talaria_core::settings::{FieldKind, FieldView, ValueKind, parse_input, value_kind};

fn field(kind: FieldKind) -> FieldView {
    FieldView {
        key: "display.example".into(),
        kind,
        description: String::new(),
        category: "display".into(),
        options: None,
        searchable: false,
        clearable: false,
    }
}

#[test]
fn value_kind_follows_current_then_default_then_schema() {
    let number = field(FieldKind::Number);
    assert_eq!(
        value_kind(&number, Some(&json!(1)), Some(&json!(1.5)), None),
        ValueKind::Integer
    );
    assert_eq!(
        value_kind(&number, Some(&Value::Null), Some(&json!(1.5)), None),
        ValueKind::Float
    );
    assert_eq!(
        value_kind(&field(FieldKind::Boolean), None, None, None),
        ValueKind::Bool
    );
    assert_eq!(
        value_kind(&field(FieldKind::Select), None, None, None),
        ValueKind::Choice
    );
    assert_eq!(
        value_kind(
            &field(FieldKind::String),
            Some(&Value::Null),
            Some(&Value::Null),
            None
        ),
        ValueKind::Nullable
    );
    let list = field(FieldKind::List);
    assert_eq!(
        value_kind(&list, Some(&json!(["a", "b"])), None, None),
        ValueKind::StringList
    );
    assert_eq!(
        value_kind(&list, Some(&json!(["a,b"])), None, None),
        ValueKind::JsonList
    );
    assert_eq!(
        value_kind(&list, Some(&json!([1, 2])), None, None),
        ValueKind::JsonList
    );
}

#[test]
fn parse_input_by_kind() {
    let f = field(FieldKind::String);
    for (s, v) in [
        ("true", true),
        ("ON", true),
        ("yes", true),
        ("1", true),
        ("false", false),
        ("off", false),
        ("NO", false),
        ("0", false),
    ] {
        assert_eq!(
            parse_input(&ValueKind::Bool, &f, s, None).unwrap(),
            json!(v)
        );
    }
    assert!(parse_input(&ValueKind::Bool, &f, "maybe", None).is_err());
    for (s, v) in [("-0", 0), ("+5", 5), ("-42", -42)] {
        assert_eq!(
            parse_input(&ValueKind::Integer, &f, s, None).unwrap(),
            json!(v)
        );
    }
    for s in ["1.5", "9223372036854775808", "", "1e3"] {
        assert!(
            parse_input(&ValueKind::Integer, &f, s, None).is_err(),
            "{s}"
        );
    }
    assert_eq!(
        parse_input(&ValueKind::Float, &f, "1e3", None).unwrap(),
        json!(1000.0)
    );
    for s in ["NaN", "inf", "-inf"] {
        assert!(parse_input(&ValueKind::Float, &f, s, None).is_err());
    }
    assert_eq!(
        parse_input(&ValueKind::Text, &f, "single line", None).unwrap(),
        json!("single line")
    );
    assert!(parse_input(&ValueKind::Text, &f, "a\nb", None).is_err());
    assert_eq!(
        parse_input(&ValueKind::StringList, &f, " a, ,b ", None).unwrap(),
        json!(["a", "b"])
    );
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[1,2]", None).unwrap(),
        json!([1, 2])
    );
    assert!(parse_input(&ValueKind::JsonList, &f, "not an array", None).is_err());
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[3,4]", Some(&json!([1, 2]))).unwrap(),
        json!([3, 4])
    );
    assert!(
        parse_input(
            &ValueKind::JsonList,
            &f,
            "[3,\"four\"]",
            Some(&json!([1, 2]))
        )
        .is_err()
    );
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[]", Some(&json!([1, 2]))).unwrap(),
        json!([])
    );
    assert!(parse_input(&ValueKind::JsonList, &f, "[5]", Some(&json!(["a,b"]))).is_err());

    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "", None).unwrap(),
        Value::Null
    );
    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "true", None).unwrap(),
        json!(true)
    );
    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "hello", None).unwrap(),
        json!("hello")
    );
    let mut choice = field(FieldKind::Select);
    choice.options = Some(vec![json!("one"), json!(3)]);
    choice.clearable = true;
    assert_eq!(
        parse_input(&ValueKind::Choice, &choice, "3", None).unwrap(),
        json!(3)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &choice, "", None).unwrap(),
        json!("")
    );
    assert!(parse_input(&ValueKind::Choice, &choice, "unknown", None).is_err());
}

fn keyed(key: &str, kind: FieldKind, options: Option<Vec<Value>>) -> FieldView {
    FieldView {
        key: key.into(),
        options,
        ..field(kind)
    }
}

#[test]
fn a_string_key_with_a_built_in_list_becomes_a_choice() {
    let text = keyed("browser.dialog_policy", FieldKind::String, None);
    assert_eq!(
        value_kind(&text, Some(&json!("must_respond")), None, None),
        ValueKind::Choice
    );
    let nullable = keyed("computer_use.no_overlay", FieldKind::String, None);
    assert_eq!(
        value_kind(&nullable, Some(&Value::Null), Some(&Value::Null), None),
        ValueKind::Choice
    );
}

#[test]
fn plain_string_key_without_spec_stays_text() {
    let region = keyed("bedrock.region", FieldKind::String, None);
    assert_eq!(
        value_kind(&region, Some(&json!("example-region")), None, None),
        ValueKind::Text
    );
}

#[test]
fn modal_mode_command_accepts_direct_and_refuses_sandbox() {
    let modal = keyed(
        "terminal.modal_mode",
        FieldKind::Select,
        Some(vec![json!("sandbox"), json!("function")]),
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &modal, "direct", Some(&json!("auto"))).unwrap(),
        json!("direct")
    );
    let refused = parse_input(&ValueKind::Choice, &modal, "sandbox", Some(&json!("auto")));
    assert_eq!(
        refused.unwrap_err().message,
        "Choose one of: auto, managed, direct."
    );
}

#[test]
fn the_current_value_is_accepted_even_when_unlisted() {
    let backup = keyed("updates.pre_update_backup", FieldKind::String, None);
    assert_eq!(
        parse_input(
            &ValueKind::Choice,
            &backup,
            "legacy",
            Some(&json!("legacy"))
        )
        .unwrap(),
        json!("legacy")
    );
    assert!(parse_input(&ValueKind::Choice, &backup, "legacy", Some(&json!("off"))).is_err());
}

#[test]
fn context_engine_accepts_compressor_and_a_plugin_name() {
    let engine = keyed(
        "context.engine",
        FieldKind::Select,
        Some(vec![json!("default"), json!("custom")]),
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &engine, "compressor", None).unwrap(),
        json!("compressor")
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &engine, "example-engine", None).unwrap(),
        json!("example-engine")
    );
}

#[test]
fn null_default_choice_saves_json() {
    let overlay = keyed("computer_use.no_overlay", FieldKind::String, None);
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "true", None).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "false", None).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "", None).unwrap(),
        Value::Null
    );
    let effort = keyed("x_search.reasoning_effort", FieldKind::String, None);
    assert_eq!(
        parse_input(&ValueKind::Choice, &effort, "high", None).unwrap(),
        json!("high")
    );
}

#[test]
fn x_search_effort_refuses_values_its_reader_rejects() {
    // Hermes's x_search tool accepts only its own four efforts, not the general effort list.
    let effort = keyed("x_search.reasoning_effort", FieldKind::String, None);
    for refused in ["minimal", "max", "ultra"] {
        assert_eq!(
            parse_input(&ValueKind::Choice, &effort, refused, None)
                .unwrap_err()
                .message,
            "Choose one of: \"\", low, medium, high, xhigh.",
            "{refused}"
        );
    }
}

#[test]
fn schema_options_merge_with_the_built_in_list() {
    let provider = keyed(
        "tts.provider",
        FieldKind::Select,
        Some(vec![json!("edge"), json!("example-command")]),
    );
    for name in ["deepinfra", "example-command", "edge"] {
        assert_eq!(
            parse_input(&ValueKind::Choice, &provider, name, None).unwrap(),
            json!(name)
        );
    }
}

fn edit_as(key: &str, fk: FieldKind, current: Value, text: &str) -> Result<Value, String> {
    let field = keyed(key, fk, None);
    let kind = value_kind(&field, Some(&current), None, None);
    parse_input(&kind, &field, text, Some(&current)).map_err(|error| error.message)
}

fn kind_name(key: &str, fk: FieldKind, current: Value) -> String {
    format!(
        "{:?}",
        value_kind(&keyed(key, fk, None), Some(&current), None, None)
    )
}

#[test]
fn ranged_number_refuses_out_of_range() {
    for (key, bad, message) in [
        (
            "compression.threshold",
            "1.5",
            "Enter a number from 0.0 to 1.0.",
        ),
        (
            "compression.threshold",
            "-0.1",
            "Enter a number from 0.0 to 1.0.",
        ),
        (
            "wake_word.sensitivity",
            "2",
            "Enter a number from 0.0 to 1.0.",
        ),
        (
            "compression.max_attempts",
            "11",
            "Enter a number from 1 to 10.",
        ),
        (
            "approvals.discord_prompt_timeout",
            "10",
            "Enter a number of 30 or more.",
        ),
        (
            "tools.tool_search.threshold_pct",
            "101",
            "Enter a number from 0 to 100.",
        ),
    ] {
        let current = if key == "compression.threshold" || key == "wake_word.sensitivity" {
            json!(0.5)
        } else {
            json!(5)
        };
        assert_eq!(
            edit_as(key, FieldKind::Number, current, bad),
            Err(message.to_owned()),
            "{key} {bad}"
        );
    }
    assert_eq!(
        edit_as(
            "compression.threshold",
            FieldKind::Number,
            json!(0.5),
            "0.8"
        ),
        Ok(json!(0.8))
    );
    assert_eq!(
        edit_as(
            "compression.max_attempts",
            FieldKind::Number,
            json!(3),
            "10"
        ),
        Ok(json!(10))
    );
    assert_eq!(
        edit_as(
            "compression.max_attempts",
            FieldKind::Number,
            json!(3),
            "2.5"
        ),
        Err("Enter a whole number.".to_owned())
    );
}

#[test]
fn a_ratio_that_must_lie_strictly_between_its_bounds_refuses_the_bounds() {
    for bad in ["0", "1", "1.5"] {
        assert_eq!(
            edit_as(
                "agent.budget_warning_ratio",
                FieldKind::String,
                Value::Null,
                bad
            ),
            Err("Enter a number above 0.0 and below 1.0, or leave it blank.".to_owned()),
            "{bad}"
        );
    }
    assert_eq!(
        edit_as(
            "agent.budget_warning_ratio",
            FieldKind::String,
            Value::Null,
            "0.75"
        ),
        Ok(json!(0.75))
    );
    assert_eq!(
        edit_as(
            "agent.budget_warning_ratio",
            FieldKind::String,
            json!(0.75),
            ""
        ),
        Ok(Value::Null)
    );
}

#[test]
fn a_number_whose_blank_means_unset_saves_null_and_refuses_words() {
    for key in [
        "agent.run_budget_seconds",
        "compression.threshold_tokens",
        "context_file_max_chars",
        "cron.max_parallel_jobs",
        "database.journal_size_limit",
        "kanban.max_in_progress",
        "vision.max_calls_per_image",
        "agent.max_turns",
    ] {
        assert_eq!(
            edit_as(key, FieldKind::String, Value::Null, ""),
            Ok(Value::Null),
            "{key}"
        );
        assert_eq!(
            edit_as(key, FieldKind::String, Value::Null, "40"),
            Ok(json!(40)),
            "{key}"
        );
        assert_eq!(
            edit_as(key, FieldKind::String, Value::Null, "soon"),
            Err("Enter a whole number, or leave it blank.".to_owned()),
            "{key}"
        );
    }
    assert_eq!(
        edit_as("agent.max_turns", FieldKind::String, Value::Null, "0"),
        Err("Enter a number of 1 or more, or leave it blank.".to_owned())
    );
    assert_eq!(
        edit_as(
            "max_concurrent_sessions",
            FieldKind::String,
            Value::Null,
            "0"
        ),
        Ok(json!(0))
    );
    assert_eq!(
        edit_as(
            "max_concurrent_sessions",
            FieldKind::String,
            Value::Null,
            "-1"
        ),
        Err("Enter a number of 0 or more, or leave it blank.".to_owned())
    );
}

#[test]
fn a_number_without_a_stated_range_keeps_its_old_behaviour() {
    assert_eq!(
        edit_as(
            "agent.agent_cache.max_size",
            FieldKind::Number,
            json!(128),
            "5000"
        ),
        Ok(json!(5000))
    );
    assert_eq!(
        edit_as(
            "agent.agent_cache.max_size",
            FieldKind::Number,
            json!(128),
            ""
        ),
        Err("Enter a whole number.".to_owned())
    );
}

#[test]
fn a_word_or_own_entry_key_saves_the_word_as_json_and_an_own_list_as_an_array() {
    let key = "agent.tool_use_enforcement";
    assert_eq!(kind_name(key, FieldKind::String, json!("auto")), "Mixed");
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "auto"),
        Ok(json!("auto"))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "true"),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "false"),
        Ok(Value::Bool(false))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "gpt, codex"),
        Ok(json!(["gpt", "codex"]))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "gpt"),
        Ok(json!(["gpt"]))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), " , "),
        Err("Enter model names separated by commas, for example gpt, codex.".to_owned())
    );
    for other in ["agent.execution_guidance", "agent.intent_ack_continuation"] {
        assert_eq!(kind_name(other, FieldKind::String, json!("auto")), "Mixed");
        assert_eq!(
            edit_as(other, FieldKind::String, json!(true), "false"),
            Ok(Value::Bool(false))
        );
    }
}

#[test]
fn a_current_own_list_is_a_mixed_key_too() {
    // The schema types the key as a string, but a list already stored there is still its own list.
    let key = "agent.tool_use_enforcement";
    assert_eq!(
        kind_name(key, FieldKind::String, json!(["gpt", "codex"])),
        "Mixed"
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!(["gpt"]), "gpt, codex, qwen"),
        Ok(json!(["gpt", "codex", "qwen"]))
    );
}

#[test]
fn memory_high_mb_takes_a_word_or_a_number_of_megabytes() {
    let key = "agent.agent_cache.memory_high_mb";
    assert_eq!(kind_name(key, FieldKind::String, json!("auto")), "Mixed");
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "off"),
        Ok(json!("off"))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "2048"),
        Ok(Value::from(2048))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "lots"),
        Err("Enter a whole number of MB, or choose auto or off.".to_owned())
    );
}

#[test]
fn approvers_save_as_a_list_of_ids() {
    let key = "approvals.approvers";
    assert_eq!(kind_name(key, FieldKind::List, json!(["*"])), "Mixed");
    assert_eq!(
        edit_as(key, FieldKind::List, json!(["u1"]), "*"),
        Ok(json!(["*"]))
    );
    assert_eq!(
        edit_as(key, FieldKind::List, json!(["*"]), ""),
        Ok(json!([]))
    );
    assert_eq!(
        edit_as(key, FieldKind::List, json!(["*"]), "u1, u2"),
        Ok(json!(["u1", "u2"]))
    );
}

#[test]
fn multiple_choice_keys_take_a_comma_list_of_their_listed_values() {
    let key = "display.status_bar.fields";
    assert_eq!(kind_name(key, FieldKind::List, json!([])), "Many");
    assert_eq!(
        edit_as(key, FieldKind::List, json!([]), "model, git_branch"),
        Ok(json!(["model", "git_branch"]))
    );
    assert_eq!(
        edit_as(key, FieldKind::List, json!(["model"]), ""),
        Ok(json!([]))
    );
    let refused = edit_as(key, FieldKind::List, json!([]), "model, sparkle").unwrap_err();
    assert!(
        refused.starts_with("Choose from: model, context_detail, context_pct,"),
        "{refused}"
    );
    assert!(refused.contains("sparkle"), "{refused}");
}

#[test]
fn an_ordered_list_keeps_the_order_typed() {
    assert_eq!(
        edit_as(
            "display.runtime_footer.fields",
            FieldKind::List,
            json!(["model"]),
            "cwd, model"
        ),
        Ok(json!(["cwd", "model"]))
    );
}

#[test]
fn a_comma_string_key_keeps_its_string_type() {
    let key = "discord.server_actions";
    assert_eq!(kind_name(key, FieldKind::String, json!("")), "Many");
    assert_eq!(
        edit_as(key, FieldKind::String, json!(""), "list_guilds,server_info"),
        Ok(json!("list_guilds, server_info"))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("list_guilds"), ""),
        Ok(json!(""))
    );
}

#[test]
fn delegation_gateway_mode_is_a_choice() {
    let key = "delegation.gateway_mode";
    assert_eq!(kind_name(key, FieldKind::String, json!("auto")), "Choice");
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "background"),
        Ok(json!("background"))
    );
    assert_eq!(
        edit_as(key, FieldKind::String, json!("auto"), "sideways").unwrap_err(),
        "Choose one of: inline, background, auto."
    );
}

#[test]
fn only_the_leaves_the_model_screens_write_stay_read_only() {
    let owned = |key: &str, fk: FieldKind, current: Value| {
        value_kind(&keyed(key, fk, None), Some(&current), None, None)
            == ValueKind::ReadOnly(talaria_core::settings::ReadOnlyReason::OwnedElsewhere)
    };
    for key in [
        "delegation.provider",
        "delegation.model",
        "delegation.base_url",
        "delegation.api_mode",
        "delegation.reasoning_effort",
        "delegation.fallback_providers",
        "auxiliary.vision.provider",
        "auxiliary.vision.model",
        "auxiliary.vision.base_url",
        "auxiliary.vision.reasoning_effort",
        "moa.default_preset",
        "moa.active_preset",
        "moa.presets.default.aggregator.model",
        "model",
        "providers",
    ] {
        assert!(owned(key, FieldKind::String, json!("x")), "{key}");
    }
    for (key, fk, current) in [
        (
            "delegation.max_concurrent_children",
            FieldKind::Number,
            json!(10),
        ),
        ("delegation.max_spawn_depth", FieldKind::Number, json!(1)),
        (
            "delegation.orchestrator_enabled",
            FieldKind::Boolean,
            json!(true),
        ),
        (
            "delegation.child_timeout_seconds",
            FieldKind::Number,
            json!(0),
        ),
        ("auxiliary.vision.timeout", FieldKind::Number, json!(120)),
        (
            "auxiliary.vision.download_timeout",
            FieldKind::Number,
            json!(30),
        ),
        ("moa.save_traces", FieldKind::Boolean, json!(false)),
        ("moa.trace_dir", FieldKind::String, json!("")),
    ] {
        assert!(!owned(key, fk, current), "{key} should be editable");
    }
    assert_eq!(
        edit_as(
            "delegation.max_concurrent_children",
            FieldKind::Number,
            json!(10),
            "4"
        ),
        Ok(json!(4))
    );
}

#[test]
fn a_shared_container_name_is_not_a_secret() {
    use talaria_core::settings::is_secret_key;
    assert!(!is_secret_key("terminal.docker_shared_container_key"));
    assert!(is_secret_key("terminal.docker_api_key"));
    assert!(is_secret_key("tts.openai.api_key"));
    let name = keyed(
        "terminal.docker_shared_container_key",
        FieldKind::String,
        None,
    );
    assert_eq!(
        value_kind(&name, Some(&json!("team-shared")), Some(&json!("")), None),
        ValueKind::Text
    );
}
