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
    pub fn from_result(result: &ModelOptionsResult) -> RouteCatalog {
        let _ = result;
        todo!("model roles stub")
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

pub fn filter_routes(catalog: &RouteCatalog, step: &PickerStep, filter: &str) -> Vec<RouteRow> {
    let _ = (catalog, step, filter);
    todo!("model roles stub")
}
