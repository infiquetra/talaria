//! Provider and model lists for the route picker.

use hermes_contract::schemas::ModelOptionsResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteProvider {
    pub slug: String,
    pub name: String,
    pub models: Vec<String>,
    pub authenticated: bool,
    pub is_user_defined: bool,
    pub api_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteCatalog {
    pub providers: Vec<RouteProvider>,
}

impl RouteCatalog {
    /// Keeps Hermes's order and drops the virtual Mixture of Agents provider.
    pub fn from_result(result: &ModelOptionsResult) -> RouteCatalog {
        let providers = result
            .providers
            .iter()
            .filter(|provider| provider.slug != "moa")
            .map(|provider| RouteProvider {
                slug: provider.slug.clone(),
                name: if provider.name.trim().is_empty() {
                    provider.slug.clone()
                } else {
                    provider.name.clone()
                },
                models: provider.models.clone(),
                authenticated: provider.authenticated.unwrap_or(false),
                is_user_defined: provider.is_user_defined.unwrap_or(false),
                api_url: provider.api_url.clone().unwrap_or_default(),
            })
            .collect();
        RouteCatalog { providers }
    }

    pub(super) fn provider(&self, slug: &str) -> Option<&RouteProvider> {
        self.providers.iter().find(|provider| provider.slug == slug)
    }

    /// True when the catalogue lists this exact route.
    pub(super) fn lists(&self, provider: &str, model: &str) -> bool {
        self.provider(provider)
            .is_some_and(|row| row.models.iter().any(|candidate| candidate == model))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerStep {
    Providers,
    Models { provider: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteRow {
    Provider {
        slug: String,
        name: String,
        choosable: bool,
    },
    Model {
        provider: String,
        model: String,
        api_url: String,
    },
}

/// Case-insensitive substring match; the order is the catalogue's.
pub fn filter_routes(catalog: &RouteCatalog, step: &PickerStep, filter: &str) -> Vec<RouteRow> {
    let needle = filter.trim().to_lowercase();
    match step {
        PickerStep::Providers => catalog
            .providers
            .iter()
            .filter(|provider| {
                needle.is_empty()
                    || provider.name.to_lowercase().contains(&needle)
                    || provider.slug.to_lowercase().contains(&needle)
            })
            .map(|provider| RouteRow::Provider {
                slug: provider.slug.clone(),
                name: provider.name.clone(),
                choosable: provider.authenticated && !provider.models.is_empty(),
            })
            .collect(),
        PickerStep::Models { provider } => {
            let Some(row) = catalog.provider(provider) else {
                return Vec::new();
            };
            row.models
                .iter()
                .filter(|model| needle.is_empty() || model.to_lowercase().contains(&needle))
                .map(|model| RouteRow::Model {
                    provider: row.slug.clone(),
                    model: model.clone(),
                    api_url: row.api_url.clone(),
                })
                .collect()
        }
    }
}
