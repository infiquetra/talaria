use std::collections::BTreeSet;
use std::path::PathBuf;

use talaria_app::launch::NewSession;
use talaria_app::sessions::{
    create_params, detach_params, resume_params_id, resume_params_latest, submission_id,
    submit_params,
};

fn keys(value: &serde_json::Value) -> BTreeSet<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

#[test]
fn create_params_exact_keys() {
    let full = NewSession {
        request_id: "0123456789abcdef0123456789abcdef".to_string(),
        source: "tui".to_string(),
        cwd: PathBuf::from("/work/proj"),
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        reasoning: Some("high".to_string()),
    };
    let params = create_params(&full);
    assert_eq!(
        keys(&params),
        BTreeSet::from([
            "cwd".to_string(),
            "model".to_string(),
            "provider".to_string(),
            "reasoning".to_string(),
            "request_id".to_string(),
            "source".to_string(),
        ])
    );
    assert_eq!(params["source"], "tui");
    assert_eq!(params["cwd"], "/work/proj");
    assert!(params["cwd"].as_str().unwrap_or("").starts_with('/'));
    for absent in [
        "cwd_explicit",
        "title",
        "toolsets",
        "yolo",
        "api_key",
        "profile",
    ] {
        assert!(params.get(absent).is_none(), "{absent}");
    }

    let mut bare = full.clone();
    bare.model = None;
    bare.provider = None;
    bare.reasoning = None;
    let bare_params = create_params(&bare);
    assert_eq!(
        keys(&bare_params),
        BTreeSet::from([
            "cwd".to_string(),
            "request_id".to_string(),
            "source".to_string()
        ])
    );
}

#[test]
fn resume_detach_submit_params_exact_keys() {
    let latest = resume_params_latest(std::path::Path::new("/work/proj"));
    assert_eq!(
        keys(&latest),
        BTreeSet::from([
            "display_projection".to_string(),
            "latest".to_string(),
            "workspace".to_string(),
        ])
    );
    assert_eq!(latest["latest"], "tui");
    assert_eq!(latest["display_projection"], true);
    assert_eq!(latest["workspace"], "/work/proj");

    let by_id = resume_params_id(" local-session-1 ");
    assert_eq!(
        keys(&by_id),
        BTreeSet::from(["display_projection".to_string(), "session_id".to_string()])
    );
    assert_eq!(by_id["session_id"], "local-session-1");
    assert_eq!(by_id["display_projection"], true);

    let detach = detach_params("local-session-1", "sub-1");
    assert_eq!(
        keys(&detach),
        BTreeSet::from(["session_id".to_string(), "subscription_id".to_string()])
    );
    assert!(
        detach.get("end_reason").is_none(),
        "the key is absent, not null"
    );
    let typed = hermes_contract::schemas::SessionDetachParams {
        end_reason: None,
        session_id: "local-session-1".to_string(),
        subscription_id: "sub-1".to_string(),
    };
    assert_eq!(detach, serde_json::to_value(typed).expect("detach value"));

    let launch = "0123456789abcdef";
    let mut seen = BTreeSet::new();
    for n in 1..=1_000 {
        let id = submission_id(launch, n);
        assert!(seen.insert(id.clone()), "{id}");
        assert_eq!(id, format!("talaria-{launch}-{n}"));
    }
    let submit = submit_params("local-session-1", "hello", &submission_id(launch, 1));
    assert_eq!(
        keys(&submit),
        BTreeSet::from([
            "session_id".to_string(),
            "submission_id".to_string(),
            "text".to_string(),
        ])
    );
}
