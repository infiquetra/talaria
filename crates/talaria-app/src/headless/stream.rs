//! Streaming record types and projection.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Text,
    StreamJson,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamRecord {
    Init {
        session_id: String,
        profile: String,
        model: Option<String>,
        provider: Option<String>,
        input_format: InputFormat,
        elapsed_ms: u64,
    },
    Submitted {
        turn: u32,
        admission_id: String,
        status: String,
        elapsed_ms: u64,
    },
    Text {
        turn: u32,
        text: String,
        elapsed_ms: u64,
    },
    ToolUse {
        turn: u32,
        tool_id: String,
        name: String,
        context: String,
        input: Value,
        elapsed_ms: u64,
    },
    ToolResult {
        turn: u32,
        tool_id: String,
        name: String,
        is_error: Option<bool>,
        output: String,
        duration_ms: Option<u64>,
        elapsed_ms: u64,
    },
    Warning {
        turn: Option<u32>,
        text: String,
        elapsed_ms: u64,
    },
    Gap {
        turn: u32,
        elapsed_ms: u64,
    },
    InputError {
        turn: Option<u32>,
        line: Option<u64>,
        what: String,
        elapsed_ms: u64,
    },
}

pub const STREAM_VERSION: u32 = 1;
pub const MAX_OPEN_TOOL_STARTS: usize = 4096;
pub const MAX_TOOL_OUTPUT_BYTES: usize = 65_536;

pub fn encode(record: &StreamRecord) -> String {
    use serde::ser::{Serialize, SerializeMap, Serializer};

    struct Encoded<'a>(&'a StreamRecord);
    impl Serialize for Encoded<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(None)?;
            match self.0 {
                StreamRecord::Init {
                    session_id,
                    profile,
                    model,
                    provider,
                    input_format,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "system")?;
                    map.serialize_entry("subtype", "init")?;
                    map.serialize_entry("stream_version", &STREAM_VERSION)?;
                    map.serialize_entry("turn", &Option::<u32>::None)?;
                    map.serialize_entry("session_id", session_id)?;
                    map.serialize_entry("profile", profile)?;
                    map.serialize_entry("model", model)?;
                    map.serialize_entry("provider", provider)?;
                    map.serialize_entry(
                        "input_format",
                        match input_format {
                            InputFormat::Text => "text",
                            InputFormat::StreamJson => "stream-json",
                        },
                    )?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::Submitted {
                    turn,
                    admission_id,
                    status,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "submitted")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("admission_id", admission_id)?;
                    map.serialize_entry("status", status)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::Text {
                    turn,
                    text,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "text")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("text", text)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::ToolUse {
                    turn,
                    tool_id,
                    name,
                    context,
                    input,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "tool_use")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("tool_id", tool_id)?;
                    map.serialize_entry("name", name)?;
                    map.serialize_entry("context", context)?;
                    map.serialize_entry("input", input)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::ToolResult {
                    turn,
                    tool_id,
                    name,
                    is_error,
                    output,
                    duration_ms,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "tool_result")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("tool_id", tool_id)?;
                    map.serialize_entry("name", name)?;
                    map.serialize_entry("is_error", is_error)?;
                    map.serialize_entry("output", output)?;
                    map.serialize_entry("duration_ms", duration_ms)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::Warning {
                    turn,
                    text,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "warning")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("text", text)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::Gap { turn, elapsed_ms } => {
                    map.serialize_entry("type", "gap")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
                StreamRecord::InputError {
                    turn,
                    line,
                    what,
                    elapsed_ms,
                } => {
                    map.serialize_entry("type", "input_error")?;
                    map.serialize_entry("turn", turn)?;
                    map.serialize_entry("line", line)?;
                    map.serialize_entry("what", what)?;
                    map.serialize_entry("elapsed_ms", elapsed_ms)?;
                }
            }
            map.end()
        }
    }
    serde_json::to_string(&Encoded(record)).expect("stream record serializes to JSON")
}

pub struct StreamProjector {
    turn: u32,
    starts: HashMap<String, u64>,
    gap: bool,
    warned: HashSet<&'static str>,
}

impl StreamProjector {
    pub fn new(turn: u32) -> Self {
        Self {
            turn,
            starts: HashMap::new(),
            gap: false,
            warned: HashSet::new(),
        }
    }

    pub fn observe(&mut self, event_type: &str, payload: &Value, now_ms: u64) -> Vec<StreamRecord> {
        let kind = match event_type {
            "message.delta" => "message.delta",
            "tool.start" => "tool.start",
            "tool.complete" => "tool.complete",
            _ => return Vec::new(),
        };
        let Some(object) = payload.as_object() else {
            return if self.warned.insert(kind) {
                vec![StreamRecord::Warning {
                    turn: Some(self.turn),
                    text: format!(
                        "Hermes sent a {kind} event that was not an object; it was skipped."
                    ),
                    elapsed_ms: now_ms,
                }]
            } else {
                Vec::new()
            };
        };
        if kind == "message.delta" {
            return match object.get("text").and_then(Value::as_str) {
                Some(text) if !text.is_empty() && !self.gap => vec![StreamRecord::Text {
                    turn: self.turn,
                    text: text.into(),
                    elapsed_ms: now_ms,
                }],
                _ => Vec::new(),
            };
        }
        let field = |first: &str, second: &str, fallback: &str| {
            object
                .get(first)
                .and_then(Value::as_str)
                .or_else(|| object.get(second).and_then(Value::as_str))
                .unwrap_or(fallback)
                .to_owned()
        };
        let tool_id = field("tool_id", "tool_call_id", "");
        let name = field("name", "tool_name", "tool");
        if kind == "tool.start" {
            let mut records = vec![StreamRecord::ToolUse {
                turn: self.turn,
                tool_id: tool_id.clone(),
                name,
                context: object
                    .get("context")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                input: object
                    .get("args")
                    .filter(|value| value.is_object())
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                elapsed_ms: now_ms,
            }];
            if self.starts.contains_key(&tool_id) || self.starts.len() < MAX_OPEN_TOOL_STARTS {
                self.starts.insert(tool_id, now_ms);
            } else if self.warned.insert("tool-cap") {
                records.push(StreamRecord::Warning {
                    turn: Some(self.turn),
                    text:
                        "more than 4096 tool calls were open at once; later ones have no duration."
                            .into(),
                    elapsed_ms: now_ms,
                });
            }
            records
        } else {
            let mut output = match object.get("result") {
                Some(Value::String(text)) => text.clone(),
                Some(value) => value.to_string(),
                None => String::new(),
            };
            if output.len() > MAX_TOOL_OUTPUT_BYTES {
                let mut end = MAX_TOOL_OUTPUT_BYTES;
                while !output.is_char_boundary(end) {
                    end -= 1;
                }
                output.truncate(end);
                output.push_str("...");
            }
            vec![StreamRecord::ToolResult {
                turn: self.turn,
                duration_ms: self
                    .starts
                    .remove(&tool_id)
                    .map(|start| now_ms.saturating_sub(start)),
                tool_id,
                name,
                is_error: object.get("is_error").and_then(Value::as_bool),
                output,
                elapsed_ms: now_ms,
            }]
        }
    }

    pub fn mark_gap(&mut self, now_ms: u64) -> StreamRecord {
        self.gap = true;
        StreamRecord::Gap {
            turn: self.turn,
            elapsed_ms: now_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn encode_key_order_golden() {
        let cases = [
            (
                StreamRecord::Init {
                    session_id: "20261022_120000_ab12cd".into(),
                    profile: "fixture".into(),
                    model: Some("minimax-m3.1-flash".into()),
                    provider: Some("ollama-cloud".into()),
                    input_format: InputFormat::Text,
                    elapsed_ms: 180,
                },
                r#"{"type":"system","subtype":"init","stream_version":1,"turn":null,"session_id":"20261022_120000_ab12cd","profile":"fixture","model":"minimax-m3.1-flash","provider":"ollama-cloud","input_format":"text","elapsed_ms":180}"#,
            ),
            (
                StreamRecord::Submitted {
                    turn: 1,
                    admission_id: "adm-1".into(),
                    status: "started".into(),
                    elapsed_ms: 210,
                },
                r#"{"type":"submitted","turn":1,"admission_id":"adm-1","status":"started","elapsed_ms":210}"#,
            ),
            (
                StreamRecord::Text {
                    turn: 1,
                    text: "Listing".into(),
                    elapsed_ms: 900,
                },
                r#"{"type":"text","turn":1,"text":"Listing","elapsed_ms":900}"#,
            ),
            (
                StreamRecord::ToolUse {
                    turn: 1,
                    tool_id: "call_1".into(),
                    name: "terminal".into(),
                    context: "ls".into(),
                    input: json!({"command":"ls"}),
                    elapsed_ms: 1400,
                },
                r#"{"type":"tool_use","turn":1,"tool_id":"call_1","name":"terminal","context":"ls","input":{"command":"ls"},"elapsed_ms":1400}"#,
            ),
            (
                StreamRecord::ToolResult {
                    turn: 1,
                    tool_id: "call_1".into(),
                    name: "terminal".into(),
                    is_error: Some(false),
                    output: "a.txt\n".into(),
                    duration_ms: Some(350),
                    elapsed_ms: 1750,
                },
                r#"{"type":"tool_result","turn":1,"tool_id":"call_1","name":"terminal","is_error":false,"output":"a.txt\n","duration_ms":350,"elapsed_ms":1750}"#,
            ),
            (
                StreamRecord::Warning {
                    turn: Some(1),
                    text: "Hermes did not return the turn's usage figures.".into(),
                    elapsed_ms: 2600,
                },
                r#"{"type":"warning","turn":1,"text":"Hermes did not return the turn's usage figures.","elapsed_ms":2600}"#,
            ),
            (
                StreamRecord::Gap {
                    turn: 1,
                    elapsed_ms: 2000,
                },
                r#"{"type":"gap","turn":1,"elapsed_ms":2000}"#,
            ),
            (
                StreamRecord::InputError {
                    turn: None,
                    line: Some(1),
                    what: "line 1 is not valid JSON.".into(),
                    elapsed_ms: 200,
                },
                r#"{"type":"input_error","turn":null,"line":1,"what":"line 1 is not valid JSON.","elapsed_ms":200}"#,
            ),
        ];
        for (record, expected) in cases {
            assert_eq!(encode(&record), expected);
        }
    }

    #[test]
    fn text_deltas_skip_null_empty_and_non_strings() {
        let mut p = StreamProjector::new(1);
        assert_eq!(
            p.observe("message.delta", &json!({"text":"a"}), 1),
            vec![StreamRecord::Text {
                turn: 1,
                text: "a".into(),
                elapsed_ms: 1
            }]
        );
        for payload in [
            json!({"text":null}),
            json!({"text":""}),
            json!({"text":5}),
            json!({}),
        ] {
            assert!(p.observe("message.delta", &payload, 2).is_empty());
        }
    }

    #[test]
    fn tool_field_fallbacks() {
        let mut p = StreamProjector::new(2);
        let out = p.observe("tool.start", &json!({"tool_id":"primary","tool_call_id":"fallback","name":"terminal","tool_name":"other","context":"ls","args":{"command":"ls"}}), 10);
        assert_eq!(
            out,
            vec![StreamRecord::ToolUse {
                turn: 2,
                tool_id: "primary".into(),
                name: "terminal".into(),
                context: "ls".into(),
                input: json!({"command":"ls"}),
                elapsed_ms: 10
            }]
        );
        let out = p.observe("tool.start", &json!({"tool_id":4,"tool_call_id":"fallback","name":null,"tool_name":"shell","context":false,"args":[]}), 11);
        assert_eq!(
            out,
            vec![StreamRecord::ToolUse {
                turn: 2,
                tool_id: "fallback".into(),
                name: "shell".into(),
                context: "".into(),
                input: json!({}),
                elapsed_ms: 11
            }]
        );
        let out = p.observe(
            "tool.complete",
            &json!({"tool_id":null,"name":4,"is_error":"false"}),
            12,
        );
        assert_eq!(
            out,
            vec![StreamRecord::ToolResult {
                turn: 2,
                tool_id: "".into(),
                name: "tool".into(),
                is_error: None,
                output: "".into(),
                duration_ms: None,
                elapsed_ms: 12
            }]
        );
        assert_eq!(
            p.observe("tool.start", &json!(null), 13),
            vec![StreamRecord::Warning {
                turn: Some(2),
                text: "Hermes sent a tool.start event that was not an object; it was skipped."
                    .into(),
                elapsed_ms: 13
            }]
        );
        assert!(p.observe("tool.start", &json!([]), 14).is_empty());
        assert_eq!(
            p.observe("tool.complete", &json!(42), 15),
            vec![StreamRecord::Warning {
                turn: Some(2),
                text: "Hermes sent a tool.complete event that was not an object; it was skipped."
                    .into(),
                elapsed_ms: 15
            }]
        );
    }

    #[test]
    fn tool_result_output_cap_and_duration() {
        for text in ["x".repeat(70_000), "😀".repeat(20_000)] {
            let mut p = StreamProjector::new(1);
            p.observe("tool.start", &json!({"tool_id":"t"}), 100);
            let rows = p.observe(
                "tool.complete",
                &json!({"tool_id":"t","result":text,"is_error":true}),
                450,
            );
            match &rows[0] {
                StreamRecord::ToolResult {
                    output,
                    duration_ms,
                    is_error,
                    ..
                } => {
                    assert!(output.len() <= MAX_TOOL_OUTPUT_BYTES + 3);
                    assert!(output.ends_with("..."));
                    assert!(output.is_char_boundary(output.len()));
                    assert_eq!(*duration_ms, Some(350));
                    assert_eq!(*is_error, Some(true));
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        let mut p = StreamProjector::new(1);
        let rows = p.observe("tool.complete", &json!({"result":{"a":1}}), 1);
        assert!(
            matches!(&rows[0], StreamRecord::ToolResult { output, .. } if output == "{\"a\":1}")
        );
    }

    #[test]
    fn orphan_complete_and_open_start_cap() {
        let mut p = StreamProjector::new(1);
        let orphan = p.observe("tool.complete", &json!({"tool_id":"orphan"}), 1);
        assert!(matches!(
            &orphan[0],
            StreamRecord::ToolResult {
                duration_ms: None,
                ..
            }
        ));
        for n in 0..MAX_OPEN_TOOL_STARTS {
            let rows = p.observe("tool.start", &json!({"tool_id":format!("t{n}")}), 10);
            assert_eq!(rows.len(), 1);
        }
        let rows = p.observe("tool.start", &json!({"tool_id":"overflow"}), 11);
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().any(
                |r| matches!(r, StreamRecord::ToolUse { tool_id, .. } if tool_id == "overflow")
            )
        );
        assert!(rows.iter().any(|r| matches!(r, StreamRecord::Warning { text, .. } if text == "more than 4096 tool calls were open at once; later ones have no duration.")));
        let overflow = p.observe("tool.complete", &json!({"tool_id":"overflow"}), 12);
        assert!(matches!(
            &overflow[0],
            StreamRecord::ToolResult {
                duration_ms: None,
                ..
            }
        ));
        assert_eq!(
            p.observe("tool.start", &json!({"tool_id":"another"}), 13)
                .len(),
            1
        );
    }

    #[test]
    fn gap_stops_text_not_tools() {
        let mut p = StreamProjector::new(1);
        assert_eq!(
            p.mark_gap(2),
            StreamRecord::Gap {
                turn: 1,
                elapsed_ms: 2
            }
        );
        assert!(
            p.observe("message.delta", &json!({"text":"lost"}), 3)
                .is_empty()
        );
        assert!(matches!(
            &p.observe("tool.start", &json!({"tool_id":"t"}), 4)[0],
            StreamRecord::ToolUse { .. }
        ));
        assert!(matches!(
            &p.observe("tool.complete", &json!({"tool_id":"t"}), 5)[0],
            StreamRecord::ToolResult { .. }
        ));
    }
}
