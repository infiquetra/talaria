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
        "agent": {"reasoning_effort": "medium"}, "delegation": {"model": "example"},
        "auxiliary": {"vision": {"model": "example"}}, "moa": {"default_preset": "example"},
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
        ("delegation.model", FieldKind::String, "edit in Model roles"),
        (
            "auxiliary.vision.model",
            FieldKind::String,
            "edit in Model roles",
        ),
        (
            "moa.default_preset",
            FieldKind::String,
            "edit in Model roles",
        ),
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

#[test]
fn the_cursor_stays_on_the_current_value_when_its_list_arrives() {
    let mut state = browsing(
        &["display.skin"],
        json!({"display": {"skin": "example-zzz-skin"}}),
    );
    action(&mut state, SettingsAction::Edit);
    // Until the list arrives nobody knows the value is unlisted.
    assert_eq!(selected_choice(&state), "example-zzz-skin (current)");
    let items = vec![
        listed("example-aaa-skin", "Another user skin."),
        listed("example-zzz-skin", "A user skin."),
    ];
    loaded(&mut state, "display.skin", items.clone());
    assert_eq!(
        selected_choice(&state),
        "example-zzz-skin (current) (default)"
    );
    edit_key(&mut state, EditKey::Enter);
    assert_eq!(state.view().rows[0].value, "\"example-zzz-skin\"");

    // From the loading row the cursor moves to the value in force, and Enter chooses it.
    let mut state = browsing(
        &["display.skin"],
        json!({"display": {"skin": "example-zzz-skin"}}),
    );
    action(&mut state, SettingsAction::Edit);
    edit_key(&mut state, EditKey::End);
    assert_eq!(selected_choice(&state), "Loading the list…");
    loaded(&mut state, "display.skin", items);
    assert_eq!(
        selected_choice(&state),
        "example-zzz-skin (current) (default)"
    );
    edit_key(&mut state, EditKey::Enter);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(state.view().rows[0].value, "\"example-zzz-skin\"");

    // A value the arriving list lacks keeps its row and is now called unlisted.
    let mut state = browsing(
        &["display.skin"],
        json!({"display": {"skin": "example-zzz-skin"}}),
    );
    action(&mut state, SettingsAction::Edit);
    loaded(
        &mut state,
        "display.skin",
        vec![listed("example-aaa-skin", "Another user skin.")],
    );
    assert_eq!(
        selected_choice(&state),
        "example-zzz-skin (current, not in the list)"
    );
}

/// What applying the staged changes would send, read through the review screen.
fn staged_patch(state: &mut SettingsState, effective: Value) -> Value {
    action(state, SettingsAction::Review);
    assert_eq!(enter(state), vec![SettingsEffect::FetchFresh]);
    match state.update(SettingsInput::Fresh(effective)).as_slice() {
        [SettingsEffect::Write { patch }] => patch.clone(),
        other => panic!("expected a write, got {other:?}"),
    }
}
fn nested(key: &str, value: Value) -> Value {
    key.rsplit('.')
        .fold(value, |inner, part| json!({ part: inner }))
}
fn many_chooser(key: &str, kind: FieldKind, current: Value) -> (SettingsState, Value) {
    let doc = nested(key, current);
    (
        chooser(choice_field(key, kind, None), doc.clone(), doc.clone()),
        doc,
    )
}
fn move_to(state: &mut SettingsState, name: &str) {
    for _ in 0..60 {
        if selected_choice(state) == name {
            return;
        }
        edit_key(state, EditKey::Down);
    }
    panic!("no row {name:?} in {:?}", choice_names(state));
}
fn space(state: &mut SettingsState) {
    ch_key(state, ' ');
}

#[test]
fn space_toggles_and_enter_stages_the_set() {
    let (mut state, doc) = many_chooser("display.status_bar.fields", FieldKind::List, json!([]));
    let names = choice_names(&state);
    assert_eq!(names.len(), 22, "{names:?}");
    assert_eq!(names[0], "[x] Default set (empty list)");
    assert_eq!(names[1], "[ ] model");
    assert_eq!(names[12], "[ ] git_branch");
    assert!(
        state.view().footer.contains("Space tick"),
        "{}",
        state.view().footer
    );
    move_to(&mut state, "[ ] model");
    space(&mut state);
    move_to(&mut state, "[ ] git_branch");
    space(&mut state);
    let names = choice_names(&state);
    assert_eq!(names[0], "[ ] Default set (empty list)");
    assert_eq!(names[1], "[x] model");
    assert_eq!(names[12], "[x] git_branch");
    assert_eq!(state.view().screen, Screen::Choosing);
    // Nothing is staged until Enter.
    assert_eq!(marker(&state, "display.status_bar.fields"), "");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "display.status_bar.fields"), "*");
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"display": {"status_bar": {"fields": ["model", "git_branch"]}}})
    );
}

#[test]
fn a_status_bar_set_is_written_in_the_listed_order_whatever_order_it_was_ticked_in() {
    let (mut state, doc) = many_chooser("display.status_bar.fields", FieldKind::List, json!([]));
    move_to(&mut state, "[ ] git_branch");
    space(&mut state);
    move_to(&mut state, "[ ] model");
    space(&mut state);
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"display": {"status_bar": {"fields": ["model", "git_branch"]}}})
    );
}

#[test]
fn ordered_many_appends_new_picks_and_unticking_removes() {
    let (mut state, doc) = many_chooser(
        "display.runtime_footer.fields",
        FieldKind::List,
        json!(["model", "context_pct", "cwd"]),
    );
    assert_eq!(
        choice_names(&state),
        [
            "[x] model",
            "[x] context_pct",
            "[x] cwd",
            "[ ] latency",
            "[ ] served_model",
            "Edit as text…"
        ]
    );
    move_to(&mut state, "[ ] latency");
    space(&mut state);
    enter(&mut state);
    assert!(
        state.view().rows[0]
            .value
            .contains("[model, context_pct, cwd, latency]")
    );
    action(&mut state, SettingsAction::Edit);
    move_to(&mut state, "[x] context_pct");
    space(&mut state);
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"display": {"runtime_footer": {"fields": ["model", "cwd", "latency"]}}})
    );
}

#[test]
fn edit_as_text_reorders_an_ordered_list() {
    let (mut state, doc) = many_chooser(
        "display.runtime_footer.fields",
        FieldKind::List,
        json!(["model", "context_pct", "cwd"]),
    );
    move_to(&mut state, "Edit as text…");
    enter(&mut state);
    let view = state.view();
    assert_eq!(view.screen, Screen::Editing);
    assert_eq!(view.editor.unwrap().text, "model, context_pct, cwd");
    edit_key(&mut state, EditKey::ClearLine);
    for c in "cwd, model".chars() {
        ch_key(&mut state, c);
    }
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"display": {"runtime_footer": {"fields": ["cwd", "model"]}}})
    );
}

#[test]
fn default_set_row_clears_the_list() {
    let current = json!(["model", "goal"]);
    let (mut state, doc) = many_chooser(
        "display.status_bar.fields",
        FieldKind::List,
        current.clone(),
    );
    assert_eq!(choice_names(&state)[0], "[ ] Default set (empty list)");
    space(&mut state);
    let names = choice_names(&state);
    assert_eq!(names[0], "[x] Default set (empty list)");
    assert!(
        !names.iter().any(|n| n.starts_with("[x] model")),
        "{names:?}"
    );
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc.clone()),
        json!({"display": {"status_bar": {"fields": []}}})
    );
    // Enter keeps the ticks as they are; it does not press the row under the cursor, so opening
    // the list and pressing Enter never clears what was ticked.
    let (mut state, _) = many_chooser("display.status_bar.fields", FieldKind::List, current);
    enter(&mut state);
    assert_eq!(marker(&state, "display.status_bar.fields"), "");
}

#[test]
fn comma_store_keeps_string_type() {
    let (mut state, doc) = many_chooser("discord.server_actions", FieldKind::String, json!(""));
    let names = choice_names(&state);
    assert_eq!(names[0], "[x] All actions");
    assert_eq!(names[1], "[ ] list_guilds");
    assert_eq!(names.len(), 15);
    move_to(&mut state, "[ ] list_guilds");
    space(&mut state);
    move_to(&mut state, "[ ] server_info");
    space(&mut state);
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"discord": {"server_actions": "list_guilds, server_info"}})
    );
}

#[test]
fn a_listed_set_in_another_order_is_not_a_change() {
    let (mut state, _) = many_chooser(
        "display.status_bar.fields",
        FieldKind::List,
        json!(["goal", "model"]),
    );
    move_to(&mut state, "[ ] latency");
    space(&mut state);
    space(&mut state);
    enter(&mut state);
    assert_eq!(marker(&state, "display.status_bar.fields"), "");
}

#[test]
fn a_stored_name_the_list_lacks_stays_ticked_and_survives() {
    let (mut state, doc) = many_chooser(
        "display.status_bar.fields",
        FieldKind::List,
        json!(["model", "future_field"]),
    );
    let names = choice_names(&state);
    assert!(
        names.contains(&"[x] future_field (not in the list)".to_owned()),
        "{names:?}"
    );
    enter(&mut state);
    assert_eq!(marker(&state, "display.status_bar.fields"), "");
    action(&mut state, SettingsAction::Edit);
    move_to(&mut state, "[x] model");
    space(&mut state);
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"display": {"status_bar": {"fields": ["future_field"]}}})
    );
}

#[test]
fn escape_discards_the_ticks() {
    let (mut state, _) = many_chooser(
        "display.status_bar.fields",
        FieldKind::List,
        json!(["model"]),
    );
    move_to(&mut state, "[ ] goal");
    space(&mut state);
    escape(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "display.status_bar.fields"), "");
    action(&mut state, SettingsAction::Edit);
    assert!(choice_names(&state).contains(&"[ ] goal".to_owned()));
}

#[test]
fn typing_filters_a_many_list_but_space_still_ticks() {
    let (mut state, _) = many_chooser("display.status_bar.fields", FieldKind::List, json!([]));
    for c in "bg_".chars() {
        ch_key(&mut state, c);
    }
    assert_eq!(
        choice_names(&state),
        ["[ ] bg_tasks", "[ ] bg_processes", "[ ] bg_subagents"]
    );
    space(&mut state);
    assert_eq!(state.view().choice_filter, "bg_");
    assert_eq!(choice_names(&state)[0], "[x] bg_tasks");
}

#[test]
fn config_command_sets_a_multiple_choice_key() {
    let (mut state, _) = many_chooser("display.status_bar.fields", FieldKind::List, json!([]));
    escape(&mut state);
    review(&mut state, "display.status_bar.fields=model,goal");
    let rows = state.view().review_rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].new, "[model, goal]");
}

fn mixed_chooser(key: &str, current: Value, default: Value) -> (SettingsState, Value) {
    let doc = nested(key, current);
    (
        chooser(
            choice_field(key, FieldKind::String, None),
            doc.clone(),
            nested(key, default),
        ),
        doc,
    )
}

#[test]
fn a_word_or_own_list_chooser_lists_the_words_then_an_own_row() {
    let (mut state, doc) =
        mixed_chooser("agent.tool_use_enforcement", json!("auto"), json!("auto"));
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
                "auto (current) (default)".to_owned(),
                "Only for GPT and Codex models.".to_owned()
            ),
            ("true".to_owned(), "For every model.".to_owned()),
            ("false".to_owned(), "Never.".to_owned()),
            (
                "Own list…".to_owned(),
                "Model names to match, separated by commas, for example gpt, codex.".to_owned()
            ),
        ]
    );
    move_to(&mut state, "false");
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"agent": {"tool_use_enforcement": false}})
    );
}

#[test]
fn the_own_list_row_opens_the_text_editor_and_saves_an_array() {
    let (mut state, doc) =
        mixed_chooser("agent.tool_use_enforcement", json!("auto"), json!("auto"));
    move_to(&mut state, "Own list…");
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Editing);
    assert_eq!(state.view().editor.unwrap().text, "");
    for c in "gpt, codex".chars() {
        ch_key(&mut state, c);
    }
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"agent": {"tool_use_enforcement": ["gpt", "codex"]}})
    );
}

#[test]
fn a_current_own_list_shows_as_custom_and_opens_for_editing() {
    let (mut state, _) = mixed_chooser(
        "agent.tool_use_enforcement",
        json!(["gpt", "codex"]),
        json!("auto"),
    );
    assert_eq!(selected_choice(&state), "custom: gpt, codex (current)");
    enter(&mut state);
    assert_eq!(marker(&state, "agent.tool_use_enforcement"), "");
    action(&mut state, SettingsAction::Edit);
    move_to(&mut state, "Own list…");
    enter(&mut state);
    assert_eq!(state.view().editor.unwrap().text, "gpt, codex");
}

#[test]
fn memory_high_mb_chooser_takes_a_number_of_megabytes() {
    let (mut state, doc) = mixed_chooser(
        "agent.agent_cache.memory_high_mb",
        json!("auto"),
        json!("auto"),
    );
    assert_eq!(
        choice_names(&state),
        ["auto (current) (default)", "off", "Number of MB…"]
    );
    move_to(&mut state, "Number of MB…");
    enter(&mut state);
    for c in "2048".chars() {
        ch_key(&mut state, c);
    }
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"agent": {"agent_cache": {"memory_high_mb": 2048}}})
    );
}

#[test]
fn a_number_of_megabytes_already_stored_shows_as_custom() {
    let (state, _) = mixed_chooser(
        "agent.agent_cache.memory_high_mb",
        json!(1024),
        json!("auto"),
    );
    assert_eq!(selected_choice(&state), "custom: 1024 (current)");
}

#[test]
fn approvers_offer_the_wildcard_the_requester_only_row_and_an_own_list() {
    let (mut state, doc) = mixed_chooser("approvals.approvers", json!(["*"]), json!(["*"]));
    assert_eq!(
        choice_names(&state),
        [
            "* (current) (default)",
            "Requester and admins only",
            "Own list…"
        ]
    );
    move_to(&mut state, "Requester and admins only");
    enter(&mut state);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"approvals": {"approvers": []}})
    );
}

fn number_editor(key: &str, kind: FieldKind, current: Value) -> SettingsState {
    let (mut state, _) = mixed_number(key, kind, current);
    assert!(action(&mut state, SettingsAction::Edit).is_empty());
    assert_eq!(state.view().screen, Screen::Editing);
    state
}
fn mixed_number(key: &str, kind: FieldKind, current: Value) -> (SettingsState, Value) {
    let doc = nested(key, current);
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![field(key, kind, "general")],
            category_order: vec!["general".into()],
        },
        effective: doc.clone(),
        raw: json!({}),
        defaults: doc.clone(),
    }));
    (state, doc)
}
fn editor_text(state: &SettingsState) -> String {
    state.view().editor.expect("editor").text
}

#[test]
fn up_and_down_move_a_ranged_number_one_step_and_stop_at_the_ends() {
    let mut state = number_editor("wake_word.sensitivity", FieldKind::Number, json!(0.6));
    assert_eq!(editor_text(&state), "0.6");
    edit_key(&mut state, EditKey::Up);
    assert_eq!(editor_text(&state), "0.65");
    edit_key(&mut state, EditKey::Down);
    edit_key(&mut state, EditKey::Down);
    assert_eq!(editor_text(&state), "0.55");
    edit_key(&mut state, EditKey::ClearLine);
    for c in "1.0".chars() {
        ch_key(&mut state, c);
    }
    edit_key(&mut state, EditKey::Up);
    assert_eq!(editor_text(&state), "1.0");
    edit_key(&mut state, EditKey::ClearLine);
    ch_key(&mut state, '0');
    edit_key(&mut state, EditKey::Down);
    assert_eq!(editor_text(&state), "0.0");
}

#[test]
fn a_whole_number_steps_by_one_and_a_blank_one_starts_at_its_lowest() {
    let mut state = number_editor("compression.max_attempts", FieldKind::Number, json!(3));
    edit_key(&mut state, EditKey::Up);
    assert_eq!(editor_text(&state), "4");
    for _ in 0..20 {
        edit_key(&mut state, EditKey::Up);
    }
    assert_eq!(editor_text(&state), "10");
    let mut blank = number_editor("agent.max_turns", FieldKind::String, Value::Null);
    assert_eq!(editor_text(&blank), "");
    edit_key(&mut blank, EditKey::Up);
    assert_eq!(editor_text(&blank), "1");
    edit_key(&mut blank, EditKey::Down);
    assert_eq!(editor_text(&blank), "1");
    let mut ratio = number_editor("agent.budget_warning_ratio", FieldKind::String, Value::Null);
    edit_key(&mut ratio, EditKey::Up);
    assert_eq!(editor_text(&ratio), "0.05");
}

#[test]
fn the_number_editor_says_its_range_step_and_unit() {
    let state = number_editor("wake_word.sensitivity", FieldKind::Number, json!(0.6));
    let view = state.view();
    assert!(
        view.detail
            .contains("range 0.0 to 1.0 · step 0.05 · threshold"),
        "{}",
        view.detail
    );
    assert_eq!(
        view.footer,
        "Enter save edit · ↑↓ change by 0.05 · Esc cancel"
    );
    let state = number_editor("agent.run_budget_seconds", FieldKind::String, Value::Null);
    let detail = state.view().detail;
    assert!(detail.contains("whole number of seconds"), "{detail}");
    assert!(detail.contains("blank = off"), "{detail}");
    // A number with no stated range keeps the plain editor.
    let state = number_editor("agent.agent_cache.max_size", FieldKind::Number, json!(128));
    assert_eq!(state.view().footer, "Enter save edit · Esc cancel");
    assert!(
        !state.view().detail.contains("range"),
        "{}",
        state.view().detail
    );
}

#[test]
fn an_out_of_range_number_keeps_the_editor_open_with_the_range() {
    let mut state = number_editor("compression.threshold", FieldKind::Number, json!(0.5));
    edit_key(&mut state, EditKey::ClearLine);
    for c in "1.5".chars() {
        ch_key(&mut state, c);
    }
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Editing);
    assert_eq!(banner(&state), "Enter a number from 0.0 to 1.0.");
    edit_key(&mut state, EditKey::ClearLine);
    for c in "0.8".chars() {
        ch_key(&mut state, c);
    }
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(marker(&state, "compression.threshold"), "*");
}

#[test]
fn a_blank_number_saves_null() {
    let (mut state, doc) = mixed_number("agent.run_budget_seconds", FieldKind::String, json!(300));
    action(&mut state, SettingsAction::Edit);
    edit_key(&mut state, EditKey::ClearLine);
    enter(&mut state);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert_eq!(
        staged_patch(&mut state, doc),
        json!({"agent": {"run_budget_seconds": null}})
    );
}

#[test]
fn delegation_behaviour_keys_open_an_editor_and_route_keys_do_not() {
    let (mut state, _) = mixed_number(
        "delegation.max_concurrent_children",
        FieldKind::Number,
        json!(10),
    );
    action(&mut state, SettingsAction::Edit);
    assert_eq!(state.view().screen, Screen::Editing);
    let (mut state, _) = mixed_number("delegation.model", FieldKind::String, json!("example"));
    action(&mut state, SettingsAction::Edit);
    assert_eq!(state.view().screen, Screen::Browsing);
    assert!(!state.view().banner.is_empty());
}
