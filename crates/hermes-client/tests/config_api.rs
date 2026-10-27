#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    config_api::{self, ConfigSchema, FieldKind},
    rest::{RestClient, RestOptions},
};
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn routes_bodies_and_order_preserved() {
    let schema: ConfigSchema = serde_json::from_str(r#"{
        "fields": {
            "zeta": {"type": "boolean", "category": "display", "description": "First", "unrecognised": 1},
            "alpha": {"type": "future-kind"},
            "last": {}
        },
        "category_order": ["display", "general"]
    }"#).unwrap();
    assert_eq!(
        schema
            .fields
            .iter()
            .map(|f| f.key.as_str())
            .collect::<Vec<_>>(),
        ["zeta", "alpha", "last"]
    );
    assert_eq!(schema.category_order, ["display", "general"]);
    assert_eq!(schema.fields[0].kind, FieldKind::Boolean);
    assert_eq!(
        schema.fields[1].kind,
        FieldKind::Other("future-kind".into())
    );
    assert_eq!(schema.fields[1].description, "alpha");
    assert_eq!(schema.fields[1].category, "general");
    assert_eq!(schema.fields[2].kind, FieldKind::String);
    assert!(!schema.fields[2].searchable);
    assert!(!schema.fields[2].clearable);

    let mock = MockRest::start().await;
    let minter = Arc::new(CountingMinter::default());
    let rest = RestClient::new(&mock.origin(), minter.clone(), RestOptions::default()).unwrap();
    let recorded_schema = config_api::config_schema(&rest).await.unwrap();
    assert!(!recorded_schema.fields.is_empty());
    let defaults = config_api::config_defaults(&rest).await.unwrap();
    let effective = config_api::config_read(&rest, true).await.unwrap();
    let raw = config_api::config_read(&rest, false).await.unwrap();
    assert!(defaults.is_object() && effective.is_object() && raw.is_object());
    let patch = json!({"display":{"compact":true}});
    config_api::config_write(&rest, &patch).await.unwrap();
    let requests = mock.requests().await;
    assert_eq!(
        requests
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("GET", "/api/config/schema"),
            ("GET", "/api/config/defaults"),
            ("GET", "/api/config"),
            ("GET", "/api/config?include_defaults=false"),
            ("PUT", "/api/config")
        ]
    );
    assert_eq!(minter.count(), 5);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[4].body).unwrap(),
        json!({"config":patch})
    );
    for request in requests {
        assert!(!request.path.contains("profile="));
        if !request.body.is_empty() {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert!(body.get("profile").is_none());
        }
    }
}
