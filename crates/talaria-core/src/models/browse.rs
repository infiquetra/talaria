//! Pure grouping and presentation facts above the unchanged gateway catalogue.

use std::collections::BTreeSet;

use super::{Catalog, CatalogKind, CatalogRow, DisplayZone, Effort, PickerRow, RowKey};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListScope {
    #[default]
    All,
    Starred,
    Configured,
    SetUp,
}

impl ListScope {
    pub(super) fn step(self, backwards: bool) -> Self {
        let scopes = [Self::All, Self::Starred, Self::Configured, Self::SetUp];
        let index = scopes.iter().position(|scope| *scope == self).unwrap_or(0);
        scopes[(index + if backwards { 3 } else { 1 }) % scopes.len()]
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Starred => "Starred",
            Self::Configured => "Configured",
            Self::SetUp => "Set up",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowseCounts {
    pub models: usize,
    pub providers: usize,
    pub starred: usize,
    pub configured: usize,
    pub setup: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowseView<'a> {
    pub scope: ListScope,
    pub counts: BrowseCounts,
    pub details: &'a [String],
    pub details_open: bool,
    pub details_scroll: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RowFacts {
    pub current: bool,
    pub offers_effort: bool,
    pub not_listed: bool,
    pub unavailable: bool,
}

pub(super) struct BrowseRows {
    pub rows: Vec<PickerRow>,
    pub facts: Vec<RowFacts>,
    pub counts: BrowseCounts,
}

pub(super) fn same_route(left: &RowKey, right: &RowKey) -> bool {
    match (left, right) {
        (RowKey::NotSetUp { provider: a }, RowKey::NotSetUp { provider: b }) => a == b,
        (
            RowKey::Favourite {
                provider: a,
                model: x,
            }
            | RowKey::Model {
                provider: a,
                model: x,
            },
            RowKey::Favourite {
                provider: b,
                model: y,
            }
            | RowKey::Model {
                provider: b,
                model: y,
            },
        ) => a == b && x == y,
        _ => false,
    }
}

fn matches(row: &CatalogRow, terms: &[String]) -> bool {
    let text = format!("{} {} {}", row.provider_name, row.provider_slug, row.model).to_lowercase();
    terms.iter().all(|term| text.contains(term))
}

/// Transform catalogue rows without changing their source order or switch semantics.
pub(super) fn rows(
    catalog: &Catalog,
    favourites: &[String],
    zone: DisplayZone,
    scope: ListScope,
    filter: &str,
) -> BrowseRows {
    let configured = catalog
        .result
        .providers
        .iter()
        .filter(|p| p.authenticated == Some(true));
    let counts = BrowseCounts {
        models: configured.clone().map(|p| p.models.len()).sum(),
        providers: catalog.result.providers.len(),
        starred: favourites
            .iter()
            .filter_map(|f| f.split_once("::"))
            .collect::<BTreeSet<_>>()
            .len(),
        configured: configured.count(),
        setup: catalog
            .result
            .providers
            .iter()
            .filter(|p| p.authenticated != Some(true))
            .count(),
    };
    let mut result = BrowseRows {
        rows: Vec::new(),
        facts: Vec::new(),
        counts,
    };
    let terms: Vec<_> = filter.split_whitespace().map(str::to_lowercase).collect();
    let mut seen = BTreeSet::new();
    let mut visible_providers = BTreeSet::new();
    let mut pending_header = None;
    for row in catalog.rows(favourites, zone) {
        if row.kind == CatalogKind::Header {
            let allowed = if row.provider_slug.is_empty() {
                if row.text == "Favourites" {
                    matches!(scope, ListScope::All | ListScope::Starred)
                } else {
                    matches!(scope, ListScope::All | ListScope::SetUp)
                }
            } else {
                matches!(scope, ListScope::All | ListScope::Configured)
            };
            pending_header = allowed.then(|| PickerRow {
                key: None,
                text: if row.text == "Favourites" {
                    "Starred".into()
                } else if row.text.starts_with("Not set up (") {
                    format!("Needs setup ({})", counts.setup)
                } else {
                    std::iter::once(row.text.clone())
                        .chain(row.segments.clone())
                        .collect::<Vec<_>>()
                        .join(" · ")
                },
                badges: Vec::new(),
                price: None,
                dim: false,
                header: true,
            });
            // A configured provider can return no models; retain an honest non-action row.
            if allowed
                && !row.provider_slug.is_empty()
                && catalog
                    .result
                    .providers
                    .iter()
                    .any(|p| p.slug == row.provider_slug && p.models.is_empty())
                && matches(&row, &terms)
            {
                if let Some(header) = pending_header.take() {
                    result.rows.push(header);
                    result.facts.push(RowFacts::default());
                }
                result.rows.push(PickerRow {
                    key: None,
                    text: "No models listed for this provider; Ctrl+R reloads".into(),
                    badges: Vec::new(),
                    price: None,
                    dim: true,
                    header: false,
                });
                result.facts.push(RowFacts::default());
                visible_providers.insert(row.provider_slug.clone());
            }
            continue;
        }
        let allowed = match row.kind {
            CatalogKind::Favourite => matches!(scope, ListScope::All | ListScope::Starred),
            CatalogKind::Model => {
                matches!(scope, ListScope::All | ListScope::Configured)
                    && !(scope == ListScope::All && row.favourite)
            }
            CatalogKind::NotSetUp => matches!(scope, ListScope::All | ListScope::SetUp),
            CatalogKind::Header => false,
        };
        if !allowed || !matches(&row, &terms) {
            continue;
        }
        if !seen.insert((row.provider_slug.clone(), row.model.clone())) {
            continue;
        }
        visible_providers.insert(row.provider_slug.clone());
        if let Some(header) = pending_header.take() {
            result.rows.push(header);
            result.facts.push(RowFacts::default());
        }
        let facts = RowFacts {
            current: row.current,
            offers_effort: !row.badges.iter().any(|b| b == "no effort"),
            not_listed: row.badges.iter().any(|b| b == "not listed"),
            unavailable: row.badges.iter().any(|b| b == "unavailable"),
        };
        let badge = if facts.not_listed {
            "not listed".into()
        } else if facts.unavailable {
            "unavailable".into()
        } else if let Some(limit) = row.badges.iter().find(|b| b.starts_with("rate-limited")) {
            limit.clone()
        } else if row.kind == CatalogKind::NotSetUp {
            "needs setup".into()
        } else if !facts.offers_effort {
            "not offered".into()
        } else {
            "effort ?".into()
        };
        let key = match row.kind {
            CatalogKind::Favourite => RowKey::Favourite {
                provider: row.provider_slug.clone(),
                model: row.model.clone(),
            },
            CatalogKind::Model => RowKey::Model {
                provider: row.provider_slug.clone(),
                model: row.model.clone(),
            },
            _ => RowKey::NotSetUp {
                provider: row.provider_slug.clone(),
            },
        };
        let text = if row.kind == CatalogKind::NotSetUp {
            row.provider_name.clone()
        } else if row.kind == CatalogKind::Favourite {
            format!(
                "★{} {} / {}",
                if row.current { "●" } else { " " },
                row.provider_name,
                row.model
            )
        } else {
            row.text.clone()
        };
        result.rows.push(PickerRow {
            key: Some(key),
            text,
            badges: vec![badge],
            price: row.price,
            dim: row.dim,
            header: false,
        });
        result.facts.push(facts);
    }
    result.counts.models = result
        .rows
        .iter()
        .zip(&result.facts)
        .filter(|(row, facts)| {
            !facts.not_listed
                && matches!(
                    row.key,
                    Some(RowKey::Model { .. } | RowKey::Favourite { .. })
                )
        })
        .count();
    result.counts.providers = visible_providers.len();
    result
}

pub(super) fn details(
    catalog: &Catalog,
    row: Option<&PickerRow>,
    facts: RowFacts,
    requested: Option<Effort>,
    pending: bool,
    unknown: bool,
) -> Vec<String> {
    let Some(key) = row.and_then(|r| r.key.as_ref()) else {
        return vec!["No action until a valid row is selected.".into()];
    };
    let (slug, model) = match key {
        RowKey::NotSetUp { provider } => {
            let p = catalog
                .result
                .providers
                .iter()
                .find(|p| &p.slug == provider);
            let mut lines = vec![
                format!(
                    "Selected: {}",
                    catalog.display_name(provider).unwrap_or(provider)
                ),
                "Status: needs setup".into(),
                "Enter: set up in /providers".into(),
            ];
            if let Some(env) = p.and_then(|p| p.key_env.as_deref()) {
                lines.push(format!("Credential: needs {env}"));
            }
            return lines;
        }
        RowKey::Favourite { provider, model } | RowKey::Model { provider, model } => {
            (provider, model)
        }
    };
    let provider = catalog.result.providers.iter().find(|p| &p.slug == slug);
    let mut lines = vec![
        format!(
            "Selected: {} / {model}",
            catalog.display_name(slug).unwrap_or(slug)
        ),
        format!(
            "Requested: {}{}",
            requested.map_or("unknown", Effort::as_wire),
            if pending { "; pending turn end" } else { "" }
        ),
        if requested.is_none() {
            "Support: could not be read".into()
        } else {
            "Support: unverified for this route".into()
        },
        "Applied: unknown".into(),
        "Context: unknown".into(),
    ];
    if unknown {
        lines.insert(1, "Outcome unknown; reloading".into());
    }
    if !facts.offers_effort {
        lines.insert(3, "Catalogue: reasoning not advertised".into());
    }
    if facts.not_listed {
        lines.push("Availability: not listed; Ctrl+F removes this star".into());
    }
    if facts.unavailable {
        lines.push("Availability: unavailable on this provider's plan; Enter explains".into());
    }
    if let Some(badge) = row
        .and_then(|r| r.badges.first())
        .filter(|b| b.starts_with("rate-limited"))
    {
        lines.push(format!("Availability: {badge}"));
    }
    if let Some(price) = row.and_then(|r| r.price.as_deref()) {
        lines.push(format!("Price: {price}"));
    }
    if let Some(p) = provider {
        if p.capabilities
            .as_ref()
            .and_then(|caps| caps.get(model))
            .is_some_and(|caps| caps.fast)
        {
            lines.push("Catalogue: fast".into());
        }
        if let Some(warning) = &p.warning {
            lines.push(format!("Warning: {warning}"));
        }
        if let Some(total) = p
            .total_models
            .filter(|total| *total > p.models.len() as i64)
        {
            lines.push(format!(
                "Catalogue: showing {} of {total}; search covers loaded models",
                p.models.len()
            ));
        }
        if p.authenticated == Some(true) {
            lines.push("Provider: catalogue-authenticated; credential not verified".into());
        }
    }
    lines.push("F2/Ctrl+E details; Esc back".into());
    lines
}
