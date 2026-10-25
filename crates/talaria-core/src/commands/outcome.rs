//! What a slash reply means. Issue #223.

use serde_json::Value;

use super::catalog::CommandCatalog;
use super::parse::SlashLine;

/// The outcome of one command. The application turns it into session events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Show `text` under the typed `line`.
    Output { line: String, text: String },
    /// Submit `text` to the model and show `display`.
    Submit {
        text: String,
        display: String,
        notice: Option<String>,
    },
    /// Load the composer. Nothing is sent.
    Prefill {
        text: String,
        notice: Option<String>,
    },
    /// Run the alias target once inside the same `execute`.
    Rerun { line: String },
    /// A refusal. `dim` is set only for a bare catalogue name.
    Refused {
        name: String,
        message: String,
        dim: bool,
    },
    /// A plain error notice.
    Failed { message: String },
    /// The command may have run. Never retried.
    OutcomeUnknown { message: String },
    /// A bare `/`. Nothing runs.
    Nothing,
}

/// A failure before a directive, or instead of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorView {
    /// A JSON-RPC error.
    Rpc {
        code: i64,
        message: String,
        reason: Option<String>,
        field: Option<String>,
        detail: Option<String>,
    },
    /// The call timed out after it was sent.
    TimedOut,
    /// The connection dropped after the call was sent.
    ConnectionLost,
    /// Nothing was sent.
    NotConnected,
    /// The result did not decode.
    BadResult,
    /// Talaria built a call it cannot send.
    LocalBug,
}

/// Turn a raw result into an outcome. An unknown `type` is [`CommandOutcome::Failed`].
#[must_use]
pub fn interpret_result(line: &SlashLine, typed: &str, result: &Value) -> CommandOutcome {
    let _ = (line, typed, result);
    todo!()
}

/// Turn a transport or refusal error into an outcome.
#[must_use]
pub fn describe_error(
    line: &SlashLine,
    error: &ErrorView,
    catalog: Option<&CommandCatalog>,
) -> CommandOutcome {
    let _ = (line, error, catalog);
    todo!()
}
