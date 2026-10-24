//! The child process environment: default deny.

use std::collections::BTreeMap;

use url::Url;

/// The five `TALARIA_*` names the contract forwards.
pub const FORWARDED_TALARIA: [&str; 5] = [
    "TALARIA_CONFIG_DIR",
    "TALARIA_GATEWAY_URL",
    "TALARIA_LOG_LEVEL",
    "TALARIA_PROFILE",
    "TALARIA_STATUS_INTERVAL",
];

/// Provider key names that never forward, in any letter case.
pub const DENIED_PROVIDER_KEYS: [&str; 4] = [
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "AWS_ACCESS_KEY_ID",
    "GITHUB_PAT",
];

/// The base names every child gets.
const BASE_NAMES: [&str; 5] = ["PATH", "HOME", "SHELL", "TERM", "TMPDIR"];
/// The locale name that is forwarded as a whole name.
const LANG: &str = "LANG";
/// The prefix of the locale names that are forwarded.
const LOCALE_PREFIX: &str = "LC_";
/// The prefix of the Talaria names. Only the five above are ever candidates.
const TALARIA_PREFIX: &str = "TALARIA_";
/// The one name whose value is cleaned.
const GATEWAY_URL: &str = "TALARIA_GATEWAY_URL";

/// The environment to forward, sorted by name.
///
/// The rule is default deny: a name is forwarded only when the contract lists it, the parent
/// holds it, the fixed deny list does not name it, and the caller's credential check does not
/// flag it. The value of every forwarded name passes the by-name cleaner, so an allow-list
/// entry cannot bring a raw value back.
pub fn build_child_env(
    parent: &[(String, String)],
    allowlist: &[String],
    is_credential_name: &dyn Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let mut candidates: Vec<&str> = Vec::new();
    candidates.extend(BASE_NAMES);
    candidates.push(LANG);
    for (name, _) in parent {
        if name.starts_with(LOCALE_PREFIX) {
            candidates.push(name.as_str());
        }
    }
    candidates.extend(FORWARDED_TALARIA);
    for name in allowlist {
        // A Talaria name outside the five is never a candidate, allow-list or not.
        if name.starts_with(TALARIA_PREFIX) && !FORWARDED_TALARIA.contains(&name.as_str()) {
            continue;
        }
        candidates.push(name.as_str());
    }

    let mut forwarded: BTreeMap<String, String> = BTreeMap::new();
    for name in candidates {
        if !valid_name(name) || is_denied(name) || is_credential_name(name) {
            continue;
        }
        let Some(value) = parent
            .iter()
            .find(|(parent_name, _)| parent_name == name)
            .map(|(_, value)| value.as_str())
        else {
            continue;
        };
        let Some(value) = clean_value(name, value) else {
            continue;
        };
        forwarded.insert(name.to_string(), value);
    }
    forwarded.into_iter().collect()
}

/// A name the operating system can hold.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('=') && !name.contains('\0')
}

/// One of the four provider key names, in any letter case.
fn is_denied(name: &str) -> bool {
    DENIED_PROVIDER_KEYS
        .iter()
        .any(|denied| name.eq_ignore_ascii_case(denied))
}

/// The value a name forwards, cleaned by name.
fn clean_value(name: &str, value: &str) -> Option<String> {
    if name == GATEWAY_URL {
        clean_gateway_url(value)
    } else {
        Some(value.to_string())
    }
}

/// The gateway address without its query, fragment or user information.
///
/// A value that is not a URL is not forwarded at all.
pub fn clean_gateway_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value).ok()?;
    let host = parsed.host_str()?;
    let mut out = String::new();
    out.push_str(parsed.scheme());
    out.push_str("://");
    out.push_str(host);
    if let Some(port) = parsed.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(parsed.path());
    Some(out)
}
