//! Endpoint rows and form drafts. The reducer does not fill them yet.

use serde_json::Value;

use super::model::ApiKeyChange;

/// One custom endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Endpoint id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Base URL.
    pub base_url: String,
    /// Selected model.
    pub model: String,
    /// Known models.
    pub models: Vec<String>,
    /// API mode, when set.
    pub api_mode: Option<String>,
    /// Context length, when set.
    pub context_length: Option<u64>,
    /// Whether models are discovered.
    pub discover_models: bool,
    /// Whether a key is stored.
    pub has_api_key: bool,
    /// Whether this endpoint is the current provider.
    pub is_current: bool,
    /// Source tag. `custom_providers` is the older format.
    pub source: String,
}

/// Endpoint list plus the current selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointList {
    /// Saved endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Current provider slug. Empty when none.
    pub current_provider: String,
    /// Current model id.
    pub current_model: String,
}

/// Form contents a check or save carries. The key text is not here.
#[derive(Clone, Debug, PartialEq)]
pub struct EndpointDraft {
    /// Row id. Empty when adding.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Base URL.
    pub base_url: String,
    /// Model id.
    pub model: String,
    /// What to do with the saved key.
    pub api_key: ApiKeyChange,
    /// API mode. Empty string means auto when the field changed.
    pub api_mode: Option<String>,
    /// Context length, when the field has one.
    pub context_length: Option<u64>,
    /// Discover-models flag.
    pub discover_models: bool,
    /// Models from a test in this form.
    pub models: Option<Vec<String>>,
    /// Model details from a test in this form.
    pub model_details: Option<Vec<Value>>,
}

/// Result of testing an endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct EndpointCheck {
    /// Whether the test passed.
    pub ok: bool,
    /// Whether the address answered.
    pub reachable: bool,
    /// Provider message.
    pub message: String,
    /// Discovered models.
    pub models: Vec<String>,
    /// Extra model details.
    pub model_details: Vec<Value>,
    /// Address that answered.
    pub resolved_base_url: Option<String>,
}
