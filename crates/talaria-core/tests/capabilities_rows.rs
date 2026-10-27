use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::capabilities::*;

fn skills(value: Value) -> Vec<SkillRow> {
    match CapData::from_json(CapTab::Skills, &value).unwrap() {
        CapData::Skills(rows) => rows,
        _ => panic!("skills expected"),
    }
}
fn plugins(value: Value) -> Vec<PluginRow> {
    match CapData::from_json(CapTab::Plugins, &value).unwrap() {
        CapData::Plugins(rows) => rows,
        _ => panic!("plugins expected"),
    }
}

#[test]
fn decode_and_group_rows() {
    let rows = skills(json!([
        {"name":"top", "enabled":true, "category":null, "description":null, "provenance":"agent", "ignored":1, "path":"/discarded/SKILL.md"},
        {"name":"z", "enabled":false,"category":"zeta","usage":3,"provenance":"hub"},
        {"name":"a", "enabled":true,"category":"alpha","provenance":"bundled"},
        {"name":"external", "enabled":true,"provenance":"external"},
        {"name":"other", "enabled":true,"provenance":"new-source"}
    ]));
    assert_eq!(rows[0].description, "");
    assert_eq!(rows[0].provenance, Provenance::Local);
    assert_eq!(rows[0].usage, 0);
    assert_eq!(rows[1].provenance, Provenance::Hub);
    assert_eq!(rows[2].provenance, Provenance::Bundled);
    assert_eq!(rows[3].provenance, Provenance::External);
    assert_eq!(rows[4].provenance, Provenance::Other("new-source".into()));
    assert!(!format!("{rows:?}").contains("discarded"));
    let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
    let effects = state.update(CapInput::Open(CapTab::Skills), 0);
    let generation = match effects[0] {
        CapEffect::Fetch { generation, .. } => generation,
        _ => panic!("fetch expected"),
    };
    state.update(
        CapInput::Loaded {
            tab: CapTab::Skills,
            generation,
            result: Ok(CapData::Skills(rows)),
        },
        0,
    );
    let view = state.view();
    let headers: Vec<_> = view
        .rows
        .iter()
        .filter(|r| r.is_header)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(headers, ["alpha", "zeta", "(no category)"]);
    assert_eq!(
        view.rows.iter().find(|r| r.name == "top").unwrap().label,
        "local"
    );
    assert_eq!(
        view.rows.iter().find(|r| r.name == "z").unwrap().marker,
        "[ ]"
    );
    assert_eq!(
        view.rows.iter().find(|r| r.name == "a").unwrap().marker,
        "[x]"
    );
    let p = plugins(
        json!({"plugins":[{"name":"git-one","key":"g","source":"git","status":"disabled"},{"name":"package-one","source":"entrypoint","status":"not enabled"},{"name":"odd","status":"future","install_dir":"/discarded/private","settings_schema":[{"key":"TOKEN","label":"Token","type":"secret","has_value":true,"value":"must-not-survive"}]}]}),
    );
    assert_eq!(p[0].source, PluginSource::Git);
    assert_eq!(p[0].status, PluginStatus::Disabled);
    assert_eq!(p[1].source, PluginSource::Entrypoint);
    assert_eq!(p[2].key, None);
    assert_eq!(p[2].settings[0].display_value, "set");
    assert!(!format!("{p:?}").contains("must-not-survive"));
    assert!(!format!("{p:?}").contains("discarded"));
    let effects = state.update(CapInput::Open(CapTab::Plugins), 1);
    let generation = match effects[0] {
        CapEffect::Fetch { generation, .. } => generation,
        _ => panic!("fetch expected"),
    };
    state.update(
        CapInput::Loaded {
            tab: CapTab::Plugins,
            generation,
            result: Ok(CapData::Plugins(p)),
        },
        1,
    );
    let view = state.view();
    for (name, label, marker) in [
        ("git-one", "yours", "[-]"),
        ("package-one", "package", "[ ]"),
        ("odd", "-", "[?]"),
    ] {
        let row = view.rows.iter().find(|r| r.name == name).unwrap();
        assert_eq!((&*row.label, &*row.marker), (label, marker));
    }
    for nulls in [false, true] {
        let mut skill = json!({"name":"minimal","enabled":false});
        let mut plugin = json!({"name":"minimal"});
        let mut toolset = json!({"name":"minimal","enabled":true});
        if nulls {
            for field in ["description", "category", "usage", "provenance"] {
                skill[field] = Value::Null;
            }
            for field in [
                "key",
                "version",
                "description",
                "source",
                "status",
                "servers",
                "catalog_tier",
                "update_available",
                "settings_schema",
            ] {
                plugin[field] = Value::Null;
            }
            for field in [
                "label",
                "description",
                "platform",
                "platform_label",
                "configured",
                "tools",
            ] {
                toolset[field] = Value::Null;
            }
        }
        let row = skills(json!([skill])).remove(0);
        assert_eq!(
            (row.description, row.category, row.usage, row.provenance),
            (String::new(), None, 0, Provenance::Other(String::new()))
        );
        let row = plugins(json!({"plugins":[plugin]})).remove(0);
        assert_eq!(
            (
                row.key,
                row.version,
                row.description,
                row.servers,
                row.settings,
                row.update_available,
                row.status
            ),
            (
                None,
                String::new(),
                String::new(),
                vec![],
                vec![],
                false,
                PluginStatus::Other(String::new())
            )
        );
        assert_eq!(row.source, PluginSource::Other(String::new()));
        assert_eq!(row.catalog_tier, None);
        let CapData::Toolsets(rows) =
            CapData::from_json(CapTab::Toolsets, &json!([toolset])).unwrap()
        else {
            panic!("toolsets expected")
        };
        assert_eq!(
            (&*rows[0].label, rows[0].configured, rows[0].tools.len()),
            ("", false, 0)
        );
        assert_eq!(
            (
                &*rows[0].description,
                &*rows[0].platform,
                &*rows[0].platform_label
            ),
            ("", "", "")
        );
    }
    for tab in [CapTab::Skills, CapTab::Toolsets, CapTab::Plugins] {
        for bad in [
            json!({}),
            json!({"name":null,"enabled":true}),
            json!({"name":1,"enabled":true}),
        ] {
            let value = if tab == CapTab::Plugins {
                json!({"plugins":[bad]})
            } else {
                json!([bad])
            };
            assert!(CapData::from_json(tab, &value).is_err());
        }
    }
    for bad in [
        json!({"name":"x"}),
        json!({"name":"x","enabled":null}),
        json!({"name":"x","enabled":"yes"}),
        json!({"name":"x","enabled":true,"usage":"3"}),
    ] {
        assert!(CapData::from_json(CapTab::Skills, &json!([bad])).is_err());
    }
    for bad in [
        json!({"name":"x"}),
        json!({"name":"x","enabled":null}),
        json!({"name":"x","enabled":"yes"}),
    ] {
        assert!(CapData::from_json(CapTab::Toolsets, &json!([bad])).is_err());
    }
    for bad in [
        json!({"name":"x","status":3}),
        json!({"name":"x","update_available":"yes"}),
        json!({"name":"x","key":false}),
    ] {
        assert!(CapData::from_json(CapTab::Plugins, &json!({"plugins":[bad]})).is_err());
    }
    for (tab, bad) in [
        (
            CapTab::Plugins,
            json!({"plugins":[{"name":"x","servers":"bad"}]}),
        ),
        (
            CapTab::Toolsets,
            json!([{"name":"x","enabled":true,"tools":"bad"}]),
        ),
    ] {
        assert!(CapData::from_json(tab, &bad).is_err());
    }
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        failure_persistence: None,
        ..Default::default()
    });
    runner
        .run(
            &("[a-z]{0,8}", "[a-z]{1,8}", "[a-z]{0,8}"),
            |(prefix, query, suffix)| {
                let name = format!("{prefix}{query}{suffix}");
                let mut state = CapabilitiesState::new("fixture".into(), None, CapTab::Skills);
                let load = state.update(CapInput::Open(CapTab::Skills), 0);
                let generation = match load[0] {
                    CapEffect::Fetch { generation, .. } => generation,
                    _ => panic!("skills fetch expected"),
                };
                state.update(
                    CapInput::Loaded {
                        tab: CapTab::Skills,
                        generation,
                        result: Ok(CapData::Skills(vec![SkillRow {
                            name: name.clone(),
                            description: String::new(),
                            category: Some("group".into()),
                            enabled: true,
                            usage: 0,
                            provenance: Provenance::Local,
                        }])),
                    },
                    0,
                );
                state.update(CapInput::Key(KeyAction::Filter), 1);
                for c in query.to_uppercase().chars() {
                    state.update(CapInput::Char(c), 1);
                }
                prop_assert!(
                    state
                        .view()
                        .rows
                        .iter()
                        .any(|r| r.name == name && !r.is_header)
                );
                Ok(())
            },
        )
        .unwrap();
}
