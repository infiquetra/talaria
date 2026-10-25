//! The session working folder and the `/cd` move for #237.
//!
//! The reducer is pure: it reads no clock and no filesystem, and it returns
//! effects as data. The application checks the folder on disk and sends the
//! move, then feeds the answers back in. The fenced mutation types come from
//! the shared helper.

use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value};

use crate::mutation::{MutationLedger, MutationOutcome, MutationRequest};
use crate::session::NoticeKind;

/// One parsed `/cd` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdCommand {
    /// Show the session's folder.
    Show,
    /// Move the session to the named folder.
    Move(String),
}

/// Parses the text after `/cd`; empty text shows the folder.
pub fn parse_cd_command(args: &str) -> CdCommand {
    let text = args.trim();
    if text.is_empty() {
        CdCommand::Show
    } else {
        CdCommand::Move(text.to_owned())
    }
}

/// Why a `/cd` target could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdTargetError {
    /// A relative path with no known session folder.
    RelativeWithoutFolder,
    /// A `~name` path, which Talaria never expands.
    TildeUser,
    /// A `~` path with no known home folder.
    NoHome,
}

/// Resolves a `/cd` argument to an absolute normalised path.
pub fn resolve_cd_target(
    arg: &str,
    session_cwd: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, CdTargetError> {
    let text = strip_matching_quotes(arg);
    let joined = if text == "~" || text.starts_with("~/") {
        let home = home.ok_or(CdTargetError::NoHome)?;
        if !home.is_absolute() {
            return Err(CdTargetError::NoHome);
        }
        home.join(text[1..].trim_start_matches('/'))
    } else if text.starts_with('~') {
        return Err(CdTargetError::TildeUser);
    } else if Path::new(text).is_absolute() {
        PathBuf::from(text)
    } else {
        match session_cwd {
            Some(folder) if folder.is_absolute() => folder.join(text),
            _ => return Err(CdTargetError::RelativeWithoutFolder),
        }
    };
    Ok(normalise(&joined))
}

/// Removes one pair of matching quotes around a path, and nothing else.
fn strip_matching_quotes(text: &str) -> &str {
    for quote in ['\'', '"'] {
        if text.chars().count() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            // Both ends are one-byte quote characters, so the slice is on boundaries.
            return &text[1..text.len() - 1];
        }
    }
    text
}

/// Lexical normalisation of an absolute path: no `.`, `..` or repeated or
/// trailing separators, and `..` never climbs above the root.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            Component::Normal(name) => out.push(name),
        }
    }
    out
}

/// Shortens a leading home folder to `~` by whole components.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home.filter(|home| home.parent().is_some())
        && let Ok(rest) = path.strip_prefix(home)
    {
        return if rest.as_os_str().is_empty() {
            "~".to_owned()
        } else {
            format!("~/{}", rest.display())
        };
    }
    path.display().to_string()
}

/// The foreground session's folder and branch as Talaria shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFolder {
    /// The session's working folder.
    pub cwd: Option<String>,
    /// The branch of the folder, when known.
    pub branch: Option<String>,
}

impl SessionFolder {
    /// Both fields as the snapshot gives them.
    pub fn from_snapshot(cwd: Option<&str>, branch: Option<&str>) -> SessionFolder {
        SessionFolder {
            cwd: cwd.map(str::to_owned),
            branch: branch.map(str::to_owned),
        }
    }

    /// Applies a `session.info` folder: a different folder clears the branch.
    pub fn apply_info_cwd(&mut self, cwd: &str) {
        if self.cwd.as_deref() != Some(cwd) {
            self.cwd = Some(cwd.to_owned());
            self.branch = None;
        }
    }

    /// Applies a confirmed move; a missing branch clears it.
    pub fn apply_move(&mut self, cwd: &str, branch: Option<&str>) {
        self.cwd = Some(cwd.to_owned());
        self.branch = branch.map(str::to_owned);
    }
}

/// What the local folder check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderCheck {
    /// A directory: the move may be sent.
    Directory,
    /// Nothing at the path.
    Missing,
    /// A file or other non-directory.
    NotADirectory,
    /// Any other error, or the check timed out.
    Unreadable,
}

/// Everything the `/cd` reducer needs from the application.
#[derive(Clone, Debug)]
pub struct CdContext<'a> {
    /// The foreground session, if any.
    pub session_id: Option<&'a str>,
    /// Whether a turn is running on the foreground session.
    pub turn_running: bool,
    /// The foreground session's folder.
    pub folder: &'a SessionFolder,
    /// The home folder, if known.
    pub home: Option<&'a Path>,
    /// The interrupt action's label, if bound.
    pub interrupt_label: Option<&'a str>,
}

/// One input to the `/cd` reducer.
#[derive(Clone, Debug, PartialEq)]
pub enum CdInput {
    /// A parsed `/cd` command line.
    Command(CdCommand),
    /// A finished local folder check.
    Checked {
        /// The session the check was for.
        session_id: String,
        /// The checked target.
        target: PathBuf,
        /// What the check found.
        check: FolderCheck,
    },
    /// The outcome of a sent move request.
    Outcome {
        /// The request the outcome answers.
        request: MutationRequest,
        /// The outcome.
        outcome: MutationOutcome,
    },
}

/// One effect the `/cd` reducer emits.
#[derive(Clone, Debug, PartialEq)]
pub enum CdEffect {
    /// Check the folder on the local machine.
    CheckFolder {
        /// The session the check is for.
        session_id: String,
        /// The target to check.
        target: PathBuf,
    },
    /// Send the move request.
    Send(MutationRequest),
    /// The move committed: show the new folder.
    UpdateFolder {
        /// The session that moved.
        session_id: String,
        /// The new folder.
        cwd: String,
        /// The branch of the new folder, when Hermes reports one.
        branch: Option<String>,
    },
    /// Show a notice to the developer.
    Notice {
        /// The notice kind.
        kind: NoticeKind,
        /// The notice text.
        text: String,
    },
}

/// The `/cd` reducer state: at most one check or move at a time.
#[derive(Clone, Debug)]
pub struct CdState {
    /// The session and target of the pending folder check, if any.
    checking: Option<(String, PathBuf)>,
    /// The move request in flight, if any.
    in_flight: Option<MutationRequest>,
}

impl Default for CdState {
    fn default() -> CdState {
        CdState::new()
    }
}

impl CdState {
    /// An idle state.
    pub fn new() -> CdState {
        CdState {
            checking: None,
            in_flight: None,
        }
    }

    /// Reduces one input into effects.
    pub fn update(
        &mut self,
        ctx: &CdContext<'_>,
        ledger: &mut MutationLedger,
        input: CdInput,
    ) -> Vec<CdEffect> {
        match input {
            CdInput::Command(CdCommand::Show) => vec![show_notice(ctx)],
            CdInput::Command(CdCommand::Move(arg)) => self.start_move(ctx, &arg),
            CdInput::Checked {
                session_id,
                target,
                check,
            } => self.finish_check(ctx, ledger, &session_id, &target, check),
            CdInput::Outcome { request, outcome } => {
                self.finish_move(ctx, ledger, request, outcome)
            }
        }
    }

    /// Validates a `/cd <folder>` line and asks for the local folder check.
    fn start_move(&mut self, ctx: &CdContext<'_>, arg: &str) -> Vec<CdEffect> {
        let Some(session_id) = ctx.session_id else {
            return vec![notice(
                NoticeKind::System,
                "No session is open, so there is no folder to change.".to_owned(),
            )];
        };
        if ctx.turn_running {
            let interrupt = ctx.interrupt_label.unwrap_or("Esc");
            return vec![notice(
                NoticeKind::Warning,
                format!(
                    "A turn is running. Wait for it to finish, or press {interrupt} to stop it, \
                     then run /cd again."
                ),
            )];
        }
        if self.checking.is_some() || self.in_flight.is_some() {
            return vec![notice(
                NoticeKind::Warning,
                "A /cd is already being checked or sent. Wait for its answer.".to_owned(),
            )];
        }
        let current = ctx.folder.cwd.as_deref().map(Path::new);
        let target = match resolve_cd_target(arg, current, ctx.home) {
            Ok(target) => target,
            Err(error) => {
                let text = match error {
                    CdTargetError::RelativeWithoutFolder => {
                        "This session's folder is unknown, so a relative path cannot be used. \
                         Give the full path."
                    }
                    CdTargetError::TildeUser => {
                        "Talaria does not expand ~name. Give the full path."
                    }
                    CdTargetError::NoHome => {
                        "Talaria does not know the home folder, so ~ cannot be used. Give the \
                         full path."
                    }
                };
                return vec![notice(NoticeKind::Error, text.to_owned())];
            }
        };
        if current.is_some_and(|folder| folder.is_absolute() && normalise(folder) == target) {
            return vec![notice(
                NoticeKind::System,
                format!(
                    "This session already works in {}.",
                    display_path(&target, ctx.home)
                ),
            )];
        }
        self.checking = Some((session_id.to_owned(), target.clone()));
        vec![CdEffect::CheckFolder {
            session_id: session_id.to_owned(),
            target,
        }]
    }

    /// Handles the answer of the local folder check.
    fn finish_check(
        &mut self,
        ctx: &CdContext<'_>,
        ledger: &mut MutationLedger,
        session_id: &str,
        target: &Path,
        check: FolderCheck,
    ) -> Vec<CdEffect> {
        let pending = matches!(
            &self.checking,
            Some((pending_session, pending_target))
                if pending_session == session_id && pending_target == target
        );
        if !pending {
            return Vec::new();
        }
        self.checking = None;
        if ctx.session_id != Some(session_id) {
            return vec![notice(
                NoticeKind::Warning,
                "The session changed before the move was sent; nothing was moved.".to_owned(),
            )];
        }
        let shown = display_path(target, ctx.home);
        match check {
            FolderCheck::Missing => {
                vec![notice(NoticeKind::Error, format!("No folder at {shown}."))]
            }
            FolderCheck::NotADirectory => vec![notice(
                NoticeKind::Error,
                format!("{shown} is a file, not a folder."),
            )],
            FolderCheck::Unreadable => vec![notice(
                NoticeKind::Error,
                format!("Talaria cannot read {shown}."),
            )],
            FolderCheck::Directory => {
                let mut payload = Map::new();
                payload.insert(
                    "cwd".to_owned(),
                    Value::String(target.to_string_lossy().into_owned()),
                );
                match ledger.build(session_id, WORKSPACE_OPERATION, payload, true) {
                    Ok(request) => {
                        self.in_flight = Some(request.clone());
                        vec![
                            CdEffect::Send(request),
                            notice(
                                NoticeKind::System,
                                format!("Moving this session to {shown}\u{2026}"),
                            ),
                        ]
                    }
                    Err(_) => vec![notice(
                        NoticeKind::Warning,
                        "Talaria does not know the session's revision yet. Try /cd again in a \
                         moment."
                            .to_owned(),
                    )],
                }
            }
        }
    }

    /// Handles the outcome of a sent move.
    fn finish_move(
        &mut self,
        ctx: &CdContext<'_>,
        ledger: &mut MutationLedger,
        request: MutationRequest,
        outcome: MutationOutcome,
    ) -> Vec<CdEffect> {
        if self.in_flight.as_ref() != Some(&request) {
            return Vec::new();
        }
        if ctx.session_id != Some(request.session_id.as_str()) {
            self.in_flight = None;
            ledger.settle(&request, &outcome);
            return vec![notice(
                NoticeKind::System,
                "A /cd sent from another session has finished; open that session to see its \
                 folder."
                    .to_owned(),
            )];
        }
        if matches!(outcome, MutationOutcome::FenceRefused { .. })
            && let Some(retry) = ledger.retry_after_fence(&request, &outcome)
        {
            self.in_flight = Some(retry.clone());
            return vec![CdEffect::Send(retry)];
        }
        self.in_flight = None;
        ledger.settle(&request, &outcome);
        let asked = request
            .payload
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let asked_shown = display_path(Path::new(asked), ctx.home);
        match outcome {
            MutationOutcome::Committed(result) => {
                let cwd = result
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or(asked)
                    .to_owned();
                let branch = result
                    .get("branch")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let suffix = branch_suffix(branch.as_deref());
                let text = format!(
                    "This session now works in {}{suffix}. Its next command runs there; the \
                     conversation is kept.",
                    display_path(Path::new(&cwd), ctx.home)
                );
                vec![
                    CdEffect::UpdateFolder {
                        session_id: request.session_id,
                        cwd,
                        branch,
                    },
                    notice(NoticeKind::System, text),
                ]
            }
            MutationOutcome::FenceRefused { .. } => vec![notice(
                NoticeKind::Warning,
                "The session changed while it was being moved. Run /cd again.".to_owned(),
            )],
            MutationOutcome::Refused {
                reason, rpc_code, ..
            } => vec![refusal_notice(&reason, rpc_code, &asked_shown)],
            MutationOutcome::Unknown => vec![notice(
                NoticeKind::Warning,
                "Talaria could not confirm the move. Run the same /cd again to retry it safely."
                    .to_owned(),
            )],
            MutationOutcome::NotSent { .. } => {
                vec![not_moved("Talaria is not connected to Hermes")]
            }
            MutationOutcome::Deferred(_)
            | MutationOutcome::NeedsConfirmation(_)
            | MutationOutcome::OtherStatus { .. }
            | MutationOutcome::Unreadable => {
                vec![not_moved("Hermes gave an answer Talaria could not read")]
            }
        }
    }
}

/// The only session operation `/cd` sends.
const WORKSPACE_OPERATION: &str = "workspace";

/// The longest refusal reason shown to the developer, in characters.
const REASON_LIMIT: usize = 80;

fn notice(kind: NoticeKind, text: String) -> CdEffect {
    CdEffect::Notice { kind, text }
}

/// The `{branch}` part of a folder text: ` (name)`, or nothing.
fn branch_suffix(branch: Option<&str>) -> String {
    branch.map(|name| format!(" ({name})")).unwrap_or_default()
}

/// The notice for a bare `/cd`.
fn show_notice(ctx: &CdContext<'_>) -> CdEffect {
    match ctx.folder.cwd.as_deref() {
        Some(cwd) => notice(
            NoticeKind::System,
            format!(
                "This session works in {}{}. Use /cd <folder> to move it.",
                display_path(Path::new(cwd), ctx.home),
                branch_suffix(ctx.folder.branch.as_deref())
            ),
        ),
        None => notice(
            NoticeKind::System,
            "This session's folder is unknown. Use /cd <full path> to set it.".to_owned(),
        ),
    }
}

/// The notice for a move Hermes did not make.
fn not_moved(words: &str) -> CdEffect {
    notice(
        NoticeKind::Error,
        format!("The session was not moved: {words}."),
    )
}

/// The notice for a refusal, by Hermes's reason first and the code second.
fn refusal_notice(reason: &str, rpc_code: i64, path: &str) -> CdEffect {
    match reason {
        "session_busy" => notice(
            NoticeKind::Warning,
            "Hermes is busy with this session (a turn or a context compression is running). Try \
             /cd again when it finishes."
                .to_owned(),
        ),
        "unknown_execution" => notice(
            NoticeKind::Warning,
            "Hermes does not know yet whether the last turn finished. Try /cd again in a moment."
                .to_owned(),
        ),
        "invalid_params" => notice(
            NoticeKind::Error,
            format!("Hermes could not use {path} as the session's folder."),
        ),
        "runtime_coordination_required" => notice(
            NoticeKind::Error,
            "Hermes cannot move this session to another folder.".to_owned(),
        ),
        "permission_denied" => notice(
            NoticeKind::Error,
            "This connection may not move the session.".to_owned(),
        ),
        _ if rpc_code == -32601 => notice(
            NoticeKind::Error,
            "This Hermes gateway cannot move a session to another folder.".to_owned(),
        ),
        _ => {
            let words = if reason.is_empty() {
                format!("error {rpc_code}")
            } else {
                reason.chars().take(REASON_LIMIT).collect()
            };
            not_moved(&words)
        }
    }
}
