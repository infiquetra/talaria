//! Background fleet attention: quiet window, watched keys, and alert text.
//!
//! Pure: no clock, no input or output, no terminal library. The caller supplies every
//! instant, so the same inputs always give the same alerts.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::attention::{Alert, AlertKind, NeedSummary, cut};
use crate::fleet::{FleetRow, NeedDetail, NeedId, Outcome, RowKey, RowState};

/// Which background rows may raise an alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FleetScope {
    /// Every background row.
    All,
    /// Rows this process has watched.
    Watched,
    /// Never.
    Off,
}

/// Channels, the done threshold, and the two scopes.
#[derive(Clone, Debug, PartialEq)]
pub struct FleetAttentionConfig {
    /// Kinds that ring the terminal bell.
    pub bell: Vec<AlertKind>,
    /// Kinds that send a desktop notification.
    pub desktop: Vec<AlertKind>,
    /// A finished turn alerts only at or above this duration.
    pub done_after: Duration,
    /// Scope for needs-you alerts. Default is every background row.
    pub needs_you: FleetScope,
    /// Scope for done and failed alerts. Default is watched rows.
    pub finished: FleetScope,
}

/// Quiet time after the link connects, before the first full listing shortens it.
pub const QUIET_AFTER_CONNECT: Duration = Duration::from_secs(4);
/// Quiet time after the first snapshot whose profiles are all listed.
pub const QUIET_AFTER_SETTLED: Duration = Duration::from_secs(3);
/// The longest a quiet window stays open.
pub const QUIET_CAP: Duration = Duration::from_secs(15);

/// The most watched keys kept at once. The oldest leaves first.
const WATCHED_MAX: usize = 512;
/// The most turn marks kept at once. The oldest leaves first.
const MARKS_MAX: usize = 512;
/// The most alerted need ids kept at once. The oldest leaves first.
const ALERTED_MAX: usize = 512;

/// The characters a title must not carry: the ones that move the cursor or the reading
/// order. The same set the terminal writer drops.
fn is_invisible(character: char) -> bool {
    matches!(
        character,
        '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'
    )
}

/// The quiet window, while it is open.
struct QuietWindow {
    /// When the window opened.
    opened_at: Instant,
    /// When the window stops being quiet. The window is half open: an alert at this
    /// instant is not quiet.
    end: Instant,
    /// Whether a full listing already shortened the window.
    settled: bool,
}

/// How long a turn ran, as far as the fleet knows.
enum TurnLength {
    /// The turn started at this instant.
    Known(Instant),
    /// The turn was already running when the fleet learned about it.
    Unknown,
    /// The fleet never saw the turn running.
    Missing,
}

/// A mark on one row: when its current turn started, or that the fleet cannot say.
#[derive(Clone, Copy)]
enum TurnMark {
    /// The turn started at this instant.
    Known(Instant),
    /// The turn was already running when the fleet learned about it.
    Unknown,
}

/// Watched keys, turn marks, and the quiet window for background alerts.
pub struct FleetAttention {
    config: FleetAttentionConfig,
    foreground_profile: String,
    connected: bool,
    window: Option<QuietWindow>,
    watched: VecDeque<RowKey>,
    watched_set: HashSet<RowKey>,
    marks: Vec<(RowKey, TurnMark)>,
    alerted: VecDeque<NeedId>,
    alerted_set: HashSet<NeedId>,
    suppressed_needs: bool,
}

impl FleetAttention {
    /// A tracker with an empty watched set and no open window.
    pub fn new(cfg: FleetAttentionConfig, foreground_profile: String) -> FleetAttention {
        FleetAttention {
            config: cfg,
            foreground_profile,
            connected: false,
            window: None,
            watched: VecDeque::new(),
            watched_set: HashSet::new(),
            marks: Vec::new(),
            alerted: VecDeque::new(),
            alerted_set: HashSet::new(),
            suppressed_needs: false,
        }
    }

    /// The profile whose rows are not prefixed in a label.
    pub fn set_foreground_profile(&mut self, profile: String) {
        self.foreground_profile = profile;
    }

    /// A link transition. Connecting opens a window and returns its provisional end.
    pub fn observe_link(&mut self, connected: bool, at: Instant) -> Option<Instant> {
        if !connected {
            self.connected = false;
            return None;
        }
        if self.connected {
            return None;
        }
        self.connected = true;
        self.open_window(at)
    }

    /// Open a quiet window that ends at the cap.
    fn open_window(&mut self, at: Instant) -> Option<Instant> {
        let end = at + QUIET_CAP;
        self.window = Some(QuietWindow {
            opened_at: at,
            end,
            settled: false,
        });
        self.suppressed_needs = false;
        Some(end)
    }

    /// The link's state, without a quiet window.
    ///
    /// The window opens when the fleet starts listing its rows, which the caller knows
    /// and the tracker does not. This only records the link, for the turn marks.
    pub fn set_link_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// The fleet started listing: the quiet window opens, so a request that was already
    /// waiting does not ring the bell. A window that is still quiet stays as it is.
    pub fn begin_discovery(&mut self, at: Instant) -> Option<Instant> {
        if self.window.as_ref().is_some_and(|window| at < window.end) {
            return None;
        }
        self.open_window(at)
    }

    /// The first full listing inside an open window fixes the window end.
    pub fn observe_settled(&mut self, at: Instant) -> Option<Instant> {
        let window = self.window.as_mut()?;
        if window.settled || at >= window.end {
            return None;
        }
        window.settled = true;
        let opened_at = window.opened_at;
        window.end = (opened_at + QUIET_AFTER_CONNECT)
            .max(at + QUIET_AFTER_SETTLED)
            .min(opened_at + QUIET_CAP);
        Some(window.end)
    }

    /// When the quiet window stops, while one is open.
    pub fn window_end(&self) -> Option<Instant> {
        self.window.as_ref().map(|window| window.end)
    }

    /// One snapshot: update the watched set and the turn marks.
    pub fn observe_rows(&mut self, rows: &[FleetRow], at: Instant) {
        let mut present: HashSet<&RowKey> = HashSet::new();
        for row in rows {
            present.insert(&row.key);
            if row.foreground || row.subscribed {
                self.watch(&row.key);
            }
            match row.state {
                RowState::Working | RowState::NeedsYou => {
                    if self.mark_of(&row.key).is_none() {
                        let mark = if !self.connected || self.quiet_at(at) {
                            TurnMark::Unknown
                        } else {
                            TurnMark::Known(at)
                        };
                        self.set_mark(row.key.clone(), mark);
                    }
                }
                RowState::Idle => {
                    self.remove_mark(&row.key);
                }
                RowState::Done | RowState::Failed => {}
            }
        }
        let gone: Vec<RowKey> = self
            .marks
            .iter()
            .map(|(key, _)| key.clone())
            .filter(|key| !present.contains(key))
            .collect();
        for key in gone {
            self.remove_mark(&key);
        }
    }

    /// One attention notice, after the hub has finished holding it.
    #[allow(clippy::too_many_arguments)]
    pub fn on_attention(
        &mut self,
        kind: AlertKind,
        key: &RowKey,
        need: Option<&NeedId>,
        detail: Option<&NeedDetail>,
        foreground: bool,
        row: Option<&FleetRow>,
        at: Instant,
    ) -> Option<Alert> {
        // The foreground session has its own tracker; this one never speaks for it.
        if foreground {
            return None;
        }
        // A finished turn takes its mark before any filter, so a notice the scope drops
        // still clears the mark and the next turn is measured from its own start.
        let length = match kind {
            AlertKind::Done | AlertKind::Failed => self.take_mark(key, row, at),
            AlertKind::NeedsYou => TurnLength::Missing,
        };
        let scope = match kind {
            AlertKind::NeedsYou => self.config.needs_you,
            AlertKind::Done | AlertKind::Failed => self.config.finished,
        };
        match scope {
            FleetScope::Off => return None,
            FleetScope::Watched if !self.is_watched(key) => return None,
            FleetScope::Watched | FleetScope::All => {}
        }
        // A need id alerts once. The id is remembered even when a later step drops the
        // alert, so a repeat of the same request stays quiet. The fleet sends an id with
        // every needs-you notice, and a notice that carries one never alerts twice.
        if let Some(id) = need {
            if self.alerted_set.contains(id) {
                return None;
            }
            self.remember_alerted(id.clone());
        }
        if kind == AlertKind::Done {
            if row.and_then(|row| row.outcome) == Some(Outcome::Interrupted) {
                return None;
            }
            match length {
                TurnLength::Missing => return None,
                TurnLength::Known(started) => {
                    if at.saturating_duration_since(started) < self.config.done_after {
                        return None;
                    }
                }
                TurnLength::Unknown => {}
            }
        }
        if self.quiet_at(at) {
            if kind == AlertKind::NeedsYou {
                self.suppressed_needs = true;
            }
            return None;
        }
        let bell = self.config.bell.contains(&kind);
        let desktop = self.config.desktop.contains(&kind);
        if !bell && !desktop {
            return None;
        }
        let title = row.map_or("", |row| row.title.as_str());
        let label = row_label(key, title, &self.foreground_profile);
        let outcome = row.and_then(|row| row.outcome);
        let text = fleet_alert_text(kind, &label, detail, outcome)?;
        Some(Alert {
            kind,
            bell,
            desktop,
            text,
        })
    }

    /// A wake. Returns the start-up notice at most once per window.
    pub fn on_wake(
        &mut self,
        at: Instant,
        sessions_needing_you: usize,
        key_label: Option<&str>,
    ) -> Option<String> {
        let end = self.window.as_ref()?.end;
        if at < end {
            return None;
        }
        let suppressed = self.suppressed_needs;
        self.window = None;
        self.suppressed_needs = false;
        if suppressed && sessions_needing_you > 0 {
            Some(startup_notice_text(sessions_needing_you, key_label))
        } else {
            None
        }
    }

    /// Whether the quiet window covers this instant.
    fn quiet_at(&self, at: Instant) -> bool {
        self.window.as_ref().is_some_and(|window| at < window.end)
    }

    /// Take the row's mark. A row that is working again owns the mark now, so the notice
    /// that arrives with it belongs to a turn whose length nobody knows.
    fn take_mark(&mut self, key: &RowKey, row: Option<&FleetRow>, at: Instant) -> TurnLength {
        let running =
            row.is_some_and(|row| matches!(row.state, RowState::Working | RowState::NeedsYou));
        if running {
            self.set_mark(key.clone(), TurnMark::Known(at));
            return TurnLength::Unknown;
        }
        match self.remove_mark(key) {
            Some(TurnMark::Known(started)) => TurnLength::Known(started),
            Some(TurnMark::Unknown) => TurnLength::Unknown,
            None => TurnLength::Missing,
        }
    }

    /// The mark on one row, when it has one.
    fn mark_of(&self, key: &RowKey) -> Option<TurnMark> {
        self.marks
            .iter()
            .find(|(marked, _)| marked == key)
            .map(|(_, mark)| *mark)
    }

    /// Put a mark on one row, keeping the newest marks.
    fn set_mark(&mut self, key: RowKey, mark: TurnMark) {
        if let Some(slot) = self.marks.iter_mut().find(|(marked, _)| *marked == key) {
            slot.1 = mark;
            return;
        }
        self.marks.push((key, mark));
        while self.marks.len() > MARKS_MAX {
            self.marks.remove(0);
        }
    }

    /// Take the mark off one row.
    fn remove_mark(&mut self, key: &RowKey) -> Option<TurnMark> {
        let index = self.marks.iter().position(|(marked, _)| marked == key)?;
        Some(self.marks.remove(index).1)
    }

    /// Whether this process has watched the row.
    fn is_watched(&self, key: &RowKey) -> bool {
        self.watched_set.contains(key)
    }

    /// Watch a row, keeping the newest keys.
    fn watch(&mut self, key: &RowKey) {
        if self.watched_set.contains(key) {
            self.watched.retain(|marked| marked != key);
        }
        self.watched.push_back(key.clone());
        self.watched_set.insert(key.clone());
        while self.watched.len() > WATCHED_MAX {
            if let Some(oldest) = self.watched.pop_front() {
                self.watched_set.remove(&oldest);
            }
        }
    }

    /// Remember a need id, keeping the newest ids.
    fn remember_alerted(&mut self, id: NeedId) {
        if self.alerted_set.contains(&id) {
            self.alerted.retain(|remembered| remembered != &id);
        }
        self.alerted.push_back(id.clone());
        self.alerted_set.insert(id);
        while self.alerted.len() > ALERTED_MAX {
            if let Some(oldest) = self.alerted.pop_front() {
                self.alerted_set.remove(&oldest);
            }
        }
    }
}

/// The label of a row: title, or a short session id, with a profile prefix when needed.
pub fn row_label(key: &RowKey, title: &str, foreground_profile: &str) -> String {
    let clean: String = title
        .chars()
        .filter(|character| !character.is_control() && !is_invisible(*character))
        .collect();
    let name = clean.trim();
    let name = if name.is_empty() {
        let tail: String = key
            .session_id
            .chars()
            .rev()
            .take(6)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        format!("#{tail}")
    } else {
        name.to_owned()
    };
    let label = if key.profile == foreground_profile {
        name
    } else {
        format!("{}/{}", cut(&key.profile, 12), name)
    };
    cut(&label, 32)
}

/// The phrase after `needs you: `, matching the foreground tracker.
pub fn need_phrase(summary: &NeedSummary) -> String {
    let first = |text: &str| cut(text.lines().next().unwrap_or("").trim(), 60);
    match summary {
        NeedSummary::Approval {
            command,
            description,
        } => {
            let text = first(if command.trim().is_empty() {
                description
            } else {
                command
            });
            if text.is_empty() {
                "approval".into()
            } else {
                format!("approval: {text}")
            }
        }
        NeedSummary::Question(text) => format!("question: {}", first(text)),
        NeedSummary::Sudo => "password for sudo".into(),
        NeedSummary::Secret => "a secret".into(),
        NeedSummary::VaultCode => "a vault code".into(),
        NeedSummary::VaultUnlock => "a password manager unlock".into(),
    }
}

/// Alert text before the product prefix. `None` means no alert.
pub fn fleet_alert_text(
    kind: AlertKind,
    label: &str,
    detail: Option<&NeedDetail>,
    outcome: Option<Outcome>,
) -> Option<String> {
    match kind {
        AlertKind::NeedsYou => {
            let phrase = match detail {
                Some(NeedDetail::Known(summary)) => Some(need_phrase(summary)),
                Some(NeedDetail::Waiting) | None => None,
            };
            Some(match phrase {
                Some(phrase) => format!("{label}: needs you: {phrase}"),
                None => format!("{label}: needs you"),
            })
        }
        AlertKind::Done => match outcome {
            Some(Outcome::Completed) => Some(format!("{label}: done")),
            Some(Outcome::Interrupted) => None,
            Some(Outcome::Failed) | Some(Outcome::Unknown) | None => {
                Some(format!("{label}: finished"))
            }
        },
        AlertKind::Failed => Some(format!("{label}: failed")),
    }
}

/// The start-up notice for sessions already waiting.
pub fn startup_notice_text(sessions: usize, key_label: Option<&str>) -> String {
    let plural = sessions != 1;
    let noun = if plural { "sessions" } else { "session" };
    let verb = if plural { "need" } else { "needs" };
    match key_label {
        Some(key) if !key.trim().is_empty() => {
            let tail = if plural {
                format!("press {key} to see the first.")
            } else {
                format!("press {key} to see it.")
            };
            format!("{sessions} background {noun} {verb} you; {tail}")
        }
        _ => format!("{sessions} background {noun} {verb} you."),
    }
}
