//! What a slash reply means, and the plain words for a refusal. Issue #223.

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

/// Command output longer than this many bytes is cut.
pub const OUTPUT_LIMIT: usize = 262_144;
/// Untrusted refusal text (`detail`, a reason) is cut to this many characters.
pub const DETAIL_CHARS: usize = 160;

fn text_field<'a>(result: &'a Value, field: &str) -> Option<&'a str> {
    result.get(field).and_then(Value::as_str)
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

fn not_understood(name: &str) -> CommandOutcome {
    CommandOutcome::Failed {
        message: format!(
            "Hermes answered /{name} in a form this version of Talaria does not understand."
        ),
    }
}

/// `output` cut to [`OUTPUT_LIMIT`] bytes on a character boundary, with a closing line that
/// says how many bytes were removed.
fn cut_output(output: &str) -> String {
    if output.len() <= OUTPUT_LIMIT {
        return output.to_owned();
    }
    let mut end = OUTPUT_LIMIT;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n(output cut: {} more bytes)",
        &output[..end],
        output.len() - end
    )
}

fn output(typed: &str, result: &Value) -> CommandOutcome {
    let body = text_field(result, "output").unwrap_or_default();
    let mut text = if body.trim().is_empty() {
        "(no output)".to_owned()
    } else {
        cut_output(body)
    };
    if let Some(warning) = non_blank(text_field(result, "warning")) {
        text = format!("warning: {warning}\n{text}");
    }
    CommandOutcome::Output {
        line: typed.trim().to_owned(),
        text,
    }
}

fn submit(typed: &str, result: &Value, empty: String) -> CommandOutcome {
    let Some(message) = non_blank(text_field(result, "message")) else {
        return CommandOutcome::Failed { message: empty };
    };
    let display = non_blank(text_field(result, "display")).unwrap_or(typed.trim());
    CommandOutcome::Submit {
        text: message.to_owned(),
        display: display.to_owned(),
        notice: non_blank(text_field(result, "notice")).map(str::to_owned),
    }
}

/// Turn a raw result into an outcome. An unknown `type` is [`CommandOutcome::Failed`].
///
/// The raw value is read, not the generated type, so a directive type that a newer Hermes
/// adds becomes plain words instead of a decode failure.
#[must_use]
pub fn interpret_result(line: &SlashLine, typed: &str, result: &Value) -> CommandOutcome {
    let name = line.name.as_str();
    if !result.is_object() {
        return not_understood(name);
    }
    match text_field(result, "type") {
        Some("exec" | "plugin") => output(typed, result),
        None if result.get("output").is_some() => output(typed, result),
        Some("send") => submit(typed, result, format!("/{name} returned an empty message.")),
        Some("skill") => submit(
            typed,
            result,
            format!("/{name}: the skill sent no instructions."),
        ),
        Some("prefill") => CommandOutcome::Prefill {
            text: text_field(result, "message").unwrap_or_default().to_owned(),
            notice: non_blank(text_field(result, "notice")).map(str::to_owned),
        },
        Some("alias") => {
            let Some(target) = non_blank(text_field(result, "target")) else {
                return not_understood(name);
            };
            let target = target.trim().trim_start_matches('/');
            let line = if line.arg.is_empty() {
                format!("/{target}")
            } else {
                format!("/{target} {}", line.arg)
            };
            CommandOutcome::Rerun { line }
        }
        _ => not_understood(name),
    }
}

/// One line of untrusted text: line breaks become spaces, cut to [`DETAIL_CHARS`].
fn one_line(text: &str) -> String {
    text.replace("\r\n", " ")
        .replace(['\n', '\r'], " ")
        .chars()
        .take(DETAIL_CHARS)
        .collect()
}

fn with_detail(message: String, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("{message}\n{detail}"),
        None => message,
    }
}

fn refused(name: String, message: String, dim: bool) -> CommandOutcome {
    CommandOutcome::Refused { name, message, dim }
}

fn unsupported(
    line: &SlashLine,
    detail: Option<&str>,
    catalog: Option<&CommandCatalog>,
) -> CommandOutcome {
    let name = line.name.as_str();
    let Some(catalog) = catalog else {
        return refused(
            name.to_owned(),
            with_detail(
                format!("Hermes refused /{name}: it is not a command this gateway runs."),
                detail,
            ),
            false,
        );
    };
    // Hermes attaches a hint only from its table of registry commands, never to a name it
    // does not know, so a hint marks the name as known even if this catalogue omits it.
    if !catalog.knows(name) && detail.is_none() {
        return refused(
            name.to_owned(),
            format!("Hermes does not know /{name}. Type / to see the commands you can use."),
            false,
        );
    }
    let canonical = catalog.canonical(name).map_or_else(
        || name.to_owned(),
        |found| found.trim_start_matches('/').to_owned(),
    );
    match line.arg.split_whitespace().next() {
        None => refused(
            canonical,
            with_detail(
                format!("/{name} is not available on the shared Hermes gateway."),
                detail,
            ),
            true,
        ),
        Some(word) => refused(
            canonical,
            with_detail(
                format!("/{name} {word} is not available on the shared Hermes gateway."),
                detail,
            ),
            false,
        ),
    }
}

fn rpc_refusal(
    line: &SlashLine,
    code: i64,
    message: &str,
    reason: Option<&str>,
    field: Option<&str>,
    detail: Option<&str>,
    catalog: Option<&CommandCatalog>,
) -> CommandOutcome {
    let name = line.name.as_str();
    let detail = detail.map(one_line);
    let detail = detail.as_deref();
    let text = match (code, reason) {
        (4001, Some("unsupported_command")) => return unsupported(line, detail, catalog),
        (4001, Some("permission_denied")) => {
            format!("/{name} was refused: this connection is not allowed to run it.")
        }
        (4001, Some("not_found")) => format!(
            "/{name} could not run: Hermes no longer has this session. Quit and start a new one \
             with talaria, or open another with talaria --resume <id>."
        ),
        (4001, Some("profile_mismatch")) => {
            format!("/{name} was refused: the session belongs to a different profile.")
        }
        (4001, Some("session_superseded")) => format!(
            "/{name} did not run: this conversation has moved on to a newer session and is \
             read-only."
        ),
        (4001, Some("invalid_params")) if field == Some("arg") => {
            let what = detail.map_or_else(|| one_line(message), str::to_owned);
            format!("/{name} could not read its arguments: {what}.")
        }
        (4001, Some("invalid_params")) => format!(
            "Hermes did not accept /{name} as sent. If it is one of your quick commands, check \
             its alias target; otherwise this is a Talaria bug, please report it."
        ),
        (4001, Some("command_unavailable")) => {
            format!("/{name} could not be loaded. The skill may be empty or missing.")
        }
        (4001, Some("cwd_unavailable")) => {
            format!("/{name} could not run: the session's working folder no longer exists.")
        }
        (4001, Some("runtime_draining")) => {
            format!("Hermes is restarting, so /{name} did not run. Try again in a moment.")
        }
        (5001, Some("storage_unavailable")) => {
            format!("Hermes could not read its session store, so /{name} did not run.")
        }
        _ => {
            let why = reason
                .filter(|reason| !reason.is_empty())
                .map(one_line)
                .or_else(|| (!message.is_empty()).then(|| one_line(message)))
                .unwrap_or_else(|| format!("code {code}"));
            match detail {
                Some(detail) => format!("/{name} failed: {why}: {detail}"),
                None => format!("/{name} failed: {why}"),
            }
        }
    };
    refused(name.to_owned(), text, false)
}

/// Turn a transport or refusal error into an outcome.
///
/// `catalog` is the catalogue held on this run, if one has ever arrived; it decides between
/// "does not know" and "is not available" for a refused command.
#[must_use]
pub fn describe_error(
    line: &SlashLine,
    error: &ErrorView,
    catalog: Option<&CommandCatalog>,
) -> CommandOutcome {
    let name = line.name.as_str();
    match error {
        ErrorView::Rpc {
            code,
            message,
            reason,
            field,
            detail,
        } => rpc_refusal(
            line,
            *code,
            message,
            reason.as_deref(),
            field.as_deref(),
            detail.as_deref(),
            catalog,
        ),
        ErrorView::TimedOut | ErrorView::ConnectionLost => CommandOutcome::OutcomeUnknown {
            message: format!(
                "The connection to Hermes was lost before /{name} answered. It may or may not \
                 have run; check before running it again."
            ),
        },
        ErrorView::NotConnected => CommandOutcome::Failed {
            message: format!("Talaria is not connected to Hermes, so /{name} did not run."),
        },
        ErrorView::BadResult => not_understood(name),
        ErrorView::LocalBug => CommandOutcome::Failed {
            message: format!(
                "Talaria could not send /{name}. This is a Talaria bug; please report it."
            ),
        },
    }
}
