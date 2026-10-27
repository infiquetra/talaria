#[path = "support/configure_ctx.rs"]
mod configure_ctx;
use configure_ctx::{context, detect};
use serde_json::{Value, json};
use talaria_core::{
    configure::*,
    settings::{FieldKind, FieldView, ValueKind},
};

fn proposal(changes: Vec<Change>) -> Proposal {
    Proposal {
        summary: "change".into(),
        profile: None,
        changes,
    }
}
fn unavailable<T>() -> Section<T> {
    Section::Unavailable("offline".into())
}
fn set(key: &str, value: Value) -> Change {
    Change::Set {
        key: key.into(),
        value,
        why: None,
    }
}
fn rows(changes: Vec<Change>) -> Vec<Row> {
    validate(&proposal(changes), &context(), detect).unwrap()
}
fn field() -> FieldView {
    FieldView {
        key: "test".into(),
        kind: FieldKind::String,
        description: String::new(),
        category: String::new(),
        options: Some(vec![json!("a"), json!("b")]),
        searchable: false,
        clearable: false,
    }
}

#[test]
fn set_rows_follow_the_schema() {
    let rows = rows(vec![
        set("display.bell_on_complete", json!(true)),
        set("display.compact", json!(false)),
        set("not_a_key", json!(true)),
        set("model", json!("a")),
    ]);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].status, RowStatus::Ready);
    assert_eq!(rows[0].effect_label.as_deref(), Some("may need a restart"));
    assert_eq!(rows[1].status, RowStatus::NoChange);
    assert!(matches!(&rows[2].status, RowStatus::Invalid(s) if s.contains("not a Hermes setting")));
    assert!(matches!(&rows[3].status, RowStatus::Invalid(s) if s.contains("main_model")));
    let mut ctx = context();
    let mut list_field = field();
    list_field.key = "custom.list".into();
    list_field.kind = FieldKind::List;
    ctx.schema.fields.push(list_field);
    ctx.effective["custom"] = json!({"list":["one"]});
    ctx.raw["custom"] = json!({"list":["one"]});
    let emptied = validate(&proposal(vec![set("custom.list", json!([]))]), &ctx, detect).unwrap();
    assert_eq!(emptied[0].status, RowStatus::Ready);
    assert!(emptied[0].destructive);
    ctx.raw["display"]["compact"] = json!("${NAME}");
    let readonly = validate(
        &proposal(vec![set("display.compact", json!(true))]),
        &ctx,
        detect,
    )
    .unwrap();
    assert!(matches!(&readonly[0].status, RowStatus::Invalid(s) if s.contains("NAME")));
    let mut ctx = context();
    ctx.effective["display"]["compact"] = json!(true);
    let boolean = validate(
        &proposal(vec![set("display.compact", json!(false))]),
        &ctx,
        detect,
    )
    .unwrap();
    assert_eq!(boolean[0].status, RowStatus::Ready);
    assert!(!boolean[0].destructive);
    let mut ctx = context();
    let mut owned = field();
    owned.key = "delegation.test".into();
    owned.kind = FieldKind::Boolean;
    ctx.schema.fields.push(owned);
    ctx.effective["delegation"] = json!({"test":false});
    let result = validate(
        &proposal(vec![set("delegation.test", json!(true))]),
        &ctx,
        detect,
    )
    .unwrap();
    assert!(matches!(&result[0].status, RowStatus::Invalid(s) if s.contains("own screen")));
    let mut ctx = context();
    let mut held = field();
    held.key = "custom.held".into();
    ctx.schema.fields.push(held);
    ctx.effective["custom"] = json!({"held":{"api_key":"value"}});
    let result = validate(
        &proposal(vec![set("custom.held", json!("new"))]),
        &ctx,
        detect,
    )
    .unwrap();
    assert!(matches!(&result[0].status, RowStatus::Refused(_)));
    let f = field();
    for (kind, value) in [
        (ValueKind::Bool, json!(true)),
        (ValueKind::Integer, json!(2.0)),
        (ValueKind::Float, json!(2.5)),
        (ValueKind::Text, json!("a\tb")),
        (ValueKind::StringList, json!(["a"])),
        (ValueKind::JsonList, json!([1, {}])),
        (ValueKind::Nullable, Value::Null),
        (ValueKind::Nullable, json!(true)),
        (ValueKind::Choice, json!("a")),
    ] {
        assert!(check_value(&kind, &f, &value).is_ok(), "{kind:?}");
    }
    for (kind, value) in [
        (ValueKind::Bool, json!("true")),
        (ValueKind::Integer, json!(1.5)),
        (ValueKind::Text, json!("a\nb")),
        (ValueKind::StringList, json!([1])),
        (ValueKind::Nullable, json!([])),
        (ValueKind::Nullable, json!({})),
        (ValueKind::Choice, json!("c")),
    ] {
        assert!(check_value(&kind, &f, &value).is_err(), "{kind:?}");
    }
}

#[test]
fn secrets_are_refused_and_never_echoed() {
    let sentinel = "sk-or-v1-talaria-sentinel-0000000000000000";
    let rows = rows(vec![
        set("api_key", json!("value")),
        set("display.compact", json!({"nested":{"api_key":"value"}})),
        set("display.bell_on_complete", json!(sentinel)),
        Change::McpAdd {
            name: "new".into(),
            source: McpSource::Forbidden("env"),
            why: None,
        },
    ]);
    assert_eq!(rows.len(), 4);
    for row in &rows {
        assert!(matches!(row.status, RowStatus::Refused(_)));
        assert!(!format!("{row:?}").contains(sentinel));
        assert!(!row.text.contains(sentinel));
    }
    assert_eq!(withhold(sentinel, detect), WITHHELD);
}

#[test]
fn mcp_rows() {
    let rows = rows(vec![
        Change::McpAdd {
            name: "alpha".into(),
            source: McpSource::Preset("deepwiki".into()),
            why: None,
        },
        Change::McpAdd {
            name: "new".into(),
            source: McpSource::Preset("needs-key".into()),
            why: None,
        },
        Change::McpRemove {
            name: "plug".into(),
            why: None,
        },
        Change::McpEnable {
            name: "alpha".into(),
            enabled: false,
            why: None,
        },
    ]);
    assert_eq!(rows.len(), 4);
    assert!(matches!(rows[0].status, RowStatus::Invalid(_)));
    assert_eq!(rows[1].status, RowStatus::HandOff);
    assert!(matches!(rows[2].status, RowStatus::Invalid(_)));
    assert!(matches!(rows[3].status, RowStatus::Invalid(_))); // same identity as the first row
    let single = validate(
        &proposal(vec![Change::McpEnable {
            name: "alpha".into(),
            enabled: false,
            why: None,
        }]),
        &context(),
        detect,
    )
    .unwrap();
    assert_eq!(single[0].status, RowStatus::Ready);
    assert!(single[0].destructive);
    for (name, source, ready) in [
        ("new", McpSource::Preset("deepwiki".into()), true),
        ("bad name", McpSource::Preset("deepwiki".into()), false),
        ("new", McpSource::Preset("missing".into()), false),
        (
            "new",
            McpSource::Command {
                command: String::new(),
                args: vec![],
            },
            false,
        ),
        (
            "new",
            McpSource::Url {
                url: "http://127.0.0.1".into(),
                oauth: false,
            },
            true,
        ),
        (
            "new",
            McpSource::Url {
                url: "ftp://127.0.0.1".into(),
                oauth: false,
            },
            false,
        ),
    ] {
        let result = validate(
            &proposal(vec![Change::McpAdd {
                name: name.into(),
                source,
                why: None,
            }]),
            &context(),
            detect,
        )
        .unwrap();
        assert_eq!(result[0].status == RowStatus::Ready, ready, "{name}");
    }
}

#[test]
fn capability_cron_and_model_rows() {
    let rows = rows(vec![
        Change::Toggle {
            kind: CapKind::Skill,
            name: "notes".into(),
            enabled: true,
            why: None,
        },
        Change::CronAdd {
            name: "new".into(),
            schedule: "daily".into(),
            prompt: "hello".into(),
            repeat: Some(10_000),
            deliver: None,
            why: None,
        },
        Change::CronAdd {
            name: "other".into(),
            schedule: "daily".into(),
            prompt: "hello".into(),
            repeat: None,
            deliver: Some("remote".into()),
            why: None,
        },
        Change::CronPause {
            job_id: "missing".into(),
            why: None,
        },
        Change::MainModel {
            provider: "openrouter".into(),
            model: "model".into(),
            why: None,
        },
    ]);
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].status, RowStatus::NoChange);
    assert!(matches!(rows[1].status, RowStatus::Invalid(_)));
    assert_eq!(rows[2].status, RowStatus::HandOff);
    assert!(matches!(rows[3].status, RowStatus::Invalid(_)));
    assert_eq!(rows[4].status, RowStatus::HandOff);
    for (name, schedule, prompt, repeat, valid) in [
        ("", "daily", "hello", None, false),
        ("new", "", "hello", None, false),
        ("new", "daily", "", None, false),
        ("new", "daily", "hello", Some(1), true),
        ("new", "daily", "hello", Some(9_999), true),
        ("new", "daily", "hello", Some(10_000), false),
    ] {
        let result = validate(
            &proposal(vec![Change::CronAdd {
                name: name.into(),
                schedule: schedule.into(),
                prompt: prompt.into(),
                repeat,
                deliver: None,
                why: None,
            }]),
            &context(),
            detect,
        )
        .unwrap();
        assert_eq!(
            result[0].status == RowStatus::Ready,
            valid,
            "{name} {schedule} {repeat:?}"
        );
    }
    let same = validate(
        &proposal(vec![Change::MainModel {
            provider: "ollama-cloud".into(),
            model: "minimax-m3.1-flash".into(),
            why: None,
        }]),
        &context(),
        detect,
    )
    .unwrap();
    assert_eq!(same[0].status, RowStatus::NoChange);
    let pause = validate(
        &proposal(vec![Change::CronPause {
            job_id: "j1".into(),
            why: None,
        }]),
        &context(),
        detect,
    )
    .unwrap();
    assert_eq!(pause[0].status, RowStatus::Ready);
    assert!(pause[0].destructive);
}

#[test]
fn open_rows_are_hand_offs() {
    let screens = [
        Screen::Providers,
        Screen::Mcp,
        Screen::Capabilities,
        Screen::Cron,
        Screen::Profiles,
        Screen::Connectors,
        Screen::Settings,
    ];
    let rows = rows(
        screens
            .into_iter()
            .map(|screen| Change::Open {
                screen,
                target: None,
                why: None,
            })
            .collect(),
    );
    assert_eq!(rows.len(), screens.len());
    assert!(rows.iter().all(|row| row.status == RowStatus::HandOff));
    for (screen, expected) in [
        (Screen::Providers, "Providers"),
        (Screen::Mcp, "Mcp"),
        (Screen::Capabilities, "Capabilities"),
        (Screen::Cron, "Cron"),
        (Screen::Profiles, "Profiles"),
        (Screen::Connectors, "Connectors"),
        (Screen::Settings, "Config"),
    ] {
        assert!(format!("{:?}", screen_action(screen, None)).starts_with(expected));
    }
}

#[test]
fn proposal_limits_and_profile() {
    let mut p = proposal(vec![set("display.compact", json!(true)); 21]);
    assert_eq!(
        validate(&p, &context(), detect),
        Err(ProposalError::TooMany(21))
    );
    p.changes.truncate(1);
    p.profile = Some("default".into());
    assert_eq!(
        validate(&p, &context(), detect),
        Err(ProposalError::WrongProfile {
            named: "default".into(),
            connected: "fixture".into()
        })
    );
    let rows = rows(vec![
        set("display.compact", json!(true)),
        set("display.compact", json!(false)),
    ]);
    assert!(matches!(&rows[1].status, RowStatus::Invalid(s) if s.contains("duplicate of row 1")));
    let mut ctx = context();
    ctx.inventory.cron_jobs = Section::Unavailable("offline".into());
    let result = validate(
        &proposal(vec![Change::CronPause {
            job_id: "j1".into(),
            why: None,
        }]),
        &ctx,
        detect,
    )
    .unwrap();
    assert!(matches!(&result[0].status, RowStatus::Invalid(s) if s.contains("could not read")));
    let pairs = vec![
        (
            Change::McpEnable {
                name: "alpha".into(),
                enabled: false,
                why: None,
            },
            Change::McpRemove {
                name: "alpha".into(),
                why: None,
            },
        ),
        (
            Change::Toggle {
                kind: CapKind::Skill,
                name: "notes".into(),
                enabled: false,
                why: None,
            },
            Change::Toggle {
                kind: CapKind::Skill,
                name: "notes".into(),
                enabled: true,
                why: None,
            },
        ),
        (
            Change::CronPause {
                job_id: "j1".into(),
                why: None,
            },
            Change::CronRemove {
                job_id: "j1".into(),
                why: None,
            },
        ),
        (
            Change::CronAdd {
                name: "new".into(),
                schedule: "daily".into(),
                prompt: "hello".into(),
                repeat: None,
                deliver: None,
                why: None,
            },
            Change::CronAdd {
                name: "new".into(),
                schedule: "weekly".into(),
                prompt: "hello".into(),
                repeat: None,
                deliver: None,
                why: None,
            },
        ),
        (
            Change::MainModel {
                provider: "ollama-cloud".into(),
                model: "a".into(),
                why: None,
            },
            Change::MainModel {
                provider: "openrouter".into(),
                model: "b".into(),
                why: None,
            },
        ),
        (
            Change::Open {
                screen: Screen::Settings,
                target: None,
                why: None,
            },
            Change::Open {
                screen: Screen::Settings,
                target: None,
                why: None,
            },
        ),
    ];
    for (first, second) in pairs {
        let result = validate(&proposal(vec![first, second]), &context(), detect).unwrap();
        assert!(
            matches!(&result[1].status, RowStatus::Invalid(s) if s.contains("duplicate of row 1"))
        );
    }
    let mut ctx = context();
    ctx.inventory.mcp_servers = unavailable();
    let change = Change::McpEnable {
        name: "alpha".into(),
        enabled: false,
        why: None,
    };
    assert!(
        matches!(&validate(&proposal(vec![change]), &ctx, detect).unwrap()[0].status, RowStatus::Invalid(s) if s.contains("could not read"))
    );
    let mut ctx = context();
    ctx.inventory.mcp_catalogue = unavailable();
    let change = Change::McpAdd {
        name: "new".into(),
        source: McpSource::Preset("deepwiki".into()),
        why: None,
    };
    assert!(
        matches!(&validate(&proposal(vec![change]), &ctx, detect).unwrap()[0].status, RowStatus::Invalid(s) if s.contains("could not read"))
    );
    for kind in [CapKind::Skill, CapKind::Toolset, CapKind::Plugin] {
        let mut ctx = context();
        match kind {
            CapKind::Skill => ctx.inventory.skills = unavailable(),
            CapKind::Toolset => ctx.inventory.toolsets = unavailable(),
            CapKind::Plugin => ctx.inventory.plugins = unavailable(),
        }
        let change = Change::Toggle {
            kind,
            name: "item".into(),
            enabled: false,
            why: None,
        };
        assert!(
            matches!(&validate(&proposal(vec![change]), &ctx, detect).unwrap()[0].status, RowStatus::Invalid(s) if s.contains("could not read"))
        );
    }
    for routes in [false, true] {
        let mut ctx = context();
        if routes {
            ctx.inventory.routes = Section::Unavailable("offline".into());
        } else {
            ctx.inventory.main_model = Section::Unavailable("offline".into());
        }
        let change = Change::MainModel {
            provider: "ollama-cloud".into(),
            model: "model".into(),
            why: None,
        };
        assert!(
            matches!(&validate(&proposal(vec![change]), &ctx, detect).unwrap()[0].status, RowStatus::Invalid(s) if s.contains("could not read"))
        );
    }
}
