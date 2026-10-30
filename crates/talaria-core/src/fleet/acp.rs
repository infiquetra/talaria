//! Pure projection of an agent session into fleet state.

use super::{Outcome, RowState};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AcpSessionKey {
    pub agent_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcpUsage {
    pub used: u64,
    pub size: u64,
    pub cost_amount: Option<f64>,
    pub cost_currency: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcpCell {
    Text(String),
    Thought(String),
    Tool {
        id: String,
        title: String,
        body: String,
        status: Option<String>,
    },
    Plan(String),
    Notice(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AcpUpdate {
    MessageChunk {
        key: AcpSessionKey,
        text: String,
    },
    ThoughtChunk {
        key: AcpSessionKey,
        text: String,
    },
    ToolCall {
        key: AcpSessionKey,
        id: String,
        title: String,
    },
    ToolCallPatch {
        key: AcpSessionKey,
        id: String,
        title: Option<String>,
        body: Option<String>,
        status: Option<String>,
    },
    Plan {
        key: AcpSessionKey,
        summary: String,
    },
    AvailableCommands {
        key: AcpSessionKey,
        names: Vec<String>,
    },
    Mode {
        key: AcpSessionKey,
        name: String,
    },
    ConfigOption {
        key: AcpSessionKey,
        name: String,
        value: String,
    },
    SessionInfo {
        key: AcpSessionKey,
        title: String,
    },
    Notice {
        key: AcpSessionKey,
        text: String,
    },
    Compaction {
        key: AcpSessionKey,
        text: String,
    },
    Usage {
        key: AcpSessionKey,
        usage: AcpUsage,
    },
    UsageFields {
        key: AcpSessionKey,
        used: Option<u64>,
        size: Option<u64>,
        cost_amount: Option<f64>,
        cost_currency: Option<String>,
    },
    Unknown {
        key: AcpSessionKey,
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequest {
    pub key: AcpSessionKey,
    pub responder: String,
    pub tool_call_id: String,
    pub tool_title: String,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AcpMapping {
    pub transcript: Vec<AcpCell>,
    pub usage: Option<AcpUsage>,
    pub title: Option<String>,
    pub state: RowState,
    pub outcome: Option<Outcome>,
    pub diagnostics: Vec<String>,
    pub needs: Vec<PermissionRequest>,
    pub activity: Option<String>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub commands: Vec<String>,
}

impl Default for AcpMapping {
    fn default() -> Self {
        Self {
            transcript: Vec::new(),
            usage: None,
            title: None,
            state: RowState::Idle,
            outcome: None,
            diagnostics: Vec::new(),
            needs: Vec::new(),
            activity: None,
            model: None,
            mode: None,
            commands: Vec::new(),
        }
    }
}

impl AcpMapping {
    pub fn start_turn(&mut self) {
        self.state = RowState::Working;
        self.outcome = None;
    }
    pub fn apply(&mut self, update: AcpUpdate) {
        use AcpCell::{Text, Thought, Tool};
        match update {
            AcpUpdate::MessageChunk { text, .. } => {
                let text = clean(&text);
                if let Some(Text(last)) = self.transcript.last_mut() {
                    last.push_str(&text);
                } else if !text.is_empty() {
                    self.transcript.push(Text(text));
                }
            }
            AcpUpdate::ThoughtChunk { text, .. } => {
                let text = clean(&text);
                if let Some(Thought(last)) = self.transcript.last_mut() {
                    last.push_str(&text);
                } else if !text.is_empty() {
                    self.transcript.push(Thought(text));
                }
            }
            AcpUpdate::ToolCall { id, title, .. } => {
                self.transcript.push(Tool {
                    id,
                    title: clean(&title),
                    body: String::new(),
                    status: None,
                });
            }
            AcpUpdate::ToolCallPatch {
                id,
                title,
                body,
                status,
                ..
            } => {
                if let Some(Tool {
                    title: old_title,
                    body: old_body,
                    status: old_status,
                    ..
                }) = self
                    .transcript
                    .iter_mut()
                    .find(|cell| matches!(cell, Tool { id: known, .. } if *known == id))
                {
                    if let Some(title) = title {
                        *old_title = clean(&title);
                    }
                    if let Some(body) = body {
                        *old_body = clean(&body);
                    }
                    if let Some(status) = status {
                        *old_status = Some(clean(&status));
                    }
                } else {
                    self.note(format!("Unknown tool call: {}", clean(&id)));
                }
            }
            AcpUpdate::Plan { summary, .. } => {
                self.activity = summary
                    .lines()
                    .next()
                    .filter(|line| !line.is_empty())
                    .map(clean);
                if !summary.is_empty() {
                    self.transcript.push(AcpCell::Plan(clean(&summary)));
                }
            }
            AcpUpdate::AvailableCommands { names, .. } => {
                self.commands = names.iter().map(|name| clean(name)).collect();
            }
            AcpUpdate::Mode { name, .. } => self.mode = Some(clean(&name)),
            AcpUpdate::ConfigOption { name, value, .. } if name == "model" => {
                self.model = Some(clean(&value));
            }
            AcpUpdate::ConfigOption { .. } => {}
            AcpUpdate::SessionInfo { title, .. } => {
                if !title.is_empty() {
                    self.title = Some(clean(&title));
                }
            }
            AcpUpdate::Notice { text, .. } => self.transcript.push(AcpCell::Notice(clean(&text))),
            AcpUpdate::Compaction { .. } => self
                .transcript
                .push(AcpCell::Notice("context compacted".into())),
            AcpUpdate::Usage { usage, .. } => self.merge_usage(usage),
            AcpUpdate::UsageFields {
                used,
                size,
                cost_amount,
                cost_currency,
                ..
            } => {
                if let (Some(used), Some(size)) = (used, size) {
                    self.merge_usage(AcpUsage {
                        used,
                        size,
                        cost_amount,
                        cost_currency,
                    });
                } else {
                    self.note("Malformed usage update".into());
                }
            }
            AcpUpdate::Unknown { kind, .. } => {
                self.note(format!("Unknown agent update: {}", clean(&kind)))
            }
        }
    }
    pub fn finish(&mut self, stop_reason: &str) {
        let outcome = match stop_reason {
            "end_turn" | "max_tokens" | "max_turn_requests" => Outcome::Completed,
            "cancelled" => Outcome::Interrupted,
            "refusal" => Outcome::Unknown,
            other => {
                self.note(format!("Unknown stop reason: {}", clean(other)));
                Outcome::Unknown
            }
        };
        self.outcome = Some(outcome);
        self.state = if self.needs.is_empty() {
            RowState::Done
        } else {
            RowState::NeedsYou
        };
    }
    pub fn request_permission(&mut self, mut request: PermissionRequest) {
        request.options.retain(|option| {
            !option.id.is_empty() && !option.name.is_empty() && !option.kind.is_empty()
        });
        if request.options.is_empty() {
            self.note("Permission request has no usable options".into());
            return;
        }
        request.tool_title = clean(&request.tool_title);
        for option in &mut request.options {
            option.name = clean(&option.name);
            option.kind = clean(&option.kind);
        }
        if !self
            .needs
            .iter()
            .any(|need| need.responder == request.responder)
        {
            self.needs.push(request);
            self.state = RowState::NeedsYou;
        }
    }

    fn merge_usage(&mut self, mut usage: AcpUsage) {
        if let Some(currency) = &mut usage.cost_currency {
            *currency = clean(currency);
        }
        if usage.cost_amount.is_none() || usage.cost_currency.is_none() {
            usage.cost_amount = self.usage.as_ref().and_then(|old| old.cost_amount);
            usage.cost_currency = self
                .usage
                .as_ref()
                .and_then(|old| old.cost_currency.clone());
        }
        self.usage = Some(usage);
    }

    fn note(&mut self, text: String) {
        if !self.diagnostics.contains(&text) {
            self.diagnostics.push(text);
        }
    }
}

fn clean(text: &str) -> String {
    crate::text::sanitize(text).into_owned()
}
