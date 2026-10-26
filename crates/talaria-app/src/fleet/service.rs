//! The fleet task. The stub returns an empty connecting snapshot and sends nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use talaria_core::attention::AlertKind;
use talaria_core::fleet::{
    FleetCounts, FleetRow, NeedId, NeedItem, ProfileStatus, RowKey, SubscribedView,
};

use super::config::FleetConfig;
use super::host_dial::HostDial;
use crate::side_connection::SideScope;

/// What the link is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetLinkState {
    /// Before the first connection.
    Connecting,
    /// The link is up.
    Connected,
    /// The link dropped and will try again.
    Reconnecting {
        /// What a person can read.
        reason: String,
    },
    /// The link stopped trying.
    Failed {
        /// What a person can read.
        reason: String,
    },
}

/// One published view of the fleet.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetSnapshot {
    /// Raised by one each time a snapshot is published.
    pub version: u64,
    /// Host scope, or the one profile in view.
    pub scope: SideScope,
    /// Link state.
    pub link: FleetLinkState,
    /// Profiles in served order, then parked.
    pub profiles: Vec<(String, ProfileStatus)>,
    /// Rows in canonical order.
    pub rows: Vec<FleetRow>,
    /// Queue items, ascending sequence.
    pub queue: Vec<NeedItem>,
    /// Background counts.
    pub counts: FleetCounts,
    /// Distinct diagnostic texts, oldest first.
    pub diagnostics: Vec<String>,
    /// Seconds since 1970 when this snapshot was published.
    pub as_of: f64,
}

/// A published snapshot, or an attention the foreground owner should hear.
#[derive(Clone, Debug)]
pub enum FleetNotice {
    /// The snapshot changed.
    Changed(Arc<FleetSnapshot>),
    /// An attention effect.
    Attention {
        /// Which alert.
        kind: AlertKind,
        /// Row.
        key: RowKey,
        /// Queue item, when the alert is a need opening.
        need: Option<NeedId>,
        /// The row is foreground.
        foreground: bool,
    },
}

/// Why a one-shot list stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetListError {
    /// No gateway is running for this Hermes home.
    NoGateway,
    /// The ticket, the socket, or the handshake failed.
    Attach(String),
    /// `--profile` names nothing being served.
    UnknownProfile {
        /// How many profiles are served.
        served: usize,
    },
    /// `--profile` names a parked profile.
    ParkedProfile {
        /// The gateway's reason.
        reason: String,
    },
}

/// The running fleet task.
pub struct FleetService;

impl FleetService {
    /// Return at once. No frame is sent and `notify` is not called.
    #[must_use]
    pub fn start(
        home: PathBuf,
        foreground_profile: String,
        cfg: FleetConfig,
        notify: impl Fn(FleetNotice) + Send + Sync + 'static,
    ) -> FleetHandle {
        let _ = (cfg, notify);
        let dial = HostDial::new(home, foreground_profile);
        let snapshot = FleetSnapshot {
            version: 0,
            scope: dial.scope(),
            link: FleetLinkState::Connecting,
            profiles: Vec::new(),
            rows: Vec::new(),
            queue: Vec::new(),
            counts: FleetCounts::default(),
            diagnostics: Vec::new(),
            as_of: 0.0,
        };
        FleetHandle {
            snapshot: Arc::new(snapshot),
        }
    }
}

/// Commands into the fleet task, and the latest snapshot.
pub struct FleetHandle {
    snapshot: Arc<FleetSnapshot>,
}

impl FleetHandle {
    /// The latest snapshot. The stub's snapshot never changes.
    #[must_use]
    pub fn snapshot(&self) -> Arc<FleetSnapshot> {
        Arc::clone(&self.snapshot)
    }

    /// An owner is reporting this session. The stub ignores it.
    pub fn observe_subscribed(&self, key: RowKey, view: SubscribedView) {
        let _ = (key, view);
    }

    /// The owner let go. The stub ignores it.
    pub fn observe_unsubscribed(&self, key: RowKey) {
        let _ = key;
    }

    /// The foreground session changed. The stub ignores it.
    pub fn set_foreground(&self, key: Option<RowKey>) {
        let _ = key;
    }

    /// The person acknowledged a done or failed row. The stub ignores it.
    pub fn acknowledge(&self, key: RowKey) {
        let _ = key;
    }

    /// Ask for a roster and a liveness round now. The stub ignores it.
    pub fn refresh_now(&self) {}

    /// Stop. The stub returns at once and waits on nothing.
    pub async fn close(self) {}
}

/// One-shot read. The stub does not discover or attach.
pub async fn list_once(_home: &Path, _cfg: &FleetConfig) -> Result<FleetSnapshot, FleetListError> {
    Err(FleetListError::Attach("stub".into()))
}
