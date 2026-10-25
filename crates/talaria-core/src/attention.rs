//! Foreground attention: pane state, alerts, and the rules that raise them.
//!
//! Pure: the tracker reads [`Activity`] values only. Behavior contract: issue 228.

use std::time::{Duration, Instant};

use crate::session::PromptKey;

/// What the foreground pane is doing, as Herdr and the screen show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneState {
    /// Ready for input.
    Idle,
    /// A turn is running.
    Working,
    /// A need is open: the session waits on the person.
    Blocked,
}

/// Why an alert fired. Urgency, highest first: needs you, failed, done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertKind {
    /// A need opened.
    NeedsYou,
    /// A long turn finished.
    Done,
    /// A turn failed.
    Failed,
}

/// How a finished turn ended, from the reduced turn outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnOutcome {
    /// The turn completed.
    Complete,
    /// The turn failed.
    Failed,
    /// The turn was interrupted.
    Interrupted,
}

/// One open need: an approval or clarification prompt, or a masked request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NeedKey {
    /// A prompt in the reduced session state.
    Prompt(PromptKey),
    /// A masked request id.
    Masked(String),
}

/// What a need asks for, for alert and pane message text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NeedSummary {
    /// An approval: raw command and description; the tracker picks the first line.
    Approval {
        /// Raw command text, may be blank.
        command: String,
        /// Raw description text, may be blank.
        description: String,
    },
    /// A clarification question.
    Question(String),
    /// A sudo password prompt.
    Sudo,
    /// A secret prompt.
    Secret,
    /// A vault code prompt.
    VaultCode,
    /// A password manager unlock prompt.
    VaultUnlock,
}

/// One thing that happened in the foreground session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// The turn state moved into running.
    TurnStarted {
        /// When the tracker saw it start.
        at: Instant,
    },
    /// The turn state moved from running to finished.
    TurnEnded {
        /// The reduced outcome.
        outcome: TurnOutcome,
        /// When it ended.
        at: Instant,
    },
    /// An approval, clarification, or masked request appeared.
    NeedOpened {
        /// Which need.
        key: NeedKey,
        /// What it asks for.
        summary: NeedSummary,
        /// True only when it came from the launch snapshot.
        restored: bool,
        /// When it opened.
        at: Instant,
    },
    /// A prompt left the open set, or a masked id left the masked list.
    NeedClosed {
        /// Which need.
        key: NeedKey,
    },
    /// The foreground session changed, or its title became known.
    SessionChanged {
        /// The new session id.
        session_id: String,
        /// Title, else folder name, else `session`.
        label: String,
    },
}

/// Which alert kinds ring the bell or raise a desktop notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionConfig {
    /// Kinds that ring the terminal bell.
    pub bell: Vec<AlertKind>,
    /// Kinds that send a desktop notification.
    pub desktop: Vec<AlertKind>,
    /// A finished turn alerts only at or above this duration.
    pub done_after: Duration,
}

/// One alert the tracker emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Why it fired.
    pub kind: AlertKind,
    /// Ring the bell.
    pub bell: bool,
    /// Send a desktop notification.
    pub desktop: bool,
    /// Text before the product prefix is added.
    pub text: String,
}

/// The current pane report, for the screen and Herdr.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaneReport {
    /// The derived state.
    pub state: PaneState,
    /// Set only for the blocked state.
    pub message: Option<String>,
}

/// What one observed activity produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionOutput {
    /// The alert to write, if any.
    pub alert: Option<Alert>,
    /// The pane report, only when the state or its message changed.
    pub pane: Option<PaneReport>,
}

/// Tracks open needs and the running turn for the foreground session.
#[derive(Debug)]
pub struct AttentionTracker {
    config: AttentionConfig,
    needs: Vec<(NeedKey, NeedSummary)>,
    remembered: std::collections::VecDeque<NeedKey>,
    started: Option<Instant>,
    last_alert: Option<(Instant, AlertKind)>,
    session: Option<String>,
    label: String,
}

impl AttentionTracker {
    /// A tracker with no open needs and no running turn.
    pub fn new(config: AttentionConfig) -> Self {
        Self {
            config,
            needs: Vec::new(),
            remembered: std::collections::VecDeque::new(),
            started: None,
            last_alert: None,
            session: None,
            label: "session".into(),
        }
    }

    /// Fold one activity into the tracker.
    pub fn observe(&mut self, activity: Activity) -> AttentionOutput {
        let before = self.pane();
        let candidate = match activity {
            Activity::TurnStarted { at } => {
                self.started.get_or_insert(at);
                None
            }
            Activity::TurnEnded { outcome, at } => self.started.take().and_then(|start| {
                let kind = match outcome {
                    TurnOutcome::Complete
                        if at.saturating_duration_since(start) >= self.config.done_after =>
                    {
                        AlertKind::Done
                    }
                    TurnOutcome::Failed => AlertKind::Failed,
                    _ => return None,
                };
                let prefix = if kind == AlertKind::Done {
                    "done"
                } else {
                    "failed"
                };
                Some((kind, at, format!("{prefix}: {}", cut(&self.label, 60))))
            }),
            Activity::NeedOpened {
                key,
                summary,
                restored,
                at,
            } => {
                let message = summary_text(&summary);
                if !self.needs.iter().any(|(k, _)| k == &key) {
                    self.needs.push((key.clone(), summary));
                }
                if restored || self.remembered.contains(&key) {
                    None
                } else {
                    self.remembered.push_back(key);
                    if self.remembered.len() > 256 {
                        self.remembered.pop_front();
                    }
                    Some((AlertKind::NeedsYou, at, format!("needs you: {message}")))
                }
            }
            Activity::NeedClosed { key } => {
                self.needs.retain(|(k, _)| k != &key);
                None
            }
            Activity::SessionChanged { session_id, label } => {
                if self.session.as_ref() != Some(&session_id) {
                    self.needs.clear();
                    self.started = None;
                }
                self.session = Some(session_id);
                self.label = label;
                None
            }
        };
        let alert = candidate.and_then(|(kind, at, text)| {
            let bell = self.config.bell.contains(&kind);
            let desktop = self.config.desktop.contains(&kind);
            if !bell && !desktop {
                return None;
            }
            if self.last_alert.is_some_and(|(last, previous)| {
                at.saturating_duration_since(last) < Duration::from_secs(2)
                    && urgency(kind) <= urgency(previous)
            }) {
                return None;
            }
            self.last_alert = Some((at, kind));
            Some(Alert {
                kind,
                bell,
                desktop,
                text,
            })
        });
        let after = self.pane();
        AttentionOutput {
            alert,
            pane: (before != after).then_some(after),
        }
    }

    /// The current report, for re-sending after a resume.
    pub fn pane(&self) -> PaneReport {
        match self.needs.last() {
            Some((_, summary)) => PaneReport {
                state: PaneState::Blocked,
                message: Some(summary_text(summary)),
            },
            None => PaneReport {
                state: if self.started.is_some() {
                    PaneState::Working
                } else {
                    PaneState::Idle
                },
                message: None,
            },
        }
    }
}

/// Relative importance of an alert while output is held.
pub fn urgency(kind: AlertKind) -> u8 {
    match kind {
        AlertKind::NeedsYou => 2,
        AlertKind::Failed => 1,
        AlertKind::Done => 0,
    }
}

/// Cut a scalar-value string, including the ellipsis in the limit.
pub fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    if limit == 0 {
        return String::new();
    }
    text.chars()
        .take(limit - 1)
        .chain(std::iter::once('…'))
        .collect()
}

fn summary_text(summary: &NeedSummary) -> String {
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

/// Remove terminal controls and invisible direction-changing characters from alert fields.
pub fn sanitize_alert_field(text: &str, limit: usize) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control() && !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')).collect();
    cut(&clean, limit)
}
