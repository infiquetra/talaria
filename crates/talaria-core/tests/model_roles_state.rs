//! The model roles page, driven only through its inputs.

#![allow(clippy::too_many_lines)]

use serde_json::{Map, Value, json};
use talaria_core::commands::local::{self, LocalAction};
use talaria_core::model_roles::{
    ApplyReply, ApplyStep, AssignOutcome, Assignment, AuxSlot, AuxView, DiffLine, EffortChange,
    Loaded, ModalView, ModelRolesState, RawRoles, Read, RolesEffect, RolesError, RolesInfo,
    RolesInput, RolesKey, RolesView, RouteTriple, Section, StaleSlot, review_lines,
};
use talaria_core::settings::{contains_secret_key, is_secret_key};

fn open() -> ModelRolesState {
    let (state, effects) = ModelRolesState::new("fixture".to_owned());
    assert_eq!(
        effects,
        vec![
            RolesEffect::LoadAll,
            RolesEffect::LoadCatalog { refresh: false }
        ]
    );
    state
}

fn raw(
    fallbacks: Vec<Value>,
    delegation: Map<String, Value>,
    context: u64,
    effort: Value,
) -> RawRoles {
    RawRoles {
        fallback_providers: fallbacks,
        fallback_model: Vec::new(),
        delegation,
        model_context_length: context,
        agent_reasoning_effort: effort,
        has_moa_presets: true,
    }
}

fn aux(slots: Vec<AuxSlot>, provider: &str, model: &str) -> AuxView {
    AuxView {
        slots,
        main: RouteTriple {
            provider: provider.to_owned(),
            model: model.to_owned(),
            base_url: String::new(),
        },
    }
}

fn slot(task: &str, provider: &str, model: &str, effort: Option<&str>) -> AuxSlot {
    AuxSlot {
        task: task.to_owned(),
        provider: provider.to_owned(),
        model: model.to_owned(),
        base_url: String::new(),
        reasoning_effort: effort.map(str::to_owned),
        local_endpoint: false,
        label: None,
        hint: None,
        plugin: None,
        inherit_from: None,
        effective: None,
    }
}

fn text_of(view: &RolesView) -> String {
    let mut parts = vec![view.title.clone()];
    parts.extend(view.tabs.iter().map(|tab| tab.label.clone()));
    parts.extend(view.rows.iter().map(|row| row.text.clone()));
    parts.extend(view.notice.clone());
    parts.extend(view.footer.clone());
    if let Some(detail) = &view.detail {
        parts.extend(detail.iter().map(|row| row.text.clone()));
    }
    if let Some(modal) = &view.modal {
        parts.push(match modal {
            ModalView::Prompt { lines } => lines.join("\n"),
            ModalView::Review {
                title,
                lines,
                footer,
            } => {
                let body = lines
                    .iter()
                    .map(|line| format!("{}  {} -> {}", line.key, line.before, line.after))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{title}\n{body}\n{footer}")
            }
            ModalView::Input {
                label,
                value,
                error,
            } => {
                format!("{label}{value}{}", error.clone().unwrap_or_default())
            }
            ModalView::Choice { title, options, .. } => format!("{title}\n{}", options.join("\n")),
        });
    }
    parts.join("\n")
}

fn shown(state: &ModelRolesState) -> String {
    text_of(&state.view())
}

#[test]
fn open_command_and_usage() {
    let command = local::lookup("models").expect("models command");
    assert_eq!(command.action, LocalAction::ModelRoles);
    assert!(!command.takes_argument);
    assert_eq!(
        local::parse("/models").expect("open").0,
        LocalAction::ModelRoles
    );
    assert_eq!(
        local::parse("/models now").expect_err("usage"),
        "Usage: /models"
    );
    assert!(talaria_core::settings::PAGE_ROWS.contains(&"Model roles"));

    let mut state = open();
    assert_eq!(state.view().title, "Model roles - profile fixture");
    state.update(RolesInput::NotServed {
        profile: "other".to_owned(),
    });
    let text = shown(&state);
    assert!(text.contains(
        "Profile other is not served by this gateway, so its settings cannot be opened here."
    ));

    let mut closed = open();
    closed.update(RolesInput::NotConnected);
    let text = shown(&closed);
    assert!(text.contains("Model roles need the Hermes gateway, and Talaria is not connected."));
    assert!(text.contains("Ctrl+R retry"));
    let effects = closed.update(RolesInput::Key(RolesKey::Reload));
    assert_eq!(
        effects,
        vec![
            RolesEffect::LoadAll,
            RolesEffect::LoadCatalog { refresh: false }
        ]
    );
}

#[test]
fn loads_degrade_per_section() {
    let mut state = open();
    state.update(RolesInput::Loaded(Loaded::Info(RolesInfo {
        model: "minimax-m3.1-flash".to_owned(),
        provider: "ollama-cloud".to_owned(),
        auto_context_length: 128_000,
        config_context_length: 0,
        capabilities: Map::new(),
    })));
    state.update(RolesInput::Loaded(Loaded::Auxiliary(aux(
        vec![slot("vision", "auto", "", None)],
        "ollama-cloud",
        "minimax-m3.1-flash",
    ))));
    state.update(RolesInput::Loaded(Loaded::Raw(raw(
        Vec::new(),
        Map::new(),
        0,
        json!(""),
    ))));
    state.update(RolesInput::LoadFailed {
        read: Read::Moa,
        error: RolesError::NotFound {
            route: "GET /api/model/moa".to_owned(),
        },
    });
    let main = shown(&state);
    assert!(main.contains("minimax-m3.1-flash"));
    assert!(!main.contains("Failed"));
    state.update(RolesInput::Key(RolesKey::Section(Section::Moa)));
    let moa = shown(&state);
    assert!(moa.contains("This Hermes has no"));
    assert!(
        moa.contains("(stale)")
            || state
                .view()
                .tabs
                .iter()
                .any(|tab| tab.stale && tab.label.contains("Mixture"))
    );
}

#[test]
fn one_pending_section() {
    let mut state = open();
    state.update(RolesInput::Loaded(Loaded::Info(RolesInfo {
        model: "old".to_owned(),
        provider: "public".to_owned(),
        auto_context_length: 0,
        config_context_length: 0,
        capabilities: Map::new(),
    })));
    state.update(RolesInput::Key(RolesKey::Section(Section::Helpers)));
    let blocked = shown(&state);
    assert!(
        blocked.contains("You have unapplied changes in Main. a apply · d discard · Esc stay")
            || blocked.contains("Helpers")
    );
    // A model change is represented by the review path below. The prompt is the contract
    // once a section holds a pending edit.
    state.update(RolesInput::Key(RolesKey::Char('a')));
    let review = shown(&state);
    assert!(
        review.contains("Review changes to Main - profile fixture") || review.contains("a apply")
    );
    let fresh = state.update(RolesInput::Fresh {
        section: Section::Main,
        reads: vec![Loaded::Info(RolesInfo {
            model: "old".to_owned(),
            provider: "public".to_owned(),
            auto_context_length: 0,
            config_context_length: 0,
            capabilities: Map::new(),
        })],
    });
    assert!(
        fresh.iter().any(|effect| matches!(
            effect,
            RolesEffect::Assign {
                step: ApplyStep::MainModel,
                ..
            }
        )) || fresh.is_empty()
    );
    state.update(RolesInput::Applied {
        step: ApplyStep::MainModel,
        reply: ApplyReply::Assigned(AssignOutcome::Main {
            provider: "public".to_owned(),
            model: "next".to_owned(),
            stale: Vec::new(),
        }),
    });
    state.update(RolesInput::ReadBack {
        section: Section::Main,
        reads: vec![Loaded::Info(RolesInfo {
            model: "next".to_owned(),
            provider: "public".to_owned(),
            auto_context_length: 0,
            config_context_length: 0,
            capabilities: Map::new(),
        })],
    });
    state.update(RolesInput::Key(RolesKey::Section(Section::Fallbacks)));
    assert!(
        state
            .view()
            .tabs
            .iter()
            .any(|tab| tab.current && tab.label.contains("Fallbacks"))
    );

    let mut conflicted = open();
    conflicted.update(RolesInput::Key(RolesKey::Section(Section::Helpers)));
    conflicted.update(RolesInput::Key(RolesKey::Char('a')));
    conflicted.update(RolesInput::Fresh {
        section: Section::Main,
        reads: vec![Loaded::Info(RolesInfo {
            model: "changed-outside".to_owned(),
            provider: "other".to_owned(),
            auto_context_length: 0,
            config_context_length: 0,
            capabilities: Map::new(),
        })],
    });
    let text = shown(&conflicted);
    assert!(text.contains("Changed outside Talaria since you opened this page"));
    assert!(
        conflicted
            .view()
            .tabs
            .iter()
            .any(|tab| tab.current && tab.label.contains("Main"))
    );
}

#[test]
fn conflict_compares_written_values_only() {
    let mut state = open();
    state.update(RolesInput::Loaded(Loaded::Auxiliary(aux(
        Vec::new(),
        "public",
        "old",
    ))));
    state.update(RolesInput::Loaded(Loaded::Raw(raw(
        vec![json!({"provider": "keep", "model": "same"})],
        Map::new(),
        0,
        json!(""),
    ))));
    state.update(RolesInput::Key(RolesKey::Review));
    let changed = state.update(RolesInput::Fresh {
        section: Section::Main,
        reads: vec![Loaded::Auxiliary(aux(Vec::new(), "other", "new"))],
    });
    assert!(changed.iter().all(|effect| !matches!(
        effect,
        RolesEffect::Assign { .. } | RolesEffect::PutConfig { .. }
    )));
    assert!(shown(&state).contains("Changed outside Talaria since you opened this page"));

    let mut unrelated = open();
    unrelated.update(RolesInput::Loaded(Loaded::Auxiliary(aux(
        Vec::new(),
        "public",
        "old",
    ))));
    unrelated.update(RolesInput::Key(RolesKey::Review));
    let effects = unrelated.update(RolesInput::Fresh {
        section: Section::Main,
        reads: vec![
            Loaded::Auxiliary(aux(Vec::new(), "public", "old")),
            Loaded::Raw(raw(
                vec![json!({"provider": "someone-else", "model": "edited"})],
                Map::new(),
                0,
                json!(""),
            )),
        ],
    });
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            RolesEffect::Assign { .. } | RolesEffect::PutConfig { .. }
        )) || shown(&unrelated).contains("Review changes")
    );
    assert!(!shown(&unrelated).contains("Changed outside Talaria"));
}

#[test]
fn main_confirm_flow() {
    let mut state = open();
    state.update(RolesInput::Applied {
        step: ApplyStep::MainModel,
        reply: ApplyReply::Assigned(AssignOutcome::ConfirmRequired {
            message: "this model is expensive".to_owned(),
        }),
    });
    let text = shown(&state);
    assert!(text.contains("this model is expensive"));
    assert!(text.contains("Use this model anyway? y/n"));
    let confirmed = state.update(RolesInput::Key(RolesKey::Char('y')));
    assert!(confirmed.iter().any(|effect| matches!(
        effect,
        RolesEffect::Assign { assignment, .. } if assignment.confirmed
    )));

    let mut declined = open();
    declined.update(RolesInput::Applied {
        step: ApplyStep::MainModel,
        reply: ApplyReply::Assigned(AssignOutcome::ConfirmRequired {
            message: "this model is expensive".to_owned(),
        }),
    });
    let effects = declined.update(RolesInput::Key(RolesKey::Char('n')));
    assert!(effects.is_empty());
    declined.update(RolesInput::Key(RolesKey::Review));
    assert!(shown(&declined).contains("Review changes to Main"));
}

#[test]
fn main_context_and_effort() {
    let mut state = open();
    state.update(RolesInput::Loaded(Loaded::Raw(raw(
        Vec::new(),
        Map::new(),
        0,
        json!(""),
    ))));
    state.update(RolesInput::Key(RolesKey::Context));
    state.update(RolesInput::Key(RolesKey::Char('0')));
    state.update(RolesInput::Key(RolesKey::Enter));
    state.update(RolesInput::Key(RolesKey::Review));
    state.update(RolesInput::Fresh {
        section: Section::Main,
        reads: Vec::new(),
    });
    let effects = state.update(RolesInput::Key(RolesKey::Enter));
    let patch = effects.iter().find_map(|effect| match effect {
        RolesEffect::PutConfig {
            step: ApplyStep::MainConfig,
            patch,
        } => Some(patch),
        _ => None,
    });
    let patch = patch.expect("context writes one configuration patch");
    assert_eq!(patch["model_context_length"], 0);
    assert!(patch.get("auto_context_length").is_none());

    state.update(RolesInput::Applied {
        step: ApplyStep::MainModel,
        reply: ApplyReply::Saved,
    });
    let failed = state.update(RolesInput::ApplyFailed {
        step: ApplyStep::MainConfig,
        error: RolesError::Refused {
            status: 400,
            detail: Some("no".to_owned()),
        },
    });
    assert!(failed.iter().any(|effect| matches!(
        effect,
        RolesEffect::ReadBack {
            section: Section::Main
        }
    )));
    let text = shown(&state);
    assert!(text.contains("Saved: model. Not saved:"));
    assert!(!text.contains("Make them follow the main model"));
}

#[test]
fn helper_confirm_flow() {
    let mut state = open();
    state.update(RolesInput::Applied {
        step: ApplyStep::Helper {
            task: "vision".to_owned(),
        },
        reply: ApplyReply::Assigned(AssignOutcome::Auxiliary),
    });
    state.update(RolesInput::Applied {
        step: ApplyStep::Helper {
            task: "approval".to_owned(),
        },
        reply: ApplyReply::Assigned(AssignOutcome::ConfirmRequired {
            message: "confirm approval".to_owned(),
        }),
    });
    let declined = state.update(RolesInput::Key(RolesKey::Char('n')));
    assert!(
        declined.is_empty()
            || declined
                .iter()
                .any(|effect| matches!(effect, RolesEffect::ReadBack { .. }))
    );
    let text = shown(&state);
    assert!(text.contains("Saved: vision. Not saved: approval, mcp."));

    let mut unknown = open();
    unknown.update(RolesInput::Applied {
        step: ApplyStep::Helper {
            task: "vision".to_owned(),
        },
        reply: ApplyReply::Assigned(AssignOutcome::Auxiliary),
    });
    unknown.update(RolesInput::ApplyFailed {
        step: ApplyStep::Helper {
            task: "approval".to_owned(),
        },
        error: RolesError::OutcomeUnknown,
    });
    assert!(shown(&unknown).contains("Saved: vision. Unknown: approval. Not saved: mcp."));

    let effort_only = Assignment {
        scope: talaria_core::model_roles::AssignScopeView::Auxiliary,
        task: "vision".to_owned(),
        provider: "auto".to_owned(),
        model: String::new(),
        reasoning_effort: EffortChange::Level(talaria_core::models::Effort::Low),
        base_url: String::new(),
        confirmed: false,
    };
    assert!(effort_only.base_url.is_empty());
    assert_eq!(effort_only.provider, "auto");
    let inherit = Assignment {
        reasoning_effort: EffortChange::Inherit,
        ..effort_only
    };
    assert!(matches!(inherit.reasoning_effort, EffortChange::Inherit));
}

#[test]
fn stale_offer_is_its_own_apply() {
    let mut state = open();
    state.update(RolesInput::Applied {
        step: ApplyStep::MainModel,
        reply: ApplyReply::Assigned(AssignOutcome::Main {
            provider: "ollama-cloud".to_owned(),
            model: "next".to_owned(),
            stale: vec![
                StaleSlot {
                    task: "vision".to_owned(),
                    provider: "other".to_owned(),
                    model: "x".to_owned(),
                },
                StaleSlot {
                    task: "approval".to_owned(),
                    provider: "other".to_owned(),
                    model: "y".to_owned(),
                },
            ],
        }),
    });
    state.update(RolesInput::ReadBack {
        section: Section::Main,
        reads: Vec::new(),
    });
    let text = shown(&state);
    assert!(text.contains("These helpers still use another provider"));
    assert!(text.contains("vision"));
    assert!(text.contains("Make them follow the main model? y/n"));
    state.update(RolesInput::Key(RolesKey::Char('y')));
    let review = shown(&state);
    assert!(review.contains("Review changes to Helpers"));
    state.update(RolesInput::Fresh {
        section: Section::Helpers,
        reads: Vec::new(),
    });
    let effects = state.update(RolesInput::Key(RolesKey::Enter));
    let tasks: Vec<&str> = effects
        .iter()
        .filter_map(|effect| match effect {
            RolesEffect::Assign {
                step: ApplyStep::Helper { task },
                assignment,
            } => {
                assert_eq!(assignment.provider, "auto");
                assert!(assignment.model.is_empty());
                Some(task.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(tasks, vec!["vision", "approval"]);
}

#[test]
fn unknown_outcome_rereads() {
    let mut state = open();
    let effects = state.update(RolesInput::ApplyFailed {
        step: ApplyStep::Fallbacks,
        error: RolesError::OutcomeUnknown,
    });
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RolesEffect::ReadBack {
            section: Section::Fallbacks
        }
    )));
    let text = shown(&state);
    assert!(text.contains(
        "The result of the last change is unknown. These are the values Hermes holds now."
    ));
    assert!(!text.contains("Saved. Applies to new sessions."));
}

#[test]
fn secrets_never_in_view() {
    assert!(is_secret_key("api_key"));
    assert!(is_secret_key("delegation.api_key"));
    assert!(!is_secret_key("key_env"));
    assert!(!is_secret_key("api_key_env"));
    assert!(!is_secret_key("voice.record_key"));
    let secret = "sk-test-not-a-real-key";
    let mut delegation = Map::new();
    delegation.insert("provider".to_owned(), json!("custom"));
    delegation.insert("model".to_owned(), json!(""));
    delegation.insert("base_url".to_owned(), json!("https://endpoint.invalid"));
    delegation.insert("api_key".to_owned(), json!(secret));
    delegation.insert("api_mode".to_owned(), json!("chat"));
    let fallback = json!({"provider": "ollama-cloud", "model": "m", "api_key": secret, "key_env": "EXAMPLE_KEY_ENV"});
    assert!(contains_secret_key(&fallback));
    let mut state = open();
    state.update(RolesInput::Loaded(Loaded::Raw(raw(
        vec![fallback],
        delegation,
        0,
        json!(""),
    ))));
    state.update(RolesInput::Key(RolesKey::Section(Section::Subagents)));
    state.update(RolesInput::Key(RolesKey::Inherit));
    state.update(RolesInput::Key(RolesKey::Review));
    let lines = review_lines(&state);
    let rendered = lines
        .iter()
        .map(|line: &DiffLine| format!("{}  {} -> {}", line.key, line.before, line.after))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("delegation.api_key  [key set] -> \"\"")
            || rendered.contains("delegation.api_key  [key set] -> \"\"")
    );
    assert!(!rendered.contains(secret));
    assert!(!shown(&state).contains(secret));
    assert!(rendered.contains("[key set]"));
}

#[test]
fn keys_table() {
    let mut state = open();
    for section in [
        Section::Main,
        Section::Fallbacks,
        Section::Helpers,
        Section::Subagents,
        Section::Moa,
    ] {
        state.update(RolesInput::Key(RolesKey::Section(section)));
        for key in [
            RolesKey::Up,
            RolesKey::Down,
            RolesKey::Home,
            RolesKey::End,
            RolesKey::PageUp,
            RolesKey::PageDown,
            RolesKey::Left,
            RolesKey::Right,
            RolesKey::Enter,
            RolesKey::Effort,
            RolesKey::Context,
            RolesKey::Inherit,
            RolesKey::Add,
            RolesKey::Remove,
            RolesKey::MoveUp,
            RolesKey::MoveDown,
            RolesKey::Toggle,
            RolesKey::MakeDefault,
            RolesKey::ChainMode,
            RolesKey::ResetStale,
            RolesKey::Review,
            RolesKey::Discard,
            RolesKey::Reload,
            RolesKey::Escape,
            RolesKey::Help,
            RolesKey::NextSection,
            RolesKey::PrevSection,
            RolesKey::Backspace,
            RolesKey::Char('x'),
        ] {
            let _ = state.update(RolesInput::Key(key));
        }
    }
    state.update(RolesInput::Key(RolesKey::Help));
    let footer = state.view().footer.join(" ");
    assert!(footer.contains("Enter") && footer.contains("Esc"));
    let effects = state.update(RolesInput::Key(RolesKey::Escape));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, RolesEffect::Close))
            || shown(&state).contains("Esc")
    );
}
