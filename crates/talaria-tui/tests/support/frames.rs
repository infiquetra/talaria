//! Version 3 frame-log reader, copied from `crates/talaria-core/tests/support/frames.rs`.
//!
//! A frame `EventFrame::from_params` refuses is skipped and counted. It is not turned into an
//! `Event`. A `prompt.submit` with `unattended: true` is a read error.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use std::path::Path;
use talaria_core::session::{Event, PromptKey, SessionId};

/// One outbound frame kept so a test can compare an effect with the recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Outbound {
    /// JSON-RPC method.
    pub method: String,
    /// `params` object.
    pub params: Value,
}

/// A version 3 log reduced to reducer inputs and any snapshots it carried.
#[derive(Clone, Debug)]
pub struct FrameLog {
    /// Reducer events in file order.
    pub events: Vec<Event>,
    /// Snapshot from a `session.create` or `session.resume` result that arrived before any
    /// reducer event. The example builds state from this.
    pub initial_snapshot: Option<CanonicalSnapshot>,
    /// Every snapshot result in the file, including [`Self::initial_snapshot`].
    pub snapshots: Vec<CanonicalSnapshot>,
    /// Frames whose params `EventFrame::from_params` refused, plus snapshot results that
    /// did not decode.
    pub undecodable: usize,
    /// `dropped` records, null frames, and records this reader does not apply.
    pub skipped: usize,
    /// `type` names of undecodable event frames, in file order.
    pub undecodable_types: Vec<String>,
    /// `pending.len()` of the first snapshot in the file. Those rows are not cells.
    pub queued_admissions_ignored: usize,
    /// Outbound `prompt.submit`, `session.interrupt`, `approval.respond` and `clarify.respond`.
    pub outbound: Vec<Outbound>,
}

impl FrameLog {
    /// The snapshot a resume test should build from: the initial one, else the first parsed.
    pub fn snapshot(&self) -> Option<&CanonicalSnapshot> {
        self.initial_snapshot
            .as_ref()
            .or_else(|| self.snapshots.first())
    }
}

/// Read a version 3 frame log. The error is one plain sentence and contains no path.
pub fn read_path(path: &Path) -> Result<FrameLog, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| "The frame log could not be read.".to_string())?;
    read_str(&text)
}

/// Read the log text. A file whose only line is `not json` fails the header check.
pub fn read_str(text: &str) -> Result<FrameLog, String> {
    if text.is_empty() {
        return Err(header_error());
    }
    let ends_with_newline = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        return Err(header_error());
    }
    let header: Value = match serde_json::from_str(lines[0]) {
        Ok(value) => value,
        Err(_) => return Err(header_error()),
    };
    if header.get("kind").and_then(Value::as_str) != Some("header")
        || header.get("version").and_then(Value::as_u64) != Some(3)
    {
        return Err(header_error());
    }

    let mut records = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(1) {
        if line.is_empty() {
            return Err("The frame log contains a line that is not JSON.".to_string());
        }
        match serde_json::from_str::<Value>(line) {
            Ok(value) => records.push(value),
            Err(_) => {
                let is_last = index + 1 == lines.len();
                if is_last && !ends_with_newline {
                    break;
                }
                return Err("The frame log contains a line that is not JSON.".to_string());
            }
        }
    }
    interpret(&records)
}

fn header_error() -> String {
    "The frame log does not start with a version 3 header.".to_string()
}

struct SeenOut {
    id: Value,
    generation: Value,
    method: String,
    params: Value,
}

fn interpret(records: &[Value]) -> Result<FrameLog, String> {
    let mut log = FrameLog {
        events: Vec::new(),
        initial_snapshot: None,
        snapshots: Vec::new(),
        undecodable: 0,
        skipped: 0,
        undecodable_types: Vec::new(),
        queued_admissions_ignored: 0,
        outbound: Vec::new(),
    };
    let mut seen: Vec<SeenOut> = Vec::new();

    for record in records {
        let kind = record.get("kind").and_then(Value::as_str).unwrap_or("");
        if kind != "frame" {
            log.skipped += 1;
            continue;
        }
        if record.get("parseError").is_some() || record.get("frame").is_none() {
            log.skipped += 1;
            continue;
        }
        let Some(frame) = record.get("frame") else {
            log.skipped += 1;
            continue;
        };
        if frame.is_null() {
            log.skipped += 1;
            continue;
        }
        let Some(frame) = frame.as_object() else {
            log.undecodable += 1;
            continue;
        };
        let dir = record.get("dir").and_then(Value::as_str).unwrap_or("");
        let generation = record.get("generation").cloned().unwrap_or(Value::Null);
        if dir == "out" {
            apply_outbound(&mut log, &mut seen, frame, generation)?;
        } else if dir == "in" {
            apply_inbound(&mut log, &seen, frame, generation);
        } else {
            log.skipped += 1;
        }
    }
    Ok(log)
}

fn apply_outbound(
    log: &mut FrameLog,
    seen: &mut Vec<SeenOut>,
    frame: &serde_json::Map<String, Value>,
    generation: Value,
) -> Result<(), String> {
    let method = frame
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let params = frame
        .get("params")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    if method == "prompt.submit" {
        reject_unattended(&params)?;
    }
    let id = frame.get("id").cloned().unwrap_or(Value::Null);
    seen.push(SeenOut {
        id,
        generation,
        method: method.clone(),
        params: params.clone(),
    });
    match method.as_str() {
        "prompt.submit" => {
            log.outbound.push(Outbound {
                method,
                params: params.clone(),
            });
            match submit_event(&params) {
                Some(event) => log.events.push(event),
                None => log.undecodable += 1,
            }
        }
        "session.interrupt" => {
            log.outbound.push(Outbound { method, params });
            log.events.push(Event::Interrupt);
        }
        "approval.respond" => {
            log.outbound.push(Outbound {
                method,
                params: params.clone(),
            });
            match answer_event(&params, true) {
                Some(event) => log.events.push(event),
                None => log.undecodable += 1,
            }
        }
        "clarify.respond" => {
            log.outbound.push(Outbound {
                method,
                params: params.clone(),
            });
            match answer_event(&params, false) {
                Some(event) => log.events.push(event),
                None => log.undecodable += 1,
            }
        }
        _ => {}
    }
    Ok(())
}

fn reject_unattended(params: &Value) -> Result<(), String> {
    match params.get("unattended") {
        None | Some(Value::Bool(false)) => Ok(()),
        _ => Err("prompt.submit must not set unattended to true.".to_string()),
    }
}

fn submit_event(params: &Value) -> Option<Event> {
    let text = params.get("text").and_then(Value::as_str)?.to_string();
    let submission_id = params
        .get("submission_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            params
                .get("input_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })?
        .to_string();
    Some(Event::Submit {
        display: text.clone(),
        text,
        submission_id,
        attachments: Vec::new(),
    })
}

fn answer_event(params: &Value, approval: bool) -> Option<Event> {
    let key = prompt_key(params)?;
    if approval {
        let choice = params.get("choice").and_then(Value::as_str)?.to_string();
        Some(Event::AnswerApproval { key, choice })
    } else {
        let answer = params.get("answer").and_then(Value::as_str)?.to_string();
        Some(Event::AnswerClarify { key, answer })
    }
}

fn prompt_key(params: &Value) -> Option<PromptKey> {
    let session_id = params.get("session_id").and_then(Value::as_str)?;
    let prompt_id = params.get("prompt_id").and_then(Value::as_str)?;
    let execution_generation = params.get("execution_generation").and_then(Value::as_i64)?;
    Some(PromptKey {
        session_id: SessionId::new(session_id),
        prompt_id: prompt_id.to_string(),
        execution_generation,
    })
}

fn apply_inbound(
    log: &mut FrameLog,
    seen: &[SeenOut],
    frame: &serde_json::Map<String, Value>,
    generation: Value,
) {
    if frame.get("method").and_then(Value::as_str) == Some("event") {
        let params = frame.get("params").cloned().unwrap_or(Value::Null);
        match EventFrame::from_params(&params) {
            Ok(decoded) => {
                let session_id = decoded.session_id.clone().unwrap_or_default();
                log.events.push(Event::Session {
                    session_id: SessionId::new(session_id),
                    frame: decoded,
                    raw: params,
                });
            }
            Err(_) => {
                log.undecodable += 1;
                let name = params
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("an unnamed type");
                log.undecodable_types.push(name.to_string());
            }
        }
        return;
    }

    let is_reply = frame.get("method").is_none()
        && (frame.get("result").is_some() || frame.get("error").is_some());
    if !is_reply {
        log.skipped += 1;
        return;
    }
    let id = frame.get("id").cloned().unwrap_or(Value::Null);
    let matched = seen
        .iter()
        .rev()
        .find(|outbound| outbound.id == id && outbound.generation == generation);
    let Some(outbound) = matched else {
        if let Some(result) = frame.get("result") {
            if looks_like_snapshot(result) {
                keep_snapshot(log, result);
            } else {
                log.skipped += 1;
            }
        } else {
            log.skipped += 1;
        }
        return;
    };

    if let Some(error) = frame.get("error") {
        apply_error(log, outbound, error);
        return;
    }
    let Some(result) = frame.get("result") else {
        log.skipped += 1;
        return;
    };
    match outbound.method.as_str() {
        "session.create" | "session.resume" => keep_snapshot(log, result),
        "approval.respond" | "clarify.respond"
            if result.get("status").and_then(Value::as_str) == Some("already_resolved") =>
        {
            match prompt_key(&outbound.params) {
                Some(key) => log.events.push(Event::AnswerAlreadyResolved { key }),
                None => log.undecodable += 1,
            }
        }
        _ => log.skipped += 1,
    }
}

fn apply_error(log: &mut FrameLog, outbound: &SeenOut, error: &Value) {
    let reason = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("The gateway returned an error.")
        .to_string();
    match outbound.method.as_str() {
        "prompt.submit" => {
            let Some(submission_id) = outbound
                .params
                .get("submission_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .or_else(|| outbound.params.get("input_id").and_then(Value::as_str))
            else {
                log.undecodable += 1;
                return;
            };
            log.events.push(Event::SubmitFailed {
                submission_id: submission_id.to_string(),
                reason,
            });
        }
        "session.interrupt" => log.events.push(Event::InterruptFailed),
        "approval.respond" | "clarify.respond" => match prompt_key(&outbound.params) {
            Some(key) => log.events.push(Event::AnswerFailed { key }),
            None => log.undecodable += 1,
        },
        _ => log.skipped += 1,
    }
}

fn looks_like_snapshot(result: &Value) -> bool {
    let Some(object) = result.as_object() else {
        return false;
    };
    object.get("session_id").and_then(Value::as_str).is_some()
        && object.get("messages").and_then(Value::as_array).is_some()
        && object.get("replay_epoch").and_then(Value::as_str).is_some()
}

fn keep_snapshot(log: &mut FrameLog, result: &Value) {
    match serde_json::from_value::<CanonicalSnapshot>(result.clone()) {
        Ok(snapshot) => {
            if log.snapshots.is_empty() {
                log.queued_admissions_ignored = snapshot.pending.len();
            }
            if log.events.is_empty() && log.initial_snapshot.is_none() {
                log.initial_snapshot = Some(snapshot.clone());
            }
            log.snapshots.push(snapshot);
        }
        Err(_) => log.undecodable += 1,
    }
}
