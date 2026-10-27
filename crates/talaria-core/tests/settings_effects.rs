use talaria_core::settings::{ApplyEffect, ConfigSchemaView, apply_effect};
#[test]
fn every_entry_has_evidence_and_matches_the_schema() {
    let schema =
        ConfigSchemaView::from_json_str(include_str!("../../../fixtures/rest/config-schema.json"))
            .expect("schema");
    for key in ["display.skin", "approvals.mode"] {
        assert!(schema.fields.iter().any(|f| f.key == key), "{key}");
        let (effect, evidence) = apply_effect(key);
        assert_eq!(effect, ApplyEffect::Now);
        assert!(evidence.is_some_and(|e| !e.trim().is_empty()));
    }
    for field in &schema.fields {
        let (effect, evidence) = apply_effect(&field.key);
        if matches!(field.key.as_str(), "display.skin" | "approvals.mode") {
            assert_eq!(effect, ApplyEffect::Now);
            assert!(evidence.is_some_and(|value| !value.trim().is_empty()));
        } else {
            assert_eq!(effect, ApplyEffect::MayNeedRestart, "{}", field.key);
        }
    }
    for key in ["display.compact", "agent.max_turns", "unknown.setting"] {
        assert_eq!(apply_effect(key).0, ApplyEffect::MayNeedRestart);
    }
}
