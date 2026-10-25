//! One headless run as a pure state machine.

use serde_json::Value;

use crate::failure::LaunchFailure;
use crate::headless::output::HeadlessResult;

/// What asked the run to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopCause {
    Timeout,
    Signal(i32),
}

/// How the submit call answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitReply {
    Admitted {
        admission_id: String,
        status: String,
        execution_generation: Option<i64>,
    },
    VoiceStopped,
    Refused {
        reason: Option<String>,
    },
    Unknown,
}

/// How a finished call is handed to the machine.
#[derive(Debug, Clone, PartialEq)]
pub enum CallReply {
    Ok(Value),
    Refused { reason: Option<String> },
    Failed,
}

/// Every event the machine can take.
#[derive(Debug, Clone, PartialEq)]
pub enum HeadlessInput {
    Opened {
        session_id: String,
        stored_session_id: String,
        profile: String,
        model: Option<String>,
        provider: Option<String>,
        open_prompts: Vec<OpenPrompt>,
        unknown_admissions: usize,
    },
    OpenFailed(LaunchFailure),
    SubmitReplied(SubmitReply),
    Event {
        admission_id: Option<String>,
        execution_generation: Option<i64>,
        event_type: String,
        payload: Value,
    },
    ServerRequest {
        request_key: u64,
        kind: String,
    },
    ReceiptRead(CallReply),
    CancelReplied(CallReply),
    InterruptReplied(CallReply),
    Gap,
    Recovered {
        own_pending_status: Option<String>,
        own_pending_generation: Option<i64>,
        open_prompts: Vec<OpenPrompt>,
    },
    LinkFailed {
        reason: String,
        next_step: String,
    },
    SessionGone,
    Stop(StopCause),
    SettleDeadline,
    StopBudgetExpired,
}

/// One open approval or question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPrompt {
    pub prompt_id: String,
    pub kind: String,
    pub admission_id: Option<String>,
    pub execution_generation: Option<i64>,
}

/// What the service must do next.
#[derive(Debug, Clone, PartialEq)]
pub enum HeadlessEffect {
    Submit {
        text: String,
    },
    ReadReceipt,
    Cancel,
    Interrupt {
        execution_generation: i64,
    },
    DenyApproval {
        execution_generation: i64,
        prompt_id: String,
    },
    SkipClarify {
        execution_generation: i64,
        prompt_id: String,
    },
    RefuseServerRequest {
        request_key: u64,
    },
    Recover,
    StartSettleTimer,
    StartStopBudget,
    Detach,
    Finish(HeadlessResult),
    ExitNow(HeadlessResult),
}

/// The machine. Owns the phase and everything learned so far.
#[derive(Debug)]
#[allow(dead_code)]
pub struct HeadlessRun {
    text: String,
}

impl HeadlessRun {
    pub fn new(text: String) -> Self {
        let _ = text;
        todo!("#235")
    }

    pub fn step(&mut self, input: HeadlessInput) -> Vec<HeadlessEffect> {
        let _ = input;
        todo!("#235")
    }
}

/// Most events held while the submit reply is missing.
pub const MAX_HELD_EVENTS: usize = 4096;

#[cfg(test)]
mod tests {
    use super::{
        CallReply, HeadlessEffect, HeadlessInput, HeadlessRun, OpenPrompt, StopCause, SubmitReply,
    };
    use crate::headless::output::HeadlessResult;
    use serde_json::json;

    fn opened() -> HeadlessInput {
        HeadlessInput::Opened {
            session_id: "local-session-1".to_string(),
            stored_session_id: "local-session-1".to_string(),
            profile: "fixture".to_string(),
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            open_prompts: Vec::new(),
            unknown_admissions: 0,
        }
    }

    fn admitted(status: &str) -> HeadlessInput {
        HeadlessInput::SubmitReplied(SubmitReply::Admitted {
            admission_id: "adm-1".to_string(),
            status: status.to_string(),
            execution_generation: None,
        })
    }

    fn event(admission: Option<&str>, kind: &str, payload: serde_json::Value) -> HeadlessInput {
        HeadlessInput::Event {
            admission_id: admission.map(str::to_string),
            execution_generation: Some(7),
            event_type: kind.to_string(),
            payload,
        }
    }

    fn start(admission: &str) -> HeadlessInput {
        event(Some(admission), "message.start", json!({}))
    }

    fn completion(admission: &str) -> HeadlessInput {
        event(
            Some(admission),
            "message.complete",
            json!({
                "text": "5555",
                "content": "5555",
                "admission_id": admission,
                "outcome": "completed",
                "status": "complete",
            }),
        )
    }

    fn finish_of(effects: &[HeadlessEffect]) -> Option<&HeadlessResult> {
        effects.iter().find_map(|effect| match effect {
            HeadlessEffect::Finish(done) | HeadlessEffect::ExitNow(done) => Some(done),
            _ => None,
        })
    }

    #[test]
    fn happy_path_effects_in_order() {
        let mut run = HeadlessRun::new("hello".to_string());
        assert_eq!(
            run.step(opened()),
            vec![HeadlessEffect::Submit {
                text: "hello".to_string()
            }]
        );
        assert!(run.step(admitted("queued")).is_empty());
        assert!(run.step(start("adm-1")).is_empty());
        assert_eq!(
            run.step(completion("adm-1")),
            vec![HeadlessEffect::ReadReceipt]
        );
        let effects = run.step(HeadlessInput::ReceiptRead(CallReply::Ok(json!({
            "admission_id": "adm-1",
            "status": "terminal",
            "outcome": "completed",
            "result": { "completed": true, "final_response": "5555" },
        }))));
        assert!(effects.contains(&HeadlessEffect::Detach));
        let done = finish_of(&effects).expect("a finish");
        assert_eq!(done.exit_code, 0);
        assert_eq!(done.text, "5555");
    }

    #[test]
    fn events_before_submit_reply_are_held() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        // Another turn's events before the reply are held, never acted on.
        assert!(run.step(start("adm-9")).is_empty());
        assert!(run.step(completion("adm-9")).is_empty());
        // A foreign approval before the reply is not held: once the reply
        // lands, our queued prompt is withdrawn for it.
        let effects = run.step(event(
            Some("adm-9"),
            "approval.request",
            json!({ "prompt_id": "p-1", "execution_generation": 3 }),
        ));
        assert!(effects.is_empty());
        let effects = run.step(admitted("queued"));
        assert!(effects.contains(&HeadlessEffect::Cancel));
    }

    #[test]
    fn other_admissions_never_finish_the_run() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        assert!(run.step(completion("adm-9")).is_empty());
        assert!(run.step(start("adm-9")).is_empty());
    }

    #[test]
    fn submit_reply_kinds() {
        for (reply, code) in [
            (
                SubmitReply::Refused {
                    reason: Some("busy".to_string()),
                },
                4,
            ),
            (SubmitReply::Refused { reason: None }, 4),
            (SubmitReply::VoiceStopped, 4),
            (SubmitReply::Unknown, 1),
        ] {
            let mut run = HeadlessRun::new("hello".to_string());
            let _ = run.step(opened());
            let effects = run.step(HeadlessInput::SubmitReplied(reply));
            let done = finish_of(&effects).expect("a finish");
            assert_eq!(done.exit_code, code);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
            );
        }
        // An unknown status also ends unknown with no second submit.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let effects = run.step(admitted("unknown"));
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn blocked_session_refused_before_submit() {
        let with_prompt = HeadlessInput::Opened {
            session_id: "s".to_string(),
            stored_session_id: "s".to_string(),
            profile: "fixture".to_string(),
            model: None,
            provider: None,
            open_prompts: vec![OpenPrompt {
                prompt_id: "p-1".to_string(),
                kind: "approval".to_string(),
                admission_id: None,
                execution_generation: Some(3),
            }],
            unknown_admissions: 0,
        };
        let with_unknown = HeadlessInput::Opened {
            session_id: "s".to_string(),
            stored_session_id: "s".to_string(),
            profile: "fixture".to_string(),
            model: None,
            provider: None,
            open_prompts: Vec::new(),
            unknown_admissions: 1,
        };
        for input in [with_prompt, with_unknown] {
            let mut run = HeadlessRun::new("hello".to_string());
            let effects = run.step(input);
            assert!(effects.contains(&HeadlessEffect::Detach));
            assert_eq!(finish_of(&effects).expect("a finish").exit_code, 3);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
            );
        }
    }

    #[test]
    fn foreign_prompt_while_queued_cancels_ours() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(event(
            Some("adm-9"),
            "approval.request",
            json!({ "prompt_id": "p-9", "execution_generation": 3 }),
        ));
        assert_eq!(effects, vec![HeadlessEffect::Cancel]);
        // The cancel's own settlement arrives before the reply.
        let effects = run.step(event(
            Some("adm-1"),
            "message.complete",
            json!({
                "text": "",
                "content": "",
                "admission_id": "adm-1",
                "outcome": "cancelled",
            }),
        ));
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 3);
    }

    #[test]
    fn prompt_for_own_turn_is_declined() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(HeadlessInput::SubmitReplied(SubmitReply::Admitted {
            admission_id: "adm-1".to_string(),
            status: "started".to_string(),
            execution_generation: Some(7),
        }));
        let effects = run.step(event(
            Some("adm-1"),
            "approval.request",
            json!({ "prompt_id": "p-1", "execution_generation": 7 }),
        ));
        assert!(effects.contains(&HeadlessEffect::DenyApproval {
            execution_generation: 7,
            prompt_id: "p-1".to_string(),
        }));
        let effects = run.step(event(
            Some("adm-1"),
            "clarify.request",
            json!({ "prompt_id": "c-1", "execution_generation": 7 }),
        ));
        assert!(effects.contains(&HeadlessEffect::SkipClarify {
            execution_generation: 7,
            prompt_id: "c-1".to_string(),
        }));
    }

    #[test]
    fn server_requests_by_turn_state() {
        // Queued: another viewer's turn owns the request; leave it alone.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::ServerRequest {
            request_key: 1,
            kind: "sudo".to_string(),
        });
        assert!(!effects.contains(&HeadlessEffect::RefuseServerRequest { request_key: 1 }));
        assert!(!effects.contains(&HeadlessEffect::Cancel));
        // Started: our turn owns it; refuse at once.
        let _ = run.step(start("adm-1"));
        let effects = run.step(HeadlessInput::ServerRequest {
            request_key: 2,
            kind: "sudo".to_string(),
        });
        assert!(effects.contains(&HeadlessEffect::RefuseServerRequest { request_key: 2 }));
    }

    #[test]
    fn stop_interrupts_started_cancels_queued() {
        // Started with a known generation: interrupt it.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(start("adm-1"));
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(15)));
        assert!(effects.contains(&HeadlessEffect::StartStopBudget));
        assert!(effects.contains(&HeadlessEffect::Interrupt {
            execution_generation: 7
        }));
        // Queued: cancel it instead.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::Stop(StopCause::Timeout));
        assert!(effects.contains(&HeadlessEffect::Cancel));
    }

    #[test]
    fn settle_wait_and_stop_budget() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(start("adm-1"));
        let _ = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        // No settlement before the deadline: the receipt is read once.
        let effects = run.step(HeadlessInput::SettleDeadline);
        assert!(effects.contains(&HeadlessEffect::ReadReceipt));
        // An expired budget finishes with the stop code and no detach.
        let effects = run.step(HeadlessInput::StopBudgetExpired);
        let done = finish_of(&effects).expect("a finish");
        assert_eq!(done.exit_code, 130);
        assert!(!effects.contains(&HeadlessEffect::Detach));
    }

    #[test]
    fn second_stop_exits_now() {
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let _ = run.step(HeadlessInput::Stop(StopCause::Timeout));
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        let exited = effects
            .iter()
            .find_map(|effect| match effect {
                HeadlessEffect::ExitNow(done) => Some(done),
                _ => None,
            })
            .expect("an immediate exit");
        assert_eq!(exited.exit_code, 130);
        assert!(!effects.contains(&HeadlessEffect::Detach));
    }

    #[test]
    fn gap_rules() {
        // A queued row keeps waiting for the completion.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        assert_eq!(run.step(HeadlessInput::Gap), vec![HeadlessEffect::Recover]);
        let effects = run.step(HeadlessInput::Recovered {
            own_pending_status: Some("queued".to_string()),
            own_pending_generation: None,
            open_prompts: Vec::new(),
        });
        assert!(finish_of(&effects).is_none());
        // An unknown row ends unknown.
        let effects = run.step(HeadlessInput::Recovered {
            own_pending_status: Some("unknown".to_string()),
            own_pending_generation: None,
            open_prompts: Vec::new(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn link_failure_by_phase() {
        // Before the session opened: nothing ran.
        let mut run = HeadlessRun::new("hello".to_string());
        let effects = run.step(HeadlessInput::LinkFailed {
            reason: "gone".to_string(),
            next_step: "retry".to_string(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 4);
        // After the submit: the prompt may be running.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::LinkFailed {
            reason: "gone".to_string(),
            next_step: "retry".to_string(),
        });
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
        // A gone session after the submit is unknown too.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let _ = run.step(admitted("queued"));
        let effects = run.step(HeadlessInput::SessionGone);
        assert_eq!(finish_of(&effects).expect("a finish").exit_code, 1);
    }

    #[test]
    fn stop_before_and_during_submit() {
        // Before the session opened: no submit follows the stop.
        let mut run = HeadlessRun::new("hello".to_string());
        let effects = run.step(HeadlessInput::Stop(StopCause::Signal(2)));
        assert!(effects.contains(&HeadlessEffect::StartStopBudget));
        let effects = run.step(opened());
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, HeadlessEffect::Submit { .. }))
        );
        assert!(effects.contains(&HeadlessEffect::Detach));
        // During the submit: the reply is waited for, nothing is cancelled.
        let mut run = HeadlessRun::new("hello".to_string());
        let _ = run.step(opened());
        let effects = run.step(HeadlessInput::Stop(StopCause::Timeout));
        assert!(!effects.contains(&HeadlessEffect::Cancel));
        assert!(!effects.contains(&HeadlessEffect::Interrupt {
            execution_generation: 7
        }));
    }
}
