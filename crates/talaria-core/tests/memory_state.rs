use std::time::{Duration, Instant};
use talaria_core::memory::*;

fn status() -> StatusView {
    StatusView {
        active: String::new(),
        providers: vec![ProviderRowView {
            name: "ready".into(),
            description: "Provider".into(),
            available: true,
            configured: true,
            status: "ready".into(),
            setup: ProviderSetup {
                pip_dependencies: vec![],
                python_dependencies_declared: false,
                external_dependencies: vec![],
                required_env: vec![],
                dependencies_installed: true,
            },
        }],
        builtin_files: BuiltinFiles {
            memory: 12,
            user: 0,
        },
    }
}
fn curator() -> CuratorView {
    CuratorView {
        enabled: true,
        paused: false,
        interval_hours: None,
        last_run_at: None,
        min_idle_hours: None,
        stale_after_days: None,
        archive_after_days: None,
    }
}
fn open() -> (MemoryState, Instant) {
    let now = Instant::now();
    let mut state = MemoryState::new();
    assert_eq!(
        state.update(
            MemoryInput::Open {
                profile: "fixture".into()
            },
            now
        ),
        vec![MemoryEffect::LoadStatus, MemoryEffect::LoadCurator]
    );
    state.update(MemoryInput::StatusLoaded(Ok(status())), now);
    state.update(MemoryInput::CuratorLoaded(Ok(curator())), now);
    (state, now)
}
fn key(s: &mut MemoryState, k: MemoryKey, now: Instant) -> Vec<MemoryEffect> {
    s.update(MemoryInput::Key(k), now)
}
fn curator_row(s: &mut MemoryState, now: Instant) {
    for _ in 0..4 {
        key(s, MemoryKey::Down, now);
    }
}
fn config_field(
    key: &str,
    kind: FieldKind,
    value: serde_json::Value,
    is_set: bool,
    required: bool,
) -> FieldView {
    FieldView {
        key: key.into(),
        label: key.into(),
        kind,
        description: String::new(),
        placeholder: String::new(),
        required,
        value,
        is_set,
        options: vec![],
        when: None,
        minimum: None,
        maximum: None,
    }
}
fn open_card(active: bool, fields: Vec<FieldView>) -> (MemoryState, Instant) {
    let (mut s, now) = open();
    if active {
        let mut st = status();
        st.active = "ready".into();
        s.update(MemoryInput::StatusLoaded(Ok(st)), now);
    }
    key(&mut s, MemoryKey::Down, now);
    assert_eq!(
        key(&mut s, MemoryKey::Open, now),
        vec![MemoryEffect::LoadConfig {
            name: "ready".into()
        }]
    );
    s.update(
        MemoryInput::ConfigLoaded {
            name: "ready".into(),
            result: Ok(ConfigView {
                name: "ready".into(),
                label: "Ready".into(),
                surface: ConfigSurface::Plain,
                fields,
            }),
        },
        now,
    );
    (s, now)
}
fn edit(s: &mut MemoryState, now: Instant, text: &str) {
    key(s, MemoryKey::Open, now);
    key(s, MemoryKey::Paste(text.into()), now);
    key(s, MemoryKey::Open, now);
}

#[test]
fn save_payload_contains_only_changes_and_requires_activation_confirmation() {
    let fields = vec![
        config_field("text", FieldKind::Text, serde_json::json!(""), false, false),
        config_field(
            "unset",
            FieldKind::Text,
            serde_json::json!(""),
            false,
            false,
        ),
    ];
    let (mut s, now) = open_card(false, fields);
    edit(&mut s, now, "  hello  ");
    assert!(key(&mut s, MemoryKey::Save, now).is_empty());
    let confirm = s.view(now).confirm.unwrap();
    assert_eq!(confirm.selected, 0);
    let text = confirm.lines.join(" ");
    assert!(text.contains("makes ready the memory provider"));
    assert!(text.contains("stores the default"));
    key(&mut s, MemoryKey::Right, now);
    let effects = key(&mut s, MemoryKey::Open, now);
    let [
        MemoryEffect::SaveConfig {
            name,
            surface,
            values,
        },
    ] = effects.as_slice()
    else {
        panic!("expected save")
    };
    assert_eq!(name, "ready");
    assert_eq!(*surface, ConfigSurface::Plain);
    assert_eq!(values.plain.len(), 1);
    assert_eq!(values.plain["text"], serde_json::json!("hello"));
    assert!(values.secrets.is_empty());
}

#[test]
fn active_save_skips_activation_confirmation_and_required_field_blocks() {
    let fields = vec![config_field(
        "text",
        FieldKind::Text,
        serde_json::json!(""),
        false,
        false,
    )];
    let (mut s, now) = open_card(true, fields);
    edit(&mut s, now, "value");
    assert!(matches!(
        key(&mut s, MemoryKey::Save, now).as_slice(),
        [MemoryEffect::SaveConfig { .. }]
    ));
    let fields = vec![
        config_field("text", FieldKind::Text, serde_json::json!(""), false, false),
        config_field(
            "required",
            FieldKind::Text,
            serde_json::json!(""),
            false,
            true,
        ),
    ];
    let (mut s, now) = open_card(true, fields);
    edit(&mut s, now, "value");
    assert!(key(&mut s, MemoryKey::Save, now).is_empty());
    assert!(
        s.view(now)
            .banner
            .unwrap()
            .contains("Fill in required first")
    );
}

#[test]
fn blank_secrets_and_unchanged_values_do_not_enter_save_payload() {
    let fields = vec![
        config_field(
            "unchanged",
            FieldKind::Text,
            serde_json::json!("same"),
            true,
            false,
        ),
        config_field(
            "secret",
            FieldKind::Secret,
            serde_json::json!(null),
            true,
            false,
        ),
        config_field(
            "flag",
            FieldKind::Bool,
            serde_json::json!(false),
            true,
            false,
        ),
    ];
    let (mut s, now) = open_card(true, fields);
    key(&mut s, MemoryKey::Open, now);
    key(&mut s, MemoryKey::Open, now);
    key(&mut s, MemoryKey::Down, now);
    key(&mut s, MemoryKey::Open, now);
    key(&mut s, MemoryKey::Open, now);
    key(&mut s, MemoryKey::Down, now);
    key(&mut s, MemoryKey::Open, now);
    key(&mut s, MemoryKey::Space, now);
    key(&mut s, MemoryKey::Open, now);
    let effects = key(&mut s, MemoryKey::Save, now);
    let [MemoryEffect::SaveConfig { values, .. }] = effects.as_slice() else {
        panic!("expected save")
    };
    assert_eq!(values.plain.len(), 1);
    assert_eq!(values.plain["flag"], serde_json::json!(true));
    assert!(values.secrets.is_empty());
}

#[test]
fn pending_edits_require_discard_before_opening_settings() {
    for (pressed, filter) in [
        (MemoryKey::Settings, "memory."),
        (MemoryKey::CuratorSettings, "curator."),
    ] {
        let (mut s, now) = open_card(
            true,
            vec![config_field(
                "text",
                FieldKind::Text,
                serde_json::json!(""),
                false,
                false,
            )],
        );
        edit(&mut s, now, "new");
        assert!(key(&mut s, pressed, now).is_empty());
        assert!(
            s.view(now)
                .confirm
                .unwrap()
                .lines
                .join(" ")
                .contains("Discard 1")
        );
        key(&mut s, MemoryKey::Right, now);
        assert_eq!(
            key(&mut s, MemoryKey::Open, now),
            vec![
                MemoryEffect::CloseOverlay,
                MemoryEffect::OpenSettings {
                    profile: "fixture".into(),
                    filter
                }
            ]
        );
    }
}

#[test]
fn open_draws_loading_then_loads_both() {
    let now = Instant::now();
    let mut s = MemoryState::new();
    assert_eq!(
        s.update(
            MemoryInput::Open {
                profile: "fixture".into()
            },
            now
        ),
        vec![MemoryEffect::LoadStatus, MemoryEffect::LoadCurator]
    );
    assert_eq!(
        s.view(now).banner.as_deref(),
        Some("Loading memory settings for profile fixture…")
    );
    s.update(
        MemoryInput::StatusLoaded(Err(FailText("offline".into()))),
        now,
    );
    s.update(MemoryInput::CuratorLoaded(Ok(curator())), now);
    assert!(matches!(s.view(now).providers, Section::Failed(_)));
    assert!(matches!(s.view(now).curator, Section::Ready(_)));
}

#[test]
fn provider_rows_keep_order_and_mark_the_active_one() {
    let (s, now) = open();
    let view = s.view(now);
    assert_eq!(view.active_text, "active: built-in only");
    let Section::Ready(rows) = view.providers else {
        panic!("provider rows not ready")
    };
    assert_eq!(rows[0].name, "built-in only");
    assert_eq!(rows[0].marker, '●');
    assert_eq!(rows[0].label, "in use");
    assert_eq!(rows[1].name, "ready");
    assert_eq!(rows[1].label, "ready");
}

#[test]
fn switch_requires_confirmation_and_matching_readback() {
    let (mut s, now) = open();
    assert!(key(&mut s, MemoryKey::Use, now).is_empty());
    assert!(s.view(now).banner.unwrap().contains("already in use"));
    key(&mut s, MemoryKey::Down, now);
    assert!(key(&mut s, MemoryKey::Use, now).is_empty());
    assert_eq!(s.view(now).confirm.unwrap().selected, 0);
    assert!(key(&mut s, MemoryKey::Open, now).is_empty());
    key(&mut s, MemoryKey::Use, now);
    key(&mut s, MemoryKey::Right, now);
    assert_eq!(
        key(&mut s, MemoryKey::Open, now),
        vec![MemoryEffect::SetProvider {
            name: "ready".into()
        }]
    );
    assert_eq!(
        s.update(
            MemoryInput::ProviderSet(WriteOutcome::Ok(
                serde_json::json!({"ok":true,"active":"ready"})
            )),
            now
        ),
        vec![MemoryEffect::LoadStatus]
    );
    let mut updated = status();
    updated.active = "ready".into();
    let effects = s.update(MemoryInput::StatusLoaded(Ok(updated)), now);
    assert!(effects.iter().any(|e| matches!(e, MemoryEffect::Notice(_))));
}

#[test]
fn delete_defaults_to_cancel_and_both_keeps_all_target() {
    let (mut s, now) = open();
    key(&mut s, MemoryKey::Down, now);
    key(&mut s, MemoryKey::Down, now);
    assert!(key(&mut s, MemoryKey::Delete, now).is_empty());
    assert_eq!(s.view(now).confirm.unwrap().selected, 0);
    assert!(key(&mut s, MemoryKey::Open, now).is_empty());
    key(&mut s, MemoryKey::DeleteBoth, now);
    let confirmation = s.view(now).confirm.unwrap();
    assert!(confirmation.lines.join(" ").contains("MEMORY.md"));
    assert!(!confirmation.lines.join(" ").contains("USER.md"));
    key(&mut s, MemoryKey::Right, now);
    assert_eq!(
        key(&mut s, MemoryKey::Open, now),
        vec![MemoryEffect::Reset {
            target: ResetTarget::All
        }]
    );
}

#[test]
fn settings_links_close_overlay_and_keep_profile() {
    for (pressed, filter) in [
        (MemoryKey::Settings, "memory."),
        (MemoryKey::CuratorSettings, "curator."),
    ] {
        let (mut s, now) = open();
        assert_eq!(
            key(&mut s, pressed, now),
            vec![
                MemoryEffect::CloseOverlay,
                MemoryEffect::OpenSettings {
                    profile: "fixture".into(),
                    filter
                }
            ]
        );
    }
}

#[test]
fn pause_is_deliberate_and_run_checks_before_writing() {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    assert_eq!(
        key(&mut s, MemoryKey::Pause, now),
        vec![MemoryEffect::SetPaused(true)]
    );
    assert!(key(&mut s, MemoryKey::Reload, now).is_empty());
    assert_eq!(s.view(now).banner.as_deref(), Some("Waiting for Hermes."));
    for pressed in [
        MemoryKey::Use,
        MemoryKey::Delete,
        MemoryKey::DeleteBoth,
        MemoryKey::Pause,
        MemoryKey::Run,
        MemoryKey::Save,
    ] {
        assert!(key(&mut s, pressed, now).is_empty());
        assert_eq!(s.view(now).banner.as_deref(), Some("Waiting for Hermes."));
    }
    let (mut s, now) = open();
    curator_row(&mut s, now);
    let effects = key(&mut s, MemoryKey::Run, now);
    assert!(
        matches!(effects.as_slice(),[MemoryEffect::PollAction { after, .. }] if *after == Duration::ZERO)
    );
}

#[test]
fn idle_view_does_not_change_with_time() {
    let (s, now) = open();
    assert_eq!(s.view(now), s.view(now + Duration::from_secs(10)));
}

#[test]
fn existing_curator_run_is_followed_without_starting_another() {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    let effects = key(&mut s, MemoryKey::Run, now);
    let [MemoryEffect::PollAction { follow, .. }] = effects.as_slice() else {
        panic!("expected pre-check")
    };
    let follow = *follow;
    let action = ActionView {
        name: "curator-run".into(),
        running: true,
        exit_code: None,
        pid: Some(42),
        lines: vec![],
    };
    let effects = s.update(
        MemoryInput::ActionPolled {
            follow,
            result: Ok(action),
        },
        now,
    );
    assert!(
        matches!(effects.as_slice(),[MemoryEffect::PollAction { follow: next, after }] if *next == follow && *after == Duration::from_millis(1200))
    );
    assert!(s.view(now).banner.unwrap().contains("already going"));
    assert!(s.view(now).following.is_some());
    let effects = key(&mut s, MemoryKey::Close, now);
    assert!(effects.iter().any(|e| matches!(e, MemoryEffect::Notice(_))));
    let stale = ActionView {
        name: "curator-run".into(),
        running: false,
        exit_code: Some(0),
        pid: Some(42),
        lines: vec![],
    };
    assert!(
        s.update(
            MemoryInput::ActionPolled {
                follow,
                result: Ok(stale)
            },
            now
        )
        .is_empty()
    );
}

#[test]
fn unknown_pause_outcome_only_reads_back() {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    key(&mut s, MemoryKey::Pause, now);
    assert_eq!(
        s.update(MemoryInput::PausedSet(WriteOutcome::Unknown), now),
        vec![MemoryEffect::LoadCurator]
    );
    assert!(s.view(now).banner.unwrap().contains("may or may not"));
}

#[test]
fn pause_resume_and_off_read_back() {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    assert_eq!(
        key(&mut s, MemoryKey::Pause, now),
        vec![MemoryEffect::SetPaused(true)]
    );
    assert_eq!(
        s.update(
            MemoryInput::PausedSet(WriteOutcome::Ok(
                serde_json::json!({"ok":true,"paused":true})
            )),
            now
        ),
        vec![MemoryEffect::LoadCurator]
    );
    let mut c = curator();
    c.paused = true;
    s.update(MemoryInput::CuratorLoaded(Ok(c.clone())), now);
    assert!(s.view(now).banner.unwrap().contains("Curator paused"));
    assert_eq!(
        key(&mut s, MemoryKey::Pause, now),
        vec![MemoryEffect::SetPaused(false)]
    );
    s.update(
        MemoryInput::PausedSet(WriteOutcome::Ok(
            serde_json::json!({"ok":true,"paused":false}),
        )),
        now,
    );
    c.paused = false;
    s.update(MemoryInput::CuratorLoaded(Ok(c.clone())), now);
    assert!(s.view(now).banner.unwrap().contains("Curator resumed"));
    c.enabled = false;
    s.update(MemoryInput::CuratorLoaded(Ok(c)), now);
    assert!(key(&mut s, MemoryKey::Pause, now).is_empty());
    assert!(s.view(now).banner.unwrap().contains("curator is off"));
    assert!(key(&mut s, MemoryKey::Run, now).is_empty());
}

fn following() -> (MemoryState, Instant, u64) {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    let effects = key(&mut s, MemoryKey::Run, now);
    let [MemoryEffect::PollAction { follow, .. }] = effects.as_slice() else {
        panic!("expected pre-check")
    };
    let follow = *follow;
    s.update(
        MemoryInput::ActionPolled {
            follow,
            result: Ok(ActionView {
                name: "curator-run".into(),
                running: true,
                exit_code: None,
                pid: Some(42),
                lines: vec![],
            }),
        },
        now,
    );
    (s, now, follow)
}
fn poll(
    s: &mut MemoryState,
    now: Instant,
    follow: u64,
    running: bool,
    pid: Option<i64>,
    exit_code: Option<i64>,
) -> Vec<MemoryEffect> {
    s.update(
        MemoryInput::ActionPolled {
            follow,
            result: Ok(ActionView {
                name: "curator-run".into(),
                running,
                exit_code,
                pid,
                lines: vec![],
            }),
        },
        now,
    )
}

#[test]
fn run_precheck_refuses_failure_and_confirms_when_idle() {
    let (mut s, now) = open();
    curator_row(&mut s, now);
    let effects = key(&mut s, MemoryKey::Run, now);
    let [MemoryEffect::PollAction { follow, .. }] = effects.as_slice() else {
        panic!("expected pre-check")
    };
    let follow = *follow;
    assert!(
        s.update(
            MemoryInput::ActionPolled {
                follow,
                result: Err(FailText("offline".into()))
            },
            now
        )
        .is_empty()
    );
    assert!(s.view(now).banner.unwrap().contains("Could not check"));
    let effects = key(&mut s, MemoryKey::Run, now);
    let [MemoryEffect::PollAction { follow, .. }] = effects.as_slice() else {
        panic!("expected pre-check")
    };
    let follow = *follow;
    poll(&mut s, now, follow, false, None, None);
    assert_eq!(s.view(now).confirm.unwrap().selected, 0);
    key(&mut s, MemoryKey::Right, now);
    assert_eq!(
        key(&mut s, MemoryKey::Open, now),
        vec![MemoryEffect::RunCurator]
    );
}

#[test]
fn run_endings_report_exit_unknown_and_replaced_process() {
    for (exit, expected) in [
        (Some(0), "Curator run finished."),
        (Some(1), "Curator run failed (exit 1)."),
        (None, "Hermes no longer reports this curator run"),
    ] {
        let (mut s, now, follow) = following();
        let effects = poll(&mut s, now, follow, false, Some(42), exit);
        assert!(s.view(now).banner.unwrap().contains(expected));
        assert!(effects.iter().any(|e| matches!(e, MemoryEffect::Notice(_))));
        assert!(effects.contains(&MemoryEffect::LoadCurator));
    }
    let (mut s, now, follow) = following();
    poll(&mut s, now, follow, true, Some(43), None);
    assert!(
        s.view(now)
            .banner
            .unwrap()
            .contains("Another curator run started")
    );
    assert!(s.view(now).following.is_none());
}

#[test]
fn following_stops_after_three_failures_or_the_poll_limit() {
    let (mut s, now, follow) = following();
    for _ in 0..2 {
        assert!(matches!(
            s.update(
                MemoryInput::ActionPolled {
                    follow,
                    result: Err(FailText("offline".into()))
                },
                now
            )
            .as_slice(),
            [MemoryEffect::PollAction { .. }]
        ));
    }
    s.update(
        MemoryInput::ActionPolled {
            follow,
            result: Err(FailText("offline".into())),
        },
        now,
    );
    assert!(s.view(now).banner.unwrap().contains("Lost track"));
    assert!(s.view(now).following.is_none());
    let (mut s, now, follow) = following();
    for _ in 0..240 {
        poll(&mut s, now, follow, true, Some(42), None);
    }
    assert!(s.view(now).banner.unwrap().contains("Still running"));
    assert!(s.view(now).following.is_none());
}

#[test]
fn pause_during_a_run_warns_that_it_continues() {
    let (mut s, now, _) = following();
    assert_eq!(
        key(&mut s, MemoryKey::Pause, now),
        vec![MemoryEffect::SetPaused(true)]
    );
    s.update(
        MemoryInput::PausedSet(WriteOutcome::Ok(
            serde_json::json!({"ok":true,"paused":true}),
        )),
        now,
    );
    let mut c = curator();
    c.paused = true;
    s.update(MemoryInput::CuratorLoaded(Ok(c)), now);
    assert!(
        s.view(now)
            .banner
            .unwrap()
            .contains("A run already going is not stopped.")
    );
}

#[test]
fn following_view_displays_only_latest_log_tail() {
    let (mut s, now, follow) = following();
    let lines = [
        "=== curator-run started old",
        "old",
        "=== curator-run started new",
        "fresh",
    ]
    .map(String::from)
    .to_vec();
    s.update(
        MemoryInput::ActionPolled {
            follow,
            result: Ok(ActionView {
                name: "curator-run".into(),
                running: true,
                exit_code: None,
                pid: Some(42),
                lines,
            }),
        },
        now,
    );
    assert_eq!(
        s.view(now + Duration::from_secs(3))
            .following
            .unwrap()
            .lines,
        vec!["fresh"]
    );
}

#[test]
fn unknown_writes_only_read_back_the_relevant_state() {
    let (mut s, now) = open();
    key(&mut s, MemoryKey::Down, now);
    key(&mut s, MemoryKey::Use, now);
    key(&mut s, MemoryKey::Right, now);
    key(&mut s, MemoryKey::Open, now);
    assert_eq!(
        s.update(MemoryInput::ProviderSet(WriteOutcome::Unknown), now),
        vec![MemoryEffect::LoadStatus]
    );
    assert!(s.view(now).banner.unwrap().contains("may or may not"));
    let (mut s, now) = open();
    for _ in 0..2 {
        key(&mut s, MemoryKey::Down, now);
    }
    key(&mut s, MemoryKey::Delete, now);
    key(&mut s, MemoryKey::Right, now);
    key(&mut s, MemoryKey::Open, now);
    assert_eq!(
        s.update(MemoryInput::ResetDone(WriteOutcome::Unknown), now),
        vec![MemoryEffect::LoadStatus]
    );
    let (mut s, now) = open_card(
        true,
        vec![config_field(
            "text",
            FieldKind::Text,
            serde_json::json!(""),
            false,
            false,
        )],
    );
    edit(&mut s, now, "new");
    key(&mut s, MemoryKey::Save, now);
    assert_eq!(
        s.update(MemoryInput::ConfigSaved(WriteOutcome::Unknown), now),
        vec![
            MemoryEffect::LoadConfig {
                name: "ready".into()
            },
            MemoryEffect::LoadStatus
        ]
    );
    let (mut s, now) = open();
    curator_row(&mut s, now);
    let effects = key(&mut s, MemoryKey::Run, now);
    let [MemoryEffect::PollAction { follow, .. }] = effects.as_slice() else {
        panic!("expected pre-check")
    };
    let follow = *follow;
    poll(&mut s, now, follow, false, None, None);
    key(&mut s, MemoryKey::Right, now);
    key(&mut s, MemoryKey::Open, now);
    let effects = s.update(MemoryInput::RunStarted(WriteOutcome::Unknown), now);
    let [MemoryEffect::PollAction { follow, after }] = effects.as_slice() else {
        panic!("expected read-back")
    };
    assert_eq!(*after, Duration::ZERO);
    let follow = *follow;
    poll(&mut s, now, follow, false, None, None);
    assert!(
        s.view(now)
            .banner
            .unwrap()
            .contains("Could not confirm that the curator run started")
    );
}

#[test]
fn switch_provider_flow() {
    switch_requires_confirmation_and_matching_readback();
    let (mut s, now) = open();
    let mut st = status();
    st.active = "ready".into();
    st.providers[0].status = "missing".into();
    s.update(MemoryInput::StatusLoaded(Ok(st)), now);
    key(&mut s, MemoryKey::Down, now);
    assert!(key(&mut s, MemoryKey::Use, now).is_empty());
    assert!(s.view(now).banner.unwrap().contains("already in use"));
    let mut st = status();
    st.providers[0].status = "missing".into();
    s.update(MemoryInput::StatusLoaded(Ok(st)), now);
    assert!(key(&mut s, MemoryKey::Use, now).is_empty());
    assert!(s.view(now).banner.unwrap().contains("not ready"));
    s.update(MemoryInput::StatusLoaded(Ok(status())), now);
    key(&mut s, MemoryKey::Use, now);
    key(&mut s, MemoryKey::Right, now);
    key(&mut s, MemoryKey::Open, now);
    s.update(
        MemoryInput::ProviderSet(WriteOutcome::Ok(
            serde_json::json!({"ok":true,"active":"ready"}),
        )),
        now,
    );
    s.update(MemoryInput::StatusLoaded(Ok(status())), now);
    assert!(s.view(now).banner.unwrap().contains("still built-in only"));
}

#[test]
fn save_settings_flow() {
    save_payload_contains_only_changes_and_requires_activation_confirmation();
    active_save_skips_activation_confirmation_and_required_field_blocks();
    let fields = vec![config_field(
        "text",
        FieldKind::Text,
        serde_json::json!(""),
        false,
        false,
    )];
    let (mut s, now) = open_card(true, fields);
    edit(&mut s, now, "new");
    key(&mut s, MemoryKey::Save, now);
    s.update(
        MemoryInput::ConfigSaved(WriteOutcome::Ok(
            serde_json::json!({"ok":true,"active":"ready"}),
        )),
        now,
    );
    s.update(
        MemoryInput::ConfigLoaded {
            name: "ready".into(),
            result: Ok(ConfigView {
                name: "ready".into(),
                label: "Ready".into(),
                surface: ConfigSurface::Plain,
                fields: vec![config_field(
                    "text",
                    FieldKind::Text,
                    serde_json::json!("new"),
                    true,
                    false,
                )],
            }),
        },
        now,
    );
    let mut st = status();
    st.active = "ready".into();
    s.update(MemoryInput::StatusLoaded(Ok(st)), now);
    let view = s.view(now);
    let card = view.card.unwrap();
    assert_eq!(card.result_lines[0], "text: saved");
}

#[test]
fn reset_flow_defaults_to_cancel() {
    delete_defaults_to_cancel_and_both_keeps_all_target();
    let (mut s, now) = open();
    for _ in 0..2 {
        key(&mut s, MemoryKey::Down, now);
    }
    key(&mut s, MemoryKey::Delete, now);
    key(&mut s, MemoryKey::Right, now);
    assert_eq!(
        key(&mut s, MemoryKey::Open, now),
        vec![MemoryEffect::Reset {
            target: ResetTarget::Memory
        }]
    );
    let (mut s, now) = open();
    for _ in 0..3 {
        key(&mut s, MemoryKey::Down, now);
    }
    assert!(key(&mut s, MemoryKey::Delete, now).is_empty());
    assert!(s.view(now).banner.unwrap().contains("already empty"));
}

#[test]
fn curator_pause_resume_and_off() {
    pause_resume_and_off_read_back();
    pause_during_a_run_warns_that_it_continues();
    let (mut s, now) = open();
    curator_row(&mut s, now);
    key(&mut s, MemoryKey::Pause, now);
    s.update(
        MemoryInput::PausedSet(WriteOutcome::Ok(
            serde_json::json!({"ok":true,"paused":true}),
        )),
        now,
    );
    s.update(MemoryInput::CuratorLoaded(Ok(curator())), now);
    assert!(s.view(now).banner.unwrap().contains("still active"));
}

#[test]
fn follows_a_curator_run() {
    run_precheck_refuses_failure_and_confirms_when_idle();
    existing_curator_run_is_followed_without_starting_another();
    run_endings_report_exit_unknown_and_replaced_process();
    following_stops_after_three_failures_or_the_poll_limit();
    following_view_displays_only_latest_log_tail();
    idle_view_does_not_change_with_time();
}

#[test]
fn one_write_at_a_time_and_unknown_outcome() {
    pause_is_deliberate_and_run_checks_before_writing();
    unknown_pause_outcome_only_reads_back();
    unknown_writes_only_read_back_the_relevant_state();
}

#[test]
fn settings_links_open_filtered_config() {
    settings_links_close_overlay_and_keep_profile();
    pending_edits_require_discard_before_opening_settings();
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(200))]
    #[test]
    fn modal_keys_cannot_accidentally_write(keys in proptest::collection::vec(0u8..9, 0..80)) {
        let (mut s,now) = open();
        key(&mut s,MemoryKey::Down,now); key(&mut s,MemoryKey::Use,now);
        for n in keys {
            let pressed = match n { 0 => MemoryKey::Use, 1 => MemoryKey::Delete, 2 => MemoryKey::DeleteBoth, 3 => MemoryKey::Pause, 4 => MemoryKey::Run, 5 => MemoryKey::Save, 6 => MemoryKey::Settings, 7 => MemoryKey::CuratorSettings, _ => MemoryKey::Reload };
            let effects = key(&mut s,pressed,now);
            proptest::prop_assert!(effects.is_empty());
            proptest::prop_assert!(s.view(now).confirm.is_some());
        }
    }

    #[test]
    fn never_writes_without_confirmation(keys in proptest::collection::vec(0u8..15, 0..100)) {
        let (mut s,now) = open(); let mut allowed = 0usize; let mut writes = 0usize;
        for n in keys {
            let pressed = match n { 0 => MemoryKey::Up, 1 => MemoryKey::Down, 2 => MemoryKey::Open, 3 => MemoryKey::Use, 4 => MemoryKey::Delete, 5 => MemoryKey::DeleteBoth, 6 => MemoryKey::Pause, 7 => MemoryKey::Run, 8 => MemoryKey::Save, 9 => MemoryKey::Left, 10 => MemoryKey::Right, 11 => MemoryKey::Tab, 12 => MemoryKey::Close, 13 => MemoryKey::Reload, _ => MemoryKey::Settings };
            if pressed == MemoryKey::Pause || (pressed == MemoryKey::Open && s.view(now).confirm.as_ref().is_some_and(|c| c.selected == 1)) { allowed += 1; }
            let effects = key(&mut s,pressed,now);
            writes += effects.iter().filter(|e| matches!(e,MemoryEffect::SetProvider { .. } | MemoryEffect::SaveConfig { .. } | MemoryEffect::Reset { .. } | MemoryEffect::SetPaused(_) | MemoryEffect::RunCurator)).count();
            proptest::prop_assert!(writes <= allowed);
        }
    }
}
