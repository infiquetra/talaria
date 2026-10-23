//! Redaction, URL cleaning and the secret scan.
//!
//! Red-run stubs for P1-04: every body is `todo!("P1-04")`.

use crate::record::RecordError;

/// The marker that stands where a value was withheld.
pub const REDACTED: &str = "[redacted]";

/// One withheld value: where it was and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redaction {
    pub path: String,
    pub reason: String,
}

/// The result of redacting one text.
#[derive(Debug, Clone, PartialEq)]
pub struct Redacted {
    /// `None` when the text was not valid JSON.
    pub frame: Option<serde_json::Value>,
    pub redactions: Vec<Redaction>,
    pub parse_error: Option<String>,
}

/// One scan finding. Never holds the matched text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHit {
    pub line: usize,
    pub rule: &'static str,
}

/// Redacts frames and remembers the method of each open server request.
#[derive(Debug)]
pub struct Redactor {
    _private: (),
}

impl Redactor {
    pub fn new(substitutions: Vec<(String, String)>) -> Result<Redactor, RecordError> {
        let _ = substitutions;
        todo!("P1-04")
    }

    pub fn redact_inbound(&mut self, text: &str) -> Redacted {
        let _ = text;
        todo!("P1-04")
    }

    pub fn redact_outbound(&mut self, text: &str) -> Redacted {
        let _ = text;
        todo!("P1-04")
    }
}

pub fn redact_url(url: &str) -> String {
    let _ = url;
    todo!("P1-04")
}

pub fn is_suspicious_key(key: &str) -> bool {
    let _ = key;
    todo!("P1-04")
}

pub fn scan_for_secrets(text: &str) -> Vec<ScanHit> {
    let _ = text;
    todo!("P1-04")
}

pub fn scan_literals(text: &str, forbidden: &[String]) -> Vec<ScanHit> {
    let _ = (text, forbidden);
    todo!("P1-04")
}

pub fn local_forbidden_literals() -> Vec<String> {
    todo!("P1-04")
}
