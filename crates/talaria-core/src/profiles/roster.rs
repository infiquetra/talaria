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
pub fn normalize_profile_name(input: &str) -> String {
    input.trim().to_lowercase()
}

/// Classify one name. The first matching rule wins; an absent topology skips
/// its rows. Issue #253.
#[must_use]
pub fn classify(
    name: &str,
    gateway: &GatewayView,
    topology: Option<&HostTopology>,
    created_at_ms: Option<u64>,
    now_ms: u64,
) -> ServeState {
    let name = normalize_profile_name(name);
    let matches = |value: &String| normalize_profile_name(value) == name;
    if gateway.served.iter().any(matches) {
        return ServeState::Served;
    }
    if let Some((_, reason)) = gateway.parked.iter().find(|(value, _)| matches(value)) {
        return ServeState::Parked {
            reason: reason.clone(),
        };
    }
    if let Some(topology) = topology {
        if topology.stopped.iter().any(matches) {
            return ServeState::Stopped;
        }
        if topology
            .own_gateway
            .as_ref()
            .is_some_and(|names| names.iter().any(matches))
        {
            return ServeState::OwnGateway;
        }
        if topology
            .gateway_mode
            .as_deref()
            .is_some_and(|mode| mode != "multiplex")
            && let Some(reason) = topology
                .single_reason
                .as_ref()
                .filter(|reason| !reason.trim().is_empty())
        {
            return ServeState::HostSingle {
                reason: reason.clone(),
            };
        }
    }
    if created_at_ms.is_some_and(|created| created <= now_ms && now_ms - created < 40_000) {
        ServeState::Starting
    } else {
        ServeState::NeverDiscovered
    }
}

/// Join every source into sorted rows, each name exactly once. Issue #253.
#[must_use]
pub fn join_roster(
    roster: Option<&[RosterEntry]>,
    gateway: &GatewayView,
    topology: Option<&HostTopology>,
    created: &[(String, u64)],
    now_ms: u64,
) -> Vec<ProfileRow> {
    let mut names = std::collections::BTreeSet::new();
    names.extend(
        roster
            .into_iter()
            .flatten()
            .map(|row| normalize_profile_name(&row.name)),
    );
    names.extend(
        gateway
            .served
            .iter()
            .map(|name| normalize_profile_name(name)),
    );
    names.extend(
        gateway
            .parked
            .iter()
            .map(|(name, _)| normalize_profile_name(name)),
    );
    if let Some(topology) = topology {
        names.extend(
            topology
                .stopped
                .iter()
                .map(|name| normalize_profile_name(name)),
        );
        names.extend(
            topology
                .own_gateway
                .iter()
                .flatten()
                .map(|name| normalize_profile_name(name)),
        );
    }
    names.extend(
        created
            .iter()
            .filter(|(_, at)| *at <= now_ms && now_ms - *at < 40_000)
            .map(|(name, _)| normalize_profile_name(name)),
    );
    let mut rows: Vec<_> = names
        .into_iter()
        .map(|name| {
            let entry = roster
                .into_iter()
                .flatten()
                .find(|row| normalize_profile_name(&row.name) == name);
            let created_at = created
                .iter()
                .find(|(value, _)| normalize_profile_name(value) == name)
                .map(|(_, at)| *at);
            ProfileRow {
                display_name: entry.map_or_else(
                    || name.clone(),
                    |entry| {
                        if entry.display_name.is_empty() {
                            name.clone()
                        } else {
                            entry.display_name.clone()
                        }
                    },
                ),
                is_default: entry.is_some_and(|entry| entry.is_default),
                model: entry
                    .filter(|entry| !entry.model.is_empty())
                    .map(|entry| entry.model.clone()),
                provider: entry
                    .filter(|entry| !entry.provider.is_empty())
                    .map(|entry| entry.provider.clone()),
                in_roster: entry.is_some(),
                state: classify(&name, gateway, topology, created_at, now_ms),
                name,
            }
        })
        .collect();
    rows.sort_by_key(|row| {
        (
            row.name != "default",
            row.state != ServeState::Served,
            row.name.clone(),
        )
    });
    rows
}
