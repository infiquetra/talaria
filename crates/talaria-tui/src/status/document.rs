//! The standard-input document sent to the external command.

use serde::Serialize;

use super::segments::StatusInputs;
use super::{Connection, RunMode, TurnState};

/// The eight fields of the document, in order.
pub const DOCUMENT_FIELDS: [&str; 8] = [
    "version",
    "mode",
    "connection",
    "session",
    "turn",
    "pending_prompts",
    "subagents",
    "usage",
];

/// The document the command reads on its standard input.
#[derive(Clone, Debug, Serialize)]
pub struct StatusDocument {
    /// Always `1` for this document shape.
    pub version: u32,
    /// Whether the client is live or replaying.
    pub mode: RunMode,
    /// The attach state of the link.
    pub connection: Connection,
    /// The focused session.
    pub session: SessionDocument,
    /// The state of the focused session's turn.
    pub turn: TurnState,
    /// Outstanding prompts of the focused session only.
    pub pending_prompts: u32,
    /// The sub-agent counts.
    pub subagents: SubagentCounts,
    /// The token counts, when they have been observed.
    pub usage: Option<UsageCounts>,
}

/// The focused session in the document.
#[derive(Clone, Debug, Serialize)]
pub struct SessionDocument {
    /// The session identifier.
    pub id: String,
    /// The title, or null.
    pub title: Option<String>,
}

/// The sub-agent counts in the document.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct SubagentCounts {
    /// Sub-agents running.
    pub active: u32,
    /// Sub-agents in a terminal state.
    pub terminal: u32,
}

/// The token counts in the document.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct UsageCounts {
    /// Tokens read.
    pub input_tokens: u64,
    /// Tokens written.
    pub output_tokens: u64,
}

impl StatusDocument {
    /// The document for `inputs`, or `None` without a session.
    pub fn from_inputs(inputs: &StatusInputs) -> Option<StatusDocument> {
        let session = inputs.session.as_ref()?;
        Some(StatusDocument {
            version: 1,
            mode: inputs.mode,
            connection: inputs.connection,
            session: SessionDocument {
                id: session.id.clone(),
                title: session.title.clone(),
            },
            turn: inputs.turn,
            pending_prompts: inputs.pending_prompts,
            subagents: SubagentCounts {
                active: inputs.subagents_active,
                terminal: inputs.subagents_terminal,
            },
            usage: inputs.usage_tokens.map(|usage| UsageCounts {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
            }),
        })
    }

    /// The document as compact JSON and a newline.
    pub fn encode(&self) -> Vec<u8> {
        // The document holds only strings and integers, so it cannot fail to serialise.
        let mut bytes = serde_json::to_vec(self).expect("the document holds only plain values");
        bytes.push(b'\n');
        bytes
    }
}
