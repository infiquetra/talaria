//! Redaction, URL cleaning and the secret scan.
//!
//! [`Redactor`] is a pure function over one decoded frame plus the map from open server-request
//! ids to their methods. Nothing it returns holds a withheld value. The rules are those of the
//! frame log format (`docs/formats/frame-log.md`): a method deny-set, a key-name net, URLs by
//! position, the ticket subprotocol, and answers to server requests by correlation.

use std::collections::{HashMap, VecDeque};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::record::RecordError;

/// The marker that stands where a value was withheld.
pub const REDACTED: &str = "[redacted]";

/// The percent-encoded marker that stands where part of a URL was withheld.
const URL_MARK: &str = "%5Bredacted%5D";

/// A ticket travels as this subprotocol; any string holding it is withheld.
const TICKET_PREFIX: &str = "hermes-gateway-ticket.";

/// The most open server requests the redactor remembers. The oldest is dropped first.
const OPEN_REQUEST_LIMIT: usize = 1_024;

/// Names withheld by exact name at any depth: free text in a snapshot's `info` block.
const FREE_TEXT: [&str; 3] = ["system_prompt", "credential_hint", "launch_request"];

/// Each method and the keys withheld at any depth below its `params`.
const DENY_SET: &[(&str, &[&str])] = &[
    ("clarify.respond", &["answer"]),
    ("clarify.lock", &["answer"]),
    ("clarify", &["answers"]),
    ("sudo.respond", &["password"]),
    ("secret.respond", &["value"]),
    ("terminal.read.respond", &["text"]),
    ("model.save_key", &["api_key"]),
    ("mcp.servers.set_api_key", &["value"]),
    ("mcp.servers.oauth.callback", &["code", "state"]),
    ("connection.respond", &["env"]),
    ("config.set", &["value"]),
    ("vault.add", &["secret"]),
    ("mcp.servers.add", &["config"]),
    ("plugins.manage", &["values"]),
    ("diagnostics.share_nous", &["extra_files"]),
    ("vault.unlock", &["password"]),
    ("session.create", &["api_key", "client_env"]),
    ("session.resume", &["api_key"]),
    ("file.attach", &["path", "data_url", "name"]),
    ("image.attach", &["path"]),
    (
        "image.attach_bytes",
        &["content_base64", "data", "filename", "ext"],
    ),
    ("image.detach", &["path"]),
    (
        "pdf.attach",
        &["content_base64", "data", "filename", "path"],
    ),
    ("prompt.submit", &["path"]),
    ("prompt.replace_queued", &["path"]),
    ("request.answer", &["result"]),
];

/// The two keys of an `approval` answer that are kept.
const APPROVAL_KEYS: [&str; 2] = ["choice", "all"];

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

/// The ids of open server requests, keyed by the id's JSON text, oldest first.
#[derive(Debug, Default)]
struct OpenRequests {
    methods: HashMap<String, String>,
    order: VecDeque<String>,
}

impl OpenRequests {
    fn remember(&mut self, id: &Value, method: &str) {
        let key = id.to_string();
        if self.methods.contains_key(&key) {
            // A re-delivered request keeps the method it had and becomes the newest.
            self.order.retain(|known| known != &key);
            self.order.push_back(key);
            return;
        }
        self.methods.insert(key.clone(), method.to_owned());
        self.order.push_back(key);
        while self.order.len() > OPEN_REQUEST_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.methods.remove(&oldest);
            }
        }
    }

    fn take(&mut self, id: &Value) -> Option<String> {
        let key = id.to_string();
        let method = self.methods.remove(&key)?;
        self.order.retain(|known| known != &key);
        Some(method)
    }
}

/// Redacts frames and remembers the method of each open server request.
#[derive(Debug)]
pub struct Redactor {
    open: OpenRequests,
    substitutions: Vec<(String, String)>,
}

impl Redactor {
    /// Needles under four characters are refused: they could be neither replaced reliably nor
    /// proved absent.
    pub fn new(substitutions: Vec<(String, String)>) -> Result<Redactor, RecordError> {
        if substitutions
            .iter()
            .any(|(needle, _)| needle.chars().count() < 4)
        {
            return Err(RecordError::NeedleTooShort);
        }
        let mut substitutions = substitutions;
        substitutions.sort_by_key(|(needle, _)| std::cmp::Reverse(needle.chars().count()));
        Ok(Redactor {
            open: OpenRequests::default(),
            substitutions,
        })
    }

    /// Redact a frame from the gateway. Learns server requests and forgets cancelled ones.
    pub fn redact_inbound(&mut self, text: &str) -> Redacted {
        self.redact(text, false)
    }

    /// Redact a frame from Talaria. Answers to server requests are withheld by correlation.
    pub fn redact_outbound(&mut self, text: &str) -> Redacted {
        self.redact(text, true)
    }

    /// Apply the substitutions, longest needle first, to one string.
    pub(crate) fn substitute(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for (needle, replacement) in &self.substitutions {
            if out.contains(needle.as_str()) {
                out = out.replace(needle.as_str(), replacement);
            }
        }
        out
    }

    fn redact(&mut self, text: &str, outbound: bool) -> Redacted {
        let mut frame: Value = match serde_json::from_str(text) {
            Ok(frame) => frame,
            Err(error) => {
                // serde_json's message is a position and an expectation; it never quotes input.
                return Redacted {
                    frame: None,
                    redactions: Vec::new(),
                    parse_error: Some(error.to_string()),
                };
            }
        };
        if !outbound {
            self.learn(&frame);
        }
        let mut walk = Walk {
            redactions: Vec::new(),
            open: &mut self.open,
            substitutions: &self.substitutions,
            outbound,
        };
        match &mut frame {
            Value::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    walk.frame(item, &index.to_string());
                }
            }
            other => walk.frame(other, ""),
        }
        let redactions = walk.redactions;
        let frame = self.substitute_value(frame);
        Redacted {
            frame: Some(frame),
            redactions,
            parse_error: None,
        }
    }

    /// Fill and empty the open-request map from one inbound frame (a batch is its elements).
    fn learn(&mut self, frame: &Value) {
        match frame {
            Value::Array(items) => items.iter().for_each(|item| self.learn_one(item)),
            other => self.learn_one(other),
        }
    }

    fn learn_one(&mut self, frame: &Value) {
        let Some(object) = frame.as_object() else {
            return;
        };
        if let Some((id, method)) = server_request(object) {
            self.open.remember(id, method);
        }
        if let Some(result) = object.get("result").and_then(Value::as_object)
            && let Some(entries) = result.get("open_requests").and_then(Value::as_array)
        {
            for entry in entries.iter().filter_map(Value::as_object) {
                if let Some((id, method)) = server_request(entry) {
                    self.open.remember(id, method);
                }
            }
        }
        // `request.cancel` arrives as an event frame; a top-level method of that name is not
        // what Hermes sends and clears nothing.
        if object.get("method").and_then(Value::as_str) == Some("event")
            && let Some(params) = object.get("params")
            && params.get("type").and_then(Value::as_str) == Some("request.cancel")
            && let Some(id) = params.get("payload").and_then(|payload| payload.get("id"))
        {
            self.open.take(id);
        }
    }

    fn substitute_value(&self, value: Value) -> Value {
        if self.substitutions.is_empty() {
            return value;
        }
        match value {
            Value::String(text) => Value::String(self.substitute(&text)),
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|item| self.substitute_value(item))
                    .collect(),
            ),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, item)| (self.substitute(&key), self.substitute_value(item)))
                    .collect(),
            ),
            other => other,
        }
    }
}

/// `(id, method)` when the object is a server request: an id, a string method, object params,
/// and no result.
fn server_request(object: &Map<String, Value>) -> Option<(&Value, &str)> {
    let id = object.get("id").filter(|id| !id.is_null())?;
    let method = object.get("method")?.as_str()?;
    object.get("params")?.as_object()?;
    if object.contains_key("result") {
        return None;
    }
    Some((id, method))
}

/// The keys a listed method withholds below its `params`.
fn deny_keys(method: &str) -> Option<&'static [&'static str]> {
    DENY_SET
        .iter()
        .find(|(listed, _)| *listed == method)
        .map(|(_, keys)| *keys)
}

/// The rules in force at one point of the walk: each withheld key and the method that named it.
type Rules = Vec<(&'static str, &'static str)>;

struct Walk<'a> {
    redactions: Vec<Redaction>,
    open: &'a mut OpenRequests,
    substitutions: &'a [(String, String)],
    outbound: bool,
}

impl Walk<'_> {
    /// A frame, or one element of a batch: outbound answers are handled here.
    fn frame(&mut self, value: &mut Value, path: &str) {
        if self.outbound
            && let Value::Object(map) = value
            && map.contains_key("id")
            && !map.contains_key("method")
            && (map.contains_key("result") || map.contains_key("error"))
        {
            let method = map.get("id").and_then(|id| self.open.take(id));
            for (key, child) in map.iter_mut() {
                let child_path = self.child_path(path, key);
                if key == "result" {
                    match method.as_deref() {
                        Some("approval") => {
                            self.approval_result(child, &child_path, "server-response:approval");
                        }
                        Some(other) => {
                            self.withhold(child, &child_path, format!("server-response:{other}"));
                        }
                        None => self.withhold(child, &child_path, "server-response:unknown".into()),
                    }
                } else {
                    self.value(child, &child_path, &Vec::new());
                }
            }
            return;
        }
        self.value(value, path, &Vec::new());
    }

    fn value(&mut self, value: &mut Value, path: &str, rules: &Rules) {
        match value {
            Value::Object(map) => self.object(map, path, rules),
            Value::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    let child_path = join(path, &index.to_string());
                    self.value(item, &child_path, rules);
                }
            }
            Value::String(text) => {
                if text.contains(TICKET_PREFIX) {
                    self.withhold(value, path, "ticket".into());
                } else if let Some(clean) = clean_url(text)
                    && clean != *text
                {
                    *text = clean;
                    self.note(path, "url-credential".into());
                }
            }
            _ => {}
        }
    }

    fn object(&mut self, map: &mut Map<String, Value>, path: &str, rules: &Rules) {
        let method = map.get("method").and_then(Value::as_str).map(str::to_owned);
        let listed = method.as_deref().and_then(|method| {
            DENY_SET
                .iter()
                .find(|(name, _)| *name == method)
                .map(|(name, _)| *name)
        });
        // `request.answer` keeps `params.result` reduced to the approval keys when the request
        // it answers was an approval; the entry leaves the map either way.
        let mut answer_approval = false;
        if listed == Some("request.answer")
            && let Some(id) = map.get("params").and_then(|params| params.get("id"))
        {
            let remembered = if self.outbound {
                self.open.take(id)
            } else {
                None
            };
            answer_approval = remembered.as_deref() == Some("approval");
        }
        for (key, child) in map.iter_mut() {
            let child_path = self.child_path(path, key);
            if self.by_name(key, child, &child_path, rules) {
                continue;
            }
            if key == "params"
                && let Some(method) = listed
            {
                let mut inner = rules.clone();
                inner.extend(
                    deny_keys(method)
                        .unwrap_or(&[])
                        .iter()
                        .map(|k| (*k, method)),
                );
                if answer_approval && let Value::Object(params) = child {
                    self.answer_params(params, &child_path, &inner);
                } else {
                    self.value(child, &child_path, &inner);
                }
            } else {
                self.value(child, &child_path, rules);
            }
        }
    }

    /// Withhold `child` when its key is named by a rule in force, is free text, or is
    /// credential shaped. True when it was withheld.
    fn by_name(&mut self, key: &str, child: &mut Value, path: &str, rules: &Rules) -> bool {
        if let Some((_, method)) = rules.iter().rev().find(|(name, _)| *name == key) {
            self.withhold(child, path, format!("deny-set:{method}"));
        } else if FREE_TEXT.contains(&key) {
            self.withhold(child, path, "free-text".into());
        } else if is_suspicious_key(key) {
            self.withhold(child, path, "suspicious-key".into());
        } else {
            return false;
        }
        true
    }

    /// The `params` of a `request.answer` for a remembered approval.
    fn answer_params(&mut self, params: &mut Map<String, Value>, path: &str, rules: &Rules) {
        for (key, child) in params.iter_mut() {
            let child_path = self.child_path(path, key);
            if key == "result" {
                self.approval_result(child, &child_path, "server-response:approval");
            } else if !self.by_name(key, child, &child_path, rules) {
                self.value(child, &child_path, rules);
            }
        }
    }

    /// Keep `choice` and `all`, withhold every other key.
    fn approval_result(&mut self, result: &mut Value, path: &str, reason: &str) {
        let Value::Object(map) = result else {
            self.withhold(result, path, reason.to_owned());
            return;
        };
        for (key, child) in map.iter_mut() {
            let child_path = self.child_path(path, key);
            if APPROVAL_KEYS.contains(&key.as_str()) {
                self.value(child, &child_path, &Vec::new());
            } else {
                self.withhold(child, &child_path, reason.to_owned());
            }
        }
    }

    fn withhold(&mut self, value: &mut Value, path: &str, reason: String) {
        *value = Value::String(REDACTED.to_owned());
        self.note(path, reason);
    }

    fn note(&mut self, path: &str, reason: String) {
        self.redactions.push(Redaction {
            path: path.to_owned(),
            reason,
        });
    }

    /// The path of `key` below `path`, written with the substituted key so it matches the frame.
    fn child_path(&self, path: &str, key: &str) -> String {
        let mut key = key.to_owned();
        for (needle, replacement) in self.substitutions {
            if key.contains(needle.as_str()) {
                key = key.replace(needle.as_str(), replacement);
            }
        }
        join(path, &escape(&key))
    }
}

fn join(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_owned()
    } else {
        format!("{path}.{segment}")
    }
}

/// A `.` or `\` inside a key is written `\.` or `\\`.
fn escape(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for ch in key.chars() {
        if ch == '.' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The key-name net
// ---------------------------------------------------------------------------------------------

static CAMEL_BOUNDARY: LazyLock<Regex> = LazyLock::new(|| compile(r"([a-z0-9])([A-Z])"));

/// The patterns as written, applied to the camel-normalized name.
static CAMEL_NET: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)^(.*[-_])?(pass(word|phrase)|credentials?|authorization|(api|private|public|access|secret|signing)[-_]?keys?|token|ticket)$|secret",
    )
});

/// The same patterns without the prefix group, so they match a squashed name as a suffix.
static SQUASHED_NET: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r"(?i)(pass(word|phrase)|credentials?|authorization|(api|private|public|access|secret|signing)keys?|token|ticket)$|secret",
    )
});

fn compile(pattern: &str) -> Regex {
    // The patterns are constants that the tests exercise; a bad one is a build defect.
    Regex::new(pattern).unwrap_or_else(|_| unreachable!("a constant pattern compiles"))
}

/// True when a key name is credential shaped, in its camel-normalized or squashed form.
pub fn is_suspicious_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let camel = CAMEL_BOUNDARY.replace_all(key, "${1}_${2}").to_lowercase();
    if CAMEL_NET.is_match(&camel) {
        return true;
    }
    let squashed: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase();
    SQUASHED_NET.is_match(&squashed)
}

// ---------------------------------------------------------------------------------------------
// URLs
// ---------------------------------------------------------------------------------------------

/// The cleaned URL, or `None` when `text` is not an absolute URL with a host. The result is
/// made by editing the original string, never by re-serializing a parsed URL.
fn clean_url(text: &str) -> Option<String> {
    let scheme_end = text.find("://")?;
    let parsed = url::Url::parse(text).ok()?;
    parsed.host()?;
    let (head, rest) = text.split_at(scheme_end + 3);
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let mut out = String::with_capacity(text.len());
    out.push_str(head);
    match authority.rfind('@') {
        Some(at) if at > 0 => {
            out.push_str(URL_MARK);
            out.push_str(&authority[at..]);
        }
        _ => out.push_str(authority),
    }
    let (before_fragment, fragment) = match tail.find('#') {
        Some(hash) => (&tail[..hash], Some(&tail[hash + 1..])),
        None => (tail, None),
    };
    match before_fragment.find('?') {
        Some(question) => {
            out.push_str(&before_fragment[..=question]);
            let pairs: Vec<String> = before_fragment[question + 1..]
                .split('&')
                .map(clean_pair)
                .collect();
            out.push_str(&pairs.join("&"));
        }
        None => out.push_str(before_fragment),
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(if fragment.is_empty() { "" } else { URL_MARK });
    }
    Some(out)
}

fn clean_pair(pair: &str) -> String {
    let Some(eq) = pair.find('=') else {
        return pair.to_owned();
    };
    let (key, value) = (&pair[..eq], &pair[eq + 1..]);
    let decoded = percent_decode(key).to_lowercase();
    let credential =
        matches!(decoded.as_str(), "token" | "ticket" | "internal") || is_suspicious_key(&decoded);
    if credential && !value.is_empty() {
        format!("{key}={URL_MARK}")
    } else {
        pair.to_owned()
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The URL with any credential withheld, or `[redacted]` when the input is not an absolute URL.
/// Used for the header endpoint.
pub fn redact_url(url: &str) -> String {
    clean_url(url).unwrap_or_else(|| REDACTED.to_owned())
}

// ---------------------------------------------------------------------------------------------
// The secret scan
// ---------------------------------------------------------------------------------------------

static SCAN_RULES: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("ticket-subprotocol", r"hermes-gateway-ticket\."),
        ("bearer", r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}"),
        ("sk-key", r"\bsk-[A-Za-z0-9_-]{20,}"),
        (
            "github-token",
            r"\bgh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{20,}",
        ),
        ("aws-access-key", r"\bAKIA[0-9A-Z]{16}\b"),
        ("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        (
            "jwt",
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
        ),
        ("private-key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        ("url-userinfo", r"://[^/\s:@]+:[^/\s@]+@"),
    ]
    .into_iter()
    .map(|(rule, pattern)| (rule, compile(pattern)))
    .collect()
});

static SECRET_QUERY: LazyLock<Regex> = LazyLock::new(|| {
    compile(r#"(?i)[?&](token|ticket|internal|api_key|apikey|access_token|password)=([^&#\s"']*)"#)
});

/// The lines of `text`, numbered from 1, without their line endings.
fn numbered_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .enumerate()
        .map(|(index, line)| (index + 1, line))
}

/// Every secret-shaped string, by line and rule. A hit never carries the matched text.
pub fn scan_for_secrets(text: &str) -> Vec<ScanHit> {
    let mut hits = Vec::new();
    for (line, content) in numbered_lines(text) {
        if content.is_empty() {
            continue;
        }
        for (rule, pattern) in SCAN_RULES.iter() {
            if pattern.is_match(content) {
                hits.push(ScanHit { line, rule });
            }
        }
        let live_query = SECRET_QUERY.captures_iter(content).any(|capture| {
            let value = capture.get(2).map_or("", |value| value.as_str());
            !value.is_empty() && value != URL_MARK && value != REDACTED
        });
        if live_query {
            hits.push(ScanHit {
                line,
                rule: "url-secret-query",
            });
        }
        if let Ok(value) = serde_json::from_str::<Value>(content)
            && holds_live_suspicious_key(&value)
        {
            hits.push(ScanHit {
                line,
                rule: "unredacted-suspicious-key",
            });
        }
    }
    hits
}

fn holds_live_suspicious_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, inner)| {
            let live = inner
                .as_str()
                .is_some_and(|text| !text.is_empty() && text != REDACTED);
            (live && is_suspicious_key(key)) || holds_live_suspicious_key(inner)
        }),
        Value::Array(items) => items.iter().any(holds_live_suspicious_key),
        _ => false,
    }
}

/// Every line holding one of `forbidden`, with rule `forbidden-literal`.
pub fn scan_literals(text: &str, forbidden: &[String]) -> Vec<ScanHit> {
    let literals: Vec<&String> = forbidden.iter().filter(|text| !text.is_empty()).collect();
    if literals.is_empty() {
        return Vec::new();
    }
    numbered_lines(text)
        .filter(|(_, content)| {
            literals
                .iter()
                .any(|literal| content.contains(literal.as_str()))
        })
        .map(|(line, _)| ScanHit {
            line,
            rule: "forbidden-literal",
        })
        .collect()
}

/// The home directory (`HOME`), the login name (`USER`, else `LOGNAME`) and the host name of
/// this machine, each left out when unset or under four characters.
pub fn local_forbidden_literals() -> Vec<String> {
    let mut literals = Vec::new();
    let mut add = |value: Option<String>| {
        if let Some(value) = value
            && value.chars().count() >= 4
            && !literals.contains(&value)
        {
            literals.push(value);
        }
    };
    add(std::env::var("HOME").ok());
    add(std::env::var("USER")
        .ok()
        .or_else(|| std::env::var("LOGNAME").ok()));
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    // A host name is often written without its domain, so the short form is checked too.
    let short = host.split('.').next().map(str::to_owned);
    add(Some(host));
    add(short);
    literals
}
