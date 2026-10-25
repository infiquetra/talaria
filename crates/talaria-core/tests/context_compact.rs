//! Red-run tests 15 to 19 (#234): compaction payloads, replies and resends.

use hermes_contract::schemas::SessionMutateResult;
use serde_json::{Value, json};
use talaria_core::context::compact::{
    CompactEnd, CompactResult, compact_end, compact_payload, interpret, refusal_text, settle_as,
};
use talaria_core::mutation::{MutationLedger, MutationOutcome};

const SID: &str = "local-session-1";

fn reply(name: &str) -> Value {
    let text = match name {
        "preview" => include_str!("data/context/preview.json"),
        "noop" => include_str!("data/context/noop.json"),
        "committed" => include_str!("data/context/committed.json"),
        _ => panic!("unknown compaction reply {name}"),
    };
    serde_json::from_str(text).expect("test data parses")
}

fn refused(reason: &str, field: Option<&str>) -> MutationOutcome {
    MutationOutcome::Refused {
        reason: reason.to_owned(),
        field: field.map(str::to_owned),
        detail: None,
        rpc_code: 4001,
    }
}

#[test]
fn test_15_compact_payload_trims_or_stays_empty() {
    assert!(compact_payload("   ").is_empty());
    assert_eq!(
        compact_payload("focus on the tests "),
        serde_json::from_value::<serde_json::Map<String, Value>>(
            json!({"focus": "focus on the tests"})
        )
        .expect("object")
    );
    assert_eq!(
        compact_payload("--preview")["focus"],
        Value::from("--preview")
    );
    assert_eq!(compact_payload("here 3")["focus"], Value::from("here 3"));
}

#[test]
fn test_16_compaction_replies_parse_and_interpret() {
    for name in ["preview", "noop", "committed"] {
        let value = reply(name);
        let parsed: SessionMutateResult =
            serde_json::from_value(value.clone()).expect("reply fits the mutate result type");
        assert_eq!(parsed.operation, "compress");
    }

    let preview = MutationOutcome::OtherStatus {
        status: "preview".to_owned(),
        result: reply("preview"),
    };
    assert!(matches!(
        interpret(&preview, SID),
        CompactResult::Preview { lines } if lines.len() == 3
    ));

    let noop = MutationOutcome::OtherStatus {
        status: "noop".to_owned(),
        result: reply("noop"),
    };
    assert!(matches!(
        interpret(&noop, SID),
        CompactResult::NoChange { summary: Some(summary) }
            if summary.headline == "No changes from compression: 6 messages"
    ));

    let committed = MutationOutcome::Committed(reply("committed"));
    assert!(matches!(
        interpret(&committed, SID),
        CompactResult::Compacted {
            revision: 14,
            message_count: Some(24),
            ..
        }
    ));
}

#[test]
fn test_17_refusal_reasons_map_to_exact_texts() {
    let sid = "s-9";
    let cases: [(&str, Option<&str>, &str); 10] = [
        (
            "session_busy",
            Some("compression"),
            "Hermes is already compacting this conversation. Wait for it to finish, then run the same /compact line again.",
        ),
        (
            "session_busy",
            None,
            "Hermes cannot compact while the agent is working. Wait for the reply to finish, or press Esc to stop it, then run /compact again.",
        ),
        (
            "unknown_execution",
            None,
            "Hermes cannot compact while the outcome of the last turn is unknown. Nothing was changed.",
        ),
        (
            "compression_checkpoint_required",
            None,
            "This profile requires a memory checkpoint before compaction (compression.checkpoint_required), and none is available. Nothing was changed.",
        ),
        (
            "runtime_coordination_required",
            None,
            "Hermes cannot compact this session from this connection. Nothing was changed.",
        ),
        (
            "unsupported_compress_options",
            None,
            "Hermes does not support --aggressive. Nothing was changed.",
        ),
        (
            "revision_conflict",
            None,
            "The session changed while the compaction was being prepared. Run /compact again.",
        ),
        (
            "stale_generation",
            None,
            "The session changed while the compaction was being prepared. Run /compact again.",
        ),
        (
            "not_found",
            None,
            "Hermes does not have this session loaded. Resume it with talaria --resume s-9.",
        ),
        (
            "permission_denied",
            None,
            "Hermes refused the compaction: this connection may not change this session.",
        ),
    ];
    for (reason, field, expected) in cases {
        assert_eq!(
            refusal_text(reason, field, sid),
            expected,
            "reason {reason}"
        );
    }
    assert_eq!(
        refusal_text("bogus_reason", None, sid),
        "Hermes refused the compaction: bogus_reason."
    );

    let dropped = MutationOutcome::NotSent {
        reason: "not connected".to_owned(),
    };
    assert!(matches!(
        interpret(&dropped, SID),
        CompactResult::NotSent { text }
            if text == "Talaria could not send the compaction (not connected). Nothing was changed."
    ));
}

#[test]
fn test_18_unknown_outcomes_keep_the_request_for_resend() {
    let mut ledger = MutationLedger::new("launch-1");
    ledger.fences_mut(SID).adopt(Some(12), Some(5));
    let payload = compact_payload("here 2");
    let first = ledger
        .build(SID, "compress", payload.clone(), true)
        .expect("first build sends");
    ledger.settle(&first, &MutationOutcome::Unknown);
    let resend = ledger
        .build(SID, "compress", payload.clone(), true)
        .expect("resend reissues");
    assert_eq!(resend.request_id, first.request_id);

    let busy_compression = refused("session_busy", Some("compression"));
    ledger.settle(&first, &settle_as(&busy_compression));
    let kept = ledger
        .build(SID, "compress", payload.clone(), true)
        .expect("compression busy keeps");
    assert_eq!(kept.request_id, first.request_id);

    let busy_plain = refused("session_busy", None);
    ledger.settle(&first, &settle_as(&busy_plain));
    let fresh = ledger
        .build(SID, "compress", payload, true)
        .expect("plain busy rebuilds");
    assert_ne!(fresh.request_id, first.request_id);

    assert!(matches!(
        interpret(&MutationOutcome::Unknown, SID),
        CompactResult::Unknown
    ));
    let ends: [(MutationOutcome, CompactEnd); 6] = [
        (
            MutationOutcome::Committed(reply("committed")),
            CompactEnd::Committed { revision: 14 },
        ),
        (MutationOutcome::Unknown, CompactEnd::Unknown),
        (
            refused("session_busy", Some("compression")),
            CompactEnd::Unknown,
        ),
        (MutationOutcome::Unreadable, CompactEnd::Unknown),
        (refused("session_busy", None), CompactEnd::NothingChanged),
        (
            MutationOutcome::OtherStatus {
                status: "preview".to_owned(),
                result: reply("preview"),
            },
            CompactEnd::NothingChanged,
        ),
    ];
    for (outcome, expected) in &ends {
        assert_eq!(compact_end(outcome), *expected);
    }
}

#[test]
fn test_19_fence_refusal_retries_once_with_fresh_fences() {
    let mut ledger = MutationLedger::new("launch-1");
    ledger.fences_mut(SID).adopt(Some(12), Some(5));
    let first = ledger
        .build(SID, "compress", compact_payload(""), true)
        .expect("first build sends");
    let outcome = MutationOutcome::FenceRefused {
        reason: "stale_generation".to_owned(),
        revision: Some(13),
        execution_generation: Some(5),
    };
    let retry = ledger
        .retry_after_fence(&first, &outcome)
        .expect("one retry");
    assert_ne!(retry.request_id, first.request_id);
    assert_eq!(retry.expected_revision, 13);
    assert_eq!(retry.expected_generation, Some(5));
}
