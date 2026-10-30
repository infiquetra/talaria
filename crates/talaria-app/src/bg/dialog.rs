//! The fleet view's launch dialog, on the app side: the launch the dialog is showing, what the
//! dialog should show for it, and the notices a launch leaves once the dialog is gone.
//!
//! The dialog itself is a terminal widget the terminal loop owns; it hands its fields over as a
//! [`LaunchSubmit`] and draws the latest [`DialogStatus`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use talaria_core::session::NoticeKind;
use tokio::sync::mpsc;

use crate::app::AppMessage;
use crate::bg::outcome::{LaunchOutcome, notice_text};
use crate::bg::request::{LaunchOrigin, LaunchRequest, WorktreeWanted, explicit_slug};
use crate::bg::service::{Busy, LaunchFleet, LaunchNotice, LaunchService, Stage};
use crate::worktree::GitRunner;

/// Shown in the dialog when three launches of this process are still starting.
pub const BUSY_TEXT: &str =
    "Three launches from this Talaria are still starting; wait for one to finish.";
/// Printed after the terminal is restored, once per launch still starting at quit.
pub const QUIT_LINE: &str = "talaria: a background launch was still starting when Talaria quit; check talaria fleet list and .worktrees/.";
/// How long quitting waits for launches still starting.
pub const QUIT_WAIT: Duration = Duration::from_secs(5);

/// What the app needs from its own state to start a launch.
pub struct LaunchContext {
    /// The Hermes home this process uses.
    pub home: PathBuf,
    /// The foreground profile: a fleet launch's link is minted for it.
    pub foreground_profile: String,
    /// The fleet registry, or `None` when the fleet is disabled.
    pub fleet: Option<Arc<dyn LaunchFleet>>,
    /// The app loop, which receives the launch's notices.
    pub sender: Option<mpsc::UnboundedSender<AppMessage>>,
    /// Where `~` points.
    pub home_dir: Option<PathBuf>,
}

/// What one dialog input asks of the app.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LaunchUiOut {
    /// The dialog or its overlay changed.
    pub redraw: bool,
    /// Notices for the scrollback, in order.
    pub notices: Vec<(NoticeKind, String)>,
}

/// What the dialog's fields held when Enter was pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSubmit {
    /// The prompt, as typed.
    pub prompt: String,
    /// The target profile.
    pub profile: String,
    /// The folder, as typed; `~` is expanded here.
    pub folder: String,
    /// The model, when one was typed.
    pub model: Option<String>,
    /// Whether the launch makes a worktree.
    pub worktree: bool,
    /// The worktree name, once the operator edited it.
    pub worktree_name: Option<String>,
}

/// What the terminal reports about the dialog it owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogEvent {
    /// `n` or `w` opened it over the fleet view.
    Opened,
    /// A key or a paste changed a field.
    Edited,
    /// Esc closed it; a launch it started goes on and reports through a notice.
    Closed,
    /// Enter: check these fields, then launch.
    Submit(LaunchSubmit),
}

/// What the dialog should show for the launch it started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogStatus {
    /// One red line; nothing was sent.
    Invalid(String),
    /// The key hint line becomes this progress text.
    Progress(String),
    /// The launch failed; the fields are kept.
    Failed { what: String, next_step: String },
    /// The launch finished; the dialog closes and its draft is dropped.
    Close,
}

/// The launches the fleet view's dialog started, and what the dialog should show.
#[derive(Default)]
pub struct LaunchUi {
    service: Option<LaunchService>,
    shown: Option<Shown>,
    status: Option<(u64, DialogStatus)>,
    version: u64,
}

/// The launch the open dialog is showing.
struct Shown {
    launch: u64,
    stage: Stage,
    ticker: Arc<AtomicBool>,
}

impl Drop for Shown {
    fn drop(&mut self) {
        self.ticker.store(true, Ordering::SeqCst);
    }
}

impl LaunchUi {
    /// Use this service instead of the one made on the first launch: for tests.
    #[doc(hidden)]
    pub fn set_service(&mut self, service: LaunchService) {
        self.service = Some(service);
    }

    /// The latest status for the dialog, with a version that rises on each change.
    #[must_use]
    pub fn status(&self) -> Option<(u64, &DialogStatus)> {
        self.status
            .as_ref()
            .map(|(version, status)| (*version, status))
    }

    fn set_status(&mut self, status: DialogStatus) {
        self.version += 1;
        self.status = Some((self.version, status));
    }

    /// Show the status of an external-agent launch in the same dialog.
    pub fn external_started(&mut self) {
        self.set_status(DialogStatus::Progress("Starting agent…".into()));
    }

    /// Finish an external-agent launch without changing the Hermes launch path.
    pub fn external_finished(&mut self, result: Result<(), String>) {
        self.set_status(match result {
            Ok(()) => DialogStatus::Close,
            Err(what) => DialogStatus::Failed {
                what,
                next_step: "Check the configured agent and try again.".into(),
            },
        });
    }

    /// The dialog was closed with Esc: its launch goes on and reports through a notice.
    pub fn closed(&mut self) {
        self.shown = None;
        self.status = None;
    }

    /// Enter in the dialog: check the fields, then start the launch.
    pub fn submit(&mut self, submit: LaunchSubmit, context: &LaunchContext) -> LaunchUiOut {
        self.launch(submit, context);
        LaunchUiOut {
            redraw: true,
            notices: Vec::new(),
        }
    }

    fn launch(&mut self, submit: LaunchSubmit, context: &LaunchContext) {
        let request = match request_of(submit, context.home_dir.as_deref()) {
            Ok(request) => request,
            Err(text) => {
                self.set_status(DialogStatus::Invalid(text));
                return;
            }
        };
        let service = self.service.get_or_insert_with(|| {
            let sender = context.sender.clone();
            LaunchService::new(context.home.clone(), GitRunner::from_env(), move |notice| {
                if let Some(sender) = &sender {
                    let _ = sender.send(AppMessage::Launch(notice));
                }
            })
        });
        match service.launch(
            request,
            context.foreground_profile.clone(),
            context.fleet.clone(),
        ) {
            Ok(launch) => {
                let ticker = Arc::new(AtomicBool::new(false));
                if let Some(sender) = context.sender.clone() {
                    tokio::spawn(tick(launch, Arc::clone(&ticker), sender));
                }
                self.shown = Some(Shown {
                    launch,
                    stage: Stage::Connecting,
                    ticker,
                });
                self.show_progress();
            }
            Err(Busy) => self.set_status(DialogStatus::Invalid(BUSY_TEXT.to_owned())),
        }
    }

    /// A launch's progress or result.
    pub fn notice(&mut self, notice: LaunchNotice) -> LaunchUiOut {
        match notice {
            LaunchNotice::Progress { launch, stage } => {
                match self.shown.as_mut() {
                    Some(shown) if shown.launch == launch => shown.stage = stage,
                    _ => return LaunchUiOut::default(),
                }
                self.show_progress();
                LaunchUiOut {
                    redraw: true,
                    notices: Vec::new(),
                }
            }
            LaunchNotice::Finished { launch, outcome } => self.finished(launch, &outcome),
        }
    }

    fn finished(&mut self, launch: u64, outcome: &LaunchOutcome) -> LaunchUiOut {
        let showing = self
            .shown
            .as_ref()
            .is_some_and(|shown| shown.launch == launch);
        if showing {
            self.shown = None;
        }
        let failed = match outcome {
            LaunchOutcome::NotStarted { error, .. } => Some(error.clone()),
            _ => None,
        };
        // A failure keeps the dialog open with every field, until the operator dismisses it.
        if showing && let Some(error) = failed {
            self.set_status(DialogStatus::Failed {
                what: error.what,
                next_step: error.next_step,
            });
            return LaunchUiOut {
                redraw: true,
                notices: Vec::new(),
            };
        }
        if showing {
            self.set_status(DialogStatus::Close);
        }
        LaunchUiOut {
            redraw: showing,
            notices: notices_of(outcome),
        }
    }

    /// A second passed for `launch`: true while the dialog shows it, so the spinner and the
    /// elapsed seconds it draws move on.
    #[must_use]
    pub fn tick(&self, launch: u64) -> bool {
        self.shown
            .as_ref()
            .is_some_and(|shown| shown.launch == launch)
    }

    fn show_progress(&mut self) {
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let text = stage_text(shown.stage).to_owned();
        self.set_status(DialogStatus::Progress(text));
    }

    /// Quitting: cancel launches still starting, wait at most [`QUIT_WAIT`], and return one
    /// quit line per launch still running.
    pub async fn shutdown(&mut self) -> Vec<&'static str> {
        self.closed();
        let Some(service) = self.service.take() else {
            return Vec::new();
        };
        let left = service.shutdown(QUIT_WAIT).await;
        left.iter().map(|_| QUIT_LINE).collect()
    }
}

/// The dialog's stage text, without the spinner and the seconds.
#[must_use]
pub fn stage_text(stage: Stage) -> &'static str {
    match stage {
        Stage::Connecting => "Connecting to Hermes\u{2026}",
        Stage::Fetching => "Fetching origin\u{2026}",
        Stage::CreatingWorktree => "Creating worktree\u{2026}",
        Stage::CreatingSession => "Starting session\u{2026}",
        Stage::Submitting => "Sending prompt\u{2026}",
    }
}

/// The scrollback notices a finished launch leaves: the result, then each warning.
fn notices_of(outcome: &LaunchOutcome) -> Vec<(NoticeKind, String)> {
    let kind = match outcome {
        LaunchOutcome::Launched(_) => NoticeKind::System,
        LaunchOutcome::Unknown { .. } => NoticeKind::Warning,
        LaunchOutcome::NotStarted { .. } | LaunchOutcome::Interrupted { .. } => NoticeKind::Error,
    };
    let mut notices = vec![(kind, notice_text(outcome))];
    if let LaunchOutcome::Launched(launched) = outcome {
        notices.extend(
            launched
                .warnings
                .iter()
                .map(|warning| (NoticeKind::Warning, warning.clone())),
        );
    }
    notices
}

/// Turn what the dialog holds into a launch, or the dialog's invalid text.
fn request_of(submit: LaunchSubmit, home_dir: Option<&Path>) -> Result<LaunchRequest, String> {
    if submit.prompt.trim().is_empty() {
        return Err("Write a prompt first.".to_owned());
    }
    let typed = submit.folder.trim();
    let folder = match typed.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => match home_dir {
            Some(home) => home.join(rest.trim_start_matches('/')),
            None => return Err(format!("The folder does not exist: {typed}.")),
        },
        _ => PathBuf::from(typed),
    };
    if !folder.is_absolute() {
        return Err("The folder must be an absolute path or start with ~.".to_owned());
    }
    let folder = match std::fs::canonicalize(&folder) {
        Ok(folder) if folder.is_dir() => folder,
        _ => return Err(format!("The folder does not exist: {typed}.")),
    };
    let name = match submit.worktree_name {
        Some(name) if submit.worktree => match explicit_slug(name.as_str()) {
            Ok(_) => Some(name),
            Err(_) => {
                return Err(
                    "The worktree name needs at least one ASCII letter or digit.".to_owned(),
                );
            }
        },
        _ => None,
    };
    let model = submit.model.filter(|model| !model.trim().is_empty());
    Ok(LaunchRequest {
        prompt: submit.prompt,
        profile: submit.profile,
        folder,
        model,
        provider: None,
        effort: None,
        worktree: submit.worktree.then_some(WorktreeWanted { name }),
        origin: LaunchOrigin::FleetView,
    })
}

/// Once a second while the dialog shows a launch, so its elapsed seconds move.
async fn tick(launch: u64, stop: Arc<AtomicBool>, sender: mpsc::UnboundedSender<AppMessage>) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.tick().await;
    loop {
        interval.tick().await;
        if stop.load(Ordering::SeqCst) || sender.send(AppMessage::LaunchTick(launch)).is_err() {
            break;
        }
    }
}
