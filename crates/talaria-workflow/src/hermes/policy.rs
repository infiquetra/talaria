//! A stored result yields text, a validated value, or one correction turn.
use super::{
    brief::{brief, correction_message},
    validate::{check_schema, validate_output},
};
use crate::engine::{AgentOutcome, AgentRequest, CutShort, Isolation};
use crate::journal::{NextStep, ResolvedLaunch, SettledTurn, TurnPolicy};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaTurnPolicy;
fn failed(reason: impl Into<String>) -> NextStep {
    NextStep::Finish(AgentOutcome::Failed {
        reason: reason.into(),
    })
}
/// A kept reply, marked when Hermes stopped the turn at its step cap.
fn kept(value: Value, cut_short: Option<CutShort>) -> NextStep {
    NextStep::Finish(match cut_short {
        Some(cut_short) => AgentOutcome::CutShort { value, cut_short },
        None => AgentOutcome::Completed(value),
    })
}
/// Hermes ends a turn at its step cap with `completed: false` and a `max_iterations_reached(N/M)`
/// exit reason, yet leaves the reply it had.
fn stopped_at_cap(meta: &Value) -> Option<CutShort> {
    if meta["completed"] != false {
        return None;
    }
    CutShort::from_exit_reason(meta["turn_exit_reason"].as_str()?)
}
impl TurnPolicy for SchemaTurnPolicy {
    fn first_turn(&self, request: &AgentRequest, launch: &ResolvedLaunch) -> String {
        let mut text = brief(&request.prompt, request.schema.as_ref());
        if let Some(path) = &request.report_file {
            text.push_str(&format!(
                "\n\nWrite the report as plain JSON matching the schema to this relative path in your worktree before returning your final reply: {path:?}. Include short evidence references, such as file paths and line numbers, in the fields available in the schema."
            ));
        }
        match (&launch.isolation, &launch.cwd) {
            (Some(Isolation::Worktree), Some(cwd)) => format!(
                "Your working folder is {cwd}. Do all your work there, using relative paths, and do not change to the source checkout.\n\n{text}"
            ),
            _ => text,
        }
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
            return failed(super::failure::failure_reason(
                &turn.result_meta,
                turn.final_text.as_deref(),
            ));
        }
        match turn.outcome.as_str() {
            "completed" => {}
            "interrupted" | "cancelled" => {
                return failed(super::failure::stop_reason(
                    &turn.outcome,
                    &turn.result_meta,
                    turn.final_text.as_deref(),
                ));
            }
            other => return failed(format!("unexpected outcome '{other}'")),
        }
        let text = turn.final_text.as_deref().unwrap_or("");
        let cut_short = stopped_at_cap(&turn.result_meta);
        let Some(schema) = &request.schema else {
            return if text.trim().is_empty() {
                failed("agent returned an empty reply")
            } else {
                kept(Value::String(text.into()), cut_short)
            };
        };
        match validate_output(text, schema) {
            Ok(value) => kept(value, cut_short),
            // A cut short agent is not sent another turn, so it cannot be asked to correct itself.
            Err(errors) if cut_short.is_some() => failed(format!(
                "agent {} and its reply does not fit the schema: {}",
                cut_short.map_or_else(String::new, |cut| cut.describe()),
                errors
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
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
