//! Candidates in one pasted line. Nothing here reads the file system.

use std::path::{Path, PathBuf};

/// One path to try, and the text that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropCandidate {
    /// Absolute, or relative when the paste was relative.
    pub path: PathBuf,
    /// Text after the path. Empty when the path was the whole line.
    pub remainder: String,
}

/// Candidates in try order. Relative paths stay relative.
#[must_use]
pub fn drop_candidates(text: &str, home: &Path) -> Vec<DropCandidate> {
    let line = text.trim();
    if line.is_empty() || line.contains(['\n', '\r']) {
        return Vec::new();
    }
    if line.starts_with(['\'', '"']) {
        let quote = line.as_bytes()[0] as char;
        let Some(end) = line[1..].find(quote).map(|end| end + 1) else {
            return Vec::new();
        };
        let path = &line[1..end];
        if !path.starts_with(['/', '~']) {
            return Vec::new();
        }
        return vec![DropCandidate {
            path: expand_home(path, home),
            remainder: line[end + 1..].trim_start().to_string(),
        }];
    }
    if !(line.starts_with("file://")
        || line.starts_with("~/")
        || line.starts_with("./")
        || line.starts_with("../")
        || (line.starts_with('/') && line[1..].contains(['/', '.'])))
    {
        return Vec::new();
    }
    let mut decoded = String::new();
    let mut cuts = Vec::new();
    let mut separator = false;
    let mut chars = line.char_indices();
    while let Some((at, ch)) = chars.next() {
        if ch == '\\' {
            separator = false;
            if let Some((_, escaped)) = chars.next() {
                decoded.push(escaped);
            } else {
                decoded.push(ch);
            }
        } else {
            if ch.is_whitespace() && !separator {
                cuts.push((decoded.len(), at));
            }
            separator = ch.is_whitespace();
            decoded.push(ch);
        }
    }
    let mut candidates = vec![DropCandidate {
        path: decode_path(&decoded, home),
        remainder: String::new(),
    }];
    for (end, source_at) in cuts.into_iter().rev() {
        candidates.push(DropCandidate {
            path: decode_path(&decoded[..end], home),
            remainder: line[source_at..].trim_start().to_string(),
        });
    }
    candidates
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(relative) = path.strip_prefix("~/") {
        home.join(relative)
    } else {
        PathBuf::from(path)
    }
}

fn decode_path(path: &str, home: &Path) -> PathBuf {
    let Some(uri) = path.strip_prefix("file://") else {
        return expand_home(path, home);
    };
    let mut bytes = Vec::with_capacity(uri.len());
    let mut at = 0;
    while at < uri.len() {
        if uri.as_bytes()[at] == b'%'
            && let Some(hex) = uri
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            bytes.push(hex);
            at += 3;
            continue;
        }
        bytes.push(uri.as_bytes()[at]);
        at += 1;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(&bytes).as_ref())
    }
}
