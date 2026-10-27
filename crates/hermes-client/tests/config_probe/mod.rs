//! Helpers shared by the configuration-surface matrix checks and the live probe.
//!
//! The probe records shapes, never values: status codes, refusal reasons, key
//! names, counts and classes. A reply body is held in memory only long enough
//! to compute one of those.

#![allow(dead_code)]

use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::time::Duration;

use hermes_client::{Endpoint, ProfileId};
use serde_json::{Value, json};

/// A recorded key name is 1 to 64 characters, holds no path or whitespace
/// character, and is not a run of 40 or more token characters.
pub fn key_name_is_safe(name: &str) -> bool {
    let count = name.chars().count();
    if count == 0 || count > 64 {
        return false;
    }
    if name
        .chars()
        .any(|c| c == '/' || c == '\\' || c == '~' || c.is_whitespace())
    {
        return false;
    }
    longest_token_run(name) < 40
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn longest_token_run(text: &str) -> usize {
    let (mut best, mut run) = (0, 0);
    for c in text.chars() {
        if is_token_char(c) {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

// Hermes's `_ENV_CONFIG_KEYS`, name for name, at the pinned release.
const ENV_CONFIG_KEYS: &[&str] = &[
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "VOICE_TOOLS_OPENAI_KEY",
    "EXA_API_KEY",
    "PARALLEL_API_KEY",
    "FIRECRAWL_API_KEY",
    "FIRECRAWL_API_URL",
    "FIRECRAWL_GATEWAY_URL",
    "TOOL_GATEWAY_URL",
    "CONNECTOR_GATEWAY_URL",
    "TOOL_GATEWAY_DOMAIN",
    "TOOL_GATEWAY_SCHEME",
    "TOOL_GATEWAY_USER_TOKEN",
    "TAVILY_API_KEY",
    "PERPLEXITY_API_KEY",
    "API_SERVER_KEY",
    "BROWSERBASE_API_KEY",
    "BROWSERBASE_PROJECT_ID",
    "BROWSER_USE_API_KEY",
    "FAL_KEY",
    "TELEGRAM_BOT_TOKEN",
    "DISCORD_BOT_TOKEN",
    "TERMINAL_SSH_HOST",
    "TERMINAL_SSH_USER",
    "TERMINAL_SSH_KEY",
    "SUDO_PASSWORD",
    "SLACK_BOT_TOKEN",
    "SLACK_APP_TOKEN",
    "GITHUB_TOKEN",
    "HONCHO_API_KEY",
];

const NON_SECRET_ENV_SUFFIXES: &[&str] = &["_url", "_host", "_user", "_id", "_domain", "_scheme"];

const SECRET_LEAVES: &[&str] = &[
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "secret",
    "client_secret",
    "password",
    "passwd",
    "authorization",
    "private_key",
    "bearer",
    "jwt",
];

const SECRET_SUFFIXES: &[&str] = &[
    "_api_key",
    "_token",
    "_secret",
    "_password",
    "_key",
    "_access_key",
];

/// A port of Hermes's `_is_secret_config_key` with `_is_env_config_key`, for
/// one key segment (never a dotted path).
pub fn is_credential_key(name: &str) -> bool {
    let leaf = name.to_lowercase().replace('-', "_");
    if is_env_config_key(name) {
        return !NON_SECRET_ENV_SUFFIXES.iter().any(|s| leaf.ends_with(s));
    }
    SECRET_LEAVES.contains(&leaf.as_str()) || SECRET_SUFFIXES.iter().any(|s| leaf.ends_with(s))
}

fn is_env_config_key(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    let upper = name.to_uppercase();
    ENV_CONFIG_KEYS.contains(&upper.as_str())
        || upper.ends_with("_API_KEY")
        || upper.ends_with("_TOKEN")
        || upper.ends_with("_SECRET")
        || upper.starts_with("TERMINAL_SSH")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretClass {
    Empty,
    EnvReference,
    Masked,
    Plain,
}

impl SecretClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::EnvReference => "env-reference",
            Self::Masked => "masked",
            Self::Plain => "plain",
        }
    }
}

/// Sorts one credential-shaped leaf. `None` is JSON null.
pub fn classify(value: Option<&str>) -> SecretClass {
    let Some(text) = value else {
        return SecretClass::Empty;
    };
    if text.is_empty() {
        return SecretClass::Empty;
    }
    if is_env_reference(text) {
        return SecretClass::EnvReference;
    }
    if text.starts_with("«redacted:") || text == "***" || is_short_mask(text) {
        return SecretClass::Masked;
    }
    SecretClass::Plain
}

// `${NAME}` where NAME is `[A-Za-z_][A-Za-z0-9_]*`.
fn is_env_reference(text: &str) -> bool {
    let Some(inner) = text.strip_prefix("${").and_then(|t| t.strip_suffix('}')) else {
        return false;
    };
    let mut chars = inner.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// Four characters, `...`, four characters.
fn is_short_mask(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.len() == 11
        && chars[4..7] == ['.', '.', '.']
        && !chars[..4].contains(&'\n')
        && !chars[7..].contains(&'\n')
}

/// Counts of each class, in the order empty, env-reference, masked, plain.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SecretCounts {
    pub empty: usize,
    pub env_reference: usize,
    pub masked: usize,
    pub plain: usize,
}

impl SecretCounts {
    pub fn total(&self) -> usize {
        self.empty + self.env_reference + self.masked + self.plain
    }

    pub fn note(&self) -> String {
        format!(
            "secret-leaves: {}; empty {}, env-reference {}, masked {}, plain {}",
            self.total(),
            self.empty,
            self.env_reference,
            self.masked,
            self.plain
        )
    }

    fn add(&mut self, class: SecretClass) {
        match class {
            SecretClass::Empty => self.empty += 1,
            SecretClass::EnvReference => self.env_reference += 1,
            SecretClass::Masked => self.masked += 1,
            SecretClass::Plain => self.plain += 1,
        }
    }
}

/// Walks a configuration the way Hermes's `redact_config_value` does and
/// classifies every credential-shaped leaf holding a string or null.
pub fn count_secret_leaves(value: &Value) -> SecretCounts {
    let mut counts = SecretCounts::default();
    walk_secrets(value, &mut counts);
    counts
}

fn walk_secrets(value: &Value, counts: &mut SecretCounts) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match child {
                    Value::Object(_) | Value::Array(_) => walk_secrets(child, counts),
                    Value::String(text) if is_credential_key(key) => {
                        counts.add(classify(Some(text)));
                    }
                    Value::Null if is_credential_key(key) => counts.add(classify(None)),
                    _ => {}
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_secrets(item, counts);
            }
        }
        _ => {}
    }
}

/// Sorted top-level key names of an object, each passed through the key-name
/// guard. The flag is true when a name was replaced.
pub fn safe_keys(value: &Value) -> (Vec<String>, bool) {
    let Value::Object(map) = value else {
        return (Vec::new(), false);
    };
    let mut dropped = false;
    let mut keys: Vec<String> = map
        .keys()
        .map(|key| {
            if key_name_is_safe(key) {
                key.clone()
            } else {
                dropped = true;
                "<unsafe-key>".to_string()
            }
        })
        .collect();
    keys.sort();
    (keys, dropped)
}

pub struct HttpReply {
    pub status: u16,
    /// Header names are lower-cased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpReply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Parses a whole HTTP/1.1 response read until the server closed the stream.
pub fn parse_response(raw: &[u8]) -> Result<HttpReply, String> {
    let split = find(raw, b"\r\n\r\n").ok_or("no end of headers")?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| "headers are not text")?;
    let rest = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("no status line")?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err("not an HTTP/1 status line".to_string());
    }
    let status = parts
        .next()
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or("no status code")?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("malformed header line")?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    let reply = HttpReply {
        status,
        headers,
        body: Vec::new(),
    };
    let chunked = reply
        .header("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    let body = if chunked {
        decode_chunked(rest)?
    } else if let Some(length) = reply.header("content-length") {
        let length: usize = length.parse().map_err(|_| "bad content-length")?;
        if rest.len() < length {
            return Err("truncated body".to_string());
        }
        rest[..length].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(HttpReply { body, ..reply })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn decode_chunked(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let end = find(rest, b"\r\n").ok_or("truncated chunk size")?;
        let line = std::str::from_utf8(&rest[..end]).map_err(|_| "chunk size is not text")?;
        let size_text = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| "bad chunk size")?;
        rest = &rest[end + 2..];
        if size == 0 {
            return Ok(body);
        }
        if rest.len() < size + 2 {
            return Err("truncated chunk".to_string());
        }
        body.extend_from_slice(&rest[..size]);
        if &rest[size..size + 2] != b"\r\n" {
            return Err("chunk not followed by CRLF".to_string());
        }
        rest = &rest[size + 2..];
    }
}

/// Builds one request with exactly the headers the probe sends.
pub fn build_request(
    verb: &str,
    target: &str,
    authority: &str,
    ticket: Option<&str>,
    origin: Option<&str>,
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut head = format!(
        "{verb} {target} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if let Some(ticket) = ticket {
        head.push_str(&format!("x-hermes-gateway-ticket: {ticket}\r\n"));
    }
    if let Some(origin) = origin {
        head.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(body) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");
    let mut bytes = head.into_bytes();
    if let Some(body) = body {
        bytes.extend_from_slice(body);
    }
    bytes
}

/// The `host:port` part of a loopback `http` origin.
pub fn authority_of(api_origin: &str) -> Result<String, String> {
    let rest = api_origin
        .strip_prefix("http://")
        .ok_or("the api origin is not plain http")?;
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err("the api origin has no host".to_string());
    }
    Ok(authority.to_string())
}

/// What one request produced. `Err` is a transport failure, never a status.
pub enum Sent {
    Reply(HttpReply),
    TimedOut,
    Failed(String),
}

/// Sends one request on its own connection and reads until the server closes it.
pub async fn send(authority: &str, request: &[u8]) -> Sent {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let exchange = async {
        let mut stream = tokio::net::TcpStream::connect(authority)
            .await
            .map_err(|e| format!("connect failed: {}", e.kind()))?;
        stream
            .write_all(request)
            .await
            .map_err(|e| format!("write failed: {}", e.kind()))?;
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .await
            .map_err(|e| format!("read failed: {}", e.kind()))?;
        parse_response(&raw)
    };
    match tokio::time::timeout(Duration::from_secs(30), exchange).await {
        Ok(Ok(reply)) => Sent::Reply(reply),
        Ok(Err(reason)) => Sent::Failed(reason),
        Err(_) => Sent::TimedOut,
    }
}

/// The refusal reason recorded for an HTTP error body.
pub fn http_reason(body: &Value) -> Option<&'static str> {
    match body.get("detail").and_then(Value::as_str) {
        Some("Unauthorized") => Some("Unauthorized"),
        Some("profile_scope_mismatch") => Some("profile_scope_mismatch"),
        Some(text) if text.starts_with("No such API endpoint") => Some("no_such_api_endpoint"),
        _ => None,
    }
}

/// Write probes run only with the opt-in set to `1`, and only on `fixture`.
pub fn write_probe_allowed(profile: &str, writes_env: Option<&str>) -> Result<(), String> {
    if profile != "fixture" {
        return Err("write probes run only on the fixture profile".to_string());
    }
    if writes_env != Some("1") {
        return Err("write opt-in not set".to_string());
    }
    Ok(())
}

// The keys canonical `config.set` accepts and saves at the pinned release.
const ACCEPTED_SET_KEYS: &[&str] = &[
    "reasoning",
    "busy",
    "verbose",
    "yolo",
    "show_reasoning",
    "service_tier",
    "fast",
    "personality",
    "approvals.mode",
    "approval_mode",
    "display.message_reactions",
    "display.in_app_tips",
    "display.in_app_tours",
    "voice.voice_chat_mode",
    "density",
    "battery",
    "timestamps",
    "statusbar",
    "mouse",
    "theme",
    "indicator",
    "details_mode",
    "focus",
    "skin",
];

/// `false` for a key canonical `config.set` would save, unless writes are allowed.
pub fn config_set_allowed(key: &str, write_allowed: bool) -> bool {
    if write_allowed {
        return true;
    }
    let saves = ACCEPTED_SET_KEYS.contains(&key) || key.starts_with("details_mode.");
    !saves
}

/// `talaria-probe-` and eight hexadecimal digits, from the clock and the process id.
pub fn probe_profile_name(salt: u32) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or_default();
    let mixed = nanos ^ std::process::id().rotate_left(16) ^ salt.wrapping_mul(0x9e37_79b9);
    format!("talaria-probe-{mixed:08x}")
}

/// The median of three or more timings, in whole milliseconds.
pub fn median_ms(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or_default()
}

/// The first privacy problem in `text`, as `line <n>: <rule>`. It never quotes the text.
pub fn privacy_problem(text: &str) -> Option<String> {
    let names: Vec<String> = ["USER", "LOGNAME"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .filter(|name| name.chars().count() >= 3 && !GENERIC_ACCOUNTS.contains(&name.as_str()))
        .collect();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if let Some(rule) = line_problem(line, &names) {
            return Some(format!("line {number}: {rule}"));
        }
    }
    None
}

const GENERIC_ACCOUNTS: &[&str] = &[
    "root", "runner", "user", "admin", "test", "dev", "build", "ubuntu",
];

fn is_separator(c: char) -> bool {
    c.is_whitespace() || "\"'`()<>[]{},;".contains(c)
}

fn line_problem(line: &str, names: &[String]) -> Option<&'static str> {
    for token in line.split(is_separator).filter(|t| !t.is_empty()) {
        let mut chars = token.chars();
        if chars.next() == Some('/') {
            let second = chars.next();
            let pathlike = second
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '~');
            if pathlike && !token.starts_with("/api/") {
                return Some("absolute path");
            }
        }
        if token.contains(['~', '/'].iter().collect::<String>().as_str()) {
            return Some("home-relative path");
        }
        if token
            .as_bytes()
            .windows(3)
            .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && w[2] == b'\\')
        {
            return Some("drive-letter path");
        }
    }
    if has_long_run(line) {
        return Some("long token run");
    }
    for name in names {
        if contains_word(line, name) {
            return Some("user name");
        }
    }
    if let Some(at) = line.find("hermes-gateway-ticket") {
        let after = &line[at + "hermes-gateway-ticket".len()..];
        let mut chars = after.chars();
        if chars.next() == Some('.') && chars.next().is_some() {
            return Some("ticket subprotocol");
        }
    }
    None
}

fn has_long_run(line: &str) -> bool {
    let mut run = String::new();
    let check =
        |run: &str| run.chars().count() >= 40 && !is_commit_id(run) && !is_snake_identifier(run);
    for c in line.chars() {
        if is_token_char(c) {
            run.push(c);
        } else {
            if check(&run) {
                return true;
            }
            run.clear();
        }
    }
    check(&run)
}

fn is_commit_id(run: &str) -> bool {
    run.len() == 40
        && run
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn is_snake_identifier(run: &str) -> bool {
    run.contains('_')
        && run
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn contains_word(line: &str, word: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut start = 0;
    while let Some(found) = line[start..].find(word) {
        let at = start + found;
        let before = line[..at].chars().next_back();
        let after = line[at + word.len()..].chars().next();
        if !before.is_some_and(is_word) && !after.is_some_and(is_word) {
            return true;
        }
        start = at + word.len();
    }
    false
}

/// Mints a `native-http` ticket on the control socket and returns its secret.
///
/// The library keeps a ticket's secret private to its WebSocket connect, so the
/// probe sends the same `session-ticket` request itself. It checks the home and
/// the socket the way discovery does, and that the peer is this user. The
/// secret is only ever placed in a request header; it is never printed.
pub async fn mint_native_http(
    home: &Path,
    endpoint: &Endpoint,
    profile: &ProfileId,
) -> Result<String, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let home = home
        .canonicalize()
        .map_err(|_| "home could not be resolved")?;
    let me = rustix::process::getuid().as_raw();
    let home_meta = std::fs::symlink_metadata(&home).map_err(|_| "home could not be read")?;
    if !home_meta.is_dir() || home_meta.uid() != me || home_meta.mode() & 0o022 != 0 {
        return Err("the home is not a private folder of this user".to_string());
    }
    let socket = home.join("gateway.sock");
    let meta = std::fs::symlink_metadata(&socket)
        .map_err(|_| "the probe mints only through a socket directly in the home")?;
    if !meta.file_type().is_socket() || meta.uid() != me || meta.mode() & 0o077 != 0 {
        return Err("the control socket is not a private socket of this user".to_string());
    }
    let exchange = async {
        let mut stream = tokio::net::UnixStream::connect(&socket)
            .await
            .map_err(|e| format!("control connect failed: {}", e.kind()))?;
        let peer = stream
            .peer_cred()
            .map_err(|e| format!("peer check failed: {}", e.kind()))?;
        if peer.uid() != me {
            return Err("the control socket peer is another user".to_string());
        }
        let request = json!({"protocol": 1, "id": 2, "verb": "session-ticket", "params": {
            "profile_id": profile.as_ref(),
            "instance_id": endpoint.instance_id(),
            "purpose": "native-http",
            "scope": "profile",
        }});
        let mut line = serde_json::to_vec(&request).map_err(|_| "request did not encode")?;
        line.push(b'\n');
        stream
            .write_all(&line)
            .await
            .map_err(|e| format!("control write failed: {}", e.kind()))?;
        let mut reader = tokio::io::BufReader::new(stream);
        let mut reply = Vec::new();
        reader
            .read_until(b'\n', &mut reply)
            .await
            .map_err(|e| format!("control read failed: {}", e.kind()))?;
        let reply: Value =
            serde_json::from_slice(&reply).map_err(|_| "control reply did not parse")?;
        Ok::<Value, String>(reply)
    };
    let reply = tokio::time::timeout(Duration::from_secs(5), exchange)
        .await
        .map_err(|_| "control socket timed out")??;
    if reply["ok"] == false {
        return Err("the gateway refused the ticket request".to_string());
    }
    let result = &reply["result"];
    if result["runtime_protocol"].as_u64() != Some(1)
        || result["instance_id"].as_str() != Some(endpoint.instance_id())
        || result["profile_id"].as_str() != Some(profile.as_ref())
    {
        return Err("the gateway identity changed".to_string());
    }
    let ticket = result["ticket"].as_str().ok_or("the reply had no ticket")?;
    let well_formed = !ticket.is_empty()
        && ticket.len() <= 256
        && ticket
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !well_formed {
        return Err("the ticket was malformed".to_string());
    }
    Ok(ticket.to_string())
}
