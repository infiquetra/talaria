//! Wire decoding: one received text frame -> `Incoming`.
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event(EventFrame),
    Response { id: Option<i64>, result: Option<Value>, error: Option<Value> },
    /// Server-to-client request or anything we do not model.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventFrame {
    pub session_id: Option<String>,
    pub seq: Option<i64>,
    pub execution_generation: Option<i64>,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub prompt_id: String,
    pub command: String,
    pub description: String,
    pub choices: Vec<String>,
    pub execution_generation: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Ready,
    MessageStart,
    MessageDelta { text: Option<String> },
    MessageComplete { text: Option<String>, status: Option<String>, error: Option<String>, warning: Option<String> },
    ReasoningDelta { text: String },
    ToolStart { id: String, name: String, preview: String },
    ToolComplete { id: String, name: String, preview: String, duration_s: Option<f64>, diff: Option<String>, failed: bool },
    StatusUpdate { kind: String, text: String },
    SessionUsage { input_tokens: Option<i64>, output_tokens: Option<i64>, model: Option<String> },
    Error { message: String },
    ApprovalRequest(Approval),
    ApprovalSettled { prompt_id: String },
    Other(String),
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

/// Hermes pre-colours `inline_diff` with ANSI escapes; the sign must be read from clean text.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            if it.peek() == Some(&'[') {
                it.next();
                for n in it.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn preview_of(p: &Value) -> String {
    for k in ["preview", "context", "args_text", "summary"] {
        if let Some(t) = s(p, k) {
            if !t.is_empty() {
                return t;
            }
        }
    }
    // Fall back to a compact view of the arguments.
    match p.get("args") {
        Some(Value::Object(m)) if !m.is_empty() => {
            if let Some(Value::String(c)) = m.get("command").or_else(|| m.get("path")) {
                return c.clone();
            }
            Value::Object(m.clone()).to_string()
        }
        _ => String::new(),
    }
}

pub fn decode(line: &str) -> Incoming {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Incoming::Other(line.chars().take(80).collect()),
    };
    if v.get("method").and_then(Value::as_str) == Some("event") {
        let params = v.get("params").cloned().unwrap_or(Value::Null);
        return Incoming::Event(decode_event(&params));
    }
    if v.get("method").is_none() && (v.get("result").is_some() || v.get("error").is_some()) {
        return Incoming::Response {
            id: v.get("id").and_then(Value::as_i64),
            result: v.get("result").cloned(),
            error: v.get("error").cloned(),
        };
    }
    Incoming::Other(s(&v, "method").unwrap_or_default())
}

fn decode_event(params: &Value) -> EventFrame {
    let ty = s(params, "type").unwrap_or_default();
    let p = params.get("payload").cloned().unwrap_or(Value::Null);
    let event = match ty.as_str() {
        "gateway.ready" => Event::Ready,
        "message.start" => Event::MessageStart,
        "message.delta" => Event::MessageDelta { text: s(&p, "text") },
        "message.complete" => Event::MessageComplete {
            text: s(&p, "text"),
            status: s(&p, "status"),
            error: s(&p, "error"),
            warning: s(&p, "warning"),
        },
        "reasoning.delta" | "thinking.delta" => Event::ReasoningDelta { text: s(&p, "text").unwrap_or_default() },
        "tool.start" => Event::ToolStart {
            id: s(&p, "tool_id").unwrap_or_default(),
            name: s(&p, "name").unwrap_or_default(),
            preview: preview_of(&p),
        },
        "tool.complete" => Event::ToolComplete {
            id: s(&p, "tool_id").unwrap_or_default(),
            name: s(&p, "name").unwrap_or_default(),
            preview: preview_of(&p),
            duration_s: p.get("duration_s").and_then(Value::as_f64),
            diff: s(&p, "inline_diff").map(|d| strip_ansi(&d)).filter(|d| !d.trim().is_empty()),
            failed: p.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        },
        "status.update" => Event::StatusUpdate {
            kind: s(&p, "kind").unwrap_or_default(),
            text: s(&p, "text").unwrap_or_default(),
        },
        "session.usage" => {
            let u = p.get("usage").unwrap_or(&p);
            Event::SessionUsage {
                input_tokens: p.get("input_tokens").or_else(|| u.get("input")).and_then(Value::as_i64),
                output_tokens: p.get("output_tokens").or_else(|| u.get("output")).and_then(Value::as_i64),
                model: s(&p, "model"),
            }
        }
        "error" => Event::Error { message: s(&p, "message").unwrap_or_else(|| p.to_string()) },
        "approval.request" => Event::ApprovalRequest(Approval {
            prompt_id: s(&p, "prompt_id").or_else(|| s(params, "prompt_id")).unwrap_or_default(),
            command: s(&p, "command").unwrap_or_default(),
            description: s(&p, "description").unwrap_or_default(),
            choices: p
                .get("choices")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect())
                .filter(|c: &Vec<String>| !c.is_empty())
                .unwrap_or_else(|| ["once", "session", "always", "deny"].map(String::from).to_vec()),
            execution_generation: p
                .get("execution_generation")
                .or_else(|| params.get("execution_generation"))
                .and_then(Value::as_i64),
        }),
        "approval.settled" => Event::ApprovalSettled {
            prompt_id: s(&p, "prompt_id").or_else(|| s(params, "prompt_id")).unwrap_or_default(),
        },
        other => Event::Other(other.to_owned()),
    };
    EventFrame {
        session_id: s(params, "session_id"),
        seq: params.get("seq").and_then(Value::as_i64),
        execution_generation: params.get("execution_generation").and_then(Value::as_i64),
        event,
    }
}
