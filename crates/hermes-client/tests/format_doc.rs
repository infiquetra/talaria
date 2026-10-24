//! Test row 12: `docs/formats/frame-log.md` is the contract for the frame log, and it
//! must describe version 3, every field the recorder writes, every redaction reason and the
//! limits of the redaction.

use std::path::PathBuf;

use regex::Regex;

fn doc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/formats/frame-log.md")
}

/// The text of the section whose heading line is `heading` (any level), up to the next heading of
/// the same or a higher level. Lines inside code fences are never headings. `None` when no such
/// heading exists.
fn section(doc: &str, heading: &str) -> Option<String> {
    let mut in_fence = false;
    let mut found_level: Option<usize> = None;
    let mut body = String::new();
    for line in doc.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        let level = if in_fence {
            0
        } else {
            line.chars().take_while(|ch| *ch == '#').count()
        };
        let is_heading = (1..=6).contains(&level) && line[level..].starts_with(' ');
        match found_level {
            None if is_heading && line[level..].trim() == heading => found_level = Some(level),
            Some(start) if is_heading && level <= start => return Some(body),
            Some(_) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    found_level.map(|_| body)
}

#[test]
fn frame_log_doc_describes_version_3_and_every_field() {
    let path = doc_path();
    let doc = std::fs::read_to_string(&path).expect("docs/formats/frame-log.md exists");
    let mut missing: Vec<String> = Vec::new();

    // Line 5 carries the version.
    match doc.lines().nth(4) {
        Some(line) if line.starts_with("Version: 3") => {}
        Some(line) => missing.push(format!(
            "line 5 must start `Version: 3`, found a line of {} characters starting {:?}",
            line.chars().count(),
            line.chars().take(12).collect::<String>()
        )),
        None => missing.push("line 5 must start `Version: 3`, but the file is shorter".to_owned()),
    }

    // Every header field and every record field, written as code.
    let header_fields = [
        "kind",
        "version",
        "startedAt",
        "endpoint",
        "subprotocols",
        "gateway",
        "codeVersion",
        "codeSha",
        "scenario",
        "expectedStatuses",
    ];
    for field in header_fields {
        if !doc.contains(&format!("`{field}`")) {
            missing.push(format!("header field `{field}` is not described"));
        }
    }
    let record_fields = [
        "kind",
        "seq",
        "tMs",
        "generation",
        "dir",
        "frame",
        "redactions",
        "parseError",
        "count",
    ];
    for field in record_fields {
        if !doc.contains(&format!("`{field}`")) {
            missing.push(format!("record field `{field}` is not described"));
        }
    }
    if !doc.contains("dropped") {
        missing.push("the `dropped` record kind is not described".to_owned());
    }
    if !doc.contains("[redacted]") {
        missing.push("the `[redacted]` marker is not described".to_owned());
    }

    // Every reason a value can be withheld.
    let reasons = [
        "deny-set:<method>",
        "suspicious-key",
        "url-credential",
        "ticket",
        "server-response:<method>",
        "server-response:unknown",
        "free-text",
    ];
    for reason in reasons {
        if !doc.contains(reason) {
            missing.push(format!("reason `{reason}` is not listed"));
        }
    }

    // The redaction path grammar: dots join keys, and a dot or backslash inside a key is escaped.
    if !doc.contains(r"\.") {
        missing.push(r"the path escape `\.` (a dot inside a key) is not described".to_owned());
    }
    if !doc.contains(r"\\") {
        missing
            .push(r"the path escape `\\` (a backslash inside a key) is not described".to_owned());
    }

    // The limit: the path component of a URL is not covered.
    let plain: String = doc
        .chars()
        .filter(|ch| *ch != '*' && *ch != '`')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let limit = Regex::new(r"(?i)\bpaths?\b[^.]{0,200}\bnot covered\b").expect("limit pattern");
    if !limit.is_match(&plain) {
        missing.push("the limit that URL paths are not covered is not stated".to_owned());
    }

    // Two sections.
    if section(&doc, "Earlier versions").is_none() {
        missing.push("there is no 'Earlier versions' section".to_owned());
    }
    match section(&doc, "Reading a recording") {
        None => missing.push("there is no 'Reading a recording' section".to_owned()),
        Some(body) => {
            let commands = body
                .lines()
                .filter(|line| line.trim_start().starts_with("jq "))
                .count();
            if commands < 3 {
                missing.push(format!(
                    "'Reading a recording' must show three jq commands, found {commands}"
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "docs/formats/frame-log.md does not describe version 3 yet:\n- {}",
        missing.join("\n- ")
    );
}
