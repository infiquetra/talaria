//! Fleet link: timings, the host dial, and the service that owns the registry.

mod config;
mod external;
mod host_dial;
mod schedule;
mod service;

use talaria_core::fleet::{RowKey, SubscribedView};

pub use config::FleetConfig;
pub use external::{ExternalCommand, ExternalHandle, ExternalResult};
pub use host_dial::HostDial;
pub use service::{
    FleetHandle, FleetLinkState, FleetListError, FleetNotice, FleetReporter, FleetService,
    FleetSnapshot, check_fleet_profile, list_once, list_once_with_herdr,
};

/// Whether an interactive launch attaches after its first frame.
///
/// Print mode never attaches, so it mints no host-scoped ticket. A disabled setting never
/// attaches.
#[must_use]
pub fn attaches_after_first_frame(print_mode: bool, enabled: bool) -> bool {
    !print_mode && enabled
}

/// One report the application makes about its foreground session.
#[derive(Clone, Debug, PartialEq)]
pub enum ForegroundReport {
    /// The foreground session changed.
    Foreground(Option<RowKey>),
    /// The previous foreground session is no longer reported.
    Unsubscribed(RowKey),
    /// The foreground session's view changed.
    Subscribed(RowKey, SubscribedView),
}

/// What to tell the fleet service, given what was last reported and the foreground now: on a
/// switch, the new foreground and the old row's release; then the view, only when it changed.
#[must_use]
pub fn foreground_reports(
    reported: Option<&(RowKey, SubscribedView)>,
    current: Option<&(RowKey, SubscribedView)>,
) -> Vec<ForegroundReport> {
    let mut reports = Vec::new();
    let reported_key = reported.map(|(key, _)| key);
    let current_key = current.map(|(key, _)| key);
    let switched = reported_key != current_key;
    if switched {
        reports.push(ForegroundReport::Foreground(current_key.cloned()));
        if let Some(old) = reported_key {
            reports.push(ForegroundReport::Unsubscribed(old.clone()));
        }
    }
    if let Some((key, view)) = current
        && (switched || reported.map(|(_, sent)| sent) != Some(view))
    {
        reports.push(ForegroundReport::Subscribed(key.clone(), view.clone()));
    }
    reports
}

impl FleetHandle {
    /// Apply one foreground report.
    pub fn report(&self, report: ForegroundReport) {
        match report {
            ForegroundReport::Foreground(key) => self.set_foreground(key),
            ForegroundReport::Unsubscribed(key) => self.observe_unsubscribed(key),
            ForegroundReport::Subscribed(key, view) => self.observe_subscribed(key, view),
        }
    }
}
