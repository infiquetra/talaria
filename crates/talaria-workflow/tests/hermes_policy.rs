#[path = "hermes_common/mod.rs"]
mod common;

use serde_json::{Value, json};
use talaria_workflow::{
    engine::AgentOutcome,
    hermes::{
        AgentUsage, NextStep, SchemaTurnPolicy, TurnPolicy, brief, call_usage, correction_message,
        validate_output,
    },
};

fn reason(step: NextStep) -> String {
    match step {
        NextStep::Finish(AgentOutcome::Failed { reason }) => reason,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn first_turn_is_brief() {
    let policy = SchemaTurnPolicy;
    let launch = common::launch();
    let plain = common::request("Report one finding.");
    let text = policy.first_turn(&plain, &launch);
    assert_eq!(text, brief(&plain.prompt, None));
    let mut with_schema = plain.clone();
    let schema = common::read_json("schemas/finding.json");
    with_schema.schema = Some(schema.clone());
    let briefed = policy.first_turn(&with_schema, &launch);
    assert_eq!(briefed, brief(&with_schema.prompt, Some(&schema)));
}

#[test]
fn text_call_completes() {
    let policy = SchemaTurnPolicy;
    let request = common::request("Say ready");
    let step = policy.after_turn(
        &request,
        &[common::settled(0, "completed", Some("ready"), json!({}))],
    );
    assert_eq!(
        step,
        NextStep::Finish(AgentOutcome::Completed(Value::String("ready".into())))
    );
}

#[test]
fn schema_invalid_then_correction_turn() {
    let policy = SchemaTurnPolicy;
    let mut request = common::request("Report one finding.");
    let schema = common::read_json("schemas/finding.json");
    request.schema = Some(schema.clone());
    let turns = [common::settled(0, "completed", Some("{}"), json!({}))];
    let step = policy.after_turn(&request, &turns);
    let again = policy.after_turn(&request, &turns);
    assert_eq!(step, again);
    let errors = validate_output("{}", &schema).unwrap_err();
    match step {
        NextStep::Turn { text } => assert_eq!(text, correction_message(&errors)),
        other => panic!("expected a correction turn, got {other:?}"),
    }
}

#[test]
fn schema_fails_after_one_correction() {
    let policy = SchemaTurnPolicy;
    let mut request = common::request("Report one finding.");
    request.schema = Some(common::read_json("schemas/finding.json"));
    let turns = [
        common::settled(0, "completed", Some("{}"), json!({})),
        common::settled(1, "completed", Some("{}"), json!({})),
    ];
    let step = policy.after_turn(&request, &turns);
    match &step {
        NextStep::Turn { .. } => panic!("never a third turn"),
        NextStep::Finish(AgentOutcome::Failed { reason }) => {
            assert!(
                reason.starts_with("schema validation failed after one correction: "),
                "{reason}"
            );
            assert!(reason.contains(';') || reason.contains('$'), "{reason}");
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn outcomes_map_as_specified() {
    let policy = SchemaTurnPolicy;
    let request = common::request("Say ready");
    let missing = policy.after_turn(
        &request,
        &[common::settled(0, "completed", None, json!({}))],
    );
    assert_eq!(reason(missing), "terminal without a stored result");
    let failed = policy.after_turn(
        &request,
        &[common::settled(
            0,
            "failed",
            None,
            json!({"turn_exit_reason": "boom"}),
        )],
    );
    assert_eq!(reason(failed), "agent failed: boom");
    let unnamed = policy.after_turn(&request, &[common::settled(0, "failed", None, json!({}))]);
    assert_eq!(reason(unnamed), "agent failed: no reason given");
    let preferred = policy.after_turn(
        &request,
        &[common::settled(
            0,
            "failed",
            None,
            json!({"error": "nope", "turn_exit_reason": "boom"}),
        )],
    );
    assert_eq!(reason(preferred), "agent failed: nope");
    let flagged = policy.after_turn(
        &request,
        &[common::settled(
            0,
            "completed",
            Some("ready"),
            json!({"failed": true, "turn_exit_reason": "boom"}),
        )],
    );
    assert_eq!(reason(flagged), "agent failed: boom");
    let interrupted = policy.after_turn(
        &request,
        &[common::settled(
            0,
            "interrupted",
            Some("partial"),
            json!({}),
        )],
    );
    assert_eq!(
        reason(interrupted),
        "stopped in Hermes (outcome 'interrupted')"
    );
    let cancelled = policy.after_turn(
        &request,
        &[common::settled(0, "cancelled", None, json!({}))],
    );
    assert_eq!(reason(cancelled), "stopped in Hermes (outcome 'cancelled')");
    let other = policy.after_turn(
        &request,
        &[common::settled(0, "steered", Some("nope"), json!({}))],
    );
    assert_eq!(reason(other), "unexpected outcome 'steered'");
    let blank = policy.after_turn(
        &request,
        &[common::settled(0, "completed", Some("  "), json!({}))],
    );
    assert_eq!(reason(blank), "agent returned an empty reply");
    let mut bad = request.clone();
    bad.schema = Some(json!({"type": "no-such-type"}));
    let schema = policy.after_turn(
        &bad,
        &[common::settled(0, "completed", Some("{}"), json!({}))],
    );
    let schema_reason = reason(schema);
    assert!(
        schema_reason.starts_with("schema does not compile: "),
        "{schema_reason}"
    );
}

#[test]
fn call_usage_sums_tokens_keeps_last_cost() {
    let usage = call_usage(&[
        json!({
            "input_tokens": 3,
            "output_tokens": 4,
            "total_tokens": 7,
            "session_cost": {"estimated_cost_usd": 1.5, "cost_status": "known"}
        }),
        json!({
            "input_tokens": 1,
            "output_tokens": 2,
            "total_tokens": 3,
            "session_cost": {"estimated_cost_usd": 2.25, "cost_status": "final"}
        }),
    ]);
    assert_eq!(
        usage,
        AgentUsage {
            input_tokens: Some(4),
            output_tokens: Some(6),
            total_tokens: Some(10),
            cost_usd: Some(2.25),
            cost_status: Some("final".into()),
        }
    );
    let partial = call_usage(&[json!({"input_tokens": 5}), json!({"output_tokens": 1})]);
    assert_eq!(partial.input_tokens, Some(5));
    assert_eq!(partial.output_tokens, Some(1));
    assert_eq!(partial.total_tokens, None);
    assert_eq!(partial.cost_usd, None);
    let absent = call_usage(&[json!({}), json!({})]);
    assert_eq!(absent.input_tokens, None);
    assert_eq!(absent.output_tokens, None);
    assert_eq!(absent.total_tokens, None);
    assert_eq!(absent.cost_usd, None);
    assert_eq!(absent.cost_status, None);
}
