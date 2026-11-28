//! The one-line reason shown when Hermes settles a turn as failed.
use serde_json::Value;
use talaria_core::text::{Replacement, SanitizeOptions, sanitize_with_options};

/// The longest message taken from Hermes's `error` field or its final text.
const MESSAGE_LIMIT: usize = 500;
/// The longest failure code taken from Hermes's `failure_reason` field.
const CODE_LIMIT: usize = 80;

/// One line of untrusted text: control characters removed, white space collapsed, length bounded.
fn one_line(text: &str, limit: usize) -> String {
    let clean = sanitize_with_options(text, SanitizeOptions::LAYOUT, Replacement::Drop);
    let line = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        return line;
    }
    let head: String = line.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", head.trim_end())
}

fn text(meta: &Value, field: &str, limit: usize) -> Option<String> {
    let line = one_line(meta[field].as_str()?, limit);
    (!line.is_empty()).then_some(line)
}

/// Why a failed turn failed, as one plain line.
///
/// Hermes may send a message (`error`) and a short code (`failure_reason`). Both are used when
/// both are present, then either alone, then the turn exit reason, then the first line of the
/// final text, then a fixed phrase.
pub(super) fn failure_reason(meta: &Value, final_text: Option<&str>) -> String {
    let error = text(meta, "error", MESSAGE_LIMIT);
    let code = text(meta, "failure_reason", CODE_LIMIT);
    let reason = match (error, code) {
        (Some(error), Some(code)) if error != code => format!("{error} ({code})"),
        (Some(only), _) | (None, Some(only)) => only,
        (None, None) => text(meta, "turn_exit_reason", MESSAGE_LIMIT)
            .or_else(|| {
                let first = final_text?.lines().find(|l| !l.trim().is_empty())?;
                Some(one_line(first, MESSAGE_LIMIT)).filter(|l| !l.is_empty())
            })
            .unwrap_or_else(|| "no reason given".into()),
    };
    format!("agent failed: {reason}")
}

/// Why a turn that Hermes ended because of a stop request ended, as one plain line.
///
/// It names who asked for the stop when Talaria knows (the kept `stopped_by` source), and keeps
/// Hermes's own exit reason and the first line of its final text. Hermes's text gives the time
/// the turn had been running when it stopped, which reads like a limit; the line says plainly
/// that it is not one.
pub(super) fn stop_reason(outcome: &str, meta: &Value, final_text: Option<&str>) -> String {
    use crate::journal::StopSource;
    let who = match StopSource::from_meta(meta) {
        Some(StopSource::RunStop) => "the runner stopped it because the run was being stopped",
        Some(StopSource::WorkflowView) => {
            "it was stopped with the stop-agent key in the Talaria workflow view"
        }
        Some(StopSource::WorkflowViewRestart) => {
            "the Talaria workflow view stopped it so that it can be restarted"
        }
        Some(StopSource::Budget) => "the runner stopped it because the run's budget ran out",
        None => {
            "Talaria's runner did not send this stop; a person or Hermes did, for example with a key press in a window showing the agent's session, or Hermes itself when it interrupts a turn"
        }
    };
    let mut line = format!("agent stopped: {who}; Hermes ended the turn as '{outcome}'");
    if let Some(exit) = text(meta, "turn_exit_reason", CODE_LIMIT) {
        line.push_str(&format!(" ({exit})"));
    }
    if let Some(first) = final_text
        .and_then(|t| t.lines().find(|l| !l.trim().is_empty()))
        .map(|l| one_line(l, MESSAGE_LIMIT))
        .filter(|l| !l.is_empty())
    {
        line.push_str(&format!(": {}", first.trim_end_matches('.')));
    }
    line.push_str(
        ". This was a stop, not a timeout: a time in Hermes's text is how long the turn had run",
    );
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{AdmissionState, Receipt, Record, SettledTurn, settled_record};
    use crate::{engine::AgentOutcome, hermes::SchemaTurnPolicy, journal::NextStep};
    use crate::{engine::AgentRequest, engine::CallId, journal::TurnPolicy};
    use serde_json::json;

    fn request() -> AgentRequest {
        AgentRequest {
            call_id: CallId {
                digest: "digest".into(),
                occurrence: 0,
            },
            label: "review".into(),
            phase: None,
            prompt: "Review it.".into(),
            schema: None,
            report_file: None,
            tier: None,
            model: None,
            provider: None,
            effort: None,
            profile: None,
            toolsets: None,
            skills: None,
            isolation: None,
            max_turns: None,
            workflow_path: vec![],
        }
    }

    /// Settle a receipt the way the journal does, then ask the policy what happened.
    fn reason_after(result: Value) -> String {
        reason_after_outcome("failed", result)
    }

    fn reason_after_outcome(outcome: &str, result: Value) -> String {
        match next_after(outcome, result, request()) {
            NextStep::Finish(AgentOutcome::Failed { reason }) => reason,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    /// Settle a receipt the way the journal does, then ask the policy what comes next.
    fn next_after(outcome: &str, result: Value, request: AgentRequest) -> NextStep {
        let receipt: Receipt = serde_json::from_value(json!({
            "admission_id": "adm-1",
            "session_id": "sess-1",
            "status": "started",
            "outcome": outcome,
            "authority_epoch": 1,
            "execution_generation": 1,
            "result": result,
            "usage": null,
        }))
        .unwrap();
        assert_eq!(receipt.status, AdmissionState::Started);
        let Record::TurnSettled {
            outcome,
            final_text,
            result_meta,
            ..
        } = settled_record("key", 0, 0, receipt)
        else {
            panic!("a receipt settles into a turn record");
        };
        let turn = SettledTurn {
            turn: 0,
            outcome,
            final_text,
            result_meta,
            usage: None,
        };
        SchemaTurnPolicy.after_turn(&request, &[turn])
    }

    #[test]
    fn a_context_overflow_receipt_keeps_its_message_and_code() {
        let message = "This conversation has grown too long for the model to read. Start a new session with /new.";
        let reason = reason_after(json!({
            "final_response": message,
            "error": message,
            "completed": false,
            "failed": true,
            "failure_reason": "context_overflow",
            "interrupted": false,
            "model": "model-a",
            "provider": "provider-a",
            "messages": [],
        }));
        assert_eq!(
            reason,
            format!("agent failed: {message} (context_overflow)")
        );
    }

    #[test]
    fn a_failed_result_on_a_completed_turn_reads_the_same() {
        // Hermes settles some failed turns as outcome "completed" with `failed: true`.
        assert_eq!(
            reason_after_outcome(
                "completed",
                json!({"completed": false, "failed": true, "error": "Over the limit.",
                    "failure_reason": "context_overflow"}),
            ),
            "agent failed: Over the limit. (context_overflow)"
        );
    }

    #[test]
    fn either_field_alone_is_used() {
        assert_eq!(
            reason_after(json!({"failed": true, "failure_reason": "rate_limit"})),
            "agent failed: rate_limit"
        );
        assert_eq!(
            reason_after(json!({"failed": true, "error": "The key was refused."})),
            "agent failed: The key was refused."
        );
        // A null error, as Hermes sends when nothing failed loudly, is not a reason.
        assert_eq!(
            reason_after(json!({"failed": true, "error": null, "failure_reason": null})),
            "agent failed: no reason given"
        );
    }

    #[test]
    fn the_exit_reason_then_the_first_line_of_the_final_text_come_next() {
        assert_eq!(
            reason_after(json!({"failed": true, "turn_exit_reason": "max_iterations"})),
            "agent failed: max_iterations"
        );
        assert_eq!(
            reason_after(json!({"failed": true, "final_response": "\nIt stopped early.\nMore."})),
            "agent failed: It stopped early."
        );
    }

    #[test]
    fn the_reason_is_one_bounded_line_without_control_characters() {
        let noisy = format!("first\x1b[31m line\nsecond\tline {}", "x".repeat(2000));
        let reason = reason_after(json!({"failed": true, "error": noisy}));
        assert!(reason.starts_with("agent failed: first[31m line second line xxx"));
        assert!(!reason.chars().any(char::is_control), "{reason:?}");
        assert!(reason.chars().count() <= "agent failed: ".len() + MESSAGE_LIMIT);
        assert!(reason.ends_with('…'));
    }

    /// What Hermes sends for a turn a stop request ended while it waited for the model.
    fn stopped_receipt() -> Value {
        json!({
            "final_response": "Operation interrupted: waiting for model response (80.0s elapsed).",
            "completed": false,
            "failed": false,
            "interrupted": true,
            "turn_exit_reason": "interrupted_during_api_call",
            "model": "model-a",
            "provider": "provider-a",
        })
    }

    #[test]
    fn a_stopped_turn_says_a_stop_was_requested_and_that_it_was_not_a_timeout() {
        let reason = reason_after_outcome("interrupted", stopped_receipt());
        assert_eq!(
            reason,
            "agent stopped: Talaria's runner did not send this stop; a person or Hermes did, for example with a key press in a window showing the agent's session, or Hermes itself when it interrupts a turn; Hermes ended the turn as 'interrupted' (interrupted_during_api_call): Operation interrupted: waiting for model response (80.0s elapsed). This was a stop, not a timeout: a time in Hermes's text is how long the turn had run"
        );
        // A cancelled queued turn has no result at all and still says what happened.
        assert_eq!(
            reason_after_outcome("cancelled", Value::Null),
            "agent stopped: Talaria's runner did not send this stop; a person or Hermes did, for example with a key press in a window showing the agent's session, or Hermes itself when it interrupts a turn; Hermes ended the turn as 'cancelled'. This was a stop, not a timeout: a time in Hermes's text is how long the turn had run"
        );
    }

    #[test]
    fn a_stop_talaria_sent_names_its_source() {
        for (source, phrase) in [
            (
                "run_stop",
                "agent stopped: the runner stopped it because the run was being stopped;",
            ),
            (
                "workflow_view",
                "agent stopped: it was stopped with the stop-agent key in the Talaria workflow view;",
            ),
            (
                "workflow_view_restart",
                "agent stopped: the Talaria workflow view stopped it so that it can be restarted;",
            ),
            (
                "budget",
                "agent stopped: the runner stopped it because the run's budget ran out;",
            ),
        ] {
            let meta =
                json!({"stopped_by": source, "turn_exit_reason": "interrupted_during_api_call"});
            let reason = stop_reason("interrupted", &meta, None);
            assert!(reason.starts_with(phrase), "{reason}");
            assert!(reason.contains("not a timeout"), "{reason}");
        }
    }

    /// What Hermes sends when it stops a turn at its step cap but still has a reply.
    fn capped_receipt(reply: &str) -> Value {
        json!({
            "final_response": reply,
            "completed": false,
            "failed": false,
            "interrupted": false,
            "turn_exit_reason": "max_iterations_reached(60/60)",
            "model": "model-a",
            "provider": "provider-a",
            "messages": [],
        })
    }

    #[test]
    fn a_turn_stopped_at_its_step_cap_keeps_its_reply_and_is_marked() {
        let next = next_after("completed", capped_receipt("Half the review."), request());
        let NextStep::Finish(AgentOutcome::CutShort { value, cut_short }) = next else {
            panic!("expected a kept, marked reply, got {next:?}");
        };
        assert_eq!(value, json!("Half the review."));
        assert_eq!(cut_short.describe(), "cut short (60 of 60 steps)");
    }

    #[test]
    fn a_cut_short_reply_for_a_schema_is_checked_and_never_sent_a_correction_turn() {
        let mut with_schema = request();
        with_schema.schema = Some(json!({
            "type": "object",
            "properties": {"verdict": {"type": "string"}},
            "required": ["verdict"],
        }));
        let fits = next_after(
            "completed",
            capped_receipt("{\"verdict\": \"ship\"}"),
            with_schema.clone(),
        );
        assert!(
            matches!(&fits, NextStep::Finish(AgentOutcome::CutShort { value, .. })
                if value == &json!({"verdict": "ship"})),
            "{fits:?}"
        );
        // A reply that does not fit would get one correction turn if the agent had finished.
        // A cut short agent is not sent another turn, so it ends as failed instead.
        let broken = next_after("completed", capped_receipt("not json"), with_schema);
        let NextStep::Finish(AgentOutcome::Failed { reason }) = broken else {
            panic!("expected a failure, got {broken:?}");
        };
        assert!(
            reason.starts_with("agent cut short (60 of 60 steps) and its reply does not fit"),
            "{reason}"
        );
    }

    #[test]
    fn only_a_step_cap_exit_marks_a_turn_cut_short() {
        for result in [
            // Finished normally.
            json!({"final_response": "Done.", "completed": true, "failed": false,
                "turn_exit_reason": "completed"}),
            // Incomplete for another reason.
            json!({"final_response": "Stopped.", "completed": false, "failed": false,
                "turn_exit_reason": "interrupted_during_api_call"}),
        ] {
            let next = next_after("completed", result, request());
            assert!(
                matches!(&next, NextStep::Finish(AgentOutcome::Completed(_))),
                "{next:?}"
            );
        }
        // A failed turn is a failure even when its exit reason names the cap.
        let mut failed = capped_receipt("Half.");
        failed["failed"] = json!(true);
        let next = next_after("completed", failed, request());
        assert!(
            matches!(next, NextStep::Finish(AgentOutcome::Failed { .. })),
            "{next:?}"
        );
    }
}
