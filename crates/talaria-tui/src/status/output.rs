//! Reading and validating what the external command printed.

use super::Tone;

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalRow {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    NoOutput,
    InvalidUtf8,
    InvalidDocument(String),
    NonzeroExit {
        code: Option<i32>,
        stderr_line: Option<String>,
    },
    TimedOut,
    NotFound,
    SpawnFailed(String),
}

impl Failure {
    /// The plain text shown in the status row.
    pub fn marker(&self) -> String {
        todo!("status")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParsedOutput {
    Rows {
        rows: Vec<ExternalRow>,
        truncated: bool,
    },
    Failure(Failure),
}

pub fn parse_output(raw: &[u8], byte_limit: usize, row_limit: usize) -> ParsedOutput {
    let _ = (raw, byte_limit, row_limit);
    todo!("status")
}
