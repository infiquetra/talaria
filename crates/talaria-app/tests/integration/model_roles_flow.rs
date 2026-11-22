//! The model roles page of a profile Talaria is not connected to, edited from /profiles to a
//! saved change on every tab. The page runs over the real service against a standalone gateway
//! whose dashboard routes answer like Hermes and keep what was written.

#![allow(clippy::too_many_lines)]

use crate::mock_gateway::StandaloneGateway;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, LinkView, UserAction, update};
use talaria_core::model_roles::{ModalView, RolesInput, RolesKey, RolesView, Section};
use talaria_core::profiles::{GatewayView, HostTopology, KeyAction, ProfilesInput};

const SAVED: &str = "Saved. Applies to new sessions.";

fn catalogue() -> Value {
    json!({
        "model": "",
        "provider": "",
        "providers": [
            {"slug": "alpha-cloud", "name": "Alpha Cloud", "models": ["alpha-small", "alpha-large"], "authenticated": true},
            {"slug": "beta-hub", "name": "Beta Hub", "models": ["beta-one", "beta-two"], "authenticated": true},
            {"slug": "gamma", "name": "Gamma", "models": [], "authenticated": false}
        ]
    })
}

/// What the fake dashboard holds for the edited profile.
struct Held {
    main: (String, String),
    tasks: Vec<Value>,
    moa: Value,
}

/// Dashboard routes that answer like Hermes and keep what was written, so read-backs see it.
fn serve_dashboard(gateway: &StandaloneGateway) {
    let held = Arc::new(Mutex::new(Held {
        main: ("alpha-cloud".to_owned(), "alpha-small".to_owned()),
        tasks: vec![json!({
            "task": "vision",
            "provider": "auto",
            "model": "",
            "base_url": "",
            "reasoning_effort": null,
        })],
        moa: json!({
            "default_preset": "first",
            "active_preset": "",
            "presets": {"first": {
                "enabled": true,
                "reference_models": [{"provider": "alpha-cloud", "model": "alpha-small"}],
                "aggregator": {"provider": "alpha-cloud", "model": "alpha-small"},
                "reference_temperature": null,
                "aggregator_temperature": null,
                "reference_timeout": null,
                "degraded_reference_policy": "loud",
                "fanout": "user_turn"
            }}
        }),
    }));
    // The profile already has presets of its own, so a save is not the first one.
    gateway.configure_rest_merge_raw(&json!({"moa": {"presets": {"first": {}}}}));
    gateway.configure_rest_route("GET", "/api/model/options", |_| (200, catalogue()));
    let state = held.clone();
    gateway.configure_rest_route("GET", "/api/model/info", move |_| {
        let held = state.lock().unwrap();
        (
            200,
            json!({
                "model": held.main.1,
                "provider": held.main.0,
                "auto_context_length": 0,
                "config_context_length": 0,
                "effective_context_length": 0,
                "capabilities": {}
            }),
        )
    });
    let state = held.clone();
    gateway.configure_rest_route("GET", "/api/model/auxiliary", move |_| {
        let held = state.lock().unwrap();
        (
            200,
            json!({
                "tasks": held.tasks,
                "main": {"provider": held.main.0, "model": held.main.1}
            }),
        )
    });
    let state = held.clone();
    gateway.configure_rest_route("POST", "/api/model/set", move |request| {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let provider = body["provider"].as_str().unwrap_or_default().to_owned();
        let model = body["model"].as_str().unwrap_or_default().to_owned();
        let mut held = state.lock().unwrap();
        if body["scope"] == "main" {
            held.main = (provider.clone(), model.clone());
            return (
                200,
                json!({"ok": true, "scope": "main", "provider": provider, "model": model,
                       "base_url": "", "gateway_tools": [], "stale_aux": []}),
            );
        }
        let task = body["task"].as_str().unwrap_or_default().to_owned();
        for slot in &mut held.tasks {
            if slot["task"] == task.as_str() {
                slot["provider"] = json!(provider);
                slot["model"] = json!(model);
                if let Some(effort) = body.get("reasoning_effort") {
                    slot["reasoning_effort"] = effort.clone();
                }
            }
        }
        (
            200,
            json!({"ok": true, "scope": "auxiliary", "tasks": [task], "provider": provider, "model": model}),
        )
    });
    let state = held.clone();
    gateway.configure_rest_route("GET", "/api/model/moa", move |_| {
        (200, state.lock().unwrap().moa.clone())
    });
    let state = held;
    gateway.configure_rest_route("PUT", "/api/model/moa", move |request| {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let mut held = state.lock().unwrap();
        held.moa = body.clone();
        let mut reply = body;
        reply["ok"] = json!(true);
        (200, reply)
    });
}

/// The app connected to `default`, with the page of `fixture` open from /profiles.
struct Flow {
    _home: tempfile::TempDir,
    /// Runs the page's calls. The gateway keeps a runtime of its own.
    runtime: tokio::runtime::Runtime,
    gateway: StandaloneGateway,
    state: AppState,
}

impl Flow {
    fn open() -> Self {
        let home = tempfile::tempdir().expect("a home");
        let gateway = StandaloneGateway::start(home.path());
        gateway.enable_configure_rest();
        serve_dashboard(&gateway);
        let mut state = AppState::new("model-roles-flow");
        state.profile = "default".into();
        state.hermes_home = gateway.home().to_path_buf();
        state.link_state = LinkView::Connected;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let mut flow = Self {
            _home: home,
            runtime,
            gateway,
            state,
        };
        flow.profiles(ProfilesInput::Open);
        flow.profiles(ProfilesInput::Discovered(
            0,
            Ok(GatewayView {
                served: vec!["default".into(), "fixture".into()],
                parked: Vec::new(),
            }),
        ));
        flow.profiles(ProfilesInput::Listed(0, Err("no roster".into())));
        flow.profiles(ProfilesInput::Topology(
            0,
            Ok(HostTopology {
                stopped: Vec::new(),
                own_gateway: Some(Vec::new()),
                gateway_mode: Some("multiplex".into()),
                single_reason: None,
            }),
        ));
        // The connected profile is selected first; the edited one sorts below it.
        flow.send(AppMessage::User(UserAction::Profiles(ProfilesInput::Key(
            KeyAction::Down,
        ))));
        flow.send(AppMessage::User(UserAction::Profiles(ProfilesInput::Key(
            KeyAction::Models,
        ))));
        let page = flow.page();
        assert!(page.title.ends_with("fixture"), "{}", page.title);
        flow
    }

    fn profiles(&mut self, input: ProfilesInput) {
        let _ = update(&mut self.state, AppMessage::Profiles(input));
    }

    /// One message, then every model roles call it causes, run as the app's executor runs them,
    /// with each reply fed back until the page asks for nothing more.
    fn send(&mut self, message: AppMessage) {
        let mut queue: VecDeque<AppEffect> = update(&mut self.state, message).into();
        while let Some(effect) = queue.pop_front() {
            let AppEffect::ModelRoles {
                generation,
                profile,
                home,
                service,
                effects,
                ..
            } = effect
            else {
                continue;
            };
            let service = service.unwrap_or_else(|| {
                self.runtime
                    .block_on(talaria_app::model_roles::open_service(
                        home,
                        profile.clone(),
                    ))
                    .expect("the gateway is found")
            });
            for effect in effects {
                for input in self.runtime.block_on(service.execute(effect)) {
                    queue.extend(update(
                        &mut self.state,
                        AppMessage::ModelRoles {
                            generation,
                            profile: profile.clone(),
                            service: Some(service.clone()),
                            input,
                        },
                    ));
                }
            }
        }
    }

    fn key(&mut self, key: RolesKey) {
        self.send(AppMessage::User(UserAction::ModelRoles(RolesInput::Key(
            key,
        ))));
    }

    fn type_text(&mut self, text: &str) {
        for letter in text.chars() {
            self.key(RolesKey::Char(letter));
        }
    }

    fn page(&self) -> RolesView {
        self.state.view().model_roles.expect("the page is open")
    }

    /// Moves the cursor of the focused pane to the first row whose text holds `label`.
    fn select(&mut self, label: &str) {
        for _ in 0..40 {
            let page = self.page();
            let rows = page
                .detail
                .clone()
                .filter(|_| page.focus_detail)
                .unwrap_or(page.rows);
            if rows
                .iter()
                .any(|row| row.selected && row.text.contains(label))
            {
                return;
            }
            self.key(RolesKey::Down);
        }
        panic!("no row holds {label}: {:?}", self.page());
    }

    /// Chooses a provider, then a model, in the open route picker.
    fn choose_route(&mut self, provider: &str, model: &str) {
        let picker = self.page().picker.expect("the route picker is open");
        assert_eq!(picker.title, "Choose a provider");
        assert!(
            picker.rows.iter().any(|row| row.text == provider),
            "the picker lists {provider}: {picker:?}"
        );
        self.type_text(provider);
        self.key(RolesKey::Enter);
        let picker = self.page().picker.expect("the model step is open");
        assert_eq!(picker.title, format!("Choose a model - {provider}"));
        self.type_text(model);
        self.key(RolesKey::Enter);
        assert!(
            self.page().picker.is_none(),
            "choosing a model closes the picker"
        );
    }

    /// Chooses one effort word in the open effort choice.
    fn choose_effort(&mut self, word: &str) {
        for _ in 0..12 {
            let Some(ModalView::Choice {
                title,
                options,
                selected,
            }) = self.page().modal
            else {
                panic!("the effort choice is open: {:?}", self.page());
            };
            assert_eq!(title, "Effort");
            if options[selected] == word {
                self.key(RolesKey::Enter);
                return;
            }
            self.key(RolesKey::Down);
        }
        panic!("no effort {word}");
    }

    /// Opens the review, checks it lists each change, and applies it.
    fn review_and_save(&mut self, section: &str, changes: &[&str]) {
        self.key(RolesKey::Review);
        let Some(ModalView::Review { title, lines, .. }) = self.page().modal else {
            panic!("the review opens: {:?}", self.page());
        };
        assert_eq!(
            title,
            format!("Review changes to {section} - profile fixture")
        );
        let shown: Vec<String> = lines
            .iter()
            .map(|line| format!("{} {} {}", line.key, line.before, line.after))
            .collect();
        for change in changes {
            assert!(
                shown.iter().any(|line| line.contains(change)),
                "the review shows {change}: {shown:?}"
            );
        }
        self.key(RolesKey::Enter);
        let page = self.page();
        assert!(
            page.notice.iter().any(|line| line == SAVED),
            "the page reports the save: {page:?}"
        );
        assert!(page.modal.is_none(), "{page:?}");
    }

    /// Every write left on a ticket the gateway minted for `fixture`, never for the connection.
    fn assert_writes_ticketed_for_the_page(&self, wanted: &[(&str, &str)]) {
        let fixture = std::fs::canonicalize(self.gateway.home().join("profiles/fixture"))
            .expect("the fixture folder");
        let requests = self.gateway.configure_rest_recorded();
        let writes: Vec<_> = requests
            .iter()
            .filter(|request| request.method != "GET")
            .collect();
        for (method, path) in wanted {
            assert!(
                writes
                    .iter()
                    .any(|request| request.method == *method && request.path == *path),
                "{method} {path} was sent: {writes:?}"
            );
        }
        for request in requests {
            let ticket = request
                .headers
                .iter()
                .find(|(name, _)| name == "x-hermes-gateway-ticket")
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{} {} has a ticket", request.method, request.path));
            let minted = self
                .gateway
                .ticket_for(&ticket)
                .unwrap_or_else(|| panic!("the gateway minted the ticket of {}", request.path));
            assert_eq!(minted.purpose, "native-http", "{}", request.path);
            assert_eq!(
                std::path::Path::new(&minted.profile_id),
                fixture,
                "{} {} is ticketed for the edited profile",
                request.method,
                request.path
            );
        }
    }

    fn assert_no_socket_naming_the_page(&self) {
        assert!(
            self.gateway.requests("model.options").is_empty(),
            "the route list never goes over the connection's socket"
        );
    }
}

#[test]
fn main_tab_saves_a_model_and_effort_for_another_profile() {
    let mut flow = Flow::open();
    flow.select("Model");
    flow.key(RolesKey::Enter);
    flow.choose_route("Beta Hub", "beta-two");
    flow.key(RolesKey::Effort);
    flow.choose_effort("high");
    flow.review_and_save("Main", &["beta-two", "high"]);
    let rows: Vec<String> = flow.page().rows.into_iter().map(|row| row.text).collect();
    assert!(
        rows.iter().any(|row| row.contains("beta-two")),
        "the saved model is shown: {rows:?}"
    );
    flow.assert_writes_ticketed_for_the_page(&[("POST", "/api/model/set"), ("PUT", "/api/config")]);
    flow.assert_no_socket_naming_the_page();
}

#[test]
fn fallbacks_tab_saves_a_new_fallback_for_another_profile() {
    let mut flow = Flow::open();
    flow.key(RolesKey::Section(Section::Fallbacks));
    flow.key(RolesKey::Add);
    flow.choose_route("Beta Hub", "beta-one");
    flow.review_and_save("Fallbacks", &["beta-one"]);
    flow.assert_writes_ticketed_for_the_page(&[("PUT", "/api/config")]);
    flow.assert_no_socket_naming_the_page();
}

#[test]
fn helpers_tab_saves_a_route_and_effort_for_another_profile() {
    let mut flow = Flow::open();
    flow.key(RolesKey::Section(Section::Helpers));
    flow.select("Vision");
    flow.key(RolesKey::Enter);
    flow.choose_route("Beta Hub", "beta-one");
    flow.key(RolesKey::Effort);
    flow.choose_effort("low");
    flow.review_and_save("Helpers", &["beta-one", "low"]);
    flow.assert_writes_ticketed_for_the_page(&[("POST", "/api/model/set")]);
    flow.assert_no_socket_naming_the_page();
}

#[test]
fn subagents_tab_saves_a_route_and_effort_for_another_profile() {
    let mut flow = Flow::open();
    flow.key(RolesKey::Section(Section::Subagents));
    flow.select("Route");
    flow.key(RolesKey::Enter);
    flow.choose_route("Beta Hub", "beta-two");
    flow.key(RolesKey::Effort);
    flow.choose_effort("medium");
    flow.review_and_save("Subagents", &["beta-two", "medium"]);
    flow.assert_writes_ticketed_for_the_page(&[("PUT", "/api/config")]);
    flow.assert_no_socket_naming_the_page();
}

#[test]
fn mixture_of_agents_tab_saves_a_reference_and_effort_for_another_profile() {
    let mut flow = Flow::open();
    flow.key(RolesKey::Section(Section::Moa));
    flow.select("first");
    flow.key(RolesKey::Right);
    flow.select("alpha-small");
    flow.key(RolesKey::Enter);
    flow.choose_route("Beta Hub", "beta-one");
    flow.key(RolesKey::Effort);
    flow.choose_effort("xhigh");
    flow.review_and_save("Mixture of Agents", &["first: changed (reference_models)"]);
    let detail: Vec<String> = flow
        .page()
        .detail
        .unwrap_or_default()
        .into_iter()
        .map(|row| row.text)
        .collect();
    assert!(
        detail
            .iter()
            .any(|row| row.contains("beta-one") && row.contains("xhigh")),
        "the saved reference is shown: {detail:?}"
    );
    flow.assert_writes_ticketed_for_the_page(&[("PUT", "/api/model/moa")]);
    flow.assert_no_socket_naming_the_page();
}
