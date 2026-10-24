//! The child process environment: default deny.

pub const FORWARDED_TALARIA: [&str; 5] = ["", "", "", "", ""];
pub const DENIED_PROVIDER_KEYS: [&str; 4] = ["", "", "", ""];

/// The environment to forward, sorted by name.
pub fn build_child_env(
    parent: &[(String, String)],
    allowlist: &[String],
    is_credential_name: &dyn Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let _ = (parent, allowlist, is_credential_name);
    todo!("status")
}

/// Scheme, host, port and path only. `None` for a value that is not a URL.
pub fn clean_gateway_url(value: &str) -> Option<String> {
    let _ = value;
    todo!("status")
}
