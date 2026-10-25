//! Issue 230 saved-file and command scenarios.

#[path = "support/catalogue.rs"]
mod catalogue;

use serde_json::json;
use talaria_core::models::{
    Effort, EffortCommand, ModelCommand, SavedCheck, SavedDefault, StaleReason,
    check_saved_default, parse_effort_command, parse_model_command,
};

const USAGE_MODEL: &str =
    "Usage: /model [<model> [--provider <id>] [--effort <level>] [--default]]";
const USAGE_EFFORT: &str =
    "Usage: /effort [none|minimal|low|medium|high|xhigh|max|ultra|default] [--default]";

#[test]
fn commands_parse() {
    let parsed = parse_model_command("glm-5 --provider ollama-cloud --effort high --default")
        .expect("full model command");
    assert_eq!(
        parsed,
        ModelCommand::Switch {
            model: "glm-5".to_owned(),
            provider: Some("ollama-cloud".to_owned()),
            effort: Some(Effort::High),
            save_default: true,
        }
    );
    let reordered = parse_model_command("glm-5 --default --effort high --provider ollama-cloud")
        .expect("flags after the model");
    assert_eq!(reordered, parsed);
    assert_eq!(
        parse_model_command("").expect("open"),
        ModelCommand::OpenPicker
    );

    for arg in [
        "--effort high glm-5",
        "--effort high",
        "glm-5 --provider ollama-cloud --provider other",
        "glm-5 --fast",
        "glm-5 --provider",
        "glm-5 --effort default",
        "glm-5 --effort hgih",
    ] {
        assert_eq!(parse_model_command(arg).expect_err(arg), USAGE_MODEL);
    }

    assert_eq!(
        parse_effort_command("default").expect("profile effort"),
        EffortCommand::Set {
            level: None,
            save_default: false
        }
    );
    assert_eq!(
        parse_effort_command("high --default").expect("save effort"),
        EffortCommand::Set {
            level: Some(Effort::High),
            save_default: true
        }
    );
    assert_eq!(parse_effort_command("").expect("show"), EffortCommand::Show);
    for arg in ["extreme", "--default", "high low"] {
        assert_eq!(parse_effort_command(arg).expect_err(arg), USAGE_EFFORT);
    }
}

#[test]
fn stale_check_rules() {
    let saved = SavedDefault {
        model: Some("glm-5".to_owned()),
        provider: Some("ollama-cloud".to_owned()),
        effort: Some(Effort::High),
    };
    assert_eq!(
        check_saved_default(
            &saved,
            Some(&catalogue::stale_options(true, vec!["glm-5"], Some(1)))
        ),
        SavedCheck::Keep
    );
    assert_eq!(
        check_saved_default(
            &saved,
            Some(&catalogue::parse(json!({
                "model": "",
                "provider": "",
                "providers": [{
                    "slug": "other",
                    "name": "Other",
                    "models": ["glm-5"],
                    "authenticated": true
                }]
            })))
        ),
        SavedCheck::DropModelAndProvider(StaleReason::ProviderNotSetUp)
    );
    assert_eq!(
        check_saved_default(
            &saved,
            Some(&catalogue::stale_options(false, vec!["glm-5"], Some(1)))
        ),
        SavedCheck::DropModelAndProvider(StaleReason::ProviderNotSetUp)
    );
    assert_eq!(
        check_saved_default(
            &saved,
            Some(&catalogue::stale_options(
                true,
                vec!["other-model"],
                Some(1)
            ))
        ),
        SavedCheck::DropModelAndProvider(StaleReason::ModelNotListed)
    );
    assert_eq!(
        check_saved_default(
            &saved,
            Some(&catalogue::stale_options(
                true,
                vec!["other-model"],
                Some(30)
            ))
        ),
        SavedCheck::Keep
    );
    assert_eq!(
        check_saved_default(&saved, None),
        SavedCheck::DropModelAndProvider(StaleReason::ListUnreadable)
    );
}
