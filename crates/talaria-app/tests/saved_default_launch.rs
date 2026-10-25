//! Issue 230, scenarios for saved defaults at launch.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::path::PathBuf;

use mock_gateway::MockGateway;
use serde_json::{Value, json};
use talaria_app::launch::{NewSession, StartMode};
use talaria_app::models::{
    FlagsGiven, OfferedCreateKeys, SavedLaunchOutcome, apply_saved_default, saved_defaults_for_mode,
};
use talaria_core::models::{Effort, SavedDefault};

const PROVIDER_NOT_SET_UP: &str = "Your saved default model minimax-m3.1-flash on ollama-cloud is not available on this profile any more (the provider is not set up), so this session uses the profile's default. Choose a new default with /model and ctrl+g.";
const LIST_UNREADABLE: &str = "Your saved default model minimax-m3.1-flash on ollama-cloud is not available on this profile any more (the model list could not be read), so this session uses the profile's default. Choose a new default with /model and ctrl+g.";
const EFFORT_REFUSED: &str = "Your saved default (effort high) was refused by Hermes (invalid_params), so this session uses the profile's default. Choose a new default with /model and ctrl+g.";
const REASONING_NOT_OFFERED: &str = "Your saved default reasoning was not used: this Hermes gateway does not accept it when creating a session.";

fn fresh() -> NewSession {
    NewSession {
        request_id: "req-saved-1".to_string(),
        source: "tui".to_string(),
        cwd: PathBuf::from("/work/proj"),
        model: None,
        provider: None,
        reasoning: None,
    }
}

fn saved() -> SavedDefault {
    SavedDefault {
        model: Some("glm-5".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: Some(Effort::High),
    }
}

fn offered(keys: &[&str]) -> OfferedCreateKeys {
    OfferedCreateKeys(keys.iter().map(|key| (*key).to_string()).collect())
}

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn applied_names(applied: &talaria_app::models::SavedApplied) -> BTreeSet<String> {
    applied
        .applied
        .iter()
        .map(|item| (*item).to_string())
        .collect()
}

fn profile_view(authenticated: bool) -> Value {
    json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["minimax-m3.1-flash", "glm-5"],
            "authenticated": authenticated
        }]
    })
}

fn saved_model() -> SavedDefault {
    SavedDefault {
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: Some(Effort::High),
    }
}

/// Issue 230, scenario "launch precedence".
#[test]
fn launch_precedence() {
    assert!(!saved_defaults_for_mode(&StartMode::Continue));
    assert!(!saved_defaults_for_mode(&StartMode::Resume(None)));
    assert!(!saved_defaults_for_mode(&StartMode::Resume(Some(
        "sess-kept".to_string(),
    ))));
    assert!(saved_defaults_for_mode(&StartMode::New));

    let all = offered(&["model", "provider", "reasoning", "cwd"]);
    let no_reasoning = offered(&["model", "provider", "cwd"]);
    let stored = saved();

    let mut flagged_model = fresh();
    flagged_model.model = Some("flagged-model".to_string());
    let (session, applied) = apply_saved_default(
        flagged_model,
        Some(&stored),
        FlagsGiven {
            model_or_provider: true,
            effort: false,
        },
        &all,
    );
    assert_eq!(session.model.as_deref(), Some("flagged-model"));
    assert_eq!(session.provider, None);
    assert_eq!(session.reasoning.as_deref(), Some("high"));
    assert_eq!(applied_names(&applied), names(&["reasoning"]));
    assert!(applied.dropped_not_offered.is_empty());

    let mut flagged_effort = fresh();
    flagged_effort.reasoning = Some("low".to_string());
    let (session, applied) = apply_saved_default(
        flagged_effort,
        Some(&stored),
        FlagsGiven {
            model_or_provider: false,
            effort: true,
        },
        &all,
    );
    assert_eq!(session.model.as_deref(), Some("glm-5"));
    assert_eq!(session.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(session.reasoning.as_deref(), Some("low"));
    assert_eq!(applied_names(&applied), names(&["model", "provider"]));
    assert!(applied.dropped_not_offered.is_empty());

    let (session, applied) = apply_saved_default(
        fresh(),
        Some(&stored),
        FlagsGiven {
            model_or_provider: false,
            effort: false,
        },
        &all,
    );
    assert_eq!(session.model.as_deref(), Some("glm-5"));
    assert_eq!(session.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(session.reasoning.as_deref(), Some("high"));
    assert_eq!(
        applied_names(&applied),
        names(&["model", "provider", "reasoning"])
    );
    assert!(applied.dropped_not_offered.is_empty());

    let (session, applied) = apply_saved_default(
        fresh(),
        Some(&stored),
        FlagsGiven {
            model_or_provider: false,
            effort: false,
        },
        &no_reasoning,
    );
    assert_eq!(session.model.as_deref(), Some("glm-5"));
    assert_eq!(session.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(session.reasoning, None);
    assert_eq!(applied.dropped_not_offered, ["reasoning"]);
    let notice = format!(
        "Your saved default {} was not used: this Hermes gateway does not accept it when creating a session.",
        applied.dropped_not_offered[0]
    );
    assert_eq!(notice, REASONING_NOT_OFFERED);
}

fn assert_create_keeps_effort(gateway: &MockGateway, outcome: &SavedLaunchOutcome, notice: &str) {
    let options = gateway.requests("model.options");
    assert_eq!(options.len(), 1, "one profile catalogue before create");
    assert!(options[0].params.get("session_id").is_none());
    assert_eq!(options[0].params["explicit_only"], json!(true));

    let creates = gateway.requests("session.create");
    assert_eq!(creates.len(), 1);
    assert!(creates[0].params.get("model").is_none());
    assert!(creates[0].params.get("provider").is_none());
    assert_eq!(creates[0].params["reasoning"], json!("high"));
    assert_eq!(creates[0].params["cwd"], json!("/work/proj"));
    assert!(
        outcome.notices.iter().any(|line| line == notice),
        "notices: {:?}",
        outcome.notices
    );
}

/// Issue 230, scenario "stale saved default is dropped before create".
#[tokio::test]
async fn stale_saved_default_is_dropped_before_create() {
    let stored = saved_model();
    let flags = FlagsGiven {
        model_or_provider: false,
        effort: false,
    };
    let keys = offered(&["model", "provider", "reasoning", "cwd"]);

    let (gateway, client, _stream) = MockGateway::start().await;
    let catalogue = profile_view(false);
    gateway.set_model_options(catalogue.clone(), catalogue);
    let outcome =
        talaria_app::models::prepare_saved_launch(&client, fresh(), Some(&stored), flags, &keys)
            .await;
    assert_create_keeps_effort(&gateway, &outcome, PROVIDER_NOT_SET_UP);

    let (gateway, client, _stream) = MockGateway::start().await;
    let catalogue = profile_view(true);
    gateway.set_model_options(catalogue.clone(), catalogue);
    gateway.fail_model_options(Some("timed out"));
    let outcome =
        talaria_app::models::prepare_saved_launch(&client, fresh(), Some(&stored), flags, &keys)
            .await;
    assert_create_keeps_effort(&gateway, &outcome, LIST_UNREADABLE);
}

/// Issue 230, scenario "create refused with saved fields retries without".
#[tokio::test]
async fn create_refused_with_saved_fields_retries_without() {
    let (gateway, client, _stream) = MockGateway::start().await;
    gateway.set_create_refusal_efforts(&["high"]);
    let stored = SavedDefault {
        model: None,
        provider: None,
        effort: Some(Effort::High),
    };
    let outcome = talaria_app::models::prepare_saved_launch(
        &client,
        fresh(),
        Some(&stored),
        FlagsGiven {
            model_or_provider: false,
            effort: false,
        },
        &offered(&["model", "provider", "reasoning", "cwd"]),
    )
    .await;

    let creates = gateway.requests("session.create");
    assert_eq!(creates.len(), 2);
    assert_eq!(creates[0].params["reasoning"], json!("high"));
    assert!(creates[0].params.get("model").is_none());
    assert!(creates[0].params.get("provider").is_none());
    assert!(creates[1].params.get("reasoning").is_none());
    assert!(creates[1].params.get("model").is_none());
    assert!(creates[1].params.get("provider").is_none());
    assert_ne!(
        creates[0].params["request_id"],
        creates[1].params["request_id"]
    );
    assert_eq!(outcome.session_id, "local-session-1");
    assert!(
        outcome.notices.iter().any(|line| line == EFFORT_REFUSED),
        "notices: {:?}",
        outcome.notices
    );
}
