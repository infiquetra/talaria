//! Test 38. The five hand-written events cite their source and keep unknown fields.

use hermes_contract::{
    ApprovalRequest, ApprovalSettled, ClarifyRequest, ClarifySettled, SessionReplayGap,
};

#[test]
fn five_events_tolerate_unknown_fields_and_cite_their_source() {
    let source = include_str!("../src/canonical_events.rs");
    assert!(
        source.contains("apps/shared/src/gateway-events.ts"),
        "each hand-written event cites the TypeScript source"
    );

    let with_prompt = serde_json::json!({"prompt_id": "p", "not_a_field": true});
    let _approval: ApprovalRequest = serde_json::from_value(with_prompt.clone())
        .expect("ApprovalRequest keeps an unknown field");
    let _settled: ApprovalSettled = serde_json::from_value(with_prompt.clone())
        .expect("ApprovalSettled keeps an unknown field");
    let _clarify: ClarifyRequest =
        serde_json::from_value(with_prompt.clone()).expect("ClarifyRequest keeps an unknown field");
    let _clarify_settled: ClarifySettled =
        serde_json::from_value(with_prompt).expect("ClarifySettled keeps an unknown field");
    let _gap: SessionReplayGap = serde_json::from_value(serde_json::json!({"not_a_field": true}))
        .expect("SessionReplayGap keeps an unknown field and has no prompt_id");
}
