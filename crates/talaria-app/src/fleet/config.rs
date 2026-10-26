//! Fleet timings. Only `enabled`, the two poll intervals, and the backstop come from the file.

use std::time::Duration;

/// How often the fleet link reads, and how long it waits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetConfig {
    /// Background service of an interactive launch. A one-shot list ignores this.
    pub enabled: bool,
    /// Hot profiles. Default 2 s, allowed 1 s to 10 s.
    pub hot_poll: Duration,
    /// Other profiles. Default 4 s, allowed 2 s to 30 s.
    pub cold_poll: Duration,
    /// Rediscovery. Default 30 s, allowed 10 s to 300 s.
    pub backstop: Duration,
    /// Minimum gap between roster reads of one profile.
    pub roster_gap: Duration,
    /// Minimum gap between liveness reads of one profile.
    pub liveness_gap: Duration,
    /// How long profile-wide notices wait to be folded into one round.
    pub coalesce: Duration,
    /// How long one call waits.
    pub call_timeout: Duration,
    /// Calls in flight across every profile.
    pub max_in_flight: usize,
    /// Of [`Self::max_in_flight`], how many may sit on profiles that are not answering.
    pub suspect_in_flight: usize,
    /// How long an outcome probe may run.
    pub probe_deadline: Duration,
    /// Minimum gap between approval reads of one session.
    pub approvals_gap: Duration,
    /// How long a one-shot list waits to connect.
    pub connect_timeout: Duration,
    /// When set, only this profile is read. The application leaves it unset.
    pub only_profile: Option<String>,
}

impl Default for FleetConfig {
    fn default() -> Self {
        let hot_poll = Duration::from_secs(2);
        Self {
            enabled: true,
            hot_poll,
            cold_poll: Duration::from_secs(4),
            backstop: Duration::from_secs(30),
            roster_gap: Duration::from_secs(3),
            liveness_gap: Duration::from_secs(1).min(hot_poll / 2),
            coalesce: Duration::from_millis(250),
            call_timeout: Duration::from_secs(10),
            max_in_flight: 8,
            suspect_in_flight: 2,
            probe_deadline: Duration::from_secs(3),
            approvals_gap: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(15),
            only_profile: None,
        }
    }
}

impl FleetConfig {
    /// The same values with every duration divided by 20. Counts stay put. Real time.
    #[must_use]
    pub fn scaled_for_tests() -> Self {
        let base = Self::default();
        Self {
            enabled: base.enabled,
            hot_poll: div20(base.hot_poll),
            cold_poll: div20(base.cold_poll),
            backstop: div20(base.backstop),
            roster_gap: div20(base.roster_gap),
            liveness_gap: div20(base.liveness_gap),
            coalesce: div20(base.coalesce),
            call_timeout: div20(base.call_timeout),
            max_in_flight: base.max_in_flight,
            suspect_in_flight: base.suspect_in_flight,
            probe_deadline: div20(base.probe_deadline),
            approvals_gap: div20(base.approvals_gap),
            connect_timeout: div20(base.connect_timeout),
            only_profile: base.only_profile,
        }
    }
}

fn div20(duration: Duration) -> Duration {
    Duration::from_nanos(u64::try_from(duration.as_nanos() / 20).unwrap_or(0))
}
