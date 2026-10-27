//! The exact words the screen shows for each state. Issue #253.

use super::ServeState;

/// The label, the full text and the short next step for one state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    /// The list label, such as `served`.
    pub label: &'static str,
    /// The full explanation the detail shows.
    pub text: String,
    /// The short fix the list and the command show.
    pub next_step: String,
}

/// Explain one state for one profile, character for character. Issue #253.
#[must_use]
pub fn explain(state: &ServeState, name: &str) -> Explanation {
    let (label, text, next_step) = match state {
        ServeState::Served => ("served", format!("Served by this gateway. Start a session on it with: talaria --profile {name}"), format!("talaria --profile {name}")),
        ServeState::Parked { reason } => ("parked", format!("The gateway could not serve this profile: {}. Fix that, then press R to ask the gateway to try again.", reason.chars().take(300).collect::<String>()), "fix the reason, then rescan with R in /profiles".into()),
        ServeState::Stopped => ("stopped", format!("This profile's service was stopped (hermes -p {name} gateway stop). Start it again with: hermes -p {name} gateway start"), format!("hermes -p {name} gateway start")),
        ServeState::OwnGateway => ("own gateway", format!("This profile runs its own gateway, so this gateway does not serve it. To serve it here, stop that gateway (hermes -p {name} gateway stop) and remove gateway.standalone: true from its config if it is set."), format!("hermes -p {name} gateway stop, then rescan with R in /profiles")),
        ServeState::HostSingle { reason } => ("not served", format!("This gateway serves only its own profile: {}", reason.chars().take(300).collect::<String>()), "hermes gateway migrate --multiplex".into()),
        ServeState::Starting => ("starting", "Created moments ago. The gateway usually serves a new profile within 30 seconds.".into(), "wait up to 40 s, then refresh".into()),
        ServeState::NeverDiscovered => ("not discovered", "The gateway never discovered this profile. It may opt out with gateway.standalone: true in its config, or it was created after the gateway's last scan. Press R to ask the gateway to rescan now.".into(), "rescan with R in /profiles".into()),
        ServeState::Unknown => ("unknown", String::new(), String::new()),
    };
    Explanation {
        label,
        text,
        next_step,
    }
}
