//! Joining the roster, the gateway view and the host topology into rows. Issue #253.

use super::ServeState;

/// What the gateway serves or parked, from a fresh `identify`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GatewayView {
    /// Served profile names.
    pub served: Vec<String>,
    /// Parked `(name, scrubbed reason)` pairs.
    pub parked: Vec<(String, String)>,
}

/// What the host topology call reported. Issue #253.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostTopology {
    /// Names carrying the stopped marker.
    pub stopped: Vec<String>,
    /// Profiles running their own gateway. `None` when the reply held no list.
    pub own_gateway: Option<Vec<String>>,
    /// The gateway mode word, when the reply carried one.
    pub gateway_mode: Option<String>,
    /// Why the gateway serves one profile, scrubbed. `None` when absent.
    pub single_reason: Option<String>,
}

/// One row of the profile roster. It holds no path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RosterEntry {
    /// The profile name.
    pub name: String,
    /// The display name from the profile file, or the name.
    pub display_name: String,
    /// True for the connection's own profile row.
    pub is_default: bool,
    /// The model, set only on the connection's own row.
    pub model: String,
    /// The provider, set only on the connection's own row.
    pub provider: String,
    /// The description, set only on the connection's own row.
    pub description: String,
    /// The skill count, set only on the connection's own row.
    pub skill_count: u32,
}

/// One joined row of the profiles screen. It holds no path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileRow {
    /// The profile name.
    pub name: String,
    /// The display name, or the name.
    pub display_name: String,
    /// True for the connection's own profile row.
    pub is_default: bool,
    /// The model, from the roster's own row or a cached describe.
    pub model: Option<String>,
    /// The provider, from the roster's own row or a cached describe.
    pub provider: Option<String>,
    /// False when the name came from the gateway or the topology, not the roster.
    pub in_roster: bool,
    /// The classified state.
    pub state: ServeState,
}

/// Hermes's profile name normalisation: trim, then lower-case.
#[must_use]
pub fn normalize_profile_name(_input: &str) -> String {
    todo!("profiles stub")
}

/// Classify one name. The first matching rule wins; an absent topology skips
/// its rows. Issue #253.
#[must_use]
pub fn classify(
    _name: &str,
    _gateway: &GatewayView,
    _topology: Option<&HostTopology>,
    _created_at_ms: Option<u64>,
    _now_ms: u64,
) -> ServeState {
    todo!("profiles stub")
}

/// Join every source into sorted rows, each name exactly once. Issue #253.
#[must_use]
pub fn join_roster(
    _roster: Option<&[RosterEntry]>,
    _gateway: &GatewayView,
    _topology: Option<&HostTopology>,
    _created: &[(String, u64)],
    _now_ms: u64,
) -> Vec<ProfileRow> {
    todo!("profiles stub")
}
