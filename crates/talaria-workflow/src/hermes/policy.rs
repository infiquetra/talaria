//! A stored result yields text, a validated value, or one correction turn.
use super::{
    brief::{brief, correction_message},
    validate::{check_schema, validate_output},
};
use crate::engine::{AgentOutcome, AgentRequest};
use crate::journal::{NextStep, ResolvedLaunch, SettledTurn, TurnPolicy};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaTurnPolicy;
fn failed(reason: impl Into<String>) -> NextStep {
    NextStep::Finish(AgentOutcome::Failed {
        reason: reason.into(),
    })
}
impl TurnPolicy for SchemaTurnPolicy {
    fn first_turn(&self, request: &AgentRequest, _launch: &ResolvedLaunch) -> String {
        brief(&request.prompt, request.schema.as_ref())
    }
    fn after_turn(&self, request: &AgentRequest, turns: &[SettledTurn]) -> NextStep {
        let Some(turn) = turns.last() else {
            return failed("terminal without a stored result");
        };
        if turn.outcome == "completed" {
            if let Some(schema) = &request.schema
                && let Err(reason) = check_schema(schema)
            {
                return failed(reason);
            }
            if turn.final_text.is_none()
                && turn.result_meta.as_object().is_none_or(|m| m.is_empty())
            {
                return failed("terminal without a stored result");
            }
        }
        if turn.outcome == "failed"
            || turn.outcome == "completed" && turn.result_meta["failed"] == true
        {
            let reason = turn.result_meta["error"]
                .as_str()
                .or_else(|| turn.result_meta["turn_exit_reason"].as_str())
                .unwrap_or("no reason given");
            return failed(format!("agent failed: {reason}"));
        }
        match turn.outcome.as_str() {
            "completed" => {}
            "interrupted" | "cancelled" => {
                return failed(format!("stopped in Hermes (outcome '{}')", turn.outcome));
            }
            other => return failed(format!("unexpected outcome '{other}'")),
        }
        let text = turn.final_text.as_deref().unwrap_or("");
        let Some(schema) = &request.schema else {
            return if text.trim().is_empty() {
                failed("agent returned an empty reply")
            } else {
                NextStep::Finish(AgentOutcome::Completed(Value::String(text.into())))
            };
        };
        match validate_output(text, schema) {
            Ok(value) => NextStep::Finish(AgentOutcome::Completed(value)),
            Err(errors) if turn.turn == 0 => NextStep::Turn {
                text: correction_message(&errors),
            },
            Err(errors) => failed(format!(
                "schema validation failed after one correction: {}",
                errors
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }
}
