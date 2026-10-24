//! Reading and validating what the external command printed.

use serde_json::Value;

use super::Tone;

/// One row of the command's output.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalRow {
    /// The text of the row, shown literally.
    pub text: String,
    /// The tone the row asks for.
    pub tone: Tone,
}

/// Why a run produced no usable rows.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The command printed nothing at all.
    NoOutput,
    /// The command printed bytes that are not text.
    InvalidUtf8,
    /// The command printed a document that is not a valid version two document.
    InvalidDocument(String),
    /// The command failed. The code is `None` when it died from a signal.
    NonzeroExit {
        /// The exit code, or `None` for a signal.
        code: Option<i32>,
        /// The first non-blank line of the standard error.
        stderr_line: Option<String>,
    },
    /// The command did not finish in time.
    TimedOut,
    /// The program does not exist.
    NotFound,
    /// The program could not be started.
    SpawnFailed(String),
}

impl Failure {
    /// The plain text shown in the status row.
    pub fn marker(&self) -> String {
        match self {
            Failure::NoOutput => "status: no output".to_string(),
            Failure::InvalidUtf8 => "status: invalid output (not UTF-8)".to_string(),
            Failure::InvalidDocument(_) => "status: invalid script document".to_string(),
            Failure::NonzeroExit { code, stderr_line } => {
                let head = match code {
                    Some(code) => format!("status: command failed (exit {code})"),
                    None => "status: command failed (signal)".to_string(),
                };
                match stderr_line {
                    Some(line) => format!("{head}: {line}"),
                    None => head,
                }
            }
            Failure::TimedOut => "status: command timed out".to_string(),
            Failure::NotFound => "status: command not found".to_string(),
            Failure::SpawnFailed(_) => "status: could not start command".to_string(),
        }
    }
}

/// What the command printed.
#[derive(Clone, Debug, PartialEq)]
pub enum ParsedOutput {
    /// The rows, and whether the output was cut.
    Rows {
        /// The rows, at most `row_limit` of them.
        rows: Vec<ExternalRow>,
        /// Whether the output was cut at the byte cap or at the row bound.
        truncated: bool,
    },
    /// The run produced no rows.
    Failure(Failure),
}

/// The rows of the output, and whether it was cut.
///
/// A text that parses as a JSON object must be a version two document; anything else stays
/// plain text. Output cut at the byte cap is never parsed as a document.
pub fn parse_output(raw: &[u8], byte_limit: usize, row_limit: usize) -> ParsedOutput {
    let cut = raw.len() > byte_limit;
    let slice = if cut { &raw[..byte_limit] } else { raw };
    let text = match std::str::from_utf8(slice) {
        Ok(text) => text.to_string(),
        Err(error) => {
            // A character split by the cap is tolerated; a bad byte is not.
            if !(cut && error.error_len().is_none()) {
                return ParsedOutput::Failure(Failure::InvalidUtf8);
            }
            match std::str::from_utf8(&slice[..error.valid_up_to()]) {
                Ok(text) => text.to_string(),
                Err(_) => return ParsedOutput::Failure(Failure::InvalidUtf8),
            }
        }
    };
    if text.is_empty() {
        return ParsedOutput::Failure(Failure::NoOutput);
    }

    if !cut
        && let Ok(value) = serde_json::from_str::<Value>(&text)
        && value.is_object()
    {
        return match parse_document(&value, row_limit) {
            Ok((rows, bounded)) => ParsedOutput::Rows {
                rows,
                truncated: bounded,
            },
            Err(reason) => ParsedOutput::Failure(Failure::InvalidDocument(reason)),
        };
    }

    let (rows, bounded) = bound_rows(plain_rows(&text), row_limit, Tone::Muted);
    ParsedOutput::Rows {
        rows,
        truncated: cut || bounded,
    }
}

/// The plain rows of a text: one per line, without the line ends.
fn plain_rows(text: &str) -> Vec<ExternalRow> {
    let normalized = text.replace("\r\n", "\n");
    let normalized = normalized.strip_suffix('\r').unwrap_or(&normalized);
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    if normalized.ends_with('\n') {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| ExternalRow {
            text: line.to_string(),
            tone: Tone::Muted,
        })
        .collect()
}

/// The rows of a version two document, and whether the row bound was applied.
fn parse_document(value: &Value, row_limit: usize) -> Result<(Vec<ExternalRow>, bool), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "the document is not an object".to_string())?;
    for key in object.keys() {
        if key != "version" && key != "rows" {
            return Err(format!("the document holds the unknown field {key}"));
        }
    }
    let version = object
        .get("version")
        .ok_or_else(|| "the document holds no version".to_string())?;
    if version.as_u64() != Some(2) {
        return Err("the document version is not 2".to_string());
    }
    let entries = object
        .get("rows")
        .ok_or_else(|| "the document holds no rows".to_string())?
        .as_array()
        .ok_or_else(|| "the rows are not a list".to_string())?;

    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        rows.push(document_row(entry)?);
    }
    Ok(bound_rows(rows, row_limit, Tone::Text))
}

/// One row of a version two document: a string, or an object with a text and an optional colour.
fn document_row(entry: &Value) -> Result<ExternalRow, String> {
    match entry {
        Value::String(text) => Ok(ExternalRow {
            text: text.clone(),
            tone: Tone::Text,
        }),
        Value::Object(fields) => {
            for key in fields.keys() {
                if key != "text" && key != "color" {
                    return Err(format!("the row holds the unknown field {key}"));
                }
            }
            let text = fields
                .get("text")
                .ok_or_else(|| "the row holds no text".to_string())?
                .as_str()
                .ok_or_else(|| "the row's text is not a string".to_string())?;
            let tone = match fields.get("color") {
                None => Tone::Text,
                Some(color) => {
                    let color = color
                        .as_str()
                        .ok_or_else(|| "the row's colour is not a string".to_string())?;
                    // An unknown colour falls back to the text tone: a request, never a style.
                    Tone::from_token(color)
                }
            };
            Ok(ExternalRow {
                text: text.to_string(),
                tone,
            })
        }
        _ => Err("a row is neither a string nor an object".to_string()),
    }
}

/// The rows at most `row_limit` of them, with a marker row when there were more.
fn bound_rows(
    rows: Vec<ExternalRow>,
    row_limit: usize,
    marker_tone: Tone,
) -> (Vec<ExternalRow>, bool) {
    if rows.len() <= row_limit {
        return (rows, false);
    }
    if row_limit == 0 {
        return (Vec::new(), true);
    }
    let total = rows.len();
    let mut kept: Vec<ExternalRow> = rows.into_iter().take(row_limit - 1).collect();
    kept.push(ExternalRow {
        text: format!("… truncated ({total} rows)"),
        tone: marker_tone,
    });
    (kept, true)
}
