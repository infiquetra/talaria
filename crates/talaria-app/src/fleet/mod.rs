//! Fleet link: timings, the host dial, and the service that owns the registry.

mod config;
mod host_dial;
mod service;

pub use config::FleetConfig;
pub use host_dial::HostDial;
pub use service::{
    FleetHandle, FleetLinkState, FleetListError, FleetNotice, FleetService, FleetSnapshot,
    list_once,
};

/// Whether an interactive launch attaches after its first frame.
///
/// Print mode never attaches. A disabled setting never attaches. The stub attaches neither.
#[must_use]
pub fn attaches_after_first_frame(print_mode: bool, enabled: bool) -> bool {
    let _ = (print_mode, enabled);
    false
}
