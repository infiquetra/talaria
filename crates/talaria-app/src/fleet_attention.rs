//! Decide fleet notices: hold them until the snapshot, then emit effects.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use talaria_core::alert_gate::AlertGate;
use talaria_core::attention::{Alert, AlertKind};
use talaria_core::fleet::{NeedId, ProfileStatus, RowKey, RowState};
use talaria_core::fleet_attention::{FleetAttention, FleetAttentionConfig};

use crate::fleet::{FleetLinkState, FleetNotice, FleetSnapshot};

/// The window in which one alert passes. Every alert, foreground and background, shares it.
const GATE_WINDOW: Duration = Duration::from_secs(2);

/// The sentence the jump key gives when the fleet view is off.
const FLEET_OFF: &str =
    "The fleet view is off: [fleet] enabled = false in Talaria's configuration.";
/// The sentence the jump key gives while the fleet is still listing.
const FLEET_LOADING: &str = "The fleet is still loading; try again in a moment.";
/// The sentence the jump key gives when nothing waits.
const NOTHING_WAITING: &str = "Nothing in the background needs you.";

/// Status-line counters. `needs_you` counts requests; the others count background rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusCounts {
    /// Foreground requests plus background queue items.
    pub needs_you: u32,
    /// Background rows working.
    pub working: u32,
    /// Background rows failed.
    pub failed: u32,
    /// Background rows done.
    pub done: u32,
}

/// Counters for the status line. Without a snapshot, only `foreground_open` is kept.
pub fn status_counts(foreground_open: u32, snapshot: Option<&FleetSnapshot>) -> StatusCounts {
    let (items, working, failed, done) = snapshot.map_or((0, 0, 0, 0), |snapshot| {
        (
            snapshot.counts.needs_you_items,
            snapshot.counts.working,
            snapshot.counts.failed,
            snapshot.counts.done,
        )
    });
    StatusCounts {
        needs_you: foreground_open.saturating_add(count(items)),
        working: count(working),
        failed: count(failed),
        done: count(done),
    }
}

/// A background count, saturated into the status line's own width.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Where the jump key goes.
#[derive(Clone, Debug, PartialEq)]
pub enum NextNeed {
    /// Open the fleet view on this row.
    Peek(RowKey),
    /// A sentence for the notice path. It does not ring.
    Notice(String),
}

/// The oldest background request, or the sentence that says why there is none.
pub fn next_need(snapshot: Option<&FleetSnapshot>, fleet_enabled: bool) -> NextNeed {
    if !fleet_enabled {
        return NextNeed::Notice(FLEET_OFF.to_owned());
    }
    let Some(snapshot) = snapshot else {
        return NextNeed::Notice(FLEET_LOADING.to_owned());
    };
    // A request that is already listed is always reachable, even while the link is down.
    if let Some(item) = snapshot
        .queue
        .iter()
        .filter(|item| !item.foreground)
        .min_by_key(|item| item.seq)
    {
        return NextNeed::Peek(item.id.key.clone());
    }
    match &snapshot.link {
        FleetLinkState::Failed { reason } => {
            NextNeed::Notice(format!("The fleet is not available: {reason}."))
        }
        FleetLinkState::Connecting | FleetLinkState::Reconnecting { .. } => {
            NextNeed::Notice(FLEET_LOADING.to_owned())
        }
        FleetLinkState::Connected => {
            if snapshot
                .profiles
                .iter()
                .any(|(_, status)| matches!(status, ProfileStatus::Loading))
            {
                NextNeed::Notice(FLEET_LOADING.to_owned())
            } else {
                NextNeed::Notice(NOTHING_WAITING.to_owned())
            }
        }
    }
}

/// What one notice or wake asks the application to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubOutput {
    /// Alerts the shared gate admitted.
    pub alerts: Vec<Alert>,
    /// A start-up or jump notice. Never rings.
    pub notice: Option<String>,
    /// The next wake, when a window end or a hold deadline moved.
    pub wake_at: Option<Instant>,
}

/// How long a notice may wait for the snapshot that describes it.
pub const HOLD_LIMIT: Duration = Duration::from_secs(1);
/// The most notices held at once. The oldest is decided when another arrives.
pub const HOLD_MAX: usize = 64;

/// One notice waiting for the snapshot that describes it.
#[derive(Clone)]
struct Held {
    /// Which alert.
    kind: AlertKind,
    /// The row it belongs to.
    key: RowKey,
    /// The queue item, when the alert is a need opening.
    need: Option<NeedId>,
    /// When the notice has to be decided.
    deadline: Instant,
}

/// The fleet tracker, the shared gate, and notices waiting for a snapshot.
pub struct AttentionHub {
    tracker: FleetAttention,
    gate: AlertGate,
    held: VecDeque<Held>,
    key_label: Option<String>,
    /// Whether the fleet was listing when the last snapshot came.
    listing: bool,
}

impl AttentionHub {
    /// A hub with an empty hold queue.
    pub fn new(
        cfg: FleetAttentionConfig,
        foreground_profile: String,
        key_label: Option<String>,
    ) -> AttentionHub {
        AttentionHub {
            tracker: FleetAttention::new(cfg, foreground_profile),
            gate: AlertGate::new(GATE_WINDOW),
            held: VecDeque::new(),
            key_label,
            listing: false,
        }
    }

    /// One fleet notice, decided now or held until the snapshot agrees.
    pub fn on_notice(
        &mut self,
        notice: &FleetNotice,
        latest: &FleetSnapshot,
        at: Instant,
    ) -> HubOutput {
        let mut out = HubOutput::default();
        match notice {
            FleetNotice::Changed(snapshot) => {
                self.observe_fleet(snapshot, at);
                self.decide_ready(latest, at, &mut out);
            }
            FleetNotice::Attention {
                kind,
                key,
                need,
                foreground,
            } => {
                // The foreground session has its own tracker; its notices never wait here.
                if *foreground {
                    out.wake_at = self.next_wake();
                    return out;
                }
                if self.needs_hold(*kind, key, need.as_ref(), latest) {
                    self.hold(
                        Held {
                            kind: *kind,
                            key: key.clone(),
                            need: need.clone(),
                            deadline: at + HOLD_LIMIT,
                        },
                        latest,
                        at,
                        &mut out,
                    );
                } else {
                    self.decide(*kind, key, need.as_ref(), latest, at, &mut out);
                }
            }
        }
        out.wake_at = self.next_wake();
        out
    }

    /// A foreground alert passes the same gate as fleet alerts.
    pub fn on_foreground_alert(&mut self, alert: Alert, at: Instant) -> Option<Alert> {
        self.gate.admit(alert, at)
    }

    /// A wake: decide held notices whose deadline has passed, then the start-up notice.
    pub fn on_wake(&mut self, at: Instant, latest: Option<&FleetSnapshot>) -> HubOutput {
        let mut out = HubOutput::default();
        let expired: Vec<Held> = self
            .held
            .iter()
            .filter(|held| held.deadline <= at)
            .cloned()
            .collect();
        for held in expired {
            self.forget(&held);
            let empty = empty_snapshot();
            let snapshot = latest.unwrap_or(&empty);
            self.decide(
                held.kind,
                &held.key,
                held.need.as_ref(),
                snapshot,
                at,
                &mut out,
            );
        }
        let sessions = latest.map_or(0, |snapshot| snapshot.counts.needs_you);
        out.notice = self
            .tracker
            .on_wake(at, sessions, self.key_label.as_deref());
        out.wake_at = self.next_wake();
        out
    }

    /// The profile whose rows are not prefixed in a label.
    pub fn set_foreground_profile(&mut self, profile: String) {
        self.tracker.set_foreground_profile(profile);
    }

    /// One snapshot of the fleet: the quiet window, the watched rows and the turn marks.
    fn observe_fleet(&mut self, snapshot: &FleetSnapshot, at: Instant) {
        let connected = matches!(snapshot.link, FleetLinkState::Connected);
        let connecting = matches!(snapshot.link, FleetLinkState::Connecting);
        let has_loading = snapshot
            .profiles
            .iter()
            .any(|(_, status)| matches!(status, ProfileStatus::Loading));
        let listing = (connected || connecting) && has_loading;
        let listed = connected && !snapshot.profiles.is_empty() && !has_loading;
        if !connected && !connecting {
            self.listing = false;
            self.tracker.observe_link(false, at);
        } else if listing && !self.listing {
            // The fleet started listing: the quiet window opens, so a request that was
            // already waiting does not ring the bell.
            self.listing = true;
            if self.tracker.window_end().is_none_or(|end| at >= end) {
                self.tracker.observe_link(false, at);
                self.tracker.observe_link(true, at);
            }
        }
        if listed {
            // Every profile has listed: the window ends three seconds after this.
            self.listing = false;
            self.tracker.observe_settled(at);
        }
        self.tracker.set_link_connected(connected);
        self.tracker.observe_rows(&snapshot.rows, at);
    }

    /// Whether a notice has to wait for a snapshot that describes it.
    fn needs_hold(
        &self,
        kind: AlertKind,
        key: &RowKey,
        need: Option<&NeedId>,
        latest: &FleetSnapshot,
    ) -> bool {
        let Some(row) = latest.rows.iter().find(|row| &row.key == key) else {
            return true;
        };
        match kind {
            AlertKind::NeedsYou => {
                need.is_some_and(|id| !latest.queue.iter().any(|item| &item.id == id))
            }
            AlertKind::Done | AlertKind::Failed => {
                !matches!(row.state, RowState::Done | RowState::Failed)
            }
        }
    }

    /// Keep a notice until its snapshot arrives.
    fn hold(&mut self, held: Held, latest: &FleetSnapshot, at: Instant, out: &mut HubOutput) {
        self.held.push_back(held);
        while self.held.len() > HOLD_MAX {
            let Some(oldest) = self.held.pop_front() else {
                break;
            };
            self.decide(
                oldest.kind,
                &oldest.key,
                oldest.need.as_ref(),
                latest,
                at,
                out,
            );
        }
    }

    /// Forget a held notice.
    fn forget(&mut self, held: &Held) {
        self.held.retain(|waiting| {
            !(waiting.key == held.key && waiting.need == held.need && waiting.kind == held.kind)
        });
    }

    /// Decide every held notice the latest snapshot now describes.
    fn decide_ready(&mut self, latest: &FleetSnapshot, at: Instant, out: &mut HubOutput) {
        let ready: Vec<Held> = self
            .held
            .iter()
            .filter(|held| !self.needs_hold(held.kind, &held.key, held.need.as_ref(), latest))
            .cloned()
            .collect();
        for held in ready {
            self.forget(&held);
            self.decide(held.kind, &held.key, held.need.as_ref(), latest, at, out);
        }
    }

    /// Decide one notice and pass its alert through the shared gate.
    fn decide(
        &mut self,
        kind: AlertKind,
        key: &RowKey,
        need: Option<&NeedId>,
        latest: &FleetSnapshot,
        at: Instant,
        out: &mut HubOutput,
    ) {
        let row = latest.rows.iter().find(|row| &row.key == key);
        let detail = need
            .and_then(|id| latest.queue.iter().find(|item| &item.id == id))
            .map(|item| &item.detail);
        let Some(alert) = self
            .tracker
            .on_attention(kind, key, need, detail, false, row, at)
        else {
            return;
        };
        if let Some(alert) = self.gate.admit(alert, at) {
            out.alerts.push(alert);
        }
    }

    /// The next instant the hub wants to be woken at.
    fn next_wake(&self) -> Option<Instant> {
        let window = self.tracker.window_end();
        let held = self.held.iter().map(|held| held.deadline).min();
        match (window, held) {
            (Some(window), Some(held)) => Some(window.min(held)),
            (Some(window), None) => Some(window),
            (None, Some(held)) => Some(held),
            (None, None) => None,
        }
    }
}

/// The snapshot a wake falls back on when no snapshot has arrived.
pub fn empty_snapshot() -> FleetSnapshot {
    FleetSnapshot {
        version: 0,
        scope: crate::side_connection::SideScope::ProfileOnly {
            profile: String::new(),
            why: String::new(),
        },
        link: FleetLinkState::Connecting,
        profiles: Vec::new(),
        rows: Vec::new(),
        queue: Vec::new(),
        counts: talaria_core::fleet::FleetCounts::default(),
        diagnostics: Vec::new(),
        as_of: 0.0,
    }
}

/// Wait until `at`, then call `send` once.
pub fn wake_task(at: Instant, send: impl FnOnce() + Send + 'static) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
        send();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::side_connection::SideScope;
    use talaria_core::attention::NeedSummary;
    use talaria_core::fleet::{FleetCounts, FleetRow, NeedDetail, NeedItem, NeedSlot};
    use talaria_core::fleet_attention::FleetScope;

    #[test]
    fn gate_startup_sequence_suppresses_bell_and_emits_startup_notice() {
        let base = Instant::now();
        let mut hub = AttentionHub::new(
            FleetAttentionConfig {
                bell: vec![AlertKind::NeedsYou, AlertKind::Failed],
                desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
                done_after: Duration::from_secs(20),
                needs_you: FleetScope::All,
                finished: FleetScope::Watched,
            },
            "fixture".to_owned(),
            Some("alt+a".to_owned()),
        );
        let key = RowKey {
            profile: "fixture".to_owned(),
            session_id: "sess-1".to_owned(),
        };
        let need_id = NeedId {
            key: key.clone(),
            slot: NeedSlot::Prompt("p1".to_owned()),
        };

        // 1. FleetTask::connected applies FleetInput::Profiles while link is still Connecting.
        let mut snap = empty_snapshot();
        snap.version = 1;
        snap.scope = SideScope::ProfileOnly {
            profile: "fixture".to_owned(),
            why: String::new(),
        };
        snap.link = FleetLinkState::Connecting;
        snap.profiles = vec![("fixture".to_owned(), ProfileStatus::Loading)];
        let out1 = hub.on_notice(
            &FleetNotice::Changed(std::sync::Arc::new(snap.clone())),
            &snap,
            base,
        );
        assert_eq!(out1.wake_at, Some(base + Duration::from_secs(15)));

        // 2. Roster, Liveness, and Approvals complete while Gate is still open (link == Connecting).
        snap.version = 2;
        snap.profiles = vec![(
            "fixture".to_owned(),
            ProfileStatus::Listed { truncated: false },
        )];
        snap.rows = vec![FleetRow {
            key: key.clone(),
            title: "probe".to_owned(),
            model: None,
            source: None,
            started_at: None,
            message_count: 1,
            state: RowState::NeedsYou,
            state_since: 0.0,
            outcome: None,
            foreground: false,
            subscribed: false,
            stale: false,
            gone: false,
            unlisted: false,
            activity: None,
            cost_usd: None,
        }];
        snap.queue = vec![NeedItem {
            id: need_id.clone(),
            seq: 1,
            since: 0.0,
            detail: NeedDetail::Known(NeedSummary::Approval {
                command: "rm -rf ./talaria-attention-probe".to_owned(),
                description: String::new(),
            }),
            choices: Vec::new(),
            execution_generation: None,
            timeout_s: None,
            exact: false,
            foreground: false,
        }];
        snap.counts = FleetCounts {
            needs_you: 1,
            needs_you_items: 1,
            ..FleetCounts::default()
        };
        let t_approvals = base + Duration::from_millis(500);
        let _ = hub.on_notice(
            &FleetNotice::Changed(std::sync::Arc::new(snap.clone())),
            &snap,
            t_approvals,
        );
        let out_att = hub.on_notice(
            &FleetNotice::Attention {
                kind: AlertKind::NeedsYou,
                key: key.clone(),
                need: Some(need_id),
                foreground: false,
            },
            &snap,
            t_approvals,
        );
        assert!(
            out_att.alerts.is_empty(),
            "quiet window suppresses startup approval bell"
        );

        // 3. open_gate_if_done publishes link == Connected.
        snap.version = 3;
        snap.link = FleetLinkState::Connected;
        let t_connected = base + Duration::from_millis(600);
        let out_conn = hub.on_notice(
            &FleetNotice::Changed(std::sync::Arc::new(snap.clone())),
            &snap,
            t_connected,
        );
        assert_eq!(out_conn.wake_at, Some(base + Duration::from_secs(4)));

        // 4. Wake at the shortened quiet-window end emits the startup notice once.
        let out_wake = hub.on_wake(base + Duration::from_secs(4), Some(&snap));
        assert!(out_wake.alerts.is_empty());
        assert_eq!(
            out_wake.notice.as_deref(),
            Some("1 background session needs you; press alt+a to see it.")
        );
    }
}
