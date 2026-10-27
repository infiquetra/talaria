//! Key-tab rows. The reducer does not fill them yet.

/// One environment entry the Keys tab may show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvRow {
    /// Variable name.
    pub env_var: String,
    /// Provider slug.
    pub slug: String,
    /// Label drawn on the row.
    pub label: String,
    /// Whether a value is stored.
    pub is_set: bool,
    /// Raw `redacted_value` from the env list. Null is `None`. The view shows only the tail.
    pub tail: Option<String>,
    /// Other providers that use this variable: slug and label.
    pub shared_with: Vec<(String, String)>,
    /// Primary variable for the provider.
    pub primary: bool,
    /// Password variable.
    pub is_password: bool,
    /// Managed by a channel.
    pub channel_managed: bool,
    /// Custom variable.
    pub custom: bool,
    /// Profile slugs that share the variable.
    pub profile_slugs: Vec<String>,
}

/// Result of checking a typed key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCheck {
    /// Whether the key was accepted.
    pub ok: bool,
    /// Whether the provider answered.
    pub reachable: bool,
    /// Provider message.
    pub message: String,
}
