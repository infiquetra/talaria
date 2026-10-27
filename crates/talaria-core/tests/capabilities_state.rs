use talaria_core::capabilities::*;

struct Harness {
    reducer: talaria_core::capabilities::CapabilitiesState,
    now_ms: u64,
}
type CapabilitiesState = Harness;
impl Harness {
    fn new(profile: String, foreground: Option<SessionRef>, tab: CapTab) -> Self {
        Self {
            reducer: talaria_core::capabilities::CapabilitiesState::new(profile, foreground, tab),
            now_ms: 0,
        }
    }
    fn update(&mut self, input: CapInput, now_ms: u64) -> Vec<CapEffect> {
        self.now_ms = self.now_ms.max(now_ms);
        self.reducer.update(input, self.now_ms)
    }
    fn view(&self) -> CapView {
        self.reducer.view()
    }
}

fn skill(name: &str, enabled: bool) -> SkillRow {
    SkillRow {
        name: name.into(),
        description: "A guide".into(),
        category: None,
        enabled,
        usage: 0,
        provenance: Provenance::Hub,
    }
}
fn tool(enabled: bool) -> ToolsetRow {
    ToolsetRow {
        name: "web".into(),
        label: "Web".into(),
        description: String::new(),
        platform: "cli".into(),
        platform_label: String::new(),
        enabled,
        configured: true,
        tools: vec!["search".into()],
    }
}
fn plugin(name: &str, source: PluginSource, status: PluginStatus) -> PluginRow {
    PluginRow {
        name: name.into(),
        key: Some(format!("key-{name}")),
        version: String::new(),
        description: String::new(),
        source,
        status,
        servers: vec![],
        catalog_tier: None,
        update_available: false,
        settings: vec![],
    }
}
fn hub() -> CapData {
    CapData::Hub(HubPage {
        items: vec![HubItem {
            name: "guide".into(),
            description: "A guide".into(),
            source: "official".into(),
            trust: Some("trusted".into()),
            install_key: "official/guide".into(),
        }],
        page: 1,
        total_pages: 2,
        total: 25,
        ..HubPage::default()
    })
}
fn generation(effects: &[CapEffect], tab: CapTab) -> u64 {
    effects
        .iter()
        .find_map(|e| match e {
            CapEffect::Fetch {
                tab: t, generation, ..
            } if *t == tab => Some(*generation),
            CapEffect::HubBrowse { generation, .. } | CapEffect::HubSearch { generation, .. }
                if tab == CapTab::Hub =>
            {
                Some(*generation)
            }
            _ => None,
        })
        .expect("expected load")
}
fn load(state: &mut CapabilitiesState, tab: CapTab, data: CapData, now: u64) {
    let effects = state.update(CapInput::Open(tab), now);
    let gen_id = generation(&effects, tab);
    state.update(
        CapInput::Loaded {
            tab,
            generation: gen_id,
            result: Ok(data),
        },
        now,
    );
}
fn ready(tab: CapTab, data: CapData) -> CapabilitiesState {
    let mut state = CapabilitiesState::new("fixture".into(), None, tab);
    load(&mut state, tab, data, 0);
    state
}
fn ready_skill() -> CapabilitiesState {
    ready(CapTab::Skills, CapData::Skills(vec![skill("guide", true)]))
}
fn key(state: &mut CapabilitiesState, action: KeyAction) -> Vec<CapEffect> {
    state.update(CapInput::Key(action), 1)
}
fn op(effects: &[CapEffect]) -> u64 {
    effects
        .iter()
        .find_map(|e| match e {
            CapEffect::ToggleSkill { op_id, .. }
            | CapEffect::ToggleToolset { op_id, .. }
            | CapEffect::TogglePlugin { op_id, .. }
            | CapEffect::InstallSkill { op_id, .. }
            | CapEffect::InstallPlugin { op_id, .. }
            | CapEffect::RemovePlugin { op_id, .. } => Some(*op_id),
            _ => None,
        })
        .expect("expected write")
}
fn notices(effects: &[CapEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|e| {
            if let CapEffect::Notice(text) = e {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect()
}
fn assert_readback(effects: &[CapEffect], id: u64, tab: CapTab) {
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, CapEffect::ReadBack { .. }))
            .count(),
        1
    );
    assert!(effects.contains(&CapEffect::ReadBack {
        profile: "fixture".into(),
        tab,
        op_id: id
    }));
}
fn done(state: &mut CapabilitiesState, id: u64, receipt: WriteReceipt) -> Vec<CapEffect> {
    state.update(
        CapInput::WriteDone {
            op_id: id,
            result: Ok(receipt),
        },
        2,
    )
}
fn back(state: &mut CapabilitiesState, id: u64, data: CapData) -> Vec<CapEffect> {
    state.update(
        CapInput::ReadBack {
            op_id: id,
            result: Ok(data),
        },
        3,
    )
}
fn type_line(state: &mut CapabilitiesState, text: &str) {
    for ch in text.chars() {
        state.update(CapInput::Char(ch), 1);
    }
}
fn scan(policy: ScanPolicy) -> ScanReport {
    ScanReport {
        verdict: "clean".into(),
        policy,
        policy_reason: "policy says no".into(),
        source: "official".into(),
        trust: "trusted".into(),
        counts: [1, 2, 3, 4],
        findings: vec![],
    }
}

#[test]
fn tabs_load_lazily_with_generations() {
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Plugins);
    assert_eq!(state.view().tab, CapTab::Plugins);
    assert!(state.update(CapInput::Tick, 0).is_empty());
    let first = state.update(CapInput::Open(CapTab::Skills), 0);
    assert_eq!(first.len(), 1);
    let a = generation(&first, CapTab::Skills);
    assert_eq!(state.view().message, vec!["Loading skills..."]);
    let newer = key(&mut state, KeyAction::Refresh);
    let b = generation(&newer, CapTab::Skills);
    assert!(b > a);
    state.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation: b,
            result: Ok(CapData::Skills(vec![skill("new", true)])),
        },
        1,
    );
    assert!(
        state
            .update(
                CapInput::Loaded {
                    tab: CapTab::Skills,
                    generation: a,
                    result: Ok(CapData::Skills(vec![skill("old", false)]))
                },
                1
            )
            .is_empty()
    );
    assert!(state.view().rows.iter().any(|r| r.name == "new"));
    assert!(!state.view().rows.iter().any(|r| r.name == "old"));
    assert!(state.update(CapInput::Open(CapTab::Skills), 2).is_empty());
    assert!(state.update(CapInput::Tick, 2000).is_empty());
    let effects = state.update(CapInput::Open(CapTab::Hub), 2001);
    assert_eq!(effects.len(), 1);
    assert!(matches!(effects[0], CapEffect::HubBrowse { page: 1, .. }));
    let mut fresh = CapabilitiesState::new("fixture".into(), None, CapTab::Hub);
    let effects = fresh.update(CapInput::Open(CapTab::Hub), 0);
    assert_eq!(effects.len(), 2);
    generation(&effects, CapTab::Skills);
    generation(&effects, CapTab::Hub);
    let mut failed = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
    let effects = failed.update(CapInput::Open(CapTab::Skills), 0);
    failed.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation: generation(&effects, CapTab::Skills),
            result: Err(CapError::Connect),
        },
        1,
    );
    assert!(failed.update(CapInput::Tick, 5000).is_empty());
    assert!(
        failed
            .update(CapInput::Open(CapTab::Skills), 5001)
            .is_empty()
    );
    assert_eq!(key(&mut failed, KeyAction::Refresh).len(), 1);
}

#[test]
fn toggle_reads_back_and_reports() {
    for actual in [Some(false), None] {
        let mut s = ready(CapTab::Toolsets, CapData::Toolsets(vec![tool(false)]));
        let id = op(&key(&mut s, KeyAction::Toggle));
        done(&mut s, id, WriteReceipt::default());
        let rows = actual
            .map(|enabled| vec![tool(enabled)])
            .unwrap_or_default();
        let expected = if actual.is_some() {
            "Hermes did not keep the change to web: it now reads off. Press r to refresh."
        } else {
            "Hermes did not keep the change to web: it is no longer installed. Press r to refresh."
        };
        assert_eq!(
            notices(&back(&mut s, id, CapData::Toolsets(rows))),
            [expected]
        );
    }
    for inline in [false, true] {
        let row = plugin("fixture-plugin", PluginSource::User, PluginStatus::Disabled);
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![row.clone()]));
        let id = op(&key(&mut s, KeyAction::Toggle));
        let effects = if inline {
            done(
                &mut s,
                id,
                WriteReceipt {
                    plugin: Some(row),
                    ..WriteReceipt::default()
                },
            )
        } else {
            done(&mut s, id, WriteReceipt::default());
            back(&mut s, id, CapData::Plugins(vec![row]))
        };
        assert_eq!(
            notices(&effects),
            [
                "Hermes did not keep the change to fixture-plugin: it now reads off. Press r to refresh."
            ]
        );
    }
    let mut state = ready_skill();
    let id = op(&key(&mut state, KeyAction::Toggle));
    assert_eq!(
        state
            .view()
            .rows
            .iter()
            .find(|r| r.name == "guide")
            .unwrap()
            .marker,
        "[…]"
    );
    assert_readback(
        &done(&mut state, id, WriteReceipt::default()),
        id,
        CapTab::Skills,
    );
    assert_eq!(
        notices(&back(
            &mut state,
            id,
            CapData::Skills(vec![skill("guide", false)])
        )),
        ["Turned off skill guide in profile fixture. New sessions use this."]
    );
    for (rows, expected) in [
        (
            vec![skill("guide", true)],
            "Hermes did not keep the change to guide: it now reads on. Press r to refresh.",
        ),
        (
            vec![],
            "Hermes did not keep the change to guide: it is no longer installed. Press r to refresh.",
        ),
    ] {
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        done(&mut s, id, WriteReceipt::default());
        assert_eq!(
            notices(&back(&mut s, id, CapData::Skills(rows))),
            [expected]
        );
    }
    let mut s = ready(CapTab::Toolsets, CapData::Toolsets(vec![tool(false)]));
    let id = op(&key(&mut s, KeyAction::Toggle));
    assert_readback(
        &done(
            &mut s,
            id,
            WriteReceipt {
                post_setup_started: Some("install-key".into()),
                ..WriteReceipt::default()
            },
        ),
        id,
        CapTab::Toolsets,
    );
    assert_eq!(
        notices(&back(&mut s, id, CapData::Toolsets(vec![tool(true)]))),
        [
            "Turned on toolset Web in profile fixture. Sessions started from now on use this; open sessions keep their toolsets.\nHermes is installing what Web needs (install-key) in the background."
        ]
    );
    for status in [PluginStatus::Disabled, PluginStatus::NotEnabled] {
        let row = plugin("fixture-plugin", PluginSource::Git, status);
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![row.clone()]));
        let effects = key(&mut s, KeyAction::Toggle);
        let id = op(&effects);
        assert!(effects.contains(&CapEffect::TogglePlugin {
            profile: "fixture".into(),
            op_id: id,
            key: "key-fixture-plugin".into(),
            enable: true
        }));
        let enabled = PluginRow {
            status: PluginStatus::Enabled,
            ..row
        };
        let effects = done(
            &mut s,
            id,
            WriteReceipt {
                plugin: Some(enabled),
                ..WriteReceipt::default()
            },
        );
        assert_eq!(notices(&effects), ["Turned on plugin fixture-plugin."]);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, CapEffect::ReadBack { .. }))
        );
    }
    let row = plugin("fixture-plugin", PluginSource::User, PluginStatus::Enabled);
    let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![row.clone()]));
    let id = op(&key(&mut s, KeyAction::Toggle));
    assert_readback(
        &done(&mut s, id, WriteReceipt::default()),
        id,
        CapTab::Plugins,
    );
    assert_eq!(
        notices(&back(
            &mut s,
            id,
            CapData::Plugins(vec![PluginRow {
                status: PluginStatus::NotEnabled,
                ..row.clone()
            }])
        )),
        ["Turned off plugin fixture-plugin."]
    );
    let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![row.clone()]));
    let id = op(&key(&mut s, KeyAction::Toggle));
    assert_eq!(
        notices(&done(
            &mut s,
            id,
            WriteReceipt {
                unchanged: true,
                ..WriteReceipt::default()
            }
        )),
        ["Plugin fixture-plugin was already on."]
    );
    for row in [
        PluginRow {
            key: None,
            ..row.clone()
        },
        PluginRow {
            status: PluginStatus::Other("future".into()),
            ..row
        },
    ] {
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![row]));
        assert_eq!(key(&mut s,KeyAction::Toggle),vec![CapEffect::Notice("Talaria shows fixture-plugin read-only: the gateway listed it without a key or with a status Talaria does not know.".into())]);
    }
}

#[test]
fn one_write_at_a_time() {
    let mut reading = ready_skill();
    let reading_id = op(&key(&mut reading, KeyAction::Toggle));
    done(&mut reading, reading_id, WriteReceipt::default());
    assert_eq!(
        key(&mut reading, KeyAction::Toggle),
        vec![CapEffect::Notice(
            "Wait for the current change to finish.".into()
        )]
    );
    let mut state = ready_skill();
    let id = op(&key(&mut state, KeyAction::Toggle));
    assert_eq!(
        key(&mut state, KeyAction::Toggle),
        vec![CapEffect::Notice(
            "Wait for the current change to finish.".into()
        )]
    );
    load(
        &mut state,
        CapTab::Toolsets,
        CapData::Toolsets(vec![tool(true)]),
        1,
    );
    assert_eq!(
        key(&mut state, KeyAction::Toggle),
        vec![CapEffect::Notice(
            "Wait for the current change to finish.".into()
        )]
    );
    assert!(state.update(CapInput::Close, 2).is_empty());
    assert!(!state.view().open);
    let effects = state.update(
        CapInput::WriteDone {
            op_id: id,
            result: Err(CapError::TimedOut),
        },
        3,
    );
    assert_eq!(
        notices(&effects),
        ["Talaria lost the answer to this change. Refreshing to see what Hermes did."]
    );
    assert_readback(&effects, id, CapTab::Skills);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, CapEffect::ToggleSkill { .. }))
    );
    assert_eq!(
        back(&mut state, id, CapData::Skills(vec![skill("guide", false)])),
        vec![
            CapEffect::Notice(
                "Turned off skill guide in profile fixture. New sessions use this.".into()
            ),
            CapEffect::Dispose
        ]
    );
    let mut idle = ready_skill();
    assert_eq!(idle.update(CapInput::Close, 1), vec![CapEffect::Dispose]);
    let mut reopen = ready_skill();
    let id = op(&key(&mut reopen, KeyAction::Toggle));
    reopen.update(CapInput::Close, 2);
    assert!(reopen.update(CapInput::Open(CapTab::Skills), 3).is_empty());
    assert!(reopen.view().open);
    assert_eq!(
        notices(&key(&mut reopen, KeyAction::Toggle)),
        ["Wait for the current change to finish."]
    );
    done(&mut reopen, id, WriteReceipt::default());
    assert!(
        !back(
            &mut reopen,
            id,
            CapData::Skills(vec![skill("guide", false)])
        )
        .contains(&CapEffect::Dispose)
    );
}

#[test]
fn skill_install_scan_confirm_install_reload() {
    for next in [
        None,
        Some(SessionRef {
            session_id: "new-session".into(),
            profile: "fixture".into(),
        }),
    ] {
        let mut s = ready(CapTab::Hub, hub());
        s.update(
            CapInput::SetForeground(Some(SessionRef {
                session_id: "old-session".into(),
                profile: "other".into(),
            })),
            1,
        );
        key(&mut s, KeyAction::Install);
        s.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
        let id = op(&s.update(CapInput::Key(KeyAction::Confirm), 3));
        done(&mut s, id, WriteReceipt::default());
        s.update(CapInput::SetForeground(next.clone()), 4);
        let effects = s.update(
            CapInput::ReadBack {
                op_id: id,
                result: Ok(CapData::Skills(vec![skill("guide", true)])),
            },
            5,
        );
        assert_eq!(
            effects.iter().any(
                |e| matches!(e,CapEffect::ReloadSkills{session_id} if session_id=="new-session")
            ),
            next.is_some()
        );
        assert!(
            !effects.iter().any(
                |e| matches!(e,CapEffect::ReloadSkills{session_id} if session_id=="old-session")
            )
        );
        if next.is_some() {
            let effects = s.update(
                CapInput::ReloadDone(Err(CapError::Rpc {
                    code: 5999,
                    message: "reload denied".into(),
                    reason: None,
                    log: None,
                })),
                6,
            );
            assert!(notices(&effects).iter().any(|text| {
                text.ends_with("This session could not reload its skills: reload denied.")
            }));
        }
    }
    let CapData::Hub(mut page) = hub() else {
        unreachable!()
    };
    page.items[0].name = "Guide display".into();
    let mut matched = ready(CapTab::Hub, CapData::Hub(page));
    matched.update(CapInput::Open(CapTab::Skills), 1);
    let effects = key(&mut matched, KeyAction::Refresh);
    let existing = SkillRow {
        provenance: Provenance::Bundled,
        ..skill("guide", true)
    };
    matched.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation: generation(&effects, CapTab::Skills),
            result: Ok(CapData::Skills(vec![existing.clone()])),
        },
        1,
    );
    matched.update(CapInput::Open(CapTab::Hub), 1);
    key(&mut matched, KeyAction::Install);
    matched.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
    let id = op(&matched.update(CapInput::Key(KeyAction::Confirm), 3));
    done(&mut matched, id, WriteReceipt::default());
    assert_eq!(
        notices(&back(&mut matched, id, CapData::Skills(vec![existing]))),
        ["Installed guide into profile fixture."]
    );
    let mut s = ready(CapTab::Hub, hub());
    assert_eq!(
        key(&mut s, KeyAction::Install),
        vec![CapEffect::HubScan {
            profile: "fixture".into(),
            identifier: "official/guide".into()
        }]
    );
    let mut report = scan(ScanPolicy::Allow);
    for (i, severity) in [
        "low", "critical", "high", "medium", "critical", "unknown", "low",
    ]
    .iter()
    .enumerate()
    {
        report.findings.push(Finding {
            severity: (*severity).into(),
            category: "security".into(),
            file: format!("file-{i}"),
            line: i as u64,
            description: format!("finding-{i}"),
        });
    }
    assert!(s.update(CapInput::ScanDone(Ok(report)), 2).is_empty());
    let message = s.view().message.join("\n");
    assert!(message.starts_with("Install guide into profile fixture? Source: official (trusted). Scan: clean; 1 critical, 2 high, 3 medium, 4 low. Press y to install, any other key to cancel."));
    let ordered = [
        "critical security file-1:1 finding-1",
        "critical security file-4:4 finding-4",
        "high security file-2:2 finding-2",
        "medium security file-3:3 finding-3",
        "low security file-0:0 finding-0",
        "and 2 more",
    ];
    let mut previous = 0;
    for line in ordered {
        let at = message.find(line).unwrap();
        assert!(at >= previous);
        previous = at;
    }
    assert!(!message.contains("finding-5"));
    assert!(!message.contains("finding-6"));
    assert!(s.update(CapInput::Char('n'), 3).is_empty());
    assert!(!s.view().message.join("\n").contains("Press y"));
    for policy in [
        ScanPolicy::Ask,
        ScanPolicy::Block,
        ScanPolicy::Other("new-policy".into()),
    ] {
        let mut s = ready(CapTab::Hub, hub());
        key(&mut s, KeyAction::Install);
        s.update(CapInput::ScanDone(Ok(scan(policy))), 2);
        assert!(
            s.view()
                .message
                .join("\n")
                .contains("Hermes will not install guide from Talaria: policy says no")
        );
        assert!(
            !s.update(CapInput::Key(KeyAction::Confirm), 3)
                .iter()
                .any(|e| matches!(e, CapEffect::InstallSkill { .. }))
        );
    }
    for foreground in [
        None,
        Some(SessionRef {
            session_id: "other-session".into(),
            profile: "other".into(),
        }),
        Some(SessionRef {
            session_id: "same-session".into(),
            profile: "fixture".into(),
        }),
    ] {
        let mut s = ready(CapTab::Hub, hub());
        s.update(CapInput::SetForeground(foreground.clone()), 1);
        key(&mut s, KeyAction::Install);
        s.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
        let effects = s.update(CapInput::Key(KeyAction::Confirm), 3);
        let id = op(&effects);
        assert!(effects.contains(&CapEffect::InstallSkill {
            profile: "fixture".into(),
            op_id: id,
            install_key: "official/guide".into()
        }));
        assert!(
            s.view().message.join("\n").contains(
                "Installing guide... Hermes fetches and scans it; this can take a minute."
            )
        );
        assert_readback(
            &done(&mut s, id, WriteReceipt::default()),
            id,
            CapTab::Skills,
        );
        let effects = back(&mut s, id, CapData::Skills(vec![skill("guide", true)]));
        assert!(notices(&effects).contains(&"Installed guide into profile fixture."));
        let reload = effects
            .iter()
            .any(|e| matches!(e,CapEffect::ReloadSkills{session_id} if session_id=="same-session"));
        assert_eq!(
            reload,
            foreground.as_ref().is_some_and(|f| f.profile == "fixture")
        );
        if reload {
            let effects = s.update(
                CapInput::ReloadDone(Ok("Reloading skills...\n41 skill(s) available\n".into())),
                4,
            );
            assert!(
                notices(&effects)
                    .iter()
                    .any(|text| text.ends_with("This session: 41 skill(s) available."))
            );
        }
    }
    for (rows, expected) in [
        (
            vec![],
            "Hermes reported guide installed, but profile fixture does not list it. Press r to refresh.",
        ),
        (
            vec![skill("guide", true)],
            "Installed guide into profile fixture.",
        ),
    ] {
        let mut s = ready(CapTab::Hub, hub());
        key(&mut s, KeyAction::Install);
        s.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
        let id = op(&s.update(CapInput::Key(KeyAction::Confirm), 3));
        done(&mut s, id, WriteReceipt::default());
        assert!(notices(&back(&mut s, id, CapData::Skills(rows))).contains(&expected));
    }
    let mut s = ready(CapTab::Hub, hub());
    key(&mut s, KeyAction::Install);
    s.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
    let id = op(&s.update(CapInput::Key(KeyAction::Confirm), 3));
    let effects = s.update(
        CapInput::WriteDone {
            op_id: id,
            result: Err(CapError::Rpc {
                code: 5031,
                message: "ambiguous".into(),
                reason: None,
                log: Some("first\nsecond\nambiguous".into()),
            }),
        },
        4,
    );
    assert_eq!(
        notices(&effects),
        ["Hermes did not install guide: ambiguous"]
    );
    assert_eq!(s.view().detail, vec!["first", "second", "ambiguous"]);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, CapEffect::InstallSkill { .. }))
    );
}

#[test]
fn hub_browse_search_and_marks() {
    let mut s = CapabilitiesState::new("fixture".into(), None, CapTab::Hub);
    let effects = s.update(CapInput::Open(CapTab::Hub), 0);
    s.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation: generation(&effects, CapTab::Skills),
            result: Ok(CapData::Skills(vec![skill("guide", true)])),
        },
        0,
    );
    s.update(
        CapInput::Loaded {
            tab: CapTab::Hub,
            generation: generation(&effects, CapTab::Hub),
            result: Ok(hub()),
        },
        0,
    );
    assert!(
        s.view()
            .rows
            .iter()
            .find(|r| r.name == "guide")
            .unwrap()
            .installed
    );
    assert_eq!(s.view().footer, "Page 1 of 2 · 25 skills");
    let effects = key(&mut s, KeyAction::NextPage);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, CapEffect::HubBrowse { page: 2, .. }))
    );
    let page = HubPage {
        items: vec![HubItem {
            name: "bare".into(),
            description: String::new(),
            source: String::new(),
            trust: None,
            install_key: "bare".into(),
        }],
        page: 2,
        total_pages: 2,
        total: 25,
        ..HubPage::default()
    };
    s.update(
        CapInput::Loaded {
            tab: CapTab::Hub,
            generation: generation(&effects, CapTab::Hub),
            result: Ok(CapData::Hub(page)),
        },
        1,
    );
    assert_eq!(s.view().footer, "Page 2 of 2 · 25 skills");
    assert!(key(&mut s, KeyAction::NextPage).is_empty());
    assert_eq!(
        key(&mut s, KeyAction::Install),
        vec![CapEffect::HubScan {
            profile: "fixture".into(),
            identifier: "bare".into()
        }]
    );
    s.update(
        CapInput::ScanDone(Err(CapError::NotFound {
            route: "GET /api/skills/hub/scan".into(),
        })),
        2,
    );
    assert!(
        s.view()
            .message
            .join("\n")
            .contains("The skill hub has no skill bare.")
    );
    let effects = key(&mut s, KeyAction::PrevPage);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, CapEffect::HubBrowse { page: 1, .. }))
    );
    s.update(
        CapInput::Loaded {
            tab: CapTab::Hub,
            generation: generation(&effects, CapTab::Hub),
            result: Ok(CapData::Hub(HubPage {
                page: 1,
                total_pages: 1,
                ..HubPage::default()
            })),
        },
        2,
    );
    assert_eq!(
        s.view().message,
        vec!["The skill hub returned nothing. Check the network, then press r."]
    );
    key(&mut s, KeyAction::Filter);
    type_line(&mut s, "zzz");
    let effects = key(&mut s, KeyAction::Enter);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e,CapEffect::HubSearch{q,..} if q=="zzz"))
    );
    s.update(
        CapInput::Loaded {
            tab: CapTab::Hub,
            generation: generation(&effects, CapTab::Hub),
            result: Ok(CapData::Hub(HubPage {
                query: Some("zzz".into()),
                ..HubPage::default()
            })),
        },
        2,
    );
    assert_eq!(s.view().message, vec!["No hub skills match \"zzz\"."]);
    key(&mut s, KeyAction::Filter);
    type_line(&mut s, "guide");
    let effects = key(&mut s, KeyAction::Enter);
    let CapData::Hub(mut page) = hub() else {
        unreachable!()
    };
    page.query = Some("guide".into());
    page.timed_out = vec!["github".into(), "gitlab".into()];
    page.installed = vec!["official/guide".into()];
    s.update(
        CapInput::Loaded {
            tab: CapTab::Hub,
            generation: generation(&effects, CapTab::Hub),
            result: Ok(CapData::Hub(page)),
        },
        2,
    );
    assert_eq!(
        s.view().message,
        vec!["Some hub sources did not answer in time: github, gitlab."]
    );
    assert!(s.view().rows[0].installed);
    assert_eq!(s.view().footer, "Search: \"guide\" · 1 results");
    assert!(key(&mut s, KeyAction::NextPage).is_empty());
    assert_eq!(
        key(&mut s, KeyAction::Enter),
        vec![CapEffect::HubPreview {
            profile: "fixture".into(),
            identifier: "official/guide".into()
        }]
    );
    s.update(
        CapInput::PreviewDone(Err(CapError::NotFound {
            route: "GET /api/skills/hub/preview".into(),
        })),
        2,
    );
    assert_eq!(
        s.view().detail,
        vec!["The skill hub has no skill official/guide."]
    );
    assert!(!s.view().loading);
    key(&mut s, KeyAction::Cancel);
    let effects = key(&mut s, KeyAction::Cancel);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, CapEffect::HubBrowse { page: 1, .. }))
    );
    for error in [
        CapError::TimedOut,
        CapError::Server { status: 502 },
        CapError::Rpc {
            code: 5024,
            message: "offline".into(),
            reason: None,
            log: None,
        },
    ] {
        let mut s = CapabilitiesState::new("fixture".into(), None, CapTab::Hub);
        let effects = s.update(CapInput::Open(CapTab::Hub), 0);
        s.update(
            CapInput::Loaded {
                tab: CapTab::Hub,
                generation: generation(&effects, CapTab::Hub),
                result: Err(error),
            },
            1,
        );
        assert_eq!(
            s.view().message,
            vec!["The skill hub did not answer. Press r to try again."]
        );
    }
    let decoded=CapData::from_json(CapTab::Hub,&serde_json::json!({"items":[{"name":"bare","identifier":null}],"page":2,"total_pages":2,"total":25})).unwrap();
    let CapData::Hub(page) = decoded else {
        panic!("hub expected")
    };
    assert_eq!(page.items[0].install_key, "bare");
}

#[test]
fn plugin_install_and_remove_paths() {
    let mut missing = ready(CapTab::Plugins, CapData::Plugins(vec![]));
    key(&mut missing, KeyAction::Install);
    type_line(&mut missing, "fixture-plugin");
    key(&mut missing, KeyAction::Enter);
    missing.update(
        CapInput::LookupDone(Ok(PluginLookup::Found {
            name: "fixture-plugin".into(),
            repository: "https://example.org/plugin".into(),
            sha: None,
        })),
        2,
    );
    let id = op(&missing.update(CapInput::Key(KeyAction::Confirm), 3));
    done(
        &mut missing,
        id,
        WriteReceipt {
            plugin_name: Some("fixture-plugin".into()),
            enabled: true,
            ..WriteReceipt::default()
        },
    );
    assert_eq!(
        notices(&back(&mut missing, id, CapData::Plugins(vec![]))),
        [
            "Hermes reported fixture-plugin installed, but profile fixture does not list it. Press r to refresh."
        ]
    );
    for (input, reply, expected) in [
        (
            "My.Plugin",
            None,
            "Type a catalog name (lowercase letters, digits, _ or -) or an https:// repository address.",
        ),
        (
            "nosuch",
            Some(PluginLookup::Unknown),
            "The Hermes plugin catalog has no plugin named nosuch.",
        ),
        (
            "fixture-plugin",
            Some(PluginLookup::Unavailable),
            "Hermes could not read its plugin catalog. Try again later, or install from an https:// repository address.",
        ),
        (
            "invalid",
            Some(PluginLookup::InvalidName),
            "Type a catalog name (lowercase letters, digits, _ or -) or an https:// repository address.",
        ),
    ] {
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![]));
        key(&mut s, KeyAction::Install);
        type_line(&mut s, input);
        let effects = key(&mut s, KeyAction::Enter);
        if let Some(reply) = reply {
            assert_eq!(
                effects,
                vec![CapEffect::LookupPlugin {
                    profile: "fixture".into(),
                    name: input.into()
                }]
            );
            s.update(CapInput::LookupDone(Ok(reply)), 2);
        } else {
            assert!(effects.iter().all(|e| matches!(e, CapEffect::Notice(_))));
        }
        assert_eq!(s.view().input, Some(format!("Install: {input}")));
        assert!(s.view().text_focus);
        assert_eq!(s.view().message, vec![expected]);
        key(&mut s, KeyAction::Cancel);
        assert!(s.view().input.is_none());
        key(&mut s, KeyAction::Install);
        assert_eq!(s.view().input, Some("Install: ".into()));
    }
    for sha in [Some("abcdef123456".to_string()), None] {
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![]));
        key(&mut s, KeyAction::Install);
        type_line(&mut s, "fixture-plugin");
        key(&mut s, KeyAction::Enter);
        s.update(
            CapInput::LookupDone(Ok(PluginLookup::Found {
                name: "fixture-plugin".into(),
                repository: "https://example.org/plugin".into(),
                sha: sha.clone(),
            })),
            2,
        );
        let commit = if sha.is_some() {
            "commit abcdef1"
        } else {
            "commit unknown"
        };
        assert_eq!(
            s.view().message,
            vec![format!(
                "Install plugin fixture-plugin from the Hermes catalog (https://example.org/plugin, {commit}) into profile fixture? Press y to install, any other key to cancel."
            )]
        );
        assert!(!s.view().text_focus);
        let effects = s.update(CapInput::Key(KeyAction::Confirm), 3);
        let id = op(&effects);
        assert!(effects.contains(&CapEffect::InstallPlugin {
            profile: "fixture".into(),
            op_id: id,
            target: InstallTarget::CatalogName("fixture-plugin".into())
        }));
        let effects = done(
            &mut s,
            id,
            WriteReceipt {
                plugin_name: Some("fixture-plugin".into()),
                warnings: vec!["review configuration".into()],
                missing_env: vec!["FIXTURE_TOKEN".into()],
                enabled: false,
                ..WriteReceipt::default()
            },
        );
        assert_readback(&effects, id, CapTab::Plugins);
        assert!(notices(&effects).contains(&"Installed plugin fixture-plugin.\nWarning: review configuration\nIt needs these settings before it works: FIXTURE_TOKEN.\nIt is turned off; press Space to turn it on."));
        assert!(
            !back(
                &mut s,
                id,
                CapData::Plugins(vec![plugin(
                    "fixture-plugin",
                    PluginSource::User,
                    PluginStatus::NotEnabled
                )])
            )
            .iter()
            .any(|e| matches!(e,CapEffect::Notice(t) if t.contains("does not list")))
        );
    }
    let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![]));
    key(&mut s, KeyAction::Install);
    type_line(&mut s, "https://example.org/plugin");
    key(&mut s, KeyAction::Enter);
    assert_eq!(
        s.view().message,
        vec![
            "Install the plugin at https://example.org/plugin into profile fixture? It is not from the Hermes catalog, so nobody has reviewed it. Press y to install, any other key to cancel."
        ]
    );
    assert!(s.update(CapInput::Char('n'), 2).is_empty());
    key(&mut s, KeyAction::Install);
    type_line(&mut s, "https://example.org/plugin");
    key(&mut s, KeyAction::Enter);
    let effects = s.update(CapInput::Key(KeyAction::Confirm), 2);
    assert!(effects.iter().any(|e|matches!(e,CapEffect::InstallPlugin{target:InstallTarget::Repository(url),..} if url=="https://example.org/plugin")));
    for error in [
        CapError::Unauthorized,
        CapError::NotFound {
            route: "GET /api/dashboard/plugins/catalog".into(),
        },
    ] {
        let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![]));
        let effects = key(&mut s, KeyAction::Catalog);
        let gen_id = match effects[0] {
            CapEffect::FetchCatalog { generation, .. } => generation,
            _ => panic!("catalog fetch expected"),
        };
        s.update(
            CapInput::CatalogLoaded {
                generation: gen_id,
                result: Err(error),
            },
            2,
        );
        assert_eq!(
            s.view().message,
            vec![
                "The plugin catalog list is not available from this gateway; type a catalog name instead."
            ]
        );
    }
    let mut s = ready(CapTab::Plugins, CapData::Plugins(vec![]));
    let effects = key(&mut s, KeyAction::Catalog);
    let gen_id = match effects[0] {
        CapEffect::FetchCatalog { generation, .. } => generation,
        _ => panic!("catalog fetch expected"),
    };
    s.update(
        CapInput::CatalogLoaded {
            generation: gen_id,
            result: Ok(vec![CatalogEntry {
                name: "fixture-plugin".into(),
                ..CatalogEntry::default()
            }]),
        },
        2,
    );
    assert_eq!(
        key(&mut s, KeyAction::Enter),
        vec![CapEffect::LookupPlugin {
            profile: "fixture".into(),
            name: "fixture-plugin".into()
        }]
    );
    for (source, expected) in [
        (
            PluginSource::Bundled,
            "fixture-plugin ships with Hermes and cannot be removed; turn it off instead.",
        ),
        (
            PluginSource::Entrypoint,
            "fixture-plugin is a Python package; remove its package outside Talaria.",
        ),
    ] {
        let mut s = ready(
            CapTab::Plugins,
            CapData::Plugins(vec![plugin(
                "fixture-plugin",
                source.clone(),
                PluginStatus::Enabled,
            )]),
        );
        if source == PluginSource::Bundled {
            key(&mut s, KeyAction::ShowAll);
        }
        assert_eq!(
            key(&mut s, KeyAction::Remove),
            vec![CapEffect::Notice(expected.into())]
        );
    }
    let mut s = ready(
        CapTab::Plugins,
        CapData::Plugins(vec![
            plugin("duplicate", PluginSource::User, PluginStatus::Enabled),
            plugin("duplicate", PluginSource::Git, PluginStatus::Disabled),
        ]),
    );
    assert_eq!(key(&mut s,KeyAction::Remove),vec![CapEffect::Notice("More than one installed plugin is named duplicate, so Talaria cannot tell which one Hermes would remove.".into())]);
    for source in [
        PluginSource::User,
        PluginSource::Git,
        PluginSource::Other("future".into()),
    ] {
        let mut s = ready(
            CapTab::Plugins,
            CapData::Plugins(vec![plugin(
                "fixture-plugin",
                source,
                PluginStatus::Enabled,
            )]),
        );
        key(&mut s, KeyAction::Remove);
        assert_eq!(
            s.view().message,
            vec![
                "Remove plugin fixture-plugin from profile fixture? Press y to remove, any other key to cancel."
            ]
        );
        let effects = s.update(CapInput::Key(KeyAction::Confirm), 2);
        let id = op(&effects);
        assert!(effects.contains(&CapEffect::RemovePlugin {
            profile: "fixture".into(),
            op_id: id,
            name: "fixture-plugin".into()
        }));
        let effects = done(
            &mut s,
            id,
            WriteReceipt {
                cleared_memory_provider: true,
                ..WriteReceipt::default()
            },
        );
        assert_eq!(
            notices(&effects),
            [
                "Removed plugin fixture-plugin.\nIt was the memory provider; Hermes now uses its built-in memory."
            ]
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            CapEffect::Fetch {
                tab: CapTab::Plugins,
                ..
            }
        )));
    }
}

#[test]
fn skills_changed_refreshes_after_inflight_write() {
    let mut s = ready_skill();
    assert_eq!(
        s.update(CapInput::SkillsChanged, 100),
        vec![CapEffect::ScheduleTick { after_ms: 500 }]
    );
    assert!(s.update(CapInput::SkillsChanged, 200).is_empty());
    assert_eq!(
        s.update(CapInput::Tick, 400),
        vec![CapEffect::ScheduleTick { after_ms: 200 }]
    );
    let effects = s.update(CapInput::Tick, 700);
    assert_eq!(effects.len(), 1);
    generation(&effects, CapTab::Skills);
    assert!(s.update(CapInput::Tick, 800).is_empty());
    let mut s = ready_skill();
    let id = op(&key(&mut s, KeyAction::Toggle));
    for now in 2..5 {
        assert!(s.update(CapInput::SkillsChanged, now).is_empty());
    }
    assert_readback(
        &done(&mut s, id, WriteReceipt::default()),
        id,
        CapTab::Skills,
    );
    assert!(s.update(CapInput::SkillsChanged, 5).is_empty());
    let effects = s.update(
        CapInput::ReadBack {
            op_id: id,
            result: Ok(CapData::Skills(vec![skill("guide", false)])),
        },
        6,
    );
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(
                e,
                CapEffect::Fetch {
                    tab: CapTab::Skills,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(s.update(CapInput::Tick, 1000).is_empty());
    let mut s = ready_skill();
    s.update(CapInput::SkillsChanged, 0);
    let id = op(&key(&mut s, KeyAction::Toggle));
    assert!(s.update(CapInput::Tick, 500).is_empty());
    done(&mut s, id, WriteReceipt::default());
    let effects = s.update(
        CapInput::ReadBack {
            op_id: id,
            result: Ok(CapData::Skills(vec![skill("guide", false)])),
        },
        501,
    );
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(
                e,
                CapEffect::Fetch {
                    tab: CapTab::Skills,
                    ..
                }
            ))
            .count(),
        1
    );
    let mut untouched = CapabilitiesState::new("fixture".into(), None, CapTab::Plugins);
    assert_eq!(
        untouched.update(CapInput::SkillsChanged, 0),
        vec![CapEffect::ScheduleTick { after_ms: 500 }]
    );
    assert!(untouched.update(CapInput::Tick, 500).is_empty());
    let mut hidden = ready_skill();
    let id = op(&key(&mut hidden, KeyAction::Toggle));
    hidden.update(CapInput::Close, 2);
    assert!(hidden.update(CapInput::SkillsChanged, 3).is_empty());
    done(&mut hidden, id, WriteReceipt::default());
    let effects = hidden.update(
        CapInput::ReadBack {
            op_id: id,
            result: Ok(CapData::Skills(vec![skill("guide", false)])),
        },
        4,
    );
    assert!(
        effects.iter().any(|e| matches!(
            e,
            CapEffect::Fetch {
                tab: CapTab::Skills,
                ..
            }
        )) || effects.last() == Some(&CapEffect::Dispose)
    );
}

fn read_error(tab: CapTab, error: CapError) -> String {
    let mut s = CapabilitiesState::new("fixture".into(), None, tab);
    let effects = s.update(CapInput::Open(tab), 0);
    s.update(
        CapInput::Loaded {
            tab,
            generation: generation(&effects, tab),
            result: Err(error),
        },
        1,
    );
    s.view().message.join("\n")
}

#[test]
fn errors_map_to_texts() {
    let mut removal = ready(
        CapTab::Plugins,
        CapData::Plugins(vec![plugin(
            "fixture-plugin",
            PluginSource::Git,
            PluginStatus::Disabled,
        )]),
    );
    key(&mut removal, KeyAction::Remove);
    let id = op(&removal.update(CapInput::Key(KeyAction::Confirm), 2));
    let effects = removal.update(
        CapInput::WriteDone {
            op_id: id,
            result: Err(CapError::Rpc {
                code: 5026,
                message: "not found".into(),
                reason: None,
                log: None,
            }),
        },
        3,
    );
    assert_eq!(
        effects,
        vec![CapEffect::Notice(
            "Hermes refused this request: not found".into()
        )]
    );
    let unreachable = "Talaria could not reach the gateway. Press r to retry.";
    let timeout = "Hermes did not answer in time. Press r to try again.";
    let hub_timeout = "The skill hub did not answer. Press r to try again.";
    let lost = "Talaria lost the answer to this change. Refreshing to see what Hermes did.";
    let profile = "Profile fixture is not served by the gateway, so its capabilities cannot be read or changed.";
    for error in [
        CapError::Connect,
        CapError::Ticket,
        CapError::NotLoopback,
        CapError::TlsNotSupported,
        CapError::Timeout { sent: false },
        CapError::Dropped { sent: false },
        CapError::NotConnected,
        CapError::Encode,
        CapError::InvalidTimeout,
    ] {
        assert_eq!(read_error(CapTab::Skills, error.clone()), unreachable);
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(error),
            },
            2,
        );
        assert_eq!(effects, vec![CapEffect::Notice(unreachable.into())]);
    }
    for error in [
        CapError::Timeout { sent: true },
        CapError::Dropped { sent: true },
        CapError::TimedOut,
        CapError::Unknown,
    ] {
        assert_eq!(read_error(CapTab::Skills, error.clone()), timeout);
        assert_eq!(read_error(CapTab::Hub, error.clone()), hub_timeout);
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(error),
            },
            2,
        );
        assert_eq!(notices(&effects), [lost]);
        assert_readback(&effects, id, CapTab::Skills);
        assert_eq!(effects.len(), 2);
    }
    for error in [
        CapError::BadBody,
        CapError::BodyTooLarge,
        CapError::BadResult,
    ] {
        assert_eq!(
            read_error(CapTab::Skills, error.clone()),
            "Hermes refused this request: unreadable reply"
        );
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(error),
            },
            2,
        );
        assert_eq!(notices(&effects), [lost]);
        assert_readback(&effects, id, CapTab::Skills);
    }
    for (error, expected) in [
        (
            CapError::Unauthorized,
            "Hermes refused Talaria's sign-in for this request. Press r to retry.",
        ),
        (
            CapError::Forbidden {
                detail: "profile_scope_mismatch".into(),
            },
            profile,
        ),
        (
            CapError::Refused {
                status: 400,
                detail: Some("Unknown toolset: web".into()),
            },
            "Hermes refused this request: Unknown toolset: web",
        ),
        (
            CapError::Refused {
                status: 422,
                detail: None,
            },
            "Hermes refused this request: HTTP 422",
        ),
        (
            CapError::Server { status: 502 },
            "Hermes refused this request: HTTP 502",
        ),
        (
            CapError::Rpc {
                code: 4001,
                message: "mismatch".into(),
                reason: Some("profile_mismatch".into()),
                log: None,
            },
            profile,
        ),
        (
            CapError::Rpc {
                code: 4064,
                message: "profile not found".into(),
                reason: None,
                log: None,
            },
            profile,
        ),
        (
            CapError::Rpc {
                code: 4001,
                message: "denied".into(),
                reason: Some("permission_denied".into()),
                log: None,
            },
            "Hermes refused this request: permission_denied",
        ),
        (
            CapError::Rpc {
                code: 5999,
                message: "other failure".into(),
                reason: None,
                log: None,
            },
            "Hermes refused this request: other failure",
        ),
        (
            CapError::Rpc {
                code: 5024,
                message: "browse failed".into(),
                reason: None,
                log: None,
            },
            "Hermes refused this request: browse failed",
        ),
    ] {
        assert_eq!(read_error(CapTab::Skills, error.clone()), expected);
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        assert_eq!(
            s.update(
                CapInput::WriteDone {
                    op_id: id,
                    result: Err(error)
                },
                2
            ),
            vec![CapEffect::Notice(expected.into())]
        );
        assert_eq!(
            s.view()
                .rows
                .iter()
                .find(|r| r.name == "guide")
                .unwrap()
                .marker,
            "[x]"
        );
    }
    assert_eq!(
        read_error(CapTab::Hub, CapError::Server { status: 503 }),
        hub_timeout
    );
    assert_eq!(
        read_error(
            CapTab::Hub,
            CapError::Rpc {
                code: 5024,
                message: "failed".into(),
                reason: None,
                log: None
            }
        ),
        hub_timeout
    );
    assert_eq!(
        read_error(
            CapTab::Plugins,
            CapError::Rpc {
                code: -32601,
                message: "missing".into(),
                reason: None,
                log: None
            }
        ),
        "This gateway does not answer plugins.manage on Talaria's connection."
    );
    assert_eq!(
        read_error(
            CapTab::Toolsets,
            CapError::NotFound {
                route: "GET /api/tools/toolsets".into()
            }
        ),
        "This gateway does not answer GET /api/tools/toolsets on Talaria's connection."
    );
    for error in [
        CapError::NotFound {
            route: "PUT /api/skills/toggle".into(),
        },
        CapError::Rpc {
            code: -32601,
            message: "missing".into(),
            reason: None,
            log: None,
        },
    ] {
        let mut s = ready_skill();
        let id = op(&key(&mut s, KeyAction::Toggle));
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(error),
            },
            2,
        );
        assert!(notices(&effects)[0].starts_with("This gateway does not answer "));
        assert_eq!(effects.len(), 1);
    }
    let mut s = ready_skill();
    let effects = key(&mut s, KeyAction::Enter);
    let gen_id = match effects[0] {
        CapEffect::FetchDetail { generation, .. } => generation,
        _ => panic!("detail expected"),
    };
    s.update(
        CapInput::DetailLoaded {
            tab: CapTab::Skills,
            generation: gen_id,
            result: Err(CapError::NotFound {
                route: "GET /api/skills/content".into(),
            }),
        },
        2,
    );
    assert_eq!(
        s.view().detail,
        vec!["Hermes has no skill guide with a SKILL.md in profile fixture. Press r to refresh."]
    );
    assert!(!s.view().loading);
    let mut s = ready_skill();
    let id = op(&key(&mut s, KeyAction::Toggle));
    done(&mut s, id, WriteReceipt::default());
    let effects = s.update(
        CapInput::ReadBack {
            op_id: id,
            result: Err(CapError::Connect),
        },
        3,
    );
    assert_eq!(
        notices(&effects),
        [
            "Hermes accepted the change to guide, but Talaria could not read it back. Press r to refresh."
        ]
    );
    for (code, message) in [(5031, "skill install failed"), (5024, "ambiguous")] {
        let mut s = ready(CapTab::Hub, hub());
        key(&mut s, KeyAction::Install);
        s.update(CapInput::ScanDone(Ok(scan(ScanPolicy::Allow))), 2);
        let id = op(&s.update(CapInput::Key(KeyAction::Confirm), 3));
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(CapError::Rpc {
                    code,
                    message: message.into(),
                    reason: None,
                    log: None,
                }),
            },
            4,
        );
        assert_eq!(
            notices(&effects),
            [format!("Hermes did not install guide: {message}")]
        );
        assert_eq!(s.view().detail, vec!["(Hermes sent no install log)"]);
        assert_eq!(effects.len(), 1);
    }
    for install in [false, true] {
        let mut s = ready(
            CapTab::Plugins,
            CapData::Plugins(vec![plugin(
                "fixture-plugin",
                PluginSource::User,
                PluginStatus::Enabled,
            )]),
        );
        let id = if install {
            key(&mut s, KeyAction::Install);
            type_line(&mut s, "https://example.org/plugin");
            key(&mut s, KeyAction::Enter);
            op(&s.update(CapInput::Key(KeyAction::Confirm), 2))
        } else {
            op(&key(&mut s, KeyAction::Toggle))
        };
        let effects = s.update(
            CapInput::WriteDone {
                op_id: id,
                result: Err(CapError::Rpc {
                    code: 5026,
                    message: "denied".into(),
                    reason: None,
                    log: None,
                }),
            },
            3,
        );
        let notice = notices(&effects);
        if install {
            assert!(notice[0].starts_with("Hermes did not install "));
            assert!(notice[0].ends_with(": denied"));
        } else {
            assert_eq!(notice, ["Hermes refused this request: denied"]);
        }
    }
    assert_eq!(
        read_error(
            CapTab::Plugins,
            CapError::Rpc {
                code: 5026,
                message: "denied".into(),
                reason: None,
                log: None
            }
        ),
        "Hermes refused this request: denied"
    );
}
