//! Profile routing derived from the vendored gateway contract.
use super::ParamInjector;
pub use hermes_contract::generated::profile_methods::PROFILE_METHODS;
pub fn profile_injector(profile: String) -> ParamInjector {
    std::sync::Arc::new(move |method, params| {
        if PROFILE_METHODS.contains(&method) {
            if params.is_null() {
                *params = serde_json::json!({});
            }
            if let Some(object) = params.as_object_mut() {
                object
                    .entry("profile")
                    .or_insert_with(|| serde_json::Value::String(profile.clone()));
            }
        }
    })
}
