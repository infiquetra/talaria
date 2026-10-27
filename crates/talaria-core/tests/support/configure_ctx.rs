#![allow(dead_code)]
use serde_json::{Value, json};
use talaria_core::{
    configure::*,
    settings::{ConfigSchemaView, FieldKind, FieldView},
};

pub fn detect(text: &str) -> bool {
    fn has_run(text: &str, prefix: &str, min: usize, allowed: fn(char) -> bool) -> bool {
        text.match_indices(prefix).any(|(start, _)| {
            text[start + prefix.len()..]
                .chars()
                .take_while(|c| allowed(*c))
                .count()
                >= min
        })
    }
    has_run(text, "sk-", 20, |c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }) || has_run(text, "ghp_", 30, |c| c.is_ascii_alphanumeric())
}

pub fn context() -> Context {
    let schema = ConfigSchemaView::from_json_str(include_str!(
        "../../../../fixtures/rest/config-schema.json"
    ))
    .unwrap();
    Context {
        profile: "fixture".into(),
        hermes_version: Some("0.22.4".into()),
        schema,
        effective: json!({"display":{"bell_on_complete":false,"compact":false}}),
        raw: json!({"display":{"bell_on_complete":false,"compact":false}}),
        defaults: json!({"display":{"bell_on_complete":false,"compact":false}}),
        inventory: Inventory {
            mcp_servers: Section::Available(vec![
                McpRow {
                    name: "alpha".into(),
                    kind: "stdio".into(),
                    enabled: true,
                    plugin: false,
                },
                McpRow {
                    name: "plug".into(),
                    kind: "stdio".into(),
                    enabled: true,
                    plugin: true,
                },
            ]),
            mcp_catalogue: Section::Available(vec![
                CatalogueRow {
                    name: "deepwiki".into(),
                    description: String::new(),
                    kind: "preset".into(),
                    installed: false,
                    enabled: false,
                    requires: vec![],
                },
                CatalogueRow {
                    name: "needs-key".into(),
                    description: String::new(),
                    kind: "preset".into(),
                    installed: false,
                    enabled: false,
                    requires: vec!["X_API_KEY".into()],
                },
            ]),
            skills: Section::Available(vec![CapRow {
                kind: CapKind::Skill,
                name: "notes".into(),
                enabled: true,
                description: String::new(),
            }]),
            toolsets: Section::Available(vec![CapRow {
                kind: CapKind::Toolset,
                name: "web".into(),
                enabled: true,
                description: String::new(),
            }]),
            plugins: Section::Available(vec![]),
            cron_jobs: Section::Available(vec![CronRow {
                job_id: "j1".into(),
                name: "daily".into(),
                schedule: "0 9 * * *".into(),
                state: "active".into(),
            }]),
            providers: Section::Available(vec![
                ProviderRow {
                    slug: "ollama-cloud".into(),
                    configured: true,
                },
                ProviderRow {
                    slug: "openrouter".into(),
                    configured: false,
                },
            ]),
            main_model: Section::Available(vec![MainModelRow {
                provider: "ollama-cloud".into(),
                model: "minimax-m3.1-flash".into(),
            }]),
            routes: Section::Available(vec![
                RouteRow {
                    slug: "ollama-cloud".into(),
                    authenticated: true,
                    models: vec!["minimax-m3.1-flash".into()],
                    api_url: String::new(),
                },
                RouteRow {
                    slug: "openrouter".into(),
                    authenticated: false,
                    models: vec![],
                    api_url: String::new(),
                },
            ]),
            profiles: Section::Available(vec!["fixture".into()]),
            connectors: Section::Unavailable("consent required".into()),
        },
    }
}

pub fn generated_context() -> Context {
    let mut ctx = context();
    let mut raw = serde_json::Map::new();
    ctx.schema.category_order = (0..15).map(|n| format!("category{n:02}")).collect();
    ctx.schema.fields = (0..825)
        .map(|n| FieldView {
            key: format!("key{n:04}{}", "x".repeat(33)),
            kind: FieldKind::String,
            description: String::new(),
            category: format!("category{:02}", n / 55),
            options: None,
            searchable: false,
            clearable: false,
        })
        .collect();
    for field in ctx.schema.fields.iter().take(200) {
        raw.insert(field.key.clone(), Value::String("v".repeat(190)));
    }
    ctx.raw = Value::Object(raw);
    ctx.inventory.mcp_servers = Section::Available(
        (0..300)
            .map(|n| McpRow {
                name: format!("{n:03}{}", "x".repeat(197)),
                kind: String::new(),
                enabled: true,
                plugin: false,
            })
            .collect(),
    );
    ctx.inventory.mcp_catalogue = Section::Available(
        (0..300)
            .map(|n| CatalogueRow {
                name: format!("{n:03}{}", "x".repeat(197)),
                description: String::new(),
                kind: String::new(),
                installed: false,
                enabled: false,
                requires: vec![],
            })
            .collect(),
    );
    for kind in [CapKind::Skill, CapKind::Toolset, CapKind::Plugin] {
        let values = (0..300)
            .map(|n| CapRow {
                kind,
                name: format!("{n:03}{}", "x".repeat(197)),
                enabled: true,
                description: String::new(),
            })
            .collect();
        match kind {
            CapKind::Skill => ctx.inventory.skills = Section::Available(values),
            CapKind::Toolset => ctx.inventory.toolsets = Section::Available(values),
            CapKind::Plugin => ctx.inventory.plugins = Section::Available(values),
        }
    }
    ctx.inventory.cron_jobs = Section::Available(
        (0..300)
            .map(|n| CronRow {
                job_id: format!("j{n}"),
                name: "x".repeat(200),
                schedule: "daily".into(),
                state: "active".into(),
            })
            .collect(),
    );
    ctx.inventory.providers = Section::Available(
        (0..300)
            .map(|n| ProviderRow {
                slug: format!("{n:03}{}", "x".repeat(197)),
                configured: true,
            })
            .collect(),
    );
    ctx.inventory.profiles = Section::Available((0..300).map(|_| "x".repeat(200)).collect());
    ctx.inventory.connectors =
        Section::Available((0..300).map(|_| ("x".repeat(200), true)).collect());
    ctx
}
