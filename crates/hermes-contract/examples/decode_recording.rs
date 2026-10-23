//! Show the distinct recorded events and the prompt submission guard.

use hermes_contract::{
    events::{EventFrame, Notification},
    methods::canonical::prompt_submit::Params,
    overrides::{check_prompt_submit, prompt_submit_params},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let mut seen = BTreeSet::new();
    for line in include_str!("../tests/fixtures/live_two_turns.frames.jsonl").lines() {
        let message: Value = serde_json::from_str(line)?;
        if message.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        let params = &message["params"];
        let name = params
            .get("type")
            .and_then(Value::as_str)
            .ok_or("recorded event has no type")?;
        if !seen.insert(name.to_owned()) {
            continue;
        }
        match EventFrame::from_params(params) {
            Ok(frame) => match &frame.notification {
                Notification::Unknown(_) => println!("{name}: unknown"),
                Notification::ApprovalRequest(approval) => println!(
                    "{name}: ok ApprovalRequest choices={}",
                    approval.choices.as_deref().unwrap_or_default().join(", ")
                ),
                notification => {
                    let value = serde_json::to_value(notification)?;
                    let variant = match &value {
                        Value::Object(object) => object.keys().next().map(String::as_str),
                        Value::String(name) => Some(name.as_str()),
                        _ => None,
                    }
                    .ok_or("notification has no serialised variant name")?;
                    println!("{name}: ok {variant}");
                }
            },
            Err(error) => println!("{name}: error missing {}", error.missing_fields.join(", ")),
        }
    }

    let without_id: Params = serde_json::from_value(json!({
        "session_id": "fixture",
        "text": "hello"
    }))?;
    println!(
        "prompt.submit without submission_id: {}",
        if check_prompt_submit(&without_id).is_err() {
            "rejected"
        } else {
            "accepted"
        }
    );
    let params = prompt_submit_params("fixture", "hello", "fixture-submission");
    println!(
        "prompt.submit from prompt_submit_params: {}",
        if check_prompt_submit(&params).is_ok() {
            "accepted"
        } else {
            "rejected"
        }
    );
    Ok(())
}
