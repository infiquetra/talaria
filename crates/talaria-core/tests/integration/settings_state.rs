use serde_json::{Value, json};
use std::time::Duration;
use talaria_core::commands::local::LocalAction;
use talaria_core::settings::{
    ConfigSchemaView, EditKey, FailReason, FieldKind, FieldView, RuntimeChoice, RuntimeList,
    Screen, SettingsAction, SettingsEffect, SettingsInput, SettingsKey, SettingsLink,
    SettingsSnapshot, SettingsState, WriteResult,
};

static LINKS: &[SettingsLink] = &[SettingsLink {
    label: "Models",
    action: LocalAction::Model,
}];
fn field(key: &str, kind: FieldKind, category: &str) -> FieldView {
    FieldView {
        key: key.into(),
        kind,
        description: key.into(),
        category: category.into(),
        options: None,
        searchable: false,
        clearable: false,
    }
}
fn snapshot(effective: Value, raw: Value) -> SettingsSnapshot {
    SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![
                field("display.compact", FieldKind::Boolean, "display"),
                field("display.animations", FieldKind::Boolean, "display"),
                field("display.password", FieldKind::String, "display"),
            ],
            category_order: vec!["display".into()],
        },
        effective,
        raw,
        defaults: json!({"display":{"compact":false,"animations":false,"password":""}}),
    }
}
fn initial() -> Value {
    json!({"display":{"compact":false,"animations":false,"password":"example"}})
}
fn action(state: &mut SettingsState, key: SettingsAction) -> Vec<SettingsEffect> {
    state.update(SettingsInput::Key(SettingsKey::Action(key)))
}
fn ch(state: &mut SettingsState, key: char) -> Vec<SettingsEffect> {
    state.update(SettingsInput::Key(SettingsKey::Char(key)))
}
fn enter(state: &mut SettingsState) -> Vec<SettingsEffect> {
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Enter)))
}
fn escape(state: &mut SettingsState) -> Vec<SettingsEffect> {
    state.update(SettingsInput::Key(SettingsKey::Edit(EditKey::Esc)))
}
fn open(links: &'static [SettingsLink]) -> SettingsState {
    let mut state = SettingsState::new(links);
    assert_eq!(
        state.update(SettingsInput::Open {
            profile: "fixture".into(),
            filter: None
        }),
        vec![SettingsEffect::Load {
            profile: "fixture".into()
        }]
    );
    assert!(
        state
            .update(SettingsInput::Loaded(snapshot(initial(), json!({}))))
            .is_empty()
    );
    assert_eq!(state.view().screen, Screen::Browsing);
    state
}
fn review(state: &mut SettingsState, args: &str) {
    assert!(
        state
            .update(SettingsInput::Command {
                profile: "fixture".into(),
                args: args.into()
            })
            .is_empty()
    );
    assert_eq!(state.view().screen, Screen::Reviewing);
}
fn checking(state: &mut SettingsState, args: &str) {
    review(state, args);
    assert_eq!(enter(state), vec![SettingsEffect::FetchFresh]);
    assert_eq!(state.view().screen, Screen::Checking);
}
fn writing(state: &mut SettingsState) {
    checking(state, "display.compact=true");
    assert_eq!(
        state.update(SettingsInput::Fresh(initial())),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"compact":true}})
        }]
    );
    assert_eq!(state.view().screen, Screen::Applying);
}
fn banner(state: &SettingsState) -> String {
    state.view().banner.join("\n")
}
fn marker(state: &SettingsState, key: &str) -> String {
    state
        .view()
        .rows
        .iter()
        .find(|r| r.key == key)
        .expect("row")
        .marker
        .clone()
}

#[test]
fn open_load_browse_edit_review_apply_flow() {
    let mut state = open(LINKS);
    let view = state.view();
    assert_eq!(
        view.rows
            .iter()
            .find(|r| r.key == "display.compact")
            .unwrap()
            .origin,
        "default"
    );
    assert!(action(&mut state, SettingsAction::Edit).is_empty());
    assert!(action(&mut state, SettingsAction::Review).is_empty());
    assert_eq!(state.view().screen, Screen::Reviewing);
    assert_eq!(enter(&mut state), vec![SettingsEffect::FetchFresh]);
    assert_eq!(
        state.update(SettingsInput::Fresh(initial())),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"compact":true}})
        }]
    );
    assert_eq!(
        state.update(SettingsInput::Written(WriteResult::Ok)),
        vec![SettingsEffect::ReadBack]
    );
    let after = json!({"display":{"compact":true,"animations":false,"password":"example"}});
    assert!(
        state
            .update(SettingsInput::ReadBack {
                effective: after.clone(),
                raw: json!({"display":{"compact":true}})
            })
            .is_empty()
    );
    assert!(banner(&state).contains("Saved 1 change(s) for profile fixture."));
    assert!(banner(&state).contains("May need a gateway restart to take effect: display.compact."));
    assert_eq!(
        state
            .view()
            .rows
            .iter()
            .find(|r| r.key == "display.compact")
            .unwrap()
            .origin,
        "set"
    );
    let mut links_state = open(LINKS);
    assert!(links_state.view().categories.iter().any(|c| c == "More"));
    assert!(action(&mut links_state, SettingsAction::NextCategory).is_empty());
    assert_eq!(
        action(&mut links_state, SettingsAction::Edit),
        vec![
            SettingsEffect::CloseOverlay,
            SettingsEffect::OpenLocal(LocalAction::Model)
        ]
    );

    let values = json!({
        "model": "example", "model_context_length": 0, "fallback_providers": [],
        "agent": {"reasoning_effort": "medium"}, "delegation": {"enabled": true},
        "auxiliary": {"vision": {"model": "example"}}, "moa": {"enabled": false},
        "providers": {}, "custom_providers": []
    });
    for (key, kind, label) in [
        ("model", FieldKind::String, "edit in Model roles"),
        (
            "model_context_length",
            FieldKind::Number,
            "edit in Model roles",
        ),
        ("fallback_providers", FieldKind::List, "edit in Model roles"),
        (
            "agent.reasoning_effort",
            FieldKind::String,
            "edit in Model roles",
        ),
        (
            "delegation.enabled",
            FieldKind::Boolean,
            "edit in Model roles",
        ),
        (
            "auxiliary.vision.model",
            FieldKind::String,
            "edit in Model roles",
        ),
        ("moa.enabled", FieldKind::Boolean, "edit in Model roles"),
        ("providers", FieldKind::Object, "edit in Providers"),
        ("custom_providers", FieldKind::List, "edit in Providers"),
    ] {
        let mut readonly = SettingsState::new(&[]);
        assert_eq!(
            readonly.update(SettingsInput::Open {
                profile: "fixture".into(),
                filter: None
            }),
            vec![SettingsEffect::Load {
                profile: "fixture".into()
            }]
        );
        let selected = SettingsSnapshot {
            profile: "fixture".into(),
            schema: ConfigSchemaView {
                fields: vec![field(key, kind, "general")],
                category_order: vec!["general".into()],
            },
            effective: values.clone(),
            raw: json!({}),
            defaults: values.clone(),
        };
        assert!(readonly.update(SettingsInput::Loaded(selected)).is_empty());
        assert!(
            readonly.view().detail.contains(label),
            "incorrect ownership label for {key}"
        );
        assert!(action(&mut readonly, SettingsAction::Edit).is_empty());
        assert_eq!(readonly.view().screen, Screen::Browsing);
    }
}

#[test]
fn conflict_and_outside_changes() {
    let changed = json!({"display":{"compact":true,"animations":false,"password":"example"}});
    let mut s = open(&[]);
    checking(&mut s, "display.compact=true");
    assert!(s.update(SettingsInput::Fresh(changed.clone())).is_empty());
    assert_eq!(s.view().screen, Screen::Conflict);
    assert_eq!(
        ch(&mut s, 'o'),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"compact":true}})
        }]
    );

    let mut s = open(&[]);
    checking(&mut s, "display.compact=true");
    assert!(s.update(SettingsInput::Fresh(changed.clone())).is_empty());
    assert!(ch(&mut s, 't').is_empty());
    assert_eq!(s.view().screen, Screen::Browsing);
    assert!(banner(&s).contains("changed outside Talaria"));

    let mut s = open(&[]);
    checking(&mut s, "display.compact=true display.animations=true");
    assert!(s.update(SettingsInput::Fresh(changed.clone())).is_empty());
    assert!(ch(&mut s, 't').is_empty());
    assert_eq!(s.view().screen, Screen::Reviewing);
    assert_eq!(enter(&mut s), vec![SettingsEffect::FetchFresh]);
    assert_eq!(
        s.update(SettingsInput::Fresh(changed.clone())),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"animations":true}})
        }]
    );

    let mut s = open(&[]);
    checking(&mut s, "display.compact=true");
    assert!(s.update(SettingsInput::Fresh(changed.clone())).is_empty());
    assert!(escape(&mut s).is_empty());
    assert_eq!(s.view().screen, Screen::Reviewing);

    let mut s = open(&[]);
    checking(&mut s, "display.compact=true");
    let outside = json!({"display":{"compact":false,"animations":true,"password":"example"}});
    assert_eq!(
        s.update(SettingsInput::Fresh(outside)),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"compact":true}})
        }]
    );
    assert!(banner(&s).contains("changed outside Talaria"));

    let mut s = open(&[]);
    assert!(action(&mut s, SettingsAction::Edit).is_empty());
    assert_eq!(
        action(&mut s, SettingsAction::Reload),
        vec![SettingsEffect::Load {
            profile: "fixture".into()
        }]
    );
    assert!(
        s.update(SettingsInput::Loaded(snapshot(changed.clone(), json!({}))))
            .is_empty()
    );
    assert!(action(&mut s, SettingsAction::Review).is_empty());
    assert_eq!(enter(&mut s), vec![SettingsEffect::FetchFresh]);
    assert!(s.update(SettingsInput::Fresh(changed)).is_empty());
    assert_eq!(s.view().screen, Screen::Conflict);
}

#[test]
fn write_failures() {
    for (result, expected) in [
        (
            WriteResult::Refused {
                status: 400,
                detail: Some("bad value".into()),
            },
            "Hermes refused the change: bad value.",
        ),
        (WriteResult::Unauthorized, "Could not save:"),
        (
            WriteResult::NotSent(FailReason::NotConnected),
            "Could not save:",
        ),
        (
            WriteResult::Forbidden,
            "Talaria sent a request Hermes could not accept (HTTP 403).",
        ),
        (
            WriteResult::Refused {
                status: 422,
                detail: None,
            },
            "Talaria sent a request Hermes could not accept (HTTP 422).",
        ),
        (
            WriteResult::NotFound {
                route: "PUT /api/config".into(),
            },
            "This Hermes does not offer PUT /api/config (HTTP 404).",
        ),
    ] {
        let mut s = open(&[]);
        writing(&mut s);
        assert!(s.update(SettingsInput::Written(result)).is_empty());
        assert_eq!(s.view().screen, Screen::Reviewing);
        assert!(banner(&s).contains(expected), "{}", banner(&s));
    }
    for (result, message) in [
        (
            WriteResult::Server { status: 500 },
            "Hermes could not save (HTTP 500). Checking what was saved…",
        ),
        (
            WriteResult::Unknown,
            "The reply to the save was lost or unreadable. Checking what was saved…",
        ),
    ] {
        let mut s = open(&[]);
        writing(&mut s);
        assert_eq!(
            s.update(SettingsInput::Written(result)),
            vec![SettingsEffect::ReadBack]
        );
        assert!(banner(&s).contains(message));
        assert!(
            s.update(SettingsInput::ReadBack {
                effective: initial(),
                raw: json!({})
            })
            .is_empty()
        );
        assert!(banner(&s).contains("display.compact: not saved."));
        assert_eq!(marker(&s, "display.compact"), "!");
    }
    let mut s = open(&[]);
    writing(&mut s);
    assert_eq!(
        s.update(SettingsInput::Written(WriteResult::Ok)),
        vec![SettingsEffect::ReadBack]
    );
    assert!(
        s.update(SettingsInput::ReadBackFailed(FailReason::Dropped))
            .is_empty()
    );
    assert!(banner(&s).contains("Could not check what was saved:"));
    assert_eq!(marker(&s, "display.compact"), "?");
}

#[test]
fn managed_key_not_saved() {
    let mut s = open(&[]);
    writing(&mut s);
    assert_eq!(
        s.update(SettingsInput::Written(WriteResult::Ok)),
        vec![SettingsEffect::ReadBack]
    );
    assert!(
        s.update(SettingsInput::ReadBack {
            effective: initial(),
            raw: json!({})
        })
        .is_empty()
    );
    assert!(banner(&s).contains("Nothing was saved for profile fixture."));
    assert!(banner(&s).contains("Hermes kept the previous value; an administrator or a package-managed install may control this setting."));
    assert_eq!(marker(&s, "display.compact"), "!");
}

#[test]
fn one_apply_at_a_time_and_discard_prompt() {
    let mut s = open(&[]);
    writing(&mut s);
    for key in [
        SettingsKey::Char('a'),
        SettingsKey::Char('y'),
        SettingsKey::Edit(EditKey::Esc),
    ] {
        assert!(s.update(SettingsInput::Key(key)).is_empty());
        assert_eq!(s.view().screen, Screen::Applying);
    }
    assert!(s.view().footer.contains("Applying…"));
    let mut s = open(&[]);
    assert!(action(&mut s, SettingsAction::Edit).is_empty());
    assert!(action(&mut s, SettingsAction::Close).is_empty());
    assert_eq!(s.view().screen, Screen::ConfirmDiscard);
    assert!(s.view().footer.contains("Discard 1 unapplied change(s)?"));
    assert!(ch(&mut s, 'n').is_empty());
    assert_eq!(s.view().screen, Screen::Browsing);
    assert!(action(&mut s, SettingsAction::Close).is_empty());
    assert_eq!(ch(&mut s, 'y'), vec![SettingsEffect::CloseOverlay]);
}

#[test]
fn reveal_is_one_shot() {
    let mut s = open(&[]);
    assert!(action(&mut s, SettingsAction::Down).is_empty());
    assert!(action(&mut s, SettingsAction::Down).is_empty());
    let effects = action(&mut s, SettingsAction::Reveal);
    let id = match effects.as_slice() {
        [SettingsEffect::ScheduleReveal { id, after }] => {
            assert_eq!(*after, Duration::from_secs(10));
            *id
        }
        _ => panic!("expected reveal timer"),
    };
    assert!(s.view().detail.contains("example"));
    assert!(s.update(SettingsInput::RevealTimeout(id + 1)).is_empty());
    assert!(s.view().detail.contains("example"));
    assert!(s.update(SettingsInput::RevealTimeout(id)).is_empty());
    assert!(!s.view().detail.contains("example"));
    assert!(matches!(
        action(&mut s, SettingsAction::Reveal).as_slice(),
        [SettingsEffect::ScheduleReveal { .. }]
    ));
    assert!(action(&mut s, SettingsAction::Down).is_empty());
    assert!(!s.view().detail.contains("example"));

    let sentinel = "dummy-sensitive-value";
    let sensitive = json!({"display":{"compact":false,"animations":false,"password":sentinel}});
    let snap = snapshot(sensitive.clone(), json!({}));
    assert!(!format!("{snap:?}").contains(sentinel));
    let input = SettingsInput::Loaded(snapshot(sensitive.clone(), json!({})));
    assert!(!format!("{input:?}").contains(sentinel));
    let effect = SettingsEffect::Write {
        patch: json!({"display":{"password":sentinel}}),
    };
    assert!(!format!("{effect:?}").contains(sentinel));
    let mut debug_state = SettingsState::new(&[]);
    assert_eq!(
        debug_state.update(SettingsInput::Open {
            profile: "fixture".into(),
            filter: None
        }),
        vec![SettingsEffect::Load {
            profile: "fixture".into()
        }]
    );
    assert!(debug_state.update(SettingsInput::Loaded(snap)).is_empty());
    assert!(!format!("{debug_state:?}").contains(sentinel));

    let mut adjusted = open(&[]);
    assert!(action(&mut adjusted, SettingsAction::Down).is_empty());
    assert!(action(&mut adjusted, SettingsAction::Down).is_empty());
    assert!(action(&mut adjusted, SettingsAction::Edit).is_empty());
    assert!(
        adjusted
            .update(SettingsInput::Key(SettingsKey::Paste(
                "dummy-sent-value".into()
            )))
            .is_empty()
    );
    assert!(enter(&mut adjusted).is_empty());
    assert!(action(&mut adjusted, SettingsAction::Review).is_empty());
    let review = adjusted.view();
    assert!(!format!("{:?}", review.review_rows).contains("dummy-sent-value"));
    assert_eq!(enter(&mut adjusted), vec![SettingsEffect::FetchFresh]);
    assert_eq!(
        adjusted.update(SettingsInput::Fresh(initial())),
        vec![SettingsEffect::Write {
            patch: json!({"display":{"password":"dummy-sent-value"}})
        }]
    );
    assert_eq!(
        adjusted.update(SettingsInput::Written(WriteResult::Ok)),
        vec![SettingsEffect::ReadBack]
    );
    let after =
        json!({"display":{"compact":false,"animations":false,"password":"dummy-adjusted-value"}});
    assert!(
        adjusted
            .update(SettingsInput::ReadBack {
                effective: after,
                raw: json!({"display":{"password":"dummy-adjusted-value"}})
            })
            .is_empty()
    );
    assert!(banner(&adjusted).contains("display.password: saved as"));
    assert!(!banner(&adjusted).contains("dummy-adjusted-value"));
    assert!(
        adjusted
            .view()
            .rows
            .iter()
            .all(|row| !row.value.contains("dummy-adjusted-value"))
    );
    let close = action(&mut adjusted, SettingsAction::Close);
    assert!(
        close
            .iter()
            .any(|effect| matches!(effect, SettingsEffect::Notice(_)))
    );
    for effect in close {
        if let SettingsEffect::Notice(text) = effect {
            assert!(!text.contains("dummy-adjusted-value"));
            assert!(text.contains("(secret)"));
        }
    }
}

fn choice_field(key: &str, kind: FieldKind, options: Option<Vec<Value>>) -> FieldView {
    FieldView {
        options,
        ..field(key, kind, "general")
    }
}
fn chooser(field: FieldView, effective: Value, defaults: Value) -> SettingsState {
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![field],
            category_order: vec!["general".into()],
        },
        effective,
        raw: json!({}),
        defaults,
    }));
    // A key whose list differs per profile also asks for that list; nothing else is sent.
    let effects = action(&mut state, SettingsAction::Edit);
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, SettingsEffect::LoadChoices { .. })),
        "{effects:?}"
    );
    assert_eq!(state.view().screen, Screen::Choosing);
    state
}
fn choice_names(state: &SettingsState) -> Vec<String> {
    state.view().choices.into_iter().map(|c| c.name).collect()
}
fn selected_choice(state: &SettingsState) -> String {
    let view = state.view();
    view.choices[view.choice_cursor].name.clone()
}
fn edit_key(state: &mut SettingsState, key: EditKey) {
    state.update(SettingsInput::Key(SettingsKey::Edit(key)));
}
fn busy_input(current: &str) -> SettingsState {
    chooser(
        choice_field(
            "display.busy_input_mode",
            FieldKind::Select,
            Some(vec![json!("interrupt"), json!("queue"), json!("steer")]),
        ),
        json!({"display": {"busy_input_mode": current}}),
        json!({"display": {"busy_input_mode": "interrupt"}}),
    )
}

#[test]
fn chooser_opens_on_the_current_value() {
    let state = busy_input("queue");
    assert_eq!(selected_choice(&state), "queue (current)");
    assert_eq!(
        state.view().footer,
        "type to filter · ↑↓ move · PgUp PgDn page · Enter choose · Esc cancel"
    );
}

#[test]
fn chooser_marks_current_and_default_and_describes_every_row() {
    let state = busy_input("steer");
    let view = state.view();
    let rows: Vec<(String, String)> = view
        .choices
        .iter()
        .map(|c| (c.name.clone(), c.description.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "interrupt (default)".to_owned(),
                "Enter while a turn runs stops it and sends the new message.".to_owned()
            ),
            (
                "queue".to_owned(),
                "Enter while a turn runs waits and sends after the turn ends.".to_owned()
            ),
            (
                "steer (current)".to_owned(),
                "Enter while a turn runs adds the message to the running turn.".to_owned()
            ),
        ]
    );
}

#[test]
fn chooser_filters_by_description_and_saves_the_choice() {
    let mut state = busy_input("interrupt");
    for ch in "running turn".chars() {
        ch_key(&mut state, ch);
    }
    assert_eq!(choice_names(&state), ["steer"]);
    assert_eq!(state.view().choice_filter, "running turn");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "display.busy_input_mode"), "*");
}

fn ch_key(state: &mut SettingsState, key: char) {
    state.update(SettingsInput::Key(SettingsKey::Char(key)));
}

#[test]
fn chooser_shows_an_unlisted_current_value() {
    let mut state = chooser(
        choice_field("updates.pre_update_backup", FieldKind::String, None),
        json!({"updates": {"pre_update_backup": false}}),
        json!({"updates": {"pre_update_backup": "quick"}}),
    );
    assert_eq!(
        choice_names(&state),
        [
            "quick (default)",
            "full",
            "off",
            "false (current, not in the list)"
        ]
    );
    assert_eq!(selected_choice(&state), "false (current, not in the list)");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "updates.pre_update_backup"), "");
    let row = state.view().rows[0].value.clone();
    assert!(row.contains("(not in the list)"), "{row}");
}

#[test]
fn blank_value_uses_the_spec_label_and_merges_schema_options() {
    let state = chooser(
        choice_field(
            "memory.provider",
            FieldKind::Select,
            Some(vec![json!(""), json!("example-memory")]),
        ),
        json!({"memory": {"provider": ""}}),
        json!({"memory": {"provider": ""}}),
    );
    let names = choice_names(&state);
    assert_eq!(
        names,
        ["Built-in only (current) (default)", "example-memory"]
    );
    assert!(!names.iter().any(|n| n.contains("(clear)")));
}

#[test]
fn a_corrected_list_replaces_the_schema_list() {
    let mut state = chooser(
        choice_field(
            "terminal.modal_mode",
            FieldKind::Select,
            Some(vec![json!("sandbox"), json!("function")]),
        ),
        json!({"terminal": {"modal_mode": "auto"}}),
        json!({"terminal": {"modal_mode": "auto"}}),
    );
    assert_eq!(
        choice_names(&state),
        ["auto (current) (default)", "managed", "direct"]
    );
    let detail = state.view().detail;
    assert!(detail.contains("options auto, managed, direct"), "{detail}");
    assert!(!detail.contains("sandbox"), "{detail}");
    escape(&mut state);
    let row = state.view().rows[0].value.clone();
    assert!(!row.contains("not in the list"), "{row}");
}

#[test]
fn type_a_value_opens_the_text_editor() {
    let mut state = chooser(
        choice_field(
            "context.engine",
            FieldKind::Select,
            Some(vec![json!("default"), json!("custom")]),
        ),
        json!({"context": {"engine": "compressor"}}),
        json!({"context": {"engine": "compressor"}}),
    );
    assert_eq!(
        choice_names(&state),
        [
            "compressor (current) (default)",
            "Type a value…",
            "Loading the list…"
        ]
    );
    for ch in "example-engine".chars() {
        ch_key(&mut state, ch);
    }
    assert_eq!(choice_names(&state), ["Type a value…", "Loading the list…"]);
    enter(&mut state);
    let view = state.view();
    assert_eq!(view.screen, Screen::Editing);
    assert_eq!(view.editor.unwrap().text, "example-engine");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "context.engine"), "*");
    assert!(state.view().rows[0].value.contains("example-engine"));
}

#[test]
fn page_keys_and_wrapping_move_through_a_long_list() {
    let zones: Vec<Value> = (0..30)
        .map(|i| json!(format!("Zone/Place{i:02}")))
        .collect();
    let mut field = choice_field("timezone", FieldKind::Select, Some(zones));
    field.clearable = true;
    field.searchable = true;
    let mut state = chooser(field, json!({"timezone": ""}), json!({"timezone": ""}));
    assert_eq!(
        selected_choice(&state),
        "System time zone (current) (default)"
    );
    edit_key(&mut state, EditKey::PageDown);
    assert_eq!(selected_choice(&state), "Zone/Place09");
    edit_key(&mut state, EditKey::End);
    assert_eq!(selected_choice(&state), "Zone/Place29");
    edit_key(&mut state, EditKey::PageDown);
    assert_eq!(selected_choice(&state), "Zone/Place29");
    edit_key(&mut state, EditKey::Down);
    assert_eq!(
        selected_choice(&state),
        "System time zone (current) (default)"
    );
    edit_key(&mut state, EditKey::Up);
    assert_eq!(selected_choice(&state), "Zone/Place29");
    edit_key(&mut state, EditKey::PageUp);
    assert_eq!(selected_choice(&state), "Zone/Place19");
    edit_key(&mut state, EditKey::Home);
    assert_eq!(
        selected_choice(&state),
        "System time zone (current) (default)"
    );
}

#[test]
fn null_default_choice_marks_and_saves_json() {
    let mut state = chooser(
        choice_field("computer_use.no_overlay", FieldKind::String, None),
        json!({"computer_use": {"no_overlay": null}}),
        json!({"computer_use": {"no_overlay": null}}),
    );
    assert_eq!(
        choice_names(&state),
        ["Automatic (current) (default)", "true", "false"]
    );
    edit_key(&mut state, EditKey::Down);
    enter(&mut state);
    assert_eq!(state.view().rows[0].value, "true");
}

#[test]
fn config_command_sets_modal_mode_direct() {
    let mut state = chooser(
        choice_field(
            "terminal.modal_mode",
            FieldKind::Select,
            Some(vec![json!("sandbox"), json!("function")]),
        ),
        json!({"terminal": {"modal_mode": "auto"}}),
        json!({"terminal": {"modal_mode": "auto"}}),
    );
    escape(&mut state);
    review(&mut state, "terminal.modal_mode=direct");
    let rows = state.view().review_rows;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].new.contains("direct"), "{}", rows[0].new);
    let mut refused = chooser(
        choice_field(
            "terminal.modal_mode",
            FieldKind::Select,
            Some(vec![json!("sandbox"), json!("function")]),
        ),
        json!({"terminal": {"modal_mode": "auto"}}),
        json!({"terminal": {"modal_mode": "auto"}}),
    );
    escape(&mut refused);
    refused.update(SettingsInput::Command {
        profile: "fixture".into(),
        args: "terminal.modal_mode=sandbox".into(),
    });
    assert_eq!(
        banner(&refused),
        "terminal.modal_mode: Choose one of: auto, managed, direct."
    );
}

fn listed(value: &str, description: &str) -> RuntimeChoice {
    RuntimeChoice {
        value: value.into(),
        description: description.into(),
    }
}
/// Settings for one profile with the given string fields, in order.
fn browsing(keys: &[&str], effective: Value) -> SettingsState {
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: keys
                .iter()
                .map(|key| field(key, FieldKind::String, "general"))
                .collect(),
            category_order: vec!["general".into()],
        },
        effective: effective.clone(),
        raw: json!({}),
        defaults: effective,
    }));
    state
}
fn load_list(key: &str, list: RuntimeList) -> Vec<SettingsEffect> {
    vec![SettingsEffect::LoadChoices {
        key: key.into(),
        list,
        profile: "fixture".into(),
    }]
}
fn loaded(state: &mut SettingsState, key: &str, items: Vec<RuntimeChoice>) {
    state.update(SettingsInput::ChoicesLoaded {
        key: key.into(),
        profile: "fixture".into(),
        items,
    });
}

#[test]
fn runtime_list_shows_builtins_then_merges_the_answer() {
    let mut state = browsing(&["display.skin"], json!({"display": {"skin": "slate"}}));
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("display.skin", RuntimeList::Skins)
    );
    let names = choice_names(&state);
    assert_eq!(names.len(), 11, "{names:?}");
    assert_eq!(names[0], "default");
    assert_eq!(names[9], "Type a value…");
    assert_eq!(names[10], "Loading the list…");
    assert_eq!(selected_choice(&state), "slate (current) (default)");
    let footer = state.view().footer;
    assert!(footer.ends_with("· Ctrl+R reload list"), "{footer}");

    loaded(
        &mut state,
        "display.skin",
        vec![
            listed("default", "Fixture description of the default skin."),
            listed(
                "example-user-skin",
                "A skin from the profile's skins folder.",
            ),
        ],
    );
    let view = state.view();
    let rows: Vec<(String, String)> = view
        .choices
        .iter()
        .map(|c| (c.name.clone(), c.description.clone()))
        .collect();
    assert_eq!(rows.len(), 11, "{rows:?}");
    assert_eq!(
        rows[0],
        (
            "default".to_owned(),
            "Fixture description of the default skin.".to_owned()
        )
    );
    assert_eq!(
        rows[9],
        (
            "example-user-skin".to_owned(),
            "A skin from the profile's skins folder.".to_owned()
        )
    );
    assert_eq!(rows[10].0, "Type a value…");
    assert_eq!(selected_choice(&state), "slate (current) (default)");
    edit_key(&mut state, EditKey::End);
    edit_key(&mut state, EditKey::Up);
    assert_eq!(selected_choice(&state), "example-user-skin");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(state.view().rows[0].value, "\"example-user-skin\"");
}

#[test]
fn runtime_list_failure_keeps_the_chooser_usable_and_ctrl_r_retries() {
    let mut state = browsing(
        &["terminal.backend"],
        json!({"terminal": {"backend": "local"}}),
    );
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("terminal.backend", RuntimeList::TerminalBackends)
    );
    state.update(SettingsInput::ChoicesFailed {
        key: "terminal.backend".into(),
        profile: "fixture".into(),
        reason: "no reply within 15 s".into(),
    });
    let names = choice_names(&state);
    assert_eq!(
        names.last().unwrap(),
        "Could not load the list (no reply within 15 s). Showing built-in names. Ctrl+R retries."
    );
    assert_eq!(names[0], "local (current) (default)");
    assert_eq!(names[1], "docker");

    // Ctrl+R asks again and shows the loading row; it does not reload every setting.
    assert_eq!(
        action(&mut state, SettingsAction::Reload),
        load_list("terminal.backend", RuntimeList::TerminalBackends)
    );
    assert_eq!(choice_names(&state).last().unwrap(), "Loading the list…");
    assert_eq!(state.view().screen, Screen::Choosing);
    // A second Ctrl+R while the list loads sends nothing more.
    assert!(action(&mut state, SettingsAction::Reload).is_empty());

    edit_key(&mut state, EditKey::Down);
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(state.view().rows[0].value, "\"docker\"");
}

#[test]
fn late_answer_for_another_key_or_profile_is_dropped() {
    let mut state = browsing(
        &["display.personality", "display.skin"],
        json!({"display": {"personality": "", "skin": "default"}}),
    );
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("display.personality", RuntimeList::Personalities)
    );
    escape(&mut state);
    action(&mut state, SettingsAction::Down);
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("display.skin", RuntimeList::Skins)
    );
    // The personality answer arrives while the skin chooser is open.
    loaded(
        &mut state,
        "display.personality",
        vec![listed("example-persona", "A fixture personality.")],
    );
    // An answer for another profile is dropped too.
    state.update(SettingsInput::ChoicesLoaded {
        key: "display.skin".into(),
        profile: "other-profile".into(),
        items: vec![listed("example-foreign-skin", "Not this profile's.")],
    });
    let names = choice_names(&state);
    assert!(
        !names.iter().any(|n| n.starts_with("example-")),
        "{names:?}"
    );
    assert_eq!(names.last().unwrap(), "Loading the list…");

    // The dropped personality list is asked for again when its chooser reopens.
    escape(&mut state);
    action(&mut state, SettingsAction::Up);
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("display.personality", RuntimeList::Personalities)
    );
}

#[test]
fn a_loaded_list_is_kept_while_the_overlay_is_open() {
    let mut state = browsing(
        &["context.engine"],
        json!({"context": {"engine": "compressor"}}),
    );
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("context.engine", RuntimeList::ContextEngines)
    );
    loaded(
        &mut state,
        "context.engine",
        vec![listed("example-engine", "A fixture context engine.")],
    );
    assert_eq!(
        choice_names(&state),
        [
            "compressor (current) (default)",
            "example-engine",
            "Type a value…"
        ]
    );
    escape(&mut state);
    assert!(action(&mut state, SettingsAction::Edit).is_empty());
    assert_eq!(choice_names(&state)[1], "example-engine");
    escape(&mut state);
    // Reloading every setting forgets the lists too.
    action(&mut state, SettingsAction::Reload);
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![field("context.engine", FieldKind::String, "general")],
            category_order: vec!["general".into()],
        },
        effective: json!({"context": {"engine": "compressor"}}),
        raw: json!({}),
        defaults: json!({"context": {"engine": "compressor"}}),
    }));
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("context.engine", RuntimeList::ContextEngines)
    );
}

#[test]
fn personality_chooser_offers_no_overlay_and_the_builtins() {
    let mut state = browsing(
        &["display.personality"],
        json!({"display": {"personality": "none"}}),
    );
    action(&mut state, SettingsAction::Edit);
    let names = choice_names(&state);
    // Hermes reads "none" as no overlay, so a stored "none" selects that row.
    assert_eq!(names[0], "No personality (current) (default)");
    assert_eq!(names[1], "helpful");
    assert_eq!(names[14], "hype");
    loaded(
        &mut state,
        "display.personality",
        vec![listed(
            "example-persona",
            "A personality from the profile's config.",
        )],
    );
    let names = choice_names(&state);
    assert_eq!(names[15], "example-persona");
    assert_eq!(names[16], "Type a value…");
}

#[test]
fn kanban_profile_keys_list_the_served_profiles() {
    let mut state = browsing(
        &["kanban.default_assignee", "kanban.orchestrator_profile"],
        json!({"kanban": {"default_assignee": "", "orchestrator_profile": ""}}),
    );
    assert_eq!(
        action(&mut state, SettingsAction::Edit),
        load_list("kanban.default_assignee", RuntimeList::Profiles)
    );
    loaded(
        &mut state,
        "kanban.default_assignee",
        vec![listed("fixture", ""), listed("example-work", "")],
    );
    assert_eq!(
        choice_names(&state),
        [
            "Default profile (current) (default)",
            "fixture",
            "example-work",
            "Type a value…"
        ]
    );
    // Both kanban keys share one list of profiles.
    escape(&mut state);
    action(&mut state, SettingsAction::Down);
    assert!(action(&mut state, SettingsAction::Edit).is_empty());
    assert_eq!(choice_names(&state)[2], "example-work");
}

#[test]
fn a_value_from_a_runtime_list_is_not_called_unlisted() {
    let mut state = browsing(
        &["display.skin"],
        json!({"display": {"skin": "example-user-skin"}}),
    );
    let row = state.view().rows[0].value.clone();
    assert!(!row.contains("not in the list"), "{row}");
    action(&mut state, SettingsAction::Edit);
    loaded(
        &mut state,
        "display.skin",
        vec![listed("example-user-skin", "A user skin.")],
    );
    assert_eq!(
        selected_choice(&state),
        "example-user-skin (current) (default)"
    );
    escape(&mut state);
    let row = state.view().rows[0].value.clone();
    assert!(!row.contains("not in the list"), "{row}");
}
