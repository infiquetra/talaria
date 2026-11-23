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
        let receipt: Receipt = serde_json::from_value(json!({
            "admission_id": "adm-1",
            "session_id": "sess-1",
            "status": "started",
            "outcome": "failed",
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
        match SchemaTurnPolicy.after_turn(&request(), &[turn]) {
            NextStep::Finish(AgentOutcome::Failed { reason }) => reason,
            other => panic!("expected a failure, got {other:?}"),
        }
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
}
