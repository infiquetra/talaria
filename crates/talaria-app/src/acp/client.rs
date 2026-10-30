//! Standard input/output transport and decoding for agent sessions.

use std::collections::VecDeque;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use talaria_core::fleet::acp::{AcpMapping, AcpSessionKey, AcpUpdate};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::watch;

use super::{AcpAgentEntry, AcpError};

const HANDSHAKE: Duration = Duration::from_secs(10);
const EXIT_GRACE: Duration = Duration::from_secs(2);
const STDERR_LIMIT: usize = 64 * 1024;

pub(crate) struct Transport {
    pub child: Child,
    pub input: Option<ChildStdin>,
    pub output: BufReader<ChildStdout>,
    pub read_buffer: Vec<u8>,
    pub stderr: watch::Receiver<String>,
}

pub(crate) struct Initialization {
    pub agent_name: String,
    pub auth_methods: Vec<Value>,
}

impl Transport {
    pub async fn launch(
        entry: &AcpAgentEntry,
        powers: bool,
    ) -> Result<(Self, Initialization), AcpError> {
        let mut child = Command::new(&entry.command)
            .args(&entry.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                AcpError(format!(
                    "Could not start {}: {}",
                    entry.display_name,
                    error.kind()
                ))
            })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| AcpError("Agent input is unavailable.".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| AcpError("Agent output is unavailable.".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AcpError("Agent diagnostics are unavailable.".into()))?;
        let (sender, receiver) = watch::channel(String::new());
        tokio::spawn(capture_stderr(stderr, sender));
        let mut transport = Self {
            child,
            input: Some(input),
            output: BufReader::new(output),
            read_buffer: Vec::new(),
            stderr: receiver,
        };
        let initialized = async {
            transport.send(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":1,
                "clientInfo":{"name":"talaria","version":env!("CARGO_PKG_VERSION")},
                "clientCapabilities":{"fs":{"readTextFile":powers && entry.allow_fs,"writeTextFile":powers && entry.allow_fs},"terminal":powers && entry.allow_terminal}
            }})).await?;
            loop {
                let response = transport
                    .read()
                    .await?
                    .ok_or_else(|| AcpError("Agent exited during initialization.".into()))?;
                if response.get("id") != Some(&json!(1)) {
                    continue;
                }
                if let Some(error) = response.get("error") {
                    return Err(AcpError(format!(
                        "Agent initialization failed: {}",
                        safe_text(error["message"].as_str().unwrap_or("refused"))
                    )));
                }
                let result = response
                    .get("result")
                    .filter(|value| value.is_object())
                    .ok_or_else(|| AcpError("Malformed agent initialization response.".into()))?;
                let version = result
                    .get("protocolVersion")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| AcpError("Agent did not provide a protocol version.".into()))?;
                if version != 1 {
                    return Err(AcpError(format!(
                        "Agent protocol version {version} does not match offered version 1."
                    )));
                }
                let name = result
                    .pointer("/agentInfo/name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&entry.display_name);
                let methods = result
                    .get("authMethods")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                return Ok(Initialization {
                    agent_name: safe_text(name),
                    auth_methods: methods,
                });
            }
        };
        let deadline = if powers {
            HANDSHAKE
        } else {
            HANDSHAKE - EXIT_GRACE
        };
        match tokio::time::timeout(deadline, initialized).await {
            Ok(Ok(init)) => Ok((transport, init)),
            Ok(Err(mut error)) => {
                transport.terminate().await;
                let tail = transport.stderr.borrow().clone();
                if !tail.is_empty() {
                    error.0.push_str(&format!(" ({tail})"));
                }
                Err(error)
            }
            Err(_) => {
                transport.terminate().await;
                let tail = transport.stderr.borrow().clone();
                let detail = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" ({tail})")
                };
                Err(AcpError(format!("Agent initialization timed out.{detail}")))
            }
        }
    }

    pub async fn send(&mut self, value: &Value) -> Result<(), AcpError> {
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| AcpError("Agent input is closed.".into()))?;
        let mut bytes = serde_json::to_vec(value)
            .map_err(|_| AcpError("Could not encode agent message.".into()))?;
        bytes.push(b'\n');
        input
            .write_all(&bytes)
            .await
            .map_err(|_| AcpError("Could not write to agent.".into()))?;
        input
            .flush()
            .await
            .map_err(|_| AcpError("Could not write to agent.".into()))
    }

    pub async fn read(&mut self) -> Result<Option<Value>, AcpError> {
        read_output(&mut self.output, &mut self.read_buffer).await
    }

    pub async fn terminate(&mut self) {
        self.input.take();
        if tokio::time::timeout(EXIT_GRACE, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
            let _ = self.child.wait().await;
        }
    }
}

pub(crate) async fn read_output<R: AsyncBufRead + Unpin>(
    output: &mut R,
    pending: &mut Vec<u8>,
) -> Result<Option<Value>, AcpError> {
    const MAX_FRAME: usize = 4 * 1024 * 1024;
    let count = output
        .take((MAX_FRAME + 1 - pending.len()) as u64)
        .read_until(b'\n', pending)
        .await
        .map_err(|_| AcpError("Could not read agent output.".into()))?;
    if count == 0 && pending.is_empty() {
        return Ok(None);
    }
    if pending.len() > MAX_FRAME {
        return Err(AcpError("Agent message is too large.".into()));
    }
    if pending.last() != Some(&b'\n') {
        return Err(AcpError("Agent sent malformed protocol output.".into()));
    }
    let value: Value = serde_json::from_slice(pending)
        .map_err(|_| AcpError("Agent sent malformed protocol output.".into()))?;
    pending.clear();
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(AcpError("Agent sent malformed protocol output.".into()));
    }
    Ok(Some(value))
}

async fn capture_stderr(mut stream: tokio::process::ChildStderr, sender: watch::Sender<String>) {
    let mut chunk = [0u8; 4096];
    let mut pending = Vec::new();
    let mut tail: VecDeque<String> = VecDeque::new();
    let mut bytes = 0usize;
    let mut too_long = false;
    loop {
        let count = stream.read(&mut chunk).await.unwrap_or_default();
        for byte in &chunk[..count] {
            if *byte == b'\n' {
                let line = if too_long {
                    "[oversized agent diagnostic]".to_owned()
                } else {
                    safe_text(&String::from_utf8_lossy(&pending))
                };
                pending.clear();
                too_long = false;
                if !line.is_empty() {
                    bytes += line.len() + 1;
                    tail.push_back(line);
                    while bytes > STDERR_LIMIT {
                        if let Some(old) = tail.pop_front() {
                            bytes = bytes.saturating_sub(old.len() + 1);
                        } else {
                            break;
                        }
                    }
                    sender.send_replace(tail.iter().cloned().collect::<Vec<_>>().join("\n"));
                }
            } else if pending.len() < STDERR_LIMIT && !too_long {
                pending.push(*byte);
            } else {
                pending.clear();
                too_long = true;
            }
        }
        if count == 0 {
            if !pending.is_empty() || too_long {
                let line = if too_long {
                    "[oversized agent diagnostic]".to_owned()
                } else {
                    safe_text(&String::from_utf8_lossy(&pending))
                };
                if !line.is_empty() {
                    bytes += line.len() + 1;
                    tail.push_back(line);
                    while bytes > STDERR_LIMIT {
                        if let Some(old) = tail.pop_front() {
                            bytes = bytes.saturating_sub(old.len() + 1);
                        } else {
                            break;
                        }
                    }
                    sender.send_replace(tail.iter().cloned().collect::<Vec<_>>().join("\n"));
                }
            }
            break;
        }
    }
}

pub(crate) fn safe_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let sensitive = [
        "password=",
        "password:",
        "secret=",
        "secret:",
        "token=",
        "token:",
        "authorization:",
    ]
    .iter()
    .any(|key| lower.contains(key))
        || lower.split_inclusive(['=', ':']).any(|part| {
            let Some(prefix) = part.strip_suffix(['=', ':']) else {
                return false;
            };
            let prefix = prefix.trim_end().trim_end_matches(['\'', '"']);
            let key = prefix
                .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
                .next()
                .unwrap_or_default();
            hermes_client::redact::is_suspicious_key(key)
        });
    if sensitive || !hermes_client::redact::scan_for_secrets(text).is_empty() {
        "[redacted]".into()
    } else {
        talaria_core::text::sanitize(text).into_owned()
    }
}

pub(crate) fn decode_update(key: &AcpSessionKey, update: &Value) -> Vec<AcpUpdate> {
    let kind = update
        .get("sessionUpdate")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let string = |name: &str| update.get(name).and_then(Value::as_str).map(str::to_owned);
    let key = key.clone();
    let one = match kind {
        "user_message_chunk" | "agent_message_chunk" => update
            .pointer("/content/text")
            .and_then(Value::as_str)
            .map(|text| AcpUpdate::MessageChunk {
                key,
                text: text.into(),
            }),
        "agent_thought_chunk" => {
            update
                .pointer("/content/text")
                .and_then(Value::as_str)
                .map(|text| AcpUpdate::ThoughtChunk {
                    key,
                    text: text.into(),
                })
        }
        "tool_call" => string("toolCallId").map(|id| AcpUpdate::ToolCall {
            key,
            id,
            title: string("title").unwrap_or_default(),
        }),
        "tool_call_update" => string("toolCallId").map(|id| AcpUpdate::ToolCallPatch {
            key,
            id,
            title: string("title"),
            body: update.get("content").map(content_text),
            status: string("status"),
        }),
        "plan" => Some(AcpUpdate::Plan {
            key,
            summary: update
                .get("entries")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.get("content").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default(),
        }),
        "available_commands_update" => Some(AcpUpdate::AvailableCommands {
            key,
            names: update
                .get("availableCommands")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            item.get("name").and_then(Value::as_str).map(str::to_owned)
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "current_mode_update" => string("currentModeId").map(|name| AcpUpdate::Mode { key, name }),
        "session_info_update" => string("title").map(|title| AcpUpdate::SessionInfo { key, title }),
        "usage_update" => Some(AcpUpdate::UsageFields {
            key,
            used: update.get("used").and_then(Value::as_u64),
            size: update.get("size").and_then(Value::as_u64),
            cost_amount: update.pointer("/cost/amount").and_then(Value::as_f64),
            cost_currency: update
                .pointer("/cost/currency")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }),
        "notice" => Some(AcpUpdate::Notice {
            key,
            text: string("message").unwrap_or_default(),
        }),
        "session_compaction" | "session_compacted" | "compaction" => Some(AcpUpdate::Compaction {
            key,
            text: String::new(),
        }),
        "config_option_update" => {
            return update
                .get("configOptions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| {
                    if item.get("category").and_then(Value::as_str) != Some("model") {
                        return None;
                    }
                    let current = item.get("currentValue").and_then(Value::as_str)?;
                    let label = item
                        .get("options")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find(|option| option.get("value").and_then(Value::as_str) == Some(current))
                        .and_then(|option| option.get("name").and_then(Value::as_str))
                        .unwrap_or(current);
                    Some(AcpUpdate::ConfigOption {
                        key: key.clone(),
                        name: "model".into(),
                        value: label.into(),
                    })
                })
                .collect();
        }
        _ => Some(AcpUpdate::Unknown {
            key,
            kind: kind.into(),
        }),
    };
    one.into_iter().collect()
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(content_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| object.get("content").map(content_text))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Replay protocol notifications from an operator-supplied recording.
pub fn replay_recording(recording: &str) -> Result<AcpMapping, AcpError> {
    let key = AcpSessionKey {
        agent_id: "replay".into(),
        session_id: "replay".into(),
    };
    let mut mapping = AcpMapping::default();
    mapping.start_turn();
    for line in recording.lines().filter(|line| !line.trim().is_empty()) {
        let item: Value = serde_json::from_str(line)
            .map_err(|_| AcpError("Malformed agent recording.".into()))?;
        let frame = item.get("frame").unwrap_or(&item);
        if frame.get("method").and_then(Value::as_str) == Some("session/update") {
            if let Some(update) = frame.pointer("/params/update") {
                for event in decode_update(&key, update) {
                    mapping.apply(event);
                }
            }
        } else if let Some(reason) = frame.pointer("/result/stopReason").and_then(Value::as_str) {
            mapping.finish(reason);
        }
    }
    Ok(mapping)
}

#[cfg(test)]
mod transport_tests {
    use super::{read_output, safe_text};
    use serde_json::json;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn interrupted_read_retains_the_incomplete_frame() {
        let (mut writer, reader) = tokio::io::duplex(128);
        let mut reader = BufReader::new(reader);
        let mut pending = Vec::new();
        writer.write_all(b"{\"jsonrpc\":\"2.0\",").await.unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(10),
                read_output(&mut reader, &mut pending)
            )
            .await
            .is_err()
        );
        writer
            .write_all(b"\"id\":7,\"result\":{}}\n")
            .await
            .unwrap();
        assert_eq!(
            read_output(&mut reader, &mut pending).await.unwrap(),
            Some(json!({"jsonrpc":"2.0","id":7,"result":{}}))
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn diagnostic_assignments_with_credential_keys_are_redacted() {
        assert_eq!(safe_text("api_key=example-value"), "[redacted]");
        assert_eq!(
            safe_text(r#"{"detail":"x","api_key":"example"}"#),
            "[redacted]"
        );
        assert_eq!(
            safe_text("diagnostic line, not protocol"),
            "diagnostic line, not protocol"
        );
    }
}
