use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::json;
use talaria_app::configure::{ConfigureBackends, ConfigureService};
use talaria_core::configure::{
    ApplyStep, ConfigureEffect, ConfigureInput, Context, RowOutcome, StepResult,
};

#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

fn sibling_backends(
    origin: String,
    client: hermes_client::Client,
    consent: talaria_app::connectors::ConnectorsConsent,
) -> talaria_app::configure::SiblingBackends {
    let minter = Arc::new(mock_rest::CountingMinter::default());
    let factory: talaria_app::settings::MinterFactory = Arc::new(move |_| minter.clone());
    let served = vec!["fixture".into()];
    let settings = Arc::new(talaria_app::settings::SettingsService::new(
        Some(origin.clone()),
        served.clone(),
        factory.clone(),
    ));
    talaria_app::configure::SiblingBackends::new(
        settings,
        client,
        consent,
        Some(origin),
        served,
        factory,
    )
}

#[derive(Clone, Default)]
struct ScriptedBackends {
    calls: Arc<Mutex<Vec<String>>>,
}

impl ConfigureBackends for ScriptedBackends {
    async fn read_context(&self, profile: &str) -> Result<Context, String> {
        self.calls.lock().unwrap().push(format!("read:{profile}"));
        Err("schema unavailable".into())
    }

    async fn apply(&self, step: &ApplyStep) -> StepResult {
        let ApplyStep::Settings { rows, .. } = step else {
            panic!("unexpected step");
        };
        self.calls.lock().unwrap().push("apply:settings".into());
        StepResult::Rows(
            rows.iter()
                .map(|(row, _)| (*row, RowOutcome::Applied))
                .collect(),
        )
    }
}

#[tokio::test]
async fn gathers_context_through_sibling_services() {
    let backends = ScriptedBackends::default();
    let service = ConfigureService::new(None, backends.clone(), "default".into());
    let inputs = service.execute(ConfigureEffect::Gather).await;
    assert!(
        matches!(inputs.as_slice(), [ConfigureInput::GatherFailed(message)] if message == "schema unavailable")
    );
    assert_eq!(*backends.calls.lock().unwrap(), ["read:default"]);

    let mock = mock_rest::MockRest::start().await;
    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let consent = talaria_app::connectors::ConnectorsConsent::default();
    let siblings = sibling_backends(mock.origin(), client.clone(), consent.clone());
    let service = ConfigureService::new(Some(client.clone()), siblings, "fixture".into());
    let inputs = service.execute(ConfigureEffect::Gather).await;
    let [ConfigureInput::Gathered(context)] = inputs.as_slice() else {
        panic!("required settings reads must produce context");
    };
    assert_eq!(context.profile, "fixture");
    assert!(matches!(
        context.inventory.connectors,
        talaria_core::configure::Section::Unavailable(_)
    ));
    assert!(gateway.seen().iter().all(|frame| {
        !frame
            .method
            .as_deref()
            .is_some_and(|method| method.starts_with("connectors."))
    }));
    let reads = mock.requests().await;
    for target in [
        "/api/config/schema",
        "/api/config/defaults",
        "/api/config",
        "/api/config?include_defaults=false",
    ] {
        assert!(
            reads
                .iter()
                .any(|request| request.method == "GET" && request.path == target)
        );
    }

    consent.grant("fixture");
    let siblings = sibling_backends(mock.origin(), client.clone(), consent);
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let _ = service.execute(ConfigureEffect::Gather).await;
    assert!(gateway.seen().iter().any(|frame| {
        frame
            .method
            .as_deref()
            .is_some_and(|method| method.starts_with("connectors."))
    }));

    let rest = mock_rest::MockRest::start().await;
    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    gateway.script_error("cron.manage", None, 4001, "unavailable");
    let list: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configure/mcp_list.json")).unwrap();
    gateway.push_matched_result("mcp.servers.list", json!({}), list);
    let siblings = sibling_backends(
        rest.origin(),
        client.clone(),
        talaria_app::connectors::ConnectorsConsent::default(),
    );
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let inputs = service.execute(ConfigureEffect::Gather).await;
    let [ConfigureInput::Gathered(context)] = inputs.as_slice() else {
        panic!("one optional failure must not fail required settings");
    };
    assert!(matches!(
        context.inventory.cron_jobs,
        talaria_core::configure::Section::Unavailable(_)
    ));
    assert!(matches!(
        context.inventory.mcp_servers,
        talaria_core::configure::Section::Available(_)
    ));
    optional_sections_fail_independently().await;
}

async fn optional_sections_fail_independently() {
    use talaria_core::configure::Section;

    for failed in [
        "mcp_servers",
        "mcp_catalogue",
        "skills",
        "toolsets",
        "plugins",
        "cron_jobs",
        "providers",
        "main_model",
        "routes",
        "profiles",
        "connectors",
    ] {
        let rest = mock_rest::MockRest::start().await.allow_toolsets_profile()
            .with_route("GET", "/api/skills?profile=fixture", |_| (200, json!([])))
            .with_route("GET", "/api/tools/toolsets?profile=fixture", |_| (200, json!([])))
            .with_route("GET", "/api/env", |_| (200, json!({})))
            .with_route("GET", "/api/providers/oauth", |_| (200, json!({"providers":[]})))
            .with_route("GET", "/api/providers/custom-endpoints", |_| (200, json!({"endpoints":[]})))
            .with_route("GET", "/api/model/auxiliary", |_| (200, json!({"main":{"provider":"ollama-cloud","model":"minimax-m3.1-flash"},"tasks":[]})));
        let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
        for (method, result) in [
            ("mcp.servers.list", json!({"servers":[]})),
            ("mcp.catalog", json!({"servers":[]})),
            ("plugins.manage", json!({"plugins":[]})),
            ("cron.manage", json!({"success":true,"jobs":[]})),
            (
                "profiles.list",
                json!({"profiles":[{"name":"fixture","path":"","model":"minimax-m3.1-flash","provider":"ollama-cloud"}],"bot_mode_protocol":true,"install_id":""}),
            ),
            ("connectors.list", json!({"available":true,"connectors":[]})),
            ("connectors.catalog", json!({"connectors":[]})),
            ("connectors.accounts", json!({"accounts":[]})),
            (
                "connectors.policy.get",
                json!({"layers":[],"effective":{"version":1,"revision":"01J00000000000000000000000","issued_at_ms":1,"mode":"deny","disabled_connectors":[],"tools":{}}}),
            ),
        ] {
            gateway.push_method_result(method, result);
        }
        let target = match failed {
            "skills" => Some("/api/skills?profile=fixture"),
            "toolsets" => Some("/api/tools/toolsets?profile=fixture"),
            "main_model" => Some("/api/model/auxiliary"),
            _ => None,
        };
        if let Some(path) = target {
            rest.fail_status("GET", path, 500, Some("unavailable"))
                .await;
        }
        let method = match failed {
            "mcp_servers" => Some("mcp.servers.list"),
            "mcp_catalogue" => Some("mcp.catalog"),
            "plugins" => Some("plugins.manage"),
            "cron_jobs" => Some("cron.manage"),
            "profiles" => Some("profiles.list"),
            "connectors" => Some("connectors.list"),
            _ => None,
        };
        if let Some(method) = method {
            gateway.script_error(method, None, 4001, "unavailable");
        }
        if failed == "providers" || failed == "routes" {
            gateway.script_error_for_flag(
                "model.options",
                "explicit_only",
                failed == "routes",
                4001,
                "unavailable",
            );
        }
        let consent = talaria_app::connectors::ConnectorsConsent::default();
        consent.grant("fixture");
        let siblings = sibling_backends(rest.origin(), client.clone(), consent);
        let service = ConfigureService::new(Some(client), siblings, "fixture".into());
        let inputs = service.execute(ConfigureEffect::Gather).await;
        let [ConfigureInput::Gathered(context)] = inputs.as_slice() else {
            panic!("{failed}: required reads must still succeed")
        };
        let availability = [
            (
                "mcp_servers",
                matches!(context.inventory.mcp_servers, Section::Available(_)),
            ),
            (
                "mcp_catalogue",
                matches!(context.inventory.mcp_catalogue, Section::Available(_)),
            ),
            (
                "skills",
                matches!(context.inventory.skills, Section::Available(_)),
            ),
            (
                "toolsets",
                matches!(context.inventory.toolsets, Section::Available(_)),
            ),
            (
                "plugins",
                matches!(context.inventory.plugins, Section::Available(_)),
            ),
            (
                "cron_jobs",
                matches!(context.inventory.cron_jobs, Section::Available(_)),
            ),
            (
                "providers",
                matches!(context.inventory.providers, Section::Available(_)),
            ),
            (
                "main_model",
                matches!(context.inventory.main_model, Section::Available(_)),
            ),
            (
                "routes",
                matches!(context.inventory.routes, Section::Available(_)),
            ),
            (
                "profiles",
                matches!(context.inventory.profiles, Section::Available(_)),
            ),
            (
                "connectors",
                matches!(context.inventory.connectors, Section::Available(_)),
            ),
        ];
        for (section, available) in availability {
            assert_eq!(
                available,
                section != failed,
                "failure in {failed} changed {section}"
            );
        }
        for read in [
            "mcp.servers.list",
            "mcp.catalog",
            "plugins.manage",
            "cron.manage",
            "model.options",
            "profiles.list",
            "connectors.list",
        ] {
            assert!(
                !gateway.requests(read).is_empty(),
                "{failed}: missing {read}"
            );
        }
        let reads = rest.requests().await;
        for path in [
            "/api/skills?profile=fixture",
            "/api/tools/toolsets?profile=fixture",
            "/api/model/auxiliary",
        ] {
            assert!(
                reads
                    .iter()
                    .any(|request| request.method == "GET" && request.path == path),
                "{failed}: missing {path}"
            );
        }
        if let Some(method) = method {
            assert!(
                !gateway.requests(method).is_empty(),
                "failure injection was not reached: {failed}"
            );
        }
        if let Some(path) = target {
            assert!(
                rest.requests()
                    .await
                    .iter()
                    .any(|request| request.method == "GET" && request.path == path),
                "failure injection was not reached: {failed}"
            );
        }
        if failed == "providers" || failed == "routes" {
            assert!(
                gateway
                    .requests("model.options")
                    .iter()
                    .any(|frame| frame.params["explicit_only"] == (failed == "routes")),
                "failure injection was not reached: {failed}"
            );
        }
    }
}

#[tokio::test]
async fn apply_steps_reach_their_owners() {
    let backends = ScriptedBackends::default();
    let service = ConfigureService::new(None, backends.clone(), "default".into());
    let step = ApplyStep::Settings {
        rows: vec![
            (2, "display.compact".into()),
            (5, "display.bell_on_complete".into()),
        ],
        changes: BTreeMap::from([
            ("display.compact".into(), json!(true)),
            ("display.bell_on_complete".into(), json!(true)),
        ]),
        baseline: json!({"display": {"compact": false, "bell_on_complete": false}}),
    };
    let inputs = service
        .execute(ConfigureEffect::Apply { index: 4, step })
        .await;
    assert!(
        matches!(inputs.as_slice(), [ConfigureInput::StepDone { index: 4, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(2, RowOutcome::Applied), (5, RowOutcome::Applied)]))
    );
    assert_eq!(*backends.calls.lock().unwrap(), ["apply:settings"]);

    let rest = mock_rest::MockRest::start().await;
    rest.response_body("POST", "/api/model/set", br#"{"ok":true,"scope":"main","provider":"ollama-cloud","model":"minimax-m3.1-flash","base_url":"","gateway_tools":[],"stale_aux":[]}"#.to_vec()).await;
    rest.response_body(
        "GET",
        "/api/model/auxiliary",
        br#"{"tasks":[],"main":{"provider":"ollama-cloud","model":"other"}}"#.to_vec(),
    )
    .await;
    let (_gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let siblings = sibling_backends(
        rest.origin(),
        client.clone(),
        talaria_app::connectors::ConnectorsConsent::default(),
    );
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let _ = service.execute(ConfigureEffect::Gather).await;
    let model = || ApplyStep::MainModel {
        row: 0,
        provider: "ollama-cloud".into(),
        model: "minimax-m3.1-flash".into(),
        base_url: String::new(),
        baseline: talaria_core::configure::MainModelRow {
            provider: "ollama-cloud".into(),
            model: "other".into(),
        },
        confirmed: true,
    };
    let mismatch = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: model(),
        })
        .await;
    assert!(
        matches!(mismatch.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(0, RowOutcome::NotApplied(_))]))
    );
    let calls = rest.requests().await;
    assert!(
        calls
            .iter()
            .any(|request| request.method == "POST" && request.path == "/api/model/set")
    );
    assert!(
        calls
            .iter()
            .any(|request| request.method == "GET" && request.path == "/api/model/auxiliary")
    );
    rest.fail_status("GET", "/api/model/auxiliary", 500, None)
        .await;
    let failed = service
        .execute(ConfigureEffect::Apply {
            index: 1,
            step: model(),
        })
        .await;
    assert!(
        matches!(failed.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(0, RowOutcome::Unknown(_))]))
    );

    let rest = mock_rest::MockRest::start().await.with_route("POST", "/api/model/set", |request| {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        if body["confirm_expensive_model"] == true {
            (200, json!({"ok":true,"scope":"main","provider":"ollama-cloud","model":"minimax-m3.1-flash","base_url":"","gateway_tools":[],"stale_aux":[]}))
        } else {
            (200, json!({"ok":false,"scope":"main","provider":"ollama-cloud","model":"minimax-m3.1-flash","confirm_required":true,"confirm_message":"Confirm this model"}))
        }
    });
    rest.response_body(
        "GET",
        "/api/model/auxiliary",
        br#"{"tasks":[],"main":{"provider":"ollama-cloud","model":"minimax-m3.1-flash"}}"#.to_vec(),
    )
    .await;
    let (_gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let siblings = sibling_backends(
        rest.origin(),
        client.clone(),
        talaria_app::connectors::ConnectorsConsent::default(),
    );
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let _ = service.execute(ConfigureEffect::Gather).await;
    let model = |confirmed| ApplyStep::MainModel {
        row: 0,
        provider: "ollama-cloud".into(),
        model: "minimax-m3.1-flash".into(),
        base_url: rest.origin(),
        baseline: talaria_core::configure::MainModelRow {
            provider: "ollama-cloud".into(),
            model: "other".into(),
        },
        confirmed,
    };
    let first = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: model(false),
        })
        .await;
    assert!(matches!(
        first.as_slice(),
        [ConfigureInput::StepDone {
            result: StepResult::NeedsConfirm { row: 0, .. },
            ..
        }]
    ));
    let second = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: model(true),
        })
        .await;
    assert!(
        matches!(second.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(0, RowOutcome::Applied)]))
    );
    let calls = rest.requests().await;
    let writes: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, request)| request.method == "POST" && request.path == "/api/model/set")
        .collect();
    assert_eq!(writes.len(), 2);
    let before: serde_json::Value = serde_json::from_slice(&writes[0].1.body).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&writes[1].1.body).unwrap();
    assert_eq!(before["base_url"], rest.origin());
    assert_eq!(after["base_url"], rest.origin());
    assert_ne!(before["confirm_expensive_model"], true);
    assert_eq!(after["confirm_expensive_model"], true);
    assert!(
        calls[..writes[0].0]
            .iter()
            .any(|request| request.method == "GET" && request.path == "/api/model/auxiliary")
    );
    assert!(
        calls[writes[1].0 + 1..]
            .iter()
            .any(|request| request.method == "GET" && request.path == "/api/model/auxiliary")
    );
}

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

#[tokio::test]
async fn full_flow_on_the_mock_gateway() {
    use mock_gateway::MockGateway;
    use talaria_core::configure::CreateSpec;

    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.configure_session();
    let service =
        ConfigureService::new(Some(client), ScriptedBackends::default(), "default".into());
    let spec = CreateSpec {
        request_id: "request-1".into(),
        toolsets: vec!["clarify".into(), "no_mcp".into()],
        provider: None,
        model: None,
    };
    service.execute(ConfigureEffect::CreateSession(spec)).await;
    service
        .execute(ConfigureEffect::Submit {
            session_id: "session-1".into(),
            text: "the brief".into(),
            submission_id: "submission-1".into(),
        })
        .await;
    service
        .execute(ConfigureEffect::Detach {
            session_id: "session-1".into(),
            subscription_id: "subscription-1".into(),
        })
        .await;

    let requests: Vec<_> = gateway
        .seen()
        .into_iter()
        .filter(|frame| frame.method.is_some())
        .collect();
    let methods: Vec<_> = requests
        .iter()
        .filter_map(|frame| frame.method.as_deref())
        .collect();
    assert_eq!(
        methods,
        ["session.create", "prompt.submit", "session.detach"]
    );
    assert_eq!(
        requests[0].params,
        json!({
            "request_id": "request-1", "toolsets": ["clarify", "no_mcp"],
            "hidden": true, "ignore_rules": true, "max_turns": 8
        })
    );
    assert_eq!(
        requests[1].params,
        json!({
            "session_id": "session-1", "text": "the brief", "submission_id": "submission-1"
        })
    );
    assert_eq!(
        requests[2].params,
        json!({
            "session_id": "session-1", "subscription_id": "subscription-1", "end_reason": "configure_closed"
        })
    );
    gateway.assert_no_violations();

    let rest = mock_rest::MockRest::start().await;
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.configure_session();
    let consent = talaria_app::connectors::ConnectorsConsent::default();
    let siblings = sibling_backends(rest.origin(), client.clone(), consent);
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let gathered = service.execute(ConfigureEffect::Gather).await;
    let [ConfigureInput::Gathered(context)] = gathered.as_slice() else {
        panic!("settings context was not gathered");
    };
    let baseline = context.effective.clone();
    service
        .execute(ConfigureEffect::CreateSession(CreateSpec {
            request_id: "request-2".into(),
            toolsets: vec!["clarify".into(), "no_mcp".into()],
            provider: None,
            model: None,
        }))
        .await;
    service
        .execute(ConfigureEffect::Submit {
            session_id: "local-session-1".into(),
            text: "the brief".into(),
            submission_id: "submission-2".into(),
        })
        .await;
    let result = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: ApplyStep::Settings {
                rows: vec![(0, "display.compact".into())],
                changes: BTreeMap::from([("display.compact".into(), json!(false))]),
                baseline,
            },
        })
        .await;
    assert!(
        matches!(result.as_slice(), [ConfigureInput::StepDone { index: 0, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(0, RowOutcome::Applied)]))
    );
    let calls = rest.requests().await;
    let write = calls
        .iter()
        .position(|request| request.method == "PUT" && request.path == "/api/config")
        .expect("settings write");
    assert!(
        calls[write + 1..]
            .iter()
            .any(|request| request.method == "GET" && request.path == "/api/config")
    );
    let add: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configure/mcp_add.json")).unwrap();
    let list: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configure/mcp_list.json")).unwrap();
    gateway.push_matched_result(
        "mcp.servers.add",
        json!({"name":"deepwiki", "preset":"deepwiki"}),
        add,
    );
    gateway.push_matched_result("mcp.servers.list", json!({}), list);
    let result = service
        .execute(ConfigureEffect::Apply {
            index: 1,
            step: ApplyStep::McpAdd {
                row: 1,
                name: "deepwiki".into(),
                preset: Some("deepwiki".into()),
                config: None,
            },
        })
        .await;
    assert!(
        matches!(result.as_slice(), [ConfigureInput::StepDone { index: 1, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(1, RowOutcome::Applied)]))
    );
    service
        .execute(ConfigureEffect::Detach {
            session_id: "local-session-1".into(),
            subscription_id: "subscription-1".into(),
        })
        .await;
    let calls = gateway.seen();
    let methods: Vec<_> = calls
        .iter()
        .filter_map(|frame| frame.method.as_deref())
        .collect();
    let create = methods
        .iter()
        .position(|method| *method == "session.create")
        .unwrap();
    let submit = methods
        .iter()
        .position(|method| *method == "prompt.submit")
        .unwrap();
    let detach = methods
        .iter()
        .position(|method| *method == "session.detach")
        .unwrap();
    let add = methods
        .iter()
        .position(|method| *method == "mcp.servers.add")
        .unwrap();
    let list = methods
        .iter()
        .rposition(|method| *method == "mcp.servers.list")
        .unwrap();
    assert!(create < submit && submit < add && add < list && list < detach);
}

#[derive(Clone)]
struct OutcomeBackends {
    calls: Arc<Mutex<Vec<ApplyStep>>>,
    outcomes: Arc<Mutex<std::collections::VecDeque<StepResult>>>,
}

impl ConfigureBackends for OutcomeBackends {
    async fn read_context(&self, _profile: &str) -> Result<Context, String> {
        Err("unavailable".into())
    }

    async fn apply(&self, step: &ApplyStep) -> StepResult {
        self.calls.lock().unwrap().push(step.clone());
        self.outcomes.lock().unwrap().pop_front().unwrap()
    }
}

#[tokio::test]
async fn dropped_and_refused_steps() {
    use talaria_core::configure::MainModelRow;

    let backends = OutcomeBackends {
        calls: Arc::default(),
        outcomes: Arc::new(Mutex::new(std::collections::VecDeque::from([
            StepResult::NeedsConfirm {
                row: 1,
                message: "confirm".into(),
            },
            StepResult::Rows(vec![(
                1,
                RowOutcome::NotApplied("read-back differs".into()),
            )]),
            StepResult::Rows(vec![(
                2,
                RowOutcome::Failed("saved; do not create it again".into()),
            )]),
            StepResult::Rows(vec![(3, RowOutcome::Unknown("connection lost".into()))]),
        ]))),
    };
    let service = ConfigureService::new(None, backends.clone(), "default".into());
    let model = |confirmed| ApplyStep::MainModel {
        row: 1,
        provider: "provider".into(),
        model: "model".into(),
        base_url: "".into(),
        baseline: MainModelRow {
            provider: "provider".into(),
            model: "previous".into(),
        },
        confirmed,
    };
    let first = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: model(false),
        })
        .await;
    assert!(matches!(
        first.as_slice(),
        [ConfigureInput::StepDone {
            index: 0,
            result: StepResult::NeedsConfirm { row: 1, .. }
        }]
    ));
    let second = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: model(true),
        })
        .await;
    assert!(
        matches!(second.as_slice(), [ConfigureInput::StepDone { index: 0, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(1, RowOutcome::NotApplied(_))]))
    );
    let cron = ApplyStep::CronAdd {
        row: 2,
        name: "daily".into(),
        schedule: "0 9 * * *".into(),
        prompt: "hello".into(),
        repeat: None,
        deliver: "local".into(),
    };
    let failed = service
        .execute(ConfigureEffect::Apply {
            index: 1,
            step: cron,
        })
        .await;
    assert!(
        matches!(failed.as_slice(), [ConfigureInput::StepDone { index: 1, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(2, RowOutcome::Failed(_))]))
    );
    let mcp = ApplyStep::McpRemove {
        row: 3,
        name: "alpha".into(),
    };
    let unknown = service
        .execute(ConfigureEffect::Apply {
            index: 2,
            step: mcp,
        })
        .await;
    assert!(
        matches!(unknown.as_slice(), [ConfigureInput::StepDone { index: 2, result: StepResult::Rows(rows) }] if matches!(rows.as_slice(), [(3, RowOutcome::Unknown(_))]))
    );
    let calls = backends.calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(matches!(
        &calls[0],
        ApplyStep::MainModel {
            confirmed: false,
            ..
        }
    ));
    assert!(matches!(
        &calls[1],
        ApplyStep::MainModel {
            confirmed: true,
            ..
        }
    ));
    assert!(matches!(&calls[2], ApplyStep::CronAdd { deliver, .. } if deliver == "local"));
    assert!(matches!(&calls[3], ApplyStep::McpRemove { .. }));
    drop(calls);

    let rest = mock_rest::MockRest::start().await;
    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let siblings = sibling_backends(
        rest.origin(),
        client.clone(),
        talaria_app::connectors::ConnectorsConsent::default(),
    );
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let _ = service.execute(ConfigureEffect::Gather).await;
    let add: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/configure/mcp_add.json")).unwrap();
    gateway.push_matched_result(
        "mcp.servers.add",
        json!({"name":"deepwiki", "preset":"deepwiki"}),
        add,
    );
    gateway.push_matched_result("mcp.servers.list", json!({}), json!({"servers":[]}));
    let missing = service
        .execute(ConfigureEffect::Apply {
            index: 0,
            step: ApplyStep::McpAdd {
                row: 0,
                name: "deepwiki".into(),
                preset: Some("deepwiki".into()),
                config: None,
            },
        })
        .await;
    assert!(
        matches!(missing.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(0, RowOutcome::NotApplied(_))]))
    );
    assert_eq!(gateway.requests("mcp.servers.add").len(), 1);

    gateway.push_matched_result("cron.manage", json!({"action":"add", "name":"daily", "schedule":"0 9 * * *", "prompt":"hello", "deliver":"local"}),
        json!({"success":false, "error":"provider error", "job_id":"a1b2c3d4e5f6", "job_saved":true, "scheduler_registered":false, "retry_create":false}));
    let saved = service
        .execute(ConfigureEffect::Apply {
            index: 1,
            step: ApplyStep::CronAdd {
                row: 1,
                name: "daily".into(),
                schedule: "0 9 * * *".into(),
                prompt: "hello".into(),
                repeat: None,
                deliver: "local".into(),
            },
        })
        .await;
    assert!(
        matches!(saved.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(1, RowOutcome::Failed(message))] if message.contains("do not create")))
    );
    let adds: Vec<_> = gateway
        .requests("cron.manage")
        .into_iter()
        .filter(|frame| frame.params["action"] == "add")
        .collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0].params["deliver"], "local");

    use std::sync::atomic::{AtomicUsize, Ordering};
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = reads.clone();
    let rest = mock_rest::MockRest::start().await
        .with_route("GET", "/api/skills?profile=fixture", move |_| {
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                (200, json!([{"name":"fixture-skill-a", "description":"Synthetic fixture skill.", "category":"creative", "enabled":true, "usage":0, "provenance":"bundled"}]))
            } else {
                (200, json!([]))
            }
        })
        .with_route("PUT", "/api/skills/toggle", |_| (200, json!({"ok":true,"name":"fixture-skill-a","enabled":false})));
    let (_gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let siblings = sibling_backends(
        rest.origin(),
        client.clone(),
        talaria_app::connectors::ConnectorsConsent::default(),
    );
    let service = ConfigureService::new(Some(client), siblings, "fixture".into());
    let result = service
        .execute(ConfigureEffect::Apply {
            index: 2,
            step: ApplyStep::Capability {
                row: 2,
                change: talaria_core::configure::CapabilityChange::Skill {
                    name: "fixture-skill-a".into(),
                    enabled: false,
                },
            },
        })
        .await;
    assert!(
        matches!(result.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(2, RowOutcome::NotApplied(_))]))
    );
    let requests = rest.requests().await;
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == "PUT" && request.path == "/api/skills/toggle")
            .count(),
        1
    );
    assert!(
        requests
            .iter()
            .filter(
                |request| request.method == "GET" && request.path == "/api/skills?profile=fixture"
            )
            .count()
            >= 2
    );
    for drop_write in [true, false] {
        let counter = Arc::new(AtomicUsize::new(0));
        let rest = mock_rest::MockRest::start().await
            .with_route("GET", "/api/skills?profile=fixture", move |_| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    (200, json!([{"name":"fixture-skill-a", "description":"Synthetic fixture skill.", "category":"creative", "enabled":true, "usage":0, "provenance":"bundled"}]))
                } else { (502, json!({"detail":"Unavailable"})) }
            })
            .with_route("PUT", "/api/skills/toggle", |_| (200, json!({"ok":true,"name":"fixture-skill-a","enabled":false})));
        if drop_write {
            rest.drop_after_request("PUT", "/api/skills/toggle").await;
        }
        let (_gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
        let siblings = sibling_backends(
            rest.origin(),
            client.clone(),
            talaria_app::connectors::ConnectorsConsent::default(),
        );
        let service = ConfigureService::new(Some(client), siblings, "fixture".into());
        let result = service
            .execute(ConfigureEffect::Apply {
                index: 3,
                step: ApplyStep::Capability {
                    row: 3,
                    change: talaria_core::configure::CapabilityChange::Skill {
                        name: "fixture-skill-a".into(),
                        enabled: false,
                    },
                },
            })
            .await;
        assert!(
            matches!(result.as_slice(), [ConfigureInput::StepDone { result: StepResult::Rows(rows), .. }] if matches!(rows.as_slice(), [(3, RowOutcome::Unknown(_))]))
        );
        assert_eq!(
            rest.requests()
                .await
                .iter()
                .filter(|request| request.method == "PUT" && request.path == "/api/skills/toggle")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn brief_frame_and_logs_hold_no_secret() {
    use hermes_client::redact::scan_for_secrets;
    use hermes_client::{Recorder, RecorderOptions};
    use mock_gateway::MockGateway;
    use talaria_core::configure::{Inventory, Section, build_brief};
    use talaria_core::settings::ConfigSchemaView;

    fn detect(text: &str) -> bool {
        !scan_for_secrets(text).is_empty()
    }
    let sentinel = "sk-or-v1-talaria-sentinel-0000000000000000";
    let context = Context {
        profile: "default".into(),
        hermes_version: None,
        schema: ConfigSchemaView {
            fields: Vec::new(),
            category_order: Vec::new(),
        },
        effective: json!({}),
        raw: json!({"custom": sentinel}),
        defaults: json!({}),
        inventory: Inventory {
            mcp_servers: Section::Unavailable("unavailable".into()),
            mcp_catalogue: Section::Unavailable("unavailable".into()),
            skills: Section::Unavailable("unavailable".into()),
            toolsets: Section::Unavailable("unavailable".into()),
            plugins: Section::Unavailable("unavailable".into()),
            cron_jobs: Section::Unavailable("unavailable".into()),
            providers: Section::Unavailable("unavailable".into()),
            main_model: Section::Unavailable("unavailable".into()),
            routes: Section::Unavailable("unavailable".into()),
            profiles: Section::Unavailable("unavailable".into()),
            connectors: Section::Unavailable("unavailable".into()),
        },
    };
    assert!(detect(sentinel));
    let brief = build_brief(&context, "turn on the bell", detect);
    assert!(!brief.text.contains(sentinel));
    let temporary = tempfile::tempdir().unwrap();
    let recording = temporary.path().join("frames.jsonl");
    let recorder = Recorder::create(&recording, RecorderOptions::default()).unwrap();
    let (gateway, client, _inbound) =
        MockGateway::start_recorded("session-1", Some(recorder.clone())).await;
    let service =
        ConfigureService::new(Some(client), ScriptedBackends::default(), "default".into());
    service
        .execute(ConfigureEffect::Submit {
            session_id: "session-1".into(),
            text: brief.text,
            submission_id: "submission-1".into(),
        })
        .await;
    let requests = gateway.requests("prompt.submit");
    assert_eq!(requests.len(), 1);
    assert!(!format!("{:?}", requests[0]).contains(sentinel));
    assert!(!requests[0].params.to_string().contains(sentinel));
    let summary = recorder.finish().unwrap();
    assert!(summary.frames > 0);
    assert!(
        !std::fs::read_to_string(recording)
            .unwrap()
            .contains(sentinel)
    );
}

#[test]
fn brief_flow_process_output_holds_no_secret() {
    // This captures process output; events sent to a future subscriber need their own test.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "brief_frame_and_logs_hold_no_secret",
            "--nocapture",
        ])
        .output()
        .unwrap();
    let sentinel = "sk-or-v1-talaria-sentinel-0000000000000000";
    assert!(!String::from_utf8_lossy(&output.stdout).contains(sentinel));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(sentinel));
    assert!(
        output.status.success(),
        "secrecy flow failed in child process"
    );
}

#[test]
fn standalone_routes_settings_reads_to_the_existing_rest_mock() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let home = tempfile::tempdir().unwrap();
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    gateway.enable_configure_rest();
    let address = gateway.api_origin().strip_prefix("http://").unwrap();
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        socket,
        "GET /api/config/schema HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut reply = String::new();
    socket.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"));
    let requests = gateway.configure_rest_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (&*requests[0].0, &*requests[0].1),
        ("GET", "/api/config/schema")
    );
}

#[tokio::test]
async fn detach_after_link_loss_does_not_send_or_reconnect() {
    use hermes_client::Inbound;
    use mock_gateway::{MockGateway, next_inbound};

    let (gateway, client, mut inbound) = MockGateway::start().await;
    let service =
        ConfigureService::new(Some(client), ScriptedBackends::default(), "default".into());
    gateway.kill_connection().await;
    assert!(matches!(
        next_inbound(&mut inbound).await,
        Inbound::Closed(_)
    ));
    let before = gateway.frame_count();
    let inputs = service
        .execute(ConfigureEffect::Detach {
            session_id: "session-1".into(),
            subscription_id: "subscription-1".into(),
        })
        .await;
    assert!(matches!(inputs.as_slice(), [ConfigureInput::Detached]));
    assert_eq!(gateway.frame_count(), before);
    assert_eq!(gateway.connection_count(), 1);
}
