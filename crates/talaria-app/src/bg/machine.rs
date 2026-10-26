//! The launch state machine. Pure: no input, no clock, no files.

use std::path::PathBuf;

use crate::bg::outcome::{LaunchError, LaunchOutcome, Launched};
use crate::bg::request::{LaunchRequest, explicit_slug, slugify};
use crate::worktree::WorktreeRecord;

/// Identifiers minted for one launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchIds {
    /// Sent on `session.create`.
    pub request_id: String,
    /// Sent on `prompt.submit`.
    pub submission_id: String,
}

/// The handshake summary this launch reads. No describe frame is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescribeSummary {
    /// Session sources the gateway offers.
    pub sources: Vec<String>,
    /// Create parameters the gateway offers.
    pub parameters: Vec<String>,
}

/// A session the gateway confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedSession {
    /// The live session id.
    pub session_id: String,
    /// The id a later resume uses, when the gateway sent one.
    pub stored_session_id: Option<String>,
    /// The subscription this connection holds.
    pub subscription_id: String,
    /// `info.cwd` from the snapshot, when present.
    pub info_cwd: Option<String>,
    /// `info.model` from the snapshot, when present.
    pub model: Option<String>,
}

/// A submit the gateway answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitReply {
    /// Any admission status. The prompt was accepted.
    Admitted {
        /// The admission id.
        admission_id: String,
        /// Copied onto the outcome.
        status: String,
    },
    /// The gateway took the text as a voice stop phrase.
    VoiceStopped,
}

/// A call that did not return the shape this launch wanted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// An error reply other than 5001.
    Refused {
        /// The JSON-RPC code.
        code: i64,
        /// `data.reason`, when the reply had one.
        reason: Option<String>,
        /// `data.field` or `data.fields`.
        fields: Vec<String>,
    },
    /// Sent, then dropped, timed out, or answered 5001.
    Unknown,
    /// The frame was never written.
    NotSent(String),
    /// A reply arrived and matched neither shape.
    Malformed(String),
}

/// How the reported folder compares with the folder that was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CwdCheck {
    /// The same folder after canonicalising.
    Matches,
    /// A different folder, or a path that cannot be canonicalised.
    Differs,
    /// The snapshot named no folder.
    Missing,
}

/// Which call a failure belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The handshake summary.
    Describe,
    /// `session.create`.
    Create,
    /// `prompt.submit`.
    Submit,
    /// `session.detach`.
    Detach,
}

/// One result fed back into the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchInput {
    /// The handshake summary.
    Described(DescribeSummary),
    /// The worktree exists and is recorded.
    WorktreeReady(WorktreeRecord),
    /// Preparing the worktree failed.
    WorktreeFailed(LaunchError),
    /// The gateway created the session.
    Created(CreatedSession),
    /// The folder check finished.
    CwdChecked(CwdCheck),
    /// The session id was written, or the warning says why it was not.
    SessionRecorded { warning: Option<String> },
    /// The gateway answered the submit.
    Submitted(SubmitReply),
    /// A call failed.
    Failed { step: Step, failure: CallFailure },
    /// Detach finished or its failure was ignored.
    Detached,
    /// Rollback finished. `kept` is why the guard left the tree in place.
    RolledBack { kept: Option<String> },
    /// The operator interrupted, or Talaria is quitting.
    Cancel,
}

/// What the service should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchEffect {
    /// Read the handshake summary. No frame is sent.
    Describe,
    /// Create the worktree.
    PrepareWorktree { folder: PathBuf, slug: String },
    /// Send `session.create`.
    Create { new: NewSessionSpec },
    /// Compare the sent folder with `info.cwd`.
    CheckCwd {
        sent: PathBuf,
        reported: Option<String>,
    },
    /// Write the session id into the git record.
    RecordSession {
        record: WorktreeRecord,
        session_id: String,
    },
    /// Send `prompt.submit`.
    Submit {
        session_id: String,
        text: String,
        submission_id: String,
    },
    /// Send `session.detach`. `end_reason` is set only when nothing was admitted.
    Detach {
        session_id: String,
        subscription_id: String,
        end_reason: Option<&'static str>,
    },
    /// Remove the tree this launch created.
    Rollback(WorktreeRecord),
    /// Tell the fleet the session is running.
    FleetRunning { session_id: String },
    /// Tell the fleet the launch has let go.
    FleetReleased { session_id: String },
    /// The launch is over.
    Finish(LaunchOutcome),
}

/// The create parameters, before the wire builder adds the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSessionSpec {
    /// Idempotency key for the one retry.
    pub request_id: String,
    /// The worktree root, or the launch folder.
    pub cwd: PathBuf,
    /// Sent only when `Some`.
    pub model: Option<String>,
    /// Sent only when `Some`.
    pub provider: Option<String>,
    /// The effort, sent as `reasoning` only when `Some`.
    pub reasoning: Option<String>,
}

/// Where the launch is. Each state waits for the result of the effect that was emitted last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Describing,
    Preparing,
    Creating,
    Checking,
    Recording,
    Submitting,
    /// The prompt was handed to Hermes (or its fate is unknown); the detach is in flight.
    Releasing,
    /// A created session is being closed because nothing was admitted.
    Abandoning,
    RollingBack,
    Done,
}

/// How a launch that is being wound down will finish.
#[derive(Clone, Debug)]
enum Ending {
    NotStarted {
        what: String,
        next_step: String,
        /// Used instead of `next_step` when the worktree was removed.
        next_step_if_removed: Option<String>,
    },
    Interrupted,
    /// The finished outcome, built when the submit was answered.
    Released(Box<LaunchOutcome>),
}

/// The machine.
#[derive(Clone, Debug)]
pub struct LaunchMachine {
    request: LaunchRequest,
    ids: LaunchIds,
    state: State,
    record: Option<WorktreeRecord>,
    created: Option<CreatedSession>,
    sent_cwd: PathBuf,
    submit_sent: bool,
    cancelled: bool,
    warnings: Vec<String>,
    ending: Option<Ending>,
}

/// Detach reason used when nothing was admitted.
pub const END_REASON_ABANDONED: &str = "launch_abandoned";

const UPDATE_HERMES: &str = "Update Hermes to the release pinned for Talaria.";

impl LaunchMachine {
    /// Begin a launch.
    #[must_use]
    pub fn start(request: LaunchRequest, ids: LaunchIds) -> (Self, Vec<LaunchEffect>) {
        let machine = Self {
            sent_cwd: request.folder.clone(),
            request,
            ids,
            state: State::Describing,
            record: None,
            created: None,
            submit_sent: false,
            cancelled: false,
            warnings: Vec::new(),
            ending: None,
        };
        (machine, vec![LaunchEffect::Describe])
    }

    /// Apply one input. An input that does not belong to the current state changes nothing.
    pub fn step(&mut self, input: LaunchInput) -> Vec<LaunchEffect> {
        if matches!(input, LaunchInput::Cancel) {
            return self.cancel();
        }
        match (self.state, input) {
            (State::Describing, LaunchInput::Described(summary)) => self.described(&summary),
            (State::Preparing, LaunchInput::WorktreeReady(record)) => self.worktree_ready(record),
            (State::Preparing, LaunchInput::WorktreeFailed(error)) => {
                self.state = State::Done;
                let outcome = if self.cancelled {
                    LaunchOutcome::Interrupted {
                        tree_removed: false,
                        tree_kept: None,
                    }
                } else {
                    LaunchOutcome::NotStarted {
                        error,
                        closed_session: None,
                        tree_removed: false,
                        tree_kept: None,
                    }
                };
                vec![LaunchEffect::Finish(outcome)]
            }
            (State::Creating, LaunchInput::Created(created)) => self.created(created),
            (
                State::Creating,
                LaunchInput::Failed {
                    step: Step::Create,
                    failure,
                },
            ) => {
                let ending = if self.cancelled {
                    Ending::Interrupted
                } else {
                    create_failure(&failure)
                };
                self.wind_down(ending)
            }
            (State::Checking, LaunchInput::CwdChecked(check)) => self.cwd_checked(check),
            (State::Recording, LaunchInput::SessionRecorded { warning }) => {
                self.warnings.extend(warning);
                if self.cancelled {
                    self.abandon(Ending::Interrupted)
                } else {
                    self.submit()
                }
            }
            (State::Submitting, LaunchInput::Submitted(reply)) => self.submitted(reply),
            (
                State::Submitting,
                LaunchInput::Failed {
                    step: Step::Submit,
                    failure,
                },
            ) => self.submit_failed(&failure),
            (State::Releasing, LaunchInput::Detached)
            | (
                State::Releasing,
                LaunchInput::Failed {
                    step: Step::Detach, ..
                },
            ) => self.released(),
            (State::Abandoning, LaunchInput::Detached)
            | (
                State::Abandoning,
                LaunchInput::Failed {
                    step: Step::Detach, ..
                },
            ) => self.abandoned(),
            (State::RollingBack, LaunchInput::RolledBack { kept }) => self.rolled_back(kept),
            _ => Vec::new(),
        }
    }

    /// True from the step that emitted `Submit`.
    #[must_use]
    pub fn submit_sent(&self) -> bool {
        self.submit_sent
    }

    fn cancel(&mut self) -> Vec<LaunchEffect> {
        if self.submit_sent {
            return Vec::new();
        }
        match self.state {
            State::Describing => {
                self.state = State::Done;
                vec![LaunchEffect::Finish(LaunchOutcome::Interrupted {
                    tree_removed: false,
                    tree_kept: None,
                })]
            }
            State::Preparing | State::Creating | State::Checking | State::Recording => {
                self.cancelled = true;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn described(&mut self, summary: &DescribeSummary) -> Vec<LaunchEffect> {
        if let Some((what, next_step)) = self.unsupported(summary) {
            self.state = State::Done;
            return vec![LaunchEffect::Finish(LaunchOutcome::NotStarted {
                error: LaunchError { what, next_step },
                closed_session: None,
                tree_removed: false,
                tree_kept: None,
            })];
        }
        let Some(wanted) = &self.request.worktree else {
            return self.create();
        };
        let slug = wanted
            .name
            .as_deref()
            .and_then(|name| explicit_slug(name).ok())
            .unwrap_or_else(|| slugify(&self.request.prompt));
        self.state = State::Preparing;
        vec![LaunchEffect::PrepareWorktree {
            folder: self.request.folder.clone(),
            slug,
        }]
    }

    /// The first thing the launch needs that the gateway does not offer.
    fn unsupported(&self, summary: &DescribeSummary) -> Option<(String, String)> {
        if !summary.sources.iter().any(|source| source == "tui") {
            return Some((
                "this Hermes gateway does not offer the tui session source.".to_owned(),
                UPDATE_HERMES.to_owned(),
            ));
        }
        let needed = [
            ("cwd", "a session folder", true),
            ("model", "--model", self.request.model.is_some()),
            ("provider", "--provider", self.request.provider.is_some()),
            ("reasoning", "--effort", self.request.effort.is_some()),
        ];
        for (parameter, option, sent) in needed {
            if sent
                && !summary
                    .parameters
                    .iter()
                    .any(|offered| offered == parameter)
            {
                let next_step = if parameter == "cwd" {
                    UPDATE_HERMES.to_owned()
                } else {
                    format!("Update Hermes to the release pinned for Talaria, or drop {option}.")
                };
                return Some((
                    format!("this Hermes gateway does not accept {option} for new sessions."),
                    next_step,
                ));
            }
        }
        None
    }

    fn worktree_ready(&mut self, record: WorktreeRecord) -> Vec<LaunchEffect> {
        self.sent_cwd = record.path.clone();
        self.record = Some(record);
        if self.cancelled {
            return self.wind_down(Ending::Interrupted);
        }
        self.create()
    }

    fn create(&mut self) -> Vec<LaunchEffect> {
        self.state = State::Creating;
        vec![LaunchEffect::Create {
            new: NewSessionSpec {
                request_id: self.ids.request_id.clone(),
                cwd: self.sent_cwd.clone(),
                model: self.request.model.clone(),
                provider: self.request.provider.clone(),
                reasoning: self.request.effort.clone(),
            },
        }]
    }

    fn created(&mut self, created: CreatedSession) -> Vec<LaunchEffect> {
        self.created = Some(created);
        if self.cancelled {
            return self.abandon(Ending::Interrupted);
        }
        self.state = State::Checking;
        vec![LaunchEffect::CheckCwd {
            sent: self.sent_cwd.clone(),
            reported: self.created.as_ref().and_then(|c| c.info_cwd.clone()),
        }]
    }

    fn cwd_checked(&mut self, check: CwdCheck) -> Vec<LaunchEffect> {
        if self.cancelled {
            return self.abandon(Ending::Interrupted);
        }
        let confirmed = match check {
            CwdCheck::Matches => true,
            CwdCheck::Missing => self.record.is_none(),
            CwdCheck::Differs => false,
        };
        if !confirmed {
            return self.abandon(Ending::NotStarted {
                what: "Hermes did not confirm the session's folder, so the prompt was not sent."
                    .to_owned(),
                next_step: "Check that the profile's terminal runs on this machine; the empty \
                            session was closed."
                    .to_owned(),
                next_step_if_removed: Some(
                    "Check that the profile's terminal runs on this machine; the empty session \
                     was closed and the worktree removed."
                        .to_owned(),
                ),
            });
        }
        match (&self.record, &self.created) {
            (Some(record), Some(created)) => {
                self.state = State::Recording;
                vec![LaunchEffect::RecordSession {
                    record: record.clone(),
                    session_id: created.session_id.clone(),
                }]
            }
            _ => self.submit(),
        }
    }

    fn submit(&mut self) -> Vec<LaunchEffect> {
        let Some(created) = &self.created else {
            return Vec::new();
        };
        self.state = State::Submitting;
        self.submit_sent = true;
        vec![LaunchEffect::Submit {
            session_id: created.session_id.clone(),
            text: self.request.prompt.clone(),
            submission_id: self.ids.submission_id.clone(),
        }]
    }

    fn submitted(&mut self, reply: SubmitReply) -> Vec<LaunchEffect> {
        match reply {
            SubmitReply::Admitted {
                admission_id,
                status,
            } => {
                let Some(created) = self.created.clone() else {
                    return Vec::new();
                };
                let launched = Launched {
                    session_id: created.session_id.clone(),
                    stored_session_id: created
                        .stored_session_id
                        .clone()
                        .unwrap_or_else(|| created.session_id.clone()),
                    profile: self.request.profile.clone(),
                    admission_id,
                    admission_status: status,
                    model: created.model.clone(),
                    cwd: self.sent_cwd.clone(),
                    worktree: self.record.clone(),
                    warnings: std::mem::take(&mut self.warnings),
                    prompt: self.request.prompt.clone(),
                };
                self.ending = Some(Ending::Released(Box::new(LaunchOutcome::Launched(
                    launched,
                ))));
                self.state = State::Releasing;
                vec![
                    LaunchEffect::FleetRunning {
                        session_id: created.session_id.clone(),
                    },
                    self.detach(None),
                ]
            }
            SubmitReply::VoiceStopped => self.abandon(Ending::NotStarted {
                what: "Hermes took the prompt as a voice stop phrase and started nothing."
                    .to_owned(),
                next_step: "Rephrase the prompt.".to_owned(),
                next_step_if_removed: None,
            }),
        }
    }

    fn submit_failed(&mut self, failure: &CallFailure) -> Vec<LaunchEffect> {
        match failure {
            CallFailure::Unknown | CallFailure::Malformed(_) => {
                let Some(created) = self.created.clone() else {
                    return Vec::new();
                };
                self.ending = Some(Ending::Released(Box::new(LaunchOutcome::Unknown {
                    session_id: Some(created.session_id.clone()),
                    stored_session_id: Some(
                        created
                            .stored_session_id
                            .unwrap_or_else(|| created.session_id.clone()),
                    ),
                    profile: self.request.profile.clone(),
                    worktree: self.record.clone(),
                })));
                self.state = State::Releasing;
                vec![self.detach(None)]
            }
            CallFailure::Refused {
                code,
                reason,
                fields,
            } => {
                let (what, next_step) = if *code == METHOD_NOT_FOUND {
                    (
                        "this Hermes gateway does not know prompt.submit.".to_owned(),
                        format!("{UPDATE_HERMES} The empty session was closed."),
                    )
                } else {
                    (
                        format!(
                            "Hermes refused the prompt: {}.",
                            refusal_reason(reason, fields)
                        ),
                        "Nothing was started; the empty session was closed.".to_owned(),
                    )
                };
                self.abandon(Ending::NotStarted {
                    what,
                    next_step,
                    next_step_if_removed: None,
                })
            }
            CallFailure::NotSent(why) => self.abandon(Ending::NotStarted {
                what: format!(
                    "the prompt was never sent to Hermes ({}).",
                    sanitize_line(why)
                ),
                next_step: "Nothing was started; the empty session was closed.".to_owned(),
                next_step_if_removed: None,
            }),
        }
    }

    fn detach(&self, end_reason: Option<&'static str>) -> LaunchEffect {
        let (session_id, subscription_id) = self
            .created
            .as_ref()
            .map(|created| (created.session_id.clone(), created.subscription_id.clone()))
            .unwrap_or_default();
        LaunchEffect::Detach {
            session_id,
            subscription_id,
            end_reason,
        }
    }

    /// Close the created session with the abandoned reason, then finish as `ending` says.
    fn abandon(&mut self, ending: Ending) -> Vec<LaunchEffect> {
        self.ending = Some(ending);
        self.state = State::Abandoning;
        vec![self.detach(Some(END_REASON_ABANDONED))]
    }

    /// Remove the tree when there is one, then finish as `ending` says.
    fn wind_down(&mut self, ending: Ending) -> Vec<LaunchEffect> {
        self.ending = Some(ending);
        self.abandoned()
    }

    fn abandoned(&mut self) -> Vec<LaunchEffect> {
        match self.record.clone() {
            Some(record) => {
                self.state = State::RollingBack;
                vec![LaunchEffect::Rollback(record)]
            }
            None => self.finish(true, None),
        }
    }

    fn rolled_back(&mut self, kept: Option<String>) -> Vec<LaunchEffect> {
        let tree_kept = match (kept, &self.record) {
            (Some(reason), Some(record)) => Some((record.path.clone(), reason)),
            _ => None,
        };
        self.finish(false, tree_kept)
    }

    fn released(&mut self) -> Vec<LaunchEffect> {
        let Some(Ending::Released(outcome)) = self.ending.take() else {
            return Vec::new();
        };
        self.state = State::Done;
        let mut effects = Vec::new();
        if let LaunchOutcome::Launched(launched) = &*outcome {
            effects.push(LaunchEffect::FleetReleased {
                session_id: launched.session_id.clone(),
            });
        }
        effects.push(LaunchEffect::Finish(*outcome));
        effects
    }

    /// Finish a launch that is being wound down. `no_tree` is true when no rollback ran.
    fn finish(&mut self, no_tree: bool, tree_kept: Option<(PathBuf, String)>) -> Vec<LaunchEffect> {
        self.state = State::Done;
        let tree_removed = !no_tree && tree_kept.is_none();
        let outcome = match self.ending.take() {
            Some(Ending::NotStarted {
                what,
                next_step,
                next_step_if_removed,
            }) => LaunchOutcome::NotStarted {
                error: LaunchError {
                    what,
                    next_step: match next_step_if_removed {
                        Some(removed) if tree_removed => removed,
                        _ => next_step,
                    },
                },
                closed_session: self.created.as_ref().map(|c| c.session_id.clone()),
                tree_removed,
                tree_kept,
            },
            Some(Ending::Released(outcome)) => *outcome,
            Some(Ending::Interrupted) | None => LaunchOutcome::Interrupted {
                tree_removed,
                tree_kept,
            },
        };
        vec![LaunchEffect::Finish(outcome)]
    }
}

/// JSON-RPC code for a method the gateway does not have.
const METHOD_NOT_FOUND: i64 = -32601;

/// How a failed `session.create` ends the launch.
fn create_failure(failure: &CallFailure) -> Ending {
    match failure {
        CallFailure::Refused {
            code,
            reason,
            fields,
        } => {
            let (what, next_step) = if *code == METHOD_NOT_FOUND {
                (
                    "this Hermes gateway does not know session.create.".to_owned(),
                    UPDATE_HERMES.to_owned(),
                )
            } else {
                let what = format!(
                    "Hermes refused to create the session: {}.",
                    refusal_reason(reason, fields)
                );
                (
                    what,
                    "Nothing was started; check the options you passed.".to_owned(),
                )
            };
            Ending::NotStarted {
                what,
                next_step,
                next_step_if_removed: None,
            }
        }
        CallFailure::Unknown | CallFailure::NotSent(_) | CallFailure::Malformed(_) => {
            Ending::NotStarted {
                what: "Hermes did not confirm that the session was created.".to_owned(),
                next_step: "Nothing was sent to an agent. An empty, idle session may appear in \
                            talaria fleet list; it is safe to ignore."
                    .to_owned(),
                next_step_if_removed: None,
            }
        }
    }
}

fn refusal_reason(reason: &Option<String>, fields: &[String]) -> String {
    let mut text = sanitize_line(reason.as_deref().unwrap_or("no reason was given"));
    if !fields.is_empty() {
        let fields: Vec<String> = fields.iter().map(|field| sanitize_line(field)).collect();
        text.push_str(&format!(" (field {})", fields.join(", ")));
    }
    text
}

/// Gateway text on one line, with control characters removed.
fn sanitize_line(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
